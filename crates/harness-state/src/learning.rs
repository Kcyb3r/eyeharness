//! Bounded, user-controlled learning profiles.

#![allow(missing_docs)]

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_ENTRIES: usize = 512;
const DEFAULT_TTL_SECONDS: u64 = 30 * 24 * 60 * 60;

#[derive(Debug, thiserror::Error)]
pub enum LearningError {
    #[error("learning store IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("learning store format is invalid: {0}")]
    Format(#[from] serde_json::Error),
    #[error("learning field {field} is invalid")]
    InvalidField { field: &'static str },
    #[error("learning entry not found")]
    NotFound,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LearningEntry {
    pub profile: String,
    pub application: String,
    pub task: String,
    pub strategy: String,
    pub approved: bool,
    pub success_count: u32,
    pub failure_count: u32,
    pub last_observed: u64,
    pub expires_at: u64,
}

impl LearningEntry {
    pub fn confidence(&self) -> f32 {
        let total = self.success_count.saturating_add(self.failure_count);
        if total == 0 {
            0.0
        } else {
            self.success_count as f32 / total as f32
        }
    }

    pub fn active(&self, now: u64) -> bool {
        self.approved && self.expires_at > now
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct LearningFile {
    entries: Vec<LearningEntry>,
}

#[derive(Debug)]
pub struct LearningStore {
    path: PathBuf,
    entries: Vec<LearningEntry>,
}

impl LearningStore {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, LearningError> {
        let path = path.as_ref().to_path_buf();
        let entries = if path.exists() {
            let bytes = fs::read(&path)?;
            if bytes.is_empty() {
                Vec::new()
            } else {
                serde_json::from_slice::<LearningFile>(&bytes)?.entries
            }
        } else {
            Vec::new()
        };
        Ok(Self { path, entries })
    }

    pub fn default_path() -> PathBuf {
        std::env::var_os("EYEHARNESS_LEARNING_PATH")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("XDG_STATE_HOME")
                    .map(PathBuf::from)
                    .map(|p| p.join("eyeharness/learning.json"))
            })
            .unwrap_or_else(|| PathBuf::from(".eyeharness").join("learning.json"))
    }

    pub fn record(
        &mut self,
        profile: &str,
        application: &str,
        task: &str,
        strategy: &str,
        success: bool,
    ) -> Result<LearningEntry, LearningError> {
        validate("profile", profile)?;
        validate("application", application)?;
        validate("task", task)?;
        validate_strategy(strategy)?;
        let now = now();
        let key = (profile, application, task, strategy);
        if let Some(entry) = self.entries.iter_mut().find(|e| {
            (
                e.profile.as_str(),
                e.application.as_str(),
                e.task.as_str(),
                e.strategy.as_str(),
            ) == key
        }) {
            if success {
                entry.success_count = entry.success_count.saturating_add(1);
            } else {
                entry.failure_count = entry.failure_count.saturating_add(1);
            }
            entry.last_observed = now;
            entry.expires_at = now.saturating_add(DEFAULT_TTL_SECONDS);
        } else {
            if self.entries.len() >= MAX_ENTRIES {
                self.entries.sort_by_key(|e| e.last_observed);
                self.entries.remove(0);
            }
            self.entries.push(LearningEntry {
                profile: profile.into(),
                application: application.into(),
                task: task.into(),
                strategy: strategy.into(),
                approved: false,
                success_count: u32::from(success),
                failure_count: u32::from(!success),
                last_observed: now,
                expires_at: now.saturating_add(DEFAULT_TTL_SECONDS),
            });
        }
        self.persist()?;
        self.entries
            .iter()
            .find(|e| {
                (
                    e.profile.as_str(),
                    e.application.as_str(),
                    e.task.as_str(),
                    e.strategy.as_str(),
                ) == key
            })
            .cloned()
            .ok_or(LearningError::NotFound)
    }

    pub fn approve(
        &mut self,
        profile: &str,
        application: &str,
        task: &str,
        strategy: &str,
        approved: bool,
    ) -> Result<LearningEntry, LearningError> {
        let updated = {
            let entry = self.find_mut(profile, application, task, strategy)?;
            entry.approved = approved;
            entry.expires_at = now().saturating_add(DEFAULT_TTL_SECONDS);
            entry.clone()
        };
        self.persist()?;
        Ok(updated)
    }

    pub fn lookup(&self, profile: &str, application: &str, task: &str) -> Vec<LearningEntry> {
        let now = now();
        self.entries
            .iter()
            .filter(|e| e.profile == profile && e.application == application && e.task == task)
            .filter(|e| e.active(now))
            .cloned()
            .collect()
    }

    pub fn export(&self) -> Vec<LearningEntry> {
        self.entries.clone()
    }

    pub fn reset(&mut self, profile: Option<&str>) -> Result<usize, LearningError> {
        let before = self.entries.len();
        match profile {
            Some(profile) => self.entries.retain(|e| e.profile != profile),
            None => self.entries.clear(),
        }
        self.persist()?;
        Ok(before - self.entries.len())
    }

    fn find_mut(
        &mut self,
        profile: &str,
        application: &str,
        task: &str,
        strategy: &str,
    ) -> Result<&mut LearningEntry, LearningError> {
        self.entries
            .iter_mut()
            .find(|e| {
                e.profile == profile
                    && e.application == application
                    && e.task == task
                    && e.strategy == strategy
            })
            .ok_or(LearningError::NotFound)
    }

    fn persist(&self) -> Result<(), LearningError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = serde_json::to_vec_pretty(&LearningFile {
            entries: self.entries.clone(),
        })?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes)?;
        fs::rename(temporary, &self.path)?;
        Ok(())
    }
}

fn validate(field: &'static str, value: &str) -> Result<(), LearningError> {
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || value.len() > 256
        || lower.contains("password")
        || lower.contains("token")
        || lower.contains("secret")
        || lower.contains("cookie")
        || lower.contains("authorization")
    {
        return Err(LearningError::InvalidField { field });
    }
    Ok(())
}

fn validate_strategy(strategy: &str) -> Result<(), LearningError> {
    validate("strategy", strategy)?;
    if strategy.contains('\n') || strategy.contains('\r') {
        return Err(LearningError::InvalidField { field: "strategy" });
    }
    Ok(())
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learning_requires_approval_and_persists_outcomes() {
        let path =
            std::env::temp_dir().join(format!("eyeharness-learning-{}.json", std::process::id()));
        let _ = fs::remove_file(&path);
        let mut store = LearningStore::open(&path).unwrap();
        let entry = store
            .record("default", "Brave", "focus-address-bar", "ctrl+l", true)
            .unwrap();
        assert!(!entry.approved);
        assert!(store
            .lookup("default", "Brave", "focus-address-bar")
            .is_empty());
        store
            .approve("default", "Brave", "focus-address-bar", "ctrl+l", true)
            .unwrap();
        assert_eq!(
            store.lookup("default", "Brave", "focus-address-bar").len(),
            1
        );
        let reopened = LearningStore::open(&path).unwrap();
        assert_eq!(reopened.export()[0].success_count, 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn sensitive_values_are_rejected() {
        let path = std::env::temp_dir().join(format!(
            "eyeharness-learning-sensitive-{}.json",
            std::process::id()
        ));
        let mut store = LearningStore::open(&path).unwrap();
        assert!(store
            .record("default", "app", "password task", "click", true)
            .is_err());
        let _ = fs::remove_file(path);
    }
}
