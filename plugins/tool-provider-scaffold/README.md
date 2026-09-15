# tool-provider-scaffold

Declarative macros for Tier-2 `wild:tool-provider@0.4.0` plugins.
Every tool-provider shares the same frame — a `Component` struct, the
`wild:tool-provider/tools` and `wild:plugin-meta/meta` `Guest` impls,
the `export!`, an identical typed-signature projection — and only the
tool table, the WIT `requires` and the signatures actually vary. The
macros here generate the frame; your plugin declares what varies.

They are deliberately `macro_rules!`, not a proc-macro: the
bindgen-emitted types (`exports::wild::…`) live in the *calling*
crate, and `macro_rules!` paths resolve at the call site, so each
expansion binds to your plugin's own `wit_bindgen::generate!` output.
Invoke the macros **after** `wit_bindgen::generate!`, at the plugin
crate root.

## The macros

| Macro | Generates | Use when |
|---|---|---|
| `tool_provider_plugin!` | the whole plugin surface: `Component`, both `Guest` impls, `export!` | your plugin is a stateless tool-provider and nothing else |
| `impl_to_manifest_sig!` | the `sig::ToolSig` → manifest `Sig` projection | you declare typed `signatures` |
| `egress_fault_mapping!` | the `wild:http` egress-error → fault mapping, in five forms | your plugin imports `wild:http/outbound` |
| `enumerate_result_schema!` | the canonical `enumerate` result JSON schema | your plugin is a source connector |

### `tool_provider_plugin!`

Generates the full plugin: `list_tools` and `list_skill_mds` from your
tool table, an async `invoke` that dispatches by tool name (unknown
names return `unknown-tool`), a `manifest` built from your `slug` /
`provides` / `requires` / `signatures`, and stateless `init` /
`shutdown`. The version is read from `CARGO_PKG_VERSION`.

The macro *fixes* the parts that never vary across tool-providers:
`kind: Provider`, empty `config_keys`, empty `secret_aliases`, no-op
lifecycle. A plugin that needs config bytes, secret aliases, or extra
WIT exports (a worker handler, a function backing) writes its meta
impl by hand instead — see the SharePoint example below for that
shape.

Each `tools:` entry names the registered tool name (the `invoke`
dispatch key), a `ToolSpec` expression, and an invoke fn called as
`f(&args_json)` — so a sync tool is
`fn(&str) -> Result<ToolResult, ToolError>`. Optionally it bundles a
skill markdown as `skill: ("slug" => BODY)`, served through
`list_skill_mds`. Two optional trailing lists:

- `streaming_tools:` — same entry shape, but the invoke fn is
  `async fn` and is `.await`ed, so it can drain a
  component-model-async `stream<u8>` import while decoding a large
  source in place. A tool that never awaits stays in the plain
  `tools:` list.
- `prose_skills:` — tool-less `(slug => body)` guidance markdowns
  (an operator setup guide, say). No invoke arm; the body is
  read-only knowledge.

**One contract trap — what `args_json` holds.** Called directly (an
LLM tool-call, `wild tool invoke`, a host caller), `args_json` is
exactly the flat object your `ToolSpec` schema describes. A *flow
stage* invoking your tool sends an envelope instead:
`{ "item": { …the walking record… }, "config": { …the stage's args… } }` —
the target takes its subject from `item` and its knobs from `config`.
Write flat parameters (the default, right for every tool an LLM or an
operator calls), or accept the envelope deliberately; one
implementation does not serve both without branching on the shape.

### `impl_to_manifest_sig!`

Generates `to_manifest_sig`, the projection from your plugin's static
typed signature to the bindgen manifest `Sig`. It expects a `sig`
module in scope exposing
`ToolSig { name, description, input_schema, output_schema, examples: &[(in, out)] }`.
Invoke it before `tool_provider_plugin!` so the `signatures:` entries
can call it.

### `egress_fault_mapping!`

The mapping from the five typed `wild:http/outbound` errors
(`denied-host`, `denied-auth`, `rate-limited`, `timeout`,
`transport`) to what leaves the plugin. Usable by *any* plugin with a
`wild:http/outbound` import, not just tool-providers. Five forms, by
how much your plugin shares:

| Form | Adds | Extra input |
|---|---|---|
| `describe_error` | the stable `<kind>: <detail>` string renderer | — |
| `fault_class` | + `egress_fault_class`, the typed variant → `fault-class` read | `tools:` path |
| `connector_fault` | + `egress_cause` and the carried-fault constructors `carried_egress_error` / `carried_status_error`, which put cause · message · remedy as JSON inside `execution-failed` | `tools:` path, `wild-core` dep |
| `classified_error` | + a named `{ cause, message }` error struct with `unknown` / `context` / `into_tool_error`, `classify_ob_error`, and a non-2xx status classifier whose message you shape via `status_message` | `tools:` path, `wild-core` dep |
| `custom_map` | just the exhaustive five-variant match onto your *own* error enum — you keep the messages | — |

Every form's match is exhaustive over the five variants, so a new
`wild:http` error variant breaks every consumer loudly instead of
silently widening. Two mappings worth knowing: `denied-auth` is an
auth fault (the operator fixes the key or binding), while
`denied-host` is a *config* fault — the tribe's own egress allowlist
refused the address and the remote was never asked, so signing in
again cannot help.

### `enumerate_result_schema!`

The one declaration of the source-connector `enumerate` result shape:
an envelope of `source_kind` + `items`, each item carrying

| Key | Required | Meaning |
|---|---|---|
| `item_ref` | yes | the item's durable locator — a path, a URL, an item id |
| `name` | yes | the human-facing filename |
| `size_bytes` | no | size in bytes — the inventory pair: emit both whenever the origin knows them |
| `modified_at` | no | last change, ISO-8601 — omit only when the origin genuinely has neither |

Per-connector item extras go in as
`enumerate_result_schema!(extra: r#","fields": { "type": "object" }"#)`;
a `no_inventory` arm drops the inventory pair for origins that can
never fill it (a queue row, a URL list gathered before any HTTP
call), and an envelope arm adds top-level keys (pagination) via
`top_extra:` / `top_required:`. The `ENUMERATE_ITEM_KEYS` const is
the same key list for checkers and tests to assert against.

## Usage sketch

```rust
// src/lib.rs of your plugin — a world exporting
// wild:tool-provider/tools and wild:plugin-meta/meta.
wit_bindgen::generate!({ path: "wit", world: "my-connector", generate_all });

mod sig;    // ToolSig statics: name, description, input/output schema, examples
mod tools;  // per-tool spec() + invoke(&str) -> Result<ToolResult, ToolError>

tool_provider_scaffold::impl_to_manifest_sig!();
tool_provider_scaffold::tool_provider_plugin! {
    slug: "my-connector",
    provides: ["wild:tool-provider@0.4.0"],
    requires: ["wild:http/outbound@0.1.0"],
    signatures: [to_manifest_sig(&sig::ENUMERATE)],
    tools: [
        { name: "enumerate", spec: tools::enumerate::spec(),
          invoke: tools::enumerate::invoke,
          skill: ("enumerate" => ENUMERATE_SKILL) },
    ],
}
```

## Consuming the crate

In the development monorepo, the first-party tool-providers depend on
it by path (`tool-provider-scaffold = { path = "../../tool-provider-scaffold" }`).
The crate is not on crates.io; out of tree, **copy it** — the whole
crate, or just the macros you use out of `src/lib.rs` — into your
plugin. There is no runtime code to link: everything expands at your
call site.

One dependency to know about when copying: only the
`connector_fault` and `classified_error` forms of
`egress_fault_mapping!` name `wild_core::connector_fault` in their
expansion (the shared cause vocabulary and carried-fault format).
The scaffold crate itself carries no such dependency — a
`macro_rules!` expansion resolves at *your* call site, so the
dependency is yours to declare only if you expand one of those two
forms. `wild-core` is a monorepo crate that does not ship in this
tree, so out of tree either stick to the forms that expand std-only
(`describe_error`, `fault_class`, `custom_map`, plus the other three
macros, which need nothing) or carry your fault detail in your own
format.

## Worked material

The published examples write the same frame out *by hand* — read one
to see exactly what `tool_provider_plugin!` expands to, and what a
plugin writes itself once it outgrows the macro's fixed points:

- [`examples/tool-providers/sharepoint-connector`](../../examples/tool-providers/sharepoint-connector/)
  — a full component exercising four primitives (tool-provider,
  effect-handler, worker, function-backing) with a rich
  `sidecar.json`.
- [`examples/tool-providers/cash-forecast`](../../examples/tool-providers/cash-forecast/) ·
  [`fx-exposure`](../../examples/tool-providers/fx-exposure/) ·
  [`payment-delay`](../../examples/tool-providers/payment-delay/)
  — small dependency-light single-purpose providers.
- [`docs/plugin-developer-guide.md`](../../docs/plugin-developer-guide.md)
  — the how: building, sidecars, install, trust.
- [`docs/plugin-concept.md`](../../docs/plugin-concept.md)
  — the why: the plugin model and the tool-provider role in it.

## Tests

The crate is its own Cargo workspace and carries inline tests that
expand the dependency-free `egress_fault_mapping!` forms against mock
bindgen types and parse the `enumerate_result_schema!` output as
JSON — a missing comma between the canonical keys and a connector's
`extra:` would break every consuming plugin at once, so it fails here
first. `cargo test` in this directory runs them standalone, in the
development monorepo and in a verbatim copy alike. The two
`wild-core`-naming forms are covered where that dependency lives — in
the development repo's `folder-connector` test suite.

## License

Apache-2.0. In the public repository the root `LICENSE` states the
license map for the whole tree; this directory deliberately carries
no per-directory license file there.
