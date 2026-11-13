//! The smallest MCP client: the tribe's view, reached with the connector's
//! own device key (ADR-0398 D13).
//!
//! A connected connector's device key is served one surface — its tribe's
//! tools and commands on the door's `/mcp` — whatever `?mode=` it asks for,
//! and always on the connector's own tribe. This client speaks just enough
//! Streamable HTTP MCP for that: one `initialize`, the `initialized`
//! notification, then [`View::call`] for each tool. It is synchronous, as
//! the producer is, and one connection per message as the door closes
//! each reply.
//!
//! The view is on the door's ingest listener, which a socket door does not
//! serve; a producer that reaches Wild only by Unix socket has no view, and
//! that is said in words rather than as a connection error.
//!
//! ```no_run
//! # fn run(door: wild_kit::producer::door::Door, key: wild_kit::producer::calls::Key) {
//! let view = wild_kit::producer::mcp::View::of(door, key);
//! match view.call("task_claim", serde_json::json!({"task_id": "t-1"})) {
//!     Ok(answer) => println!("claimed: {answer}"),
//!     Err(reason) => println!("the tribe refused it: {reason}"),
//! }
//! # }
//! ```

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};

use super::calls::Key;
use super::door::Door;
use super::http::{self, Reply};

/// The MCP revision this client speaks.
const PROTOCOL: &str = "2025-06-18";
/// The header a Streamable-HTTP MCP session is carried in.
const SESSION: &str = "mcp-session-id";
/// Where the tribe's view is served: the ingest door's listener, never the
/// Unix socket.
pub const PATH: &str = "/mcp";
/// How long one call waits: a command runs before it answers, and a call
/// made on a busy machine is slower than the door's default 10 s.
const READ: Duration = Duration::from_secs(60);

/// A tribe's view as one connector's device key reaches it.
#[derive(Clone)]
pub struct View {
    door: Door,
    key: Key,
    /// The session the door opened at the first `initialize`, shared by
    /// every clone: one session, one surface.
    session: Arc<Mutex<Option<String>>>,
    id: Arc<AtomicI64>,
}

impl View {
    /// The view of `door`, called with `key`.
    pub fn of(door: Door, key: Key) -> Self {
        Self {
            door,
            key,
            session: Arc::new(Mutex::new(None)),
            id: Arc::new(AtomicI64::new(0)),
        }
    }

    /// Where the view is, as the operator hears it when it cannot be reached.
    pub fn door(&self) -> &Door {
        &self.door
    }

    /// Call `tool` with `arguments` on the tribe's view: what it answered,
    /// or why the tribe refused it — the door's own words, never code.
    pub fn call(&self, tool: &str, arguments: Value) -> Result<Value, String> {
        self.open()?;
        let answer = self.post(&json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/call",
            "params": {"name": tool, "arguments": arguments},
        }))?;
        if let Some(error) = answer.get("error") {
            return Err(why(error));
        }
        let result = answer.get("result").cloned().unwrap_or(Value::Null);
        if result["isError"].as_bool() == Some(true) {
            let said = text_of(&result);
            return Err(if said.is_empty() {
                format!("the tribe refused `{tool}`")
            } else {
                said
            });
        }
        Ok(result)
    }

    /// The tools the tribe's view offers this connector, as the door lists
    /// them — the one answer a connector can check its own view against.
    pub fn tools(&self) -> Result<Vec<String>, String> {
        self.open()?;
        let answer = self.post(&json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "tools/list",
            "params": {},
        }))?;
        if let Some(error) = answer.get("error") {
            return Err(why(error));
        }
        Ok(answer["result"]["tools"]
            .as_array()
            .map(|tools| {
                tools
                    .iter()
                    .filter_map(|t| t["name"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Open the session once: `initialize`, then the `initialized`
    /// notification. Every later message carries the session it names.
    fn open(&self) -> Result<(), String> {
        if self.session.lock().map(|s| s.is_some()).unwrap_or(false) {
            return Ok(());
        }
        let answer = self.post(&json!({
            "jsonrpc": "2.0",
            "id": self.next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": PROTOCOL,
                "capabilities": {},
                "clientInfo": {
                    "name": "wild-kit",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            },
        }))?;
        if let Some(error) = answer.get("error") {
            return Err(why(error));
        }
        self.post(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))?;
        Ok(())
    }

    fn next_id(&self) -> i64 {
        self.id.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// One JSON-RPC message to the door, keeping the session its reply
    /// names; the reply's JSON, or why the door did not take it.
    fn post(&self, message: &Value) -> Result<Value, String> {
        let body = serde_json::to_vec(message).map_err(|e| e.to_string())?;
        let auth = format!("Bearer {}", self.key.get());
        let session = self
            .session
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone();
        let mut headers = vec![
            ("authorization", auth.as_str()),
            ("content-type", "application/json"),
            ("accept", "application/json"),
        ];
        if let Some(session) = &session {
            headers.push((SESSION, session.as_str()));
        }
        let reply = http::reply_within(&self.door, "POST", PATH, &headers, &body, Some(READ))
            .map_err(|e| format!("the tribe's view could not be reached: {e}"))?;
        if let Some(session) = reply.header(SESSION) {
            if let Ok(mut kept) = self.session.lock() {
                *kept = Some(session.to_string());
            }
        }
        match reply.status {
            // A notification takes no answer, and a refused one says it in
            // the next reply that does.
            202 | 204 => Ok(Value::Null),
            200 => Ok(reply.body),
            _ => Err(format!("{} ({})", why_reply(&reply), reply.status)),
        }
    }
}

/// The reason an error object carries.
fn why(error: &Value) -> String {
    let said = error["message"]
        .as_str()
        .or_else(|| error["error"].as_str())
        .map(str::to_string)
        .unwrap_or_default();
    if said.trim().is_empty() {
        error.to_string()
    } else {
        said
    }
}

/// The words a non-200 reply carries: its JSON error, or its text.
fn why_reply(reply: &Reply) -> String {
    let said = reply.body["error"]["message"]
        .as_str()
        .or_else(|| reply.body["error"].as_str())
        .map(str::to_string)
        .unwrap_or_else(|| reply.text.trim().to_string());
    if said.trim().is_empty() {
        "Wild answered nothing".to_string()
    } else {
        said
    }
}

/// The words a refused tool result leads with. The door puts the sentence a
/// person reads first; its machine detail rides beside it (a refusal the walk
/// recorded carries `{code, detail, params}` as a second block), and that is
/// not repeated to the operator.
fn text_of(result: &Value) -> String {
    result["content"]
        .as_array()
        .and_then(|blocks| blocks.first())
        .and_then(|b| b["text"].as_str())
        .unwrap_or_default()
        .to_string()
}
