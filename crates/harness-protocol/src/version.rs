//! Protocol versioning ([plan §11]).
//!
//! The protocol is versioned from the beginning. The version is stamped on
//! every message and negotiated at session start.

use serde::Serialize;

/// A versioned protocol identifier, e.g. `harness://protocol/v1`.
///
/// Fields are `&'static str` so the struct can be built in a `const` context.
/// `Serialize` is derived for tracing/negotiation; `Deserialize` is deliberately
/// skipped so a `ProtocolVersion` can never be constructed from unvalidated
/// input at runtime (`CURRENT` is the only legal value).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct ProtocolVersion {
    scheme: &'static str,
    name: &'static str,
    major: u32,
}

impl ProtocolVersion {
    /// The current protocol version: `harness://protocol/v1`.
    pub const CURRENT: Self = Self {
        scheme: "harness",
        name: "protocol",
        major: 1,
    };

    /// Create a new version identifier. All inputs must be `'static` so the
    /// struct can be built in a `const` context (and thus usable as a compile
    /// time constant elsewhere).
    pub const fn new(scheme: &'static str, name: &'static str, major: u32) -> Self {
        Self {
            scheme,
            name,
            major,
        }
    }

    /// The version component (e.g. `1` for `v1`).
    pub fn major(&self) -> u32 {
        self.major
    }

    /// Whether this version is compatible with another version.
    ///
    /// Both must share scheme and name, and this major must equal the other's.
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self.scheme == other.scheme && self.name == other.name && self.major == other.major
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self::CURRENT
    }
}

impl std::fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}://{}/v{}", self.scheme, self.name, self.major)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_version_displays_correctly() {
        assert_eq!(
            ProtocolVersion::CURRENT.to_string(),
            "harness://protocol/v1"
        );
    }

    #[test]
    fn identical_versions_are_compatible() {
        let a = ProtocolVersion::CURRENT;
        let b = ProtocolVersion::new("harness", "protocol", 1);
        assert!(a.is_compatible_with(&b));
        assert!(b.is_compatible_with(&a));
    }

    #[test]
    fn divergent_majors_are_incompatible() {
        let a = ProtocolVersion::new("harness", "protocol", 1);
        let b = ProtocolVersion::new("harness", "protocol", 2);
        assert!(!a.is_compatible_with(&b));
    }

    #[test]
    fn serializes_to_string() {
        let v = ProtocolVersion::CURRENT;
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(
            s,
            "{\"scheme\":\"harness\",\"name\":\"protocol\",\"major\":1}"
        );
    }
}
