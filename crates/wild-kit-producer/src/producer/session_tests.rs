//! The session against a scripted door on a Unix socket: each request the
//! session makes is noted, and answered with the next reply of the route's
//! script — the routes the reference producer runs every start.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};
use wild_ingest_contract::batch;
use wild_ingest_contract::manifest::Manifest;

use super::*;
use crate::producer::door;

/// One request the door was sent: its path, the pairing code it presented,
/// its JSON body (null when the body is NDJSON) and the body as text.
#[derive(Debug, Clone)]
struct Seen {
    path: String,
    pairing: Option<String>,
    body: Value,
    raw: String,
}

/// A door that answers each path with the next `(status, body)` scripted for
/// it, and with a plausible default once that runs out: an activated
/// announcement first (revoked later on), a received batch, an epoch, an
/// empty vocabulary, no statuses.
struct Fake {
    seen: Arc<Mutex<Vec<Seen>>>,
    _dir: tempfile::TempDir,
    door: Door,
}

fn fake(script: Vec<(&str, Vec<(u16, Value)>)>) -> Fake {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("door.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let noted = Arc::clone(&seen);
    let routes: HashMap<String, VecDeque<(u16, Value)>> = script
        .into_iter()
        .map(|(p, r)| (p.to_string(), r.into()))
        .collect();
    std::thread::spawn(move || {
        let mut routes = routes;
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut words = line.split_whitespace();
            let method = words.next().unwrap_or("").to_string();
            let path = words.next().unwrap_or("").to_string();
            let mut length = 0;
            let mut pairing = None;
            loop {
                let mut h = String::new();
                reader.read_line(&mut h).unwrap();
                if h.trim().is_empty() {
                    break;
                }
                if let Some((k, v)) = h.split_once(':') {
                    let v = v.trim().to_string();
                    if k.eq_ignore_ascii_case("content-length") {
                        length = v.parse().unwrap();
                    }
                    if k.eq_ignore_ascii_case("wild-pairing-code") {
                        pairing = Some(v);
                    }
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let raw = String::from_utf8_lossy(&body).into_owned();
            let body = serde_json::from_slice(&body).unwrap_or(Value::Null);
            noted.lock().unwrap().push(Seen {
                path: path.clone(),
                pairing,
                body: body.clone(),
                raw,
            });
            // An exact path first, then the longest key the path ends with: a
            // door routes on the path's shape, not on what the producer called
            // the batch file.
            let key = if routes.contains_key(&path) {
                Some(path.clone())
            } else {
                routes
                    .keys()
                    .filter(|k| path.ends_with(k.as_str()))
                    .max_by_key(|k| k.len())
                    .cloned()
            };
            let (status, reply) = key
                .as_ref()
                .and_then(|k| routes.get_mut(k))
                .and_then(VecDeque::pop_front)
                .unwrap_or_else(|| default_for(&method, &path, &body));
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
    Fake {
        seen,
        _dir: dir,
        door,
    }
}

/// A plausible answer once a route's script runs out: an activated
/// announcement, a received batch, an epoch, an empty vocabulary, and every
/// batch id asked about reported `expired` (so a test can let the real id be
/// made by the producer).
fn default_for(method: &str, path: &str, body: &Value) -> (u16, Value) {
    if method == "POST" && path.ends_with("/api/v1/producers/announcements") {
        (202, json!({"announcement": "a1"}))
    } else if path.contains("/api/v1/producers/announcements/") {
        (200, json!({"state": "revoked"}))
    } else if path.ends_with("/device/epoch") {
        (200, json!({"epoch": "e1"}))
    } else if path.ends_with("/vocabulary") {
        (200, json!({"etag": "v1"}))
    } else if path.ends_with("/batches/status") {
        let statuses: Vec<Value> = body["ids"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|id| json!({"batch": id, "state": "expired"}))
            .collect();
        (200, json!({ "statuses": statuses }))
    } else if path.ends_with("/batches") {
        (
            202,
            json!({"state": "received", "commit_mark": 1, "directives": {}}),
        )
    } else {
        (200, Value::Null)
    }
}

impl Fake {
    fn seen(&self) -> Vec<Seen> {
        self.seen.lock().unwrap().clone()
    }

    fn batches(&self) -> Vec<Seen> {
        self.seen()
            .into_iter()
            .filter(|s| s.path.ends_with("/batches"))
            .collect()
    }
}

const BASE: &str = "/api/v1/tribes/t/producers/p";

fn activated() -> (u16, Value) {
    (
        200,
        json!({"state": "activated", "producer_key": "k1", "tribe": "t", "epoch": "e1"}),
    )
}

fn shape() -> Value {
    json!({
        "name": "tick",
        "version": 1,
        "key_class": "live",
        "key": ["_seq"],
        "target_type": "Tick",
        "fields": [
            {"name": "_seq", "kind": "number", "required": true},
            {"name": "batch", "kind": "number", "required": true},
            {"name": "lines", "kind": "number", "required": true},
        ],
        "description": "One batch, made by the producer, carrying how many lines it read",
    })
}

fn manifest() -> Manifest {
    serde_json::from_value(json!({
        "producer": "p",
        "version": "0.1.0",
        "contract_version": "3",
        "tribe": "t",
        "sources": [{"slug": "s", "description": "Lines", "shapes": [shape()]}],
        "references": [],
        "needs_vocabulary": false,
        "fingerprint": "fp",
        "tools": [],
        "answers_for": [],
    }))
    .unwrap()
}

/// A state directory of its own, kept for the test's whole life.
fn state() -> PathBuf {
    tempfile::tempdir().unwrap().keep()
}

fn pairing(with_code: bool) -> PathBuf {
    let file = state().join("pairing.json");
    let body = json!({
        "door": "/tmp/door.sock",
        "pairing_code": if with_code { json!("c1") } else { Value::Null },
    });
    std::fs::write(&file, body.to_string()).unwrap();
    file
}

fn options() -> Options {
    Options::default()
}

#[test]
fn open_announces_takes_the_code_and_keeps_the_key() {
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let dir = state();
    let pairing = pairing(true);
    let session =
        Session::open(f.door.clone(), &dir, &manifest(), Some(&pairing), options()).unwrap();
    assert_eq!(session.base(), BASE);
    assert_eq!(session.epoch(), "e1");
    assert_eq!(session.key().get(), "k1");
    assert_eq!(session.made(), 0);
    assert!(!session.revoked());
    let announced = f
        .seen()
        .into_iter()
        .find(|s| s.path.ends_with("/api/v1/producers/announcements"))
        .unwrap();
    assert_eq!(announced.pairing.as_deref(), Some("c1"));
    assert_eq!(announced.body["producer"], "p");
    let kept: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("key.json")).unwrap()).unwrap();
    assert_eq!(kept["key"], "k1");
    assert_eq!(kept["announcement"], "a1");
    assert!(!pairing.exists(), "the spent pairing file is gone");
}

/// A manifest as a connector writes it: one source declaring one live shape
/// of its own and no tick.
fn connector() -> Manifest {
    serde_json::from_value(json!({
        "producer": "c",
        "version": "0.1.0",
        "contract_version": "3",
        "tribe": "t",
        "sources": [{"slug": "s", "description": "Agent statuses", "shapes": [{
            "name": "agent_status",
            "version": 1,
            "key_class": "live",
            "key": [],
            "target_type": "AgentStatus",
            "fields": [{"name": "agent", "kind": "text", "max_len": 32, "required": true}],
            "description": "One agent's status.",
        }]}],
        "references": [],
        "needs_vocabulary": false,
        "fingerprint": "fp",
        "tools": [],
        "answers_for": [],
    }))
    .unwrap()
}

/// The door refuses a line whose shape the announced manifest did not name —
/// whole batches were answered `shape_unknown` for `tick@1` on the walk — and
/// every batch the session makes ends with a tick. So the announcement has to
/// declare the tick beside whatever the connector declared itself.
#[test]
fn the_announcement_declares_the_tick_every_batch_carries() {
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &connector(), None, options()).unwrap();
    let epoch = session.epoch().to_string();
    let _sent = session
        .send(
            "s",
            &[json!({"shape": "agent_status@1", "record": {
                "_epoch": epoch, "_seq": 0, "agent": "a",
            }})
            .to_string()],
        )
        .unwrap();

    let announced = f
        .seen()
        .into_iter()
        .find(|s| s.path.ends_with("/api/v1/producers/announcements"))
        .unwrap();
    let announced: Manifest = serde_json::from_value(announced.body.clone()).unwrap();
    announced
        .validate()
        .expect("the announcement is a manifest the door accepts");
    let declared: Vec<String> = announced.sources[0]
        .shapes
        .iter()
        .map(|s| s.shape_ref().to_string())
        .collect();
    assert!(
        declared.contains(&"tick@1".to_string()),
        "the announcement declares the tick: {declared:?}"
    );

    let batch = f.batches().pop().unwrap();
    let lines = batch::parse(batch.raw.as_bytes()).unwrap();
    batch::check_lines(&lines, &announced.sources[0].shapes)
        .expect("every line of the batch fits a shape the announcement declared");
}

#[test]
fn a_pause_directive_pauses_and_the_status_route_ends_it() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        // Keyed on the tail alone, so a path that names the wrong source
        // still finds the route and only the assertion below notices.
        (
            "/batches",
            vec![(
                202,
                json!({"state": "received", "commit_mark": 1, "directives": {"paused": true}}),
            )],
        ),
        (
            "/api/v1/tribes/t/producers/p/batches/status",
            vec![
                (200, json!({"directives": {"paused": true}})),
                (200, json!({"directives": {"paused": false}})),
            ],
        ),
    ]);
    let mut session =
        Session::open(f.door.clone(), &state(), &manifest(), None, options()).unwrap();
    assert!(matches!(
        session.send("s", &["a".into()]).unwrap(),
        Sent::Paused(_)
    ));
    assert!(session.send("s", &[]).unwrap().wait().is_zero());
    session.wait_out_pause().unwrap();
    let sent = session.send("s", &["b".into()]).unwrap();
    assert!(matches!(sent, Sent::Accepted { .. }), "{sent:?}");
    let batches = f.batches();
    assert_eq!(batches.len(), 2);
    // The slug the caller sent under, not the outbox file's stem.
    for b in &batches {
        assert!(b.path.ends_with("/sources/s/batches"), "{}", b.path);
    }
}

#[test]
fn a_revocation_is_said_and_written() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        (
            "/api/v1/tribes/t/producers/p/sources/s/batches",
            vec![(401, json!({}))],
        ),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    assert!(matches!(
        session.send("s", &["a".into()]).unwrap(),
        Sent::Revoked
    ));
    assert!(session.revoked());
    assert!(dir.join("revoked").exists());
}

#[test]
fn an_expired_batch_goes_out_again_once() {
    // The fake's status default reports every id it is asked about expired,
    // so the batch's real id need not be known up front.
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let mut options = options();
    options.resend_expired = true;
    let mut session = Session::open(f.door.clone(), &state(), &manifest(), None, options).unwrap();
    let _ = session.send("s", &["a".into()]).unwrap();
    assert_eq!(f.batches().len(), 1);
    // Ask the door about the kept batch: it says it expired.
    let _ = session.send("s", &[]).unwrap();
    let out = f.batches();
    assert_eq!(out.len(), 2, "the expired batch goes out again");
    assert_eq!(out[0].body, out[1].body, "the same bytes go out again");
    assert!(session.made() == 1, "no new batch was made");
}

#[test]
fn the_store_hook_runs_between_the_outbox_and_the_post() {
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let dir = state();
    let outbox = dir.join("outbox");
    let watched = Arc::new(Mutex::new(Vec::new()));
    let note = Arc::clone(&watched);
    let count = outbox.clone();
    let mut options = options();
    options.store = Some(Box::new(move |_, made| {
        let kept = std::fs::read_dir(&count).unwrap().count();
        note.lock().unwrap().push((kept, made));
        Ok(())
    }));
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options).unwrap();
    let _ = session.send("s", &["a".into()]).unwrap();
    assert_eq!(*watched.lock().unwrap(), vec![(1, 0)]);
}

#[test]
fn the_outbox_is_skipped_when_the_made_count_is_stored_first() {
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let dir = state();
    let outbox = dir.join("outbox");
    let watched = Arc::new(Mutex::new(Vec::new()));
    let note = Arc::clone(&watched);
    let count = outbox.clone();
    let mut options = options();
    options.send_before_store = true;
    options.store = Some(Box::new(move |_, made| {
        let kept = std::fs::read_dir(&count).unwrap().count();
        note.lock().unwrap().push((kept, made));
        Ok(())
    }));
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options).unwrap();
    let _ = session.send("s", &["a".into()]).unwrap();
    assert_eq!(*watched.lock().unwrap(), vec![(0, 0)]);
    assert_eq!(outbox.read_dir().unwrap().count(), 0, "dropped, not kept");
}

#[test]
fn a_revoked_file_at_the_start_opens_a_revoked_session() {
    let f = fake(vec![]);
    let dir = state();
    std::fs::write(dir.join("revoked"), b"").unwrap();
    let session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    assert!(session.revoked());
    assert!(f.seen().is_empty(), "it announces nothing anew");
}
