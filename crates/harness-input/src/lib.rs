//! Native input executor ([plan §5.2]).
//!
//! Executes [`PrimitiveAction`] values through native OS APIs. On Windows this
//! is `SendInput`, `SetCursorPos`, and keyboard APIs; elsewhere it's a stub
//! that rejects silently so the harness can still be built and tested.

#![warn(missing_docs)]

use harness_protocol::{Point, PrimitiveAction};

/// Errors from the input executor.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// Real input is not implemented on this platform.
    #[error("native input not implemented on this platform")]
    UnsupportedPlatform,
    /// The OS rejected the input event.
    #[error("input rejected: {0}")]
    Rejected(String),
}

/// A platform-neutral input executor.
#[derive(Debug, Clone, Copy, Default)]
pub struct NativeInput;

impl NativeInput {
    /// Execute a primitive action.
    pub fn execute(&self, action: &PrimitiveAction) -> Result<(), InputError> {
        self.dispatch(action)
    }

    /// Inner dispatch — implemented per platform.
    #[cfg(all(not(windows), not(target_os = "linux")))]
    fn dispatch(&self, _action: &PrimitiveAction) -> Result<(), InputError> {
        Err(InputError::UnsupportedPlatform)
    }

    /// Linux desktop dispatch through the standard X11 `xdotool` utility.
    #[cfg(target_os = "linux")]
    fn dispatch(&self, action: &PrimitiveAction) -> Result<(), InputError> {
        let args: Vec<String> = match action {
            PrimitiveAction::Move { x, y } => {
                vec!["mousemove".into(), x.to_string(), y.to_string()]
            }
            PrimitiveAction::Click { x, y } => vec![
                "mousemove".into(),
                x.to_string(),
                y.to_string(),
                "click".into(),
                "1".into(),
            ],
            PrimitiveAction::RightClick { x, y } => vec![
                "mousemove".into(),
                x.to_string(),
                y.to_string(),
                "click".into(),
                "3".into(),
            ],
            PrimitiveAction::DoubleClick { x, y } => vec![
                "mousemove".into(),
                x.to_string(),
                y.to_string(),
                "click".into(),
                "--repeat".into(),
                "2".into(),
                "--delay".into(),
                "100".into(),
                "1".into(),
            ],
            PrimitiveAction::Type { text } => {
                vec!["type".into(), "--clearmodifiers".into(), text.clone()]
            }
            PrimitiveAction::Keypress { key } => vec!["key".into(), key.clone()],
            PrimitiveAction::Hotkey { modifiers, key } => {
                let mut combo = modifiers.join("+");
                if !combo.is_empty() {
                    combo.push('+');
                }
                combo.push_str(key);
                vec!["key".into(), combo]
            }
            PrimitiveAction::Scroll { delta, x, y } => vec![
                "mousemove".into(),
                x.to_string(),
                y.to_string(),
                "click".into(),
                if *delta >= 0 { "4".into() } else { "5".into() },
                "--repeat".into(),
                delta.unsigned_abs().max(1).to_string(),
            ],
            PrimitiveAction::Drag { x0, y0, x1, y1 } => vec![
                "mousemove".into(),
                x0.to_string(),
                y0.to_string(),
                "mousedown".into(),
                "1".into(),
                "mousemove".into(),
                "--duration".into(),
                "0.2".into(),
                x1.to_string(),
                y1.to_string(),
                "mouseup".into(),
                "1".into(),
            ],
            PrimitiveAction::Wait { ms } => {
                std::thread::sleep(std::time::Duration::from_millis(*ms));
                return Ok(());
            }
        };
        let output = std::process::Command::new("xdotool")
            .args(&args)
            .output()
            .map_err(|e| {
                InputError::Rejected(format!(
                    "xdotool unavailable; install xdotool and use an X11 session: {e}"
                ))
            })?;
        if output.status.success() {
            Ok(())
        } else {
            Err(InputError::Rejected(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ))
        }
    }

    /// Windows dispatch through the user32 input APIs.
    #[cfg(windows)]
    fn dispatch(&self, action: &PrimitiveAction) -> Result<(), InputError> {
        match action {
            PrimitiveAction::Move { x, y } => set_cursor(*x, *y),
            PrimitiveAction::Click { x, y } => {
                set_cursor(*x, *y)?;
                mouse_click(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
            }
            PrimitiveAction::RightClick { x, y } => {
                set_cursor(*x, *y)?;
                mouse_click(MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP)
            }
            PrimitiveAction::DoubleClick { x, y } => {
                set_cursor(*x, *y)?;
                mouse_click(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)?;
                mouse_click(MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP)
            }
            PrimitiveAction::Type { text } => type_text(text),
            PrimitiveAction::Keypress { key } => key_event(key, 0),
            PrimitiveAction::Hotkey { modifiers, key } => {
                for modifier in modifiers {
                    key_event(modifier, 0)?;
                }
                key_event(key, 0)?;
                for modifier in modifiers.iter().rev() {
                    key_event(modifier, KEYEVENTF_KEYUP)?;
                }
                Ok(())
            }
            PrimitiveAction::Scroll { delta, x, y } => {
                set_cursor(*x, *y)?;
                mouse_wheel(*delta)
            }
            PrimitiveAction::Drag { x0, y0, x1, y1 } => {
                set_cursor(*x0, *y0)?;
                mouse_click(MOUSEEVENTF_LEFTDOWN, 0)?;
                set_cursor(*x1, *y1)?;
                mouse_click(MOUSEEVENTF_LEFTUP, 0)
            }
            PrimitiveAction::Wait { ms } => {
                std::thread::sleep(std::time::Duration::from_millis(*ms));
                Ok(())
            }
        }
    }

    /// Move the cursor (cross-platform coordinate helper).
    pub fn move_to(&self, p: Point) -> Result<(), InputError> {
        self.dispatch(&PrimitiveAction::Move { x: p.x, y: p.y })
    }
}

#[cfg(windows)]
const INPUT_MOUSE: u32 = 0;
#[cfg(windows)]
const INPUT_KEYBOARD: u32 = 1;
#[cfg(windows)]
const MOUSEEVENTF_LEFTDOWN: u32 = 0x0002;
#[cfg(windows)]
const MOUSEEVENTF_LEFTUP: u32 = 0x0004;
#[cfg(windows)]
const MOUSEEVENTF_RIGHTDOWN: u32 = 0x0008;
#[cfg(windows)]
const MOUSEEVENTF_RIGHTUP: u32 = 0x0010;
#[cfg(windows)]
const MOUSEEVENTF_WHEEL: u32 = 0x0800;
#[cfg(windows)]
const KEYEVENTF_KEYUP: u32 = 0x0002;
#[cfg(windows)]
const KEYEVENTF_UNICODE: u32 = 0x0004;

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
struct MouseInput {
    dx: i32,
    dy: i32,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
struct KeyboardInput {
    vk: u16,
    scan: u16,
    flags: u32,
    time: u32,
    extra_info: usize,
}

#[cfg(windows)]
#[repr(C)]
union InputUnion {
    mouse: MouseInput,
    keyboard: KeyboardInput,
}

#[cfg(windows)]
#[repr(C)]
struct Input {
    kind: u32,
    data: InputUnion,
}

#[cfg(windows)]
extern "system" {
    fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    fn SetCursorPos(x: i32, y: i32) -> i32;
    fn VkKeyScanW(ch: u16) -> i16;
}

#[cfg(windows)]
fn send(input: Input) -> Result<(), InputError> {
    let sent = unsafe { SendInput(1, &input, std::mem::size_of::<Input>() as i32) };
    (sent == 1)
        .then_some(())
        .ok_or_else(|| InputError::Rejected("SendInput rejected the event".into()))
}

#[cfg(windows)]
fn set_cursor(x: i64, y: i64) -> Result<(), InputError> {
    let ok = unsafe { SetCursorPos(x as i32, y as i32) };
    (ok != 0)
        .then_some(())
        .ok_or_else(|| InputError::Rejected("SetCursorPos failed".into()))
}

#[cfg(windows)]
fn mouse_click(down: u32, up: u32) -> Result<(), InputError> {
    let mouse = |flags| Input {
        kind: INPUT_MOUSE,
        data: InputUnion {
            mouse: MouseInput {
                dx: 0,
                dy: 0,
                mouse_data: 0,
                flags,
                time: 0,
                extra_info: 0,
            },
        },
    };
    send(mouse(down))?;
    send(mouse(up))
}

#[cfg(windows)]
fn mouse_wheel(delta: i64) -> Result<(), InputError> {
    send(Input {
        kind: INPUT_MOUSE,
        data: InputUnion {
            mouse: MouseInput {
                dx: 0,
                dy: 0,
                mouse_data: delta as u32,
                flags: MOUSEEVENTF_WHEEL,
                time: 0,
                extra_info: 0,
            },
        },
    })
}

#[cfg(windows)]
fn type_text(text: &str) -> Result<(), InputError> {
    for unit in text.encode_utf16() {
        for flags in [KEYEVENTF_UNICODE, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP] {
            send(Input {
                kind: INPUT_KEYBOARD,
                data: InputUnion {
                    keyboard: KeyboardInput {
                        vk: 0,
                        scan: unit,
                        flags,
                        time: 0,
                        extra_info: 0,
                    },
                },
            })?;
        }
    }
    Ok(())
}

#[cfg(windows)]
fn key_event(key: &str, flags: u32) -> Result<(), InputError> {
    let vk =
        virtual_key(key).ok_or_else(|| InputError::Rejected(format!("unsupported key: {key}")))?;
    send(Input {
        kind: INPUT_KEYBOARD,
        data: InputUnion {
            keyboard: KeyboardInput {
                vk,
                scan: 0,
                flags,
                time: 0,
                extra_info: 0,
            },
        },
    })
}

#[cfg(windows)]
fn virtual_key(key: &str) -> Option<u16> {
    let normalized = key.to_ascii_lowercase();
    let named = match normalized.as_str() {
        "enter" => 0x0D,
        "tab" => 0x09,
        "escape" | "esc" => 0x1B,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "space" => 0x20,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" => 0x21,
        "pagedown" => 0x22,
        "shift" => 0x10,
        "ctrl" | "control" => 0x11,
        "alt" => 0x12,
        "win" | "meta" => 0x5B,
        "f1" => 0x70,
        "f2" => 0x71,
        "f3" => 0x72,
        "f4" => 0x73,
        "f5" => 0x74,
        "f6" => 0x75,
        "f7" => 0x76,
        "f8" => 0x77,
        "f9" => 0x78,
        "f10" => 0x79,
        "f11" => 0x7A,
        "f12" => 0x7B,
        _ => {
            return normalized.chars().next().and_then(|c| {
                let code = unsafe { VkKeyScanW(c as u16) };
                (code >= 0).then_some((code as u16) & 0xff)
            })
        }
    };
    Some(named)
}

#[cfg(test)]
#[cfg(all(not(windows), not(target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    #[cfg(all(not(windows), not(target_os = "linux")))]
    fn stub_execute_returns_typed_error() {
        let input = NativeInput;
        let err = input
            .execute(&PrimitiveAction::Click { x: 1, y: 1 })
            .unwrap_err();
        match err {
            #[cfg(all(not(windows), not(target_os = "linux")))]
            InputError::UnsupportedPlatform => {}
            #[cfg(target_os = "linux")]
            InputError::Rejected(_) => {}
            #[cfg(windows)]
            InputError::Rejected(_) => {}
            _ => panic!("unexpected error"),
        }
    }
}
