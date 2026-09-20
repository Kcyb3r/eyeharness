//! Capability model ([plan §11, §39]).
//!
//! A capability is an actionable unit of computer control that a session (and,
//! transitively, a plugin) declares and is granted. Plugins can only call
//! capabilities listed in their manifest (§39.1).

use serde::{Deserialize, Serialize};

/// Namespaces for capabilities (dotted, e.g. `computer.click`).
pub const NAMESPACES: [&str; 4] = ["computer", "browser", "clipboard", "system"];

/// A capability identifier, e.g. `computer.click`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Capability(pub String);

impl Capability {
    /// The canonical [`NAMESPACES`] value check for a capability string.
    pub fn namespace(&self) -> Option<&'static str> {
        let ns = self.0.split('.').next()?;
        NAMESPACES.iter().copied().find(|n| *n == ns)
    }

    /// Whether this capability string is well-formed (`ns.action`).
    pub fn is_well_formed(&self) -> bool {
        let mut parts = self.0.split('.');
        parts.next().is_some() && parts.next().is_some() && parts.next().is_none()
    }
}

impl From<&str> for Capability {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for Capability {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for Capability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The full set of computer-control capabilities a session may hold.
pub const COMPUTER_CAPABILITIES: [&str; 10] = [
    "computer.observe",
    "computer.move",
    "computer.click",
    "computer.double_click",
    "computer.type",
    "computer.key",
    "computer.hotkey",
    "computer.scroll",
    "computer.copy",
    "computer.wait",
];

/// Browser capabilities.
pub const BROWSER_CAPABILITIES: [&str; 3] = ["browser.navigate", "browser.read", "browser.click"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn well_formed_capability_detected() {
        assert!(Capability("computer.click".into()).is_well_formed());
        assert!(!Capability("bogus".into()).is_well_formed());
        assert!(!Capability("a.b.c".into()).is_well_formed());
    }

    #[test]
    fn namespace_is_recognized() {
        assert_eq!(
            Capability("computer.click".into()).namespace(),
            Some("computer")
        );
        assert!(Capability("unknown.x".into()).namespace().is_none());
    }

    #[test]
    fn all_standard_capabilities_are_well_formed() {
        for c in COMPUTER_CAPABILITIES
            .iter()
            .chain(BROWSER_CAPABILITIES.iter())
        {
            assert!(Capability((*c).into()).is_well_formed(), "{c}");
        }
    }
}
