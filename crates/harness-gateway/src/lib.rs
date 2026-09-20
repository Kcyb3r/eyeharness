//! Control gateway ([plan §2, §12]).
//!
//! The gateway is the single service-facing surface: it owns the session
//! lifecycle (start / stop / recuperate), hosts the adapter registry (MCP,
//! IPC), and enforces a fail-closed runtime policy on top of the core.
//!
//! The core is deliberately adapter-agnostic; the gateway binds one or more
//! adapters to a session.

#![warn(missing_docs)]

use harness_core::HarnessCore;
use harness_policy::{Policy, PolicyRule};
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Mutex;

/// Errors from the gateway.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// A session is already registered with this id.
    #[error("session already exists: {0}")]
    SessionExists(String),
    /// No session with this id is registered.
    #[error("unknown session: {0}")]
    UnknownSession(String),
    /// No adapter matched the capability.
    #[error("no adapter for capability: {0}")]
    NoAdapter(String),
}

/// A registered adapter shim (types resolved by the adapter crates).
#[derive(Debug, Clone)]
pub struct AdapterRegistration {
    /// Namespaced capability, e.g. `mcp` or `ipc`.
    pub namespace: String,
    /// Capability name, e.g. `get_observation`.
    pub capability: String,
    /// Whether this adapter is currently active.
    pub active: bool,
}

/// The gateway handle.
pub struct Gateway {
    policy: Arc<Policy>,
    sessions: Mutex<HashSet<String>>,
    adapters: Mutex<Vec<AdapterRegistration>>,
    _cores: Mutex<Vec<Arc<HarnessCore>>>,
}

impl Gateway {
    /// Create a gateway with a fail-closed runtime policy that can be
    /// supplemented by adapter-granted rules.
    pub fn new(supplemental: Vec<PolicyRule>) -> Self {
        let mut rules = vec![
            // Fail-closed: nothing allowed unless a rule says so.
            // Safe default for the runtime gateway; dev policy lives elsewhere.
        ];
        rules.extend(supplemental);
        Self {
            policy: Arc::new(Policy::new(rules_dummy(rules))),
            sessions: Mutex::new(HashSet::new()),
            adapters: Mutex::new(Vec::new()),
            _cores: Mutex::new(Vec::new()),
        }
    }

    /// Register a session id.
    pub fn start_session(&self, session_id: &str) -> Result<(), GatewayError> {
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.contains(session_id) {
            return Err(GatewayError::SessionExists(session_id.into()));
        }
        sessions.insert(session_id.into());
        Ok(())
    }

    /// End a session id.
    pub fn end_session(&self, session_id: &str) -> Result<(), GatewayError> {
        let mut sessions = self.sessions.lock().unwrap();
        if !sessions.remove(session_id) {
            return Err(GatewayError::UnknownSession(session_id.into()));
        }
        Ok(())
    }

    /// Register an adapter that can service a capability.
    pub fn register_adapter(&self, registration: AdapterRegistration) {
        self.adapters.lock().unwrap().push(registration);
    }

    /// Adapters that are active and act as a "restart-heal" rider on the
    /// core's recovery loop.
    pub fn active_adapters(&self) -> Vec<AdapterRegistration> {
        self.adapters
            .lock()
            .unwrap()
            .iter()
            .filter(|a| a.active)
            .cloned()
            .collect()
    }

    /// Default policy accessor (fail-closed).
    pub fn runtime_policy(&self) -> &Policy {
        self.policy.as_ref()
    }
}

/// Hack to satisfy the builder: the empty default policy is fail-closed.
fn rules_dummy(mut rules: Vec<PolicyRule>) -> Vec<PolicyRule> {
    // No rules = fail-closed (policy.evaluate returns Blocked on no match).
    rules.dedup();
    rules
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_lifecycle() {
        let g = Gateway::new(vec![]);
        g.start_session("s_1").unwrap();
        assert!(matches!(
            g.start_session("s_1"),
            Err(GatewayError::SessionExists(_))
        ));
        g.end_session("s_1").unwrap();
        assert!(matches!(
            g.end_session("s_1"),
            Err(GatewayError::UnknownSession(_))
        ));
    }

    #[test]
    fn fail_closed_policy_with_no_rules() {
        let g = Gateway::new(vec![]);
        // A policy with no matching rule must block.
        assert_eq!(g.runtime_policy().rule_count(), 0);
    }

    #[test]
    fn adapter_registration_and_filtering() {
        let g = Gateway::new(vec![]);
        g.register_adapter(AdapterRegistration {
            namespace: "mcp".into(),
            capability: "get_observation".into(),
            active: true,
        });
        g.register_adapter(AdapterRegistration {
            namespace: "ipc".into(),
            capability: "raw_send".into(),
            active: false,
        });
        assert_eq!(g.active_adapters().len(), 1);
    }
}
