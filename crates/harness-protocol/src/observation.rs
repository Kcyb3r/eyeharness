//! Unified observation model ([plan §7, §8]).
//!
//! An [`Observation`] is the normalized view of the desktop delivered to the
//! agent. All perception providers (screenshot, accessibility, DOM, OCR,
//! vision) reduce to the same shape, so the agent never depends on a specific
//! technology.

use serde::{Deserialize, Serialize};

use crate::bounds::{Bounds, Point};
use crate::types::{ElementId, TimestampMs};

/// Monotonic observation counter. Unique within a session.
pub type ObservationId = u64;

/// The perception provider that produced an element ([plan §8]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerceptionSource {
    /// Native UI Automation / accessibility.
    Accessibility,
    /// Browser DOM.
    Dom,
    /// OCR.
    Ocr,
    /// Vision ML model.
    Vision,
    /// Bare screenshot; no semantic data.
    Screenshot,
    /// Raw coordinates supplied directly by the agent.
    Coordinates,
}

impl Default for PerceptionSource {
    fn default() -> Self {
        Self::Screenshot
    }
}

/// A normalized UI element ([plan §8]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    /// Stable id resolved by the harness (unique within a session).
    pub id: ElementId,
    /// ARIA-style role, e.g. `button`, `textbox`, `listitem`.
    pub role: String,
    /// Accessible name, e.g. `Submit`.
    pub name: String,
    /// Raw visible text, if any.
    pub text: Option<String>,
    /// Bounding rectangle in physical pixels.
    pub bounds: Bounds,
    /// Whether the element is enabled / interactive.
    pub enabled: bool,
    /// Whether the element currently has input focus.
    #[serde(default)]
    pub focused: bool,
    /// Provider confidence in `0.0..=1.0`.
    #[serde(default)]
    pub confidence: f64,
    /// Which provider produced this element.
    #[serde(default)]
    pub source: PerceptionSource,
}

/// The active foreground window, if known.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Window title (e.g. `Chrome`).
    pub title: Option<String>,
    /// Application name (e.g. `Google Chrome`).
    pub application: Option<String>,
    /// Process id of the owner process.
    pub pid: Option<u32>,
    /// Bounds of the window on screen.
    pub bounds: Option<Bounds>,
}

/// The current cursor position at observation time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorState {
    /// Horizontal position, physical pixels.
    pub x: i64,
    /// Vertical position, physical pixels.
    pub y: i64,
}

impl Point {
    /// Convert a point to a cursor state.
    pub fn as_cursor(&self) -> CursorState {
        CursorState {
            x: self.x,
            y: self.y,
        }
    }
}

/// A single unified observation of the desktop ([plan §7]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    /// Unique within a session.
    pub observation_id: ObservationId,
    /// Time the observation was produced (ms).
    pub timestamp: TimestampMs,
    /// The session the observation belongs to.
    pub session_id: crate::SessionId,
    /// Full screen dimensions, if known.
    pub screen: Option<Bounds>,
    /// Cursor position at capture time.
    #[serde(default)]
    pub cursor: Option<CursorState>,
    /// Active foreground window.
    #[serde(default)]
    pub active_window: Option<WindowInfo>,
    /// Normalized semantic elements.
    #[serde(default)]
    pub elements: Vec<Element>,
    /// Plain text extracted from the screen (OCR / DOM text), if any.
    #[serde(default)]
    pub text: Vec<String>,
    /// Whether the screen changed materially since the previous observation.
    #[serde(default)]
    pub screen_changed: bool,
}

impl Observation {
    /// Create a new observation with the given id and timestamp.
    pub fn new(
        session_id: crate::SessionId,
        observation_id: ObservationId,
        timestamp: TimestampMs,
    ) -> Self {
        Self {
            observation_id,
            timestamp,
            session_id,
            screen: None,
            cursor: None,
            active_window: None,
            elements: Vec::new(),
            text: Vec::new(),
            screen_changed: false,
        }
    }

    /// The age of this observation relative to `now`, in ms.
    ///
    /// `now` must be >= `self.timestamp`; otherwise returns `None`.
    pub fn age_ms(&self, now: TimestampMs) -> Option<u64> {
        now.checked_sub(self.timestamp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_roundtrips_through_json() {
        let e = Element {
            id: "e183".into(),
            role: "button".into(),
            name: "Submit".into(),
            text: Some("Submit".into()),
            bounds: Bounds::new(800, 600, 920, 650),
            enabled: true,
            focused: false,
            confidence: 0.98,
            source: PerceptionSource::Accessibility,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["role"], "button");
        assert_eq!(v["bounds"][0], 800);
        assert_eq!(v["confidence"], 0.98);
        assert_eq!(v["source"], "accessibility");
        let back: Element = serde_json::from_value(v).unwrap();
        assert_eq!(back.id, "e183");
        assert_eq!(back.name, "Submit");
    }

    #[test]
    fn observation_age_is_computed() {
        let o = Observation::new("s_1".into(), 1, 1000);
        assert_eq!(o.age_ms(1500), Some(500));
        assert_eq!(o.age_ms(1000), Some(0));
        assert_eq!(o.age_ms(900), None);
    }

    #[test]
    fn observation_serializes_unified_shape() {
        let mut o = Observation::new("s_1".into(), 4, 1000);
        o.screen = Some(Bounds::new(0, 0, 2560, 1440));
        o.cursor = Some(CursorState { x: 812, y: 421 });
        o.active_window = Some(WindowInfo {
            title: Some("Chrome".into()),
            application: Some("Google Chrome".into()),
            ..Default::default()
        });
        let v = serde_json::to_value(&o).unwrap();
        assert_eq!(v["screen"], serde_json::json!([0, 0, 2560, 1440]));
        assert_eq!(v["cursor"]["x"], 812);
        assert_eq!(v["active_window"]["title"], "Chrome");
    }

    #[test]
    fn unknown_source_fields_default_gracefully() {
        let raw = r#"{"id":"e1","role":"button","name":"OK","bounds":[1,2,3,4],
            "enabled":true}"#;
        let e: Element = serde_json::from_str(raw).unwrap();
        assert_eq!(e.focused, false);
        assert_eq!(e.source, PerceptionSource::default());
    }
}
