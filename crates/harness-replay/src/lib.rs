//! Session replay ([plan §6]).
//!
//! Ingests a JSONL trace produced by the harness and exposes a
//! [`ReplaySession`] for deterministic re-execution, debugging, and
//! trace-driven UI tests.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

use harness_protocol::{Event, EventKind, SessionId};

/// Errors from the replay engine.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    /// The trace file could not be read.
    #[error("trace io: {0}")]
    Io(#[from] std::io::Error),
    /// A JSON line could not be parsed.
    #[error("malformed trace line: {0}")]
    Malformed(#[from] serde_json::Error),
    /// The trace contained no session marker.
    #[error("missing session")]
    MissingSession,
}

/// A replayed session, holding events in original order.
#[derive(Debug, Clone)]
pub struct ReplaySession {
    /// The session this trace belongs to.
    pub session_id: SessionId,
    /// Events in original, replayable order.
    pub events: Vec<Event>,
}

impl ReplaySession {
    /// Load a session from a JSONL trace file.
    pub fn from_trace(path: &str) -> Result<Self, ReplayError> {
        let mut events = Vec::new();
        let mut session_id = None;
        for line in BufReader::new(File::open(path)?).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let ev: Event = serde_json::from_str(&line)?;
            if session_id.is_none() {
                session_id = ev.session_id.clone();
            }
            events.push(ev);
        }
        let session_id = session_id.ok_or(ReplayError::MissingSession)?;
        Ok(ReplaySession { session_id, events })
    }
}

/// A replayer that can walk a session's events.
#[derive(Debug, Clone)]
pub struct Replayer {
    /// Playback position.
    position: usize,
    /// Index of events by kind for fast skip.
    by_kind: HashMap<EventKind, Vec<usize>>,
    /// Replayable events.
    events: Vec<Event>,
}

impl Replayer {
    /// Create a replayer over a loaded session.
    pub fn new(session: &ReplaySession) -> Self {
        let mut by_kind: HashMap<EventKind, Vec<usize>> = HashMap::new();
        for (i, ev) in session.events.iter().enumerate() {
            by_kind.entry(ev.kind.clone()).or_default().push(i);
        }
        Self {
            position: 0,
            by_kind,
            events: session.events.clone(),
        }
    }

    /// The current playback position.
    pub fn position(&self) -> usize {
        self.position
    }

    /// The number of replayable events.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Whether there are no events.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Advance one event, returning it.
    pub fn next(&mut self) -> Option<&Event> {
        let ev = self.events.get(self.position)?;
        self.position += 1;
        Some(ev)
    }

    /// Jump to an event of the given kind, if any remains.
    pub fn seek_to_kind(&mut self, kind: EventKind) -> Option<&Event> {
        if let Some(idxs) = self.by_kind.get(&kind) {
            if let Some(&i) = idxs.iter().find(|&&i| i >= self.position) {
                self.position = i;
                return self.events.get(i);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_session() {
        let r = ReplaySession {
            session_id: "s_empty".into(),
            events: Vec::new(),
        };
        let r = Replayer::new(&r);
        assert!(r.is_empty());
    }
}
