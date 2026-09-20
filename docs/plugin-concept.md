# Wild plugin concept — the model

The canonical reference for **what a plugin is** in Wildstuff: the
three delivery tiers, the five flavors, the trust model, the sidecar
manifest, the on-disk layout, and the boot flow. It answers *why the
model is shaped this way* and *what each piece is*.

The **how** — writing a plugin, the WIT worlds, the build script, the
grant walkthroughs — lives in
[`plugin-developer-guide.md`](plugin-developer-guide.md). This doc does
not repeat those walkthroughs.

The bet: **one delivery model, one trust model, one CLI verb
(`wild plugin`)** across everything the host doesn't hard-wire, even
though the wiring under the hood differs between an LLM adapter, a tool
pack, and a background worker. A plugin is always "a Wasm component you
pull, install at some trust level, and configure per profile" — so the
loader, the trust gate, and the operator surface are written once.

---

## The three tiers — a delivery axis, not a role axis

A plugin's **tier is where its bytes come from**, decided by what it
needs from the host environment — not by who authored it or what job it
does (ADR-0013, ADR-0014).

| Tier | What it is | Distribution | Trust | Examples |
|---|---|---|---|---|
| **Tier-1** | Host-native Rust `HostPlugin`, statically linked | Compiled into the `wild-hostd` binary | Implicit — ships with the release | `wild:messaging`, `wild:secrets`, `wild:files`, `wild:blobs`, `wild:tasks`, `wild:cli-exec`, `wild:forge`, `wild:registry`, `wild:system` |
| **Tier-1.5** | Wasm component **embedded in the binary** via `include_bytes!` | Bundled at build time by `xtask bundle-chief`; no OCI artefact | Implicit — bytes ship with the release | the default chief (`plugins/chiefs/default/`) — the only Tier-1.5 component today |
| **Tier-2** | Wasm component **OCI-pulled at runtime** | `wild plugin add oci://…` (or the bootstrap inventory's first-run pull) | Verified / Community / Unknown, sandbox-gated | `openrouter`, `ai-worker`, `math-tools`, `http-fetcher`, `ollama` |

The recognition rule, applied in order — first match wins:

1. **Implements a `wild:*` capability the host owns natively** (the
   messaging bus, the SQL store, the secrets resolver) → a **Tier-1**
   backend. The operator selects it via config, not a catalog. The
   backend follows the same discipline as a plugin (capability WIT +
   trait + trust) but is not `wild plugin add`-able.
2. **Needs OS-API access Wasm can't grant** — subprocess spawn, OS
   keychain, native-socket discovery → a **Tier-1** native
   `HostPlugin` (e.g. `wild_cli_exec_plugin`). Tier-2 plugins that need
   such access *import* the matching Tier-1 capability; they never get
   raw OS access.
3. **Removing it from the binary breaks the bootstrap promise** (the
   system can't start without it) → **Tier-1.5**. Today the sole entry
   is the default chief: without a chief, no tribe runs, and a
   cold/air-gapped first boot must not depend on OCI.
4. **Fits an existing capability WIT and the system runs fine if the
   operator picked a different one** → **Tier-2**. OCI-distributed,
   signed, runtime-installable, sandboxed.

LLM adapters are Tier-2 because they fail open — if one is missing, the
operator picks another. `wild:cli-exec` is Tier-1 (native Rust), not
Tier-1.5: bridge-class plugins are ordinary native host plugins,
indistinguishable in shape from `wild_secrets_plugin` (ADR-0014
widened Tier-1.5 to "any in-binary-bundled Wasm component" — the axis
is *delivery*, not *role*).

> 🔍 **Dig deeper — the tier model.**
>
> - **Concept:** ADR-0013 (cli-exec bridge + Tier-2 LLM policy) · ADR-0014 (OCI namespace + unified chief).
> - **In code:** Tier-1 plugins are `wild_*_plugin.rs` under `crates/runtime/wild-host/src/plugins/`; the Tier-1.5 chief bundle is produced by `cargo run -p xtask -- bundle-chief`.

---

## The five flavors — the export-shape axis

A Tier-2 plugin's **flavor is the WIT contract it exports** — the shape
of how the host talks to it. The flavors split into two families
(ADR-0062 pinned the original set of five — two compute + three
adapter — with interaction transports as a layer, not a flavor;
ADR-0147 later added `rerank-adapter` as a fourth adapter flavor, a
strict mirror of the embed plane):

**Compute** — the plugin *does work*:

| Flavor | Exports | Shape | Real examples (`plugins/`) |
|---|---|---|---|
| **tool-provider** | `wild:tool-provider@0.4.0` | synchronous request → one response; agent-callable as a JSON tool | `math-tools`, `http-fetcher`, `pdf-parser`, `brave-search`, `csv-parser`, `json-parser` |
| **workload** | `wild:messaging/handler@0.3.0` | wakes on a NATS subscription; publishes outcome events over time | `ai-worker`, `enrich-worker`, `extract-worker`, `intake-runner`, `search-indexer`, `dunning-notifier` |

**Adapter** — the plugin *backs a host capability* (a provider-kind
plugin the host wraps in a reusable instance pool):

| Flavor | Exports | Backs | Real examples |
|---|---|---|---|
| **llm-adapter** | `wild:llm-adapter@0.3.0` | `wild:ai/chat` | `openrouter`, `anthropic-cli`, `openai` |
| **storage-adapter** | `wild:storage-adapter@0.1.0` | `wild:files` blob backend | *(seam defined; no plugin bundled today)* |
| **embed-adapter** | `wild:embed-adapter@0.1.0` | `wild:ai/embed` | `ollama`, `llama` |
| **rerank-adapter** | `wild:rerank-adapter@0.1.0` | `wild:rerank` (the retrieve → rerank second stage) | `llama-rerank` |

**Chief** is a distinct kind, not one of the five flavors. A chief is
the per-tribe orchestrator (cycle · dispatch · decisions), exactly one
per tribe. The default chief ships Tier-1.5; specialised flavors
(`chief-research`, `chief-rule`, …) install as Tier-2 via
`wild plugin add --kind chief`. It exports `wild:chief@0.1.0`.

### Discriminator — which compute flavor?

The line between the two compute flavors is what the caller sees:

> **Returns a single value to a synchronous caller?** → tool-provider.
> **Publishes outcome events while working over time?** → workload.

This matters for latency (a sync tool-call beats a NATS round-trip),
agent UX (a skill MD says "use the `http-fetch` tool" instead of
"publish to subject X"), trust gating (tool-providers aggregate under
one cap surface that can be tier-restricted cleanly), and pooling
(adapters and tool-providers reuse a warm instance pool; pure workloads
tend to instantiate per message). `http-fetcher` and `pdf-parser` are
tool-providers for exactly this reason — a fetch is a request/response,
not a long-running job.

The on-disk `kind:` field is coarser than flavor — it carries only
`chief` / `provider` / `workload`. The four adapter+tool-provider
flavors are all `provider`-kind; the host distinguishes them by the
capability WIT the component's binary type-section declares it exports.

> 🔍 **Dig deeper — flavors & the WIT contracts.**
>
> - **Concept:** [`plugin-developer-guide.md`](plugin-developer-guide.md) (author's view) · `llm-adapters.md` · `embed-adapters.md` · [`skill-vs-tool.md`](skill-vs-tool.md).
> - **Contracts:** `wit/{tool-provider,llm-adapter,storage-adapter,embed-adapter,chief,plugin-meta}/`.
> - **Live plugins:** `plugins/{tools,workers,llm,storage,embed,chiefs}/`.

### Channels — the transport axis, *not* a sixth flavor

A **Channel** carries *conversation* with an external party — an operator's
notifications, and (forthcoming) a customer's chat. It is the fourth noun a
plugin developer reaches for alongside **Connector** (a tool-provider that
brings external *data* in), **Worker** (a workload), and **Effect** (an
effect-handler verb) — but it sits on a **different axis**. The five flavors
above are the *compute* axis: units of work *inside* the tribe. A Channel is a
**transport**: how an external client *reaches* those units — the same axis as
MCP and REST. ADR-0062 §1 pins this deliberately: *interaction is a layer, not
a sixth flavor.* So a Channel is a peer to MCP/REST, **not** a peer to
tool-provider/workload.

Mechanically, today a channel is a **`provider`-kind Tier-2 component** that
exports `wild:operator-channel/channel@0.1.0` (`deliver` + `capabilities`) —
guest-buildable exactly like any other Tier-2 plugin (`plugins/channels/telegram-channel/`
is the live example, outbound-only). The host collects channel impls and fans
each notification out to them (`OperatorChannel` trait, host-as-caller). A
developer ships **"a Channel plugin"** as one mental unit even though the two
directions differ under the hood: outbound is a fan-out trait; **inbound** (a
webhook the host is *called on*) is host-native and modeled as a per-provider
verifier/parser registry, not a symmetric trait.

**Widgets** sit on yet another axis — the *renderer*. A widget plugin
exports `wild:ui/widget@0.1.0` and adds a new card kind to the
`wild-view` app renderer (ADR-0173); it installs like any Tier-2
component but does its work inside the dashboard surface, not inside
the tribe. The reference plugin is `plugins/widgets/hello-widget/`.

> **Direction of travel (proposed, not yet accepted).** ADR-0159 generalizes
> the outbound-only `wild:operator-channel` to a **bidirectional `wild:channel`**
> (a content-stable rename) and adds the host-native inbound seam; a later,
> deferred guest `parse-inbound` export would let a third party ship a fully
> bidirectional Channel with no host code. Until those land, a Channel is
> outbound-only and operator-scoped. See ADR-0044 (operator channels),
> ADR-0062 §1/§6 (transport-not-flavor), ADR-0158/0159 (the customer +
> inbound work).

---

## The plugin-meta contract

Every Tier-2 plugin exports `wild:plugin-meta@0.3.0` — the minimum
surface the loader can call *without knowing the flavor*. Three
functions:

- `manifest()` → the plugin's self-reported identity (slug, version,
  kind, provides, requires, config-keys, secret-aliases).
- `init(config: list<u8>)` → applied once per instance before any
  dispatch; returns a typed `init-error` (`missing-secret`,
  `missing-config-key`, `backend-init`) that the host turns into a
  degrade-and-continue skip rather than a crash.
- `shutdown()` → best-effort graceful teardown on `wild down` or
  unload.

A provider/tool-provider/workload world *includes* the `plugin-base`
world and adds its own capability export plus the utility WITs it
imports (e.g. `wild:secrets@0.1.0`, `wasi:http/outgoing-handler@0.2.0`,
`wild:ai@0.4.0`, `wild:files@0.4.0`). The imports a world declares are
what land in the manifest's `requires`.

---

## The sidecar manifest

Two artefacts describe an installed plugin, split by mutability:

- The **sidecar** (`<slug>.json`) — the immutable install-time
  identity, written once at `wild plugin add` time.
- The **per-profile config** — what's enabled, its config values,
  grants, bindings. Switching profiles changes the active set without
  re-installing bytes.

The split exists because the sidecar is signed content — a detached
ed25519 signature covers its bytes, and its `source` pins the
component digest (ADR-0153): if config lived in the sidecar, every
config change would invalidate the signature. Keeping them apart lets
one installed artefact stay byte-identical across a `dev` / `staging` /
`prod` profile set.

### Sidecar fields

`PluginManifest` (`crates/runtime/wild-plugins/src/manifest.rs`):

| Field | Meaning |
|---|---|
| `slug`, `version` | identity |
| `kind` | **optional / derived** from `provides[]` (`chief`/`provider`/`workload`) |
| `source` | `{ type: oci, ref, digest }` — where the bytes came from |
| `wasm_filename` | file relative to the plugin cache — portable across machines |
| `trust` | `{ tier, publisher, signature, verified_at }` |
| `provides` | capability WIT interfaces this plugin exports |
| `requires` | capability + WASI imports it needs at runtime |
| `capability_bundles` | grant-less role markers (`worker.`, `effect.`, `function.`, `source.`); multiple `worker.<role>` entries are allowed but each `<role>` suffix must be unique within the plugin |
| `effects` | optional default `risk` / `rule_key` / `side_effect_class` for effect-handler verbs, keyed by tool name; operator may override in the bound `VerbSpec` |
| `config_keys`, `secret_aliases` | what the profile config must supply; function backings inherit the plugin's `secret_aliases[]` |
| `auth_bindings` | ADR-0090 egress auth bindings the host must inject from `system/egress.yaml`; not guest-imported, so not binary-derived |
| `default_pool_size` | provider-flavor instance-pool hint — the size the pool settles at **under concurrency**, not a boot reservation. Start warms exactly one member; `acquire` instantiates on demand when the pool is empty and `release` returns every member, so the pool grows to the concurrency it actually sees. Idle eviction trims *named tribe* pools only — the system/default pool (where every `tribe_id: None` caller, including all LLM adapters, lands) lives for the host lifetime. So whatever start warms is held until shutdown, which is why warming the declared size charged every provider its full pool even when it never took a call (2026-08-04). |
| `wake_up` | workload-flavor: the subscription template |
| `previous_digest` | rollback anchor after a self-dev update |
| `added_at`, `added_by` | provenance |

> **Deprecated (ADR-0141 PR0.5).** `component_type`, `wit_baseline`, and
> `stateless` are no longer sidecar fields. The host derives the role from
> the binary and from `provides[]`/`capability_bundles[]`. Old sidecars that
> still carry them load successfully, but a deprecation warning is emitted.

> **Field sourcing (ADR-0045 / ADR-0090).** `kind`, `provides`, `requires`,
> `config_keys`, and `secret_aliases` are **binary-derived** — read
> from the component's type-section at wiring, not trusted from the
> authored sidecar. Any authored values are add/config-time hints the
> binary overrides. `auth_bindings` is the exception: it describes
> operator-configured host egress policy and is **sidecar-authored**.
> The load-time cross-check between the guest's `manifest()` self-report
> and the sidecar is split by severity: a `slug` or `version` mismatch is
> a **hard load error** (it defends against swapping the wasm under a tag);
> a divergence in the capability fields is only a **warning**, since the host uses the
> binary's type-section truth regardless.

The per-profile config supplies the concrete values — `model`,
`endpoint`, `bucket`, secret-alias → keychain-name bindings, pool size,
wake-up template placeholders. LLM adapters are the exception: one
plugin slug feeds many adapter ids, so their per-instance config lives
in `llm-adapters.yaml`, not the shared `plugin-config` surface.

---

## On-disk layout

Plugin state is **per-profile**, resolved through
`wild_shared::wild_home` — never hand-built paths. The `<profile_root>`
is `<profile_root>/`; operator data sits at the root, runtime
and cached state under `system/`.

| What | Path | Resolver |
|---|---|---|
| Sidecar (`.json`) + wasm bytes | `<profile_root>/system/plugin-cache/<slug>.{json,wasm}` | `plugin_cache_dir()` |
| Per-plugin component-type catalog (YAML) | `<profile_root>/system/component-types/<slug>.yaml` | `plugin_configs_dir()` |
| Pulled OCI artefacts | `<profile_root>/system/oci-cache/` | `oci_cache_dir()` |
| Secret-grant ACL | `<profile_root>/system/secret-grants.json` | `secret_grants_path()` |
| Which plugins are enabled | `<profile_root>/system/bootstrap.lock` | (bootstrap manifest) |
| LLM-adapter per-instance config | `<profile_root>/llm-adapters.yaml` | — |
| Publisher keys + tier overrides | `<wild_root>/plugin-trust.json` (**system-wide**) | `PluginTrustStore::default_path()` |

Two things are deliberately **not** per-profile:

- **`plugin-trust.json` is system-wide** (at the user root, not under a
  profile). Trusting a publisher's key is an operator-level decision,
  not a per-profile one.
- Nothing lives in a top-level `<profile_root>/system/plugins/` folder. Per
  ADR-0026, per-profile state is FS-canonical under
  `<profile_root>/system/`; there is no SQLite `component_types` cache
  and no user-root plugin directory (older design drafts assumed one).

> 🔍 **Dig deeper — where files live.**
>
> - **On disk:** `profile-layout.md` — the exhaustive per-profile map.
> - **In code:** `crates/runtime/wild-home/src/lib.rs` — every path is a named function; call it, don't concatenate.

---

## Trust model

Trust tiers gate which capabilities a Tier-2 plugin may hold. The
enforced `Tier` enum
(`crates/runtime/providers/src/storage/component_types.rs`) has **three
variants**:

| Tier | Signal | Capability allowance |
|---|---|---|
| **Verified** | Built-in bootstrap rows, or an ed25519 signature that verifies against the key shipped with Wild | The full set its `requires` declares |
| **Community** | Signed by a known, operator-allowlisted publisher | A subset — the write/dispatch surfaces are blocked (secrets-write, bundle admin, plugin storage, data/schema writes, effect submission, arbitrary tool dispatch); reads stay open |
| **Unknown** | Locally added (`wild plugin add ./file.wasm`) or unsigned | The minimum-viable subset; the operator widens it per-plugin via `wild plugin grant <slug> --cap <cap>` |

The tier is **derived, never declared** (ADR-0153): the token a
publisher writes into a sidecar is a claim, and what counts is the
detached ed25519 signature over the sidecar bytes — *which key*
verifies it decides the tier. Nothing raises itself, and the
operator may lower a publisher but never raise one to Verified. A
plugin with no signature (or no explicit tier) lands at **Unknown**,
so the plan renderer fails safe.

**`verified-local` is reserved, not yet shipped.** The parser maps a
`verified-local` string to `Verified` for capability-allowance purposes
(so the design can be exercised), but declaring a *publisher* at
`verified-local` in `plugin-trust.json` is rejected at parse and write
time (`TrustFileError::VerifiedLocalNotYetSupported`). The intended
future meaning is an org-local signing key for Forge-built plugins,
scoped so org A's local trust is org B's Unknown.

Capability composition when a plugin declares `requires`:

1. The loader checks each required cap against the plugin's tier
   allowance; a blocklisted cap from a lower tier is rejected at load
   with a hint.
2. Each granted import is wired into the wasmtime linker via the
   matching Tier-1 plugin's `add_to_linker`.
3. For `wild:secrets/get` specifically, the operator must first run the
   secret-grant flow; without a grant the plugin loads but its first
   read returns a not-granted error. This is the same ACL as
   [`secrets.md`](secrets.md), generalised across capabilities.

> 🔍 **Dig deeper — trust in practice.**
>
> - **Concept:** [`plugin-trust.md`](plugin-trust.md) (operator schema + CLI) · ADR-0153 (the derived tier + signing) · ADR-0045 (out-of-tree metadata + trust) · `bootstrap-and-default-inventory.md` (the default-shipped inventory + tiers).
> - **In code:** `crates/wild-core/src/provenance.rs` (signature → tier derivation) · `crates/runtime/wild-plugins/src/trust.rs` (`PluginTrustStore`) · the load-time gate in `crates/runtime/wild-host/src/`.

---

## Boot flow — how a Tier-2 plugin comes alive

`wild plugin add oci://…` pulls the image, cross-checks the sidecar,
and writes `<slug>.{json,wasm}` into the active profile's
`plugin-cache/`. The plugin comes alive on the next boot:

1. The wiring layer resolves `plugin_cache_dir()` and calls
   `PluginManifest::load_all_in_dir(dir)` to read every sidecar.
2. For each entry it instantiates the component and — through
   `ComponentBackedHostPlugin` — calls the guest's `meta::manifest()`,
   cross-checks it against the sidecar (hard-fail on slug/version skew,
   warn on capability skew), then `meta::init(config_bytes)` with the
   profile's config.
3. The shim registers as a `dyn HostPlugin`, advertising the
   `provided_interfaces()` its manifest declares. The backend registry
   now sees the new capability — a fresh `ChatBackend`, a
   `storage-adapter`, a tool catalog entry — and dispatch resolves to
   it (e.g. a `wild:ai/chat` call routes by requested model to the
   registered adapter).
4. Workload-flavor plugins instead flow through the bundle pathway:
   the component-type catalog entry drives per-subscription
   instantiation, with the trust gate filtering imports by tier.

A running daemon applies plugin lifecycle changes without a restart:
`wild plugin {add,remove,enable,disable,config}` publish
`wild.system.plugin.{installed,removed,enabled,disabled,reconfigured}`,
and the host hot-loads, drains, or re-inits the affected pool live.

> 🔍 **Dig deeper — the loader.**
>
> - **In code:** `crates/runtime/wild-host/src/plugins/component_backed/` (the generic shim) · `crates/runtime/daemon-lib/src/manager/wiring/tier2_*.rs` (per-flavor wiring) · `crates/runtime/wild-host/src/core/plugin_manifest.rs` (cross-check).

---

## The `wild plugin` CLI

One verb spans install (system-wide bytes) and activation (per-profile).
Shipped subcommands (`crates/runtime/frontend/src/cli/plugin.rs`):

| Verb | Scope | Effect |
|---|---|---|
| `add` | install | pull/verify, write sidecar + wasm to the profile cache |
| `install` | install + profile | bundled-plugin shortcut: resolve the ref from the embedded bootstrap manifest, add + enable in one shot |
| `upgrade` | install | apply the feed-offered update through the governed daemon door (ADR-0223); needs a running daemon |
| `replace` | install | manual re-install from a target you name (deprecated alias: `update`) |
| `sync` | install | offline local-dev provisioning: materialise every in-tree bundled plugin into the cache |
| `remove` / `uninstall` | install | delete the sidecar + wasm (`uninstall` = disable + remove in one shot) |
| `list` | install | installed plugins, joined with active-profile enable status |
| `show` | install | one plugin's sidecar + per-profile config + trust |
| `enable` / `disable` | profile | flip the plugin's enable flag in the bootstrap manifest (whole plugin; ADR-0141 D15) |
| `config` | profile | set a config key (validated against the sidecar's `config_keys`) |
| `grant` / `revoke` / `grants` | profile | manage capability + secret overrides |
| `publisher add/rm/ls` | system | the operator's signing-key allowlist (ADR-0153) |

Per-primitive adoption (`worker.change-feed`, `effect.sharepoint`,
`function.sharepoint`) is controlled per-tribe via `wild tribe config`
(`tribes/<slug>/settings.yaml`). Disabling a primitive stops the host from
routing to it; the plugin instance itself stays loaded.

> 🔍 **Dig deeper — the operator surface.**
>
> - **CLI:** `plugin-cli.md` — the canonical verb reference · [`cli.md`](cli.md).

---

## What is deliberately *not* a plugin

| What | Why not |
|---|---|
| `wild:registry` | It resolves Tier-2 trust — chicken/egg |
| `wild:system`, `wild:forge` | Trust anchors; the host must own them |
| Wasmtime, the signature-verify path | Platform + trust bootstrap |
| Plugin-to-plugin direct calls | Composition is host-mediated only, so the trust graph stays acyclic. Two plugins that must compose do it through a third plugin that imports both as capabilities |

---

## Glossary

| Term | One line | Section |
|---|---|---|
| **Tier** | Where the bytes come from: native / embedded / OCI | *The three tiers* |
| **Flavor** | The WIT contract a Tier-2 plugin exports | *The five flavors* |
| **Kind** | The coarse on-disk role: `chief` / `provider` / `workload` | *The five flavors* |
| **Channel** | A transport carrying conversation to/from an external party (Telegram, whapi) — the transport axis, not a flavor | *Channels — the transport axis* |
| **Sidecar** | The immutable install-time `<slug>.json` manifest | *The sidecar manifest* |
| **plugin-meta** | The `manifest`/`init`/`shutdown` contract every Tier-2 plugin exports | *The plugin-meta contract* |
| **Trust tier** | Verified / Community / Unknown — gates capability allowance | *Trust model* |
| **ComponentBackedHostPlugin** | The generic shim that turns a capability-exporting component into a host plugin | *Boot flow* |

## References

- [`plugin-developer-guide.md`](plugin-developer-guide.md) — the author's handbook (WIT, build, walkthroughs).
- [`plugin-trust.md`](plugin-trust.md) · `bootstrap-and-default-inventory.md` — trust + the default inventory.
- `plugin-cli.md` — the `wild plugin` verb reference.
- [`architecture.md`](architecture.md) — where plugins sit in the embedded-host + NATS-bus picture.
- ADRs: 0002 (Elder/Chief/Worker) · 0013 (Tier-2 LLM policy) · 0014 (OCI namespace + chief registry) · 0045 (out-of-tree metadata + trust).
