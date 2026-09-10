# Examples

Worked examples, grouped by what they are. Two axes live here: things
you **deploy** (tribes) and things you **build** (components + the host
itself).

```
examples/
├── tribes/          # deployable tribe bundles + templates
├── packages/        # ADR-0156 domain packages (declarations-only bundles)
├── tool-providers/  # components that expose tools to workloads
├── llm-adapters/    # components that back wild:ai/chat with a model provider
├── embed-adapters/  # components that back wild:ai/embed (text → vector)
├── rerank-adapters/ # components that back the retrieve → rerank second stage
├── workers/         # workload components that wake on the bus and publish outcomes
├── channels/        # transports that carry operator notifications outward
├── widgets/         # renderer cards for derived apps (wild:ui)
├── consumers/       # source-free builds against the published wild:* WIT
└── embedding/       # downstream binaries that embed wild-hostd as a library
```

## tribes/

Pre-fabricated tribes you can apply to a fresh profile to see
end-to-end behaviour without writing a manifest from scratch.

| Bundle | What it does |
|---|---|
| `math-tribe/` | Smallest useful tribe — chief + one worker, multi-path tool use. End-to-end smoke fixture. |
| `news-pipeline/` | Hourly Hacker-News pull → two workers produce a B2B brief and a microfiction vignette from the same feed. |
| `liquidity-management/` | **DDD reference** (ADR-0108) — the model-first liquidity tribe: one `ontology/model.yaml` is the constitution, `authoring_method: ddd` compiles it into the full ontology (types **and** the 8 gated verbs) plus the ADR-0118 process flows. Start here for new tribes. |
| `fitness-tracker/` | **Template** — authored (source-less) ontology: Exercise/Workout/Goal vocabulary the operator fills over time. |
| `used-car-crm/` | **Goal-statement reference** — a used-car dealership CRM distilled from a production system: 30-min mobile.de folder sync, inquiry/viewing funnel with a calendar app, and a notify-and-wait price-offer flow. The reference answer for the "prompt → tribe" walk (`specs/goal-statement.md`). |
| `web-crawler/` | ADR-0114 queue-fed intake: a governed crawl with zero crawler-specific core code. |

Two bundle shapes coexist:

- **manifest bundles** (`math-tribe`, `news-pipeline`, `web-crawler`)
  carry `manifest.yaml` + `workers/` + `blueprint.md` — the on-disk
  shape `wild tribe apply <dir>` consumes. Every such bundle parses
  against the current schema in CI (`crates/runtime/wild-tribe-ops/src/bundle.rs::
  tests::every_examples_bundle_parses_against_current_schema`).
- **DDD templates** (`liquidity-management`, `fitness-tracker`) carry
  `specs/*.md` + a single `ontology/model.yaml` (`authoring_method: ddd`) —
  `wild tribe apply` compiles the model and pins the ontology. See each
  template's README.

The directories `invoice-formats-branch/`, `invoice-scans-extract/`,
`notify-and-wait/`, `receipts-gather/`, and `receipts-review-gather/` are
intake-flow **fixtures** used by the E2E suite (`tests/e2e/tribes/`). They
are valid bundles, but they are not aimed at operators learning the system.
`fleet-management/` is the same kind of fixture for the selection-eval
corpus: a deliberately near-empty genesis world (one `notiz` type) the
`a-genesis-interview-proposes-one-round` walk interviews its way out of.

### blueprint.md in the prompt-layer model

A manifest bundle's `blueprint.md` is the **mission layer** for the
chief that runs the tribe. Three layers shape every chief, generic to
specific:

1. **Engine core** — `prompts/chief-default/*.md` (binary-shipped).
2. **Operator overlay** — optional `<bundle>/CHIEF.md` (planned).
3. **Mission** — `<bundle>/blueprint.md`. What *this tribe* is for.

Full reference: `docs/prompt-layers.md` in the development repository
(not part of the published doc set).

### Deploying a tribe

```bash
wild profile new demo
wild --profile demo up &
wild --profile demo llm add claude          # the adapter the README names
wild --profile demo tribe apply examples/tribes/<name>/  # apply AND run
# (add --dormant to register it without starting, then `wild tribe activate <name>`)
wild --profile demo tui                         # watch it work
```

Tear down with `wild --profile demo down && wild profile delete demo --force`.

## tool-providers/

Components that implement the `wild:tool-provider` contract — they hand
tools to workloads.

| Component | What it shows |
|---|---|
| `sharepoint-connector/` | ADR-0141 PR1 existence proof: one component exercising all four installable compute primitives. Built with `cargo build --target wasm32-wasip2`. |
| `cash-forecast/` | The ADR-0202 reference Procedure — a seasonal cash-inflow forecast, the thinnest artifact exercising the whole model chain (build → pin → declare → gate → store → describe). |
| `payment-delay/` | The second ADR-0202 Procedure — per-debtor settlement dates: the learned correction to "everyone pays on the due date". |
| `fx-exposure/` | The two-axis ADR-0202 Procedure — what a foreign-currency receivable is worth in euro by the time it settles, answered as a band. |
| `ts-tool-provider/` | The same `wild:tool-provider` contract authored in TypeScript (built with `bun` + `jco`), proving the contract is language-agnostic. **Development repository only** — componentize-js has no component-model-async yet, which the async `invoke` of `wild:tool-provider@0.4.0` requires, so this sample is not published to the public repo until jco catches up. |

## llm-adapters/

Components that implement the `wild:llm-adapter` contract — they back
`wild:ai/chat` with a model provider.

| Component | What it shows |
|---|---|
| `echo-llm/` | The smallest complete llm-adapter: a deterministic echo backend exercising `chat`, the poll-cursor stream, and the native stream — the contract shape without any provider integration. Start here, then swap the echo for real HTTP calls. |

## embed-adapters/

Components that implement the `wild:embed-adapter` contract — they back
`wild:ai/embed` with a vector backend.

| Component | What it shows |
|---|---|
| `hash-embed/` | The smallest complete embed-adapter: deterministic hashed bag-of-words vectors (l2-normalized, no model, no network) — the whole contract on any machine, with one function to swap for a real provider. |

## rerank-adapters/

Components that implement the `wild:rerank-adapter` contract — the
cross-encoder second stage after embed retrieval.

| Component | What it shows |
|---|---|
| `overlap-rerank/` | The smallest complete rerank-adapter: joint query/document term overlap — the degenerate-but-real form of cross-encoding, deterministic and model-free. |

## workers/

Workload components — they wake on a bus subscription, do work, and
publish outcome events.

| Component | What it shows |
|---|---|
| `annotate-worker/` | The smallest complete worker: `wild:worker/handler` + `meta`, the standard `worker.bus.basic` sidecar shape, a deterministic transform, and a published result envelope (completed / failed / NACK paths). |

## channels/

Transport-axis plugins — they carry operator notifications to an
external party (or, here, somewhere inspectable).

| Component | What it shows |
|---|---|
| `journal-channel/` | The smallest complete channel: exports `wild:operator-channel/channel`, journals every delivered notification as a bus event you can watch — the wake → deliver shape without any account or token, triggerable end-to-end today. |

## widgets/

Renderer-side plugins (ADR-0173) — new card kinds for derived apps.

| Component | What it shows |
|---|---|
| `sticky-note/` | The smallest complete widget: `wild:ui/widget` rendering an escaped, config-driven note card; referenced from an app spec via the `custom` view kind. |
| `hello-widget.sidecar.json` | A sidecar-only manifest sample for the first-party reference widget. |

## consumers/

Source-free builds against the published `wild:*` WIT — reference
callers that show how to consume a contract without the monorepo.

| Component | What it shows |
|---|---|
| `embed-consumer/` | Reference `wild:ai/embed` caller (text→vector). |

## embedding/

Downstream binaries that embed the host as a library.

| Binary | What it shows |
|---|---|
| `custom-daemon/` | A custom `wild-hostd` that injects its own native provider via `manager::extension`, WITHOUT forking bootstrap. A workspace member so CI compiles it on every change (the real guard against a silent break of the extension contract). **Development repository only** — it path-depends on the private `wild-daemon` crate, so it cannot build outside this repo and is not published to the public repo. |

## Adding a new example

1. Put it under the right category (`tribes/`, `tool-providers/`,
   `llm-adapters/`, `embed-adapters/`, `rerank-adapters/`, `workers/`,
   `channels/`, `widgets/`, `consumers/`, `embedding/`).
2. Give it a self-contained README: what it does, which adapter /
   secrets it expects.
3. Tribes: keep schedules conservative (hourly+) so an unattended demo
   doesn't run up an LLM bill; no hard-coded secrets (`wild secret add`).
4. Components: standalone Wasm crates stay out of the root workspace
   (built via `cargo build --target wasm32-wasip2`); the only workspace
   member is `embedding/custom-daemon` (it builds a native binary).
