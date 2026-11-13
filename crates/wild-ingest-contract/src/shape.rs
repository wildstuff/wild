//! A declared shape: what one line of a batch may carry (ADR-0365 D4, D4a).
//!
//! A line is resolved by the `shape@version` it names, never by its column
//! set, so an absent optional field is null and not a new shape.

use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A shape's name and version, written `name@version` on the wire.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ShapeRef {
    pub name: String,
    pub version: u32,
}

impl fmt::Display for ShapeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

impl FromStr for ShapeRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (name, version) = s
            .split_once('@')
            .ok_or_else(|| format!("`{s}` names no version; write `name@version`"))?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        {
            return Err(format!(
                "`{name}` is not a shape name: lowercase letters, digits and `_` only"
            ));
        }
        let version = version
            .parse::<u32>()
            .map_err(|_| format!("`{version}` is not a shape version: a whole number"))?;
        Ok(Self {
            name: name.to_string(),
            version,
        })
    }
}

impl Serialize for ShapeRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ShapeRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for ShapeRef {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "ShapeRef".into()
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[a-z0-9_]+@[0-9]+$",
            "description": "A shape's name and version, `name@version`."
        })
    }
}

// ADR-0365 D4.
/// How a shape's records get their ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum KeyClass {
    /// The id is the shape's declared key fields: a re-read writes nothing
    /// new, and a record changed under its id is refused
    /// (`source_rewrote_history`).
    Natural,
    /// The id is an append-only file's generation and byte offset, in the
    /// reserved fields `_generation` (text) and `_offset` (a whole number); a
    /// restored file's line is re-keyed by Wild (`generation_reassigned`).
    Position,
    /// The id is a per-install epoch and counter, in the reserved fields
    /// `_epoch` and `_seq` (a whole number), for events seen only in the
    /// moment; never backfilled.
    Live,
}

impl KeyClass {
    /// The two reserved fields a record of this class carries its key parts
    /// in: a text part, then a whole-number part. `None` for `natural`, whose
    /// key is its declared fields.
    pub fn reserved_parts(self) -> Option<(&'static str, &'static str)> {
        use crate::record_id::{
            LIVE_COUNTER_FIELD, LIVE_EPOCH_FIELD, POSITION_GENERATION_FIELD, POSITION_OFFSET_FIELD,
        };
        match self {
            KeyClass::Natural => None,
            KeyClass::Position => Some((POSITION_GENERATION_FIELD, POSITION_OFFSET_FIELD)),
            KeyClass::Live => Some((LIVE_EPOCH_FIELD, LIVE_COUNTER_FIELD)),
        }
    }
}

/// The kind of value a field carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FieldKind {
    /// Text, at most `max_len` characters.
    Text { max_len: u32 },
    /// A number.
    Number,
    /// `true` or `false`.
    Bool,
    /// An RFC 3339 timestamp, the record's own event time.
    Timestamp,
    // ADR-0365 D4: the door prefixes record ids with the device, never a
    // reference.
    /// The key of a record of another type. A reference keeps the target
    /// type's own key; it is never prefixed with a device.
    Reference { target_type: String },
}

/// The longest description a source, a shape or a field may carry, in
/// characters.
pub const MAX_DESCRIPTION_CHARS: usize = 280;

/// A description's own rule: when given, it says something and fits
/// [`MAX_DESCRIPTION_CHARS`]. `what` names its holder for the refusal.
pub fn check_description(what: &str, description: Option<&str>) -> Result<(), String> {
    match description {
        Some(d) if d.trim().is_empty() => Err(format!(
            "{what}: a description says what it holds; leave it out rather than empty"
        )),
        Some(d) if d.chars().count() > MAX_DESCRIPTION_CHARS => Err(format!(
            "{what}: its description is over {MAX_DESCRIPTION_CHARS} characters"
        )),
        _ => Ok(()),
    }
}

// Built-in connectors declare their columns' meaning the same way
// (`wild_core::source_connector::ProvenanceColumn::description`): one rule
// for every connector, built in or outside Wild.
/// One field of a shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FieldDecl {
    pub name: String,
    #[serde(flatten)]
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    /// What the field holds, in the operator's words: its meaning, its unit,
    /// an example ("Estimated cost of the session in US dollars, e.g. 0.42").
    /// Wild carries it onto the table's field. Optional; at most 280
    /// characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

// ADR-0365 D4a, D16.
/// A declared shape: its version, its key class, its fields and the type its
/// records are proposed to fill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ShapeDecl {
    #[schemars(regex(pattern = r"^[a-z0-9_]+$"))]
    pub name: String,
    pub version: u32,
    pub key_class: KeyClass,
    /// The fields that form a `natural` key. Empty for a `position` or a
    /// `live` shape, which carry their key parts in reserved fields.
    #[serde(default)]
    pub key: Vec<String>,
    pub fields: Vec<FieldDecl>,
    // ADR-0365 Revision 18: the tribe's model decides where a shape's rows go.
    /// The type the producer proposes this shape fills, usually a new one of
    /// its own. The operator accepts it or maps the shape into another type.
    #[schemars(regex(pattern = r"^[A-Za-z][A-Za-z0-9_]*$"), length(max = 48))]
    pub target_type: String,
    /// What one record of this shape is, in the operator's words ("One
    /// finished work session in OpenCode"). Optional; at most 280 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ShapeDecl {
    /// This shape's `name@version`.
    pub fn shape_ref(&self) -> ShapeRef {
        ShapeRef {
            name: self.name.clone(),
            version: self.version,
        }
    }

    /// The declaration's own consistency: its target type is a type name; a
    /// `natural` shape names a key of declared, required fields; a `position`
    /// or `live` shape names none; no field is declared twice.
    pub fn validate(&self) -> Result<(), String> {
        let t = self.target_type.as_bytes();
        if t.is_empty()
            || t.len() > 48
            || !t[0].is_ascii_alphabetic()
            || !t.iter().all(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            return Err(format!(
                "{}: `{}` is not a type name: an ASCII letter, then letters, digits or `_`",
                self.shape_ref(),
                self.target_type
            ));
        }
        check_description(&self.shape_ref().to_string(), self.description.as_deref())?;
        let mut seen = std::collections::BTreeSet::new();
        for f in &self.fields {
            check_description(
                &format!("{}: field `{}`", self.shape_ref(), f.name),
                f.description.as_deref(),
            )?;
            if !seen.insert(f.name.as_str()) {
                return Err(format!(
                    "{}: field `{}` is declared twice",
                    self.shape_ref(),
                    f.name
                ));
            }
        }
        match self.key_class {
            KeyClass::Live if !self.key.is_empty() => Err(format!(
                "{}: a `live` shape takes its id from an epoch and a counter, so it names no key",
                self.shape_ref()
            )),
            KeyClass::Position if !self.key.is_empty() => Err(format!(
                "{}: a `position` shape takes its id from a file generation and an offset, so it names no key",
                self.shape_ref()
            )),
            KeyClass::Natural if self.key.is_empty() => Err(format!(
                "{}: a `natural` shape names the fields that form its key",
                self.shape_ref()
            )),
            KeyClass::Natural => {
                for k in &self.key {
                    match self.fields.iter().find(|f| &f.name == k) {
                        None => {
                            return Err(format!(
                                "{}: key field `{k}` is not a declared field",
                                self.shape_ref()
                            ))
                        }
                        Some(f) if !f.required => {
                            return Err(format!(
                                "{}: key field `{k}` must be required",
                                self.shape_ref()
                            ))
                        }
                        Some(_) => {}
                    }
                }
                Ok(())
            }
            KeyClass::Position | KeyClass::Live => Ok(()),
        }
    }
}
