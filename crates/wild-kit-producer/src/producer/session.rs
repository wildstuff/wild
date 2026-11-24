//! A producer's whole connection to one door, from its announcement to its
//! batches and the door's answers.
//!
//! [`Session::open`] announces the producer — or resumes from the state it
//! kept — and waits for the operator's yes. [`Session::send`] then writes
//! every batch through the outbox, obeys the door's directives, and notices
//! a revocation. What a producer keeps beyond the contract — the source it
//! reads, how far it got — reaches the session through [`Options::store`],
//! which runs between the outbox and the POST.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use wild_ingest_contract::batch::BatchId;
use wild_ingest_contract::manifest::Manifest;
use wild_ingest_contract::pairing_file::PairingFile;
use wild_ingest_contract::shape::{FieldDecl, FieldKind, KeyClass, ShapeDecl};

use super::calls;
use super::door::Door;
use super::http::request;
use super::write_owner_only;

/// How long it rests after a reply that kept a batch.
const BACKOFF: Duration = Duration::from_millis(200);
/// The longest pause between two questions about the announcement.
const ANSWER_PAUSE_MAX: Duration = Duration::from_secs(30);
/// The rounds a pause is asked after before it is left for the next start.
const PAUSE_ROUNDS: usize = 100;

/// The shape [`Session::send`] appends to every batch. It is declared in the
/// announcement beside what the producer declared itself, so a door that
/// checks every line against the announced manifest takes the tick too. Its
/// `live` key parts are reserved: `_epoch` comes from the session, `_seq` and
/// the rest are the batch's own numbers.
#[must_use]
pub fn tick_shape() -> ShapeDecl {
    fn number(name: &str, description: &str) -> FieldDecl {
        FieldDecl {
            name: name.into(),
            kind: FieldKind::Number,
            required: true,
            description: Some(description.into()),
        }
    }
    ShapeDecl {
        name: "tick".into(),
        version: 1,
        key_class: KeyClass::Live,
        key: Vec::new(),
        fields: vec![
            number("_seq", "The line's number on the device, counting from 0."),
            number("batch", "The batch's number, counting from 0."),
            number("lines", "How many lines the batch carries before the tick."),
        ],
        target_type: "Tick".into(),
        description: Some(
            "One batch the producer made, placed last, carrying how many lines it read.".into(),
        ),
    }
}

/// How the session is told to behave: the knobs the kit's reference producer
/// exposes as `--break`, and the ones every producer has.
#[derive(Default)]
pub struct Options {
    /// Where the key lives: apart from the state when given.
    pub keys: Option<PathBuf>,
    /// Keep the pairing file after its code is spent.
    pub keep_pairing_file: bool,
    /// Announce afresh at every start, though a key is kept.
    pub announce_every_start: bool,
    /// Give up waiting for the operator's yes after a while.
    pub give_up_waiting: bool,
    /// Ignore the `vocabulary_changed` directive.
    pub ignore_vocabulary: bool,
    /// Ignore the `new_epoch` directive.
    pub ignore_new_epoch: bool,
    /// Keep sending after a `401`.
    pub ignore_revoked: bool,
    /// Take any answer for `daemon_rewound`.
    pub ignore_rewound: bool,
    /// Send again every kept batch on a rewind, not only those above it.
    pub rewind_everything: bool,
    /// Drop a batch answered `unknown`.
    pub drop_unknown: bool,
    /// Send a lost batch's lines in another order.
    pub rebatch_lost: bool,
    /// Count the batch as made before it is anywhere but in memory.
    pub send_before_store: bool,
    /// Ask the door about kept batches and send again those it reports
    /// expired.
    pub resend_expired: bool,
    /// Fetch no vocabulary and announce needing none.
    pub no_vocabulary: bool,
    /// Where the epoch is kept: beside the key when the producer keeps the
    /// two together.
    pub epoch_file: Option<PathBuf>,
    /// Written between the outbox and the POST, with the bytes read and the
    /// batch number: where the producer keeps how far it got.
    pub store: Option<Store>,
}

/// A producer's own progress store, called with the bytes it read and the
/// batch it is about to send — or, when no batch went out, the index the
/// next one takes.
pub type Store = Box<dyn FnMut(&[u8], usize) -> io::Result<()>>;

/// What a send left the producer.
#[must_use]
#[derive(Debug)]
#[non_exhaustive]
pub enum Sent {
    /// The door took the batch: wait this long before the next round.
    Accepted { wait: Duration },
    /// The door paused the producer; it asks after the pause and stays.
    Paused(Duration),
    /// The door cut the producer off: it sends nothing more.
    Revoked,
}

impl Sent {
    /// How long the send says to wait before the next round.
    pub fn wait(&self) -> Duration {
        match self {
            Sent::Accepted { wait } | Sent::Paused(wait) => *wait,
            Sent::Revoked => Duration::ZERO,
        }
    }
}

/// One producer's live connection to its door.
pub struct Session {
    door: Door,
    state: PathBuf,
    base: String,
    key: calls::Key,
    tribe: String,
    announcement: String,
    epoch: String,
    source: String,
    made: usize,
    pairing: Option<PathBuf>,
    options: Options,
    paused: bool,
    revoked: bool,
    interval: Option<Duration>,
    last_send: Option<Instant>,
}

impl Session {
    /// Announce, or resume from the state, and wait for the operator's yes.
    pub fn open(
        door: Door,
        state: &Path,
        manifest: &Manifest,
        pairing: Option<&Path>,
        options: Options,
    ) -> io::Result<Session> {
        fs::create_dir_all(state.join("outbox"))?;
        fs::create_dir_all(state.join("journal"))?;
        fs::create_dir_all(keys_of(&options, state))?;
        let mut session = Session {
            door,
            state: state.to_path_buf(),
            base: format!(
                "/api/v1/tribes/{}/producers/{}",
                manifest.tribe.as_deref().unwrap_or_default(),
                manifest.producer
            ),
            key: calls::Key::default(),
            tribe: manifest.tribe.clone().unwrap_or_default(),
            announcement: String::new(),
            epoch: String::new(),
            source: manifest
                .sources
                .first()
                .map_or_else(String::new, |s| s.slug.clone()),
            made: fs::read_to_string(state.join("made"))
                .ok()
                .and_then(|n| n.trim().parse().ok())
                .unwrap_or(0),
            pairing: pairing.map(Path::to_path_buf),
            options,
            paused: false,
            revoked: false,
            interval: None,
            last_send: None,
        };
        let revoked = keys_of(&session.options, state).join("revoked");
        if revoked.exists() {
            println!(
                "revoked earlier: this install sends nothing; only a new announcement pairs it anew"
            );
            session.revoked = true;
            return Ok(session);
        }
        let mut announced = manifest.clone();
        if let Some(source) = announced.sources.first_mut() {
            source.shapes.push(tick_shape());
        }
        session.take_key(&announced)?;
        session.base = format!(
            "/api/v1/tribes/{}/producers/{}",
            session.tribe, manifest.producer
        );
        session.epoch = session.take_epoch()?;
        // ADR-0365 D1a / D7 — take the vocabulary once at every start, before
        // the first batch. A device that has never fetched is owed no
        // `vocabulary_changed`, so skipping this would make the directive
        // unreachable forever.
        if !session.options.no_vocabulary {
            session.fetch_vocabulary("vocabulary")?;
        }
        if session.options.resend_expired {
            session.resend_expired()?;
        }
        Ok(session)
    }

    /// The key the call half presents.
    pub fn key(&self) -> &calls::Key {
        &self.key
    }

    /// The base the call half and the batches are reached under.
    pub fn base(&self) -> &str {
        &self.base
    }

    /// The epoch its `live` records carry.
    pub fn epoch(&self) -> &str {
        &self.epoch
    }

    /// How many batches it has made.
    pub fn made(&self) -> usize {
        self.made
    }

    /// Whether the outbox holds no batch: nothing was left from an earlier
    /// start.
    pub fn outbox_empty(&self) -> bool {
        self.outbox().map_or(true, |out| out.is_empty())
    }

    /// Told `revoked`: it sends nothing more.
    pub fn revoked(&self) -> bool {
        self.revoked
    }

    /// Store progress though no batch went out (a source that moved without
    /// a line read): `made` is the index the next batch takes.
    pub fn store(&mut self, read: &[u8], made: usize) -> io::Result<()> {
        if let Some(store) = &mut self.options.store {
            store(read, made)?;
        }
        Ok(())
    }

    /// Make one batch of `lines` and send what the outbox holds.
    pub fn send(&mut self, source: &str, lines: &[String]) -> io::Result<Sent> {
        if self.revoked {
            return Ok(Sent::Revoked);
        }
        if self.paused {
            return Ok(Sent::Paused(Duration::ZERO));
        }
        self.source = source.to_string();
        let mut sent = None;
        if !lines.is_empty() {
            let n = self.made;
            let tick = tick_shape().shape_ref().to_string();
            let mut records = lines.to_vec();
            records.push(
                json!({"shape": tick, "record": {
                    "_epoch": self.epoch, "_seq": n, "batch": n, "lines": lines.len(),
                }})
                .to_string(),
            );
            let body = records.join("\n") + "\n";
            let file = self.state.join("outbox").join(format!("{n:04}.ndjson"));
            if self.options.send_before_store {
                // The mistake: the batch counts as made before it is anywhere
                // but in memory.
                self.stored(body.as_bytes(), n)?;
                self.flush_made(n + 1)?;
                let (one, drop) = self.send_body(&file, body.as_bytes())?;
                let wait = one.wait();
                sent = Some(one);
                if !drop {
                    // Kept: it goes back into the outbox and is tried again
                    // in this round, after the wait the reply gave.
                    write_owner_only(&file, body.as_bytes())?;
                    if !self.paused && !self.revoked {
                        std::thread::sleep(wait);
                    }
                }
            } else {
                // The outbox first, then the progress it was made from: a
                // kill between the two makes the same batch from the same
                // lines.
                write_owner_only(&file, body.as_bytes())?;
                self.stored(body.as_bytes(), n)?;
                self.flush_made(n + 1)?;
            }
            if self.paused || self.revoked {
                let out = sent.unwrap_or(Sent::Paused(BACKOFF));
                std::thread::sleep(out.wait());
                return Ok(out);
            }
        } else {
            self.resend_expired()?;
        }
        let mut pending = self.outbox()?;
        pending.sort();
        for file in pending {
            if self.paused || self.revoked {
                break;
            }
            sent = Some(self.send_file(&file)?);
        }
        let out = sent.unwrap_or(Sent::Accepted {
            wait: Duration::ZERO,
        });
        std::thread::sleep(out.wait());
        Ok(out)
    }

    /// Send one batch already in the outbox.
    pub fn send_file(&mut self, file: &Path) -> io::Result<Sent> {
        let body = fs::read(file)?;
        let (sent, drop) = self.send_body(file, &body)?;
        if drop {
            fs::remove_file(file)?;
        }
        Ok(sent)
    }

    /// Ask the status route until a pause ends.
    pub fn wait_out_pause(&mut self) -> io::Result<()> {
        println!("paused");
        let path = format!("{}/batches/status", self.base);
        for _ in 0..PAUSE_ROUNDS {
            std::thread::sleep(BACKOFF);
            let auth = format!("Bearer {}", self.key.get());
            let headers = [
                ("authorization", auth.as_str()),
                ("content-type", "application/json"),
            ];
            if let Ok((200, _, reply)) =
                request(&self.door, "POST", &path, &headers, br#"{"ids":[]}"#)
            {
                self.obey(&reply["directives"])?;
                if !self.paused {
                    println!("resumed");
                    return Ok(());
                }
            }
        }
        Ok(())
    }

    /// The epoch for its `live` records: the one kept with its state, or,
    /// started without its state, a new one it asks the door for (ADR-0365
    /// D4).
    fn take_epoch(&mut self) -> io::Result<String> {
        let kept = self.epoch_path();
        if let Ok(e) = fs::read_to_string(&kept) {
            return Ok(e.trim().to_string());
        }
        let auth = format!("Bearer {}", self.key.get());
        let path = format!("{}/device/epoch", self.base);
        let headers = [("authorization", auth.as_str())];
        match request(&self.door, "POST", &path, &headers, &[])? {
            (200, _, reply) => {
                let e = reply["epoch"]
                    .as_str()
                    .ok_or_else(|| io::Error::other(format!("no epoch in {reply}")))?;
                println!(
                    "no state of its own: asked for a new epoch, {}",
                    super::printable(e)
                );
                write_owner_only(&kept, e.as_bytes())?;
                Ok(e.to_string())
            }
            (status, _, reply) => Err(io::Error::other(format!(
                "a new epoch was refused: {status} {reply}"
            ))),
        }
    }

    /// The producer key and its tribe: kept from an earlier start, or
    /// announced for and waited on.
    fn take_key(&mut self, manifest: &Manifest) -> io::Result<()> {
        let kept = keys_of(&self.options, &self.state).join("key.json");
        let fresh = self.options.announce_every_start;
        if let Some(text) = fs::read_to_string(&kept).ok().filter(|_| !fresh) {
            let v: Value = serde_json::from_str(&text).map_err(io::Error::other)?;
            if let (Some(k), Some(t)) = (v["key"].as_str(), v["tribe"].as_str()) {
                self.key = calls::Key::new(k.into());
                self.tribe = t.into();
                self.announcement = v["announcement"].as_str().unwrap_or_default().into();
                self.let_go_of_pairing()?;
                return Ok(());
            }
        }
        let id = match fs::read_to_string(self.state.join("announcement"))
            .ok()
            .filter(|_| !fresh)
        {
            Some(id) => id.trim().to_string(),
            None => {
                let mut headers = vec![("content-type", "application/json")];
                let code = self.pairing_code()?;
                if let Some(code) = &code {
                    headers.push(("wild-pairing-code", code.as_str()));
                }
                let body = serde_json::to_string(manifest).map_err(io::Error::other)?;
                let (status, _, reply) = request(
                    &self.door,
                    "POST",
                    "/api/v1/producers/announcements",
                    &headers,
                    body.as_bytes(),
                )?;
                let id = reply["announcement"]
                    .as_str()
                    .ok_or_else(|| {
                        io::Error::other(format!("announcement answered {status}: {reply}"))
                    })?
                    .to_string();
                write_owner_only(&self.state.join("announcement"), id.as_bytes())?;
                id
            }
        };
        self.let_go_of_pairing()?;
        // The yes is a person's: it may come minutes or days later. Ask on,
        // each pause twice the last up to `ANSWER_PAUSE_MAX`, and never give
        // up on our own — only `revoked` or `rejected` end the wait.
        let mut pause = BACKOFF;
        let mut said = false;
        let mut asked = 0;
        loop {
            asked += 1;
            if self.options.give_up_waiting && asked > 100 {
                return Err(io::Error::other(
                    "no key handed over; is the producer activated?",
                ));
            }
            let path = format!("/api/v1/producers/announcements/{id}");
            if let Ok((200, _, reply)) = request(&self.door, "GET", &path, &[], &[]) {
                if let (Some(k), Some(t)) =
                    (reply["producer_key"].as_str(), reply["tribe"].as_str())
                {
                    let epoch = reply["epoch"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("the key came without its first epoch"))?;
                    write_owner_only(&self.epoch_path(), epoch.as_bytes())?;
                    let v = json!({"key": k, "tribe": t, "announcement": id});
                    write_owner_only(&kept, v.to_string().as_bytes())?;
                    self.key = calls::Key::new(k.into());
                    self.tribe = t.into();
                    self.announcement = id;
                    return Ok(());
                }
                if reply["state"] == "revoked" || reply["state"] == "rejected" {
                    return Err(io::Error::other(format!(
                        "the announcement reads `{}`: this install pairs anew only by a new announcement",
                        reply["state"].as_str().unwrap_or_default()
                    )));
                }
                if !said {
                    eprintln!(
                        "producer: announced as {}; waiting for the operator's yes",
                        super::printable(&id)
                    );
                    said = true;
                }
            }
            std::thread::sleep(pause);
            if !self.options.give_up_waiting {
                pause = (pause * 2).min(ANSWER_PAUSE_MAX);
            }
        }
    }

    /// The pairing code, read from the pairing file it was started with.
    fn pairing_code(&self) -> io::Result<Option<String>> {
        let Some(file) = &self.pairing else {
            return Ok(None);
        };
        // A start that announces every time finds no file once its code is
        // spent, and announces without one.
        let text = match fs::read_to_string(file) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let kept: PairingFile = serde_json::from_str(&text).map_err(io::Error::other)?;
        Ok(Some(kept.pairing_code))
    }

    /// Remove the pairing file once its code is spent.
    fn let_go_of_pairing(&self) -> io::Result<()> {
        let Some(file) = self
            .pairing
            .as_ref()
            .filter(|_| !self.options.keep_pairing_file)
        else {
            return Ok(());
        };
        match fs::remove_file(file) {
            Ok(()) => {
                println!("pairing file removed: its code is spent");
                Ok(())
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Send one batch body: whether it may leave the outbox, and what the
    /// send left the producer.
    fn send_body(&mut self, file: &Path, body: &[u8]) -> io::Result<(Sent, bool)> {
        if let (Some(every), Some(last)) = (self.interval, self.last_send) {
            std::thread::sleep(every.saturating_sub(last.elapsed()));
        }
        let mark = fs::read_to_string(self.state.join("commit-mark")).unwrap_or_default();
        let mark = mark.trim().to_string();
        let path = format!("{}/sources/{}/batches", self.base, self.source);
        let answer = {
            let auth = format!("Bearer {}", self.key.get());
            let mut headers = vec![("authorization", auth.as_str())];
            if !mark.is_empty() {
                headers.push(("wild-commit-mark", mark.as_str()));
            }
            request(&self.door, "POST", &path, &headers, body)
        };
        self.last_send = Some(Instant::now());
        match answer {
            Ok((202, _, reply)) => {
                let told = reply["state"].as_str().unwrap_or("unknown");
                if let Some(m) = reply["commit_mark"].as_u64() {
                    if m > mark.parse().unwrap_or(0) {
                        write_owner_only(
                            &self.state.join("commit-mark"),
                            m.to_string().as_bytes(),
                        )?;
                    }
                }
                println!("{}: {}", name(file), super::printable(told));
                let drop = matches!(told, "received" | "processed" | "failed" | "expired")
                    || self.options.drop_unknown;
                if drop {
                    // Kept by the mark it took, to send again should Wild be
                    // rewound below it.
                    let m = reply["commit_mark"].as_u64().unwrap_or(0);
                    let entry = self
                        .state
                        .join("journal")
                        .join(format!("{m:08}-{}", name(file)));
                    write_owner_only(&entry, body)?;
                }
                self.obey(&reply["directives"])?;
                let wait = if drop { Duration::ZERO } else { BACKOFF };
                if self.revoked {
                    return Ok((Sent::Revoked, drop));
                }
                let sent = if self.paused {
                    Sent::Paused(wait)
                } else {
                    Sent::Accepted { wait }
                };
                Ok((sent, drop))
            }
            Ok((409, _, reply))
                if reply["code"] == "daemon_rewound" && !self.options.ignore_rewound =>
            {
                let known = reply["params"]["known_mark"].as_u64().unwrap_or(0);
                let again = self.rewound(known)?;
                println!(
                    "{}: daemon_rewound to mark {known}; {again} batches above it go out again",
                    name(file)
                );
                Ok((
                    Sent::Accepted {
                        wait: Duration::ZERO,
                    },
                    false,
                ))
            }
            Ok((401, _, _)) if !self.options.ignore_revoked => {
                self.revoked = self.announcement_revoked()?;
                println!("{}: refused 401; kept", name(file));
                if self.revoked {
                    write_owner_only(&keys_of(&self.options, &self.state).join("revoked"), b"")?;
                    return Ok((Sent::Revoked, false));
                }
                Ok((Sent::Accepted { wait: BACKOFF }, false))
            }
            Ok((status, retry_after, reply)) => {
                println!(
                    "{}: refused {status} {}; kept",
                    name(file),
                    super::printable(reply["code"].as_str().unwrap_or(""))
                );
                Ok((
                    Sent::Accepted {
                        wait: retry_after.unwrap_or(BACKOFF),
                    },
                    false,
                ))
            }
            Err(e) => {
                println!(
                    "{}: no reply ({}); kept",
                    name(file),
                    super::printable(&e.to_string())
                );
                if self.options.rebatch_lost {
                    // The mistake: the same lines, in another order, are
                    // another batch.
                    let text = String::from_utf8_lossy(body);
                    let mut lines: Vec<&str> = text.lines().collect();
                    lines.reverse();
                    write_owner_only(file, (lines.join("\n") + "\n").as_bytes())?;
                }
                Ok((Sent::Accepted { wait: BACKOFF }, false))
            }
        }
    }

    /// Wild was rewound to `known`: every batch a reply above it took goes
    /// back into the outbox, and `known` is the mark presented from now on.
    /// How many went back.
    fn rewound(&mut self, known: u64) -> io::Result<usize> {
        let mut again = 0;
        for e in fs::read_dir(self.state.join("journal"))? {
            let path = e?.path();
            let entry = name(&path);
            let Some((mark, file)) = entry.split_once('-') else {
                continue;
            };
            if mark
                .parse::<u64>()
                .is_ok_and(|m| m > known || self.options.rewind_everything)
            {
                fs::rename(&path, self.state.join("outbox").join(file))?;
                again += 1;
            }
        }
        write_owner_only(
            &self.state.join("commit-mark"),
            known.to_string().as_bytes(),
        )?;
        Ok(again)
    }

    /// Ask the status route about the batches it kept and send again those
    /// reported `expired`.
    fn resend_expired(&mut self) -> io::Result<()> {
        let mut kept = Vec::new();
        for e in fs::read_dir(self.state.join("journal"))? {
            let path = e?.path();
            let body = fs::read(&path)?;
            kept.push((BatchId::of(&body).0, path));
        }
        if kept.is_empty() {
            return Ok(());
        }
        let ids: Vec<&String> = kept.iter().map(|(id, _)| id).collect();
        let auth = format!("Bearer {}", self.key.get());
        let headers = [
            ("authorization", auth.as_str()),
            ("content-type", "application/json"),
        ];
        let path = format!("{}/batches/status", self.base);
        let ask = json!({ "ids": ids }).to_string();
        let (200, _, reply) = request(&self.door, "POST", &path, &headers, ask.as_bytes())? else {
            return Ok(());
        };
        for status in reply["statuses"].as_array().into_iter().flatten() {
            if status["state"] != "expired" {
                continue;
            }
            let Some((_, path)) = kept.iter().find(|(id, _)| status["batch"] == *id) else {
                continue;
            };
            let entry = name(path);
            let file = entry.split_once('-').map_or(entry.as_str(), |(_, f)| f);
            fs::rename(path, self.state.join("outbox").join(file))?;
            println!("{entry}: reported expired; goes out again");
        }
        Ok(())
    }

    /// Whether its announcement now reads `revoked`: the door no longer
    /// opens to its key because the device was cut off or continued
    /// elsewhere.
    fn announcement_revoked(&self) -> io::Result<bool> {
        if self.announcement.is_empty() {
            return Ok(false);
        }
        let path = format!("/api/v1/producers/announcements/{}", self.announcement);
        Ok(matches!(
            request(&self.door, "GET", &path, &[], &[]),
            Ok((200, _, reply)) if reply["state"] == "revoked"
        ))
    }

    /// Take the directives of a reply.
    fn obey(&mut self, directives: &Value) -> io::Result<()> {
        if let Some(k) = directives["rotate_key"].as_str() {
            if k != self.key.get() {
                println!("rotate_key: taking the new key");
                let file = keys_of(&self.options, &self.state).join("key.json");
                let mut kept: Value =
                    serde_json::from_str(&fs::read_to_string(&file)?).map_err(io::Error::other)?;
                kept["key"] = json!(k);
                write_owner_only(&file, kept.to_string().as_bytes())?;
                self.key.set(k.to_string());
            }
        }
        if directives["vocabulary_changed"].as_bool() == Some(true)
            && !self.options.ignore_vocabulary
            && !self.options.no_vocabulary
        {
            self.fetch_vocabulary("vocabulary_changed")?;
        }
        if let Some(e) = directives["new_epoch"].as_str() {
            if e != self.epoch && !self.options.ignore_new_epoch {
                println!("new_epoch: {}", super::printable(e));
                write_owner_only(&self.epoch_path(), e.as_bytes())?;
                self.epoch = e.to_string();
            }
        }
        self.paused = directives["paused"].as_bool().unwrap_or(false);
        self.interval = directives["send_interval_secs"]
            .as_u64()
            .map(Duration::from_secs);
        Ok(())
    }

    /// Fetch the vocabulary, presenting the ETag held. It has no reference
    /// fields, so it only keeps the ETag.
    fn fetch_vocabulary(&mut self, why: &str) -> io::Result<()> {
        let kept = self.state.join("vocabulary-etag");
        let held = fs::read_to_string(&kept).unwrap_or_default();
        let path = format!("{}/vocabulary", self.base);
        let answer = {
            let auth = format!("Bearer {}", self.key.get());
            let mut headers = vec![("authorization", auth.as_str())];
            if !held.is_empty() {
                headers.push(("if-none-match", held.as_str()));
            }
            request(&self.door, "GET", &path, &headers, &[])
        };
        match answer {
            Ok((200, _, reply)) => {
                let etag = reply["etag"].as_str().unwrap_or_default();
                println!(
                    "{why}: fetched the vocabulary, etag {}",
                    super::printable(etag)
                );
                write_owner_only(&kept, etag.as_bytes())
            }
            Ok((status, _, _)) => {
                println!("{why}: the vocabulary answered {status}");
                Ok(())
            }
            Err(e) => {
                println!("{why}: no reply ({e})");
                Ok(())
            }
        }
    }

    /// Call the store hook and keep the batch number.
    fn stored(&mut self, read: &[u8], made: usize) -> io::Result<()> {
        if let Some(store) = &mut self.options.store {
            store(read, made)?;
        }
        Ok(())
    }

    /// Write the made count.
    fn flush_made(&mut self, made: usize) -> io::Result<()> {
        self.made = made;
        write_owner_only(&self.state.join("made"), made.to_string().as_bytes())
    }

    /// Where the epoch is kept: the producer's own file when it named one.
    fn epoch_path(&self) -> PathBuf {
        self.options
            .epoch_file
            .clone()
            .unwrap_or_else(|| self.state.join("epoch"))
    }

    /// The batches the outbox holds.
    fn outbox(&self) -> io::Result<Vec<PathBuf>> {
        let mut out = Vec::new();
        for e in fs::read_dir(self.state.join("outbox"))? {
            out.push(e?.path());
        }
        Ok(out)
    }
}

#[cfg(all(test, unix))]
#[path = "session_tests.rs"]
mod tests;

fn keys_of(options: &Options, state: &Path) -> PathBuf {
    options.keys.clone().unwrap_or_else(|| state.to_path_buf())
}

/// The file name of a batch.
fn name(file: &Path) -> String {
    file.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}
