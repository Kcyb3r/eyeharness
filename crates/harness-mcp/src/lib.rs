//! MCP adapter ([plan §5.5]).
//!
//! Exposes the harness to MCP-capable agents over a JSON-RPC 2.0 surface,
//! including a model-command channel (`mcp.scan`, `mcp.gate` ...). The core
//! remains the authority; this crate only translates model commands into core
//! calls, never policy decisions.

#![forbid(unsafe_code)]

use harness_protocol::{Action, Observation, SessionId};

/// A unit of MCP work.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpRequest {
    /// JSON-RPC 2.0 request id.
    pub id: u64,
    /// Method name, e.g. `mcp.scan`.
    pub method: String,
    /// Optional params.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

/// A unit of MCP response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpResponse {
    /// JSON-RPC 2.0 request id echoed back.
    pub id: u64,
    /// Success payload.
    pub result: serde_json::Value,
}

/// An MCP-served error response.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct McpError {
    /// JSON-RPC 2.0 error code.
    pub code: i64,
    /// Message.
    pub message: String,
}

impl McpError {
    /// JSON-RPC `-32601` — method not found.
    pub fn method_not_found(method: &str) -> Self {
        Self {
            code: -32601,
            message: format!("method not found: {method}"),
        }
    }

    /// JSON-RPC `-32600` — invalid request.
    pub fn invalid_request(detail: &str) -> Self {
        Self {
            code: -32600,
            message: format!("invalid request: {detail}"),
        }
    }

    /// JSON-RPC `-32603` — internal error.
    pub fn internal(detail: &str) -> Self {
        Self {
            code: -32603,
            message: format!("internal error: {detail}"),
        }
    }
}

/// A loose JSON-RPC 2.0 response envelope.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum McpReply {
    /// Success.
    Success(McpResponse),
    /// Error.
    Err { error: McpError },
}

/// The MCP surface. Proof-of-life scaffold; the JSON-RPC transport and
/// method registry are layered in with the gateway (§5.5.1).
#[derive(Debug, Clone, Default)]
pub struct McpSurface {
    method_names: Vec<String>,
}

impl McpSurface {
    /// Register a method name.
    pub fn register_method(&mut self, name: impl Into<String>) {
        self.method_names.push(name.into());
    }

    /// Whether a method is known.
    pub fn has_method(&self, name: &str) -> bool {
        self.method_names.iter().any(|n| n == name)
    }

    /// The set of registered method names.
    pub fn methods(&self) -> &[String] {
        &self.method_names
    }
}

/// A thin model-command channel used by MCP backends to hand requests to a
/// core. Kept minimal to stay a scaffold.
#[derive(Debug, Clone)]
pub struct ModelChannel {
    /// Session context.
    pub session_id: SessionId,
    /// Latest observation.
    pub latest: Option<Observation>,
    /// Latest action for review.
    pub pending: Option<Action>,
}

impl Default for ModelChannel {
    fn default() -> Self {
        Self {
            session_id: SessionId::new(""),
            latest: None,
            pending: None,
        }
    }
}
