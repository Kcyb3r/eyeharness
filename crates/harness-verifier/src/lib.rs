//! Action verification ([plan §16]).
//!
//! Every meaningful action is framed as precondition → action → postcondition.
//! The harness never reports `success=true` merely because the OS accepted the
//! input; it must observe the expected state transition.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use harness_protocol::{Observation, ObservationId};

/// Verification outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Expected postcondition observed.
    Passed,
    /// Postcondition not observed within the timeout.
    Failed(VerifyFailure),
    /// The action had no meaningful side effect to verify.
    NotApplicable,
}

/// Why verification failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyFailure {
    /// Screen did not change at all after the action.
    NoScreenChange,
    /// The expected target vanished before verification.
    TargetDisappeared,
    /// A dialog/popup appeared unexpectedly.
    UnexpectedDialog,
    /// Timed out waiting for the screen to reach the expected state.
    Timeout,
}

/// Evidence captured for an [`ActionResult`] ([plan §16]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    /// The observation id before the action.
    pub observation_before: Option<ObservationId>,
    /// The observation id after the action.
    pub observation_after: Option<ObservationId>,
    /// Whether the screen changed materially.
    pub screen_changed: bool,
    /// The outcome.
    pub outcome: VerifyOutcome,
}

/// A verifier implements a concrete precondition/postcondition check.
///
/// The harness runs one or more verifiers after each semantic action and fuses
/// the strongest available evidence.
pub trait Verifier: Send + Sync {
    /// Check a postcondition using the observations produced before and after
    /// the action.
    fn verify(&self, before: Option<&Observation>, after: &Observation) -> VerifyOutcome;
}

/// Simple structural verifier: uses `screen_changed` and element liveness as
/// weak-but-always-available evidence.
#[derive(Debug, Clone, Copy, Default)]
pub struct ScreenChangeVerifier;

impl Verifier for ScreenChangeVerifier {
    fn verify(&self, _before: Option<&Observation>, after: &Observation) -> VerifyOutcome {
        if after.screen_changed {
            VerifyOutcome::Passed
        } else {
            VerifyOutcome::Failed(VerifyFailure::NoScreenChange)
        }
    }
}

/// A verifier for "target should now exist" postconditions, e.g. after a click
/// that opens a dialog or navigates.
#[derive(Debug, Clone, Default)]
pub struct TargetPresenceVerifier(pub String);

impl Verifier for TargetPresenceVerifier {
    fn verify(&self, _before: Option<&Observation>, after: &Observation) -> VerifyOutcome {
        let present = after
            .elements
            .iter()
            .any(|e| e.name.eq_ignore_ascii_case(&self.0));
        if present {
            VerifyOutcome::Passed
        } else {
            VerifyOutcome::Failed(VerifyFailure::TargetDisappeared)
        }
    }
}

/// Runs a verification and produces the evidence report.
pub fn run_verification(
    before: Option<&Observation>,
    after: &Observation,
    verifier: &dyn Verifier,
) -> VerificationReport {
    let outcome = verifier.verify(before, after);
    VerificationReport {
        observation_before: before.map(|o| o.observation_id),
        observation_after: Some(after.observation_id),
        screen_changed: after.screen_changed,
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs(id: u64, changed: bool, names: &[&str]) -> Observation {
        let mut ob = Observation::new("s_1".into(), id, 0);
        ob.screen_changed = changed;
        ob.elements = names
            .iter()
            .enumerate()
            .map(|(i, n)| harness_protocol::Element {
                id: format!("e{i}"),
                role: "button".into(),
                name: (*n).into(),
                text: None,
                bounds: harness_protocol::Bounds::new(0, 0, 10, 10),
                enabled: true,
                focused: false,
                confidence: 1.0,
                source: harness_protocol::PerceptionSource::Accessibility,
            })
            .collect();
        ob
    }

    #[test]
    fn screen_change_verifier_passes_on_change() {
        let before = obs(1, false, &["Skip"]);
        let after = obs(2, true, &["Submit"]);
        let report = run_verification(Some(&before), &after, &ScreenChangeVerifier);
        assert_eq!(report.outcome, VerifyOutcome::Passed);
        assert_eq!(report.observation_before, Some(1));
        assert_eq!(report.observation_after, Some(2));
        assert!(report.screen_changed);
    }

    #[test]
    fn screen_change_verifier_fails_when_no_change() {
        let after = obs(2, false, &["Submit"]);
        let report = run_verification(None, &after, &ScreenChangeVerifier);
        assert!(matches!(
            report.outcome,
            VerifyOutcome::Failed(VerifyFailure::NoScreenChange)
        ));
    }

    #[test]
    fn target_presence_verifier_detects_appearance() {
        let before = obs(1, false, &["Skip"]);
        let after = obs(2, true, &["Skip", "Confirm deletion"]);
        let v = TargetPresenceVerifier("Confirm deletion".into());
        let report = run_verification(Some(&before), &after, &v);
        assert_eq!(report.outcome, VerifyOutcome::Passed);
    }
}
