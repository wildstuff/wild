//! The routes of the ingest door, as data (ADR-0365 D1, D2, D3, D4, D7, D16;
//! ADR-0398 D4: the call wait and the answer).
//!
//! The door builds its router from [`ROUTES`], and [`crate::openapi::document`]
//! renders them, so the published OpenAPI document, the door and this list
//! never disagree. Every header a route reads is declared here
//! ([`Route::headers`]); the door reads no other.
//!
//! V1 serves every route over the profile's `ingest.sock`, and over a TCP
//! listener when the operator opens one (TLS on loopback and on the local
//! network alike; only an engineer's `WILD_INGEST_BIND` on loopback serves
//! plain). Fleet enrollment (V2) adds routes in a later, additive step.

/// An HTTP method a route answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "get",
            Method::Post => "post",
        }
    }
}

/// What a route's request carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    /// No body.
    None,
    /// A JSON body of this published schema (a [`crate::schema::documents`] name).
    Json(&'static str),
    /// An NDJSON body, one [`crate::batch::Line`] per line, optionally gzip.
    Ndjson,
}

/// One answer a route may give.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    /// The published schema of its body, or `None` for an empty body.
    pub schema: Option<&'static str>,
    pub description: &'static str,
}

/// What opens a route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Auth {
    /// No producer key yet: over the profile's `ingest.sock` (mode 0600, the
    /// operator's own user) nothing more; over the TCP listener a route that
    /// creates something takes a one-device pairing code
    /// ([`crate::device::PAIRING_CODE_HEADER`], D2, D16).
    BeforeKey,
    /// A producer key, `Authorization: Bearer wdk_<keyid>_<secret>` (D2).
    ProducerKey,
}

/// When a request must carry a header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// On every request.
    Always,
    /// Over the TCP listener; over `ingest.sock` it is not read.
    OverTcp,
    /// When the client has a value for it.
    Optional,
}

/// The value a header carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderValue {
    /// A whole number, at least 0.
    Count,
    /// One of these tokens.
    Token(&'static [&'static str]),
    /// Text of this pattern.
    Pattern(&'static str),
}

/// A request header a route reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub name: &'static str,
    pub presence: Presence,
    pub value: HeaderValue,
    pub description: &'static str,
}

/// The pairing code an announcement over TCP presents.
pub const PAIRING_CODE: Header = Header {
    name: crate::device::PAIRING_CODE_HEADER,
    presence: Presence::OverTcp,
    value: HeaderValue::Pattern("^wdp_[0-9a-f]+_[0-9a-f]+$"),
    description: "The one-device pairing code the operator made on the daemon's machine, as the pairing file's `pairing_code` carries it. It opens exactly one announcement, once, until it expires; without it, or with a used or expired code, the announcement is answered 401 `unauthenticated`.",
};

/// The highest commit mark the producer has stored.
pub const COMMIT_MARK: Header = Header {
    name: crate::device::COMMIT_MARK_HEADER,
    presence: Presence::Optional,
    value: HeaderValue::Count,
    description: "The highest commit mark this producer has stored; absent means 0. A higher one than the daemon knows is answered `daemon_rewound` before anything is received (D4a-R).",
};

/// A gzip body.
pub const CONTENT_ENCODING: Header = Header {
    name: "content-encoding",
    presence: Presence::Optional,
    value: HeaderValue::Token(&["gzip"]),
    description: "`gzip` when the body is compressed. The decoded body is held to the batch bounds, and its ratio to the compressed body to `x-wild-bounds.max_compression_ratio`.",
};

/// The vocabulary's ETag the client holds.
pub const IF_NONE_MATCH: Header = Header {
    name: "if-none-match",
    presence: Presence::Optional,
    value: HeaderValue::Pattern("^\\S+$"),
    description: "The `etag` of the vocabulary the client holds. When it is still current the answer is 304 with no body.",
};

/// A route of the ingest door.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Route {
    pub id: &'static str,
    pub method: Method,
    /// The path under [`crate::ROUTE_PREFIX`], with `{param}` segments.
    pub path: &'static str,
    pub summary: &'static str,
    pub auth: Auth,
    /// The request headers the route reads, beside `Authorization`.
    pub headers: &'static [Header],
    pub body: Body,
    pub answers: &'static [Answer],
}

const REFUSALS: [Answer; 3] = [
    Answer {
        status: 401,
        schema: Some("Refusal"),
        description: "The credential does not verify: the producer key, or before a key the pairing code over TCP. Every failure before a credential verifies is this one answer.",
    },
    Answer {
        status: 429,
        schema: Some("Refusal"),
        description: "Backpressure. Retry after `Retry-After`.",
    },
    Answer {
        status: 503,
        schema: Some("Refusal"),
        description: "Wild could not store the request durably (`unavailable`), or the operator paused this device or its producer (`paused`: the key still opens the door, and the same key delivers again once resumed). Nothing was received; retry after `Retry-After`.",
    },
];

macro_rules! answers {
    ($($a:expr),* $(,)?) => {
        &[$($a,)* REFUSALS[0], REFUSALS[1], REFUSALS[2]]
    };
}

/// Every route of contract version [`crate::CONTRACT_VERSION`].
pub const ROUTES: &[Route] = &[
    Route {
        id: "announce",
        method: Method::Post,
        path: "/producers/announcements",
        summary: "A producer announces itself with its manifest. It shows dashed in the atlas until the operator activates it (D16).",
        auth: Auth::BeforeKey,
        headers: &[PAIRING_CODE],
        body: Body::Json("Manifest"),
        answers: answers![
            Answer { status: 202, schema: Some("AnnouncementReceipt"), description: "The announcement is waiting for the operator." },
            Answer { status: 422, schema: Some("Refusal"), description: "The manifest is inconsistent, or names a contract version this daemon does not serve (`contract_unsupported`)." },
        ],
    },
    Route {
        id: "announcement_status",
        method: Method::Get,
        path: "/producers/announcements/{announcement}",
        summary: "Poll an announcement. Once activated, the reply carries the key and first epoch exactly once; every later poll returns status only (D2).",
        auth: Auth::BeforeKey,
        headers: &[],
        body: Body::None,
        answers: answers![
            Answer { status: 200, schema: Some("AnnouncementStatus"), description: "The announcement's state." },
            Answer { status: 404, schema: Some("Refusal"), description: "No such announcement on this profile." },
        ],
    },
    Route {
        id: "send_batch",
        method: Method::Post,
        path: "/tribes/{tribe}/producers/{producer}/sources/{slug}/batches",
        summary: "Send one batch. The request carries the highest commit mark the producer has stored (`wild-commit-mark`). `202` means received: Wild holds the batch durably and runs it through the source's intake flow.",
        auth: Auth::ProducerKey,
        headers: &[COMMIT_MARK, CONTENT_ENCODING],
        body: Body::Ndjson,
        answers: answers![
            Answer { status: 202, schema: Some("BatchReply"), description: "Received. Drop the batch from the outbox; its later states are for information only." },
            Answer { status: 413, schema: Some("Refusal"), description: "A body, line or field is over its bound (`over_bound`). Nothing is received." },
            Answer { status: 409, schema: Some("Refusal"), description: "The daemon was restored from a backup (`daemon_rewound`). Nothing is received; recover from `known_mark`." },
            Answer { status: 422, schema: Some("Refusal"), description: "A line fails its shape (`batch_refused`), or names an undeclared version (`shape_version_undeclared`). Nothing is received." },
        ],
    },
    Route {
        id: "batch_status",
        method: Method::Post,
        path: "/tribes/{tribe}/producers/{producer}/batches/status",
        summary: "Ask for the states of batches by id (D3): received, processed or failed. Informational; only `unknown` asks for a re-send.",
        auth: Auth::ProducerKey,
        headers: &[],
        body: Body::Json("StatusRequest"),
        answers: answers![
            Answer { status: 200, schema: Some("StatusReply"), description: "Each id's state; `unknown` means re-send it." },
        ],
    },
    Route {
        id: "new_epoch",
        method: Method::Post,
        path: "/tribes/{tribe}/producers/{producer}/device/epoch",
        summary: "Take a new random epoch for `live` records: after a fresh install. After a collision Wild assigns one itself and hands it over in the `new_epoch` directive (D4).",
        auth: Auth::ProducerKey,
        headers: &[],
        body: Body::None,
        answers: answers![
            Answer { status: 200, schema: Some("Epoch"), description: "The new epoch." },
        ],
    },
    Route {
        id: "vocabulary",
        method: Method::Get,
        path: "/tribes/{tribe}/producers/{producer}/vocabulary",
        summary: "The project and customer names an approved device matches window titles against, locally (D7). Supports `If-None-Match`.",
        auth: Auth::ProducerKey,
        headers: &[IF_NONE_MATCH],
        body: Body::None,
        answers: answers![
            Answer { status: 200, schema: Some("Vocabulary"), description: "The vocabulary, scoped to the device's person." },
            Answer { status: 304, schema: None, description: "Unchanged since the ETag given." },
        ],
    },
    Route {
        id: "call_wait",
        method: Method::Post,
        path: "/tribes/{tribe}/producers/{producer}/calls/wait",
        summary: "Wait for calls (ADR-0398 D4). Only a device whose manifest announced `tools` holds one. The door keeps the request open for up to 25 s and answers with the calls queued for this device, or with none once the time is up; open the next wait at once. A call handed out here counts as sent: answer each one on `call_answer`.",
        auth: Auth::ProducerKey,
        headers: &[],
        body: Body::None,
        answers: answers![
            Answer { status: 200, schema: Some("Calls"), description: "The calls queued for this device, oldest first; none when the wait ran out. Open the next wait." },
            Answer { status: 409, schema: Some("Refusal"), description: "This device's manifest announced no tools, so no call ever reaches it (`no_tools`)." },
        ],
    },
    Route {
        id: "call_answer",
        method: Method::Post,
        path: "/tribes/{tribe}/producers/{producer}/calls/answers",
        summary: "Answer one call a call wait handed out, by its `call`, with the result as JSON (ADR-0398 D4).",
        auth: Auth::ProducerKey,
        headers: &[],
        body: Body::Json("CallAnswer"),
        answers: answers![
            Answer { status: 202, schema: None, description: "The answer reached Wild." },
            Answer { status: 404, schema: Some("Refusal"), description: "No call of this device waits for this answer (`no_such_call`): it was answered already, Wild stopped waiting for it, or it was handed to another device. Drop the answer." },
        ],
    },
];
