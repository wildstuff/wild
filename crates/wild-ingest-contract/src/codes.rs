//! The closed sets of codes the ingest door answers with (ADR-0365 D0, D1, D4,
//! D4a-R, D1b).
//!
//! Every code rides an ADR-0361 body: `{error, code, params}`. The sentence in
//! `error` is the fallback for a human; a producer decides on `code` alone.
//!
//! Both sets end in `Unknown`: a code a later daemon adds deserialises to it, so
//! an older producer never fails to read a reply. A producer treats `Unknown`
//! as "not received: keep the batch and back off" (D1b).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Why the door refused a whole request. Wild keeps nothing of a refused
/// request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum RefusalCode {
    /// A line fails its declared shape. The reply lists the rejected lines;
    /// the producer fixes them and re-sends.
    BatchRefused,
    /// A body, a line or a field is over its bound (HTTP 413).
    OverBound,
    /// A line names a shape version the source does not declare, or one it
    /// has retired.
    ShapeVersionUndeclared,
    /// The manifest's contract version is not served. `params.supported`
    /// lists the versions that are.
    ContractUnsupported,
    /// The producer presented a commit mark higher than the daemon's: the
    /// daemon was restored from a backup. `params` is a
    /// `Rewound`: what it still knows, and how many live
    /// lines of this device it still holds (D4a-R).
    DaemonRewound,
    /// The device's received batches not yet processed are over its cap;
    /// keep the batch and retry after `Retry-After` (HTTP 429).
    Backpressure,
    /// The key does not verify, or opens no route of this request. Every
    /// failure before a key verifies is this one code (HTTP 401).
    Unauthenticated,
    /// Wild could not store the batch durably, so it received nothing. Keep
    /// the batch and retry after `Retry-After` (HTTP 503).
    Unavailable,
    /// The operator paused this device, or its whole producer. Nothing was
    /// received: keep the batch and retry after `Retry-After` (HTTP 503,
    /// `Retry-After: 60`). The key still opens the door, so the
    /// status route answers, with the `paused` directive, and once the
    /// operator resumes it the same key delivers again: no new pairing, no
    /// new key, no new epoch. A revoked key is `unauthenticated` instead.
    Paused,
    /// No route of the contract answers this method and path (HTTP 404; 405
    /// for a route's path asked with another method). `params` holds the
    /// `method` and `path` asked. Nothing was received: fix the request, as
    /// sending it again unchanged reaches nothing.
    NoSuchRoute,
    /// A call wait from a device whose manifest announced no tools: no call
    /// ever reaches it (HTTP 409, ADR-0398 D4). Announce the tools, or do
    /// not wait.
    NoTools,
    /// An announcement claims a tool or a value another device of the same
    /// tribe already answers for (HTTP 409, ADR-0398 D3). Nothing was
    /// activated: the announcement stays pending, and the device claims no
    /// value. `params` names the two connections: the `holder` device id
    /// with `holder_connection`, and the `announcing_connection` that asked.
    // INTERIM: #14085 — the code itself is the builder's answer to a product
    // choice (an additive refusal inside v1, rather than holding at an honest
    // `pending`), and the sentence beside it is the door's own English. Both
    // stand until the owner picks; then this code stays, changes, or goes.
    ToolClaimed,
    /// An answer to a call no wait handed this device, or one already
    /// answered or no longer awaited (HTTP 404, ADR-0398 D4). Drop it.
    NoSuchCall,
    /// A code this contract version does not know.
    #[serde(other)]
    Unknown,
}

/// The `Retry-After` the door sends with [`RefusalCode::Paused`]: a pause
/// lasts until the operator lifts it, so a paused device asks again no more
/// often than this.
pub const PAUSED_RETRY_AFTER_SECS: u32 = 60;

/// The `Retry-After` the door sends with [`RefusalCode::Backpressure`]: the
/// device's backlog shrinks as its flow takes batches over, a batch or so a
/// second, so a full device asks again after a few of them.
pub const BACKPRESSURE_RETRY_AFTER_SECS: u32 = 5;

/// The `Retry-After` the door sends with [`RefusalCode::Unavailable`]: the
/// store failed to keep the batch, and a retry at once mostly meets the same
/// failure.
pub const UNAVAILABLE_RETRY_AFTER_SECS: u32 = 5;

impl RefusalCode {
    /// The seconds a producer waits before it sends a batch refused with this
    /// code again: every code that says "retry after `Retry-After`" has one,
    /// every other code none.
    pub fn retry_after_secs(self) -> Option<u32> {
        match self {
            RefusalCode::Paused => Some(PAUSED_RETRY_AFTER_SECS),
            RefusalCode::Backpressure => Some(BACKPRESSURE_RETRY_AFTER_SECS),
            RefusalCode::Unavailable => Some(UNAVAILABLE_RETRY_AFTER_SECS),
            _ => None,
        }
    }
}

/// What the source's flow did with one line of a received batch instead of
/// writing it as sent. The other lines are written as sent (D0, D4). Every
/// code is information: none asks the producer to act or to re-send.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConflictCode {
    /// A `natural` key already holds other values: the source changed its
    /// own history. The line is not written; Wild counts it on the device
    /// record and shows it in the inbox (D4).
    SourceRewroteHistory,
    /// A `position` id already held other bytes — a restored file reached an
    /// offset sent before. Wild wrote the line under a generation it derived
    /// from the line itself (D4).
    GenerationReassigned,
    /// A `live` id already held other values — a restored copy reused an old
    /// counter. Wild wrote the line under an epoch it derived from the line
    /// itself, and the reply's `new_epoch` directive hands the producer a
    /// fresh epoch for its next records (D4).
    EpochReassigned,
    /// A code this contract version does not know.
    #[serde(other)]
    Unknown,
}

/// Why one line was rejected before the batch was received (D1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LineCode {
    /// The line is not one JSON object.
    NotAnObject,
    /// The line names no `shape@version`, or one the source does not declare.
    ShapeUnknown,
    /// A field the shape does not declare.
    FieldUndeclared,
    /// A field the shape requires is missing.
    FieldMissing,
    /// A field's value is not of the declared kind.
    FieldWrongKind,
    /// A field's string is longer than its cap.
    FieldTooLong,
    /// The line is longer than the line bound.
    LineTooLong,
    /// A code this contract version does not know.
    #[serde(other)]
    Unknown,
}
