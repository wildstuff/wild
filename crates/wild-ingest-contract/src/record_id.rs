//! How a record's id is formed (ADR-0365 D4).
//!
//! The door prefixes every **record id** with the device it came from, so ids
//! of different devices never meet. A **reference** field is never prefixed:
//! it keeps the target type's own key, so a view grouped by a reference sees
//! every device's rows.
//!
//! The producer supplies only the source part of the id — its `natural` key
//! fields, its file generation and offset, or its epoch and counter. The door composes the whole id; a
//! producer never writes the device part itself.

use crate::shape::ShapeDecl;
use serde_json::{Map, Value};

/// The separator between the parts of a record id. A part's own `:` is
/// escaped (see [`compose`]), so it never runs two parts together.
pub const SEPARATOR: char = ':';

/// A `live` record's source part: the epoch and the producer's counter,
/// carried in the record under these two field names.
pub const LIVE_EPOCH_FIELD: &str = "_epoch";
pub const LIVE_COUNTER_FIELD: &str = "_seq";

/// A `position` record's source part: the file generation and the record's
/// byte offset in it, carried in the record under these two field names.
pub const POSITION_GENERATION_FIELD: &str = "_generation";
pub const POSITION_OFFSET_FIELD: &str = "_offset";

/// Compose a record's id from the device and the record, under its shape's
/// key class: the parts joined by [`SEPARATOR`], each with its `%`, its `:`
/// and every control character percent-escaped (`%25`, `%3A`, `%1F`, …). The
/// id is readable, holds no control character, and is one key in any store:
/// the Wild joins a composite key's parts with U+001F, so an id holding that
/// character would be split into several. `None` when the record lacks a
/// part of its key.
pub fn compose(device: &str, shape: &ShapeDecl, record: &Map<String, Value>) -> Option<String> {
    let mut parts: Vec<String> = vec![device.to_string(), shape.name.clone()];
    match shape.key_class.reserved_parts() {
        None => {
            for k in &shape.key {
                parts.push(key_part(record.get(k)?)?);
            }
        }
        Some((text, number)) => {
            parts.push(key_part(record.get(text)?)?);
            parts.push(key_part(record.get(number)?)?);
        }
    }
    let escaped: Vec<String> = parts.iter().map(|p| escape(p)).collect();
    Some(escaped.join(&SEPARATOR.to_string()))
}

/// A part as it stands in an id: `%`, the separator and every control
/// character written as the percent-escapes of their UTF-8 bytes.
fn escape(part: &str) -> String {
    let mut out = String::with_capacity(part.len());
    for c in part.chars() {
        if c == '%' || c == SEPARATOR || c.is_control() {
            let mut buf = [0u8; 4];
            for b in c.encode_utf8(&mut buf).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn key_part(v: &Value) -> Option<String> {
    match v {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}
