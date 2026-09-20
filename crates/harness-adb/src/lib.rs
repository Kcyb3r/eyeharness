//! Android control through the platform `adb` client.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::process::{Command, Output};
use std::time::Duration;

use harness_core::Executor;
use harness_perception::{PerceptionError, PerceptionProvider};
use harness_protocol::{
    Action, Bounds, Element, Observation, PerceptionSource, PrimitiveAction, SessionId, WindowInfo,
};

/// A discovered Android device.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AdbDevice {
    /// ADB serial.
    pub serial: String,
    /// ADB state (`device`, `offline`, or `unauthorized`).
    pub state: String,
    /// Optional model reported by Android.
    pub model: Option<String>,
}

/// A configured ADB connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbClient {
    serial: String,
}

/// Errors returned by ADB operations.
#[derive(Debug, thiserror::Error)]
pub enum AdbError {
    /// The adb executable is unavailable.
    #[error("adb executable unavailable: {0}")]
    Unavailable(String),
    /// ADB returned a non-zero status.
    #[error("adb command failed: {0}")]
    Command(String),
    /// A response could not be parsed.
    #[error("adb response malformed: {0}")]
    Malformed(String),
    /// A bounded screenshot could not be stored.
    #[error("screenshot rejected: {0}")]
    Screenshot(String),
}

impl AdbClient {
    /// Connect to a device by serial.
    pub fn new(serial: impl Into<String>) -> Self {
        Self {
            serial: serial.into(),
        }
    }

    /// The selected serial.
    pub fn serial(&self) -> &str {
        &self.serial
    }

    /// Run an ADB command on this device.
    pub fn command(&self, args: &[&str]) -> Result<Vec<u8>, AdbError> {
        let mut full = vec!["-s", self.serial.as_str()];
        full.extend_from_slice(args);
        let output = Command::new("adb")
            .args(full)
            .output()
            .map_err(|e| AdbError::Unavailable(e.to_string()))?;
        checked_output(output)
    }

    fn command_owned(&self, args: &[String]) -> Result<Vec<u8>, AdbError> {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.command(&refs)
    }

    /// Return the device display size.
    pub fn screen_size(&self) -> Result<(i64, i64), AdbError> {
        let text = String::from_utf8_lossy(&self.command(&["shell", "wm", "size"])?).into_owned();
        let value = text
            .lines()
            .find_map(|line| line.rsplit_once(": ").map(|(_, size)| size.trim()))
            .ok_or_else(|| AdbError::Malformed("wm size missing".into()))?;
        let (w, h) = value
            .split_once('x')
            .ok_or_else(|| AdbError::Malformed("invalid screen size".into()))?;
        Ok((
            w.parse()
                .map_err(|_| AdbError::Malformed("invalid width".into()))?,
            h.parse()
                .map_err(|_| AdbError::Malformed("invalid height".into()))?,
        ))
    }

    /// Dump the UI hierarchy directly to stdout.
    pub fn ui_xml(&self) -> Result<String, AdbError> {
        let bytes = self.command(&["exec-out", "uiautomator", "dump", "/dev/tty"])?;
        Ok(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Return the foreground window diagnostic.
    pub fn current_window(&self) -> Result<String, AdbError> {
        Ok(
            String::from_utf8_lossy(&self.command(&["shell", "dumpsys", "window", "windows"])?)
                .lines()
                .find(|line| line.contains("mCurrentFocus"))
                .unwrap_or_default()
                .trim()
                .to_owned(),
        )
    }

    /// Capture a bounded PNG into the temporary screenshot ring.
    pub fn screenshot(&self) -> Result<std::path::PathBuf, AdbError> {
        let dir = std::env::temp_dir().join("eyeharness-screenshots");
        std::fs::create_dir_all(&dir).map_err(|e| AdbError::Screenshot(e.to_string()))?;
        cleanup_screenshots(&dir)?;
        let bytes = self.command(&["exec-out", "screencap", "-p"])?;
        if bytes.len() > 10 * 1024 * 1024 {
            return Err(AdbError::Screenshot("screenshot exceeds 10MB".into()));
        }
        let path = dir.join(format!("screen-{}.png", crate::now_ms()));
        std::fs::write(&path, bytes).map_err(|e| AdbError::Screenshot(e.to_string()))?;
        Ok(path)
    }
}

fn checked_output(output: Output) -> Result<Vec<u8>, AdbError> {
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(AdbError::Command(
            String::from_utf8_lossy(&output.stderr).trim().into(),
        ))
    }
}

fn cleanup_screenshots(dir: &std::path::Path) -> Result<(), AdbError> {
    const MAX_FILES: usize = 5;
    const MAX_BYTES: u64 = 20 * 1024 * 1024;
    const TTL: Duration = Duration::from_secs(30);
    let now = std::time::SystemTime::now();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| AdbError::Screenshot(e.to_string()))?
        .filter_map(Result::ok)
        .collect();
    entries.retain(|entry| {
        let expired = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > TTL);
        if expired {
            let _ = std::fs::remove_file(entry.path());
        }
        !expired
    });
    entries.sort_by_key(|entry| entry.metadata().and_then(|m| m.modified()).ok());
    let mut total_bytes: u64 = entries
        .iter()
        .filter_map(|entry| entry.metadata().ok().map(|m| m.len()))
        .sum();
    while entries.len() >= MAX_FILES || total_bytes > MAX_BYTES {
        if let Some(entry) = entries.first() {
            total_bytes =
                total_bytes.saturating_sub(entry.metadata().map(|m| m.len()).unwrap_or_default());
            let _ = std::fs::remove_file(entry.path());
        }
        entries.remove(0);
    }
    Ok(())
}

/// List connected devices.
pub fn list_devices() -> Result<Vec<AdbDevice>, AdbError> {
    let output = Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .map_err(|e| AdbError::Unavailable(e.to_string()))?;
    let bytes = checked_output(output)?;
    let mut devices = Vec::new();
    for line in String::from_utf8_lossy(&bytes).lines().skip(1) {
        let mut fields = line.split_whitespace();
        let Some(serial) = fields.next() else {
            continue;
        };
        let Some(state) = fields.next() else { continue };
        let model = fields
            .find_map(|field| field.strip_prefix("model:"))
            .map(str::to_owned);
        devices.push(AdbDevice {
            serial: serial.into(),
            state: state.into(),
            model,
        });
    }
    Ok(devices)
}

/// Live Android perception provider.
#[derive(Debug, Clone)]
pub struct AdbProvider {
    client: AdbClient,
}

impl AdbProvider {
    /// Construct a provider for a device serial.
    pub fn new(client: AdbClient) -> Self {
        Self { client }
    }
}

impl PerceptionProvider for AdbProvider {
    fn observe(&self, session: &SessionId) -> Result<Observation, PerceptionError> {
        let (width, height) = self.client.screen_size().map_err(to_perception)?;
        let mut observation = Observation::new(session.clone(), 0, now_ms());
        observation.screen = Some(Bounds::new(0, 0, width, height));
        observation.active_window = Some(WindowInfo {
            title: Some(self.client.current_window().map_err(to_perception)?),
            application: None,
            pid: None,
            bounds: observation.screen,
        });
        observation.elements =
            parse_ui_xml(&self.client.ui_xml().map_err(to_perception)?).map_err(to_perception)?;
        Ok(observation)
    }

    fn source(&self) -> PerceptionSource {
        PerceptionSource::Accessibility
    }
}

/// Native Android executor.
#[derive(Debug, Clone)]
pub struct AdbExecutor {
    client: AdbClient,
}

impl AdbExecutor {
    /// Construct an executor for a device serial.
    pub fn new(client: AdbClient) -> Self {
        Self { client }
    }
}

impl Executor for AdbExecutor {
    fn execute(&self, action: &PrimitiveAction) -> Result<(), String> {
        let args: Vec<String> = match action {
            PrimitiveAction::Click { x, y } => vec![
                "shell".into(),
                "input".into(),
                "tap".into(),
                x.to_string(),
                y.to_string(),
            ],
            PrimitiveAction::DoubleClick { x, y } => {
                let tap = vec![
                    "shell".into(),
                    "input".into(),
                    "tap".into(),
                    x.to_string(),
                    y.to_string(),
                ];
                self.client.command_owned(&tap).map_err(|e| e.to_string())?;
                tap
            }
            PrimitiveAction::Type { text } => {
                vec![
                    "shell".into(),
                    "input".into(),
                    "text".into(),
                    text.replace(' ', "%s"),
                ]
            }
            PrimitiveAction::Keypress { key } => {
                vec![
                    "shell".into(),
                    "input".into(),
                    "keyevent".into(),
                    key.clone(),
                ]
            }
            PrimitiveAction::Scroll { x, y, delta } => {
                let dy = if *delta >= 0 { y + 300 } else { y - 300 };
                vec![
                    "shell".into(),
                    "input".into(),
                    "swipe".into(),
                    x.to_string(),
                    y.to_string(),
                    x.to_string(),
                    dy.to_string(),
                    "50".into(),
                ]
            }
            PrimitiveAction::Drag { x0, y0, x1, y1 } => vec![
                "shell".into(),
                "input".into(),
                "swipe".into(),
                x0.to_string(),
                y0.to_string(),
                x1.to_string(),
                y1.to_string(),
                "300".into(),
            ],
            PrimitiveAction::Wait { ms } => {
                std::thread::sleep(Duration::from_millis(*ms));
                return Ok(());
            }
            _ => return Err("action is not supported by ADB".into()),
        };
        self.client
            .command_owned(&args)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}

fn parse_ui_xml(xml: &str) -> Result<Vec<Element>, AdbError> {
    let mut elements = Vec::new();
    for raw in xml.split("<node ").skip(1) {
        let Some(end) = raw.find("/>") else { continue };
        let attrs = &raw[..end];
        let get = |key: &str| -> Option<String> {
            let marker = format!("{key}=\"");
            let start = attrs.find(&marker)? + marker.len();
            let rest = &attrs[start..];
            Some(
                rest.get(..rest.find('"')?)?
                    .replace("&quot;", "\"")
                    .replace("&amp;", "&"),
            )
        };
        let bounds = get("bounds").and_then(parse_bounds);
        let Some(bounds) = bounds else { continue };
        let text = get("text").filter(|v| !v.is_empty());
        let name = text
            .clone()
            .or_else(|| get("content-desc"))
            .unwrap_or_default();
        if name.is_empty() && get("resource-id").is_none() {
            continue;
        }
        let role = get("class")
            .unwrap_or_else(|| "android.view.View".into())
            .trim_start_matches("android.widget.")
            .to_owned();
        elements.push(Element {
            id: get("resource-id")
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| format!("adb-element-{}", elements.len())),
            role,
            name,
            text,
            bounds,
            enabled: get("enabled").as_deref() != Some("false"),
            focused: get("focused").as_deref() == Some("true"),
            confidence: 0.95,
            source: PerceptionSource::Accessibility,
        });
    }
    Ok(elements)
}

fn parse_bounds(value: String) -> Option<Bounds> {
    let nums: Vec<i64> = value
        .split(|c: char| !c.is_ascii_digit() && c != '-')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    (nums.len() == 4).then(|| Bounds::new(nums[0], nums[1], nums[2], nums[3]))
}

fn to_perception(error: AdbError) -> PerceptionError {
    PerceptionError::Unavailable(error.to_string())
}

/// Current Unix time in milliseconds.
pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or_default()
}

/// Build an action for an ADB primitive (useful to callers integrating core).
pub fn action_for(
    session: SessionId,
    observation_id: u64,
    id: &str,
    primitive: PrimitiveAction,
) -> Action {
    Action {
        id: id.into(),
        session_id: session,
        observation_id,
        kind: harness_protocol::ActionKind::Primitive(primitive),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_android_nodes() {
        let xml = r#"<node text="Settings" class="android.widget.TextView" resource-id="pkg:id/settings" bounds="[1,2][30,40]" enabled="true" focused="false"/>"#;
        let nodes = parse_ui_xml(xml).unwrap();
        assert_eq!(nodes[0].name, "Settings");
        assert_eq!(nodes[0].bounds, Bounds::new(1, 2, 30, 40));
    }
}
