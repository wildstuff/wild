//! What a connector may be asked to do, and the calls that reach it
//! (ADR-0398 D1, D4).
//!
//! A manifest may announce `tools:` beside its sources: functions Wild may
//! call on the connector once the operator said yes. Each one names what it
//! does in the operator's words, what it acts on, how risky it is, and the
//! JSON Schema of its arguments. The door refuses an announced tool Wild could
//! not call as safely as an installed plugin's ([`ToolDecl::validate`]).
//!
//! A device that announced tools holds a **call wait**: a request the door
//! keeps open for up to [`CALL_WAIT_HOLD_SECS`] and answers with the calls
//! queued for it ([`Calls`]), or with none. It answers each call with a
//! [`CallAnswer`]. The device always opens; the door never dials out.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// How long the door holds a call wait open at most, in seconds: it answers
/// sooner with the calls queued for the device, or after this (a little
/// less, jittered) with none. The device then opens the next one.
pub const CALL_WAIT_HOLD_SECS: u64 = 25;

/// The argument every announced tool's schema declares: one call's key, so
/// a call asked again is recognised as the same call.
pub const IDEMPOTENCY_KEY: &str = "idempotency_key";

/// The JSON Schema keywords an announced tool's argument schema may use:
/// the closed set Wild checks a call's arguments by, for a plugin's tools
/// and a connector's alike. A schema using any other is refused, because a
/// constraint Wild cannot check must not be declared as if it held.
///
/// Wild enforces `type` (one name or a list), `properties`, `required`,
/// `items`, `enum`, `const`, `minimum`, `maximum`, `minLength`, `maxLength`,
/// `pattern` (an unanchored search), `minItems`, `maxItems`, `anyOf`,
/// `oneOf` and `$ref` (to `#`, `#/$defs/<name>` or `#/definitions/<name>` of
/// the same schema). `description`, `title`, `$schema`, `$comment`,
/// `default`, `examples`, `deprecated`, `readOnly`, `writeOnly` and `format`
/// are annotations: they pass and check nothing. `additionalProperties` is
/// accepted but not yet enforced. A key starting with
/// [`ARGS_SCHEMA_ANNOTATION_PREFIX`] is an annotation too.
///
/// The set is the one `wild_tool_provider::args_schema` enforces. It is
/// published here as well because a connector's developer builds against
/// this contract and never against Wild's code; a test in `wild-mcp-tools`
/// (`the_announced_schema_keywords_are_the_ones_the_call_check_enforces`)
/// holds the two equal.
pub const ARGS_SCHEMA_KEYWORDS: &[&str] = &[
    "type",
    "properties",
    "required",
    "items",
    "enum",
    "const",
    "minimum",
    "maximum",
    "minLength",
    "maxLength",
    "pattern",
    "minItems",
    "maxItems",
    "anyOf",
    "oneOf",
    "$ref",
    "$defs",
    "definitions",
    "description",
    "title",
    "$schema",
    "$comment",
    "default",
    "examples",
    "deprecated",
    "readOnly",
    "writeOnly",
    "format",
    "additionalProperties",
];

/// A key starting with this (`x-mcp-header`) is an extension annotation in
/// an argument schema: carried for whoever reads it, checked by nobody.
pub const ARGS_SCHEMA_ANNOTATION_PREFIX: &str = "x-";

/// What a tool does when it is called.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolActs {
    /// It only reads: nothing changes anywhere.
    Reads,
    /// It changes or starts something outside Wild: writes a document,
    /// starts an agent.
    Outward,
    /// A word this contract does not know — `writes` among them: a
    /// connector's call changes nothing inside Wild (its records come over
    /// the data road), so `writes` is not a connector's word. Refused.
    #[serde(other)]
    Unknown,
}

/// How much harm a call could do if it went wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolRisk {
    Low,
    Medium,
    High,
    /// A word this contract does not know. Refused.
    #[serde(other)]
    Unknown,
}

/// One function a connector offers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolDecl {
    /// The function's name, the same on every device of the connector
    /// (`agent_start`): lowercase letters, digits and `_`, at most 64.
    #[schemars(regex(pattern = r"^[a-z0-9_]{1,64}$"), length(min = 1, max = 64))]
    pub name: String,
    /// What it does, in the operator's words ("Starts an agent on this
    /// host"). The operator reads it on the card before the yes. At most 280
    /// characters.
    pub description: String,
    /// The JSON Schema of its arguments: an object whose `properties`
    /// include `idempotency_key`, using only the keywords Wild checks
    /// and annotations whose key starts with `x-`.
    pub schema: Value,
    /// `reads`, or `outward` for anything it changes or starts. Required:
    /// the door refuses a tool that does not say.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acts: Option<ToolActs>,
    /// How risky a call is: `low`, `medium` or `high`. Absent, Wild takes
    /// the lowest it allows for what the tool acts on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk: Option<ToolRisk>,
    /// The argument whose value names the device that answers a call, when
    /// several devices offer it (normally `host`). One of the schema's
    /// properties.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routes_by: Option<String>,
}

/// Whether `s` is a tool name: lowercase letters, digits and `_`, 1 to 64
/// characters. No dot: a tool's name reaches language models' tool-use
/// APIs, which refuse one.
pub fn is_tool_name(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

impl ToolDecl {
    /// Whether a call to it changes something outside Wild — the one rule
    /// every reader of a tool's stance shares (the card, the connect
    /// dialog, the grant's acts). Anything not declared `reads` counts as
    /// outward: a stance nobody declared is never said as the safe one.
    /// `validate` refuses a missing or unknown stance, so on an accepted
    /// announcement this is exactly `acts == Some(Outward)`.
    pub fn outward(&self) -> bool {
        self.acts != Some(ToolActs::Reads)
    }

    /// Whether Wild can call this tool as safely as an installed plugin's.
    /// The refusal names the tool and says what to change.
    pub fn validate(&self) -> Result<(), String> {
        let name = &self.name;
        if !is_tool_name(name) {
            return Err(format!(
                "`{name}` is not a tool name: lowercase letters, digits and `_`, at most 64 \
                 characters, no dot"
            ));
        }
        let tool = format!("tool `{name}`");
        if self.description.trim().is_empty() {
            return Err(format!(
                "{tool} says nothing of what it does: the operator reads its description \
                 before saying yes"
            ));
        }
        crate::shape::check_description(&tool, Some(&self.description))?;
        match self.acts {
            None => {
                return Err(format!(
                    "{tool} does not say what it acts on: `acts` is `reads`, or `outward` for \
                     anything it changes or starts"
                ))
            }
            Some(ToolActs::Reads | ToolActs::Outward) => {}
            Some(ToolActs::Unknown) => {
                return Err(format!(
                    "{tool}: `acts` is `reads` or `outward`. A connector's call changes \
                     nothing inside Wild, so `writes` is not its word; whatever it changes \
                     or starts is `outward`"
                ))
            }
        }
        if self.risk == Some(ToolRisk::Unknown) {
            return Err(format!("{tool}: `risk` is `low`, `medium` or `high`"));
        }
        check_schema_node(&self.schema, &tool, "schema")?;
        let properties = self.schema.get("properties").and_then(Value::as_object);
        if !properties.is_some_and(|p| p.contains_key(IDEMPOTENCY_KEY)) {
            return Err(format!(
                "{tool}: its schema declares no `{IDEMPOTENCY_KEY}` property. Every call \
                 carries one, so a call asked again is answered once"
            ));
        }
        if let Some(by) = &self.routes_by {
            if !properties.is_some_and(|p| p.contains_key(by)) {
                return Err(format!(
                    "{tool}: it routes by `{by}`, which its schema declares no property for"
                ));
            }
        }
        Ok(())
    }
}

/// One schema node, as Wild's call check walks it: its keywords, then every
/// shape it holds — each property's, its items', each `anyOf`/`oneOf`
/// branch and each entry of `$defs`/`definitions`. `true` and `false` are
/// shapes too (anything, nothing).
fn check_schema_node(node: &Value, tool: &str, at: &str) -> Result<(), String> {
    if node.is_boolean() {
        return Ok(());
    }
    let Some(obj) = node.as_object() else {
        return Err(format!("{tool}: `{at}` is not a JSON Schema object"));
    };
    if let Some(key) = obj.keys().find(|k| {
        !ARGS_SCHEMA_KEYWORDS.contains(&k.as_str()) && !k.starts_with(ARGS_SCHEMA_ANNOTATION_PREFIX)
    }) {
        return Err(format!(
            "{tool}: `{at}` uses `{key}`, which Wild does not check. A constraint that \
             cannot be checked must not be declared as if it held; the keywords Wild checks \
             are {}, and a key starting with `{ARGS_SCHEMA_ANNOTATION_PREFIX}` is a note",
            ARGS_SCHEMA_KEYWORDS.join(", ")
        ));
    }
    for holder in ["properties", "$defs", "definitions"] {
        if let Some(map) = obj.get(holder).and_then(Value::as_object) {
            for (name, sub) in map {
                check_schema_node(sub, tool, &format!("{at}.{holder}.{name}"))?;
            }
        }
    }
    if let Some(items) = obj.get("items") {
        check_schema_node(items, tool, &format!("{at}.items"))?;
    }
    for list in ["anyOf", "oneOf"] {
        if let Some(branches) = obj.get(list).and_then(Value::as_array) {
            for (i, sub) in branches.iter().enumerate() {
                check_schema_node(sub, tool, &format!("{at}.{list}[{i}]"))?;
            }
        }
    }
    Ok(())
}

/// One call Wild hands a device.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolCall {
    /// The call's id: the device names it in its answer.
    pub call: String,
    /// The tool called, by the name the manifest announced.
    pub tool: String,
    /// The arguments, checked against the tool's schema before the call was
    /// queued.
    pub arguments: Value,
}

/// The answer to a call wait: the calls queued for the device, oldest first.
/// Empty when none came while the wait was held: open the next one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Calls {
    pub calls: Vec<ToolCall>,
}

/// A device's answer to one call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CallAnswer {
    /// The `call` of the call it answers.
    pub call: String,
    /// What the call did, as JSON of the tool's own shape.
    pub result: Value,
}

#[cfg(test)]
#[path = "tools_tests.rs"]
mod tests;
