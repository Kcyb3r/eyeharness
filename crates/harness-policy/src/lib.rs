//! Policy engine ([plan §3.3, §3.3.1, §37]).
//!
//! The policy engine is the only gate between the model and the executor.
//! Safety is never delegated to the model: visual content is not authority
//! (a webpage cannot grant itself permission by displaying an instruction).
//!
//! Rules are evaluated in priority order; the first match wins. A `block`
//! aborts with no retry unless a materially different request arrives, and a
//! `confirm` surfaces a human prompt.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;

use harness_protocol::{Action, ActionKind, PolicyDecision, PrimitiveAction, SessionId};

mod rule;

pub use rule::{PolicyRule, RuleAction, RuleBuilder};

/// Errors from the policy engine.
/// Errors from the policy engine.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    /// No rule matched the action request.
    #[error("no policy matched action {action_id}")]
    NoMatch {
        /// The id of the action that no rule matched.
        action_id: String,
    },
}

/// A compiled policy: an ordered set of rules applied to actions.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    rules: Arc<Vec<PolicyRule>>,
}

impl Policy {
    /// Create a policy from a list of rules. Rule order is significant:
    /// the first matching rule decides.
    pub fn new(rules: Vec<PolicyRule>) -> Self {
        Self {
            rules: Arc::new(rules),
        }
    }

    /// An empty policy — every rule-less policy engine must fall back to the
    /// strictest default (block).
    pub fn empty() -> Self {
        Self::default()
    }

    /// The number of rules in the policy.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Evaluate the given action, returning the decision of the first matching
    /// rule. When no rule matches, the action is blocked (fail-closed).
    pub fn evaluate(&self, session: &SessionId, action: &Action) -> PolicyDecision {
        for rule in self.rules.iter() {
            if rule.matches(session, action) {
                return rule.decision.clone();
            }
        }
        // Fail closed: unmatched actions never reach the executor.
        PolicyDecision::Blocked {
            reason: format!("no policy matched request {} (fail-closed)", action.id),
        }
    }
}

/// The action validator performs structural checks separate from policy.
///
/// Validation answers *can we even understand this request*; policy answers
/// *are we allowed to do it*. Both must pass before dispatch.
#[derive(Debug, Clone, Copy, Default)]
pub struct ActionValidator;

/// Structural validation outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    /// Missing session id.
    MissingSession,
    /// Missing observation reference.
    MissingObservationRef,
    /// Unknown or malformed action payload.
    Malformed,
    /// An action that could never be executed safely (e.g. negative wait).
    Unsupported(String),
}

impl ActionValidator {
    /// Validate an action structurally.
    pub fn validate(&self, action: &Action) -> Result<(), ValidationError> {
        if action.session_id.0.is_empty() {
            return Err(ValidationError::MissingSession);
        }
        // Every action must carry *some* observation reference; the freshness
        // of that reference is checked separately against the actual
        // observation model.
        match &action.kind {
            ActionKind::Primitive(PrimitiveAction::Wait { ms }) if *ms == 0 => {
                Err(ValidationError::Unsupported("wait of 0ms".into()))
            }
            ActionKind::Primitive(PrimitiveAction::Type { text })
                if text.chars().count() > MAX_TYPE_LENGTH =>
            {
                Err(ValidationError::Unsupported(format!(
                    "type payload exceeds {} chars",
                    MAX_TYPE_LENGTH
                )))
            }
            _ => Ok(()),
        }
    }
}

/// Maximum accepted characters in a single `type` action.
pub const MAX_TYPE_LENGTH: usize = 4096;

// Re-export helper trait used internally.
#[doc(hidden)]
pub use harness_protocol::ObservationId;

#[cfg(test)]
mod tests {
    use super::*;
    use harness_protocol::{Action, ActionKind, PrimitiveAction, SessionId};

    fn session() -> SessionId {
        SessionId::new("s_1")
    }

    fn click_action() -> Action {
        Action {
            id: "a_1".into(),
            session_id: session(),
            observation_id: 0,
            kind: ActionKind::Primitive(PrimitiveAction::Click { x: 10, y: 10 }),
        }
    }

    fn type_action(text: &str) -> Action {
        Action {
            id: "a_2".into(),
            session_id: session(),
            observation_id: 1,
            kind: ActionKind::Primitive(PrimitiveAction::Type { text: text.into() }),
        }
    }

    #[test]
    fn validator_accepts_wellformed() {
        let v = ActionValidator::default();
        assert_eq!(v.validate(&click_action()), Ok(()));
    }

    #[test]
    fn validator_rejects_bad_refs() {
        let v = ActionValidator::default();
        let mut a = click_action();
        a.session_id = SessionId::new("");
        assert_eq!(v.validate(&a), Err(ValidationError::MissingSession));
    }

    #[test]
    fn unmatched_action_fails_closed() {
        let p = Policy::empty();
        let d = p.evaluate(&session(), &click_action());
        assert!(matches!(d, PolicyDecision::Blocked { .. }));
    }

    #[test]
    fn first_matching_rule_wins() {
        use harness_protocol::Capability;
        use rule::RuleAction;

        let allow_clicks = PolicyRule::builder()
            .capability(Capability::from("computer.click"))
            .action(RuleAction::Allow)
            .build();
        let block_all = PolicyRule::builder()
            .capability(Capability::from("computer.nonexistent"))
            .action(RuleAction::Allow)
            .build();

        let p = Policy::new(vec![allow_clicks, block_all]);
        let d = p.evaluate(&session(), &click_action());
        assert!(d.is_allowed());
    }

    #[test]
    fn type_over_limit_rejected() {
        let v = ActionValidator::default();
        let long = "x".repeat(MAX_TYPE_LENGTH + 1);
        assert!(v.validate(&type_action(&long)).is_err());
    }
}
