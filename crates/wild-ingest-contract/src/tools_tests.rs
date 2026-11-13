//! ADR-0398 D1 — what the door refuses of an announced tool, and what it
//! lets through.

use serde_json::json;

use super::*;
use crate::manifest::Manifest;

/// `agent_start` as the host-fleet sample announces it: sound.
pub(crate) fn agent_start() -> ToolDecl {
    ToolDecl {
        name: "agent_start".into(),
        description: "Starts an agent on this host".into(),
        schema: json!({
            "type": "object",
            "properties": {
                "host": {"type": "string", "description": "The host it starts on"},
                "prompt": {"type": "string"},
                "idempotency_key": {"type": "string"}
            },
            "required": ["host", "prompt"]
        }),
        acts: Some(ToolActs::Outward),
        risk: Some(ToolRisk::High),
        routes_by: Some("host".into()),
    }
}

fn refusal(t: ToolDecl) -> String {
    t.validate().expect_err("refused")
}

#[test]
fn a_sound_tool_is_let_through() {
    assert_eq!(agent_start().validate(), Ok(()));
    let mut reads = agent_start();
    reads.acts = Some(ToolActs::Reads);
    reads.risk = None;
    reads.routes_by = None;
    assert_eq!(reads.validate(), Ok(()));
}

#[test]
fn a_tool_whose_name_is_not_a_tool_name_is_refused_by_name() {
    for bad in [
        "agent.start",
        "AgentStart",
        "agent-start",
        "agent start",
        "",
        &"a".repeat(65),
    ] {
        let mut t = agent_start();
        t.name = bad.to_string();
        let e = refusal(t);
        assert!(e.contains("is not a tool name"), "{bad:?}: {e}");
        assert!(e.contains(&format!("`{bad}`")), "the refusal names it: {e}");
    }
    let mut longest = agent_start();
    longest.name = "a".repeat(64);
    assert_eq!(longest.validate(), Ok(()));
}

#[test]
fn a_tool_that_does_not_say_what_it_acts_on_is_refused() {
    let mut t = agent_start();
    t.acts = None;
    let e = refusal(t);
    assert!(e.contains("tool `agent_start`"), "{e}");
    assert!(e.contains("does not say what it acts on"), "{e}");
}

#[test]
fn a_tool_that_says_it_writes_is_refused_as_not_a_connectors_word() {
    // Read off the wire, as the door reads it: `writes` is no member.
    let mut announced = serde_json::to_value(agent_start()).unwrap();
    announced["acts"] = json!("writes");
    let t: ToolDecl = serde_json::from_value(announced).unwrap();
    assert_eq!(t.acts, Some(ToolActs::Unknown));
    let e = refusal(t);
    assert!(e.contains("tool `agent_start`"), "{e}");
    assert!(e.contains("`writes` is not its word"), "{e}");
}

#[test]
fn a_tool_schema_using_a_keyword_wild_does_not_check_is_refused() {
    let mut top = agent_start();
    top.schema["allOf"] = json!([]);
    let e = refusal(top);
    assert!(e.contains("`schema` uses `allOf`"), "{e}");

    // A keyword deep in a property is found where the call check would meet it.
    let mut deep = agent_start();
    deep.schema["properties"]["prompt"]["multipleOf"] = json!(2);
    let e = refusal(deep);
    assert!(
        e.contains("`schema.properties.prompt` uses `multipleOf`"),
        "{e}"
    );
    let mut items = agent_start();
    items.schema["properties"]["tags"] =
        json!({"type": "array", "items": {"type": "string", "uniqueItems": true}});
    let e = refusal(items);
    assert!(
        e.contains("`schema.properties.tags.items` uses `uniqueItems`"),
        "{e}"
    );

    // Every shape the call check can reach is walked: a branch, a `$defs`
    // entry, a `definitions` entry.
    for (holder, at) in [
        (
            json!({"anyOf": [{"type": "string"}, {"not": {}}]}),
            "anyOf[1]",
        ),
        (json!({"oneOf": [{"not": {}}]}), "oneOf[0]"),
    ] {
        let mut t = agent_start();
        t.schema["properties"]["mode"] = holder;
        let e = refusal(t);
        assert!(
            e.contains(&format!("`schema.properties.mode.{at}` uses `not`")),
            "{e}"
        );
    }
    for holder in ["$defs", "definitions"] {
        let mut t = agent_start();
        t.schema[holder] = json!({"x": {"not": {}}});
        let e = refusal(t);
        assert!(
            e.contains(&format!("`schema.{holder}.x` uses `not`")),
            "{e}"
        );
    }

    let mut not_an_object = agent_start();
    not_an_object.schema = json!("object");
    assert!(refusal(not_an_object).contains("is not a JSON Schema object"));
}

/// What common MCP servers' schemas carry passes: the keywords the call
/// check enforces, the annotations, and an `x-` key.
#[test]
fn a_tool_schema_using_what_wild_checks_and_notes_is_let_through() {
    let mut t = agent_start();
    t.schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    t.schema["$defs"] = json!({"name": {"type": "string", "minLength": 1, "maxLength": 64}});
    t.schema["properties"]["mode"] = json!({
        "anyOf": [{"$ref": "#/$defs/name"}, {"const": "auto"}],
        "default": "auto",
        "x-mcp-header": "X-Mode",
    });
    t.schema["properties"]["tags"] = json!({
        "type": ["array", "null"],
        "items": {"type": "string", "pattern": "^[a-z]+$"},
        "maxItems": 5,
    });
    t.validate().expect("every key is checked or a note");
}

#[test]
fn a_tool_without_an_idempotency_key_is_refused() {
    let mut t = agent_start();
    t.schema["properties"]
        .as_object_mut()
        .unwrap()
        .remove("idempotency_key");
    let e = refusal(t);
    assert!(e.contains("tool `agent_start`"), "{e}");
    assert!(e.contains("declares no `idempotency_key` property"), "{e}");
}

#[test]
fn a_tool_that_routes_by_an_argument_it_does_not_declare_is_refused() {
    let mut t = agent_start();
    t.routes_by = Some("machine".into());
    assert!(refusal(t).contains("routes by `machine`"));
}

#[test]
fn a_tool_with_no_description_or_an_unknown_risk_is_refused() {
    let mut quiet = agent_start();
    quiet.description = "  ".into();
    assert!(refusal(quiet).contains("says nothing of what it does"));
    let mut announced = serde_json::to_value(agent_start()).unwrap();
    announced["risk"] = json!("critical");
    let t: ToolDecl = serde_json::from_value(announced).unwrap();
    assert!(refusal(t).contains("`risk` is `low`, `medium` or `high`"));
}

fn manifest() -> Manifest {
    serde_json::from_value(json!({
        "producer": "kit-host-fleet",
        "version": "0.1.0",
        "contract_version": crate::CONTRACT_VERSION,
        "sources": [{"slug": "hosts", "shapes": [{
            "name": "host_load", "version": 1, "key_class": "live", "key": [],
            "target_type": "HostLoad",
            "fields": [{"name": "host", "kind": "text", "max_len": 64, "required": true}]
        }]}],
        "fingerprint": "hmac"
    }))
    .expect("a manifest without tools reads, as before them")
}

#[test]
fn a_manifest_without_tools_reads_and_writes_as_before_them() {
    let m = manifest();
    assert!(m.tools.is_empty());
    assert_eq!(m.validate(), Ok(()));
    assert!(
        serde_json::to_value(&m).unwrap().get("tools").is_none(),
        "a connector that only sends announces no `tools`"
    );
}

#[test]
fn a_manifest_refuses_a_tool_it_cannot_call_and_one_announced_twice() {
    let mut m = manifest();
    m.tools = vec![agent_start()];
    m.answers_for = vec!["buero-laptop".into()];
    assert_eq!(m.validate(), Ok(()));
    m.tools[0].acts = None;
    assert!(m.validate().unwrap_err().contains("tool `agent_start`"));
    let mut twice = manifest();
    twice.tools = vec![agent_start(), agent_start()];
    twice.answers_for = vec!["buero-laptop".into()];
    assert!(twice
        .validate()
        .unwrap_err()
        .contains("tool `agent_start` is announced twice"));
}

/// A tool that routes by an argument needs values to route by (ADR-0398
/// D3): a device that claims none is let in, placed, and then answers
/// `unreachable` to every call for that tool. The refusal names the tool,
/// while the producer can still say which values it answers for.
#[test]
fn a_routing_tool_with_no_value_to_answer_for_is_refused_by_name() {
    let mut m = manifest();
    m.tools = vec![agent_start()];
    let e = m
        .validate()
        .expect_err("a routing tool answers for nothing");
    assert!(
        e.contains("tool `agent_start`"),
        "the refusal names it: {e}"
    );
    assert!(e.contains("names no value it answers for"), "{e}");
    // A tool that routes by nothing needs no value: it has one device per
    // tribe, chosen by tribe alone.
    m.tools[0].routes_by = None;
    assert_eq!(m.validate(), Ok(()));
}

#[test]
fn every_announced_field_of_a_tool_is_part_of_its_build() {
    let mut m = manifest();
    m.tools = vec![agent_start()];
    let first = m.tools_digest();
    assert!(
        first.starts_with("sha256:") && first.len() == 7 + 64,
        "{first}"
    );
    assert_eq!(m.tools_digest(), first, "the same tools are the same build");
    let changed: [fn(&mut ToolDecl); 4] = [
        |t| t.description.push('!'),
        |t| t.risk = Some(ToolRisk::Medium),
        |t| t.acts = Some(ToolActs::Reads),
        |t| t.schema["properties"]["prompt"]["description"] = json!("What it is told"),
    ];
    for change in changed {
        let mut other = m.clone();
        change(&mut other.tools[0]);
        assert_ne!(other.tools_digest(), first);
    }
}

#[test]
fn the_tool_name_rule_is_published_in_the_manifest_schema() {
    let s = schemars::schema_for!(Manifest).to_value();
    let name = s
        .pointer("/$defs/ToolDecl/properties/name")
        .cloned()
        .unwrap_or_default();
    assert_eq!(name["pattern"], "^[a-z0-9_]{1,64}$");
}

#[test]
fn the_same_tools_listed_in_another_order_are_the_same_build() {
    let mut m = manifest();
    let mut second = agent_start();
    second.name = "agent_stop".into();
    m.tools = vec![agent_start(), second];
    let first = m.tools_digest();
    m.tools.reverse();
    assert_eq!(m.tools_digest(), first);
}

#[test]
fn only_a_declared_read_is_said_as_reading() {
    let with = |acts| ToolDecl {
        acts,
        ..agent_start()
    };
    assert!(!with(Some(ToolActs::Reads)).outward());
    for acts in [Some(ToolActs::Outward), Some(ToolActs::Unknown), None] {
        assert!(with(acts).outward(), "{acts:?} was said as only reading");
    }
}
