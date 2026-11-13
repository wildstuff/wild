//! The OpenAPI 3.1 document of the ingest door, rendered from [`ROUTES`] and
//! the published schemas (ADR-0365 D12 item 3).
//!
//! Nothing here is written by hand: paths and request headers come from
//! [`ROUTES`], bodies from [`crate::schema::components`], the batch bounds from
//! [`crate::batch`]'s constants. The document is published beside the JSON
//! Schemas and freshness-tested with them.

use serde_json::{json, Map, Value};

use crate::batch::{MAX_COMPRESSION_RATIO, MAX_DECODED_BYTES, MAX_LINES, MAX_LINE_BYTES};
use crate::routes::{Auth, Body, Header, HeaderValue, Presence, Route, ROUTES};
use crate::schema;

/// The document's introduction: the road a producer takes, where its records
/// land, and how to describe and key them. Plain prose a developer or an
/// agent reads first; every route and header it names is checked against the
/// route table (`tests::the_introduction_names_only_real_routes_and_headers`).
pub const INTRO: &str = include_str!("openapi_intro.md");

/// The whole document.
pub fn document() -> Value {
    let mut paths = Map::new();
    for r in ROUTES {
        let item = paths
            .entry(format!("{}{}", crate::ROUTE_PREFIX, r.path))
            .or_insert_with(|| json!({}));
        item.as_object_mut()
            .expect("a path item is an object")
            .insert(r.method.as_str().to_string(), operation(r));
    }

    let mut components = Map::new();
    for (name, value) in schema::components() {
        let mut value = value;
        let obj = value.as_object_mut().expect("a schema is an object");
        obj.remove("$schema");
        if let Some(Value::Object(defs)) = obj.remove("$defs") {
            for (k, v) in defs {
                components.entry(k).or_insert(v);
            }
        }
        components.insert(name.to_string(), value);
    }

    let doc = json!({
        "openapi": "3.1.0",
        "info": {
            "title": "Wild producer ingest contract",
            "version": crate::CONTRACT_VERSION,
            "description": INTRO,
            "license": { "name": "Apache-2.0", "identifier": "Apache-2.0" }
        },
        "servers": [{
            "url": crate::ROUTE_PREFIX,
            "description": "V1: the socket `system/ingest.sock` under the operator's profile, and the daemon's TCP ingest listener when the operator opens one: TLS on loopback and on the local network alike, so a connector takes one road wherever it runs (only an engineer's `WILD_INGEST_BIND` on loopback serves plain). Over TLS the door's certificate is its own, self-signed: the producer pins it by the fingerprint its pairing file carries, `sha256:` and 64 lowercase hex digits of the SHA-256 of the certificate's DER, compares it with the certificate the door shows before it writes a byte, and trusts no CA or name in its place and never sends with the comparison skipped."
        }],
        "x-wild-bounds": {
            "max_lines": MAX_LINES,
            "max_decoded_bytes": MAX_DECODED_BYTES,
            "max_line_bytes": MAX_LINE_BYTES,
            "max_compression_ratio": MAX_COMPRESSION_RATIO,
            "description": "What one batch may carry. Over a bound the batch is answered 413 `over_bound` and nothing is received. A text field is held to its declared `max_len`."
        },
        "paths": Value::Object(paths),
        "components": {
            "schemas": Value::Object(components),
            "securitySchemes": {
                "producerKey": {
                    "type": "http",
                    "scheme": "bearer",
                    "bearerFormat": "wdk_<keyid>_<secret>",
                    "description": "A producer key, minted when the operator activates the producer (D2)."
                }
            }
        }
    });
    // schemars refers to its local definitions as `#/$defs/…`; in the
    // document they live under the components.
    let text = serde_json::to_string(&doc)
        .expect("the document serialises")
        .replace("\"#/$defs/", "\"#/components/schemas/");
    let mut doc: Value = serde_json::from_str(&text).expect("the document parses back");
    // Every reader of the document, the kit and the daemon included, gets
    // the outside developer's text, not the doc comments' own vocabulary.
    schema::scrub(&mut doc);
    doc
}

fn operation(r: &Route) -> Value {
    let mut parameters: Vec<Value> = path_params(r.path)
        .into_iter()
        .map(|p| {
            json!({
                "name": p, "in": "path", "required": true,
                "description": path_param(p).unwrap_or_default(),
                "schema": {"type": "string", "pattern": "^[a-z0-9]+(-[a-z0-9]+)*$"}
            })
        })
        .collect();
    parameters.extend(r.headers.iter().map(header_param));

    let mut op = json!({
        "operationId": r.id,
        "summary": r.summary,
        "parameters": parameters,
        "responses": responses(r),
    });
    let obj = op.as_object_mut().expect("an operation is an object");
    match r.body {
        Body::None => {}
        Body::Json(name) => {
            obj.insert(
                "requestBody".into(),
                json!({"required": true, "content": {"application/json": {"schema": reference(name)}}}),
            );
        }
        Body::Ndjson => {
            obj.insert(
                "requestBody".into(),
                json!({
                    "required": true,
                    "description": format!(
                        "One `Line` per line, at most {MAX_LINES} lines and {MAX_DECODED_BYTES} decoded bytes, each line at most {MAX_LINE_BYTES} bytes. The batch's id is the SHA-256 of the decoded body."
                    ),
                    "content": {"application/x-ndjson": {"schema": {"type": "string"}, "x-line-schema": reference("Line")}}
                }),
            );
        }
    }
    match r.auth {
        Auth::ProducerKey => {
            obj.insert("security".into(), json!([{"producerKey": []}]));
        }
        Auth::BeforeKey => {
            obj.insert("security".into(), json!([]));
            let tcp = if r.headers.iter().any(|h| h.presence == Presence::OverTcp) {
                "over the TCP listener, with the headers marked `x-wild-over-tcp`"
            } else {
                "over the TCP listener as is"
            };
            obj.insert(
                "x-wild-reach".into(),
                json!(format!(
                    "Before a producer key: over the profile's ingest.sock (mode 0600, the operator's own user), or {tcp}."
                )),
            );
        }
    }
    op
}

fn header_param(h: &Header) -> Value {
    let schema = match h.value {
        HeaderValue::Count => json!({"type": "integer", "minimum": 0}),
        HeaderValue::Token(tokens) => json!({"type": "string", "enum": tokens}),
        HeaderValue::Pattern(p) => json!({"type": "string", "pattern": p}),
    };
    let mut param = json!({
        "name": h.name,
        "in": "header",
        "required": h.presence == Presence::Always,
        "description": h.description,
        "schema": schema,
    });
    if h.presence == Presence::OverTcp {
        param
            .as_object_mut()
            .expect("a parameter is an object")
            .insert("x-wild-over-tcp".into(), json!(true));
    }
    param
}

fn responses(r: &Route) -> Value {
    let mut out = Map::new();
    for a in r.answers {
        let mut resp = json!({"description": a.description});
        if let Some(name) = a.schema {
            resp.as_object_mut()
                .expect("a response is an object")
                .insert(
                    "content".into(),
                    json!({"application/json": {"schema": reference(name)}}),
                );
        }
        out.insert(a.status.to_string(), resp);
    }
    Value::Object(out)
}

fn reference(name: &str) -> Value {
    json!({"$ref": format!("#/components/schemas/{name}")})
}

/// What a path parameter is, and where the producer takes its value from.
pub fn path_param(name: &str) -> Option<&'static str> {
    match name {
        "announcement" => Some("The `announcement` the announce answered."),
        "tribe" => Some(
            "The tribe the operator placed the producer in when saying yes: the `tribe` the \
             announcement's poll answers with the key. A key opens its own tribe's routes \
             alone; another tribe here is answered `401`.",
        ),
        "producer" => Some("The `producer` the manifest names."),
        "slug" => Some("The source's `slug`, as the manifest names it."),
        _ => None,
    }
}

/// The `{param}` segments of a path, in order.
pub fn path_params(path: &str) -> Vec<&str> {
    path.split('/')
        .filter_map(|seg| seg.strip_prefix('{').and_then(|s| s.strip_suffix('}')))
        .collect()
}
