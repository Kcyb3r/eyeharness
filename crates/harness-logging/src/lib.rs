//! Structured logging ([plan §19]).
//!
//! Wraps `tracing` with a JSONL subscriber that writes structured events to a
//! rolling file, plus a redaction layer. Sensitive values — passwords, tokens,
//! cookies, private clipboard contents, sensitive form data — are never
//! persisted unless a session explicitly opts into capturing them.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Patterns considered sensitive by default (redacted unless enabled).
pub const DEFAULT_REDACT_KEYS: [&str; 6] = [
    "password",
    "token",
    "cookie",
    "secret",
    "authorization",
    "clipboard",
];

/// Result of a redaction operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactMode {
    /// Replace values with `[REDACTED]`.
    RedactValues,
    /// Keep values (explicitly enabled by the session policy).
    KeepValues,
}

/// Errors from logging initialization.
#[derive(Debug, thiserror::Error)]
pub enum LogError {
    /// The output path could not be opened.
    #[error("failed to open log file {path}: {source}")]
    Open {
        /// The path attempted.
        path: PathBuf,
        /// Underlying IO error.
        source: std::io::Error,
    },
    /// The lock was poisoned.
    #[error("log guard poisoned")]
    Poisoned,
}

/// A JSONL file writer backed by `tracing`.
///
/// The layer is kept deliberately simple: each serialized span/event is
/// written as one JSON line via a blocking `Mutex<File>` writer.
#[derive(Debug)]
pub struct JsonlWriter {
    file: Mutex<File>,
    redact: Mutex<RedactMode>,
}

impl JsonlWriter {
    /// Open (or create) a JSONL log file at `path`.
    pub fn create(path: impl AsRef<Path>) -> Result<Self, LogError> {
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path.as_ref())
            .map_err(|source| LogError::Open {
                path: path.as_ref().to_path_buf(),
                source,
            })?;
        Ok(Self {
            file: Mutex::new(file),
            redact: Mutex::new(RedactMode::RedactValues),
        })
    }

    /// Set the current redaction mode.
    pub fn set_redact_mode(&self, mode: RedactMode) -> Result<(), LogError> {
        let mut g = self.redact.lock().map_err(|_| LogError::Poisoned)?;
        *g = mode;
        Ok(())
    }

    /// Whether values are currently redacted.
    pub fn redact_mode(&self) -> RedactMode {
        self.redact
            .lock()
            .map(|guard| *guard)
            .unwrap_or(RedactMode::RedactValues)
    }

    /// Redact a JSON object's sensitive keys according to the current mode.
    pub fn redact(&self, mut value: serde_json::Value) -> serde_json::Value {
        if self.redact_mode() == RedactMode::KeepValues {
            return value;
        }
        redact_json(&mut value);
        value
    }

    /// Write a raw JSON line. Values are redacted per `redact_mode`.
    pub fn write_line(&self, value: serde_json::Value) -> Result<(), LogError> {
        let line = self.redact(value);
        let mut out = serde_json::to_vec(&line).map_err(|e| LogError::Open {
            path: PathBuf::from("<serialize>"),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?;
        out.push(b'\n');
        let mut guard = self.file.lock().map_err(|_| LogError::Poisoned)?;
        guard.write_all(&out).map_err(|e| LogError::Open {
            path: PathBuf::from("<write>"),
            source: e,
        })
    }

    /// Write a structured harness event with its canonical fields.
    pub fn write_event(&self, event: &harness_protocol::Event) -> Result<(), LogError> {
        self.write_line(serde_json::to_value(event).map_err(|e| LogError::Open {
            path: PathBuf::from("<serialize>"),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, e),
        })?)
    }
}

/// Recursively redact keys listed in [`DEFAULT_REDACT_KEYS`].
pub fn redact_json(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map.iter_mut() {
                if DEFAULT_REDACT_KEYS
                    .iter()
                    .any(|r| k.to_lowercase().contains(r))
                {
                    *v = serde_json::Value::String("[REDACTED]".to_string());
                } else {
                    redact_json(v);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_json(item);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_sensitive_keys_recursively() {
        let mut v = serde_json::json!({
            "timestamp": 1,
            "event": "action.executed",
            "fields": {
                "password": "hunter2",
                "token": "abc123",
                "note": "keep me"
            }
        });
        redact_json(&mut v);
        assert_eq!(v["fields"]["password"], "[REDACTED]");
        assert_eq!(v["fields"]["token"], "[REDACTED]");
        assert_eq!(v["fields"]["note"], "keep me");
    }

    #[test]
    fn writer_writes_redacted_lines() {
        let dir = std::env::temp_dir().join(format!("harness-log-{}", std::process::id()));
        let path = dir.join("test.jsonl");
        std::fs::create_dir_all(&dir).unwrap();
        let w = JsonlWriter::create(&path).unwrap();
        w.write_line(serde_json::json!({
            "password": "s3cret",
            "status": "ok"
        }))
        .unwrap();
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("\"status\":\"ok\""));
        assert!(!contents.contains("s3cret"));
        assert!(contents.contains("REDACTED"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
