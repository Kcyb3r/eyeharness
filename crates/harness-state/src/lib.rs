//! State engine ([plan §20]).
//!
//! Maintains local, task-relevant desktop state so the harness can reduce the
//! number of model calls: current window, application, URL, focused element,
//! recently observed elements, clipboard, and dialog awareness survive between
//! observations.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod learning;

use harness_protocol::{Element, Observation, WindowInfo};

/// The set of locally tracked state the harness caches between observations.
#[derive(Debug, Clone, Default)]
pub struct HarnessState {
    /// The active foreground window.
    pub active_window: Option<WindowInfo>,
    /// The most recently focused element id, if any.
    pub focused_element: Option<String>,
    /// Elements from the last observation, indexed by element id.
    elements: std::collections::HashMap<String, Element>,
    /// Clipboard snapshot (contents intentionally NOT stored by default —
    /// see secret handling policy).
    clipboard_has_content: bool,
    /// Whether a modal dialog was detected.
    pub dialog_open: bool,
    /// The most recently observed application name.
    pub current_application: Option<String>,
    /// Monotonic id of the last observation integrated.
    pub last_observation_id: Option<harness_protocol::ObservationId>,
    /// Number of elements in the last integrated observation.
    element_count: usize,
}

impl HarnessState {
    /// Create an empty state engine.
    pub fn new() -> Self {
        Self::default()
    }

    /// Integrate a new observation, updating cached state.
    pub fn integrate(&mut self, ob: &Observation) {
        self.last_observation_id = Some(ob.observation_id);
        if let Some(w) = &ob.active_window {
            self.active_window = Some(w.clone());
            self.current_application = w.application.clone();
        }
        self.focused_element = ob.elements.iter().find(|e| e.focused).map(|e| e.id.clone());
        self.dialog_open = ob.text.iter().any(|t| is_dialogish(t));
        self.elements.clear();
        for e in &ob.elements {
            self.elements.insert(e.id.clone(), e.clone());
        }
        self.element_count = ob.elements.len();
    }

    /// Whether the state has any observation integrated yet.
    pub fn has_observation(&self) -> bool {
        self.last_observation_id.is_some()
    }

    /// Look up an element by id from the last observation.
    pub fn element(&self, id: &str) -> Option<&Element> {
        self.elements.get(id)
    }

    /// Number of elements in the last observation.
    pub fn element_count(&self) -> usize {
        self.element_count
    }

    /// Whether the clipboard held content at last check.
    pub fn clipboard_has_content(&self) -> bool {
        self.clipboard_has_content
    }

    /// Update the clipboard presence bit. Contents are intentionally not kept.
    pub fn set_clipboard_has_content(&mut self, has: bool) {
        self.clipboard_has_content = has;
    }
}

fn is_dialogish(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    t.contains("ok") || t.contains("cancel") || t.contains("confirm") || t.contains("close")
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_protocol::{CursorState, Element, Observation, PerceptionSource};

    fn element(id: &str, focused: bool) -> Element {
        Element {
            id: id.into(),
            role: "button".into(),
            name: id.into(),
            text: None,
            bounds: harness_protocol::Bounds::new(0, 0, 10, 10),
            enabled: true,
            focused,
            confidence: 0.9,
            source: PerceptionSource::Accessibility,
        }
    }

    #[test]
    fn integrates_observation() {
        let mut st = HarnessState::new();
        assert!(!st.has_observation());
        let mut ob = Observation::new("s_1".into(), 1, 0);
        ob.active_window = Some(WindowInfo {
            title: Some("Chrome".into()),
            application: Some("Google Chrome".into()),
            ..Default::default()
        });
        ob.elements = vec![element("e1", true)];
        ob.cursor = Some(CursorState { x: 1, y: 2 });
        st.integrate(&ob);
        assert!(st.has_observation());
        assert_eq!(st.last_observation_id, Some(1));
        assert_eq!(st.focused_element.as_deref(), Some("e1"));
        assert_eq!(st.current_application.as_deref(), Some("Google Chrome"));
        assert_eq!(st.element_count(), 1);
        assert!(st.element("e1").is_some());
        assert!(st.element("zz").is_none());
    }

    #[test]
    fn dialog_detection_via_text() {
        let mut st = HarnessState::new();
        let mut ob = Observation::new("s_1".into(), 2, 0);
        ob.text = vec!["Are you sure you want to close?".into()];
        st.integrate(&ob);
        assert!(st.dialog_open);
    }

    #[test]
    fn clipboard_tracks_presence_not_content() {
        let mut st = HarnessState::new();
        st.set_clipboard_has_content(true);
        assert!(st.clipboard_has_content());
    }
}
