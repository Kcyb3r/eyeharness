//! Linux desktop MCP server using live X11 perception and native input.

#![forbid(unsafe_code)]

use std::io::{self, BufRead, BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use harness_core::{Executor, HarnessCore};
use harness_events::EventBus;
use harness_perception::PerceptionHub;
use harness_protocol::{Action, ActionKind, PrimitiveAction, Session};

#[derive(Debug, Clone, Copy, Default)]
struct NativeExecutor;

impl Executor for NativeExecutor {
    fn execute(&self, action: &PrimitiveAction) -> Result<(), String> {
        harness_input::NativeInput::default()
            .execute(action)
            .map_err(|e| e.to_string())
    }
}

fn make_core() -> (HarnessCore, EventBus) {
    let mut perception = PerceptionHub::new();
    perception.register(Box::new(
        harness_windows_uia::UiaProvider::new()
            .expect("desktop perception provider construction must not fail"),
    ));
    let bus = EventBus::new();
    let core = HarnessCore::new(
        Session::new("s_mcp", 0),
        Arc::new(perception),
        Arc::new(NativeExecutor),
        bus.clone(),
    );
    (core, bus)
}

struct McpServer {
    core: HarnessCore,
    learning: Mutex<harness_state::learning::LearningStore>,
    audit: Arc<harness_logging::JsonlWriter>,
}

impl McpServer {
    fn new() -> Result<Self, String> {
        let learning = harness_state::learning::LearningStore::open(
            harness_state::learning::LearningStore::default_path(),
        )
        .map_err(|e| e.to_string())?;
        let audit_path = audit_path();
        if let Some(parent) = audit_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let audit =
            Arc::new(harness_logging::JsonlWriter::create(&audit_path).map_err(|e| e.to_string())?);
        let (core, bus) = make_core();
        let mut events = bus.subscribe();
        let audit_sink = Arc::clone(&audit);
        std::thread::Builder::new()
            .name("eyeharness-audit".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("failed to start audit subscriber: {error}");
                        return;
                    }
                };
                runtime.block_on(async move {
                    while let Some(event) = events.recv().await {
                        if let Err(error) = audit_sink.write_event(&event) {
                            eprintln!("failed to persist audit event: {error}");
                        }
                    }
                });
            })
            .map_err(|e| e.to_string())?;
        Ok(Self {
            core,
            learning: Mutex::new(learning),
            audit,
        })
    }

    fn handle(&self, line: &str) -> String {
        let request = serde_json::from_str::<serde_json::Value>(line).unwrap_or_else(|_| {
            serde_json::json!({
                "method": "invalid-json",
                "raw": line,
            })
        });
        let _ = self.audit.write_line(serde_json::json!({
            "kind": "mcp.requested",
            "timestamp": harness_adb::now_ms(),
            "payload": request,
        }));
        let req: serde_json::Value = match serde_json::from_str(line) {
            Ok(r) => r,
            Err(e) => return err(0, -32700, &e.to_string()),
        };
        let id = req.get("id").and_then(|v| v.as_u64()).unwrap_or(0);
        match req.get("method").and_then(|v| v.as_str()).unwrap_or("") {
            "initialize" => ok(
                id,
                serde_json::json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "harness-mcp", "version": env!("CARGO_PKG_VERSION") },
                }),
            ),
            "ping" => ok(id, serde_json::json!({})),
            "tools/list" => ok(
                id,
                serde_json::json!({
                    "tools": [
                        { "name": "observe", "description": "Observe the live desktop.",
                          "inputSchema": { "type": "object", "properties": {} } },
                        { "name": "execute", "description": "Gate and execute a real desktop click.",
                          "inputSchema": { "type": "object", "required": ["x", "y"],
                            "properties": { "x": { "type": "integer" }, "y": { "type": "integer" } } } },
                        { "name": "keypress", "description": "Press a key or hotkey on the live desktop.",
                          "inputSchema": { "type": "object", "required": ["key"],
                            "properties": { "key": { "type": "string" }, "modifiers": { "type": "array", "items": { "type": "string" } } } } },
                        { "name": "type", "description": "Type text into the focused application.",
                          "inputSchema": { "type": "object", "required": ["text"],
                            "properties": { "text": { "type": "string" } } } },
                        { "name": "launch", "description": "Launch an application through KDE KRunner.",
                          "inputSchema": { "type": "object", "required": ["app"],
                            "properties": { "app": { "type": "string" } } } },
                        { "name": "hud", "description": "Show a floating desktop status box.",
                          "inputSchema": { "type": "object", "required": ["message"],
                            "properties": { "message": { "type": "string" } } } },
                        { "name": "verify", "description": "Verify the latest live observation.",
                          "inputSchema": { "type": "object", "properties": {} } }
                        ,{ "name": "learn_record", "description": "Record an approved outcome for a candidate strategy. New strategies remain unapproved.",
                          "inputSchema": { "type": "object", "required": ["profile", "application", "task", "strategy", "success"],
                            "properties": { "profile": { "type": "string" }, "application": { "type": "string" }, "task": { "type": "string" }, "strategy": { "type": "string" }, "success": { "type": "boolean" } } } }
                        ,{ "name": "learn_approve", "description": "Explicitly approve or revoke a learned strategy for automatic suggestions.",
                          "inputSchema": { "type": "object", "required": ["profile", "application", "task", "strategy", "approved"],
                            "properties": { "profile": { "type": "string" }, "application": { "type": "string" }, "task": { "type": "string" }, "strategy": { "type": "string" }, "approved": { "type": "boolean" } } } }
                        ,{ "name": "learn_lookup", "description": "List non-expired, explicitly approved strategies for a task.",
                          "inputSchema": { "type": "object", "required": ["profile", "application", "task"],
                            "properties": { "profile": { "type": "string" }, "application": { "type": "string" }, "task": { "type": "string" } } } }
                        ,{ "name": "learn_export", "description": "Export the inspectable learning profile.",
                          "inputSchema": { "type": "object", "properties": {} } }
                        ,{ "name": "learn_reset", "description": "Delete all learned entries or entries for one profile.",
                          "inputSchema": { "type": "object", "properties": { "profile": { "type": "string" } } } }
                        ,{ "name": "adb_devices", "description": "List connected Android devices.",
                          "inputSchema": { "type": "object", "properties": {} } }
                        ,{ "name": "adb_screenshot", "description": "Capture a bounded Android screenshot.",
                          "inputSchema": { "type": "object", "properties": { "serial": { "type": "string" } } } }
                        ,{ "name": "adb_tap", "description": "Tap an Android screen coordinate.",
                          "inputSchema": { "type": "object", "required": ["x", "y"],
                            "properties": { "serial": { "type": "string" }, "x": { "type": "integer" }, "y": { "type": "integer" } } } }
                        ,{ "name": "adb_type", "description": "Type text on Android.",
                          "inputSchema": { "type": "object", "required": ["text"],
                            "properties": { "serial": { "type": "string" }, "text": { "type": "string" } } } }
                        ,{ "name": "adb_key", "description": "Send an Android keyevent.",
                          "inputSchema": { "type": "object", "required": ["key"],
                            "properties": { "serial": { "type": "string" }, "key": { "type": "string" } } } }
                        ,{ "name": "adb_launch", "description": "Launch an Android package with monkey.",
                          "inputSchema": { "type": "object", "required": ["package"],
                            "properties": { "serial": { "type": "string" }, "package": { "type": "string" } } } }
                        ,{ "name": "adb_current_app", "description": "Read the Android foreground window.",
                          "inputSchema": { "type": "object", "properties": { "serial": { "type": "string" } } } }
                    ]
                }),
            ),
            "tools/call" => self.tool_call(id, req.get("params")),
            method => err(id, -32601, &format!("method not found: {method}")),
        }
    }

    fn tool_call(&self, id: u64, params: Option<&serde_json::Value>) -> String {
        let name = params
            .and_then(|p| p.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let args = params
            .and_then(|p| p.get("arguments"))
            .cloned()
            .unwrap_or_default();
        match name {
            "observe" => match self.core.observe() {
                Ok(ob) => ok(id, serde_json::json!({ "observation": ob })),
                Err(e) => err(id, -32001, &e.to_string()),
            },
            "execute" => {
                let Some(x) = args.get("x").and_then(|v| v.as_i64()) else {
                    return err(id, -32602, "execute requires integer x");
                };
                let Some(y) = args.get("y").and_then(|v| v.as_i64()) else {
                    return err(id, -32602, "execute requires integer y");
                };
                let ob = match self.core.observe() {
                    Ok(ob) => ob,
                    Err(e) => return err(id, -32001, &e.to_string()),
                };
                let action = Action {
                    id: format!("a_{id}").into(),
                    session_id: ob.session_id.clone(),
                    observation_id: ob.observation_id,
                    kind: ActionKind::Primitive(PrimitiveAction::Click { x, y }),
                };
                match self.core.gate(&action, &ob) {
                    Ok(target) => match self.core.execute(&action, &target) {
                        Ok(completion) => ok(
                            id,
                            serde_json::json!({
                                "result": self.core.result(completion)
                            }),
                        ),
                        Err(e) => err(id, -32002, &e.to_string()),
                    },
                    Err(e) => err(id, -32003, &e.to_string()),
                }
            }
            "keypress" => {
                let Some(key) = args.get("key").and_then(|v| v.as_str()) else {
                    return err(id, -32602, "keypress requires key");
                };
                let modifiers: Vec<String> = args
                    .get("modifiers")
                    .and_then(|v| v.as_array())
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default();
                let action = if modifiers.is_empty() {
                    PrimitiveAction::Keypress { key: key.into() }
                } else {
                    PrimitiveAction::Hotkey {
                        modifiers,
                        key: key.into(),
                    }
                };
                match self.execute_primitive(id, action) {
                    Ok(()) => ok(id, serde_json::json!({ "ok": true })),
                    Err(e) => err(id, -32002, &e),
                }
            }
            "type" => {
                let Some(text) = args.get("text").and_then(|v| v.as_str()) else {
                    return err(id, -32602, "type requires text");
                };
                match self.execute_primitive(id, PrimitiveAction::Type { text: text.into() }) {
                    Ok(()) => ok(id, serde_json::json!({ "ok": true })),
                    Err(e) => err(id, -32002, &e),
                }
            }
            "launch" => {
                let Some(app) = args.get("app").and_then(|v| v.as_str()) else {
                    return err(id, -32602, "launch requires app");
                };
                for action in [
                    PrimitiveAction::Hotkey {
                        modifiers: vec!["alt".into()],
                        key: "space".into(),
                    },
                    PrimitiveAction::Type { text: app.into() },
                    PrimitiveAction::Keypress {
                        key: "Return".into(),
                    },
                ] {
                    if let Err(e) = self.execute_primitive(id, action) {
                        return err(id, -32002, &e);
                    }
                }
                ok(id, serde_json::json!({ "ok": true, "app": app }))
            }
            "hud" => {
                let Some(message) = args.get("message").and_then(|v| v.as_str()) else {
                    return err(id, -32602, "hud requires message");
                };
                let output = std::process::Command::new("notify-send")
                    .args([
                        "--app-name=eyeharness",
                        "--icon=dialog-information",
                        "eyeharness",
                        message,
                    ])
                    .output();
                match output {
                    Ok(result) if result.status.success() => {
                        ok(id, serde_json::json!({ "ok": true }))
                    }
                    Ok(result) => err(id, -32004, &String::from_utf8_lossy(&result.stderr)),
                    Err(e) => err(id, -32004, &format!("notify-send unavailable: {e}")),
                }
            }
            "verify" => match self.core.observe() {
                Ok(ob) => {
                    let report = harness_verifier::run_verification(
                        None,
                        &ob,
                        &harness_verifier::ScreenChangeVerifier,
                    );
                    ok(
                        id,
                        serde_json::json!({
                            "outcome": format!("{:?}", report.outcome),
                            "screen_changed": ob.screen_changed,
                        }),
                    )
                }
                Err(e) => err(id, -32001, &e.to_string()),
            },
            "learn_record" => {
                let Some((profile, application, task, strategy)) = learning_key(&args) else {
                    return err(
                        id,
                        -32602,
                        "learn_record requires profile, application, task, and strategy",
                    );
                };
                let Some(success) = args.get("success").and_then(|v| v.as_bool()) else {
                    return err(id, -32602, "learn_record requires boolean success");
                };
                match self.learning.lock() {
                    Ok(mut store) => {
                        match store.record(profile, application, task, strategy, success) {
                            Ok(entry) => ok(
                                id,
                                serde_json::json!({
                                    "entry": entry,
                                    "approved": false,
                                    "message": "Recorded outcome; explicit learn_approve is required before lookup."
                                }),
                            ),
                            Err(e) => err(id, -32020, &e.to_string()),
                        }
                    }
                    Err(_) => err(id, -32020, "learning store lock poisoned"),
                }
            }
            "learn_approve" => {
                let Some((profile, application, task, strategy)) = learning_key(&args) else {
                    return err(
                        id,
                        -32602,
                        "learn_approve requires profile, application, task, and strategy",
                    );
                };
                let Some(approved) = args.get("approved").and_then(|v| v.as_bool()) else {
                    return err(id, -32602, "learn_approve requires boolean approved");
                };
                match self.learning.lock() {
                    Ok(mut store) => {
                        match store.approve(profile, application, task, strategy, approved) {
                            Ok(entry) => ok(id, serde_json::json!({ "entry": entry })),
                            Err(e) => err(id, -32020, &e.to_string()),
                        }
                    }
                    Err(_) => err(id, -32020, "learning store lock poisoned"),
                }
            }
            "learn_lookup" => {
                let Some((profile, application, task)) = learning_lookup_key(&args) else {
                    return err(
                        id,
                        -32602,
                        "learn_lookup requires profile, application, and task",
                    );
                };
                match self.learning.lock() {
                    Ok(store) => ok(
                        id,
                        serde_json::json!({
                            "entries": store.lookup(profile, application, task)
                        }),
                    ),
                    Err(_) => err(id, -32020, "learning store lock poisoned"),
                }
            }
            "learn_export" => match self.learning.lock() {
                Ok(store) => ok(id, serde_json::json!({ "entries": store.export() })),
                Err(_) => err(id, -32020, "learning store lock poisoned"),
            },
            "learn_reset" => {
                let profile = args.get("profile").and_then(|v| v.as_str());
                match self.learning.lock() {
                    Ok(mut store) => match store.reset(profile) {
                        Ok(removed) => ok(id, serde_json::json!({ "removed": removed })),
                        Err(e) => err(id, -32020, &e.to_string()),
                    },
                    Err(_) => err(id, -32020, "learning store lock poisoned"),
                }
            }
            "adb_devices" => match harness_adb::list_devices() {
                Ok(devices) => ok(id, serde_json::json!({ "devices": devices })),
                Err(e) => err(id, -32010, &e.to_string()),
            },
            "adb_screenshot" => {
                match Self::adb_client(&args)
                    .and_then(|c| c.screenshot().map_err(|e| e.to_string()))
                {
                    Ok(path) => ok(id, serde_json::json!({ "path": path })),
                    Err(e) => err(id, -32010, &e.to_string()),
                }
            }
            "adb_tap" => {
                let Some(x) = args.get("x").and_then(|v| v.as_i64()) else {
                    return err(id, -32602, "adb_tap requires x");
                };
                let Some(y) = args.get("y").and_then(|v| v.as_i64()) else {
                    return err(id, -32602, "adb_tap requires y");
                };
                match Self::adb_client(&args)
                    .and_then(|c| Self::adb_execute(&c, PrimitiveAction::Click { x, y }))
                {
                    Ok(()) => ok(id, serde_json::json!({ "ok": true })),
                    Err(e) => err(id, -32010, &e),
                }
            }
            "adb_type" => {
                let Some(text) = args.get("text").and_then(|v| v.as_str()) else {
                    return err(id, -32602, "adb_type requires text");
                };
                match Self::adb_client(&args).and_then(|c| {
                    Self::adb_execute(&c, PrimitiveAction::Type { text: text.into() })
                }) {
                    Ok(()) => ok(id, serde_json::json!({ "ok": true })),
                    Err(e) => err(id, -32010, &e),
                }
            }
            "adb_key" => {
                let Some(key) = args.get("key").and_then(|v| v.as_str()) else {
                    return err(id, -32602, "adb_key requires key");
                };
                match Self::adb_client(&args).and_then(|c| {
                    Self::adb_execute(&c, PrimitiveAction::Keypress { key: key.into() })
                }) {
                    Ok(()) => ok(id, serde_json::json!({ "ok": true })),
                    Err(e) => err(id, -32010, &e),
                }
            }
            "adb_launch" => {
                let Some(package) = args.get("package").and_then(|v| v.as_str()) else {
                    return err(id, -32602, "adb_launch requires package");
                };
                if !package.split('.').all(|part| {
                    !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                }) {
                    return err(id, -32602, "invalid Android package name");
                }
                match Self::adb_client(&args).and_then(|c| {
                    c.command(&["shell", "monkey", "-p", package, "1"])
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                }) {
                    Ok(()) => ok(id, serde_json::json!({ "ok": true, "package": package })),
                    Err(e) => err(id, -32010, &e.to_string()),
                }
            }
            "adb_current_app" => match Self::adb_client(&args)
                .and_then(|c| c.current_window().map_err(|e| e.to_string()))
            {
                Ok(window) => ok(id, serde_json::json!({ "window": window })),
                Err(e) => err(id, -32010, &e.to_string()),
            },
            _ => err(id, -32602, "unknown tool"),
        }
    }

    fn execute_primitive(&self, id: u64, primitive: PrimitiveAction) -> Result<(), String> {
        let ob = self.core.observe().map_err(|e| e.to_string())?;
        let action = Action {
            id: format!("a_{id}").into(),
            session_id: ob.session_id.clone(),
            observation_id: ob.observation_id,
            kind: ActionKind::Primitive(primitive),
        };
        let target = self.core.gate(&action, &ob).map_err(|e| e.to_string())?;
        self.core
            .execute(&action, &target)
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn adb_client(args: &serde_json::Value) -> Result<harness_adb::AdbClient, String> {
        let serial = args
            .get("serial")
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .or_else(|| std::env::var("EYEHARNESS_ADB_SERIAL").ok())
            .or_else(|| {
                harness_adb::list_devices()
                    .ok()?
                    .into_iter()
                    .find(|d| d.state == "device")
                    .map(|d| d.serial)
            })
            .ok_or_else(|| {
                "no online Android device; provide serial or set EYEHARNESS_ADB_SERIAL".to_string()
            })?;
        Ok(harness_adb::AdbClient::new(serial))
    }

    fn adb_execute(client: &harness_adb::AdbClient, action: PrimitiveAction) -> Result<(), String> {
        use harness_core::Executor;
        harness_adb::AdbExecutor::new(client.clone()).execute(&action)
    }
}

fn ok(id: u64, result: serde_json::Value) -> String {
    serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "result": result
    }))
    .unwrap_or_else(|_| "{\"jsonrpc\":\"2.0\",\"id\":0}".into())
}

fn err(id: u64, code: i64, message: &str) -> String {
    serde_json::to_string(&serde_json::json!({
        "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message }
    }))
    .unwrap_or_else(|_| "{\"jsonrpc\":\"2.0\",\"id\":0}".into())
}

fn learning_key(args: &serde_json::Value) -> Option<(&str, &str, &str, &str)> {
    Some((
        args.get("profile")?.as_str()?,
        args.get("application")?.as_str()?,
        args.get("task")?.as_str()?,
        args.get("strategy")?.as_str()?,
    ))
}

fn learning_lookup_key(args: &serde_json::Value) -> Option<(&str, &str, &str)> {
    Some((
        args.get("profile")?.as_str()?,
        args.get("application")?.as_str()?,
        args.get("task")?.as_str()?,
    ))
}

fn audit_path() -> PathBuf {
    std::env::var_os("EYEHARNESS_AUDIT_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("XDG_STATE_HOME")
                .map(PathBuf::from)
                .map(|path| path.join("eyeharness/audit.jsonl"))
        })
        .unwrap_or_else(|| PathBuf::from(".eyeharness/audit.jsonl"))
}

fn main() {
    let server = match McpServer::new() {
        Ok(server) => server,
        Err(error) => {
            eprintln!("failed to initialize eyeharness: {error}");
            std::process::exit(1);
        }
    };
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if writeln!(out, "{}", server.handle(&line)).is_err() {
            break;
        }
        if out.flush().is_err() {
            break;
        }
    }
}
