//! Screen capture ([plan §5.1]).
//!
//! The capture backend grabs desktop frames with minimal CPU copies. On
//! Windows this is Windows Graphics Capture / DXGI Desktop Duplication; the
//! type surface here is identical on all platforms so the rest of the harness
//! compiles everywhere.

#![warn(missing_docs)]

use harness_protocol::{Bounds, TimestampMs};

/// A captured screen frame.
#[derive(Debug, Clone)]
pub struct CapturedFrame {
    /// Monotonic frame id.
    pub frame_id: u64,
    /// Capture timestamp (ms).
    pub timestamp: TimestampMs,
    /// Screen dimensions, if known.
    pub screen: Option<Bounds>,
}

/// Errors from the capture backend.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    /// Real capture is not implemented on this platform yet.
    #[error("capture backend not implemented")]
    UnsupportedPlatform,
    /// The capture device was lost (monitor hotplug, session lock, ...).
    #[error("capture device lost: {0}")]
    DeviceLost(String),
    /// A transient capture failure.
    #[error("capture failed: {0}")]
    Failed(String),
}

/// The capture backend. Concrete platform implementations are selected via
/// `cfg`.
#[cfg(not(windows))]
#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureBackend;

#[cfg(not(windows))]
impl CaptureBackend {
    /// Attempt to capture the desktop. Stub returns unsupported off-Windows.
    pub fn capture(&self) -> Result<CapturedFrame, CaptureError> {
        Err(CaptureError::UnsupportedPlatform)
    }
}

/// Windows-native implementation (placeholder for the DXGI/WGC backend).
#[cfg(windows)]
pub mod windows;

/// Windows-native capture backend.
#[cfg(windows)]
pub use windows::CaptureBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_carries_metadata() {
        let f = CapturedFrame {
            frame_id: 1,
            timestamp: 0,
            screen: Some(Bounds::new(0, 0, 1920, 1080)),
        };
        assert_eq!(f.screen.unwrap().width(), 1920);
    }

    #[cfg(not(windows))]
    #[test]
    fn stub_reports_unsupported_off_windows() {
        let b = CaptureBackend;
        assert!(matches!(
            b.capture(),
            Err(CaptureError::UnsupportedPlatform)
        ));
    }
}
