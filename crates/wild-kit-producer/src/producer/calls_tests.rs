//! The call half against a scripted door on a Unix socket: each request the
//! producer makes is noted, and answered with the next reply of the script.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use wild_ingest_contract::tools::{ToolActs, ToolDecl};

use super::*;
use crate::producer::door;

/// One request the door was sent: its path, the key it presented and its
/// JSON body.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    key: String,
    body: Value,
}

/// A door that answers each call wait with the next `(status, body)` of its
/// script, and `409 no_tools` once the script ran out; each answer with the
/// next of `answers`, and `202` once those ran out. Before it answers a
/// wait `401`, it sets `rotate` to `k2`, as a `rotate_key` taken meanwhile
/// would.
struct Scripted {
    seen: Arc<Mutex<Vec<Seen>>>,
    _dir: tempfile::TempDir,
    door: Door,
}

fn scripted(script: Vec<(u16, Value)>) -> Scripted {
    scripted_with(script, Vec::new(), None)
}

fn scripted_with(
    script: Vec<(u16, Value)>,
    answers: Vec<(u16, Value)>,
    rotate: Option<Key>,
) -> Scripted {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("door.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let noted = Arc::clone(&seen);
    std::thread::spawn(move || {
        let mut script: VecDeque<(u16, Value)> = script.into();
        let mut answers: VecDeque<(u16, Value)> = answers.into();
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let path = line.split_whitespace().nth(1).unwrap_or("").to_string();
            let mut length = 0;
            let mut key = String::new();
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    if k.eq_ignore_ascii_case("content-length") {
                        length = v.trim().parse().unwrap();
                    }
                    if k.eq_ignore_ascii_case("authorization") {
                        key = v.trim().trim_start_matches("Bearer ").to_string();
                    }
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
            noted.lock().unwrap().push(Seen {
                path: path.clone(),
                key,
                body,
            });
            let (status, reply) = if path.ends_with("/calls/answers") {
                answers.pop_front().unwrap_or((202, Value::Null))
            } else {
                script
                    .pop_front()
                    .unwrap_or((409, json!({"code": "no_tools"})))
            };
            if status == 401 {
                if let Some(k) = &rotate {
                    k.set("k2".into());
                }
            }
            let text = reply.to_string();
            let mut stream = reader.into_inner();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{text}",
                text.len()
            );
        }
    });
    let door = door::socket(path.to_str().unwrap()).unwrap();
    Scripted {
        seen,
        _dir: dir,
        door,
    }
}

impl Scripted {
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }
}

fn decl(name: &str) -> ToolDecl {
    ToolDecl {
        name: name.into(),
        description: "Starts an agent on one host of the fleet".into(),
        schema: json!({"type": "object", "properties": {
            "host": {"type": "string"}, "idempotency_key": {"type": "string"}
        }}),
        acts: Some(ToolActs::Outward),
        risk: None,
        routes_by: None,
    }
}

fn call(id: &str, tool: &str, arguments: Value) -> Value {
    json!({"call": id, "tool": tool, "arguments": arguments})
}

const BASE: &str = "/api/v1/tribes/t/producers/p";

fn serve_on(door: &Scripted, tools: &mut Tools) -> Ended {
    serve_with(door, &Key::new("k".into()), tools)
}

fn serve_with(door: &Scripted, key: &Key, tools: &mut Tools) -> Ended {
    serve(&door.door, BASE, key, tools, &AtomicBool::new(false))
}

/// A state directory of its own, kept for the test's whole life: the answers
/// store lives in it, so nothing is shared between tests.
fn state() -> std::path::PathBuf {
    tempfile::tempdir().unwrap().keep()
}

fn started() -> Tools {
    Tools::new(state()).unwrap().with(decl("agent_start"), |_| {
        Answer::Done(json!({"outcome": "started"}))
    })
}

/// The last segment of each path the door was sent.
fn routes(seen: &[Seen]) -> Vec<String> {
    seen.iter()
        .map(|s| s.path.rsplit('/').next().unwrap().to_string())
        .collect()
}

const ONE_CALL: fn() -> (u16, Value) = || {
    (
        200,
        json!({"calls": [call("c1", "agent_start", json!({}))]}),
    )
};

fn answers(seen: &[Seen]) -> Vec<Value> {
    seen.iter()
        .filter(|s| s.path.ends_with("/calls/answers"))
        .map(|s| s.body.clone())
        .collect()
}

#[test]
fn a_handler_answers_the_call_it_is_named_for() {
    let door = scripted(vec![(
        200,
        json!({"calls": [
            call("c1", "agent_start", json!({"host": "host-2"})),
            call("c2", "agent_stop", json!({"agent": "agent-a"})),
        ]}),
    )]);
    let mut tools = Tools::new(state())
        .unwrap()
        .with(decl("agent_start"), |args| {
            Answer::Done(json!({"outcome": "started", "host": args["host"]}))
        })
        .with(decl("agent_stop"), |args| {
            Answer::Done(json!({"outcome": "stopped", "agent": args["agent"]}))
        });
    assert_eq!(serve_on(&door, &mut tools), Ended::NoTools);
    assert_eq!(
        answers(&door.seen()),
        [
            json!({"call": "c1", "result": {"outcome": "started", "host": "host-2"}}),
            json!({"call": "c2", "result": {"outcome": "stopped", "agent": "agent-a"}}),
        ]
    );
}

#[test]
fn a_call_no_handler_knows_is_refused_not_dropped() {
    let door = scripted(vec![(
        200,
        json!({"calls": [call("c1", "agent_delete", json!({}))]}),
    )]);
    let ran = Arc::new(AtomicBool::new(false));
    let r = Arc::clone(&ran);
    let mut tools = Tools::new(state())
        .unwrap()
        .with(decl("agent_start"), move |_| {
            r.store(true, std::sync::atomic::Ordering::SeqCst);
            Answer::Done(json!({"outcome": "started"}))
        });
    serve_on(&door, &mut tools);
    let answered = answers(&door.seen());
    assert_eq!(answered.len(), 1, "the call is answered: {answered:?}");
    assert_eq!(answered[0]["call"], "c1");
    assert_eq!(answered[0]["result"]["outcome"], REFUSED);
    let reason = answered[0]["result"]["reason"].as_str().unwrap();
    assert!(reason.contains("`agent_delete`"), "{reason}");
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "no other handler ran"
    );
    // A handler's own refusal goes out the same way.
    assert_eq!(
        Answer::Refused("no such host".into()).into_result(),
        json!({"outcome": "refused", "reason": "no such host"})
    );
}

#[test]
fn the_call_wait_re_arms_after_an_answer_and_after_a_timeout() {
    let door = scripted(vec![
        (
            200,
            json!({"calls": [call("c1", "agent_start", json!({}))]}),
        ),
        // The hold ran out with nothing queued.
        (200, json!({"calls": []})),
        // The door did not take the wait: asked again after a pause.
        (503, json!({"code": "unavailable"})),
        (
            200,
            json!({"calls": [call("c2", "agent_start", json!({}))]}),
        ),
    ]);
    let mut tools = Tools::new(state()).unwrap().with(decl("agent_start"), |_| {
        Answer::Done(json!({"outcome": "started"}))
    });
    assert_eq!(serve_on(&door, &mut tools), Ended::NoTools);
    let paths: Vec<String> = door
        .seen()
        .iter()
        .map(|s| s.path.rsplit('/').next().unwrap().to_string())
        .collect();
    assert_eq!(
        paths,
        ["wait", "answers", "wait", "wait", "wait", "answers", "wait"],
        "a wait after each answer, after the empty one and after the refused one"
    );
}

#[test]
fn a_wait_refused_after_the_key_rotated_is_opened_again_with_the_new_key() {
    let key = Key::new("k".into());
    let door = scripted_with(
        vec![(401, json!({"code": "unauthenticated"}))],
        Vec::new(),
        Some(key.clone()),
    );
    assert_eq!(serve_with(&door, &key, &mut started()), Ended::NoTools);
    let keys: Vec<String> = door.seen().into_iter().map(|s| s.key).collect();
    assert_eq!(keys, ["k", "k2"], "the next wait presents the new key");

    // Without a rotation the door no longer opens to the key: it stops.
    let door = scripted(vec![(401, json!({"code": "unauthenticated"}))]);
    assert_eq!(serve_on(&door, &mut started()), Ended::KeyRefused);
    assert_eq!(routes(&door.seen()), ["wait"]);
}

#[test]
fn an_answer_wild_no_longer_waits_for_is_dropped_and_only_that_one() {
    // `no_such_call`: dropped after the one try.
    let door = scripted_with(
        vec![ONE_CALL()],
        vec![(404, json!({"code": "no_such_call"}))],
        None,
    );
    serve_on(&door, &mut started());
    assert_eq!(routes(&door.seen()), ["wait", "answers", "wait"]);

    // A 404 that is no refusal of the contract's is no `no_such_call`: the
    // answer goes again.
    let door = scripted_with(vec![ONE_CALL()], vec![(404, Value::Null)], None);
    serve_on(&door, &mut started());
    assert_eq!(routes(&door.seen()), ["wait", "answers", "answers", "wait"]);
}

#[test]
fn an_answer_the_door_does_not_take_is_sent_again_up_to_three_times() {
    let unavailable = || (503, json!({"code": "unavailable"}));
    let door = scripted_with(
        vec![ONE_CALL()],
        vec![unavailable(), unavailable(), unavailable(), unavailable()],
        None,
    );
    assert_eq!(serve_on(&door, &mut started()), Ended::NoTools);
    let seen = door.seen();
    assert_eq!(answers(&seen).len(), ANSWER_TRIES, "{:?}", routes(&seen));
    assert_eq!(routes(&seen).last().map(String::as_str), Some("wait"));
}

#[test]
fn a_wild_without_call_routes_ends_the_wait_instead_of_asking_for_ever() {
    let door = scripted(vec![(404, Value::Null)]);
    assert_eq!(serve_on(&door, &mut started()), Ended::NoCallRoute);
    assert_eq!(routes(&door.seen()), ["wait"]);
    // A key never shows in a debug line.
    assert_eq!(format!("{:?}", Key::new("wdk_secret".into())), "Key(…)");
}

/// ADR-0398 D5: a repeat — the same key with the same arguments — is answered
/// as the first call was, and the handler runs no second time.
#[test]
fn a_repeat_under_one_key_is_answered_as_before_and_runs_nothing() {
    let args = json!({"host": "host-2", "idempotency_key": "k1"});
    let door = scripted(vec![
        (
            200,
            json!({"calls": [call("c1", "agent_start", args.clone())]}),
        ),
        (
            200,
            json!({"calls": [call("c2", "agent_start", args.clone())]}),
        ),
    ]);
    let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let r = Arc::clone(&ran);
    let mut tools = Tools::new(state())
        .unwrap()
        .with(decl("agent_start"), move |_| {
            let n = r.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            Answer::Done(json!({"outcome": "started", "agent": format!("agent-{n}")}))
        });
    assert_eq!(serve_on(&door, &mut tools), Ended::NoTools);
    let answered = answers(&door.seen());
    assert_eq!(answered.len(), 2, "{answered:?}");
    assert_eq!(
        answered[1]["result"], answered[0]["result"],
        "the repeat is answered as before: {answered:?}"
    );
    assert_eq!(
        ran.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the handler ran once: {answered:?}"
    );
}

/// ADR-0398 D5: one key used for another call is refused, saying so, and
/// neither call runs a second time.
#[test]
fn the_same_key_with_other_arguments_is_refused_and_runs_nothing() {
    let door = scripted(vec![
        (
            200,
            json!({"calls": [call(
                "c1", "agent_start",
                json!({"host": "host-2", "idempotency_key": "k1"})
            )]}),
        ),
        (
            200,
            json!({"calls": [call(
                "c2", "agent_start",
                json!({"host": "host-3", "idempotency_key": "k1"})
            )]}),
        ),
    ]);
    let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let r = Arc::clone(&ran);
    let mut tools = Tools::new(state())
        .unwrap()
        .with(decl("agent_start"), move |_| {
            r.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Answer::Done(json!({"outcome": "started"}))
        });
    assert_eq!(serve_on(&door, &mut tools), Ended::NoTools);
    let answered = answers(&door.seen());
    assert_eq!(answered.len(), 2, "{answered:?}");
    assert_eq!(answered[1]["result"]["outcome"], REFUSED, "{answered:?}");
    let reason = answered[1]["result"]["reason"].as_str().unwrap();
    assert!(reason.contains("`k1`"), "the key is named: {reason}");
    assert!(
        reason.contains("another call"),
        "the reason says what happened: {reason}"
    );
    assert_eq!(
        ran.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "neither call ran a second time: {answered:?}"
    );
}

/// ADR-0398 D5: a key names one call, not one argument string. The same key
/// with identical arguments on ANOTHER function is refused — never answered
/// with the first function's answer as if it had run.
#[test]
fn the_same_key_with_identical_arguments_on_another_function_is_refused() {
    let args = json!({"host": "host-2", "idempotency_key": "k1"});
    let door = scripted(vec![
        (
            200,
            json!({"calls": [call("c1", "agent_start", args.clone())]}),
        ),
        (
            200,
            json!({"calls": [call("c2", "agent_stop", args.clone())]}),
        ),
    ]);
    let ran = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let r = Arc::clone(&ran);
    let mut tools = Tools::new(state())
        .unwrap()
        .with(decl("agent_start"), move |_| {
            r.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Answer::Done(json!({"outcome": "started"}))
        })
        .with(decl("agent_stop"), |_| {
            Answer::Done(json!({"outcome": "stopped"}))
        });
    assert_eq!(serve_on(&door, &mut tools), Ended::NoTools);
    let answered = answers(&door.seen());
    assert_eq!(answered.len(), 2, "{answered:?}");
    assert_eq!(
        answered[1]["result"]["outcome"], REFUSED,
        "another function under the same key is refused, not replayed: {answered:?}"
    );
    let reason = answered[1]["result"]["reason"].as_str().unwrap();
    assert!(reason.contains("`k1`"), "the key is named: {reason}");
    assert_eq!(
        ran.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "the first function's handler ran once: {answered:?}"
    );
}

/// ADR-0398 D5: the kept answers outlive the producer process — a restart
/// replays them instead of running the call again.
#[test]
fn a_kept_answer_survives_a_restart() {
    let state = state();
    let args = json!({"host": "host-2", "idempotency_key": "k1"});
    let ask = |id: &str| ToolCall {
        call: id.into(),
        tool: "agent_start".into(),
        arguments: args.clone(),
    };
    let mut first = Tools::new(&state).unwrap().with(decl("agent_start"), |_| {
        Answer::Done(json!({"outcome": "started", "agent": "agent-1"}))
    });
    assert_eq!(first.answer(&ask("c1")).result["agent"], "agent-1");
    drop(first);

    let ran = Arc::new(AtomicBool::new(false));
    let r = Arc::clone(&ran);
    let mut second = Tools::new(&state)
        .unwrap()
        .with(decl("agent_start"), move |_| {
            r.store(true, std::sync::atomic::Ordering::SeqCst);
            Answer::Done(json!({"outcome": "started", "agent": "agent-99"}))
        });
    let replayed = second.answer(&ask("c2"));
    assert_eq!(
        replayed.result["agent"], "agent-1",
        "a restart replays the kept answer: {replayed:?}"
    );
    assert!(
        !ran.load(std::sync::atomic::Ordering::SeqCst),
        "the handler ran no second time"
    );
}

/// ADR-0398 D5: a key is kept a week; a call kept as started but never
/// answered is not run again, and a key past its week is written out when the
/// store is opened.
#[test]
fn a_key_is_kept_a_week_and_a_call_started_but_never_answered_is_not_run_again() {
    let dir = state();
    let args = json!({"host": "host-2", "idempotency_key": "k1"});
    let tool = "agent_start";
    let mut answers = Answers::open(&dir).unwrap();
    let now = now_secs();
    assert_eq!(answers.asked_at("k1", tool, &args, now), Asked::New);
    answers.begin("k1", tool, &args).unwrap();
    assert_eq!(
        answers.asked_at("k1", tool, &args, now),
        Asked::Pending,
        "a call that was started and never answered is not run again"
    );
    answers
        .keep("k1", tool, &args, &json!({"outcome": "started"}))
        .unwrap();
    assert_eq!(
        answers.asked_at("k1", tool, &args, now),
        Asked::Same(json!({"outcome": "started"}))
    );
    assert_eq!(
        answers.asked_at(
            "k1",
            tool,
            &json!({"host": "host-3", "idempotency_key": "k1"}),
            now
        ),
        Asked::Other
    );
    assert_eq!(
        answers.asked_at("k1", "agent_stop", &args, now),
        Asked::Other,
        "the same key and arguments on another function is another call"
    );
    let later = now_secs() + KEEP_ANSWERED.as_secs() + 1;
    assert_eq!(answers.asked_at("k1", tool, &args, later), Asked::New);
    let reopened = Answers::open_at(&dir, later).unwrap();
    assert_eq!(reopened.asked_at("k1", tool, &args, later), Asked::New);
    assert_eq!(
        fs::read_to_string(dir.join(ANSWERED)).unwrap(),
        "",
        "a key past its week is written out when the store is opened"
    );
}

/// The answers a producer keeps carry what its tools returned: the store is
/// this user's alone, whether it was appended to or written back whole.
#[test]
fn the_kept_answers_are_for_this_user_alone() {
    use std::os::unix::fs::PermissionsExt;
    let dir = state();
    let mode = || {
        fs::metadata(dir.join(ANSWERED))
            .unwrap()
            .permissions()
            .mode()
            & 0o777
    };
    let args = json!({"host": "host-2", "idempotency_key": "k1"});
    let mut answers = Answers::open(&dir).unwrap();
    answers.begin("k1", "agent_start", &args).unwrap();
    assert_eq!(mode(), 0o600, "appended");

    // A store left readable by others (an older version, a copy, a hand)
    // is tightened by the next append, not only by a fresh file.
    fs::set_permissions(dir.join(ANSWERED), fs::Permissions::from_mode(0o644)).unwrap();
    answers.begin("k2", "agent_start", &args).unwrap();
    assert_eq!(mode(), 0o600, "appended to a store already there");

    fs::set_permissions(dir.join(ANSWERED), fs::Permissions::from_mode(0o644)).unwrap();
    let later = now_secs() + KEEP_ANSWERED.as_secs() + 1;
    Answers::open_at(&dir, later).unwrap();
    assert_eq!(mode(), 0o600, "written back whole");
}

/// A call's id, tool and key come from the door: printed, no control
/// character among them reaches the terminal.
#[test]
fn text_from_the_door_is_printed_without_its_control_characters() {
    assert_eq!(
        printable("call c1: agent_start answered started"),
        "call c1: agent_start answered started"
    );
    let shown = printable("c1\u{1b}]0;owned\u{7}\u{1b}[2J\r\nfake line\u{9b}31m");
    assert!(!shown.chars().any(char::is_control), "{shown:?}");
    assert!(
        shown.contains("owned") && shown.contains("fake line"),
        "{shown:?}"
    );
}

/// Nor a bidi or invisible format character: an override would reorder what
/// the line shows, a zero-width one hide where one id ends.
#[test]
fn text_from_the_door_is_printed_without_its_bidi_or_invisible_characters() {
    let shown = printable(
        "call c1\u{202E}trats_tnega\u{202C} \u{2066}x\u{2069}\u{200B}\u{FEFF}\u{061C}\u{00AD}",
    );
    for hidden in [
        '\u{202E}', '\u{202C}', '\u{2066}', '\u{2069}', '\u{200B}', '\u{FEFF}', '\u{061C}',
        '\u{00AD}',
    ] {
        assert!(!shown.contains(hidden), "{hidden:?} in {shown:?}");
    }
    assert!(
        shown.contains("trats_tnega") && shown.contains('x'),
        "{shown:?}"
    );
    assert_eq!(printable("Grüße, ünïcödé — 日本"), "Grüße, ünïcödé — 日本");
}
