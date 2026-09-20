//! Harness core ([plan §2, §51]) — observation → state → action → verify →
//! recover orchestration.
//!
//! The core owns execution: it validates every request against policy,
//! resolves targets, dispatches through the executor, verifies the result,
//! and routes failures into recovery. It is fully deterministic and never
//! depends on which model is attached.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::sync::{Arc, Mutex};

use harness_events::{BusError, EventBus, EventSender};
use harness_perception::PerceptionHub;
use harness_policy::{ActionValidator, Policy, PolicyRule, RuleAction};
use harness_protocol::{
    check_freshness, Action, ActionKind, ActionResult, ActionStatus, Capability, Event, EventKind,
    Freshness, Observation, PolicyDecision, PrimitiveAction, SemanticAction, Session,
    SessionStatus, Target, TimestampMs, VerificationResult,
};
use harness_recovery::RecoverySession;
use harness_verifier::{run_verification, ScreenChangeVerifier, VerifyOutcome};

/// Errors surfaced by the core pipeline.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// The request is stale and must be re-observed.
    #[error("stale observation: age {age_ms}ms > max {max_ms}ms")]
    StaleObservation {
        /// Observed age.
        age_ms: u64,
        /// Maximum allowed age.
        max_ms: u64,
    },
    /// Policy denied the action.
    #[error("policy: {0}")]
    Policy(String),
    /// Structural validation failed.
    #[error("invalid action: {0}")]
    InvalidAction(String),
    /// Target resolution failed.
    #[error("target resolution failed: {0}")]
    ResolutionFailed(String),
    /// Executor rejected the primitive.
    #[error("executor failed: {0}")]
    ExecutionFailed(String),
    /// Internal bus failure.
    #[error("event bus error: {0}")]
    Bus(#[from] BusError),
}

/// The trait executed actions must implement in the platform layer.
///
/// Implementations are provided per-platform (Win32, browser/CDP, ...). The
/// core only sees the primitive contract.
pub trait Executor: Send + Sync {
    /// Execute a primitive action, returning `Ok(())` or an error string.
    fn execute(&self, action: &PrimitiveAction) -> Result<(), String>;
}

/// A completion for an action, produced after verification.
#[derive(Debug, Clone, PartialEq)]
pub struct ActionCompletion {
    /// The request id.
    pub action_id: String,
    /// Outcome status.
    pub status: ActionStatus,
    /// Optional resolved target used.
    pub target: Option<Target>,
    /// Observation captured after execution, when verification ran.
    pub observation_after: Option<u64>,
    /// Verification outcome exposed to the agent.
    pub verification: Option<VerificationResult>,
}

/// The core runtime handle.
#[derive(Clone)]
pub struct HarnessCore {
    session: Arc<Mutex<Session>>,
    state: Arc<Mutex<harness_state::HarnessState>>,
    perception: Arc<PerceptionHub>,
    sender: EventSender,
    executor: Arc<dyn Executor>,
    max_observation_age_ms: TimestampMs,
    recovery: Arc<Mutex<RecoverySession>>,
    verifier: Arc<dyn harness_verifier::Verifier>,
    default_policy: Arc<Policy>,
}

impl HarnessCore {
    /// Create a core with the given session, perception hub, executor, and
    /// event bus.
    pub fn new(
        session: Session,
        perception: Arc<PerceptionHub>,
        executor: Arc<dyn Executor>,
        bus: EventBus,
    ) -> Self {
        let sender = bus.sender();
        let default_policy = Policy::new(vec![
            // Fail-open for the default dev policy; production policies are
            // configured at the gateway and always fail-closed.
            PolicyRule::builder()
                .capability(Capability::from("computer.*"))
                .action(RuleAction::Allow)
                .build(),
        ]);
        Self {
            session: Arc::new(Mutex::new(session)),
            state: Arc::new(Mutex::new(harness_state::HarnessState::new())),
            perception,
            sender,
            executor,
            max_observation_age_ms: harness_protocol::DEFAULT_MAX_AGE_MS,
            recovery: Arc::new(Mutex::new(RecoverySession::default())),
            verifier: Arc::new(ScreenChangeVerifier),
            default_policy: Arc::new(default_policy),
        }
    }

    /// Override the observation freshness window ([plan §11.1]).
    pub fn with_max_observation_age(mut self, ms: TimestampMs) -> Self {
        self.max_observation_age_ms = ms;
        self
    }

    /// Override the verifier used for post-action evidence.
    pub fn with_verifier(mut self, v: Arc<dyn harness_verifier::Verifier>) -> Self {
        self.verifier = v;
        self
    }

    /// Begin the session.
    pub fn start(&self) {
        {
            let mut session = self.session.lock().unwrap();
            session.set_status(SessionStatus::Running);
        }
        self.emit(EventKind::SessionStarted, None);
    }

    /// Snapshot the current session.
    pub fn session(&self) -> Session {
        self.session.lock().unwrap().clone()
    }

    /// Produce a fresh observation from the perception hub and update state.
    ///
    /// This is the OBSERVE step: it integrates the result into the state
    /// engine so [`HarnessCore::state`] is always current.
    pub fn observe(&self) -> Result<Observation, CoreError> {
        let (session_id, observation_id) = {
            let mut session = self.session.lock().unwrap();
            (session.id.clone(), session.next_observation_id())
        };

        let mut ob = self
            .perception
            .observe(&session_id)
            .map_err(|e| CoreError::ResolutionFailed(e.to_string()))?;

        ob.observation_id = observation_id;
        ob.session_id = session_id.clone();
        ob.timestamp = now_ms();

        self.state.lock().unwrap().integrate(&ob);
        self.emit(EventKind::ObservationCreated, Some(observation_id));
        Ok(ob)
    }

    /// Validate + gate an action through the full pipeline.
    ///
    /// Returns the resolved target the executor should act on. The caller must
    /// pass the observation the action references (`latest`).
    pub fn gate(&self, action: &Action, latest: &Observation) -> Result<Target, CoreError> {
        if action.session_id != latest.session_id {
            return Err(CoreError::InvalidAction(
                "action session does not match observation session".into(),
            ));
        }
        if action.observation_id != latest.observation_id {
            return Err(CoreError::InvalidAction(format!(
                "action references observation {}, but latest is {}",
                action.observation_id, latest.observation_id
            )));
        }
        // 1. Freshness (§11.1): action must reference a non-stale observation.
        if let Freshness::Stale(age) =
            check_freshness(latest, now_ms(), self.max_observation_age_ms)
        {
            self.emit(EventKind::ObservationStale, Some(latest.observation_id));
            return Err(CoreError::StaleObservation {
                age_ms: age,
                max_ms: self.max_observation_age_ms,
            });
        }

        // 2. Structural validation (validator is separate from policy).
        ActionValidator::default()
            .validate(action)
            .map_err(|e| CoreError::InvalidAction(format!("{e:?}")))?;

        // 3. Policy gate (§3.3.1): the model never decides safety.
        match self.default_policy.evaluate(&action.session_id, action) {
            PolicyDecision::Allowed { .. } => {}
            decision => {
                self.emit(EventKind::PolicyBlocked, Some(latest.observation_id));
                return Err(CoreError::Policy(decision_to_reason(&decision)));
            }
        }

        // 4. Target resolution (§16): map intent to a concrete target.
        self.resolve_target(action, latest)
    }

    /// Execute a fully gated action against the executor, then verify.
    pub fn execute(&self, action: &Action, target: &Target) -> Result<ActionCompletion, CoreError> {
        let primitive = match &action.kind {
            ActionKind::Semantic(s) => map_semantic(s, target),
            ActionKind::Primitive(p) => p.clone(),
        };

        self.emit(EventKind::ActionRequested, None);
        match self.executor.execute(&primitive) {
            Ok(()) => {
                // Re-observe after acting to gather verification evidence.
                let after = self.observe()?;
                let evidence = run_verification(None, &after, self.verifier.as_ref());
                let status = match evidence.outcome {
                    VerifyOutcome::Passed | VerifyOutcome::NotApplicable => ActionStatus::Success,
                    VerifyOutcome::Failed(_) => ActionStatus::Failed,
                };
                self.emit(
                    if status == ActionStatus::Success {
                        EventKind::ActionExecuted
                    } else {
                        EventKind::ActionFailed
                    },
                    Some(after.observation_id),
                );
                if status == ActionStatus::Failed {
                    self.enter_recovery(&after);
                }
                Ok(ActionCompletion {
                    action_id: action.id.clone(),
                    status,
                    target: Some(target.clone()),
                    observation_after: Some(after.observation_id),
                    verification: Some(VerificationResult {
                        ok: status == ActionStatus::Success,
                        detail: format!("{evidence:?}"),
                    }),
                })
            }
            Err(e) => {
                self.emit(EventKind::ActionFailed, None);
                Err(CoreError::ExecutionFailed(e))
            }
        }
    }

    /// Produce an agent-facing [`ActionResult`] from a completion ([plan §16]).
    pub fn result(&self, completion: ActionCompletion) -> ActionResult {
        ActionResult {
            action_id: completion.action_id,
            status: completion.status,
            target: completion.target,
            screen_changed: true,
            observation_after: completion.observation_after,
            verification: completion.verification,
            rejection: None,
            latency_ms: None,
            timestamp: now_ms(),
        }
    }

    /// Snapshot the state engine (for HUD/debug/replay consumers).
    pub fn state(&self) -> harness_state::HarnessState {
        self.state.lock().unwrap().clone()
    }

    /// The live event bus (for HUD/replay/metrics subscribers).
    pub fn bus(&self) -> EventBus {
        self.sender.bus_ref()
    }

    fn resolve_target(&self, action: &Action, latest: &Observation) -> Result<Target, CoreError> {
        let resolver = harness_target_resolver::TargetResolver::default();
        let query = match &action.kind {
            ActionKind::Semantic(
                SemanticAction::ClickTarget { text }
                | SemanticAction::Find { text }
                | SemanticAction::WaitFor { text, .. },
            ) => harness_target_resolver::TargetQuery::text(text.clone()),
            ActionKind::Primitive(PrimitiveAction::Click { x, y })
            | ActionKind::Primitive(PrimitiveAction::Move { x, y }) => {
                harness_target_resolver::TargetQuery::Coordinates(harness_protocol::Point::new(
                    *x, *y,
                ))
            }
            _ => {
                return Ok(Target {
                    request_id: format!("r_{}", latest.observation_id),
                    element_id: None,
                    name: "action".into(),
                    bounds: harness_protocol::Bounds::new(0, 0, 0, 0),
                    confidence: 1.0,
                    method: harness_protocol::TargetMethod::Coordinates,
                })
            }
        };
        resolver
            .resolve(latest, &query, true)
            .map_err(|e| CoreError::ResolutionFailed(e.to_string()))
    }

    fn enter_recovery(&self, after: &Observation) {
        let mut rec = self.recovery.lock().unwrap();
        rec.mark_reobserved(after.observation_id);
        self.emit(EventKind::RecoveryStarted, Some(after.observation_id));
        match rec.step() {
            Ok(_) => {
                self.emit(EventKind::RecoveryCompleted, Some(after.observation_id));
                // The caller (agent loop) retries via gate/execute.
            }
            Err(_) => {
                rec.abort();
                self.emit(EventKind::RecoveryFailed, Some(after.observation_id));
            }
        }
    }

    fn emit(&self, kind: EventKind, observation_id: Option<u64>) {
        let session_id = self.session.lock().unwrap().id.clone();
        let mut ev = Event::new(kind, now_ms()).in_session(session_id);
        if let Some(id) = observation_id {
            ev = ev.for_observation(id);
        }
        let _ = self.sender.publish(ev);
    }
}

/// Map a semantic action to a concrete primitive using a resolved target.
fn map_semantic(s: &SemanticAction, target: &Target) -> PrimitiveAction {
    match s {
        SemanticAction::ClickTarget { .. } | SemanticAction::Find { .. } => {
            let c = target.bounds.center();
            PrimitiveAction::Click { x: c.x, y: c.y }
        }
        SemanticAction::WaitFor { timeout_ms, .. } => PrimitiveAction::Wait { ms: *timeout_ms },
        // Read/Select/Navigate are handled by the browser/desktop executor at
        // the platform layer; the core treats them as a no-op primitive until
        // those backends exist.
        SemanticAction::Read { .. }
        | SemanticAction::Select { .. }
        | SemanticAction::Navigate { .. } => PrimitiveAction::Wait { ms: 0 },
    }
}

fn decision_to_reason(d: &PolicyDecision) -> String {
    match d {
        PolicyDecision::Allowed { reason } => reason.clone().unwrap_or_default(),
        PolicyDecision::Blocked { reason } => reason.clone(),
        PolicyDecision::ConfirmationRequired { .. } => "confirmation_required".into(),
    }
}

/// Current monotonic-ish millisecond timestamp.
pub fn now_ms() -> TimestampMs {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use harness_events::EventBus;
    use harness_perception::PerceptionProvider;
    use harness_protocol::{Element, PerceptionSource, SessionId};
    use std::sync::Arc;

    /// A perception provider that returns a controllable static observation.
    struct StaticProvider {
        elements: Vec<Element>,
    }

    impl PerceptionProvider for StaticProvider {
        fn observe(
            &self,
            session: &SessionId,
        ) -> Result<Observation, harness_perception::PerceptionError> {
            let mut ob = Observation::new(session.clone(), 0, now_ms());
            ob.elements = self.elements.clone();
            // The stub screen "changed" once it was observed; this keeps the
            // default screen-change verifier meaningful for the click tests.
            ob.screen_changed = true;
            Ok(ob)
        }
        fn source(&self) -> PerceptionSource {
            PerceptionSource::Screenshot
        }
    }

    struct StubExecutor;

    impl Executor for StubExecutor {
        fn execute(&self, action: &PrimitiveAction) -> Result<(), String> {
            match action {
                PrimitiveAction::Click { .. } | PrimitiveAction::Move { .. } => Ok(()),
                PrimitiveAction::Wait { ms } if *ms > 0 => Ok(()),
                other => Err(format!("unsupported in stub: {other:?}")),
            }
        }
    }

    fn make_core() -> HarnessCore {
        let session = Session::new(SessionId::new("s_1"), 0);
        let mut hub = PerceptionHub::new();
        hub.register(Box::new(StaticProvider {
            elements: vec![Element {
                id: "e1".into(),
                role: "button".into(),
                name: "Submit".into(),
                text: Some("Submit".into()),
                bounds: harness_protocol::Bounds::new(0, 0, 20, 10),
                enabled: true,
                focused: false,
                confidence: 0.99,
                source: PerceptionSource::Accessibility,
            }],
        }));
        HarnessCore::new(
            session,
            Arc::new(hub),
            Arc::new(StubExecutor),
            EventBus::new(),
        )
    }

    #[test]
    fn observe_produces_observation_and_advances_state() {
        let core = make_core();
        core.start();
        let ob = core.observe().unwrap();
        assert_eq!(ob.observation_id, 0);
        assert_eq!(core.state().element_count(), 1);
        assert_eq!(core.session().status, SessionStatus::Running);
    }

    #[test]
    fn gate_rejects_stale_observation() {
        let core = make_core();
        // The referenced observation is ancient.
        let mut latest = Observation::new(SessionId::new("s_1"), 99, 0);
        latest.timestamp = 0;
        let action = Action {
            id: "a1".into(),
            session_id: SessionId::new("s_1"),
            observation_id: 99,
            kind: ActionKind::Semantic(SemanticAction::ClickTarget {
                text: "Submit".into(),
            }),
        };
        let err = core.gate(&action, &latest).unwrap_err();
        assert!(matches!(err, CoreError::StaleObservation { .. }));
    }

    #[test]
    fn gate_and_execute_semantic_click() {
        let core = make_core();
        core.start();
        let latest = core.observe().unwrap();
        let action = Action {
            id: "a2".into(),
            session_id: SessionId::new("s_1"),
            observation_id: latest.observation_id,
            kind: ActionKind::Semantic(SemanticAction::ClickTarget {
                text: "Submit".into(),
            }),
        };
        let target = core.gate(&action, &latest).unwrap();
        assert!(target.is_executable());
        let completion = core.execute(&action, &target).unwrap();
        assert_eq!(completion.status, ActionStatus::Success);
    }

    #[test]
    fn fresh_observation_passes_gate() {
        let core = make_core();
        core.start();
        let latest = core.observe().unwrap(); // timestamp = now
        let action = Action {
            id: "a3".into(),
            session_id: SessionId::new("s_1"),
            observation_id: latest.observation_id,
            kind: ActionKind::Semantic(SemanticAction::Find {
                text: "Submit".into(),
            }),
        };
        let target = core.gate(&action, &latest).unwrap();
        assert!(target.is_executable());
        assert_eq!(target.element_id.as_deref(), Some("e1"));
    }
}
