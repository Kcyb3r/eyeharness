//! Action model ([plan §10, §16, §18]).
//!
//! An [`Action`] is what the agent requests; the harness decides whether and
//! how it is executed. Every action carries the `observation_id` it was
//! derived from so the harness can enforce the freshness rule (§11.1).

use serde::{Deserialize, Serialize};

use crate::bounds::Bounds;
use crate::target::Target;
use crate::types::{RequestId, TimestampMs};
use crate::ObservationId;

/// Primitive executor-level actions ([plan §10]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum PrimitiveAction {
    /// Move the pointer to coordinates.
    Move {
        /// Target coordinates.
        x: i64,
        /// Target coordinates.
        y: i64,
    },
    /// Left click at coordinates.
    Click {
        /// X coordinate.
        x: i64,
        /// Y coordinate.
        y: i64,
    },
    /// Right click.
    RightClick {
        /// X coordinate.
        x: i64,
        /// Y coordinate.
        y: i64,
    },
    /// Double click.
    DoubleClick {
        /// X coordinate.
        x: i64,
        /// Y coordinate.
        y: i64,
    },
    /// Type text (characters).
    Type {
        /// Text to type.
        #[serde(default)]
        text: String,
    },
    /// Press a single key.
    Keypress {
        /// Key name (e.g. `enter`, `tab`).
        key: String,
    },
    /// Press a hotkey combination.
    Hotkey {
        /// Modifier list (e.g. `ctrl`, `alt`, `shift`).
        #[serde(default)]
        modifiers: Vec<String>,
        /// The primary key.
        key: String,
    },
    /// Scroll the wheel.
    Scroll {
        /// Delta lines.
        delta: i64,
        /// X coordinate.
        x: i64,
        /// Y coordinate.
        y: i64,
    },
    /// Drag from one point to another.
    Drag {
        /// X coordinate.
        x0: i64,
        /// Y coordinate.
        y0: i64,
        /// X coordinate.
        x1: i64,
        /// Y coordinate.
        y1: i64,
    },
    /// Wait a duration in ms.
    Wait {
        /// Milliseconds.
        ms: u64,
    },
}

/// Semantic, intent-oriented actions ([plan §10]) executed by the harness
/// through possibly several internal operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SemanticAction {
    /// Find a target by text/name and report it.
    Find {
        /// The text to find.
        text: String,
    },
    /// Find and click a target.
    ClickTarget {
        /// The text to find.
        text: String,
    },
    /// Read text/elements from the current screen.
    Read {
        /// Optional region hint.
        #[serde(default)]
        region: Option<Bounds>,
    },
    /// Wait until a target appears.
    WaitFor {
        /// The text to wait for.
        text: String,
        /// Timeout in ms.
        #[serde(default = "default_wait_for_timeout")]
        timeout_ms: u64,
    },
    /// Select an option from a control by displayed text.
    Select {
        /// The text to find.
        text: String,
        /// The option value to select.
        value: String,
    },
    /// Navigate the browser.
    Navigate {
        /// Target URL.
        url: String,
    },
}

fn default_wait_for_timeout() -> u64 {
    5_000
}

/// An agent→harness action request ([plan §11]).
///
/// The protocol mandates that every action references the observation it was
/// based on; a stale `observation_id` must be rejected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Action {
    /// Opaque request id (e.g. `a_1042`).
    pub id: RequestId,
    /// The session this action belongs to.
    pub session_id: crate::SessionId,
    /// The observation this action was derived from.
    pub observation_id: ObservationId,
    /// The action itself.
    #[serde(flatten)]
    pub kind: ActionKind,
}

/// Which action family the request uses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ActionKind {
    /// Primitive action.
    Primitive(PrimitiveAction),
    /// Semantic action.
    Semantic(SemanticAction),
}

/// Outcome status of an executed action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    /// The action executed and verification passed.
    Success,
    /// The action executed at the OS level but verification failed.
    Failed,
    /// The action was rejected before execution (policy / freshness / target).
    Rejected,
    /// The action was interrupted (human takeover, emergency stop).
    Interrupted,
}

/// The result of executing an action ([plan §16]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionResult {
    /// The request id this result answers.
    pub action_id: RequestId,
    /// Outcome status.
    pub status: ActionStatus,
    /// Optional resolved target used for execution.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Target>,
    /// Screen changed materially after the action.
    #[serde(default)]
    pub screen_changed: bool,
    /// The observation produced after the action.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation_after: Option<ObservationId>,
    /// Verification detail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification: Option<VerificationResult>,
    /// Rejection reason, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rejection: Option<String>,
    /// End-to-end latency in ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    /// When the action completed (ms).
    pub timestamp: TimestampMs,
}

/// Verification outcome attached to an [`ActionResult`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether verification passed.
    pub ok: bool,
    /// Human-readable detail.
    #[serde(default)]
    pub detail: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_roundtrips_and_carries_observation_ref() {
        let a = Action {
            id: "a_1042".into(),
            session_id: "s_12".into(),
            observation_id: 1842,
            kind: ActionKind::Primitive(PrimitiveAction::Click { x: 812, y: 641 }),
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["observation_id"], 1842);
        assert_eq!(v["action"], "click");
        let back: Action = serde_json::from_value(v).unwrap();
        assert_eq!(back.id, "a_1042");
        assert_eq!(back.observation_id, 1842);
    }

    #[test]
    fn semantic_action_serializes() {
        let a = Action {
            id: "a_1".into(),
            session_id: "s_1".into(),
            observation_id: 3,
            kind: ActionKind::Semantic(SemanticAction::ClickTarget {
                text: "Submit".into(),
            }),
        };
        let v = serde_json::to_value(&a).unwrap();
        assert_eq!(v["action"], "click_target");
        assert_eq!(v["text"], "Submit");
    }

    #[test]
    fn action_result_rejects_without_claiming_success() {
        let r = ActionResult {
            action_id: "a_9".into(),
            status: ActionStatus::Rejected,
            target: None,
            screen_changed: false,
            observation_after: None,
            verification: None,
            rejection: Some("stale observation (age 812ms > 500ms)".into()),
            latency_ms: Some(1),
            timestamp: 1000,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["status"], "rejected");
        assert!(v["verification"].is_null());
        assert!(v["rejection"].is_string());
    }

    #[test]
    fn success_requires_verification() {
        let r = ActionResult {
            action_id: "a_10".into(),
            status: ActionStatus::Success,
            target: None,
            screen_changed: true,
            observation_after: Some(884),
            verification: Some(VerificationResult {
                ok: true,
                detail: "expected state transitioned".into(),
            }),
            rejection: None,
            latency_ms: Some(43),
            timestamp: 2000,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["status"], "success");
        assert_eq!(v["verification"]["ok"], true);
    }
}
