//! Answering the calls Wild makes on a connector's tools (ADR-0398 D4, D6).
//!
//! A connector that announced `tools:` holds a **call wait** on the door
//! ([`serve`]): the door answers it with the calls queued for the device, or
//! with none once its hold (up to 25 s) ran out, and the device opens the
//! next one at once. Each call goes to the handler of the tool it names
//! ([`Tools::with`]), and its answer goes back on the answer route. A door
//! that does not answer is asked again after a pause that doubles up to
//! [`BACKOFF_MAX`].
//!
//! A call no handler takes is answered [`Answer::Refused`] and runs nothing
//! (D6): `refused` is an answer, never a dropped call, so the caller's
//! command reads it with its `failure_when` and its record leaves its
//! pending state. On the wire a refusal is `{"outcome": "refused", "reason":
//! "<why, in words>"}` ([`REFUSED`]); what a handler answers otherwise is
//! the tool's own JSON.
//!
//! A call is repeat-safe (D5): every `idempotency_key` [`Tools::answer`]
//! answered is kept for [`KEEP_ANSWERED`] in the state directory `Tools`
//! was opened on. A repeat — the same key with the same arguments — is
//! answered with the first answer and runs nothing; the same key with other
//! arguments is [`Answer::Refused`], saying the key was used for another
//! call. The call is kept as started before its handler runs and as
//! answered before the answer is sent, so a crash on either side of the
//! hand-over runs it no second time.
//!
//! A connector's developer writes one handler per tool and nothing else:
//!
//! ```no_run
//! use std::sync::atomic::AtomicBool;
//! use wild_kit::producer::calls::{self, Answer, Tools};
//! # fn run(door: &wild_kit::producer::door::Door, decl: wild_ingest_contract::tools::ToolDecl,
//! #        state: &std::path::Path) -> std::io::Result<()> {
//! let mut tools = Tools::new(state)?.with(decl, |args| match args["host"].as_str() {
//!     Some(host) => Answer::Done(serde_json::json!({"outcome": "started", "host": host})),
//!     None => Answer::Refused("the call names no host".into()),
//! });
//! let key = calls::Key::new("the producer key".into());
//! calls::serve(door, "/api/v1/tribes/t/producers/p", &key, &mut tools, &AtomicBool::new(false));
//! # Ok(())
//! # }
//! ```

use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use wild_ingest_contract::codes::RefusalCode;
use wild_ingest_contract::tools::{
    CallAnswer, Calls, ToolCall, ToolDecl, CALL_WAIT_HOLD_SECS, IDEMPOTENCY_KEY,
};

use super::door::Door;
use super::http::{code, request_within};
use super::{printable, write_owner_only};

/// The `outcome` of an answer that refused its call.
pub const REFUSED: &str = "refused";

/// The first pause after the door did not answer a call wait.
pub const BACKOFF_FIRST: Duration = Duration::from_millis(200);
/// The longest pause between two call waits the door did not answer.
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// How many times an answer is sent before it is given up: past that the
/// caller hears nothing and its call ends unknown (D5).
pub const ANSWER_TRIES: usize = 3;

/// How long a call wait's reply is waited for: the door's hold and a margin.
fn wait_read() -> Duration {
    Duration::from_secs(CALL_WAIT_HOLD_SECS + 10)
}

/// What a handler answers a call.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    /// What the call did, as JSON of the tool's own shape.
    Done(Value),
    /// The connector does not take this call; nothing ran. The reason says
    /// why in words the operator reads.
    Refused(String),
}

impl Answer {
    /// The answer as it goes on the wire.
    pub fn into_result(self) -> Value {
        match self {
            Answer::Done(v) => v,
            Answer::Refused(reason) => json!({ "outcome": REFUSED, "reason": reason }),
        }
    }
}

/// How long an answered key is kept: a week (ADR-0398 D5).
pub const KEEP_ANSWERED: Duration = Duration::from_secs(7 * 86_400);

/// The store's file, in the state directory: one line per start and one per
/// answer, so a crash never has to rewrite what it wrote.
const ANSWERED: &str = "calls-answered.jsonl";

/// What an `idempotency_key` asks for (ADR-0398 D5).
#[derive(Debug, PartialEq)]
pub enum Asked {
    /// Answered under this key with these arguments before: this answer,
    /// and nothing runs.
    Same(Value),
    /// A call under this key was kept as started and never answered — the
    /// connector died in it. It runs nothing: a second run could double
    /// what the first one did.
    Pending,
    /// The key was answered with other arguments: refuse, run nothing.
    Other,
    /// The key is new: run, then keep its answer.
    New,
}

/// One line of the answered-key store.
#[derive(Debug, Serialize, Deserialize)]
struct Line {
    /// The function the call named: a key names one call, so the same key on
    /// another function is another call — never a repeat of this one.
    tool: String,
    key: String,
    arguments: Value,
    #[serde(default)]
    result: Option<Value>,
    at: u64,
}

/// The latest line of one key, in memory.
#[derive(Debug, Clone)]
struct Kept {
    tool: String,
    arguments: Value,
    result: Option<Value>,
    at: u64,
}

/// The answers this connector gave, by the call's `idempotency_key`, kept
/// for [`KEEP_ANSWERED`] in `<state>/calls-answered.jsonl` (D5). A call is
/// kept as started before its handler runs and as answered before the
/// answer is sent.
#[derive(Debug)]
pub struct Answers {
    path: PathBuf,
    kept: HashMap<String, Kept>,
}

impl Answers {
    /// Read the store in `state`, dropping what is older than
    /// [`KEEP_ANSWERED`] and folding each key's lines into its latest.
    pub fn open(state: impl AsRef<Path>) -> io::Result<Self> {
        Self::open_at(state, now_secs())
    }

    /// [`Answers::open`] as of `now`, in unix seconds.
    pub fn open_at(state: impl AsRef<Path>, now: u64) -> io::Result<Self> {
        let path = state.as_ref().join(ANSWERED);
        let mut kept: HashMap<String, Kept> = HashMap::new();
        let mut compact = false;
        match fs::read_to_string(&path) {
            Ok(text) => {
                let mut lines = text.lines().enumerate().peekable();
                while let Some((i, line)) = lines.next() {
                    let line = match serde_json::from_str::<Line>(line) {
                        Ok(line) => line,
                        // A line torn by a crash is the last one, and it
                        // records a call that had not started yet.
                        Err(_) if lines.peek().is_none() => {
                            compact = true;
                            break;
                        }
                        Err(e) => {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidData,
                                format!("{}: line {}: {e}", path.display(), i + 1),
                            ))
                        }
                    };
                    if !within(line.at, now) {
                        compact = true;
                        continue;
                    }
                    kept.insert(
                        line.key,
                        Kept {
                            tool: line.tool,
                            arguments: line.arguments,
                            result: line.result,
                            at: line.at,
                        },
                    );
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let answers = Self { path, kept };
        if compact {
            answers.rewrite()?;
        }
        Ok(answers)
    }

    /// What a call under `key` on `tool` with `arguments` asks for.
    pub fn asked(&self, key: &str, tool: &str, arguments: &Value) -> Asked {
        self.asked_at(key, tool, arguments, now_secs())
    }

    /// [`Answers::asked`] as of `now`, in unix seconds.
    pub fn asked_at(&self, key: &str, tool: &str, arguments: &Value, now: u64) -> Asked {
        let Some(kept) = self.kept.get(key).filter(|k| within(k.at, now)) else {
            return Asked::New;
        };
        if kept.tool != tool || kept.arguments != *arguments {
            Asked::Other
        } else {
            match &kept.result {
                Some(result) => Asked::Same(result.clone()),
                None => Asked::Pending,
            }
        }
    }

    /// Keep that a call under `key` on `tool` with `arguments` started,
    /// before its handler runs.
    pub fn begin(&mut self, key: &str, tool: &str, arguments: &Value) -> io::Result<()> {
        self.record(key, tool, arguments, None)
    }

    /// Keep `result` as the answer of `tool`'s call under `key`, before it is
    /// sent.
    pub fn keep(
        &mut self,
        key: &str,
        tool: &str,
        arguments: &Value,
        result: &Value,
    ) -> io::Result<()> {
        self.record(key, tool, arguments, Some(result.clone()))
    }

    fn record(
        &mut self,
        key: &str,
        tool: &str,
        arguments: &Value,
        result: Option<Value>,
    ) -> io::Result<()> {
        let at = now_secs();
        let line = Line {
            tool: tool.to_string(),
            key: key.to_string(),
            arguments: arguments.clone(),
            result: result.clone(),
            at,
        };
        let mut text = serde_json::to_string(&line).map_err(io::Error::other)?;
        text.push('\n');
        let mut open = fs::OpenOptions::new();
        open.append(true).create(true);
        // The answers carry what the tools returned: this user's alone.
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut open, 0o600);
        let mut file = open.open(&self.path)?;
        // The mode above is a new file's; one already there is tightened.
        #[cfg(unix)]
        file.set_permissions(std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        file.write_all(text.as_bytes())?;
        file.sync_data()?;
        self.kept.insert(
            key.to_string(),
            Kept {
                tool: tool.to_string(),
                arguments: arguments.clone(),
                result,
                at,
            },
        );
        Ok(())
    }

    /// Write the store back whole, oldest first.
    fn rewrite(&self) -> io::Result<()> {
        let mut kept: Vec<(&String, &Kept)> = self.kept.iter().collect();
        kept.sort_by(|a, b| a.1.at.cmp(&b.1.at).then_with(|| a.0.cmp(b.0)));
        let mut text = String::new();
        for (key, k) in kept {
            let line = Line {
                tool: k.tool.clone(),
                key: key.clone(),
                arguments: k.arguments.clone(),
                result: k.result.clone(),
                at: k.at,
            };
            text.push_str(&serde_json::to_string(&line).map_err(io::Error::other)?);
            text.push('\n');
        }
        write_owner_only(&self.path, text.as_bytes())
    }
}

/// Whether a line kept at `at` is still inside [`KEEP_ANSWERED`] at `now`.
fn within(at: u64, now: u64) -> bool {
    now.saturating_sub(at) < KEEP_ANSWERED.as_secs()
}

type Handler = Box<dyn FnMut(&Value) -> Answer + Send>;

/// What answers the calls the door hands out: the connector's [`Tools`], or a
/// device that makes one of the mistakes `wild-kit run` must catch. The kit
/// measures a device through this and nothing else, so the reference
/// producer's `--break` can hand it a deliberately broken one.
pub trait Answering {
    /// Answer one call, naming the call it answers.
    fn answer(&mut self, call: &ToolCall) -> CallAnswer;
}

impl Answering for Tools {
    fn answer(&mut self, call: &ToolCall) -> CallAnswer {
        Tools::answer(self, call)
    }
}

/// The tools a connector offers, each with the handler that answers it, and
/// the answers it already gave (D5).
pub struct Tools {
    tools: Vec<(ToolDecl, Handler)>,
    answers: Answers,
}

impl Tools {
    /// The tools of a connector whose state directory is `state` — where
    /// the keys it answered are read from and kept.
    pub fn new(state: impl AsRef<Path>) -> io::Result<Self> {
        Ok(Self {
            tools: Vec::new(),
            answers: Answers::open(state)?,
        })
    }

    /// Offer `decl`, answered by `handler`. The handler gets the call's
    /// arguments, checked by Wild against the tool's schema.
    #[must_use]
    pub fn with(
        mut self,
        decl: ToolDecl,
        handler: impl FnMut(&Value) -> Answer + Send + 'static,
    ) -> Self {
        self.tools.retain(|(d, _)| d.name != decl.name);
        self.tools.push((decl, Box::new(handler)));
        self
    }

    /// The tools as the manifest announces them (`tools:`).
    pub fn announced(&self) -> Vec<ToolDecl> {
        self.tools.iter().map(|(d, _)| d.clone()).collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// Answer one call: by the handler of the tool it names, or refused when
    /// no handler takes that tool. Kept under the call's `idempotency_key`,
    /// so a repeat runs nothing (D5).
    pub fn answer(&mut self, call: &ToolCall) -> CallAnswer {
        let result = self.result(call);
        CallAnswer {
            call: call.call.clone(),
            result,
        }
    }

    fn result(&mut self, call: &ToolCall) -> Value {
        let Some(key) = call.arguments.get(IDEMPOTENCY_KEY).and_then(Value::as_str) else {
            // Without a key a repeat cannot be recognised: answer it, keep
            // nothing.
            return self.dispatch(call).into_result();
        };
        match self.answers.asked(key, &call.tool, &call.arguments) {
            Asked::Same(result) => {
                say(&format!(
                    "call {}: a repeat under key `{key}`, answered as before",
                    call.call
                ));
                result
            }
            Asked::Pending => Answer::Refused(format!(
                "the idempotency key `{key}` belongs to a call this connector started \
                 and never answered, so what it began may already have happened; this \
                 repeat is refused rather than run a second time"
            ))
            .into_result(),
            Asked::Other => Answer::Refused(format!(
                "the idempotency key `{key}` was used for another call; this call runs nothing"
            ))
            .into_result(),
            // Kept as started before the handler runs: a crash inside it
            // leaves a repeat refused, never run twice.
            Asked::New => match self.answers.begin(key, &call.tool, &call.arguments) {
                Err(e) => Answer::Refused(format!(
                    "this connector could not keep the call repeat-safe ({e}); it runs nothing"
                ))
                .into_result(),
                Ok(()) => {
                    let result = self.dispatch(call).into_result();
                    if let Err(e) = self.answers.keep(key, &call.tool, &call.arguments, &result) {
                        say(&format!(
                            "call {}: its answer could not be kept ({e}); a repeat runs nothing",
                            call.call
                        ));
                    }
                    result
                }
            },
        }
    }

    /// The handler of the tool the call names, or refused when none does.
    fn dispatch(&mut self, call: &ToolCall) -> Answer {
        match self.tools.iter_mut().find(|(d, _)| d.name == call.tool) {
            Some((_, handler)) => handler(&call.arguments),
            None => Answer::Refused(format!(
                "this connector offers no function `{}`; it runs nothing it did not announce",
                call.tool
            )),
        }
    }
}

/// The producer key the waits and answers present. Shared with whoever
/// sends the batches, which takes a `rotate_key`: the next wait presents the
/// new one.
#[derive(Clone, Default)]
pub struct Key(Arc<Mutex<String>>);

/// A key is a secret: its debug form never shows it.
impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Key(…)")
    }
}

impl Key {
    pub fn new(key: String) -> Self {
        Self(Arc::new(Mutex::new(key)))
    }

    pub fn get(&self) -> String {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set(&self, key: String) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = key;
    }
}

/// Why [`serve`] stopped holding call waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ended {
    /// The door calls no tool of this device (`409 no_tools`): it holds no
    /// call queue, or the device's manifest announced none. Nothing will be
    /// called, so no wait is held.
    NoTools,
    /// The door no longer opens to the key: the device was cut off.
    KeyRefused,
    /// The door has no call route at all (a `404` that is no refusal of the
    /// contract's): a Wild older than the call half. Nothing will be
    /// called, so no wait is held.
    NoCallRoute,
    /// `stop` was set.
    Stopped,
}

/// What one call wait came back with.
enum Waited {
    Calls(Vec<ToolCall>),
    Ended(Ended),
    /// No usable reply: wait this long (a `Retry-After`, or the back-off).
    Again(Option<Duration>),
}

/// Hold call waits on the door at `base` (`/api/v1/tribes/{tribe}/producers/
/// {producer}`) until the door says no call will come or `stop` is set:
/// answer every call handed out by the device in `tools`, then open the next
/// wait at once — after an answer and after a wait that ran out alike. A wait
/// the door did not answer is opened again after a pause that doubles from
/// [`BACKOFF_FIRST`] up to [`BACKOFF_MAX`], and starts over once it answers.
pub fn serve(
    door: &Door,
    base: &str,
    key: &Key,
    tools: &mut dyn Answering,
    stop: &AtomicBool,
) -> Ended {
    let mut backoff = BACKOFF_FIRST;
    while !stop.load(Ordering::SeqCst) {
        match wait_once(door, base, key) {
            Waited::Calls(calls) => {
                backoff = BACKOFF_FIRST;
                for call in calls {
                    let answer = tools.answer(&call);
                    say(&format!(
                        "call {}: {} answered {}",
                        call.call,
                        call.tool,
                        answer.result["outcome"].as_str().unwrap_or("")
                    ));
                    send_answer(door, base, key, &answer);
                }
            }
            Waited::Ended(ended) => return ended,
            Waited::Again(after) => {
                std::thread::sleep(after.unwrap_or(backoff));
                backoff = (backoff * 2).min(BACKOFF_MAX);
            }
        }
    }
    Ended::Stopped
}

fn wait_once(door: &Door, base: &str, key: &Key) -> Waited {
    let presented = key.get();
    let auth = format!("Bearer {presented}");
    let path = format!("{base}/calls/wait");
    let headers = [("authorization", auth.as_str())];
    match request_within(door, "POST", &path, &headers, &[], Some(wait_read())) {
        Ok((200, _, reply)) => match serde_json::from_value::<Calls>(reply) {
            Ok(c) => Waited::Calls(c.calls),
            Err(_) => Waited::Again(None),
        },
        Ok((409, _, reply)) if code(&reply) == Some(RefusalCode::NoTools) => {
            Waited::Ended(Ended::NoTools)
        }
        Ok((404, _, reply)) if code(&reply).is_none() => Waited::Ended(Ended::NoCallRoute),
        // A key rotated while the wait was out: the next wait presents it.
        Ok((401, _, _)) if key.get() != presented => Waited::Calls(Vec::new()),
        Ok((401, _, _)) => Waited::Ended(Ended::KeyRefused),
        Ok((_, retry_after, _)) => Waited::Again(retry_after),
        Err(_) => Waited::Again(None),
    }
}

/// Send one answer. `404 no_such_call` means Wild stopped waiting for it:
/// it is dropped. Any other reply but `202` sends it again, up to
/// [`ANSWER_TRIES`] times in all.
fn send_answer(door: &Door, base: &str, key: &Key, answer: &CallAnswer) {
    let path = format!("{base}/calls/answers");
    let body = serde_json::to_vec(answer).unwrap_or_default();
    for _ in 0..ANSWER_TRIES {
        let auth = format!("Bearer {}", key.get());
        let headers = [
            ("authorization", auth.as_str()),
            ("content-type", "application/json"),
        ];
        match request_within(door, "POST", &path, &headers, &body, None) {
            Ok((202, _, _)) => return,
            Ok((404, _, reply)) if code(&reply) == Some(RefusalCode::NoSuchCall) => {
                say(&format!(
                    "call {}: Wild no longer waits for it",
                    answer.call
                ));
                return;
            }
            Ok((status, _, _)) => say(&format!("call {}: answer refused {status}", answer.call)),
            Err(e) => say(&format!("call {}: answer not delivered ({e})", answer.call)),
        }
        std::thread::sleep(BACKOFF_FIRST);
    }
}

/// Print one line of what happened. The call's id, its tool, its key and
/// what a tool answered come from the door or the tool: a control character
/// among them is shown as `\u{FFFD}`, so no text from outside can move the
/// cursor, recolour or retitle the terminal this runs in.
fn say(line: &str) {
    println!("{}", printable(line));
}

#[cfg(test)]
#[path = "calls_tests.rs"]
mod tests;

/// Seconds since the Unix epoch.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
