//! Epochs, the commit mark and the vocabulary (ADR-0365 D2, D4, D4a-R, D7).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A new epoch for a producer's `live` records: a random 128-bit value as 32
/// lowercase hex digits, never a counter, so a daemon restored from a backup can
/// never hand out an epoch it issued before (D4a-R).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Epoch {
    #[schemars(regex(pattern = r"^[0-9a-f]{32}$"))]
    pub epoch: String,
}

/// What a request presents so the door can detect a rewound daemon before
/// writing anything (D4a-R): the highest commit mark the producer has stored.
pub const COMMIT_MARK_HEADER: &str = "wild-commit-mark";

/// What an announcement over the TCP listener presents in place of the
/// socket's same-user check (D2): a one-device pairing code, `wdp_<id>_<secret>`,
/// that the operator asked for on the daemon's machine. It opens one
/// announcement, once, until it expires.
pub const PAIRING_CODE_HEADER: &str = "wild-pairing-code";

/// The parameters of a `daemon_rewound` refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Rewound {
    /// The commit mark the daemon still knows.
    pub known_mark: u64,
    /// When the daemon last received a batch under that mark, RFC 3339.
    pub known_at: String,
    /// The live lines of this device that Wild still holds above
    /// `known_mark`: in receipts it kept, and in the lake. The producer
    /// reports as lost only the live events it counted above `known_mark`
    /// less these.
    pub live_held: u64,
}

/// One entry of the vocabulary: a project's or a customer's key and name. A
/// title match resolves to the key a reference field carries (D7).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VocabularyEntry {
    pub key: String,
    pub name: String,
    /// The type the key belongs to, e.g. `Kunde`.
    pub target_type: String,
}

/// The vocabulary an approved device receives, scoped to its person.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Vocabulary {
    pub etag: String,
    pub entries: Vec<VocabularyEntry>,
}
