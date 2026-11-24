//! The published JSON Schemas and the OpenAPI document, derived from this
//! crate's types.
//!
//! `cargo run -p xtask -- ingest-contract-write` writes [`documents`] to
//! [`DIR`]; a test in this crate fails when the committed files drift from
//! the types. A producer in any language builds against these files, never
//! against a copy of the types.

use crate::{batch, device, manifest, openapi, pairing_file, reply, tools};

/// Where the documents live, relative to the repository root.
pub const DIR: &str = "docs/ingest-contract/v1";

fn schema<T: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(T)).expect("a schema serialises")
}

/// Every published schema, by the name the OpenAPI document refers to it by.
pub fn components() -> Vec<(&'static str, serde_json::Value)> {
    vec![
        ("Line", schema::<batch::Line>()),
        ("Manifest", schema::<manifest::Manifest>()),
        (
            "AnnouncementReceipt",
            schema::<manifest::AnnouncementReceipt>(),
        ),
        (
            "AnnouncementStatus",
            schema::<manifest::AnnouncementStatus>(),
        ),
        ("BatchReply", schema::<reply::BatchReply>()),
        ("BatchStatus", schema::<reply::BatchStatus>()),
        ("StatusRequest", schema::<reply::StatusRequest>()),
        ("StatusReply", schema::<reply::StatusReply>()),
        ("Refusal", schema::<reply::Refusal>()),
        ("Epoch", schema::<device::Epoch>()),
        ("Rewound", schema::<device::Rewound>()),
        ("Vocabulary", schema::<device::Vocabulary>()),
        ("PairingFile", schema::<pairing_file::PairingFile>()),
        ("Calls", schema::<tools::Calls>()),
        ("CallAnswer", schema::<tools::CallAnswer>()),
    ]
}

/// `BatchReply` → `batch-reply.schema.json`.
pub fn file_name(component: &str) -> String {
    let mut out = String::new();
    for (i, c) in component.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('-');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    format!("{out}.schema.json")
}

/// Every published document: each schema under its file name, and the
/// OpenAPI document as `openapi.json`.
pub fn documents() -> Vec<(String, serde_json::Value)> {
    let mut docs: Vec<(String, serde_json::Value)> = components()
        .into_iter()
        .map(|(name, value)| (file_name(name), value))
        .collect();
    docs.push(("openapi.json".to_string(), openapi::document()));
    for (_, value) in &mut docs {
        scrub(value);
    }
    docs
}

/// Rewrite every string of a published document so it speaks to an outside
/// developer. schemars copies doc comments verbatim, and those carry the
/// repository's own vocabulary: rustdoc links and decision tags. A link
/// `` [`a::b`] `` becomes `` `b` ``; a parenthesised decision tag such as
/// `(D4a-R)` or `(ADR-0398 D3)` is dropped. What this cannot resolve is left
/// for [`leaks`] to name.
pub(crate) fn scrub(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::String(s) => *s = scrub_text(s),
        serde_json::Value::Array(items) => items.iter_mut().for_each(scrub),
        serde_json::Value::Object(map) => map.values_mut().for_each(scrub),
        _ => {}
    }
}

#[cfg(test)]
pub(crate) fn scrub_for_test(value: &mut serde_json::Value) {
    scrub(value);
}

/// A decision tag: `D4a-R`, or `ADR-0398 D3`, or a bare `ADR-0398`.
fn is_decision_tag(part: &str) -> bool {
    let part = part.trim();
    let part = match part.strip_prefix("ADR-") {
        Some(rest) => {
            let digits = rest.chars().take_while(char::is_ascii_digit).count();
            if digits == 0 {
                return false;
            }
            rest[digits..].trim_start()
        }
        None => part,
    };
    if part.is_empty() {
        return true;
    }
    let mut chars = part.chars();
    chars.next() == Some('D')
        && chars.next().is_some_and(|c| c.is_ascii_digit())
        && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn scrub_text(text: &str) -> String {
    // Decision tags: drop the tag parts of each `( … )` group.
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('(') {
        let Some(len) = rest[open + 1..].find(')') else {
            break;
        };
        let inner = &rest[open + 1..open + 1 + len];
        let parts: Vec<&str> = inner.split(", ").collect();
        if inner.contains('(') || !parts.iter().any(|p| is_decision_tag(p)) {
            out.push_str(&rest[..=open]);
            rest = &rest[open + 1..];
            continue;
        }
        let kept: Vec<&str> = parts.into_iter().filter(|p| !is_decision_tag(p)).collect();
        if kept.is_empty() {
            out.push_str(rest[..open].trim_end_matches([' ', '\n']));
        } else {
            out.push_str(&rest[..open]);
            out.push('(');
            out.push_str(&kept.join(", "));
            out.push(')');
        }
        rest = &rest[open + len + 2..];
    }
    out.push_str(rest);
    // Rustdoc links: keep the last path segment, in code font.
    let mut linked = String::with_capacity(out.len());
    let mut rest = out.as_str();
    while let Some(open) = rest.find("[`") {
        let Some(close) = rest[open..].find("`]") else {
            break;
        };
        let target = &rest[open + 2..open + close];
        linked.push_str(&rest[..open]);
        linked.push('`');
        linked.push_str(target.rsplit("::").next().unwrap_or(target));
        linked.push('`');
        rest = &rest[open + close + 2..];
    }
    linked.push_str(rest);
    linked
}

/// Every string of a published document that still speaks the repository's
/// own language: a rustdoc link, a `crate::` path, an `ADR-` number or a
/// `(D…)` decision tag. [`documents`] scrubs what it can; a hit here is
/// something it could not resolve, and the writer refuses it.
pub fn leaks(value: &serde_json::Value) -> Vec<String> {
    fn walk(value: &serde_json::Value, out: &mut Vec<String>) {
        match value {
            serde_json::Value::String(s) => {
                let tagged = s.match_indices("(D").any(|(i, _)| {
                    s[i + 2..]
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_digit())
                });
                if s.contains("[`") || s.contains("crate::") || s.contains("ADR-") || tagged {
                    out.push(s.clone());
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(|v| walk(v, out)),
            serde_json::Value::Object(map) => map.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    walk(value, &mut out);
    out
}

/// A document's committed bytes: pretty JSON with a trailing newline.
pub fn render(value: &serde_json::Value) -> String {
    let mut s = serde_json::to_string_pretty(value).expect("a schema serialises");
    s.push('\n');
    s
}
