//! Target model ([plan §9, §16]).
//!
//! A [`Target`] is the result of resolving a natural-language request (e.g.
//! "click Submit") against the current observation. Every target carries a
//! confidence score that gates execution (§9.1).

use serde::{Deserialize, Serialize};

use crate::bounds::Bounds;
use crate::types::{ElementId, RequestId};

/// How a target was resolved ([plan §16]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetMethod {
    /// Exact semantic / accessibility match.
    Accessibility,
    /// DOM match.
    Dom,
    /// OCR text match.
    Ocr,
    /// Vision-model match.
    Vision,
    /// Fuzzy text / semantic match.
    Fuzzy,
    /// Direct coordinate fallback.
    Coordinates,
}

/// A resolved interaction target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Target {
    /// Opaque id of the resolution request.
    pub request_id: RequestId,
    /// Element id in the current observation this resolves to, if any.
    pub element_id: Option<ElementId>,
    /// Display name, e.g. `Submit`.
    pub name: String,
    /// Bounds of the target in physical pixels.
    pub bounds: Bounds,
    /// Resolver confidence in `0.0..=1.0`.
    pub confidence: f64,
    /// Which method produced this target.
    pub method: TargetMethod,
}

/// Confidence bands ([plan §9.1]).
pub mod bands {
    /// Minimum confidence for unguarded execution.
    pub const EXECUTE_MIN: f64 = 0.80;
    /// Below this, the target must not be executed and resolution is attempted
    /// again or the agent is asked for clarification.
    pub const RE_RESOLVE_MIN: f64 = 0.50;
}

impl Target {
    /// Whether this target may be executed unguarded under the default bands.
    pub fn is_executable(&self) -> bool {
        self.confidence >= bands::EXECUTE_MIN
    }

    /// Whether this target is in the re-resolve / ask-agent band.
    pub fn needs_clarification(&self) -> bool {
        self.confidence >= bands::RE_RESOLVE_MIN && self.confidence < bands::EXECUTE_MIN
    }

    /// Whether this target must be rejected outright.
    pub fn is_rejected(&self) -> bool {
        self.confidence < bands::RE_RESOLVE_MIN
    }

    /// The three-way verdict ([plan §9.1]): Execute / ReResolve / Reject.
    pub fn verdict(&self) -> TargetVerdict {
        if self.is_executable() {
            TargetVerdict::Execute
        } else if self.needs_clarification() {
            TargetVerdict::ReResolve
        } else {
            TargetVerdict::Reject
        }
    }
}

/// Three-way target verdict ([plan §9.1]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetVerdict {
    /// Safe to execute.
    Execute,
    /// Below execution threshold; re-observe / ask agent.
    ReResolve,
    /// Confidence too low; abort and log.
    Reject,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidently_resolved_target_is_executable() {
        let t = Target {
            request_id: "r1".into(),
            element_id: Some("e183".into()),
            name: "Submit".into(),
            bounds: Bounds::new(800, 600, 920, 650),
            confidence: 0.98,
            method: TargetMethod::Accessibility,
        };
        assert!(t.is_executable());
        assert_eq!(t.verdict(), TargetVerdict::Execute);
    }

    #[test]
    fn ambiguous_target_needs_clarification() {
        let t = Target {
            request_id: "r2".into(),
            element_id: Some("e5".into()),
            name: "Login".into(),
            bounds: Bounds::new(0, 0, 10, 10),
            confidence: 0.71,
            method: TargetMethod::Fuzzy,
        };
        assert!(t.needs_clarification());
        assert_eq!(t.verdict(), TargetVerdict::ReResolve);
    }

    #[test]
    fn weak_target_is_rejected() {
        let t = Target {
            request_id: "r3".into(),
            element_id: None,
            name: "?".into(),
            bounds: Bounds::new(0, 0, 10, 10),
            confidence: 0.32,
            method: TargetMethod::Vision,
        };
        assert!(t.is_rejected());
        assert_eq!(t.verdict(), TargetVerdict::Reject);
    }
}
