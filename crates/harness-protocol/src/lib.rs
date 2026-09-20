//! Versioned protocol for the AI computer-use harness.
//!
//! This crate defines the stable, versioned message contract between an AI
//! agent and the harness. It is the Phase 0 deliverable and must NOT depend on
//! any implementation crate — only `serde` for serialization.
//!
//! Core entities ([plan §11]):
//! - [`Session`]
//! - [`Observation`]
//! - [`Action`]
//! - [`Target`]
//! - [`ActionResult`]
//! - [`Event`]
//! - [`Capability`]
//! - [`PolicyDecision`]
//!
//! Design rules from the implementation plan:
//! - Every [`Action`] carries the `observation_id` it was derived from.
//! - The harness rejects stale actions (freshness rule, §11.1).
//! - Targets carry a `confidence` and are gated by confidence bands (§9.1).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod action;
pub mod bounds;
pub mod capability;
pub mod event;
pub mod freshness;
pub mod observation;
pub mod policy;
pub mod session;
pub mod target;
pub mod types;
pub mod version;

pub use action::{
    Action, ActionKind, ActionResult, ActionStatus, PrimitiveAction, SemanticAction,
    VerificationResult,
};
pub use bounds::{Bounds, Point};
pub use capability::Capability;
pub use event::{Event, EventKind};
pub use freshness::{check as check_freshness, Freshness, DEFAULT_MAX_AGE_MS};
pub use observation::{CursorState, Element, Observation, PerceptionSource, WindowInfo};
pub use policy::PolicyDecision;
pub use session::{Session, SessionId, SessionStatus};
pub use target::{Target, TargetMethod};
pub use types::{ElementId, ObservationId, RequestId, SessionHandle, TimestampMs};
pub use version::ProtocolVersion;
