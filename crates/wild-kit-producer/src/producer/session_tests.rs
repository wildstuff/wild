//! The session against a scripted door on a Unix socket: each request the
//! session makes is noted, and answered with the next reply of the route's
//! script — the routes the reference producer runs every start.

use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
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
            if status == 0 {
                // Scripted `(0, …)`: the connection closes with no reply.
                continue;
            }
            let retry = reply["retry_after"]
                .as_u64()
                .map_or_else(String::new, |s| format!("retry-after: {s}\r\n"));
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\n{retry}content-length: {}\r\nconnection: close\r\n\r\n{text}",
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
fn a_batch_stored_before_it_is_sent_still_raises_the_made_count() {
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let mut options = options();
    options.send_before_store = true;
    let mut session = Session::open(f.door.clone(), &state(), &manifest(), None, options).unwrap();
    let _ = session.send("s", &["a".into()]).unwrap();
    assert_eq!(session.made(), 1, "the made count follows the stored batch");
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

/// With the revoked file ignored, a `401` is a plain refusal: the session
/// stays open, keeps the batch, and does not look the announcement up.
#[test]
fn a_401_is_kept_while_the_revoked_file_is_ignored() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", vec![(401, json!({"code": "revoked"}))]),
    ]);
    let dir = state();
    let mut options = options();
    options.ignore_revoked = true;
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options).unwrap();
    let out = session.send("s", &["a".into()]).unwrap();
    assert!(matches!(out, Sent::Accepted { .. }), "{out:?}");
    assert!(!session.revoked());
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 1, "kept");
}

/// `n` batches sit in the outbox, every tick's first POST refused `429`:
/// the device-wide refusal ends each tick before the earlier files are
/// reached, so a later tick can walk all of them.
fn pending(session: &mut Session, n: usize) {
    for i in 0..n {
        let _ = session.send("s", &[format!("line-{i}")]).unwrap();
    }
}

/// The batch bodies the door was sent, in order.
fn raws(f: &Fake) -> Vec<String> {
    f.batches().into_iter().map(|b| b.raw).collect()
}

/// A permanent refusal sets its batch aside in `refused/` with the reply
/// beside it, and the tick goes on: the good batch behind it still goes
/// out. No later tick posts the refused one again (ADR-0365: a refusal
/// keeps the batch, a resend cannot change a permanent one).
fn permanently_refused_is_set_aside(dir: PathBuf, status: u16, code: &str) {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        (
            "/batches",
            vec![
                (429, json!({})),
                (429, json!({})),
                (status, json!({"code": code})),
            ],
        ),
        // The two quiet ticks below: nothing reported expired.
        (
            "/batches/status",
            vec![
                (200, json!({"statuses": []})),
                (200, json!({"statuses": []})),
            ],
        ),
    ]);
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    pending(&mut session, 2);
    let refused_body = raws(&f)[1].clone();
    let before = f.batches().len();

    let _ = session.send("s", &[]).unwrap();
    assert_eq!(
        f.batches().len(),
        before + 2,
        "the tick goes past the refusal"
    );
    let kept = std::fs::read_dir(dir.join("refused"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(kept.contains(&"0000.ndjson".to_string()), "{kept:?}");
    assert!(
        kept.contains(&"0000.ndjson.reply.json".to_string()),
        "{kept:?}"
    );
    let reply: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("refused/0000.ndjson.reply.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(reply["code"], json!(code));
    assert_eq!(
        std::fs::read(dir.join("refused/0000.ndjson")).unwrap(),
        refused_body.as_bytes(),
        "the bytes are kept, not just the name"
    );
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 0);

    // Two ticks on, nothing of it is posted again.
    for _ in 0..2 {
        let _ = session.send("s", &[]).unwrap();
    }
    assert_eq!(f.batches().len(), before + 2);
    assert_eq!(
        raws(&f).iter().filter(|r| **r == refused_body).count(),
        3,
        "two backpressure POSTs, then the permanent refusal; never again"
    );
}

#[test]
fn a_batch_refused_422_is_set_aside_and_the_tick_goes_on() {
    let dir = state();
    permanently_refused_is_set_aside(dir.clone(), 422, "batch_refused");
    assert_eq!(
        dir.join("outbox").read_dir().unwrap().count(),
        0,
        "the script ran: the session opened and emptied the outbox"
    );
}

#[test]
fn a_batch_over_bound_or_undeclared_is_set_aside_too() {
    let over = state();
    permanently_refused_is_set_aside(over.clone(), 413, "over_bound");
    assert_eq!(over.join("outbox").read_dir().unwrap().count(), 0);
    let undeclared = state();
    permanently_refused_is_set_aside(undeclared.clone(), 422, "shape_version_undeclared");
    assert_eq!(undeclared.join("outbox").read_dir().unwrap().count(), 0);
}

/// A `422` whose code is none of the permanent ones can be accepted by a
/// resend, so it stays in the outbox and is not set aside.
#[test]
fn a_422_that_is_no_permanent_code_is_kept() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", vec![(422, json!({"code": "validation"}))]),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let out = session.send("s", &["a".into()]).unwrap();
    assert!(matches!(out, Sent::Accepted { .. }), "{out:?}");
    assert_eq!(dir.join("refused").read_dir().unwrap().count(), 0);
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 1, "kept");
}

/// A permanent refusal whose batch is already gone from the outbox still
/// succeeds: the reply files are what matters, and `NotFound` is no error.
#[test]
fn a_refused_batch_already_gone_from_the_outbox_is_no_error() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", vec![(422, json!({"code": "batch_refused"}))]),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let gone = dir.join("outbox/9999.ndjson");
    let (sent, left) = session.send_body(&gone, b"{}\n").unwrap();
    assert!(matches!(sent, Sent::Accepted { .. }), "{sent:?}");
    assert_eq!(left, Left::SetAside);
    assert_eq!(
        std::fs::read(dir.join("refused/9999.ndjson")).unwrap(),
        b"{}\n"
    );
    assert!(dir.join("refused/9999.ndjson.reply.json").exists());
}

/// If the batch stays in the outbox because its directory will not let it
/// be removed, that is reported, not swallowed: the refused state on disk
/// and the outbox no longer agree.
#[test]
fn a_refused_batch_that_cannot_leave_the_outbox_reports_it() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", vec![(422, json!({"code": "batch_refused"}))]),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let outbox = dir.join("outbox");
    std::fs::write(outbox.join("0000.ndjson"), b"{}\n").unwrap();
    std::fs::set_permissions(&outbox, std::fs::Permissions::from_mode(0o555)).unwrap();
    let out = session.send("s", &[]);
    std::fs::set_permissions(&outbox, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(out.is_err(), "{out:?}");
    assert!(dir.join("refused/0000.ndjson").exists());
}

#[test]
fn a_vocabulary_change_puts_a_set_aside_batch_back() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        (
            "/batches",
            vec![
                (429, json!({})),
                (422, json!({"code": "batch_refused"})),
                (
                    202,
                    json!({"state": "received", "commit_mark": 1,
                          "directives": {"vocabulary_changed": true}}),
                ),
            ],
        ),
        // The tick after the directive: nothing reported expired.
        ("/batches/status", vec![(200, json!({"statuses": []}))]),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    pending(&mut session, 1);
    let _ = session.send("s", &[]).unwrap();
    let refused_body = raws(&f)[0].clone();
    assert!(dir.join("refused/0000.ndjson").exists());

    // A good batch's reply carries the directive: the refused file goes back
    // and its reply file goes away.
    let _ = session.send("s", &["good".into()]).unwrap();
    assert_eq!(dir.join("refused").read_dir().unwrap().count(), 0);
    assert!(dir.join("outbox/0000.ndjson").exists());

    let _ = session.send("s", &[]).unwrap();
    assert_eq!(
        raws(&f).iter().filter(|r| **r == refused_body).count(),
        3,
        "posted once more after the directive, then taken"
    );
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 0);
}

/// One batch in `refused/` with the reply that put it there.
fn a_batch_in_refused(dir: &std::path::Path, body: &[u8]) {
    std::fs::create_dir_all(dir.join("refused")).unwrap();
    std::fs::write(dir.join("refused/0007.ndjson"), body).unwrap();
    std::fs::write(
        dir.join("refused/0007.ndjson.reply.json"),
        b"{\"status\":422}",
    )
    .unwrap();
}

/// The script of a tick whose accepted batch carries `vocabulary_changed`.
fn vocabulary_changed() -> Fake {
    fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        (
            "/batches",
            vec![(
                202,
                json!({"state": "received", "commit_mark": 1,
                       "directives": {"vocabulary_changed": true}}),
            )],
        ),
    ])
}

fn vocabulary_requests(f: &Fake) -> usize {
    f.seen()
        .iter()
        .filter(|s| s.path.ends_with("/vocabulary"))
        .count()
}

/// `ignore_vocabulary` drops the fetch the directive asks for, though the
/// batch still comes back into the outbox.
#[test]
fn the_vocabulary_changed_fetch_is_dropped_when_ignored() {
    let f = vocabulary_changed();
    let dir = state();
    a_batch_in_refused(&dir, b"{}\n");
    let mut options = options();
    options.ignore_vocabulary = true;
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options).unwrap();
    assert_eq!(vocabulary_requests(&f), 1, "the open fetched it");
    let _ = session.send("s", &["a".into()]).unwrap();
    assert_eq!(vocabulary_requests(&f), 1, "the directive fetched nothing");
    assert_eq!(dir.join("refused").read_dir().unwrap().count(), 0);
    assert!(
        dir.join("outbox/0007.ndjson").exists(),
        "back in the outbox"
    );
}

/// With `no_vocabulary` the connector never asks either: the directive
/// restores the batch and stops there.
#[test]
fn a_vocabulary_changed_directive_fetches_nothing_when_no_vocabulary() {
    let f = vocabulary_changed();
    let dir = state();
    a_batch_in_refused(&dir, b"{}\n");
    let mut options = options();
    options.no_vocabulary = true;
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options).unwrap();
    assert_eq!(vocabulary_requests(&f), 0);
    let _ = session.send("s", &["a".into()]).unwrap();
    assert_eq!(vocabulary_requests(&f), 0, "nothing was asked for");
    assert!(dir.join("outbox/0007.ndjson").exists());
}

/// `refused/` gone is state damage, not "nothing set aside": the directive
/// reports it instead of silently keeping the batch out.
#[test]
fn a_missing_refused_directory_is_reported() {
    let f = vocabulary_changed();
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    // The directive rides on a reply, so the send must carry a batch to get one.
    std::fs::remove_dir_all(dir.join("refused")).unwrap();
    assert!(
        session.send("s", &["a".into()]).is_err(),
        "the restore reports the missing directory"
    );
}

/// A device-wide refusal ends the tick: exactly one POST though three
/// batches wait, and all three stay in the outbox.
fn device_wide_ends_the_tick(dir: PathBuf, tail: (u16, Value), announcement: Vec<(u16, Value)>) {
    let mut route = vec![(429, json!({})), (429, json!({})), (429, json!({}))];
    route.push(tail);
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", announcement),
        ("/batches", route),
    ]);
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    pending(&mut session, 3);
    let before = f.batches().len();
    let _ = session.send("s", &[]).unwrap();
    assert_eq!(
        f.batches().len(),
        before + 1,
        "the tick stops at the refusal"
    );
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 3);
}

#[test]
fn a_backpressure_refusal_ends_the_tick_and_keeps_every_batch() {
    let dir = state();
    device_wide_ends_the_tick(
        dir.clone(),
        (429, json!({"code": "backpressure", "retry_after": 1})),
        vec![activated()],
    );
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 3);
}

#[test]
fn a_rewound_daemon_ends_the_tick_and_keeps_every_batch() {
    let dir = state();
    device_wide_ends_the_tick(
        dir.clone(),
        (
            409,
            json!({"code": "daemon_rewound", "params": {"known_mark": 0}}),
        ),
        vec![activated()],
    );
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 3);
}

#[test]
fn an_unauthenticated_refusal_ends_the_tick_and_keeps_every_batch() {
    let dir = state();
    device_wide_ends_the_tick(
        dir.clone(),
        (401, json!({})),
        vec![activated(), (200, json!({"state": "activated"}))],
    );
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 3);
}

#[test]
fn no_reply_ends_the_tick_and_keeps_every_batch() {
    let dir = state();
    device_wide_ends_the_tick(dir.clone(), (0, Value::Null), vec![activated()]);
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 3);
}

#[test]
fn an_unknown_status_ends_the_tick_and_keeps_the_batch() {
    let mut route = vec![(429, json!({})), (429, json!({})), (429, json!({}))];
    route.push((418, json!({"code": "a_teapot"})));
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", route),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    pending(&mut session, 3);
    let before = f.batches().len();
    let sent = session.send("s", &[]).unwrap();
    assert!(matches!(sent, Sent::Accepted { .. }), "{sent:?}");
    assert_eq!(
        f.batches().len(),
        before + 1,
        "the tick stops at the refusal"
    );
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 3);
}

#[test]
fn a_restart_does_not_send_a_set_aside_batch() {
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let dir = state();
    std::fs::create_dir_all(dir.join("refused")).unwrap();
    std::fs::write(dir.join("refused/0000.ndjson"), b"{\"shape\":\"tick@1\"}\n").unwrap();
    std::fs::write(dir.join("refused/0000.ndjson.reply.json"), b"{}").unwrap();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let _ = session.send("s", &[]).unwrap();
    assert!(f.batches().is_empty(), "nothing leaves refused/ on its own");
}

/// A restart whose start fetch finds a vocabulary newer than the one held:
/// the batch set aside under the old vocabulary goes out again.
#[test]
fn a_newer_vocabulary_at_the_start_lets_a_set_aside_batch_out_again() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/vocabulary", vec![(200, json!({"etag": "v2"}))]),
        ("/batches", vec![(202, json!({"state": "received"}))]),
    ]);
    let dir = state();
    std::fs::create_dir_all(dir.join("refused")).unwrap();
    std::fs::write(dir.join("refused/0000.ndjson"), b"{\"shape\":\"tick@1\"}\n").unwrap();
    std::fs::write(dir.join("refused/0000.ndjson.reply.json"), b"{}").unwrap();
    std::fs::write(dir.join("vocabulary-etag"), b"v1").unwrap();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let _ = session.send("s", &[]).unwrap();
    assert_eq!(f.batches().len(), 1, "the refused body is sent again");
    assert_eq!(dir.join("refused").read_dir().unwrap().count(), 0);
}

/// The same fetch holding the same ETag: nothing changed, so the batch
/// stays set aside.
#[test]
fn an_unchanged_vocabulary_at_the_start_keeps_the_set_aside_batch() {
    let f = fake(vec![(
        "/api/v1/producers/announcements/a1",
        vec![activated()],
    )]);
    let dir = state();
    std::fs::create_dir_all(dir.join("refused")).unwrap();
    std::fs::write(dir.join("refused/0000.ndjson"), b"{\"shape\":\"tick@1\"}\n").unwrap();
    std::fs::write(dir.join("refused/0000.ndjson.reply.json"), b"{}").unwrap();
    std::fs::write(dir.join("vocabulary-etag"), b"v1").unwrap();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let _ = session.send("s", &[]).unwrap();
    assert!(
        f.batches().is_empty(),
        "an unchanged vocabulary changes nothing"
    );
    assert!(dir.join("refused/0000.ndjson").exists());
}

/// The decision is the code's, not the HTTP status: a `413` whose code is
/// no permanent one can still be accepted by a resend and stays kept.
#[test]
fn a_413_whose_code_is_no_permanent_one_is_kept() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", vec![(413, json!({"code": "backpressure"}))]),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let out = session.send("s", &["a".into()]).unwrap();
    assert!(matches!(out, Sent::Accepted { .. }), "{out:?}");
    assert_eq!(dir.join("refused").read_dir().unwrap().count(), 0);
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 1, "kept");
}

/// A code this producer's vocabulary does not know reads as `Unknown`: kept, so
/// a daemon that later adds a permanent code cannot empty this producer's outbox.
#[test]
fn a_422_with_a_code_the_vocabulary_lacks_is_kept() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", vec![(422, json!({"code": "future_refusal"}))]),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let out = session.send("s", &["a".into()]).unwrap();
    assert!(matches!(out, Sent::Accepted { .. }), "{out:?}");
    assert_eq!(dir.join("refused").read_dir().unwrap().count(), 0);
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 1, "kept");
}

/// A refusal whose body carries no `code` reads as `Unknown`: kept.
#[test]
fn a_422_without_a_code_is_kept() {
    let f = fake(vec![
        ("/api/v1/producers/announcements/a1", vec![activated()]),
        ("/batches", vec![(422, json!({"error": "no code here"}))]),
    ]);
    let dir = state();
    let mut session = Session::open(f.door.clone(), &dir, &manifest(), None, options()).unwrap();
    let out = session.send("s", &["a".into()]).unwrap();
    assert!(matches!(out, Sent::Accepted { .. }), "{out:?}");
    assert_eq!(dir.join("refused").read_dir().unwrap().count(), 0);
    assert_eq!(dir.join("outbox").read_dir().unwrap().count(), 1, "kept");
}
