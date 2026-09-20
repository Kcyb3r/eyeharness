//! The central event model ([plan §18]).
//!
//! A single event stream feeds the logger, HUD, replay, and metrics. Events
//! here are the wire-container for the event system crate.

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

use crate::types::TimestampMs;

/// The kind of event on the bus ([plan §18], extended by implemented contracts).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventKind {
    /* session */
    /// `session.started`
    SessionStarted,
    /// `session.ended`
    SessionEnded,
    /// `session.paused`
    SessionPaused,
    /// `session.resumed`
    SessionResumed,

    /* observation */
    /// `observation.created`
    ObservationCreated,
    /// `observation.changed`
    ObservationChanged,
    /// `observation.stale`
    ObservationStale,
    /// `observation.fresh`
    ObservationFresh,

    /* agent */
    /// `agent.connected`
    AgentConnected,
    /// `agent.disconnected`
    AgentDisconnected,
    /// `agent.decision`
    AgentDecision,

    /* action */
    /// `action.requested`
    ActionRequested,
    /// `action.validated`
    ActionValidated,
    /// `action.executed`
    ActionExecuted,
    /// `action.failed`
    ActionFailed,
    /// `action.invalid`
    ActionInvalid,
    /// `action.stale`
    ActionRejected,

    /* target */
    /// `target.resolved`
    TargetResolved,
    /// `target.failed`
    TargetFailed,
    /// `target.low_confidence`
    TargetLowConfidence,

    /* verification */
    /// `verification.started`
    VerificationStarted,
    /// `verification.passed`
    VerificationPassed,
    /// `verification.failed`
    VerificationFailed,

    /* policy */
    /// `policy.allowed`
    PolicyAllowed,
    /// `policy.blocked`
    PolicyBlocked,
    /// `policy.confirmation_required`
    PolicyConfirmationRequired,

    /* recovery */
    /// `recovery.started`
    RecoveryStarted,
    /// `recovery.completed`
    RecoveryCompleted,
    /// `recovery.failed`
    RecoveryFailed,

    /* window / clipboard / navigation */
    /// `window.changed`
    WindowChanged,
    /// `clipboard.changed`
    ClipboardChanged,
    /// `dialog.opened`
    DialogOpened,
    /// `navigation.changed`
    NavigationChanged,

    /* plugins */
    /// `plugin.crash`
    PluginCrash,
    /// `plugin.invalid`
    PluginInvalid,
    /// `plugin.loaded`
    PluginLoaded,
    /// `plugin.failed`
    PluginFailed,
    /// `plugin.unloaded`
    PluginUnloaded,

    /* human */
    /// `human.takeover`
    HumanTakeover,
    /// `human.taken_over`
    HumanTakenOver,

    /* system */
    /// `system.health` — periodic / on-demand health heartbeat.
    SystemHealth,
    /// `system.error`
    SystemError,
}

impl Serialize for EventKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.as_str().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for EventKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::from_str(&value)
            .ok_or_else(|| de::Error::custom(format!("unknown event kind: {value}")))
    }
}

impl EventKind {
    /// The canonical dotted string, e.g. `observation.created`.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SessionStarted => "session.started",
            Self::SessionEnded => "session.ended",
            Self::SessionPaused => "session.paused",
            Self::SessionResumed => "session.resumed",
            Self::ObservationCreated => "observation.created",
            Self::ObservationChanged => "observation.changed",
            Self::ObservationStale => "observation.stale",
            Self::ObservationFresh => "observation.fresh",
            Self::AgentConnected => "agent.connected",
            Self::AgentDisconnected => "agent.disconnected",
            Self::AgentDecision => "agent.decision",
            Self::ActionRequested => "action.requested",
            Self::ActionValidated => "action.validated",
            Self::ActionExecuted => "action.executed",
            Self::ActionFailed => "action.failed",
            Self::ActionInvalid => "action.invalid",
            Self::ActionRejected => "action.stale",
            Self::TargetResolved => "target.resolved",
            Self::TargetFailed => "target.failed",
            Self::TargetLowConfidence => "target.low_confidence",
            Self::VerificationStarted => "verification.started",
            Self::VerificationPassed => "verification.passed",
            Self::VerificationFailed => "verification.failed",
            Self::PolicyAllowed => "policy.allowed",
            Self::PolicyBlocked => "policy.blocked",
            Self::PolicyConfirmationRequired => "policy.confirmation_required",
            Self::RecoveryStarted => "recovery.started",
            Self::RecoveryCompleted => "recovery.completed",
            Self::RecoveryFailed => "recovery.failed",
            Self::WindowChanged => "window.changed",
            Self::ClipboardChanged => "clipboard.changed",
            Self::DialogOpened => "dialog.opened",
            Self::NavigationChanged => "navigation.changed",
            Self::PluginCrash => "plugin.crash",
            Self::PluginInvalid => "plugin.invalid",
            Self::PluginFailed => "plugin.failed",
            Self::PluginLoaded => "plugin.loaded",
            Self::PluginUnloaded => "plugin.unloaded",
            Self::HumanTakeover => "human.takeover",
            Self::HumanTakenOver => "human.taken_over",
            Self::SystemHealth => "system.health",
            Self::SystemError => "system.error",
        }
    }

    fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "session.started" => Self::SessionStarted,
            "session.ended" => Self::SessionEnded,
            "session.paused" => Self::SessionPaused,
            "session.resumed" => Self::SessionResumed,
            "observation.created" => Self::ObservationCreated,
            "observation.changed" => Self::ObservationChanged,
            "observation.stale" => Self::ObservationStale,
            "observation.fresh" => Self::ObservationFresh,
            "agent.connected" => Self::AgentConnected,
            "agent.disconnected" => Self::AgentDisconnected,
            "agent.decision" => Self::AgentDecision,
            "action.requested" => Self::ActionRequested,
            "action.validated" => Self::ActionValidated,
            "action.executed" => Self::ActionExecuted,
            "action.failed" => Self::ActionFailed,
            "action.invalid" => Self::ActionInvalid,
            "action.stale" => Self::ActionRejected,
            "target.resolved" => Self::TargetResolved,
            "target.failed" => Self::TargetFailed,
            "target.low_confidence" => Self::TargetLowConfidence,
            "verification.started" => Self::VerificationStarted,
            "verification.passed" => Self::VerificationPassed,
            "verification.failed" => Self::VerificationFailed,
            "policy.allowed" => Self::PolicyAllowed,
            "policy.blocked" => Self::PolicyBlocked,
            "policy.confirmation_required" => Self::PolicyConfirmationRequired,
            "recovery.started" => Self::RecoveryStarted,
            "recovery.completed" => Self::RecoveryCompleted,
            "recovery.failed" => Self::RecoveryFailed,
            "window.changed" => Self::WindowChanged,
            "clipboard.changed" => Self::ClipboardChanged,
            "dialog.opened" => Self::DialogOpened,
            "navigation.changed" => Self::NavigationChanged,
            "plugin.crash" => Self::PluginCrash,
            "plugin.invalid" => Self::PluginInvalid,
            "plugin.loaded" => Self::PluginLoaded,
            "plugin.failed" => Self::PluginFailed,
            "plugin.unloaded" => Self::PluginUnloaded,
            "human.takeover" => Self::HumanTakeover,
            "human.taken_over" => Self::HumanTakenOver,
            "system.health" => Self::SystemHealth,
            "system.error" => Self::SystemError,
            _ => return None,
        })
    }
}

/// A single structured event on the bus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// Event kind, serialized as the dotted name.
    pub kind: EventKind,
    /// The session this event belongs to, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<crate::SessionId>,
    /// Timestamp (ms).
    pub timestamp: TimestampMs,
    /// The observation this event is associated with, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_id: Option<crate::ObservationId>,
    /// Optional free-form payload.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub payload: serde_json::Value,
}

impl Event {
    /// Create a new event with the given kind and timestamp.
    pub fn new(kind: EventKind, timestamp: TimestampMs) -> Self {
        Self {
            kind,
            session_id: None,
            timestamp,
            observation_id: None,
            payload: serde_json::Value::Null,
        }
    }

    /// Attach a session.
    pub fn in_session(mut self, session_id: crate::SessionId) -> Self {
        self.session_id = Some(session_id);
        self
    }

    /// Attach an observation id.
    pub fn for_observation(mut self, observation_id: crate::ObservationId) -> Self {
        self.observation_id = Some(observation_id);
        self
    }

    /// Attach a payload.
    pub fn with_payload(mut self, payload: impl Serialize) -> Self {
        self.payload = serde_json::to_value(payload).unwrap_or(serde_json::Value::Null);
        self
    }
}

impl std::fmt::Display for EventKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_serialize_to_dotted_names() {
        assert_eq!(
            serde_json::to_value(EventKind::SessionStarted).unwrap(),
            "session.started"
        );
        assert_eq!(
            serde_json::to_value(EventKind::PolicyBlocked).unwrap(),
            "policy.blocked"
        );
        assert_eq!(
            serde_json::to_value(EventKind::HumanTakeover).unwrap(),
            "human.takeover"
        );
        assert_eq!(EventKind::ObservationStale.as_str(), "observation.stale");
    }

    #[test]
    fn event_is_self_describing() {
        let e = Event::new(EventKind::ActionExecuted, 1000)
            .in_session("s_1".into())
            .with_payload(serde_json::json!({"action": "click"}));
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "action.executed");
        assert_eq!(v["session_id"], "s_1");
        assert_eq!(v["payload"]["action"], "click");
    }
}
