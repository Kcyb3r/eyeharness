//! Observation freshness rule ([plan §11.1]).
//!
//! Every action carries the `observation_id` it was derived from. The harness
//! rejects actions whose source observation is older than a maximum age. This
//! module centralizes the default window and the check itself so the same rule
//! applies whether the check is done by the gateway or the action engine.

use crate::{Observation, TimestampMs};

/// Default maximum observation age. Configurable per session.
pub const DEFAULT_MAX_AGE_MS: u64 = 500;

/// Result of a freshness check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Observation is within the window; the action may proceed.
    Fresh,
    /// Observation is stale; the action must be rejected and a fresh
    /// observation queued.
    Stale(u64),
}

/// Check whether an observation is still fresh as of `now`.
///
/// Returns [`Freshness::Stale(age)`] when `age > max_age_ms`. Observations
/// produced after `now` (clock skew) are treated as fresh.
pub fn check(observation: &Observation, now: TimestampMs, max_age_ms: u64) -> Freshness {
    match observation.age_ms(now) {
        None => Freshness::Fresh,
        Some(age) if age <= max_age_ms => Freshness::Fresh,
        Some(age) => Freshness::Stale(age),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Observation;

    fn obs_at(ts: TimestampMs) -> Observation {
        Observation::new("s_1".into(), 1, ts)
    }

    #[test]
    fn within_window_is_fresh() {
        let o = obs_at(1_000);
        assert_eq!(check(&o, 1_400, 500), Freshness::Fresh);
        assert_eq!(check(&o, 1_500, 500), Freshness::Fresh); // exactly at limit
    }

    #[test]
    fn over_window_is_stale_with_age() {
        let o = obs_at(1_000);
        assert_eq!(check(&o, 1_501, 500), Freshness::Stale(501));
        assert_eq!(check(&o, 1_812, 500), Freshness::Stale(812));
    }

    #[test]
    fn future_observation_is_fresh() {
        let o = obs_at(2_000);
        assert_eq!(check(&o, 1_000, 500), Freshness::Fresh);
    }

    #[test]
    fn default_window_used_when_not_overridden() {
        let o = obs_at(0);
        assert_eq!(
            check(&o, DEFAULT_MAX_AGE_MS, DEFAULT_MAX_AGE_MS),
            Freshness::Fresh
        );
        assert_eq!(
            check(&o, DEFAULT_MAX_AGE_MS + 1, DEFAULT_MAX_AGE_MS),
            Freshness::Stale(501)
        );
    }
}
