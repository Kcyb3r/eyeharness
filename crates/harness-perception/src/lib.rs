//! Perception architecture ([plan §8]).
//!
//! All perception providers — screenshot, accessibility, DOM, OCR, vision —
//! reduce to the same normalized [`Observation`] shape, so the agent and the
//! target resolver never depend on any single technology.
//!
//! Providers declare a [`PerceptionSource`] and a priority; the harness uses
//! the strongest available source and falls back down the ladder.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use harness_protocol::{Observation, PerceptionSource, SessionId};

/// Errors from perception providers.
#[derive(Debug, thiserror::Error)]
pub enum PerceptionError {
    /// The provider could not produce an observation.
    #[error("perception failed: {0}")]
    ProviderFailed(String),
    /// A required capability is unavailable (e.g. no accessibility API).
    #[error("not available: {0}")]
    Unavailable(String),
    /// The provider timed out.
    #[error("perception timed out")]
    Timeout,
}

/// The priority ladder from §8. Lower number = higher priority to use.
///
/// Semantic APIs (0) → Accessibility (1) → DOM (2) → OCR (3) → Vision (4) →
/// Coordinate fallback (5).
pub const fn source_priority(s: PerceptionSource) -> u8 {
    match s {
        PerceptionSource::Accessibility => 0,
        PerceptionSource::Dom => 1,
        PerceptionSource::Ocr => 2,
        PerceptionSource::Vision => 3,
        PerceptionSource::Screenshot => 4,
        PerceptionSource::Coordinates => 5,
    }
}

/// A perception provider produces normalized observations for a session.
pub trait PerceptionProvider: Send + Sync {
    /// Produce a fresh observation for the session.
    ///
    /// Providers must populate `observation_id` and `timestamp` on the
    /// returned observation; the harness stamps session-level context.
    fn observe(&self, session: &SessionId) -> Result<Observation, PerceptionError>;

    /// The source this provider represents.
    fn source(&self) -> PerceptionSource;
}

impl std::fmt::Debug for dyn PerceptionProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PerceptionProvider({:?})", self.source())
    }
}

/// A registry that hands back the best provider for a session.
#[derive(Debug, Default)]
pub struct PerceptionHub {
    providers: Vec<Box<dyn PerceptionProvider>>,
}

impl PerceptionHub {
    /// Create an empty hub.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider. The provider list is kept sorted by priority
    /// (highest first).
    pub fn register(&mut self, provider: Box<dyn PerceptionProvider>) {
        self.providers.push(provider);
        self.providers.sort_by_key(|p| source_priority(p.source()));
    }

    /// The currently registered providers, best first.
    pub fn providers(&self) -> &[Box<dyn PerceptionProvider>] {
        &self.providers
    }

    /// Produce an observation using the strongest available provider.
    ///
    /// Providers that fail are skipped; the first successful one wins. If all
    /// fail, the last error is returned.
    pub fn observe(&self, session: &SessionId) -> Result<Observation, PerceptionError> {
        let mut last_err: Option<PerceptionError> = None;
        for provider in &self.providers {
            match provider.observe(session) {
                Ok(ob) => return Ok(ob),
                Err(e) => last_err = Some(e),
            }
        }
        Err(last_err.unwrap_or(PerceptionError::Unavailable(
            "no perception providers registered".into(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_protocol::{Observation, SessionId};

    /// A provider that always produces a minimum observation.
    struct StubProvider(PerceptionSource);

    impl PerceptionProvider for StubProvider {
        fn observe(&self, session: &SessionId) -> Result<Observation, PerceptionError> {
            Ok(Observation::new(session.clone(), 0, 0))
        }
        fn source(&self) -> PerceptionSource {
            self.0
        }
    }

    struct FailingProvider;

    impl PerceptionProvider for FailingProvider {
        fn observe(&self, _s: &SessionId) -> Result<Observation, PerceptionError> {
            Err(PerceptionError::ProviderFailed("boom".into()))
        }
        fn source(&self) -> PerceptionSource {
            PerceptionSource::Vision
        }
    }

    #[test]
    fn hub_prefers_highest_priority() {
        let mut hub = PerceptionHub::new();
        hub.register(Box::new(StubProvider(PerceptionSource::Vision)));
        hub.register(Box::new(StubProvider(PerceptionSource::Accessibility)));
        assert_eq!(hub.providers()[0].source(), PerceptionSource::Accessibility);
    }

    #[test]
    fn hub_skips_failing_providers() {
        let mut hub = PerceptionHub::new();
        hub.register(Box::new(FailingProvider));
        hub.register(Box::new(StubProvider(PerceptionSource::Screenshot)));
        let ob = hub.observe(&SessionId::new("s_1")).unwrap();
        assert_eq!(ob.session_id.0, "s_1");
    }

    #[test]
    fn empty_hub_reports_unavailable() {
        let hub = PerceptionHub::new();
        assert!(matches!(
            hub.observe(&SessionId::new("s_1")),
            Err(PerceptionError::Unavailable(_))
        ));
    }

    #[test]
    fn priority_ladder_is_ordered() {
        assert!(
            source_priority(PerceptionSource::Accessibility)
                < source_priority(PerceptionSource::Dom)
        );
        assert!(source_priority(PerceptionSource::Dom) < source_priority(PerceptionSource::Ocr));
        assert!(source_priority(PerceptionSource::Ocr) < source_priority(PerceptionSource::Vision));
        assert!(
            source_priority(PerceptionSource::Screenshot)
                < source_priority(PerceptionSource::Coordinates)
        );
    }
}
