//! Shared primitive types used across the protocol.

pub use crate::observation::ObservationId;
pub use crate::session::SessionId;

/// Millisecond timestamp. Conventionally `SystemTime::now()` in ms, but the
/// protocol treats it opaquely: only *differences* are meaningful for
/// freshness checks.
pub type TimestampMs = u64;

/// Opaque identifier for a target resolution request.
pub type RequestId = String;

/// A handle that references an element within an observation.
pub type ElementRef = String;

/// Stable identifier assigned to a UI element by the harness. Must be unique
/// within a session; may be recycled across sessions.
pub type ElementId = String;

/// Handle to a session in gateway-facing APIs (e.g. `s_12`).
pub type SessionHandle = String;
