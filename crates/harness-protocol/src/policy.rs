//! Policy decision model ([plan §3.3.1]).
//!
//! The policy engine returns exactly one of three verdicts:
//! `allow`, `block`, or `confirm`. A `block` aborts the action with no retry
//! unless a materially different request is submitted; a `confirm` requires a
//! human response with a 30-second timeout.

use serde::{Deserialize, Serialize};

/// Maximum wait time (ms) for a human confirmation prompt.
pub const CONFIRM_TIMEOUT_MS: u64 = 30_000;

/// The decision a policy engine returns for an attempted action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum PolicyDecision {
    /// Action may proceed to the executor.
    Allowed {
        /// Human-readable rationale (may be omitted).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Action is aborted. No retry unless a materially different request.
    Blocked {
        /// Why the action was blocked.
        reason: String,
    },
    /// Action waits for a human response (timeout = [`CONFIRM_TIMEOUT_MS`]).
    ConfirmationRequired {
        /// How long the prompt waits before being treated as denied.
        #[serde(default = "default_confirm_timeout")]
        timeout_ms: u64,
    },
}

fn default_confirm_timeout() -> u64 {
    CONFIRM_TIMEOUT_MS
}

impl PolicyDecision {
    /// Whether the decision allows execution.
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }

    /// Short status string (`allowed`, `blocked`, `confirmation_required`).
    pub fn status_str(&self) -> &'static str {
        match self {
            Self::Allowed { .. } => "allowed",
            Self::Blocked { .. } => "blocked",
            Self::ConfirmationRequired { .. } => "confirmation_required",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decisions_serialize_to_expected_names() {
        let allowed = serde_json::to_value(PolicyDecision::Allowed { reason: None }).unwrap();
        assert_eq!(allowed["decision"], "allowed");

        let blocked = serde_json::to_value(PolicyDecision::Blocked {
            reason: "purchasing requires confirmation".into(),
        })
        .unwrap();
        assert_eq!(blocked["decision"], "blocked");

        let confirm =
            serde_json::to_value(PolicyDecision::ConfirmationRequired { timeout_ms: 30_000 })
                .unwrap();
        assert_eq!(confirm["decision"], "confirmation_required");
        assert_eq!(confirm["timeout_ms"], 30_000);
    }

    #[test]
    fn allowed_and_blocked_verify() {
        assert!(PolicyDecision::Allowed { reason: None }.is_allowed());
        assert!(!PolicyDecision::Blocked {
            reason: "no".into()
        }
        .is_allowed());
        assert_eq!(
            PolicyDecision::ConfirmationRequired { timeout_ms: 30_000 }.status_str(),
            "confirmation_required"
        );
    }
}
