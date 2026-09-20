//! Policy rule definition: the unit of enforcement evaluated by [`super::Policy`].

use harness_protocol::{Action, Capability, PolicyDecision, SessionId};

use crate::MAX_TYPE_LENGTH;

/// What a matching rule tells the harness to do.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RuleAction {
    /// Permit the action.
    Allow,
    /// Deny the action.
    #[default]
    Block,
    /// Require human confirmation.
    Confirm,
}

/// A single policy rule: a set of matchers plus the decision applied when all
/// matchers match. The first matching rule in a [`super::Policy`] wins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PolicyRule {
    /// Capabilities that trigger this rule (empty = any).
    capabilities: Vec<Capability>,
    /// The decision to apply.
    pub decision: PolicyDecision,
    /// Free-form comment (not serialized into decisions).
    note: String,
}

impl PolicyRule {
    /// Start building a rule.
    pub fn builder() -> RuleBuilder {
        RuleBuilder::default()
    }

    /// Whether this rule matches the given session/action pair.
    pub fn matches(&self, _session: &SessionId, action: &Action) -> bool {
        if self.capabilities.is_empty() {
            return true;
        }
        self.capabilities
            .iter()
            .any(|cap| action_matches_capability(action, cap))
    }
}

/// Fluent builder for [`PolicyRule`].
#[derive(Debug, Clone, Default)]
pub struct RuleBuilder {
    capabilities: Vec<Capability>,
    action: Option<RuleAction>,
    note: String,
}

impl RuleBuilder {
    /// Match a specific capability (e.g. `computer.click`).
    pub fn capability(mut self, cap: impl Into<Capability>) -> Self {
        self.capabilities.push(cap.into());
        self
    }

    /// Match any of the given capabilities (OR).
    pub fn capabilities(mut self, caps: impl IntoIterator<Item = Capability>) -> Self {
        self.capabilities.extend(caps);
        self
    }

    /// Set whether the rule allows / blocks / confirms.
    pub fn action(mut self, action: RuleAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Attach a descriptive note.
    pub fn note(mut self, note: impl Into<String>) -> Self {
        self.note = note.into();
        self
    }

    /// Build the rule.
    pub fn build(self) -> PolicyRule {
        let decision = match self.action.unwrap_or(RuleAction::Block) {
            RuleAction::Allow => PolicyDecision::Allowed {
                reason: Some(self.note.clone()),
            },
            RuleAction::Block => PolicyDecision::Blocked {
                reason: self.note.clone(),
            },
            RuleAction::Confirm => PolicyDecision::ConfirmationRequired {
                timeout_ms: harness_protocol::policy::CONFIRM_TIMEOUT_MS,
            },
        };
        PolicyRule {
            capabilities: self.capabilities,
            decision,
            note: self.note,
        }
    }
}

fn action_matches_capability(action: &Action, cap: &Capability) -> bool {
    use harness_protocol::{PrimitiveAction, SemanticAction};
    let name = match &action.kind {
        harness_protocol::ActionKind::Primitive(p) => match p {
            PrimitiveAction::Move { .. } => "move",
            PrimitiveAction::Click { .. } => "click",
            PrimitiveAction::RightClick { .. } => "right_click",
            PrimitiveAction::DoubleClick { .. } => "double_click",
            PrimitiveAction::Type { text } => {
                // Structural guard, not policy: reject oversized type payloads.
                if text.len() > MAX_TYPE_LENGTH {
                    return false;
                }
                "type"
            }
            PrimitiveAction::Keypress { .. } => "key",
            PrimitiveAction::Hotkey { .. } => "hotkey",
            PrimitiveAction::Scroll { .. } => "scroll",
            PrimitiveAction::Drag { .. } => "drag",
            PrimitiveAction::Wait { .. } => "wait",
        },
        harness_protocol::ActionKind::Semantic(s) => match s {
            SemanticAction::Find { .. } => "find",
            SemanticAction::ClickTarget { .. } => "click",
            SemanticAction::Read { .. } => "read",
            SemanticAction::WaitFor { .. } => "wait_for",
            SemanticAction::Select { .. } => "select",
            SemanticAction::Navigate { .. } => "navigate",
        },
    };

    let cap_name = cap.0.as_str();
    if cap_name == "computer.*" {
        return matches!(
            name,
            "observe"
                | "move"
                | "click"
                | "right_click"
                | "double_click"
                | "type"
                | "key"
                | "keypress"
                | "hotkey"
                | "scroll"
                | "drag"
                | "copy"
                | "find"
                | "read"
                | "select"
                | "navigate"
                | "wait"
                | "wait_for"
        );
    }
    if let Some(ns) = cap_name.strip_suffix(".*") {
        return ns == "computer"; // any namespace-local rule matches
    }
    let expected = cap_name.rsplit('.').next().unwrap_or_default();
    cap_name.starts_with("computer.")
        && (expected == name
            || (expected == "key" && name == "keypress")
            || (expected == "keypress" && name == "key"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_protocol::{Action, ActionKind, PrimitiveAction, SessionId};

    fn session() -> SessionId {
        SessionId::new("s_1")
    }

    fn action(kind: ActionKind) -> Action {
        Action {
            id: "a".into(),
            session_id: session(),
            observation_id: 1,
            kind,
        }
    }

    #[test]
    fn capability_matcher_ignores_other_namespaces() {
        let cap = Capability::from("browser.navigate");
        let click = action(ActionKind::Primitive(PrimitiveAction::Click { x: 1, y: 1 }));
        assert!(!action_matches_capability(&click, &cap));
    }

    #[test]
    fn click_action_matches_click_capability() {
        let cap = Capability::from("computer.click");
        let click = action(ActionKind::Primitive(PrimitiveAction::Click { x: 1, y: 1 }));
        assert!(action_matches_capability(&click, &cap));
    }

    #[test]
    fn type_action_matches_type_capability() {
        let cap = Capability::from("computer.type");
        let t = action(ActionKind::Primitive(PrimitiveAction::Type {
            text: "hi".into(),
        }));
        assert!(action_matches_capability(&t, &cap));
    }

    #[test]
    fn wildcard_matches_any_computer_action() {
        let cap = Capability::from("computer.*");
        let hotkey = action(ActionKind::Primitive(PrimitiveAction::Hotkey {
            modifiers: vec!["ctrl".into()],
            key: "c".into(),
        }));
        assert!(action_matches_capability(&hotkey, &cap));
    }
}
