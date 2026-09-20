//! Session model ([plan §11]).

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::version::ProtocolVersion;

/// Opaque session identifier (e.g. `s_12`, `sess_82af`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct SessionId(pub String);

impl SessionId {
    /// Create a new session id from a string.
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for SessionId {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for SessionId {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Lifecycle status of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Session created but not yet started.
    Pending,
    /// Session is active and accepting agent traffic.
    Running,
    /// Paused by human takeover or policy.
    Paused,
    /// A human took over input; the agent is disconnected.
    Human,
    /// Terminated normally.
    Ended,
    /// Terminated by emergency stop.
    Aborted,
}

/// A session encapsulates a single agent↔harness interaction on one desktop.
///
/// Sessions are created by the harness (never deserialized from the wire), so
/// `Deserialize` is deliberately omitted; `Serialize` is kept for tracing and
/// the replay log.
#[derive(Debug, Clone, Serialize)]
pub struct Session {
    /// Unique session id.
    pub id: SessionId,
    /// Protocol version negotiated at session start.
    pub protocol: ProtocolVersion,
    /// Monotonic observation counter for this session.
    pub next_observation_id: crate::ObservationId,
    /// Current lifecycle status.
    pub status: SessionStatus,
    /// Created timestamp (ms).
    pub created_at_ms: crate::TimestampMs,
}

impl Session {
    /// Create a new session with the current protocol version.
    pub fn new(id: impl Into<SessionId>, created_at_ms: crate::TimestampMs) -> Self {
        Self {
            id: id.into(),
            protocol: ProtocolVersion::CURRENT,
            next_observation_id: 0,
            status: SessionStatus::Pending,
            created_at_ms,
        }
    }

    /// Allocate the next observation id and advance the counter.
    pub fn next_observation_id(&mut self) -> crate::ObservationId {
        let id = self.next_observation_id;
        self.next_observation_id += 1;
        id
    }

    /// Transition this session's status.
    pub fn set_status(&mut self, status: SessionStatus) {
        self.status = status;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocates_monotonic_observation_ids() {
        let mut s = Session::new("s_1", 0);
        assert_eq!(s.next_observation_id(), 0);
        assert_eq!(s.next_observation_id(), 1);
        assert_eq!(s.next_observation_id(), 2);
    }

    #[test]
    fn session_serializes_with_protocol() {
        let s = Session::new(SessionId::new("s_12"), 1789912345);
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["id"], "s_12");
        assert_eq!(v["protocol"]["major"], 1);
    }
}
