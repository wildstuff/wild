//! What a producer announces about itself (ADR-0365 D16).
//!
//! A manifest becomes a change folder: a proposed producer, drawn dashed in
//! the atlas until the operator's yes activates it. Its
//! text is the producer's own claim about itself and is rendered as quoted,
//! untrusted data (D9).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::shape::{ShapeDecl, ShapeRef};
use crate::tools::ToolDecl;

/// One source a producer feeds: a slug and the shapes it sends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SourceDecl {
    #[schemars(regex(pattern = r"^[a-z0-9]+(-[a-z0-9]+)*$"), length(max = 48))]
    pub slug: String,
    pub shapes: Vec<ShapeDecl>,
    /// What the source is, in the operator's words ("Usage of the OpenCode
    /// agent on this machine"). Optional; at most 280 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A producer's announcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Manifest {
    /// The producer's name, e.g. `wild-omarchy`.
    #[schemars(regex(pattern = r"^[a-z0-9]+(-[a-z0-9]+)*$"), length(max = 48))]
    pub producer: String,
    /// The producer's own version.
    pub version: String,
    /// The contract version it was built against (D1b).
    pub contract_version: String,
    /// The tribe it proposes, if any. Absent, Wild proposes one (D16).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tribe: Option<String>,
    pub sources: Vec<SourceDecl>,
    /// Existing types of the tribe the producer's shapes reference. It never
    /// writes into them (D4a).
    #[serde(default)]
    pub references: Vec<String>,
    /// Whether it needs the vocabulary of project and customer names (D7).
    #[serde(default)]
    pub needs_vocabulary: bool,
    /// The machine fingerprint: an app-specific HMAC of the machine's stable
    /// identifier. Only a hint for offering continuation, never an identity
    /// (D2).
    pub fingerprint: String,
    /// The functions it offers Wild to call once the operator said yes
    /// (ADR-0398 D1). A connector that only sends announces none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDecl>,
    /// The values this device answers for (ADR-0398 D3): the values a call
    /// may name in a tool's `routes_by` argument to reach this device,
    /// normally this host's name. A call is routed by the calling tribe and
    /// this value together, never by value alone. Empty: the device claims
    /// no value — a device that offers no tool claims none, and so may not
    /// declare any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub answers_for: Vec<String>,
}

/// Where an announcement stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AnnouncementState {
    /// Shown dashed in the atlas, waiting for the operator's answer.
    Pending,
    /// Activated: the poll carries the producer key and first epoch once.
    Activated,
    /// The operator declined it.
    Rejected,
    /// The operator cut this device off: its key no longer opens the door,
    /// and one it never fetched is never handed over. Terminal: stop polling
    /// and stop sending; only a new announcement the operator lets in again
    /// pairs this install anew.
    Revoked,
    /// A state this contract version does not know. Keep polling.
    #[serde(other)]
    Unknown,
}

/// The reply to an announcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementReceipt {
    pub announcement: String,
    pub profile: String,
    pub state: AnnouncementState,
}

/// An announcement's state, as its poll reports it. `producer_key` and
/// `epoch` are present exactly once, on the first poll after activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnnouncementStatus {
    pub announcement: String,
    pub profile: String,
    pub state: AnnouncementState,
    /// The tribe the operator placed the producer in when saying yes, on
    /// every poll from then on: the `{tribe}` of each route the key opens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tribe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = r"^[0-9a-f]{32}$"))]
    pub epoch: Option<String>,
}

impl Manifest {
    /// What it announces, by source: each slug with its shapes, in the
    /// producer's order — the form the activation's rules read
    /// (`wild_ddd::push_ingress`).
    #[must_use]
    pub fn announced(&self) -> Vec<(String, Vec<ShapeDecl>)> {
        self.sources
            .iter()
            .map(|s| (s.slug.clone(), s.shapes.clone()))
            .collect()
    }

    /// The manifest's own consistency: a contract version, at least one
    /// source, unique slugs, and every shape valid on its own. The producer
    /// and every source slug are [`is_name`]s: Wild builds paths, queue
    /// references and type names from them.
    pub fn validate(&self) -> Result<(), String> {
        if self.producer.is_empty() || self.version.is_empty() || self.contract_version.is_empty() {
            return Err(
                "a manifest names its producer, its version and its contract version".into(),
            );
        }
        if !is_name(&self.producer) {
            return Err(format!(
                "`{}` is not a producer name: lowercase letters and digits, joined by single `-`",
                self.producer
            ));
        }
        if self.sources.is_empty() {
            return Err(format!("`{}` announces no source", self.producer));
        }
        let mut slugs = std::collections::BTreeSet::new();
        for s in &self.sources {
            if !is_name(&s.slug) {
                return Err(format!(
                    "`{}` is not a source slug: lowercase letters and digits, joined by single `-`",
                    s.slug
                ));
            }
            if !slugs.insert(s.slug.as_str()) {
                return Err(format!("source `{}` is announced twice", s.slug));
            }
            if s.shapes.is_empty() {
                return Err(format!("source `{}` declares no shape", s.slug));
            }
            crate::shape::check_description(
                &format!("source `{}`", s.slug),
                s.description.as_deref(),
            )?;
            for shape in &s.shapes {
                // What a line names its shape by; a shape no line can name
                // would be let in and never receive a record.
                shape.shape_ref().to_string().parse::<ShapeRef>()?;
                shape.validate()?;
            }
        }
        let mut names = std::collections::BTreeSet::new();
        for t in &self.tools {
            t.validate()?;
            if !names.insert(t.name.as_str()) {
                return Err(format!("tool `{}` is announced twice", t.name));
            }
            // A routing tool with no value to route by is let in, placed, and
            // then answers `unreachable` to every call: refused here, where
            // the producer can still say which values it answers for.
            if t.routes_by.is_some() && self.answers_for.is_empty() {
                return Err(format!(
                    "tool `{}` routes calls by an argument, but `{}` names no value it answers \
                     for: a device that claims no value answers no call. Name the values this \
                     device answers for",
                    t.name, self.producer
                ));
            }
        }
        let mut values = std::collections::BTreeSet::new();
        for value in &self.answers_for {
            if value.trim().is_empty() {
                return Err(format!(
                    "`{}` names an empty value among the values it answers for",
                    self.producer
                ));
            }
            if !values.insert(value.as_str()) {
                return Err(format!(
                    "`{}` is named twice among the values `{}` answers for",
                    value, self.producer
                ));
            }
        }
        if self.tools.is_empty() && !self.answers_for.is_empty() {
            return Err(format!(
                "`{}` names the values it answers for but offers no tool: a device without \
                 tools claims no value",
                self.producer
            ));
        }
        Ok(())
    }

    /// The build of the functions it announces: `sha256:` and the hex
    /// SHA-256 of the `tools` as announced, every field of every tool, taken
    /// in name order. A changed function, description or risk is a new
    /// build, and the operator's standing yes names the build it was given
    /// for (ADR-0398 D1, D2); the order a connector lists them in is not a
    /// field of any of them, so listing the same tools in another order is
    /// the same build.
    #[must_use]
    pub fn tools_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut tools: Vec<&ToolDecl> = self.tools.iter().collect();
        tools.sort_by(|a, b| a.name.cmp(&b.name));
        let bytes = serde_json::to_vec(&tools).unwrap_or_default();
        format!("sha256:{}", hex::encode(Sha256::digest(&bytes)))
    }
}

/// Whether `s` is a producer name or a source slug: lowercase letters and
/// digits, joined by single `-`, at most 48 characters.
pub fn is_name(s: &str) -> bool {
    s.len() <= 48
        && !s.is_empty()
        && s.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        })
}
