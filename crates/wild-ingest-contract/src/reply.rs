//! Every reply the ingest door gives (ADR-0365 D0, D1, D3, D4, D4a-R).
//!
//! **How a producer reads a reply.** Every reply names the profile it came
//! from and the device's current commit mark. A batch is dropped from the
//! outbox once its state reads [`BatchState::Received`] or later: from then on
//! Wild holds it durably, as it holds a file in a local folder, and runs it
//! through the source's intake flow. [`BatchState::Expired`] drops it too:
//! Wild held it and has removed it. [`BatchState::Unknown`], and any state
//! this contract version does not know, means keep the batch and back off
//! (D1b).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::batch::{BatchId, RejectedLine};
use crate::codes::{ConflictCode, RefusalCode};

/// Where a batch stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum BatchState {
    /// Wild holds the batch durably and will run it through the source's
    /// flow. Drop it from the outbox.
    Received,
    /// The flow ran it through the sink. Lines Wild did not write as sent are
    /// listed as conflicts. For information only.
    Processed,
    /// The flow failed the batch. Wild keeps it in its dead-letter, tells the
    /// operator, and retries it from its receipt. For information only;
    /// nothing to re-send.
    Failed,
    /// Wild held the batch and removed it together with its rows: its
    /// source's retention ended, or the operator erased or retracted them.
    /// Drop it from the outbox and never send it again; a re-send is answered
    /// `expired` and writes nothing.
    Expired,
    /// The door does not know the batch, or a state this contract version
    /// does not know. Keep it; re-send it.
    #[serde(other)]
    Unknown,
}

/// One line of a received batch that the source's flow did not write as
/// sent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Conflict {
    /// The line's number in the batch body, counting from 1.
    pub line: u32,
    pub code: ConflictCode,
    #[serde(default)]
    pub params: Map<String, Value>,
}

/// What the door tells a device on every reply (D1a): changes it should act
/// on without a second connection.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Directives {
    /// The copy of the vocabulary you hold is no longer the one the door
    /// serves: fetch it again (D7). Sent only to a producer that announced
    /// `needs_vocabulary: true` and has fetched the vocabulary at least
    /// once; fetching the current copy stops it.
    #[serde(default)]
    pub vocabulary_changed: bool,
    /// The operator paused this producer; keep collecting, send nothing.
    #[serde(default)]
    pub paused: bool,
    /// Use this epoch for the next `live` records: Wild assigned it after a
    /// collision with a restored copy's counter (D4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = r"^[0-9a-f]{32}$"))]
    pub new_epoch: Option<String>,
    /// Rotate to this key; the old one verifies until the overlap ends (D2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate_key: Option<String>,
    /// Send no more often than this many seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub send_interval_secs: Option<u32>,
}

/// The reply to a batch: `202`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BatchReply {
    /// The profile that answered (D1).
    pub profile: String,
    pub batch: BatchId,
    pub state: BatchState,
    /// The device's commit mark after this request: a counter of batches Wild
    /// has received (D4a-R).
    pub commit_mark: u64,
    /// The states of the device's oldest pending batches, so a device rarely
    /// has to ask (D3).
    #[serde(default)]
    pub pending: Vec<BatchStatus>,
    #[serde(default)]
    pub directives: Directives,
}

/// The status route's request: the ids to report on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatusRequest {
    pub ids: Vec<BatchId>,
}

/// The status route's reply.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StatusReply {
    pub profile: String,
    pub commit_mark: u64,
    pub statuses: Vec<BatchStatus>,
    #[serde(default)]
    pub directives: Directives,
}

/// One batch's state, as the status route reports it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BatchStatus {
    pub batch: BatchId,
    pub state: BatchState,
    #[serde(default)]
    pub conflicts: Vec<Conflict>,
}

/// A refused request: `4xx`/`5xx` with an error body. Wild keeps nothing
/// of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Refusal {
    /// A sentence for a human; never decided on.
    pub error: String,
    pub code: RefusalCode,
    #[serde(default)]
    pub params: Map<String, Value>,
    /// For `batch_refused`: every rejected line.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rejected_lines: Vec<RejectedLine>,
    /// The profile that answered, when the request got that far.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
}
