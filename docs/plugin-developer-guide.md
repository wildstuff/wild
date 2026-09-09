# Plugin Developer Guide

Practitioner-facing companion to `plugin-concept.md`. Covers how to
set up a dev environment, what tier model + WIT layout look like in
practice, and walks through building two reference plugins
(anthropic-cli — Tier-2 LLM adapter; http-fetcher — Tier-2 tool
provider).

`plugin-concept.md` is the **why**. This file is the **how**.

**Two repositories, one guide.** This file is published verbatim into
the public `wild` distribution repo (github.com/wildstuff/wild), which
carries everything an out-of-tree plugin author needs: the `wit/`
contracts (with `wit/external/*.wasm` vendored), the `examples/` trees,
and the `plugins/tool-provider-scaffold/` macro crate. Paths under
`crates/…` and `plugins/{llm,tools,channels}/…` are development-repo
sources you cannot open from a public clone — wherever one is the
reference, a published counterpart under `examples/` is named
alongside. Asides marked *in the development repository* apply only
there; an out-of-tree build never needs `xtask`.

## 1. Setup

### Toolchain

`rust-toolchain.toml` at the repo root pins **Rust 1.97.0** — the
wasmtime 44 host bindgen output uses Rust 2024 trait-call syntax,
older toolchains fail to build. `rustup` honours the file
automatically. Standalone plugin workspaces deliberately carry **no**
`rust-toolchain.toml` of their own: `rustup` walks up to the root
file, so every plugin builds on the same version with one source of
truth (in the development repository a stray sibling file fails the
pre-push gate). The root file also declares the `wasm32-wasip{1,2}`
targets the build needs.

Required cargo extensions:

```sh
cargo install cargo-component   # builds wasm32-wasip2 components
cargo install cargo-nextest     # 3× faster test runner used by CI
cargo install --locked wkg      # WebAssembly registry CLI — development
                                # repository only; the public repo vendors
                                # wit/external/*.wasm, no fetch needed
```

`cargo-component` is mandatory for every standalone plugin
workspace under `plugins/{llm,storage,tools,workloads,workers,chiefs}/*` —
those are NOT workspace members; they build via
`cargo component build --release`. The host workspace builds via
plain `cargo build`.

### External WIT — nothing to do on a public clone

External WIT packages (`wasi:http`, `wasi:config`, `wasi:clocks`)
are not committed as text-WIT. Builds consume them as binary
`.wasm` under `wit/external/`.

**Public `wild` repo:** the `.wasm` files are committed — a fresh
clone builds every example and standalone plugin with no bootstrap
step.

**In the development repository** `wit/external/` is gitignored
and keyed off `scripts/wit-external.lock`. Run once after a fresh
clone:

```sh
cargo run -p xtask -- wit-sync
```

The xtask reads the lockfile, fetches each pinned package via
`wkg get`, and writes `wit/external/<ns>-<name>-<ver>.wasm`. CI
caches the directory keyed on the lockfile hash. Without this
step every standalone plugin build there fails with `package
wasi:* not found`. To force-refresh after a registry republish:
`cargo run -p xtask -- wit-sync --refresh`.

### Sidecar layout

Tier-2 plugins are installed as a pair of files in the daemon-managed
plugin cache under the active profile (`family_dir(Family::Plugin)`):

```
<profile_root>/system/plugin-cache/
  <slug>.json       # PluginManifest sidecar (JSON, mode 0644)
  <slug>-<ver>.wasm # the component blob, named by manifest.wasm_filename
```

`wild plugin add oci://…` writes both files for you. During plugin
development you place them by hand — see §5 walkthrough.

### Tests

A standalone plugin workspace tests with plain `cargo test` (or
`cargo nextest run`) inside that workspace — see §9 for the
env-gated live-test pattern.

In the development repository, host-side changes use the xtask
lanes instead:

```sh
cargo run -p xtask -- test-affected             # affected-only (default)
cargo run -p xtask -- test-affected --since HEAD # uncommitted changes
cargo run -p xtask -- pre-push                   # full local CI gate
```

Never use `cargo test --workspace` there while iterating — cold
it runs ~27min on a single thread. The xtask mirrors CI's
per-crate matrix: 1–3 crates in <2min for a typical diff.
Mandatory before `git push`: `xtask pre-push` enforces fmt +
ADR-0004 lockdown + affected tests in CI's order.

## 2. Tier model

Three tiers describe how a plugin reaches the host runtime. Every
plugin lives in exactly one tier; the boot path picks them up
through tier-specific wiring functions.

| Tier | Polarity | Lives where | Wiring function |
|---|---|---|---|
| Tier-1 | In-process Rust impl of a `wild:*` plugin trait | `crates/runtime/wild-host/src/wild_*_plugin.rs` | `wire_h6_*` (sync, before `host.start()`) |
| Tier-1.5 | Wasm component embedded into the host via `include_bytes!` (ADR-0014; the default chief, `plugins/chiefs/default`). The ADR-0013-retired meaning was the older native-Rust LLM-adapter slot. | `dist/embedded/chief-default.wasm` | `wire_p1_component_plugins` (sync) |
| Tier-2 | Wasm component on disk, exporting a `wild:*` capability | `<profile_root>/system/plugin-cache/<slug>.{json,wasm}` | `wire_p1_component_plugins` (sync) or `wire_tier2_llm_adapters` (async post-start) |

**Tier-1 vs Tier-2 — when do you pick which:**

- Tier-1 is for capabilities the host itself owns and you want
  available to *every* component import (e.g. `wild:secrets`,
  `wild:cli-exec`, `wild:messaging`). They're compiled into the
  `wild` binary; operators can't add or remove them.
- Tier-2 is everything operators install — LLM adapters, tool
  providers, storage backends, workload components. They're
  individually trust-gated and capability-checked.

Per ADR-0012, **chief / llm / storage** plugins can't go through
the Forge build path — only `tool-provider` and `workload` can.
LLM adapters and storage adapters ship as pre-built OCI images
the operator pulls.

**Provider-flavor vs workload-flavor** (both are Tier-2):

- Provider-flavor exports a capability WIT interface (`wild:llm-adapter/...`,
  `wild:tool-provider/tools`, `wild:storage-adapter/...`, `wild:embed-adapter/...`).
  Lives in a host-managed instance pool. Stateless across calls.
- Workload-flavor exports `wild:messaging/handler` (or `wild:worker/handler`)
  and is scheduled per NATS message via the bundle pathway. Holds
  per-instance state across a session.

The loader derives the flavor from `provides[]` (`kind` is optional and
only a cache hint; see ADR-0141 PR0.5).

**Channels** (Telegram, whapi) are a `provider`-kind Tier-2 plugin too, but
they export `wild:operator-channel/channel` rather than one of the five
flavors — a channel is a *transport*, not a compute flavor (ADR-0062 §1). Build
one like any Tier-2 component (in the development repository,
`plugins/channels/telegram-channel/` is the reference, outbound `deliver` +
`capabilities`); see the *Channels* section in
[`plugin-concept.md`](plugin-concept.md) for where it sits in the taxonomy and
the proposed bidirectional direction (ADR-0159).

**Tool-provider scaffold.** Every `wild:tool-provider` plugin — the
development repository's connectors (`folder-connector`,
`web-connector`) and adapters (`csv-parser`, `json-parser`,
`pdf-parser`, `brave-search`, `math-tools`, `http-fetcher`) — shares
the SAME boilerplate: a
`Component` struct, the `ToolsGuest`/`MetaGuest` impls
(`list_tools` / `list_skill_mds` / `invoke`-by-name + a `kind:
Provider`, stateless `manifest`/`init`/`shutdown`), the `export!`, and
the identical `to_manifest_sig` projection. The
`plugins/tool-provider-scaffold` crate generates that surface from a
`macro_rules!` declaration, so each plugin spells out only what varies
— its slug, `requires` import set, typed `signatures`, and tool table.
Add the path dep, invoke `tool_provider_scaffold::tool_provider_plugin!`
(plus `impl_to_manifest_sig!` when the plugin authors typed signatures)
after `wit_bindgen::generate!`, and keep any plugin-specific helpers
below. The scaffold crate itself ships in the public repo at
`plugins/tool-provider-scaffold/`; for the minimal calling shape see
`plugins/tools/folder-connector/src/lib.rs` in the development
repository, or the published `examples/tool-providers/` components for
full openable sources. The macros are `macro_rules!` (not a proc-macro) on purpose: the
bindgen types live in the calling crate and `macro_rules!` resolves
paths at the call site.

## 3. WIT interfaces

### Where WIT lives

```
wit/                        # repo-local wild:* packages (text WIT)
├── ai/                     # wild:ai (host-exported chat surface)
├── cli-exec/               # wild:cli-exec (Tier-1 subprocess bridge)
├── llm-adapter/            # wild:llm-adapter (Tier-2 export)
├── tool-provider/          # wild:tool-provider (Tier-2 export)
├── plugin-meta/            # required base every Tier-2 plugin includes
├── secrets/, files/, …     # other host-exported capabilities
└── external/               # binary WIT packages from the WA registry
    ├── wasi-http-0.2.0.wasm
    ├── wasi-config-0.2.0-draft.wasm
    └── wasi-clocks-0.2.0.wasm
```

In the development repository `wit/external/` is gitignored —
`scripts/wit-external.lock` plus `xtask wit-sync` are the
reproduction record. The public `wild` repo commits the `.wasm`
files instead, so a fresh clone builds with no sync step (see §1).

Most packages are at `@0.1.0`. `wild:ai` is at `@0.2.0` after the
organism→tribe rename (the WIT field changed; that's a breaking
bump). New schema changes bump the respective package; renames
inside a content-stable schema do not.

### Standalone-plugin `wit/deps/` symlinks

Each standalone plugin's `wit/deps/` directory is a forest of
symlinks resolving to the canonical `wit/<package>/` and
`wit/external/*.wasm` files in the repo root. Example
(`plugins/tools/http-fetcher/wit/deps/`):

```
wasi-http-0.2.0.wasm     -> ../../../../../wit/external/wasi-http-0.2.0.wasm
wild-files-0.1.0         -> ../../../../../wit/files
wild-plugin-meta-0.1.0   -> ../../../../../wit/plugin-meta
wild-tool-provider-0.1.0 -> ../../../../../wit/tool-provider
```

Five `../` because the file is five directories deep
(`plugins/tools/http-fetcher/wit/deps/<symlink>`). Plugins under
`plugins/llm/<n>/wit/deps/` use the same depth; chiefs/workers
are also at five. The published
`examples/tool-providers/sharepoint-connector/wit/deps/` is the
same forest at the same depth — open it for a live copy you can
diff against.

The `[package.metadata.component.target.dependencies]` table in
each plugin's `Cargo.toml` references the same files via relative
paths — but `cargo-component` reads the `wit/deps/` symlinks at
build time, so both have to point at the same WIT.

### Worlds and includes

Every Tier-2 plugin's world `include`s `wild:plugin-meta/plugin-base@0.1.0`
to inherit the three lifecycle calls (`manifest`, `init`,
`shutdown`). On top of that you import the host capabilities you
need and export the capability you provide:

```wit
// plugins/tools/http-fetcher/wit/world.wit
package wildstuff:http-fetcher@0.1.0;

world http-fetcher {
    include wild:plugin-meta/plugin-base@0.1.0;
    import wasi:http/outgoing-handler@0.2.0;
    import wild:files/write@0.1.0;
    export wild:tool-provider/tools@0.4.0;
}
```

For the streaming LLM-adapter case, include the streaming variant
of the plugin world:

```wit
// plugins/llm/anthropic-cli/wit/world.wit
package wild:anthropic-cli@1.0.0;

world anthropic-cli {
    include wild:llm-adapter/llm-adapter-streaming-plugin@0.1.0;
    import wild:cli-exec/exec@0.1.0;
}
```

The streaming-plugin world already includes `plugin-meta` and the
chat exports — you just add the imports specific to your adapter.

### Record search from a component (`wild:data` retrieval, ADR-0076)

A workload component that reads the tribe's lake can search records along two
axes — import `wild:data/query@0.10.0`:

- **Lexical (offline, no embedding):** `find-by-fts(type, query, k, at)` — BM25
  full-text over the type's non-sensitive `text` fields. Pass the query text
  directly; you get `fts-hit{ record, score }` best-first. The right default for
  keyword/code/id lookups.
- **Semantic / hybrid:** `find-by-hybrid(type, field, text-query, query-vec, k,
  at)` fuses FTS + cosine kNN by Reciprocal Rank Fusion. It needs a **query
  vector** — so also import `wild:ai/embed@0.3.0`, embed the query text
  (`embed({input:[q], model: none, caller_context: {kind: "..."}, dimensions, …})`, see the
  [`examples/consumers/embed-consumer/`](../examples/consumers/embed-consumer/) walkthrough), and
  pass the vector as `query-vec`. The type's `field` must be a `vector` field
  whose records are populated — the operator/chief turns that on with
  `atlas_declare_embed_source` (the host reconciler then backfills it). With no
  embed adapter, fall back to `find-by-fts`.

Both are pure reads (no effect gate, as-of replayable); sensitive fields are
never indexed/embedded, so a search can never surface them. The host serves the
verbs — a forged tool-provider just imports them; the Forge grants
`wild:data/query` like any other capability.

## 4. Trust + capabilities

### Trust tiers

Every Tier-2 plugin sidecar declares a `trust.tier` value. The
host's `plugin_trust_gate` checks the manifest's `requires:` list
against the tier's allowance map at load time.

| Tier | Policy |
|---|---|
| `verified` | Full caps the manifest declares — gate is a no-op. |
| `community` | Blocklist: no `wild:secrets/store`/`admin`, no `wild:bundles/admin`, no `wild:plugin-storage/*`. Reads are fine. |
| `unknown` | Allowlist: `wild:blobs/read`, `wild:messaging/consumer` only. |

(`verified-local` is reserved and not shipped — declaring a
publisher at that tier is rejected with a typed error.)

Raw `wasi:http/outgoing-handler` is deliberately **not** on the
`unknown` allowlist — it is an ungoverned outbound socket, no
egress allowlist, no per-call audit. An unknown-tier plugin that
needs it gets a per-slug `capability_overrides` grant: minted
automatically when the operator confirms a marketplace connector
install (covering exactly the raw-egress cap the connector
declares, nothing more), or issued by hand via
`wild plugin grant`.

**How a plugin lands in a tier (ADR-0153):** trust is *derived*,
never self-declared, from a detached ed25519 signature over the
sidecar. A signature that verifies against the shipped wildstuff
key → `verified`; against a publisher key the operator has
allowlisted → `community`; no signature, or one that verifies
against neither → `unknown`. Nothing raises itself — the
signature is the only input that can lift a plugin above
`unknown` — and the operator may *lower* a publisher's ceiling,
never raise it: an allowlisted publisher tops out at `community`.

Capability overrides are additive across all tiers — every entry
in `<wild_root>/plugin-trust.json` `capability_overrides` for a slug
is pre-allowed regardless of tier:

```sh
wild plugin grant anthropic-cli wild:cli-exec/exec@0.1.0
wild plugin revoke anthropic-cli wild:cli-exec/exec@0.1.0
```

Block matching is exact-qname (`wild:secrets/store`) or
package-prefix with required `/` separator (`wild:plugin-storage`
blocks `wild:plugin-storage/*`).

### PluginManifest sidecar shape

> **ADR-0045 (binary-derived metadata):** `requires` is **no longer
> authored** — the host derives a Tier-2 plugin's imports from the
> component binary at wiring (the sidecar templates dropped the field).
> `provides` and `kind` are likewise binary-derived **at wiring**; an
> authored `provides` is at most an optional add-time hint (used for
> OCI namespacing + llm-adapter identification), always overridden by
> the binary's exports. Don't hand-maintain `requires`/`provides` to
> match your code — the binary IS the source of truth. The example
> below still shows them for shape illustration; `config_keys` /
> `secret_aliases` / `slug` / `version` / `trust` remain authored.

```json
{
  "slug": "anthropic-cli",
  "version": "1.1.0",
  "source": { "type": "local", "path": "anthropic-cli-1.1.0.wasm" },
  "wasm_filename": "anthropic-cli-1.1.0.wasm",
  "trust": { "tier": "community" },
  "provides": ["wild:llm-adapter/adapter@0.1.0"],
  "config_keys": ["model"],
  "secret_aliases": [],
  "auth_bindings": [
    {
      "name": "api-key",
      "description": "Provider API key",
      "hosts": ["api.example.com"],
      "inject": "header:X-Api-Key"
    }
  ],
  "capability_bundles": [],
  "default_pool_size": 4
}
```

Cross-checked at load time against the component's `meta::manifest()`
self-report — mismatch is a hard load error (defends against
swapping the wasm under a manifest's feet). `trust` is a struct,
not a string: `{ "tier": "community" }`, never `"trust": "community"`.
The richest live sidecar to crib from is
`examples/tool-providers/sharepoint-connector/sidecar.json`
(settings, auth_bindings, capability_bundles, effects — all in one
file).
The `kind` field is optional and derived from `provides[]`; the deprecated
`component_type`, `wit_baseline`, and `stateless` fields are ignored with a
warning (ADR-0141 PR0.5).

`auth_bindings` (ADR-0090) declares host-injected egress credentials the
plugin needs but does NOT import through `wild:secrets`. The guest names
the binding on its `wild:http/outbound` request; the host resolves the
secret from `system/egress.yaml` and injects it as configured. Use it for
API keys, bot tokens, and similar credentials that must never enter the
Wasm sandbox. Do not list `wild:secrets` aliases here — those belong in
`secret_aliases`.

Role-classification bundles in `capability_bundles[]` declare which
installable compute primitives the plugin provides (`worker.<role>`,
`effect.<family>`, `function.<family>`). A plugin may declare multiple
`worker.<role>` bundles, but each `<role>` suffix must be unique within
that plugin (ADR-0141 D12). Effect-handler plugins may include an optional
`effects` map, keyed by tool name, with default `risk` / `rule_key` /
`side_effect_class` values for their verbs; the operator can override these
in the bound `VerbSpec` (ADR-0141 D13).

### State persistence — pick the right capability

Most Tier-2 plugins today are stateless (`math-tools`, `pdf-parser`,
`http-fetcher`, `json-parser` — input → output, no memory).
When you DO need state, the decision tree:

| Plugin shape | Capability | Storage shape |
|---|---|---|
| **Stateless** (input → output, no memory) | none — no sidecar marker needed; `stateless` is deprecated/ignored (ADR-0141 PR0.5) | n/a |
| **State-as-files**, per-tribe (artifacts, cached payloads, intermediate results, structured records) | `wild:files/{read, write}` (+ `manage` only if you need delete) | FS-canonical per ADR-0026: bytes live at `<profile_root>/files/<tribe>/<logical-path>`; revisions on every write; caller_org stamped by the host |
| **Cross-cycle counters / sliding-window baselines / occurrence-count learning** (hypothetical today) | none today — see "Don't roll your own" below | n/a (ADR-0016 `wild:memory/store` is **Gated**; no concrete consumer has surfaced yet) |
| **Domain-specific shared state** (issue queue, decision audit, session log) | host-side capability for that domain (e.g. `wild:tasks/tracking`) — these are NOT generic plugin state, they're typed domain APIs you import | per-domain FS or KV store |

**`wild:files` is the default answer.** It's FS-canonical (operator
can inspect with `ls`, edit with any text editor, audit with `git`),
caller_org-stamped via wRPC link metadata (a plugin cannot impersonate
another), and has revisions for optimistic-concurrency patterns. Per-
tribe scope is automatic; the host derives the tribe-id from the
calling component's link table — never a payload field.

To declare it in your sidecar manifest:

```json
"requires": [
  "wild:files/read@0.1.0",
  "wild:files/write@0.1.0"
]
```

Three import-level capabilities split deliberately, granted at the
wRPC link layer:

| Interface | Grants | Use when |
|---|---|---|
| `wild:files/read` | read, list, init-download, read-chunk, abort-download | plugin consumes content (summariser, indexer) |
| `wild:files/write` | write, init-upload, append-chunk, complete-upload, abort-upload | plugin produces content (fetcher, report-emitter); cannot replace another caller's file (writes bump revision) |
| `wild:files/manage` | delete | plugin needs cleanup authority (sweepers, GC). Most plugins do NOT need this — leave it out |

Logical paths under your scope are free-form within constraints:
no leading `/`, no `.` / `..` segments, no embedded NUL. Pick a
sensible prefix (`my-plugin/v1/...`) so a future plugin schema bump
can co-exist with the old format.

#### Don't roll your own state

If your plugin needs state and the table above doesn't fit, **talk
to the host team before designing around it**. Three anti-patterns
that have bitten plugin authors elsewhere:

- **Don't subscribe a private NATS subject + maintain your own
  JetStream KV bucket.** That route is host-team territory; without
  coordination you get no boot-time provisioning, no quota, no GC
  story, no `wild watch` visibility.
- **Don't reuse `wild:tasks/tracking` for arbitrary plugin state.**
  It's the cycle-bookkeeping interface — wrong abstraction, wrong
  scope rules.
- **Don't push state into the chief's prompt** (stuffing retry
  counters / sliding windows / cached results into trigger-payload
  fields). The chief composes from typed sources; prompt-stuffing
  burns tokens forever AND breaks audit traces.

When `wild:files` doesn't fit (e.g. you need TTL-based expiry,
batch get, atomic compare-and-swap on small values), that's the
trigger to surface ADR-0016 (Gated) for promotion. File an issue
naming your use case + why the FS-shape doesn't work; we'd rather
promote the substrate than ship N orthogonal bucket designs.

## 5. Walkthrough — Tier-2 LLM adapter (anthropic-cli)

Reference: `plugins/llm/anthropic-cli/` (a development-repo tree —
first-party adapter sources are not published; the snippets below
carry everything the walkthrough needs). A published, openable
llm-adapter is
[`examples/llm-adapters/echo-llm/`](../examples/llm-adapters/echo-llm/README.md)
— the full contract (buffered chat + both stream surfaces) against a
deterministic backend, ready to have its echo swapped for real
provider calls. This plugin imports the
Tier-1 `wild:cli-exec/exec` bridge to drive the local `claude` CLI
in stream-json mode and exports `wild:llm-adapter/chat`.

### 5.1 Workspace skeleton

```toml
# plugins/llm/anthropic-cli/Cargo.toml
[workspace]                           # explicitly NOT a wild workspace member

[package]
name        = "anthropic-cli"
version     = "1.0.0"
edition     = "2021"
publish     = false

[lib]
crate-type = ["cdylib"]               # required for wasm32-wasip2

[dependencies]
wit-bindgen    = "0.35"
wit-bindgen-rt = "0.35"
serde          = { version = "1", features = ["derive"] }
serde_json     = "1"

[package.metadata.component]
package = "wild:anthropic-cli"

[package.metadata.component.target]
path  = "wit"
world = "anthropic-cli"

[package.metadata.component.target.dependencies]
"wild:plugin-meta" = { path = "../../../wit/plugin-meta/plugin-meta.wit" }
"wild:llm-adapter" = { path = "../../../wit/llm-adapter/llm-adapter.wit" }
"wild:cli-exec"    = { path = "../../../wit/cli-exec/cli-exec.wit" }
"wasi:http"        = { path = "../../../wit/external/wasi-http-0.2.0.wasm" }

[profile.release]
opt-level     = "z"
lto           = true
strip         = true
codegen-units = 1
```

### 5.2 Define the world

`wit/world.wit` — see §3. Re-uses `wild:llm-adapter`'s streaming
plugin world; adds `import wild:cli-exec/exec@0.1.0`.

### 5.3 Implement the exports

```rust
// src/lib.rs
#[allow(warnings)]
mod bindings;

use bindings::exports::wild::llm_adapter::chat::{Guest, ChatRequest, ChatResponse, ChatError};
use bindings::wild::cli_exec::exec;

struct Component;

impl Guest for Component {
    fn chat(req: ChatRequest) -> Result<ChatResponse, ChatError> {
        let exec_req = exec::ExecRequest {
            binary: "claude".into(),
            argv: vec![
                "--print".into(),
                "--input-format".into(),
                "stream-json".into(),
                /* … */
            ],
            stdin: Some(serde_json::to_vec(&frame_for(&req)).unwrap()),
            timeout_ms: None,
        };
        let handle = exec::exec_streaming(&exec_req)
            .map_err(|e| ChatError::Permanent(format!("spawn claude: {e:?}")))?;
        // … drive the handle, parse stream-json frames, build ChatResponse
    }
}

bindings::export!(Component with_types_in bindings);
```

The host calls `meta::manifest()` once at warm-up, `meta::init(config_bytes)`
once per pooled instance, and `chat(req)` per request.

### 5.4 Build

```sh
cd plugins/llm/anthropic-cli
cargo component build --release
# → target/wasm32-wasip2/release/anthropic_cli.wasm
```

### 5.5 Sidecar

Place under the active profile:

```sh
mkdir -p <profile_root>/system/plugin-cache
cp target/wasm32-wasip2/release/anthropic_cli.wasm \
   <profile_root>/system/plugin-cache/anthropic-cli-1.1.0.wasm
cat > <profile_root>/system/plugin-cache/anthropic-cli.json <<EOF
{
  "slug": "anthropic-cli",
  "version": "1.1.0",
  "source": { "type": "local", "path": "anthropic-cli-1.1.0.wasm" },
  "wasm_filename": "anthropic-cli-1.1.0.wasm",
  "trust": { "tier": "community" },
  "provides": ["wild:llm-adapter/adapter@0.1.0"],
  "capability_bundles": [],
  "default_pool_size": 4
}
EOF
```

### 5.6 Register in `llm-adapters.yaml`

```yaml
adapters:
  - id: claude-cli
    kind: component
    slug: anthropic-cli
    model: claude-sonnet-4-6
```

`kind: component` triggers the async Tier-2 wiring path
(`wire_tier2_llm_adapters` in `crates/runtime/daemon-lib/src/manager/host.rs`)
that runs after `host.start().await`. The slug links to the
sidecar; the model field becomes the runtime adapter id.

### 5.7 cli-binaries.yaml allowlist

Because the component imports `wild:cli-exec/exec`, the operator's
allowlist at `<profile_root>/cli-binaries.yaml` must
contain a matching entry. Default seed (shipped embedded) covers
`claude`:

```yaml
binaries:
  - name: claude
    path: /opt/homebrew/bin/claude        # absolute path, pinned
    allowed_argv_patterns:
      - ["--print", "--input-format", "stream-json", "*"]
    env_passthrough: []                    # claude self-manages credentials
    max_runtime_seconds: 300
    max_stdout_bytes: 16777216
    max_stderr_bytes: 1048576
    max_stream_idle_seconds: 60
```

### 5.8 Verify boot

```sh
./target/release/wild up --offline
```

Expected boot lines:

```
· wild:ai/chat adapter `claude-cli` (Component) deferred → async post-start
✓ wild:ai/chat plugin wired with empty registry — Tier-2 adapters will populate it after host.start()
✓ wild:ai/chat Tier-2 adapter `claude-cli` (Component) wired
```

## 6. Walkthrough — Tier-2 tool-provider (http-fetcher)

Reference: `plugins/tools/http-fetcher/` (development repository).
Published references you can open from a public clone:
`examples/tool-providers/sharepoint-connector/` — one component
exercising all four installable compute primitives, with the
richest `sidecar.json` in the tree — the ADR-0202 Procedure trio
`examples/tool-providers/{cash-forecast,fx-exposure,payment-delay}`
next to it, `examples/consumers/embed-consumer/` for a source-free
build against the published WIT,
`examples/llm-adapters/echo-llm/`, `examples/embed-adapters/hash-embed/`
and `examples/rerank-adapters/overlap-rerank/` for the three adapter
flavors, `examples/workers/annotate-worker/` for the workload flavor,
`examples/channels/journal-channel/` and `examples/widgets/sticky-note/`
for the channel and widget axes, and the shared
`plugins/tool-provider-scaffold/` macro crate.

Shorter walkthrough — the
shape is identical to §5 minus the `cli-exec` allowlist step.
Differences:

- World exports `wild:tool-provider/tools@0.4.0` instead of
  `wild:llm-adapter/chat`.
- Manifest `kind: "provider"`, `provides: ["wild:tool-provider@0.4.0"]`.
- Imports usually include `wasi:http/outgoing-handler@0.2.0` for
  the network fetch and (optionally) `wild:files/write` for
  fetch-to-disk variants.
- No `llm-adapters.yaml` registration — tool-providers are
  resolved through the `ToolProviderRegistry` at workload-start
  via the `requires:` list.

The implementation pattern (`impl Guest for Component`,
`bindings::export!`) is identical.

### Skill MDs (`list-skill-mds`)

`wild:tool-provider/tools@0.4.0` adds a `list-skill-mds()` export
parallel to `list-tools()`. Plugins ship one Skill MD per Tool
under `skills/<slug>.md`, bundle them via `include_str!`, and
return them from `list-skill-mds()`. The host parses each MD
once at boot and the merged Skill registry's component-source
dispatcher prefers the authored `SkillSpec` (with worked
examples + roles + version) over a description-only synthesis.
A plugin that ships zero MDs returns `vec![]` — the host falls
back to synthesis silently.

Frontmatter shape (parsed by `common::skill::spec::parse_skill_md`):

```yaml
name: http-fetch
version: 0.1.0
source: component
component_type: http-fetcher
method: http-fetch
description: …
args_schema: { type: object, properties: { url: { type: string } }, required: [url] }
```

Body uses `## Examples` → `### <name>` → `Input:` / `Output:`
lines; the parser lifts each example into `SkillSpec.examples`.
See `docs/skill-vs-tool.md` for the full pattern + comparison
against inlining examples in the Tool description.

**A FORGED tool-provider gets its method MDs seeded, not authored.**
The Wright's scaffold writes one `skills/<tool>.md` per `####
Signature:` block and `include_str!`s it from `list_skill_mds()`,
derived from the same spec that seeds `list_tools()`: the description,
the `INPUT_SCHEMA` as `args_schema`, the `OUTPUT_SCHEMA` as
`returns_schema`, and every `#### Example:` for that tool as a worked
case. So a forged tool reaches the Elder with the same structural
surface a hand-built one ships, instead of the description-only
synthesis `ComponentSourceDispatcher` falls back to. The MD is a
normal workspace file — the Wright and the operator may enrich its
prose, and a re-forge carries it.

Its tool-less `source: prose` **setup skill** is derived too, whenever
the spec declares a `#### Auth-Binding:` block — from what that block
states and nothing else: the alias to store the credential under, the
destinations to allow, the injection mode, the `content_hosts` that
must NOT receive the credential, and for an OAuth binding the grant
and scopes the guided config writer will ask for. It closes with what
it does *not* know: where to OBTAIN the credential is vendor knowledge
no build can read, so the skill says so instead of guessing a portal
URL. What stays un-derivable is the authored half a good MD also has —
the when-to-use narrative, the composition path, cost and limits.

### Setup skills — a plugin brings its own onboarding know-how

**Convention: any plugin an operator must configure before it works —
an API key, an egress/auth binding, an external account, a consent
step — SHOULD ship a tool-less `source: prose` setup skill next to its
method skills.** The Elder folds its summary into the ambient
judgement list (`scope: [operator-judgement]`) and walks the operator
through the steps in dialogue; without one, the Elder improvises from
model world-knowledge — unverified and blind to your plugin's specific
requirements. Domain knowledge stays OUT of the core skill corpus
(`assets/prompts/skills/` is connector-agnostic); the plugin is the
one place that knows its own setup.

Frontmatter (no `method` — dispatch on a prose skill is a defined
no-op, ADR-0074 D5; the registry admits tool-less prose MDs):

```yaml
name: <slug>-setup
version: 0.1.0
source: prose
scope: [operator-judgement]
description: >-
  when the operator wants <capability> working, or a call fails with
  an auth/config fault. One-sentence when-to-use — this line IS what
  the Elder sees ambiently, write it for that.
```

Wire it like any other MD (`include_str!` + `list_skill_mds()`; the
`tool_provider_plugin!` macro takes a `prose_skills: [("slug" =>
BODY)]` segment). Content rules: a decision procedure keyed on the
observable states (Vitals dimension / fault class → which step), the
honest traps, secret hygiene (values go through `wild secret add`,
never into chat or files), and plain language — the reader is the
Elder speaking to an accountant, not an engineer. Precedents (under
`plugins/tools/` in the development repository):
`folder-connector/skills/folder-setup.md` (no-config connector),
`brave-search/skills/brave-search-setup.md` (key + egress binding),
`sharepoint-connector/skills/sharepoint-setup.md` (OAuth app registration +
admin consent + per-site grant),
`telegram-send/skills/telegram-setup.md` (external account + token).
A published, openable skills tree is
`examples/tool-providers/cash-forecast/skills/`.

### Who invokes a provider tool (the common surprise)

Landing in the host's `ToolProviderRegistry` makes a tool
**dispatchable** — it does *not* put it in front of every agent's
LLM. Three distinct callers, and the chief is **not** the obvious
one:

- **Workers** (`ai-worker`, `extract-worker`) import
  `wild:tools/invoke` and pick tools from the aggregated catalog via
  their own LLM loop. This is the real *"an agent chooses and runs
  this tool"* path.
- **The Elder** reaches your tool through its **Skill** — ship a
  `list-skill-mds()` MD (`source: component`) and the host registers
  it into the `MergedSkillRegistry` the Elder runner reads (ADR-0040).
  A tool with no Skill MD is only a raw catalog entry, never a Skill.
- **The chief is deliberately NOT a generic tool-caller.** `chief.wit`
  imports `wild:tool-routing/invoke` *only* for deterministic ADR-0039
  schedule recipes — it does **not** import `wild:tools/invoke`, and
  its LLM tool set (`definitions_for_chat`) is the fixed chief
  verb-set, never the provider catalog. Asking the chief to "use
  tool X" in chat will NOT call your tool; it answers from the model.

To smoke-test a freshly-added provider — Rust or a jco/ComponentizeJS
JS component — **bypass the LLM entirely** with the operator surface:

```bash
wild tool ls                                   # confirm it's in the catalog
wild tool invoke md-render '{"markdown":"# Hi"}'   # run it, see the result
```

`wild tool invoke` dispatches through the **same** live registry a
worker's `wild:tools/invoke` hits, so a green result here proves the
component loads and executes correctly in the real host — independent
of which agent would later pick it.

### Built-in tool: `ai_chat` (LLM inference over the tool surface)

The host registers one native tool-provider, **`ai_chat`**, into that
same `ToolProviderRegistry`. It is a host-side Rust provider (not a
wasm component) holding the live `ChatBackendRegistry`, so any caller
that can dispatch a tool gets an LLM completion **without importing
`wild:ai/chat` itself** — a forged module that composes tools via
`wild:tool-routing/invoke` can reach it like any other registry tool.

Payload `{ prompt, system?, config?, use_mcp? }`:

- **`config`** — `"chat" | "logic" | "reasoning"`. Picks the adapter via
  the same `StrategyMap` the chief reads; omitted ⇒ a fixed `logic`
  default. You never name a concrete adapter id.
- **`use_mcp`** — when `true`, the chat may call the operator's
  registered MCP servers. MCP tools are surfaced the **active** way
  (ADR-0024 D1 — *not* claude-CLI `--mcp-config`): the merged
  `SkillRegistry` lists each server's tools, `ai_chat` runs an agentic
  loop, and dispatches tool-calls back through `McpSourceDispatcher`
  (`tools/call` over HTTP). Only `SkillSource::Mcp` skills are offered,
  so the sub-chat can't re-enter Wild's own catalog.

```bash
wild tool invoke ai_chat '{"prompt":"Summarise this in one line: ..."}'
wild tool invoke ai_chat '{"prompt":"What is on my calendar?","use_mcp":true}'
```

Note the difference from a direct `wild:ai/chat` import (which forged
tool-providers *also* hold): the raw import gives you a stateless
completion you drive yourself; `ai_chat` adds strategy-by-name routing
and host-mediated MCP the guest cannot wire on its own.

## 7. Boot flow + wiring

The host build is two-phased. **Sync phase** (before `host.start().await`)
wires every Tier-1 plugin and every Tier-2 component the loader
can reach without an async runtime:

```rust
// crates/runtime/daemon-lib/src/manager/host.rs (roughly)
let builder = wire_h6_plugins(builder, host)        // Tier-1 sync
    .pipe(wire_h6_secrets_plugin)
    .pipe(wire_h6_forge_plugin)
    .pipe(wire_h6_cli_exec_plugin)                  // ADR-0013 bridge
    .pipe(wire_h6_ai_plugin);                       // empty registry if all deferred

let builder = wire_p1_component_plugins(builder, sidecar_dir, engine);
let host = builder.build()?.start().await?;

// Async phase — runs only after host.start() is final.
if let Some(registry) = &ai_registry {
    wire_tier2_llm_adapters(&host, registry, …).await;
}
```

**Why two phases:** Tier-2 LLM adapters need every Tier-1 plugin's
LinkerSeed snapshot to be final before they can warm up
(anthropic-cli imports `wild:cli-exec/exec`; the bridge plugin has
to be wired first). The sync phase produces the snapshot;
`host.start()` freezes it; the async phase walks it per
instantiation.

`wire_h6_ai_plugin` registers `WildAiPlugin` against an empty
`ChatBackendRegistry` when all entries are `Deferred` (every entry
is `kind: component`). The async pass pushes the warmed Tier-2
backends into that already-registered registry. Without the
empty-registry path, components importing `wild:ai/chat` would
link against the missing-capability stub at workload_start.

## 8. Sidecar lifecycle

A **sidecar** is a long-running OS process the host spawns to back a
non-Wasm capability that has to live outside the sandbox — NATS, the
local `llama-server` instances for embed/rerank, and the optional
`wild-appd` portal are the current examples. This section describes the
contract between the host and a sidecar so plugin authors can decide when
to ship one and how to make it behave well.

### When to use a sidecar vs. an in-process provider

| Use an in-process provider (Tier-1 / Tier-2 component) when | Use a sidecar when |
|---|---|
| The capability can be expressed as a `wild:*` WIT interface. | The capability is an existing binary or server you cannot or should not port into Wasm (e.g. `nats-server`, `llama-server`). |
| Startup cost is one Wasm instantiation per call. | The process must stay alive across many host requests and warm up state (model load, JetStream metadata, mDNS registration). |
| The operator installs it as a `.wasm` sidecar. | The operator installs a native binary and the host manages its lifetime. |

Most plugins are **not** sidecars. Sidecars are the exception for
processes that need to outlive a single workload invocation and cannot
be sanely sandboxed as a component.

### How the host discovers and starts sidecars

The host drives every sidecar through the shared `ManagedProcess`
abstraction in `crates/runtime/shared/src/sidecar.rs` (ADR-0166). The
host does not exec the binary directly in ad-hoc code; it constructs a
`ManagedProcess` value that knows:

- the sidecar's human-readable `name` (used in logs);
- the `pid_path` where the child PID is written (`<profile_root>/system/<sidecar>.pid`);
- the `log_path` where stdout/stderr are redirected (`<profile_root>/logs/<sidecar>.log`).

`ManagedProcess::start` spawns the process, writes the PID file, drops
the `Child` handle so the sidecar survives the spawning process, and
optionally waits for a readiness probe before returning.

As a plugin author you do not implement `ManagedProcess` yourself. If
your plugin needs a sidecar, document the binary name, the expected
arguments, and the readiness endpoint in your plugin's setup skill or
README; the host team wires the corresponding `ManagedProcess` call into
the daemon. The host is the single owner of sidecar lifecycle so
`wild down`, restarts, and upgrades have one consistent behavior.

### PID-file conventions

The host writes the child PID to a well-known file under the active
profile:

```text
<profile_root>/system/
  nats-server.pid
  llama-embed.pid
  llama-rerank.pid
  wild-appd.pid
```

Why PID files instead of keeping the `Child` handle:

- **Detached semantics.** The spawning process may exit (`wild up`
  returns, the dashboard launcher closes, a packaged `.app` autostart
  finishes). The PID file lets a later process reap the sidecar.
- **Idempotency.** A second `wild up` can look at the PID file, probe
  the process, and skip spawning if it is already healthy.
- **Operator visibility.** An accountant persona can see which
  sidecars the profile owns by listing `system/*.pid`, and `wild doctor`
  uses the same files for its process inventory.

Do not write your own PID file from inside a sidecar. The host owns the
file and removes it when the sidecar stops.

### Stop / restart semantics

When the host stops a sidecar it owns, the sequence is:

1. Read the PID from the PID file.
2. Send `SIGTERM`.
3. Poll for graceful exit for a sidecar-specific grace period.
4. Escalate to `SIGKILL` if the grace period expires.
5. Remove the PID file.

A sidecar should handle `SIGTERM` and exit cleanly as quickly as it can
flush in-flight work. If it ignores `SIGTERM`, the host will eventually
kill it; the operator sees a `*-killed` lifecycle event instead of a
`*-stopped` event.

The host exposes a `StopOutcome` (`Stopped`, `AlreadyDead`, `Killed`,
`NotOwned`, `PidFileBroken`) so callers like `wild down` can log an
honest one-liner. A missing PID file means "we did not spawn this
process" — for example, an operator-managed `nats-server` started via
`brew services` is left alone.

### Readiness endpoint contract

Every sidecar that the host starts detached must expose a readiness
endpoint the host can probe. The probe lives in `ManagedProcess` as a
`ReadyProbe`:

- `HttpGet(url)` — the sidecar listens on loopback and answers `2xx` on
  a health path (e.g. `llama-server` answers `GET /health`).
- `NatsVarz(url)` — NATS-specific; the monitoring port answers `GET
  /varz`.
- `Custom(...)` — reserved for sidecars whose readiness cannot be
  expressed as a simple HTTP check.

Requirements for the endpoint:

- Bind to **loopback only** (`127.0.0.1` or `::1`). Sidecars are
  per-profile helpers, not public services.
- Return `2xx` only when the sidecar can actually accept work. For
  `llama-server` that means the model is loaded; for `wild-appd` that
  means the HTTP server and upstream UDS are ready.
- Be fast and cheap. The host polls every 250 ms for the configured
  startup timeout; expensive probes delay every boot.
- Log startup failures to the sidecar's `log_path`. The host logs the
  timeout at WARN level and points the operator at the log file.

Example: a sidecar that exposes `GET /health` should return a minimal
response once initialization is complete:

```text
HTTP/1.1 200 OK\r\n
Content-Length: 2\r\n
\r\n
ok
```

The host does not parse the body; a success status is enough.

### Declaring a file format (ADR-0214)

A parser plugin can teach the platform a file type it does not ship. Add a
`format` block to the sidecar; the host reads it at boot and every extraction
path resolves against it — no recompile, no central list to edit.

```json
{
  "slug": "pptx-parser",
  "format": {
    "token": "pptx",
    "mimes": ["application/vnd.openxmlformats-officedocument.presentationml.presentation"],
    "door": "document",
    "parser_verb": "pptx-parse"
  }
}
```

**The key is `token`, not `format`.** The JSON field is renamed; writing
`"format": "pptx"` inside the block makes the whole block fail to parse, and a
malformed block degrades to *no declaration* rather than failing the install
(deliberately — a bad format block must never stop a plugin's tools from
loading). The plugin installs, its tools work, and the file type silently never
arrives. If a declaration seems ignored, check this first.

`door` is `document` (parsed into text via `parser_verb`) or `structured`
(read as rows via `sample_verb` + `decode_verb`).

Three rules the host enforces, all of which reject the DECLARATION only — the
plugin still installs and its tools still work:

- **Additive only.** A token or mime the platform already handles is refused; an
  installed plugin can add `pptx`, never redefine `pdf`.
- **Every declared verb must be one this plugin exports.** Declaring
  `pptx-parse` without shipping it would map a file type to something nothing
  can dispatch. Bundled plugins get this as a build break
  (`xtask format-facet-parity`); installed ones are refused at boot.
- **No page renderers.** A renderer writes image blobs, which is a host tool
  surface rather than a mime→verb mapping.

A refusal is not silent: the operator gets a plain-language notice saying which
plugin, which format, and why. **The declaration takes effect at the next host
start** — it is read when the registries are built, not at install time.

## 9. Debugging

### Boot-log glyphs

| Glyph | Meaning |
|---|---|
| `✓` | Wired successfully — capability is live. |
| `·` | Skipped or deferred — informational, not an error. Reason follows. |

Common skip reasons:

- `kind: component` always shows `· deferred → async post-start`
  in the sync-phase log; the `✓ Tier-2 adapter <id> wired` line in
  the async phase confirms it landed.
- `· SKIPPED — sidecar at <path> missing` → the slug in
  `llm-adapters.yaml` doesn't match a `<slug>.json` under
  `<profile_root>/system/plugin-cache/`.
- `WARN trust gate denied capability` → the plugin's `requires:`
  list contains a cap not allowed for its tier. Grant the cap
  explicitly: `wild plugin grant <slug> <cap@ver>`. Tiers are
  signature-derived and nothing raises itself (§4) — the per-cap
  grant is the operator lever.

### Diagnostics commands

```sh
wild doctor                  # health snapshot — every plugin's status
wild plugin list             # installed plugins + tiers
wild config llm list         # registered LLM adapters
wild config llm test <id>    # ping an adapter through the live registry
wild secret grants           # plugin-cap overrides
```

### Self-test pattern

The smoke loop for plugin development:

```sh
# 1. Rebuild your plugin
(cd plugins/llm/anthropic-cli && cargo component build --release)

# 2. Refresh sidecar
cp .../anthropic_cli.wasm \
   <profile_root>/system/plugin-cache/anthropic-cli-1.1.0.wasm

# 3. Boot host with logs to disk
./target/release/wild up --offline > /tmp/wild-up.log 2>&1 &
sleep 15
kill %1
tail -80 /tmp/wild-up.log
```

The log carries every wire-up line and the trust-gate verdict for
every plugin — usually enough to pinpoint a manifest mismatch or
missing capability without attaching a debugger.

### Live LLM-adapter test (real network)

```sh
WILD_LIVE_CLAUDE=1 cargo test -p anthropic-cli --release
```

Standalone-plugin live tests are gated on env flags (`WILD_LIVE_*`)
so CI doesn't hit external endpoints. The test module is
`#[cfg(all(test, not(target_arch = "wasm32")))]` so it builds
host-arch and spawns the real CLI, verifying the wire format the
component's parsers handle.

## See also

- `docs/plugin-concept.md` — design rationale, tier-flavor split
- `docs/plugin-trust.md` — full trust-tier policy + override flow
- `docs/llm-adapters.md` — LLM-adapter wire format + registry semantics
- `docs/secrets.md` — `wild:secrets` chain (`keychain → env`)
- `docs/adr/0012-component-type-registry-as-yaml.md` — registry layout + Forge lockdown
- `docs/adr/0013-cli-exec-bridge-and-tier2-llm-policy.md` — Tier-1 subprocess bridge + Tier-2 LLM-adapter policy
- `docs/adr/0016-tool-memory-capability.md` — `wild:memory/store` (Gated; promote when `wild:files` shape doesn't fit a concrete consumer)
- `docs/adr/0026-fs-canonical-persistence.md` — FS-canonical persistence model that `wild:files` is built on
- `wit/files/files.wit` — `wild:files@0.1.0` interfaces (`read` / `write` / `manage`)
- `wit/plugin-meta/plugin-meta.wit` — required base every Tier-2 plugin includes
- `crates/runtime/daemon-lib/src/manager/host.rs` — sync + async wiring functions
