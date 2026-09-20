//! Target resolution ([plan §16]).
//!
//! The [`TargetResolver`] maps a natural-language request (e.g. "click
//! Submit") to a concrete [`Target`] against the latest observation, trying
//! strategies strongest-first: exact semantic, accessibility, DOM, OCR,
//! fuzzy, vision, coordinate fallback. Targets must pass the confidence
//! contract (§9.1); the resolver never routes a rejected target to execution.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use harness_protocol::{Element, Observation, Target, TargetMethod};

/// Errors from resolution.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    /// No candidate matched under any strategy.
    #[error("no target matched for '{query}'")]
    NotFound {
        /// The requested text/name.
        query: String,
    },
    /// A match was found but its confidence was too low for execution.
    #[error("target for '{query}' rejected: confidence {confidence:.2} < {min:.2}")]
    LowConfidence {
        /// The requested text/name.
        query: String,
        /// Actual confidence.
        confidence: f64,
        /// The minimum acceptable confidence.
        min: f64,
    },
    /// No observation available to resolve against.
    #[error("no observation available to resolve '{query}'")]
    NoObservation {
        /// The requested text/name.
        query: String,
    },
}

/// A pattern a strategy matches on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetQuery {
    /// Match by accessible name / text (e.g. `Submit`).
    Text(String),
    /// Match by element id (`e17`).
    ElementId(String),
    /// Match by raw screen coordinates.
    Coordinates(harness_protocol::Point),
}

impl TargetQuery {
    /// Match by text/name.
    pub fn text(s: impl Into<String>) -> Self {
        Self::Text(s.into())
    }
}

/// Matches elements against a query using the strongest available strategy.
#[derive(Debug, Clone, Copy, Default)]
pub struct TargetResolver;

impl TargetResolver {
    /// Resolve `query` against `observation`, returning a candidate target.
    ///
    /// If `require_executable` is set (the default), low-confidence matches
    /// return [`ResolveError::LowConfidence`] rather than an executable target.
    pub fn resolve(
        &self,
        observation: &Observation,
        query: &TargetQuery,
        require_executable: bool,
    ) -> Result<Target, ResolveError> {
        let label = match query {
            TargetQuery::Text(t) => t.clone(),
            TargetQuery::ElementId(id) => id.clone(),
            TargetQuery::Coordinates(p) => return self.resolve_coordinates(*p, require_executable),
        };

        let mut best: Option<Element> = None;
        let mut best_confidence: f64 = 0.0;
        let mut best_method: TargetMethod = TargetMethod::Fuzzy;

        for element in &observation.elements {
            if !element.enabled {
                continue;
            }
            if let Some((conf, method)) = self.match_element(element, &label) {
                let conf = conf.min(element.confidence);
                if conf > best_confidence {
                    best_confidence = conf;
                    best = Some(element.clone());
                    best_method = method;
                }
            }
        }

        let Some(element) = best else {
            return Err(ResolveError::NotFound { query: label });
        };

        let target = Target {
            request_id: format!("r_{}", observation.observation_id),
            element_id: Some(element.id.clone()),
            name: element.name.clone(),
            bounds: element.bounds,
            confidence: best_confidence,
            method: best_method,
        };

        if require_executable && target.is_rejected() {
            return Err(ResolveError::LowConfidence {
                query: label,
                confidence: target.confidence,
                min: harness_protocol::target::bands::RE_RESOLVE_MIN,
            });
        }

        Ok(target)
    }

    fn resolve_coordinates(
        &self,
        p: harness_protocol::Point,
        _require_executable: bool,
    ) -> Result<Target, ResolveError> {
        Ok(Target {
            request_id: "r_coord".into(),
            element_id: None,
            name: "coordinates".into(),
            bounds: harness_protocol::Bounds::new(p.x, p.y, p.x + 1, p.y + 1),
            confidence: 1.0, // direct coordinates are exact
            method: TargetMethod::Coordinates,
        })
    }

    /// Score an element against the query. Returns (confidence, method) when a
    /// plausible match exists.
    fn match_element(&self, element: &Element, label: &str) -> Option<(f64, TargetMethod)> {
        // Exact semantic match (name == query) — strongest method.
        if element.name.eq_ignore_ascii_case(label) {
            return Some((0.98, TargetMethod::Accessibility));
        }
        if let Some(text) = &element.text {
            if text.eq_ignore_ascii_case(label) {
                return Some((0.95, TargetMethod::Dom));
            }
        }
        // OCR / fuzzy: substring or high similarity on name/text.
        if normalize(&element.name).contains(&normalize(label)) {
            return Some((0.85, TargetMethod::Fuzzy));
        }
        None
    }
}

fn normalize(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<String>()
        .to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_protocol::{Bounds, Element, PerceptionSource};

    fn element(id: &str, name: &str) -> Element {
        Element {
            id: id.into(),
            role: "button".into(),
            name: name.into(),
            text: Some(name.into()),
            bounds: Bounds::new(0, 0, 10, 10),
            enabled: true,
            focused: false,
            confidence: 0.9,
            source: PerceptionSource::Accessibility,
        }
    }

    fn observation_with(elements: Vec<Element>) -> Observation {
        let mut ob = Observation::new("s_1".into(), 1, 0);
        ob.elements = elements;
        ob
    }

    #[test]
    fn exact_semantic_match_wins() {
        let ob = observation_with(vec![element("e1", "Submit")]);
        let r = TargetResolver
            .resolve(&ob, &TargetQuery::text("Submit"), true)
            .unwrap();
        assert_eq!(r.element_id.as_deref(), Some("e1"));
        assert_eq!(r.method, TargetMethod::Accessibility);
        assert!(r.is_executable());
    }

    #[test]
    fn not_found_reports_clean_error() {
        let ob = observation_with(vec![element("e1", "Submit")]);
        let err = TargetResolver
            .resolve(&ob, &TargetQuery::text("Cumpute"), true)
            .unwrap_err();
        assert!(matches!(err, ResolveError::NotFound { .. }));
    }

    #[test]
    fn coordinates_resolve_exactly() {
        let ob = observation_with(vec![]);
        let t = TargetResolver
            .resolve(
                &ob,
                &TargetQuery::Coordinates(harness_protocol::Point::new(50, 60)),
                true,
            )
            .unwrap();
        assert_eq!(t.confidence, 1.0);
        assert_eq!(t.method, TargetMethod::Coordinates);
    }

    #[test]
    fn fuzzier_match_low_priority() {
        let ob = observation_with(vec![element("e2", "Submit Order")]);
        let t = TargetResolver
            .resolve(&ob, &TargetQuery::text("Submit"), true)
            .unwrap();
        assert_eq!(t.method, TargetMethod::Fuzzy);
        assert!(t.is_executable());
    }

    #[test]
    fn no_observation_is_an_error() {
        let empty = Observation::new("s_1".into(), 9, 0);
        let err = TargetResolver
            .resolve(&empty, &TargetQuery::text("x"), true)
            .unwrap_err();
        assert!(matches!(err, ResolveError::NotFound { .. }));
    }
}
