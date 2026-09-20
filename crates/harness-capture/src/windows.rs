//! Windows capture backend ([plan §5.1]).
//!
//! Placeholder until the DXGI Desktop Duplication / Windows Graphics Capture
//! implementation lands. The type surface matches the portable forms so the
//! rest of the crate compiles identically on all targets.

use harness_protocol::Bounds;

use crate::{CaptureError, CapturedFrame};

/// Platform capture handle.
#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureBackend;

impl CaptureBackend {
    /// Attempt to capture the desktop.
    ///
    /// TODO(windows): implement via Windows Graphics Capture / DXGI.
    pub fn capture(&self) -> Result<CapturedFrame, CaptureError> {
        Ok(CapturedFrame {
            frame_id: 0,
            timestamp: 0,
            screen: Some(Bounds::new(0, 0, 1920, 1080)),
        })
    }
}
