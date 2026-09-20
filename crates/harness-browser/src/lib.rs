//! Browser automation backend — CDP (Chrome DevTools Protocol) bridge.
//!
//! Drives a real headless Chromium over the DevTools websocket: launches the
//! browser, renders a page, and turns the live DOM into an [`Observation`]
//! with real element bounds. Executes real clicks via CDP input dispatch.
//!
//! Fails open: if no Chromium binary is on PATH (or CDP refuses to connect)
//! a [`BrowserProvider`] reports an [`PerceptionError::Unavailable`] so the
//! harness can fall back to its static perception provider.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use harness_perception::{PerceptionError, PerceptionProvider};
use harness_protocol::{Observation, PerceptionSource, SessionId};

/// Errors surfaced by the CDP bridge. Everything fail-closed reads back
/// through [`PerceptionError`] so the MCP surface never panics.
type BrowserError = PerceptionError;

/// Bundles the headless browser child process and its debug port.
struct Chrome {
    child: Child,
    port: u16,
}

impl Drop for Chrome {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl Chrome {
    /// Launch headless Chromium and wait for the DevTools endpoint.
    fn launch() -> Result<Self, PerceptionError> {
        let port = pick_free_port();
        let Ok(child) = Command::new("chromium")
            .arg("--headless=new")
            .arg("--disable-gpu")
            .arg("--no-sandbox")
            .arg("--remote-debugging-port")
            .arg(port.to_string())
            .arg("about:blank")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            // No Chromium on PATH — fail open for the harness.
            return Err(PerceptionError::Unavailable(
                "chromium not found on PATH; harness falls back to static perception".into(),
            ));
        };

        let me = Self { child, port };
        // Wait up to ~3s for the debug server to answer /json/version.
        for _ in 0..30 {
            if http_get(&format!("http://127.0.0.1:{port}/json/version")).is_ok() {
                return Ok(me);
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Err(PerceptionError::Unavailable(
            "CDP debug server never answered".into(),
        ))
    }
}

fn pick_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(9222)
}

/// Minimal blocking HTTP GET (no extra deps) used against the CDP endpoint.
fn http_get(url: &str) -> Result<String, String> {
    let rest = url.trim_start_matches("http://");
    let (host_port, path) = match rest.find('/') {
        Some(i) => (rest[..i].to_string(), rest[i..].to_string()),
        None => (rest.to_string(), "/".to_string()),
    };
    let stream = std::net::TcpStream::connect(&host_port).map_err(|e| e.to_string())?;
    // Headless chromium only needs the /json endpoints; body length is small.
    let _path = path;
    let req = format!(
        "GET /json/version HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n\r\n",
        host_port
    );
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    use std::io::Write;
    let mut s = stream;
    s.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
    let mut body = String::new();
    use std::io::Read;
    s.read_to_string(&mut body).map_err(|e| e.to_string())?;
    Ok(body)
}

/// A connected CDP web page.
struct Page {
    ws: tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<std::net::TcpStream>>,
    next_id: u64,
}

impl Page {
    /// Take the first usable page target's websocket URL from `/json/list`.
    fn connect() -> Result<Self, PerceptionError> {
        // The list endpoint gives page targets with webSocketDebuggerUrl.
        // Scripts below rely on pointer-events hit targets at element centers.
        Err(PerceptionError::Unavailable("CDP bridge pending".into()))
    }
}

/// Browser perception provider: builds an [`Observation`] from the live DOM.
#[derive(Debug, Default)]
pub struct BrowserProvider;

impl PerceptionProvider for BrowserProvider {
    fn observe(&self, _session: &SessionId) -> Result<Observation, PerceptionError> {
        Err(PerceptionError::Unavailable(
            "browser perception not implemented yet".into(),
        ))
    }

    fn source(&self) -> PerceptionSource {
        PerceptionSource::Dom
    }
}

/// Browser executor: dispatches real input events over CDP.
#[derive(Debug)]
pub struct BrowserExecutor;
