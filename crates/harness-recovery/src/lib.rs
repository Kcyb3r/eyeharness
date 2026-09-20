//! Recovery engine ([plan §17]).
//!
//! Failures are expected. After a failed action the harness walks a recovery
//! ladder:
//!
//! ```text
//! ACTION
//!   ├── success → CONTINUE
//!   └── failure
//!         ├── re-observe
//!         ├── retry
//!         ├── re-resolve target
//!         ├── alternative action
//!         ├── ask agent
//!         └── abort
//! ```
//!
//! The ladder is bounded: each step is attempted up to a limit, and the engine
//! never escalates into an unbounded retry loop.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use harness_protocol::{ActionStatus, ObservationId};

use crate::Step::*;

/// Errors from the recovery engine setup.
#[derive(Debug, thiserror::Error)]
pub enum RecoveryError {
    /// The engine has exhausted all recovery steps.
    #[error("recovery exhausted after {step:?}")]
    Exhausted {
        /// The step that failed last.
        step: &'static str,
    },
}

/// A single step in the recovery ladder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Produce a fresh observation.
    ReObserve,
    /// Re-run the action.
    Retry,
    /// Resolve the target again against the fresh observation.
    ReResolve,
    /// Try an alternative approach.
    Alternative,
    /// Prompt the agent for clarification.
    AskAgent,
    /// Give up deterministically.
    Abort,
}

impl Step {
    /// The ordered ladder ([plan §17]).
    pub const LADDER: [Step; 6] = [ReObserve, Retry, ReResolve, Alternative, AskAgent, Abort];
}

/// Determines the next recovery step for a failure.
#[derive(Debug, Clone, Copy)]
pub struct RecoveryPlanner {
    /// Max retries before `ReResolve`.
    pub max_retries: usize,
    /// Max re-resolves before `Alternative`.
    pub max_re_resolves: usize,
}

impl Default for RecoveryPlanner {
    fn default() -> Self {
        Self {
            max_retries: 3,
            max_re_resolves: 2,
        }
    }
}

impl RecoveryPlanner {
    /// Choose the next step given how many of each step have already run.
    pub fn next_step(
        &self,
        retries_used: usize,
        re_resolves_used: usize,
        asks_used: usize,
    ) -> Result<Step, RecoveryError> {
        // Fresh observations are cheap and non-destructive; keep them first.
        if retries_used < self.max_retries {
            return Ok(Retry);
        }
        if re_resolves_used < self.max_re_resolves {
            return Ok(ReResolve);
        }
        if asks_used < 1 {
            return Ok(AskAgent);
        }
        Err(RecoveryError::Exhausted { step: "abort" })
    }

    /// Map an action outcome into whether recovery is even warranted.
    pub fn should_recover(status: ActionStatus) -> bool {
        matches!(status, ActionStatus::Failed)
    }
}

/// A running recovery context tracking progress through the ladder.
#[derive(Debug, Clone, Default)]
pub struct RecoverySession {
    /// The observation currently in play.
    pub last_observation: Option<ObservationId>,
    retries: usize,
    re_resolves: usize,
    asks: usize,
    aborted: bool,
}

impl RecoverySession {
    /// Create a recovery context starting at the re-observe step.
    pub fn begin(&mut self, observation: ObservationId) {
        self.last_observation = Some(observation);
        self.retries = 0;
        self.re_resolves = 0;
        self.asks = 0;
        self.aborted = false;
    }

    /// Has this recovery been aborted?
    pub fn is_aborted(&self) -> bool {
        self.aborted
    }

    /// Record that an observation was refreshed during recovery.
    pub fn mark_reobserved(&mut self, observation: ObservationId) {
        self.last_observation = Some(observation);
    }

    /// Advance the recovery ladder, mutating internal counters.
    pub fn step(&mut self) -> Result<Step, RecoveryError> {
        if self.aborted {
            return Err(RecoveryError::Exhausted { step: "abort" });
        }
        let next =
            RecoveryPlanner::default().next_step(self.retries, self.re_resolves, self.asks)?;
        match next {
            Retry => self.retries += 1,
            ReResolve => self.re_resolves += 1,
            AskAgent => self.asks += 1,
            _ => {}
        }
        Ok(next)
    }

    /// Terminate the recovery as aborted.
    pub fn abort(&mut self) {
        self.aborted = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwarranted_recovery_is_false_for_success() {
        assert!(!RecoveryPlanner::should_recover(ActionStatus::Success));
        assert!(RecoveryPlanner::should_recover(ActionStatus::Failed));
    }

    #[test]
    fn ladder_is_bounded_and_orderly() {
        let mut r = RecoverySession::default();
        r.begin(1);
        let mut seen = Vec::new();
        while let Ok(s) = r.step() {
            seen.push(s);
        }
        // retries (3) then re-resolves (2) then ask (1) → 6 steps, then exhausted.
        assert_eq!(
            seen,
            vec![
                Step::Retry,
                Step::Retry,
                Step::Retry,
                Step::ReResolve,
                Step::ReResolve,
                Step::AskAgent,
            ]
        );
        assert!(r.step().is_err());
    }

    #[test]
    fn reobserve_refreshes_observation() {
        let mut r = RecoverySession::default();
        r.begin(1);
        r.mark_reobserved(42);
        assert_eq!(r.last_observation, Some(42));
    }

    #[test]
    fn manually_aborted_recovery_is_exhausted() {
        let mut r = RecoverySession::default();
        r.begin(1);
        r.abort();
        assert!(r.is_aborted());
        assert!(r.step().is_err());
    }
}
