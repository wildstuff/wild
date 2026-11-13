//! A batch: one request's NDJSON body, its id and its bounds (ADR-0365 D1).
//!
//! One line is one record of a declared shape:
//!
//! ```json
//! {"shape": "window_span@1", "record": {"started": "…", "ended": "…", "project": "…"}}
//! ```
//!
//! A batch is identified by the hash of its decoded body, never by a sequence
//! number, and acknowledged by that id (D3, D4). The same body sent twice is
//! the same batch.
//!
//! [`parse`] and [`check_lines`] are the one implementation of the door's
//! line rules. The door, `intake_validate_batch` and the kit all call them, so a
//! verdict the agent sees before sending is the verdict the door gives.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::codes::LineCode;
use crate::shape::{FieldKind, ShapeDecl, ShapeRef};

/// The largest decoded body a batch may have. Keeps a queued batch with its
/// headers under the bus's 8 MB `max_payload` (D1).
pub const MAX_DECODED_BYTES: usize = 6 * 1024 * 1024;
/// The largest ratio of decoded to compressed bytes a gzip body may reach.
pub const MAX_COMPRESSION_RATIO: usize = 20;
/// The largest line, in bytes.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The most lines one batch may carry.
pub const MAX_LINES: usize = 10_000;

/// A batch's id: the lowercase hex SHA-256 of its decoded body.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct BatchId(pub String);

impl BatchId {
    /// The id of a decoded body.
    pub fn of(decoded_body: &[u8]) -> Self {
        Self(hex::encode(Sha256::digest(decoded_body)))
    }
}

/// One line of a batch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Line {
    pub shape: ShapeRef,
    pub record: Map<String, Value>,
    /// The line's number in the body, counting from 1 with blank lines
    /// counted: the number every reply names it by. Set by [`parse`]; not
    /// part of the wire line.
    #[serde(skip)]
    pub number: u32,
}

/// The line numbered `number` in the body, as [`parse`] numbered it. `None`
/// for a blank line or a number past the body.
pub fn line(lines: &[Line], number: u32) -> Option<&Line> {
    lines
        .binary_search_by_key(&number, |l| l.number)
        .ok()
        .map(|i| &lines[i])
}

/// A line the door rejected before the batch was received (D1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RejectedLine {
    /// The line's number in the body, counting from 1.
    pub line: u32,
    pub code: LineCode,
    #[serde(default)]
    pub params: Map<String, Value>,
}

fn rejected(line: usize, code: LineCode, params: Value) -> RejectedLine {
    RejectedLine {
        line: line as u32,
        code,
        params: params.as_object().cloned().unwrap_or_default(),
    }
}

/// Parse a decoded NDJSON body into its lines. Blank lines are skipped. A
/// body over its bounds is refused whole (`Err(None)`); otherwise every line
/// that is not a well-formed [`Line`] is listed (`Err(Some(rejected))`).
pub fn parse(decoded_body: &[u8]) -> Result<Vec<Line>, Option<Vec<RejectedLine>>> {
    if decoded_body.len() > MAX_DECODED_BYTES {
        return Err(None);
    }
    let mut lines = Vec::new();
    let mut bad = Vec::new();
    let mut count = 0usize;
    for (i, raw) in decoded_body.split(|b| *b == b'\n').enumerate() {
        let n = i + 1;
        let raw = raw.strip_suffix(b"\r").unwrap_or(raw);
        if raw.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        count += 1;
        if count > MAX_LINES {
            return Err(None);
        }
        if raw.len() > MAX_LINE_BYTES {
            bad.push(rejected(
                n,
                LineCode::LineTooLong,
                serde_json::json!({"max_bytes": MAX_LINE_BYTES}),
            ));
            continue;
        }
        let value: Value = match serde_json::from_slice(raw) {
            Ok(v) => v,
            Err(_) => {
                bad.push(rejected(n, LineCode::NotAnObject, Value::Null));
                continue;
            }
        };
        match serde_json::from_value::<Line>(value) {
            Ok(line) => lines.push(Line {
                number: n as u32,
                ..line
            }),
            Err(e) => bad.push(rejected(
                n,
                LineCode::ShapeUnknown,
                serde_json::json!({"reason": e.to_string()}),
            )),
        }
    }
    if bad.is_empty() {
        Ok(lines)
    } else {
        Err(Some(bad))
    }
}

/// Check one record against its shape (D1, D4a): no undeclared field, every
/// required field present, every value of its declared kind, every string
/// within its cap. `null` stands for an absent optional field. A `position`
/// or `live` record also carries its two key parts in reserved fields
/// ([`crate::shape::KeyClass::reserved_parts`]): a non-empty text, then a
/// whole number.
pub fn validate_line(
    shape: &ShapeDecl,
    record: &Map<String, Value>,
) -> Result<(), (LineCode, Value)> {
    let reserved = shape.key_class.reserved_parts();
    let is_reserved =
        |name: &str| reserved.is_some_and(|(text, number)| name == text || name == number);
    for name in record.keys() {
        if is_reserved(name) {
            continue;
        }
        if !shape.fields.iter().any(|f| &f.name == name) {
            return Err((
                LineCode::FieldUndeclared,
                serde_json::json!({"field": name}),
            ));
        }
    }
    if let Some((text, number)) = reserved {
        let text_ok = record
            .get(text)
            .and_then(Value::as_str)
            .is_some_and(|e| !e.is_empty());
        let number_ok = record.get(number).and_then(Value::as_u64).is_some();
        if !text_ok || !number_ok {
            return Err((
                LineCode::FieldMissing,
                serde_json::json!({"field": if text_ok { number } else { text }}),
            ));
        }
    }
    for f in &shape.fields {
        let v = record.get(&f.name).unwrap_or(&Value::Null);
        if v.is_null() {
            if f.required {
                return Err((LineCode::FieldMissing, serde_json::json!({"field": f.name})));
            }
            continue;
        }
        let ok = match &f.kind {
            FieldKind::Text { max_len } => match v.as_str() {
                Some(s) if s.chars().count() > *max_len as usize => {
                    return Err((
                        LineCode::FieldTooLong,
                        serde_json::json!({"field": f.name, "max_len": max_len}),
                    ))
                }
                Some(_) => true,
                None => false,
            },
            FieldKind::Number => v.is_number(),
            FieldKind::Bool => v.is_boolean(),
            FieldKind::Timestamp => v.as_str().is_some_and(is_rfc3339),
            FieldKind::Reference { .. } => v.is_string(),
        };
        if !ok {
            return Err((
                LineCode::FieldWrongKind,
                serde_json::json!({"field": f.name}),
            ));
        }
    }
    Ok(())
}

/// Check parsed lines against the shapes a source accepts (D1, D4a): each
/// line names one of `shapes` by name and version, and its record passes
/// [`validate_line`]. Returns the shape each line resolved to, in line order,
/// or every line that failed, by its [`Line::number`].
pub fn check_lines<'a>(
    lines: &[Line],
    shapes: &'a [ShapeDecl],
) -> Result<Vec<&'a ShapeDecl>, Vec<RejectedLine>> {
    let mut resolved = Vec::with_capacity(lines.len());
    let mut bad = Vec::new();
    for line in lines {
        let n = line.number as usize;
        let Some(shape) = shapes
            .iter()
            .find(|s| s.name == line.shape.name && s.version == line.shape.version)
        else {
            bad.push(rejected(
                n,
                LineCode::ShapeUnknown,
                serde_json::json!({"shape": line.shape.to_string()}),
            ));
            continue;
        };
        match validate_line(shape, &line.record) {
            Ok(()) => resolved.push(shape),
            Err((code, params)) => bad.push(rejected(n, code, params)),
        }
    }
    if bad.is_empty() {
        Ok(resolved)
    } else {
        Err(bad)
    }
}

/// A strict shape check for an RFC 3339 timestamp: `YYYY-MM-DDTHH:MM:SS`, an
/// optional fraction, and `Z` or a `±HH:MM` offset.
fn is_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    let digits = |r: std::ops::Range<usize>| {
        b.get(r.clone())
            .is_some_and(|x| x.iter().all(u8::is_ascii_digit))
    };
    if b.len() < 20
        || !digits(0..4)
        || b[4] != b'-'
        || !digits(5..7)
        || b[7] != b'-'
        || !digits(8..10)
        || !(b[10] == b'T' || b[10] == b't')
        || !digits(11..13)
        || b[13] != b':'
        || !digits(14..16)
        || b[16] != b':'
        || !digits(17..19)
    {
        return false;
    }
    let mut i = 19;
    if b[i] == b'.' {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    match b.get(i) {
        Some(b'Z') | Some(b'z') => i + 1 == b.len(),
        Some(b'+') | Some(b'-') => {
            i + 6 == b.len() && digits(i + 1..i + 3) && b[i + 3] == b':' && digits(i + 4..i + 6)
        }
        _ => false,
    }
}
