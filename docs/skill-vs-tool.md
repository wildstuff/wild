# Tool vs. Skill — extension layers

> **Read this before adding a new extension point** (LLM-callable
> primitive, plugin export, registry entry, audit row). The
> three-layer model below is load-bearing for Forge, the LLM
> tool-loop, and the eventual MCP / HTTP / Component dispatch
> matrix. A change in one layer should not silently leak into
> another.

## TL;DR

Two nested layers. The upper layer adds metadata to the lower
without altering the lower layer's contract. (A third "Workflow"
layer existed early on and was retired without replacement by
ADR-0038 — multi-step behaviour lives in chief recipes and
ADR-0118 pipelines instead.)

```
┌──────────────────────────────────────────────────────┐
│ Skill (Markdown wrapper around a Tool)                │
│   {persona / when-to-use / examples / roles /         │
│    args_schema / returns_schema / version / source}   │
├──────────────────────────────────────────────────────┤
│ Tool (atomic LLM-callable function)                   │
│   {name, description, json-schema}                    │
└──────────────────────────────────────────────────────┘
```

**Tool** is what the LLM API calls "tool" — Anthropic, OpenAI,
MCP all standardise on the same shape: `name + description +
input-schema`. No examples, no roles, no version, no source.

**Skill** is a Markdown document we author that wraps a Tool with
the metadata an LLM-prompt loop needs to use the Tool well:
worked examples, persona / when-to-use guidance, role-affinity
tags, version, source dispatch info.

## Layer 1 — Tool (atomic, LLM-API-native)

### Definition

A **Tool** is a callable function. The LLM-API contract is
exactly:

```rust
pub struct ToolDefinition {
    pub name: String,             // stable, snake_case_or_kebab
    pub description: String,      // freeform; the LLM reads this
    pub input_schema: serde_json::Value,  // JSON Schema
}
```

That's the whole shape. Every LLM API treats it the same; MCP
servers expose it the same; OCI plugins export the same.

### Where Tools live in the code

| Layer | Type | Where |
|---|---|---|
| Plugin-side WIT contract | `wild:tool-provider/tool-spec` | `wit/tool-provider/tool-provider.wit` |
| Plugin-host registry | `ToolProviderRegistry::CatalogEntry` | `crates/runtime/wild-tool-provider/src/registry.rs` |
| Host-LLM WIT surface | `wild:tools/catalog::tool-spec` | `wit/tools/tools.wit` |
| Hardcoded internal commands | `common::tools::ToolDef` (`TOOLS` const) | `crates/wild-decisions/src/tools.rs` |
| LLM-API wire shape | `common::skill::ToolDefinition` | `crates/common/src/skill.rs` |

### What lives at this layer (ONLY)

- `name`
- `description`
- `input_schema` (JSON Schema)
- *(optional, per call)* `cost_units` returned with the result

### What does NOT live at this layer

- **Examples.** Anthropic's tool-use docs explicitly recommend
  embedding examples in the description string — but the
  *structured* representation lives at the Skill layer. Tools are
  atomic.
- **Roles, persona, when-to-use guidance.** Skill-layer concerns.
- **Version metadata.** Tools are versioned by their containing
  plugin; the Skill layer carries declarative semver.
- **Returns schemas.** Anthropic / OpenAI tool-result is opaque
  text/JSON — no machine-checked output shape on the wire. Skill
  carries `returns_schema` for documentation, host-side validation
  (when we wire it), and audit.

### Forge generates Tools (not Skills)

When Forge builds a new component, it produces:

- A `.wasm` Wasm component implementing
  `wild:tool-provider/tools::list-tools()` and `invoke()`.
- One or more `tool-spec` records (the atomic LLM-callable
  shape).

Forge **does not** author Skill MD. It produces the lowest-level
primitive — a callable function with a JSON Schema. If a forge-
built Tool needs LLM-prompt embedding (examples, when-to-use), an
operator (or Elder) **wraps it in a Skill MD afterwards**.

This is intentional: Forge runs under prompt-injection threat
(ADR-0012 lockdown), so what it produces must be the simplest
possible artefact. The Skill layer above adds editorial metadata
that requires human / Elder review — keeping the layers separate
keeps the trust boundary sharp.

## Layer 2 — Skill (Markdown wrapper around a Tool)

### Definition

A **Skill** is a Markdown document with YAML frontmatter that
wraps a Tool with metadata the LLM-prompt loop needs to use the
Tool well. The Rust shape:

```rust
pub struct SkillSpec {
    pub name: String,                          // mirrors the Tool's name
    pub description: String,                   // gets enriched into the Tool's description on projection
    pub args_schema: serde_json::Value,        // mirrors the Tool's input_schema
    pub returns_schema: Option<serde_json::Value>,
    pub version: String,                       // semver-shaped
    pub examples: Vec<SkillExample>,           // ← the load-bearing addition over Tool
    pub roles: Vec<String>,                    // role-affinity for Worker binding
    pub scope: Vec<SkillScope>,                // operator-judgement / chief-judgement / capability
    pub tools: Vec<String>,                    // ADR-0140: bundle this skill arms when active
    pub activation: Option<String>,            // ADR-0212 amendment: language-general activation anchor
    pub source: SkillSource,                   // builtin / mcp / component / http / prose
}
```

Source: `crates/common/src/skill.rs` (`mod spec`).

### What lives at this layer (BEYOND what Tool has)

| Field | Why it lives at the Skill layer, not the Tool layer |
|---|---|
| `examples: Vec<SkillExample>` | Worked examples don't fit the LLM-tool wire shape (which is `name + description + schema`). They get **folded into the description** at projection time via `SkillSpec::to_tool_definition`. Carrying them structurally lets future surfaces (UI, audit, training-data export) use them independently. |
| `roles: Vec<String>` | The deploy-time validator binds a Skill onto a Worker only when the Worker's role-tag intersects the Skill's `roles`. This is a binding-policy concern, not an LLM concern. |
| `scope: Vec<SkillScope>` | Which reasoning surface may see the skill (`operator-judgement`, `chief-judgement`, `capability`). A skill can be visible to more than one surface. |
| `tools: Vec<String>` | ADR-0140 — the tool bundle this skill arms when active. Validated at pre-push: every name must be a registered tool and the whole bundle must fit under at least one Elder mode ceiling. Not yet consumed at runtime in PR1. |
| `activation: Option<String>` | ADR-0212 amendment — the **language-general activation anchor**: a curated block of representative operator INTENTS the semantic skill-activation tier embeds and cosine-matches the operator ask against, so a skill's reflex fires by MEANING (in any language) instead of a per-language keyword list. Distinct from `description` on purpose — `description` is the model-facing catalog menu (WHAT the skill does), `activation` is the ask-shaped anchor (WHEN an operator wants it). Absent ⇒ the tier falls back to `description`; only skills in `wild_core::…::REGISTERED_ACTIVATION_SKILLS` ride the tier at all. |
| `returns_schema: Option<Value>` | Anthropic / OpenAI tool-result is opaque text. We carry `returns_schema` for *our* host-side validation (when we wire it) and for documentation. Not in the LLM wire shape. |
| `version: String` | Skills are independently versionable artefacts. Tools inherit their version from the containing plugin. |
| `source: SkillSource` | Where to dispatch this Skill: `builtin` / `mcp` / `component` / `http`. The `MergedSkillRegistry` reads this to route invocation. |

### The single seam: `SkillSpec::to_tool_definition`

Every LLM-facing surface goes through this projection:

```rust
let tool_def = skill_spec.to_tool_definition();
// tool_def is a ToolDefinition (Layer 1) — LLM-API ready.
```

The projection:
- Copies `name` and `args_schema` (as `input_schema`) verbatim.
- Folds `examples` into `description` using a stable format
  (`Examples:\n- <name>\n  Input: …\n  Output: …`).
- Drops `roles`, `version`, `returns_schema`, `source` — they
  belong to the Skill layer, not the LLM API.

**Why one seam, not many:** future LLM-facing fields (return-shape
hints, role hints, cost hints) ride either inside `description`
via this projection or via a separate sidecar message. They
**don't** get added to the LLM tool-definition format itself —
that format is industry-standardised.

### Skill MD authoring

```markdown
---
name: web-fetch
version: 0.1.0
source: builtin
tool_id: dispatch_task        # for source: builtin
description: Fetch a URL and return the response body.
args_schema:
  type: object
  properties:
    url: { type: string, format: uri }
  required: [url]
roles: [analyst, crawler]
examples:
  - name: simple
    input: '{"url":"https://example.com"}'
    output: '{"status":200,"body":"…"}'
---

Detailed prose / examples / caveats / persona guidance.
```

The body (everything after the second `---`) is preserved by
`parse_skill_md_with_body` for prompt-template rendering when
operators want to include the prose as a system-prompt section.

### Where Skills live

| Layer | Type | Where |
|---|---|---|
| Storage | `skills` SQLite table | `providers::storage::skills` |
| Resolver | `MergedSkillRegistry` | `wild_host::merged_skill_registry` |
| Source dispatchers | `BuiltinDispatcher`, `McpSourceDispatcher`, `ComponentSourceDispatcher`, `HttpSourceDispatcher` | `common::skill_resolver::*`, `wild_host::*_source_dispatcher` |
| Authored files | `prompts/skills/<n>.md` (Elder bundle), `<profile_root>/elder/skills/<n>.md` (operator overrides), forge-built skills (when authored) | various |

## Extension-point checklist

When you add a new extension point, the question to answer first
is **which layer** it belongs to. Use this checklist:

| Question | Yes → | No → |
|---|---|---|
| Does the LLM API call this directly? | Tool | Skill |
| Does it need worked examples / persona / roles? | Skill | Tool |
| Is it multi-step / has triggers? | a chief recipe / ADR-0118 pipeline | Skill |
| Does Forge generate it? | Tool (always) | — |
| Does the operator author it as Markdown? | Skill | Tool |

### Common mistakes to avoid

- **Don't add `examples` to the Tool layer.** It would break the
  WIT contract every plugin implements (industry-wide it's
  `name + description + input-schema`), bloat the wire shape,
  and duplicate what the Skill layer already carries.
- **Don't add `version` to the Tool layer.** Tools inherit their
  version from the containing plugin; Skills carry their own
  semver because they're independently authored.
- **Don't have Forge author Skills directly.** Forge produces
  Tools (atomic, schema-typed); a Skill MD that wraps a forged
  Tool is authored by Elder or an operator after review. ADR-0012
  Forge-lockdown depends on this separation.
- **Don't bypass `SkillSpec::to_tool_definition`.** When you need
  to project a Skill into an LLM-facing tool, go through this
  helper. Examples-in-description, snake_case naming, etc. — the
  projection owns those rules.

## How `wild:tool-provider/tools` plugin authors ship Skills

Workload-tools (`math-tools`, `http-fetcher`, `pdf-parser`) export
the WIT `tool-spec` shape (atomic — no `examples` field) **plus**
a parallel `list-skill-mds()` export carrying the worked Skill MDs
that wrap each Tool. The host treats `list-skill-mds()` as the
canonical place for examples, role hints, and prose guidance; the
Tool-layer description stays terse.

### WIT contract (`wit/tool-provider/tool-provider.wit`)

```wit
package wild:tool-provider@0.4.0;

interface tools {
    record tool-spec     { name: string, description: string, json-schema: string, }
    record skill-md      { slug: string, body: string, }
    list-tools:     func() -> list<tool-spec>;
    list-skill-mds: func() -> list<skill-md>;
    invoke:         func(name: string, args-json: string) -> result<tool-result, tool-error>;
}
```

A plugin that ships zero MDs returns an empty list — the host falls
back to a description-only `SkillSpec` synthesised from the Tool
catalog. Plugins that ship MDs win the priority race: the
`ComponentSourceDispatcher` resolves authored MDs first.

### Plugin author pattern

Author one `skills/<slug>.md` next to source per Tool:

```text
plugins/tools/math-tools/
├── src/
│   ├── lib.rs
│   ├── math_eval.rs
│   ├── datetime_arith.rs
│   └── unit_convert.rs
└── skills/
    ├── math-eval.md
    ├── datetime-arith.md
    └── unit-convert.md
```

Each MD bundles into the wasm component via `include_str!` so
`cargo component build` packages it inside the OCI image — no
sidecar layer, no separate ConfigMap, no operator-staged file:

```rust
// plugins/tools/math-tools/src/lib.rs
const MATH_EVAL_SKILL:    &str = include_str!("../skills/math-eval.md");
const DATETIME_SKILL:     &str = include_str!("../skills/datetime-arith.md");
const UNIT_CONVERT_SKILL: &str = include_str!("../skills/unit-convert.md");

impl ToolsGuest for Component {
    fn list_skill_mds() -> Vec<SkillMd> {
        vec![
            SkillMd { slug: "math-eval".into(),      body: MATH_EVAL_SKILL.into() },
            SkillMd { slug: "datetime-arith".into(), body: DATETIME_SKILL.into() },
            SkillMd { slug: "unit-convert".into(),   body: UNIT_CONVERT_SKILL.into() },
        ]
    }
    // …list_tools / invoke unchanged
}
```

### MD authoring shape

Standard Skill MD — YAML frontmatter (parsed by
`common::skill::spec::parse_skill_md` into a typed `SkillSpec`),
then prose body with an `## Examples` section. Example heading
hierarchy is fixed: `## Examples` → `### <example name>` →
`Input: …` / `Output: …` lines. The parser lifts each example
into `SkillSpec.examples` so the projection
(`SkillSpec::to_tool_definition`) folds them into the LLM-facing
description.

```markdown
---
name: math-eval
version: 0.1.0
source: component
component_type: math-tools
method: math-eval
description: Evaluate a numeric expression …
args_schema:
  type: object
  properties:
    expr: { type: string }
  required: [expr]
---

# math-eval

Use this when the user asks for a calculation that fits a single
expression — arithmetic, simple trigonometry, …

## Examples

### Basic arithmetic
- Input: `{"expr":"2 + 3 * 4"}`
- Output: `{"result":14}`

### Trig with pre-bound constants
- Input: `{"expr":"sin(pi / 2)"}`
- Output: `{"result":1.0}`
```

### Host-side flow

1. `wild plugin install <component>.wasm` admits the plugin
   through the trust gate (Layer A/B/C lockdown).
2. The wild-host loader wires the plugin into a
   `ToolProviderShim`.
   The shim seeds two boot-time catalogs: `list-tools()` →
   `Vec<ToolSpec>`, and `list-skill-mds()` → `Vec<SkillMd>` parsed
   into typed `SkillSpec`s.
3. `ToolProviderRegistry::build`
   aggregates both catalogs across every loaded plugin.
4. `ComponentSourceDispatcher`
   surfaces the aggregated MDs through the merged `SkillRegistry`.
   `lookup`/`list` prefer authored MDs over synthesised
   description-only specs.
5. The LLM tool-loop renders the `SkillSpec` through
   `to_tool_definition()` — examples now ride in the description
   the LLM actually sees, automatically.

### Why this shape vs. inlining examples in the Tool description

Inlining ("Tool-description stuffing") works at the wire — the
LLM still sees the examples — but loses every other Skill axis:

| Aspect                         | Stuffed in description | `list-skill-mds()` MD |
|--------------------------------|------------------------|------------------------|
| LLM sees worked examples       | ✓                      | ✓                      |
| Examples survive re-projection | ✗ (lost in re-author)  | ✓                      |
| `roles` / `version` / source   | ✗                      | ✓                      |
| Audited / round-tripped to UI  | ✗                      | ✓                      |
| Operator override via local MD | ✗                      | ✓ (registry merge)     |
| Forge can re-emit programmatically | ✗                  | ✓                      |

Plugins authored before 0.2.0 that still inline examples in the
Tool description keep working — `list-skill-mds()` returning
empty is a valid posture.

## Forge plays here

ADR-0012 §6 Layer B locks Forge to producing **`tool-provider`
flavor** components — Tier-2 Wasm with `wild:tool-provider/tools`
exports (@0.2.0). Forge MAY emit Skill MDs into the
component's `skills/` directory + the corresponding
`list-skill-mds()` arm so a forged Tool ships its own primer; the
Skill MD is part of the build artefact, not a sidecar an operator
has to stage. Either way the operator-review gate sits at the
Skill layer — the MD prose is what a human reads before approval.

This separation is what lets Forge run under prompt-injection
threat: the operator-review gate sits at the Skill layer, where
human judgment makes sense ("does this prompt-suggestion stand
up?"), not at the Tool layer, where the gate is purely structural
(WIT exports + capability bundles + Layer-A/B/C lockdown).

## See also

- `docs/skill-registry.md` — `SkillRegistry`
  trait + `MergedSkillRegistry` resolution rules.
- [`docs/plugin-concept.md`](plugin-concept.md) — three-tier
  plugin model that carries Tools.
- ADR-0012 — Forge
  generation lockdown that pins Forge to Tool-layer output.
- `crates/common/src/skill.rs` — `SkillSpec`, `SkillExample`,
  `SkillSpec::to_tool_definition`.
