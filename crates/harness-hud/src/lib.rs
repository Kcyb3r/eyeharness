//! Heads-up display overlay ([plan §23]).
//!
//! Renders the harness's live state as a minimal overlay: current session,
//! latest observation age, gate outcome, and a small action log. The overlay
//! itself is never part of the perception stream — the HUD is drawn into a
//! region the perception layer explicitly excludes ([plan §23.1]).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use harness_events::{EventBus, EventSender};
use harness_protocol::{Event, EventKind};
use std::collections::VecDeque;

/// One HUD row drawable over the screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HudUpdate {
    /// Which fields changed.
    pub changed: Vec<HudField>,
}

/// Mutable HUD state suitable for a 60 FPS renderer.
#[derive(Debug, Clone, PartialEq)]
pub struct HudState {
    /// Current status text.
    pub status: String,
    /// Recent event labels, newest first.
    pub log: VecDeque<String>,
    /// Animation state.
    pub animation: AnimationState,
}

/// Small deterministic animation state for a floating status window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnimationState {
    /// Pulsing status-dot phase in radians.
    pub pulse_phase: f32,
    /// Current opacity.
    pub fade_alpha: f32,
    /// Animated progress value in `0.0..=1.0`.
    pub progress: f32,
    target_progress: f32,
    visible: bool,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            pulse_phase: 0.0,
            fade_alpha: 0.0,
            progress: 0.0,
            target_progress: 0.0,
            visible: false,
        }
    }
}

impl AnimationState {
    /// Advance animation by a bounded time delta.
    pub fn tick(&mut self, dt_seconds: f32) {
        let dt = dt_seconds.clamp(0.0, 0.25);
        self.pulse_phase = (self.pulse_phase + dt * 3.0) % std::f32::consts::TAU;
        self.progress += (self.target_progress - self.progress) * (dt * 5.0).min(1.0);
        let direction = if self.visible { 1.0 } else { -1.0 };
        self.fade_alpha = (self.fade_alpha + direction * dt * 4.0).clamp(0.0, 1.0);
    }

    /// Set a new progress target and reveal the HUD.
    pub fn set_progress(&mut self, current: u32, total: u32) {
        self.target_progress = if total == 0 {
            0.0
        } else {
            (current as f32 / total as f32).clamp(0.0, 1.0)
        };
        self.visible = true;
    }

    /// Reveal the HUD.
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// Begin fading the HUD out.
    pub fn hide(&mut self) {
        self.visible = false;
    }

    /// Whether the HUD is still visible during fade-out.
    pub fn is_visible(&self) -> bool {
        self.fade_alpha > 0.0
    }
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            status: "idle".into(),
            log: VecDeque::with_capacity(100),
            animation: AnimationState::default(),
        }
    }
}

impl HudState {
    /// Apply one event and retain at most 100 log entries.
    pub fn ingest(&mut self, event: &Event) {
        self.status = event.kind.to_string();
        self.log
            .push_front(format!("{} {}", glyph(&event.kind), self.status));
        self.log.truncate(100);
        self.animation.show();
    }
}

/// A single drawable HUD field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HudField {
    /// Clock / uptime.
    Clock(String),
    /// Session id.
    Session(String),
    /// Latest observation age (ms).
    ObservationAge(u64),
    /// Last gate outcome.
    Gate(String),
    /// Last emitted event kind.
    LastEvent(String),
}

/// The HUD renderer. Consumes events from the bus and produces combined
/// overlay updates.
#[derive(Debug, Clone)]
pub struct HudRenderer {
    _bus: EventSender,
    last_event: Option<EventKind>,
}

impl HudRenderer {
    /// Attach a HUD renderer to the event bus.
    pub fn new(bus: EventBus) -> Self {
        Self {
            _bus: bus.sender(),
            last_event: None,
        }
    }

    /// Consume the latest event and produce an overlay update (if any).
    pub fn ingest(&mut self, event: Event) -> HudUpdate {
        let changed = vec![HudField::LastEvent(event.kind.to_string())];
        self.last_event = Some(event.kind);
        HudUpdate { changed }
    }

    /// Produce a small snapshot update on demand (clock + age).
    pub fn snapshot(&self, age_ms: u64) -> Vec<HudField> {
        vec![HudField::Clock(now_str()), HudField::ObservationAge(age_ms)]
    }

    /// The most recent event kind seen.
    pub fn last_event(&self) -> Option<&EventKind> {
        self.last_event.as_ref()
    }
}

/// Human-readable wall-clock string for the HUD.
pub fn now_str() -> String {
    // Deterministic for tests: HH:MM:SS of the system clock.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() % 86_400)
        .unwrap_or(0);
    let (h, m, s) = (now / 3600, (now % 3600) / 60, now % 60);
    format!("{h:02}:{m:02}:{s:02}")
}

/// Convert an event kind to a compact HUD glyph.
pub fn glyph(kind: &EventKind) -> &'static str {
    match kind {
        EventKind::SessionStarted => "▶",
        EventKind::SessionEnded => "■",
        EventKind::PolicyAllowed => "✓",
        EventKind::PolicyBlocked => "⛔",
        EventKind::ActionExecuted => "⚡",
        EventKind::ActionFailed => "✗",
        EventKind::ObservationCreated => "◉",
        EventKind::RecoveryStarted => "♻",
        _ => "·",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ingest_records_last_event() {
        let mut hud = HudRenderer::new(EventBus::new());
        let ev = Event::new(EventKind::ActionExecuted, 0);
        let _ = hud.ingest(ev);
        assert_eq!(hud.last_event(), Some(&EventKind::ActionExecuted));
    }

    #[test]
    fn snapshot_is_deterministic_shape() {
        let hud = HudRenderer::new(EventBus::new());
        let fields = hud.snapshot(42);
        assert_eq!(fields.len(), 2);
        assert!(matches!(fields[0], HudField::Clock(_)));
        assert_eq!(fields[1], HudField::ObservationAge(42));
    }

    #[test]
    fn glyphs_exist_for_hot_kinds() {
        assert_eq!(glyph(&EventKind::PolicyBlocked), "⛔");
        assert_eq!(glyph(&EventKind::ActionExecuted), "⚡");
    }

    #[test]
    fn animation_reveals_and_smooths_progress() {
        let mut animation = AnimationState::default();
        animation.set_progress(1, 2);
        animation.tick(0.1);
        assert!(animation.is_visible());
        assert!(animation.progress > 0.0 && animation.progress < 0.5);
        animation.hide();
        animation.tick(0.25);
        assert!(animation.fade_alpha < 1.0);
    }

    #[test]
    fn hud_state_bounds_event_log() {
        let mut state = HudState::default();
        for _ in 0..101 {
            state.ingest(&Event::new(EventKind::ActionExecuted, 0));
        }
        assert_eq!(state.log.len(), 100);
        assert_eq!(state.status, "action.executed");
    }
}
