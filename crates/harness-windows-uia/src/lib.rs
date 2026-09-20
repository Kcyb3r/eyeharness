//! Windows UI Automation provider ([plan §5.3]).
//!
//! Extracts control type, name, text, bounds, enabled/focused state, and
//! patterns from the UI Automation tree and converts them into the normalized
//! element model. The Win32 implementation is planned; the type surface lives
//! here so the perception hub can reference it on any platform.

#![warn(missing_docs)]

use harness_perception::{PerceptionError, PerceptionProvider};
use harness_protocol::{
    Bounds, CursorState, Element, Observation, PerceptionSource, SessionId, WindowInfo,
};
use thiserror::Error;

/// Errors from the UI Automation provider.
#[derive(Debug, Error)]
pub enum UiaError {
    /// UI Automation is only available on Windows.
    #[error("UI Automation is Windows-only")]
    UnsupportedPlatform,
    /// The root element could not be obtained.
    #[error("no root element")]
    NoRoot,
}

/// Concrete provider for the accessibility tree.
pub struct UiaProvider;

impl PerceptionProvider for UiaProvider {
    fn observe(&self, session: &SessionId) -> Result<Observation, PerceptionError> {
        #[cfg(target_os = "linux")]
        {
            return observe_linux(session);
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = session;
            Err(PerceptionError::Unavailable(
                "desktop accessibility provider is not implemented on this platform".into(),
            ))
        }
    }

    fn source(&self) -> PerceptionSource {
        PerceptionSource::Accessibility
    }
}

impl UiaProvider {
    /// Build the runtime provider. Safe on all platforms; only functional on
    /// Windows.
    pub fn new() -> Result<Self, UiaError> {
        // Omitting the Windows check here keeps the crate cross-compilable;
        // the provider returns Unavailable until the Win32 backend exists.
        Ok(Self)
    }
}

#[cfg(target_os = "linux")]
fn run_xdotool(args: &[&str]) -> Result<String, PerceptionError> {
    let output = std::process::Command::new("xdotool")
        .args(args)
        .output()
        .map_err(|e| {
            PerceptionError::Unavailable(format!(
                "xdotool is required for Linux desktop perception: {e}"
            ))
        })?;
    if !output.status.success() {
        return Err(PerceptionError::ProviderFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "linux")]
fn shell_value(output: &str, key: &str) -> Option<i64> {
    output
        .lines()
        .find_map(|line| line.strip_prefix(key).and_then(|v| v.parse().ok()))
}

#[cfg(target_os = "linux")]
fn observe_linux(session: &SessionId) -> Result<Observation, PerceptionError> {
    let geometry = run_xdotool(&["getdisplaygeometry"])?;
    let mut parts = geometry.split_whitespace();
    let width: i64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    let height: i64 = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
    if width <= 0 || height <= 0 {
        return Err(PerceptionError::ProviderFailed(
            "invalid display geometry".into(),
        ));
    }
    let mut observation = Observation::new(session.clone(), 0, 0);
    observation.screen = Some(Bounds::new(0, 0, width, height));
    if let Ok(cursor) = run_xdotool(&["getmouselocation", "--shell"]) {
        observation.cursor = Some(CursorState {
            x: shell_value(&cursor, "X=").unwrap_or(0),
            y: shell_value(&cursor, "Y=").unwrap_or(0),
        });
    }
    let active_id = run_xdotool(&["getactivewindow"])
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok());
    if let Some(id) = active_id {
        observation.active_window = window_info(id);
    }
    let ids = run_xdotool(&["search", "--onlyvisible", "--name", "."])?;
    for id in ids
        .lines()
        .filter_map(|line| line.trim().parse::<u64>().ok())
    {
        let Some(bounds) = window_bounds(id) else {
            continue;
        };
        let name = run_xdotool(&["getwindowname", &id.to_string()])
            .unwrap_or_default()
            .trim()
            .to_string();
        if name.is_empty() {
            continue;
        }
        observation.elements.push(Element {
            id: format!("window-{id}"),
            role: "window".into(),
            name: name.clone(),
            text: Some(name),
            bounds,
            enabled: true,
            focused: Some(id) == active_id,
            confidence: 0.92,
            source: PerceptionSource::Accessibility,
        });
    }
    Ok(observation)
}

#[cfg(target_os = "linux")]
fn window_bounds(id: u64) -> Option<Bounds> {
    let id = id.to_string();
    let output = run_xdotool(&["getwindowgeometry", "--shell", &id]).ok()?;
    let x = shell_value(&output, "X=")?;
    let y = shell_value(&output, "Y=")?;
    let w = shell_value(&output, "WIDTH=")?;
    let h = shell_value(&output, "HEIGHT=")?;
    Some(Bounds::new(x, y, x + w, y + h))
}

#[cfg(target_os = "linux")]
fn window_info(id: u64) -> Option<WindowInfo> {
    let id_text = id.to_string();
    Some(WindowInfo {
        title: run_xdotool(&["getwindowname", &id_text])
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()),
        application: None,
        pid: run_xdotool(&["getwindowpid", &id_text])
            .ok()
            .and_then(|v| v.trim().parse().ok()),
        bounds: window_bounds(id),
    })
}

impl Default for UiaProvider {
    fn default() -> Self {
        Self
    }
}

impl std::fmt::Debug for UiaProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("UiaProvider")
    }
}
