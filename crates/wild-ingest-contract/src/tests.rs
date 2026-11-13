use serde_json::json;

use crate::batch::{self, BatchId};
use crate::codes::{ConflictCode, LineCode, RefusalCode};
use crate::manifest::{Manifest, SourceDecl};
use crate::openapi;
use crate::record_id;
use crate::reply::{BatchReply, BatchState, BatchStatus, Refusal};
use crate::routes::{self, Auth, Body, ROUTES};
use crate::schema;
use crate::shape::{FieldDecl, FieldKind, KeyClass, ShapeDecl, ShapeRef};

fn span_shape() -> ShapeDecl {
    ShapeDecl {
        name: "window_span".into(),
        version: 1,
        key_class: KeyClass::Live,
        key: vec![],
        fields: vec![
            FieldDecl {
                name: "started".into(),
                kind: FieldKind::Timestamp,
                required: true,
                description: None,
            },
            FieldDecl {
                name: "app_class".into(),
                kind: FieldKind::Text { max_len: 64 },
                required: true,
                description: None,
            },
            FieldDecl {
                name: "customer".into(),
                kind: FieldKind::Reference {
                    target_type: "Kunde".into(),
                },
                required: false,
                description: None,
            },
        ],
        target_type: "Arbeitsspanne".into(),
        description: None,
    }
}

fn session_shape() -> ShapeDecl {
    ShapeDecl {
        name: "agent_usage".into(),
        version: 2,
        key_class: KeyClass::Natural,
        key: vec!["session".into(), "period".into()],
        fields: vec![
            FieldDecl {
                name: "session".into(),
                kind: FieldKind::Text { max_len: 64 },
                required: true,
                description: None,
            },
            FieldDecl {
                name: "period".into(),
                kind: FieldKind::Timestamp,
                required: true,
                description: None,
            },
            FieldDecl {
                name: "cost".into(),
                kind: FieldKind::Number,
                required: false,
                description: None,
            },
        ],
        target_type: "Agentenkosten".into(),
        description: None,
    }
}

fn package_log_shape() -> ShapeDecl {
    ShapeDecl {
        name: "package_event".into(),
        version: 1,
        key_class: KeyClass::Position,
        key: vec![],
        fields: vec![FieldDecl {
            name: "package".into(),
            kind: FieldKind::Text { max_len: 128 },
            required: true,
            description: None,
        }],
        target_type: "Paketereignis".into(),
        description: None,
    }
}

fn record(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().unwrap().clone()
}

// ── D1b: additive only, unknown is safe ─────────────────────────────

/// A code, a state or a field a later daemon adds must not break an older
/// producer: unknown members read as `Unknown`, unknown fields are ignored.
#[test]
fn an_unknown_code_state_or_field_from_a_later_daemon_reads_without_failing() {
    let reply: BatchReply = serde_json::from_value(json!({
        "profile": "default",
        "batch": "ab",
        "state": "archived_in_glacier",
        "commit_mark": 7,
        "a_field_from_v1_9": {"anything": true}
    }))
    .expect("a later daemon's reply still reads");
    assert_eq!(reply.state, BatchState::Unknown);

    let status: BatchStatus = serde_json::from_value(json!({
        "batch": "ab", "state": "processed",
        "conflicts": [{"line": 3, "code": "quantum_conflict"}]
    }))
    .unwrap();
    assert_eq!(status.conflicts[0].code, ConflictCode::Unknown);

    let refusal: Refusal = serde_json::from_value(json!({
        "error": "…", "code": "a_new_refusal", "params": {}
    }))
    .unwrap();
    assert_eq!(refusal.code, RefusalCode::Unknown);
}

/// The known members keep their wire spelling: renaming one would break
/// every producer built against `/v1`.
#[test]
fn the_v1_codes_and_states_keep_their_wire_spelling() {
    let spell = |v: serde_json::Value| v.as_str().unwrap().to_string();
    assert_eq!(spell(json!(RefusalCode::BatchRefused)), "batch_refused");
    assert_eq!(spell(json!(RefusalCode::DaemonRewound)), "daemon_rewound");
    assert_eq!(spell(json!(RefusalCode::Unavailable)), "unavailable");
    assert_eq!(spell(json!(RefusalCode::Paused)), "paused");
    assert_eq!(spell(json!(RefusalCode::NoTools)), "no_tools");
    assert_eq!(spell(json!(RefusalCode::NoSuchCall)), "no_such_call");
    assert_eq!(
        spell(json!(RefusalCode::ContractUnsupported)),
        "contract_unsupported"
    );
    assert_eq!(
        spell(json!(ConflictCode::SourceRewroteHistory)),
        "source_rewrote_history"
    );
    assert_eq!(
        spell(json!(ConflictCode::EpochReassigned)),
        "epoch_reassigned"
    );
    assert_eq!(
        spell(json!(ConflictCode::GenerationReassigned)),
        "generation_reassigned"
    );
    assert_eq!(spell(json!(KeyClass::Natural)), "natural");
    assert_eq!(spell(json!(KeyClass::Position)), "position");
    assert_eq!(spell(json!(KeyClass::Live)), "live");
    assert_eq!(spell(json!(BatchState::Received)), "received");
    assert_eq!(spell(json!(BatchState::Processed)), "processed");
    assert_eq!(spell(json!(BatchState::Failed)), "failed");
    assert_eq!(spell(json!(BatchState::Expired)), "expired");
}

// ── D1, D3: a batch is its content ──────────────────────────────────

#[test]
fn a_batch_is_identified_by_its_decoded_body_alone() {
    let body = b"{\"shape\":\"window_span@1\",\"record\":{}}\n";
    assert_eq!(BatchId::of(body), BatchId::of(body));
    assert_ne!(BatchId::of(body), BatchId::of(b"{}\n"));
    assert_eq!(BatchId::of(body).0.len(), 64);
}

#[test]
fn a_body_parses_into_lines_and_names_every_broken_line() {
    let ok = b"{\"shape\":\"window_span@1\",\"record\":{\"a\":1}}\n\n{\"shape\":\"agent_usage@2\",\"record\":{}}\n";
    let lines = batch::parse(ok).expect("well-formed");
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].shape, "agent_usage@2".parse::<ShapeRef>().unwrap());

    let bad = b"{\"shape\":\"window_span@1\",\"record\":{}}\nnot json\n{\"shape\":\"no_version\",\"record\":{}}\n";
    let rejected = batch::parse(bad)
        .unwrap_err()
        .expect("listed, not refused whole");
    assert_eq!(
        rejected
            .iter()
            .map(|r| (r.line, r.code))
            .collect::<Vec<_>>(),
        vec![(2, LineCode::NotAnObject), (3, LineCode::ShapeUnknown)]
    );
}

#[test]
fn a_body_over_its_bounds_is_refused_whole() {
    let huge = vec![b'x'; batch::MAX_DECODED_BYTES + 1];
    assert_eq!(batch::parse(&huge), Err(None));
    let many = b"{\"shape\":\"a@1\",\"record\":{}}\n".repeat(batch::MAX_LINES + 1);
    assert_eq!(batch::parse(&many), Err(None));
}

// ── D4a: a line is checked against its shape ────────────────────────

#[test]
fn a_line_is_checked_against_its_shape() {
    let s = span_shape();
    let good = record(json!({
        "_epoch": "9f2c", "_seq": 41,
        "started": "2026-10-13T09:15:00Z", "app_class": "foot"
    }));
    assert_eq!(batch::validate_line(&s, &good), Ok(()));

    let cases = [
        (
            json!({"_epoch": "e", "_seq": 1, "started": "2026-10-13T09:15:00Z", "app_class": "foot", "title": "Rechnung"}),
            LineCode::FieldUndeclared,
        ),
        (
            json!({"_epoch": "e", "_seq": 1, "app_class": "foot"}),
            LineCode::FieldMissing,
        ),
        (
            json!({"_epoch": "e", "_seq": 1, "started": "yesterday", "app_class": "foot"}),
            LineCode::FieldWrongKind,
        ),
        (
            json!({"_epoch": "e", "_seq": 1, "started": "2026-10-13T09:15:00Z", "app_class": "x".repeat(65)}),
            LineCode::FieldTooLong,
        ),
        (
            json!({"_seq": 1, "started": "2026-10-13T09:15:00Z", "app_class": "foot"}),
            LineCode::FieldMissing,
        ),
    ];
    for (rec, want) in cases {
        let got = batch::validate_line(&s, &record(rec.clone()))
            .unwrap_err()
            .0;
        assert_eq!(got, want, "{rec}");
    }
}

/// A body is checked against the shapes its source accepts: a line naming
/// another shape or version is unknown, a line that does not fit its shape is
/// named by its number, and a clean body resolves each line to its shape.
#[test]
fn a_body_is_checked_line_by_line_against_its_sources_shapes() {
    let shapes = [span_shape(), session_shape()];
    let span = r#"{"shape":"window_span@1","record":{"_epoch":"e","_seq":1,"started":"2026-10-13T09:15:00Z","app_class":"foot"}}"#;
    let usage =
        r#"{"shape":"agent_usage@2","record":{"session":"s1","period":"2026-10-13T09:00:00Z"}}"#;

    let clean = batch::parse(format!("{span}\n\n{usage}\n").as_bytes()).unwrap();
    let resolved = batch::check_lines(&clean, &shapes).unwrap();
    let names: Vec<&str> = resolved.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["window_span", "agent_usage"]);

    let old_version =
        r#"{"shape":"agent_usage@1","record":{"session":"s1","period":"2026-10-13T09:00:00Z"}}"#;
    let titled = r#"{"shape":"window_span@1","record":{"_epoch":"e","_seq":2,"started":"2026-10-13T09:15:00Z","app_class":"foot","title":"Rechnung"}}"#;
    let body = format!("{span}\n{old_version}\n{titled}\n");
    let rejected =
        batch::check_lines(&batch::parse(body.as_bytes()).unwrap(), &shapes).unwrap_err();
    let got: Vec<(u32, LineCode)> = rejected.iter().map(|r| (r.line, r.code)).collect();
    assert_eq!(
        got,
        [(2, LineCode::ShapeUnknown), (3, LineCode::FieldUndeclared)]
    );
    assert_eq!(rejected[0].params["shape"], "agent_usage@1");
    assert_eq!(rejected[1].params["field"], "title");
}

/// A line is named by its number in the body, blank lines counted, whichever
/// check refuses it: the contract's `line` is "the line's number in the
/// body", and a producer finds the line it must fix by that number.
#[test]
fn a_line_is_named_by_its_number_in_the_body_blank_lines_counted() {
    let shapes = [span_shape(), session_shape()];
    let usage =
        r#"{"shape":"agent_usage@2","record":{"session":"s1","period":"2026-10-13T09:00:00Z"}}"#;
    let old_version =
        r#"{"shape":"agent_usage@1","record":{"session":"s1","period":"2026-10-13T09:00:00Z"}}"#;
    let body = format!("{usage}\n\n{old_version}\n");

    let lines = batch::parse(body.as_bytes()).unwrap();
    let numbers: Vec<u32> = lines.iter().map(|l| l.number).collect();
    assert_eq!(numbers, [1, 3]);
    assert_eq!(batch::line(&lines, 3).map(|l| l.shape.version), Some(1));
    assert!(batch::line(&lines, 2).is_none(), "a blank line is no line");

    let rejected = batch::check_lines(&lines, &shapes).unwrap_err();
    let got: Vec<(u32, LineCode)> = rejected.iter().map(|r| (r.line, r.code)).collect();
    assert_eq!(got, [(3, LineCode::ShapeUnknown)]);
}

/// A window title is exactly the free text D7 promises never to send; the
/// window shape declares no field that could carry it, so the door refuses it.
#[test]
fn a_window_title_is_refused_as_an_undeclared_field() {
    let rec = record(json!({
        "_epoch": "e", "_seq": 1, "started": "2026-10-13T09:15:00Z",
        "app_class": "brave-browser", "title": "Rechnung 2026-031 – Kunde Müller"
    }));
    assert_eq!(
        batch::validate_line(&span_shape(), &rec).unwrap_err().0,
        LineCode::FieldUndeclared
    );
}

#[test]
fn a_timestamp_must_be_rfc3339() {
    let s = session_shape();
    for (ts, ok) in [
        ("2026-10-13T09:15:00Z", true),
        ("2026-10-13T09:15:00.123+05:00", true),
        ("2026-10-13 09:15:00", false),
        ("2026-10-13 (Tuesday)", false),
        ("2026-10-13T09:15:00", false),
    ] {
        let rec = record(json!({"session": "s1", "period": ts}));
        assert_eq!(batch::validate_line(&s, &rec).is_ok(), ok, "{ts}");
    }
}

#[test]
fn a_shape_declaration_is_consistent_with_its_key_class() {
    assert_eq!(span_shape().validate(), Ok(()));
    assert_eq!(session_shape().validate(), Ok(()));

    let mut live_with_key = span_shape();
    live_with_key.key = vec!["started".into()];
    assert!(live_with_key.validate().is_err());

    let mut natural_without_key = session_shape();
    natural_without_key.key.clear();
    assert!(natural_without_key.validate().is_err());

    let mut optional_key = session_shape();
    optional_key.key = vec!["cost".into()];
    assert!(optional_key.validate().is_err());

    assert_eq!(package_log_shape().validate(), Ok(()));
    let mut position_with_key = package_log_shape();
    position_with_key.key = vec!["package".into()];
    assert!(position_with_key.validate().is_err());
}

/// A `position` line carries its file generation and offset in the reserved
/// fields; a line of another class may not.
#[test]
fn a_position_line_carries_its_generation_and_offset() {
    let s = package_log_shape();
    let ok = record(json!({"_generation": "g1", "_offset": 4096, "package": "hyprland"}));
    assert_eq!(batch::validate_line(&s, &ok), Ok(()));

    let no_offset = record(json!({"_generation": "g1", "package": "hyprland"}));
    assert_eq!(
        batch::validate_line(&s, &no_offset),
        Err((LineCode::FieldMissing, json!({"field": "_offset"})))
    );

    let not_a_position = record(json!({
        "_generation": "g1", "_offset": 1,
        "session": "s1", "period": "2026-10-13T09:00:00Z"
    }));
    assert_eq!(
        batch::validate_line(&session_shape(), &not_a_position)
            .unwrap_err()
            .0,
        LineCode::FieldUndeclared
    );

    let at = |offset: u64| {
        record_id::compose(
            "dev-a",
            &s,
            &record(json!({"_generation": "g1", "_offset": offset, "package": "x"})),
        )
        .unwrap()
    };
    assert_eq!(at(4096), at(4096), "a re-read gives the same id");
    assert_ne!(at(4096), at(4160), "another offset is another record");
}

// ── D4: record ids are scoped to their device; references are not ──

/// A record id is one readable text that every store keeps as one key: its
/// parts joined by `:`, a part's own `%` and `:` escaped. It carries no
/// control character — a store that joins a composite key with one (the
/// Wild does, with U+001F) would otherwise split one id into several parts —
/// and two different part lists never run together into one id. A producer's
/// key may hold any character.
#[test]
fn a_record_id_is_one_readable_text_whose_parts_cannot_run_together() {
    let s = session_shape();
    let rec = record(json!({"session": "s:1", "period": "2026-10-13T09:00:00Z"}));
    assert_eq!(
        record_id::compose("dev-a", &s, &rec).unwrap(),
        "dev-a:agent_usage:s%3A1:2026-10-13T09%3A00%3A00Z"
    );
    let a = record_id::compose(
        "dev-a",
        &s,
        &record(json!({"session": "x:y", "period": "z"})),
    );
    let b = record_id::compose(
        "dev-a",
        &s,
        &record(json!({"session": "x", "period": "y:z"})),
    );
    assert_ne!(a, b, "parts that hold the separator never run together");
    let odd = record(json!({"session": "100%\u{1f}", "period": "p"}));
    let id = record_id::compose("dev-a", &s, &odd).expect("any character may be in a key");
    assert!(!id.chars().any(char::is_control), "{id:?}");
}

#[test]
fn a_record_id_is_scoped_to_its_device_and_a_reference_is_not() {
    let s = session_shape();
    let rec = record(json!({"session": "s1", "period": "2026-10-13T09:00:00Z", "cost": 0.4}));
    let a = record_id::compose("dev-a", &s, &rec).unwrap();
    let b = record_id::compose("dev-b", &s, &rec).unwrap();
    assert_ne!(a, b, "two devices never share a record id");
    assert_eq!(
        a,
        record_id::compose("dev-a", &s, &rec).unwrap(),
        "a re-read gives the same id"
    );

    let span = span_shape();
    let with_ref = record(json!({
        "_epoch": "e1", "_seq": 7, "started": "2026-10-13T09:15:00Z",
        "app_class": "foot", "customer": "K-0042"
    }));
    let id = record_id::compose("dev-a", &span, &with_ref).unwrap();
    assert!(id.starts_with("dev-a"));
    assert_eq!(
        with_ref["customer"], "K-0042",
        "the reference keeps the target's own key"
    );

    let missing = record(json!({"session": "s1"}));
    assert_eq!(record_id::compose("dev-a", &s, &missing), None);
}

// ── D16: a manifest is consistent ───────────────────────────────────

/// #12258 — what a manifest announces, by source: each slug with its shapes,
/// in the producer's order. The one projection every door that reads an
/// announcement takes.
#[test]
fn a_manifest_announces_each_source_with_its_shapes_in_order() {
    let m = Manifest {
        producer: "wild-omarchy".into(),
        version: "0.1.0".into(),
        contract_version: crate::CONTRACT_VERSION.into(),
        tribe: None,
        sources: vec![
            SourceDecl {
                slug: "windows".into(),
                shapes: vec![span_shape(), session_shape()],
                description: Some("Window focus".into()),
            },
            SourceDecl {
                slug: "packages".into(),
                shapes: vec![package_log_shape()],
                description: None,
            },
        ],
        references: vec![],
        needs_vocabulary: false,
        fingerprint: "hmac".into(),
        tools: vec![],
        answers_for: vec![],
    };
    assert_eq!(
        m.announced(),
        vec![
            ("windows".to_string(), vec![span_shape(), session_shape()]),
            ("packages".to_string(), vec![package_log_shape()]),
        ]
    );
}

#[test]
fn a_manifest_names_its_contract_and_valid_sources() {
    let m = Manifest {
        producer: "wild-omarchy".into(),
        version: "0.1.0".into(),
        contract_version: crate::CONTRACT_VERSION.into(),
        tribe: None,
        sources: vec![SourceDecl {
            slug: "windows".into(),
            shapes: vec![span_shape()],
            description: None,
        }],
        references: vec!["Kunde".into()],
        needs_vocabulary: true,
        fingerprint: "hmac".into(),
        tools: vec![],
        answers_for: vec![],
    };
    assert_eq!(m.validate(), Ok(()));

    // A description is optional, says something, and is bounded — on the
    // source, the shape and each field.
    let mut described = m.clone();
    described.sources[0].description = Some("Window focus on this laptop".into());
    described.sources[0].shapes[0].description = Some("One span of one window".into());
    described.sources[0].shapes[0].fields[0].description = Some("The app's name".into());
    assert_eq!(described.validate(), Ok(()));
    let mut empty = described.clone();
    empty.sources[0].shapes[0].fields[0].description = Some("  ".into());
    assert!(empty.validate().unwrap_err().contains("leave it out"));
    let mut long = described.clone();
    long.sources[0].description = Some("x".repeat(crate::shape::MAX_DESCRIPTION_CHARS + 1));
    assert!(long.validate().unwrap_err().contains("over 280 characters"));

    let mut twice = m.clone();
    twice.sources.push(twice.sources[0].clone());
    assert!(twice.validate().is_err());

    let mut no_contract = m.clone();
    no_contract.contract_version.clear();
    assert!(no_contract.validate().is_err());

    // Wild builds paths, queue references and type names from these.
    for bad in [
        "Wild-Omarchy",
        "wild/omarchy",
        "../up",
        "wild--omarchy",
        "-x",
        "",
    ] {
        let mut named = m.clone();
        named.producer = bad.into();
        assert!(named.validate().is_err(), "producer `{bad}` passed");
        let mut slugged = m.clone();
        slugged.sources[0].slug = bad.into();
        assert!(slugged.validate().is_err(), "slug `{bad}` passed");
    }
    for bad in ["1Typ", "Ar beit", "Größe", "a-b", ""] {
        let mut typed = m.clone();
        typed.sources[0].shapes[0].target_type = bad.into();
        assert!(typed.validate().is_err(), "type `{bad}` passed");
    }
    // A shape name is what a line names (`name@version`): refused at the
    // announcement, not after the operator's yes, when no line could name it.
    for bad in ["Span", "window-span", "window.span", ""] {
        let mut shaped = m.clone();
        shaped.sources[0].shapes[0].name = bad.into();
        let e = shaped.validate().expect_err(bad);
        assert!(e.contains("is not a shape name"), "{bad}: {e}");
    }
}

/// The published schemas say what `validate` refuses about a name, so a
/// producer built against them alone never meets the refusal (#11171).
#[test]
fn the_manifest_schema_publishes_the_name_rules() {
    let s = schemars::schema_for!(Manifest).to_value();
    let prop = |at: &str| s.pointer(at).cloned().unwrap_or_default();
    for at in ["/properties/producer", "/$defs/SourceDecl/properties/slug"] {
        assert_eq!(prop(at)["maxLength"], 48, "{at}");
        assert_eq!(prop(at)["pattern"], "^[a-z0-9]+(-[a-z0-9]+)*$", "{at}");
    }
    let target = prop("/$defs/ShapeDecl/properties/target_type");
    assert_eq!(target["maxLength"], 48);
    assert_eq!(target["pattern"], "^[A-Za-z][A-Za-z0-9_]*$");
    assert_eq!(
        prop("/$defs/ShapeDecl/properties/name")["pattern"],
        "^[a-z0-9_]+$"
    );
}

// ── the routes and the OpenAPI document ─────────────────────────────

#[test]
fn every_route_is_unique_and_names_only_published_schemas() {
    let names: std::collections::BTreeSet<&str> =
        schema::components().iter().map(|(n, _)| *n).collect();
    let mut seen = std::collections::BTreeSet::new();
    for r in ROUTES {
        assert!(seen.insert(r.id), "route id `{}` twice", r.id);
        if let Body::Json(n) = r.body {
            assert!(
                names.contains(n),
                "{}: body schema `{n}` is not published",
                r.id
            );
        }
        for a in r.answers {
            if let Some(n) = a.schema {
                assert!(
                    names.contains(n),
                    "{}: answer schema `{n}` is not published",
                    r.id
                );
            }
        }
        if r.path.starts_with("/tribes/") {
            assert_eq!(
                r.auth,
                Auth::ProducerKey,
                "{}: a tribe route needs a producer key",
                r.id
            );
        }
    }
}

/// The document's every `$ref` resolves to a component, so a generated
/// client never meets a dangling reference.
#[test]
fn the_openapi_document_has_no_dangling_reference() {
    let doc = openapi::document();
    let comps = doc["components"]["schemas"].as_object().unwrap();
    let text = doc.to_string();
    let mut rest = text.as_str();
    let mut refs = 0;
    while let Some(i) = rest.find("#/components/schemas/") {
        let tail = &rest[i + "#/components/schemas/".len()..];
        let name: String = tail.chars().take_while(|c| *c != '"').collect();
        assert!(comps.contains_key(&name), "dangling $ref to `{name}`");
        refs += 1;
        rest = tail;
    }
    assert!(
        refs > 10,
        "the check saw {refs} references; it cannot be looking"
    );
    assert!(
        !text.contains("#/$defs/"),
        "a schemars-local reference survived"
    );
    assert_eq!(doc["paths"].as_object().unwrap().len(), ROUTES.len());
}

/// What a client cannot see in the document, it cannot send: each header a
/// route reads is a parameter of its operation, and an announcement over TCP
/// names its pairing code. Found by an outside agent building against the
/// published document alone (#9898): the code was nowhere in it.
#[test]
fn every_header_a_route_reads_is_in_the_document() {
    let doc = openapi::document();
    let mut seen = 0;
    for r in ROUTES {
        let path = format!("{}{}", crate::ROUTE_PREFIX, r.path);
        let op = &doc["paths"][&path][r.method.as_str()];
        let params: Vec<&str> = op["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["in"] == "header")
            .map(|p| p["name"].as_str().unwrap())
            .collect();
        let declared: Vec<&str> = r.headers.iter().map(|h| h.name).collect();
        assert_eq!(params, declared, "{}: header parameters", r.id);
        seen += params.len();
    }
    assert!(
        seen >= 4,
        "saw {seen} header parameters; it cannot be looking"
    );
    let announce = ROUTES.iter().find(|r| r.id == "announce").unwrap();
    assert!(announce
        .headers
        .iter()
        .any(|h| h.name == crate::device::PAIRING_CODE_HEADER
            && h.presence == routes::Presence::OverTcp));
    let send = ROUTES.iter().find(|r| r.id == "send_batch").unwrap();
    for name in [crate::device::COMMIT_MARK_HEADER, "content-encoding"] {
        assert!(
            send.headers.iter().any(|h| h.name == name),
            "send_batch reads `{name}`"
        );
    }
    let reach = doc["paths"]["/api/v1/producers/announcements"]["post"]["x-wild-reach"]
        .as_str()
        .unwrap();
    assert!(
        reach.contains("TCP"),
        "the reach names the TCP listener: {reach}"
    );
}

/// The batch bounds and the reserved key fields are published from the
/// constants that enforce them, so a producer can size its batches and fill
/// its `position` and `live` keys without guessing (#9898).
#[test]
fn the_bounds_and_reserved_fields_are_published_from_their_constants() {
    let doc = openapi::document();
    let b = &doc["x-wild-bounds"];
    assert_eq!(b["max_lines"], json!(batch::MAX_LINES));
    assert_eq!(b["max_decoded_bytes"], json!(batch::MAX_DECODED_BYTES));
    assert_eq!(b["max_line_bytes"], json!(batch::MAX_LINE_BYTES));
    assert_eq!(
        b["max_compression_ratio"],
        json!(batch::MAX_COMPRESSION_RATIO)
    );

    let key_class = doc["components"]["schemas"]["KeyClass"].to_string();
    for field in [
        record_id::POSITION_GENERATION_FIELD,
        record_id::POSITION_OFFSET_FIELD,
        record_id::LIVE_EPOCH_FIELD,
        record_id::LIVE_COUNTER_FIELD,
    ] {
        assert!(
            key_class.contains(&format!("`{field}`")),
            "KeyClass names the reserved field `{field}`: {key_class}"
        );
    }

    let epoch = doc["components"]["schemas"]["Epoch"].to_string();
    assert!(
        epoch.contains("^[0-9a-f]{32}$"),
        "the epoch has its pattern: {epoch}"
    );
}

/// A producer is told where each value in a route's path comes from: two
/// agents building producers on 2026-10-30 hard-coded `{tribe}` as `tribe`.
#[test]
fn every_path_parameter_says_where_its_value_comes_from() {
    let mut seen = 0;
    for route in routes::ROUTES {
        for p in openapi::path_params(route.path) {
            seen += 1;
            assert!(
                openapi::path_param(p).is_some_and(|d| !d.is_empty()),
                "`{{{p}}}` in {} has no description in `openapi::path_param`",
                route.path
            );
        }
    }
    assert!(seen >= 4, "the routes name their parameters: {seen}");
    let doc = openapi::document().to_string();
    assert!(
        doc.contains("the `tribe` the announcement's poll answers with the key"),
        "the document carries them"
    );
}

#[test]
fn a_path_names_its_parameters() {
    assert_eq!(
        openapi::path_params("/tribes/{tribe}/producers/{producer}/sources/{slug}/batches"),
        vec!["tribe", "producer", "slug"]
    );
}

/// The introduction names routes and headers a developer will type: each one
/// must be one the contract serves.
#[test]
fn the_introduction_names_only_real_routes_and_headers() {
    let intro = openapi::INTRO;
    let mut named = 0;
    for path in intro
        .split('`')
        .filter_map(|tok| tok.split_once(' ').map(|(_, p)| p).or(Some(tok)))
        .filter(|tok| tok.starts_with('/'))
    {
        named += 1;
        assert!(
            ROUTES.iter().any(|r| r.path == path),
            "the introduction names `{path}`, which no route serves"
        );
    }
    assert!(named >= 4, "the introduction names the four steps' routes");
    assert!(
        intro.contains(&format!("`{}`", crate::device::PAIRING_CODE_HEADER)),
        "the pairing header is named as the door reads it"
    );
    assert!(
        !intro.contains("ADR-"),
        "the introduction speaks to developers, not to Wild's records"
    );
}

/// What the outside walk had to guess, the introduction now says: what
/// "quiet" means beside `--patience`, which key wins after a `rotate_key`,
/// and where a producer's key opens MCP.
#[test]
fn the_introduction_says_what_quiet_and_a_rotated_key_mean() {
    let intro = openapi::INTRO;
    for said in [
        "quiet means not asking the door",
        "polling for the yes",
        "a wait the last answer told it to keep",
        "after a `rotate_key` the key file your producer reads must hold the new key",
        "the start after the rotation presents that one",
        "`https://<door>/mcp`",
        "missing or invalid bearer token",
    ] {
        assert!(
            intro.contains(said),
            "the introduction no longer says {said:?}"
        );
    }
}

#[test]
fn published_documents_carry_no_rustdoc_link_or_decision_tag() {
    for (name, value) in schema::documents() {
        let found = schema::leaks(&value);
        assert!(
            found.is_empty(),
            "{name} leaks internal references: {found:?}"
        );
    }
}

#[test]
fn the_leak_check_names_a_link_an_adr_and_a_decision_tag() {
    for leaked in [
        "see [`crate::x`]",
        "per ADR-0001",
        "kept (D2) for a day",
        "a path crate::x::y",
    ] {
        let doc = serde_json::json!({"description": leaked});
        assert_eq!(schema::leaks(&doc).len(), 1, "{leaked}");
    }
    let clean = serde_json::json!({"description": "A function (days) and Dx"});
    assert!(schema::leaks(&clean).is_empty());
}

#[test]
fn scrubbing_drops_tags_and_resolves_links() {
    let mut doc = serde_json::json!({"a": [
        "has received (D4a-R).",
        "a 409 (HTTP 409, ADR-0398 D4). Drop",
        "What (ADR-0398 D3): the values",
        "see [`crate::device::Rewound`] and (a note)",
    ]});
    schema::scrub_for_test(&mut doc);
    assert_eq!(
        doc,
        serde_json::json!({"a": [
            "has received.",
            "a 409 (HTTP 409). Drop",
            "What: the values",
            "see `Rewound` and (a note)",
        ]})
    );
}
