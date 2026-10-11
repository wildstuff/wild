# `annotate-worker` — minimal workload-flavor worker

The copy-paste starting point for a **worker**: a Tier-2 plugin the host
wakes once per NATS message, that does its work, and that publishes the
outcome back onto the bus. The work here is deliberately trivial and
deterministic — annotate + uppercase a text field, count words — because
the point is the **wake-on-subscription → work → publish-outcome**
shape, not the work. No LLM calls, no external services, no host
workspace membership: the whole plugin is this directory.

## The contract

| Surface | WIT | Role |
|---|---|---|
| lifecycle | `include wild:plugin-meta/plugin-base@0.3.0` | `manifest()` / `init()` / `shutdown()` — the minimum every Tier-2 plugin exports. The host cross-checks `manifest()` against the sidecar at load time; a `slug`/`version` mismatch is a hard load error. |
| wake-up | `export wild:worker/handler@0.1.0` | The installable worker primitive. There is no arbitrary export the host calls — a workload is driven by a delivered message. The host invokes `handle-message` once per task on the sidecar-declared subscription; `Err(..)` NACKs the delivery. |
| trigger discovery | `export wild:worker/meta@0.1.0` | Optional: `list-triggers()` lets a dashboard render this worker's choosable triggers without hard-coding them. |
| outcome publish | `import wild:messaging/consumer@0.3.0` | Fire-and-forget publish of the result event. The only capability this worker needs. |

The sidecar ([`sidecar.json`](sidecar.json)) carries the other half of
the contract:

- `wake_up.subscription_template: "wild.{tribe}.worker.{agent-name}.task"`
  — the registration. At deploy time the host renders the template and
  spawns the subscribe-loop that routes matching messages into
  `handle-message`. The worker never subscribes itself.
- `capability_bundles: ["worker.bus.basic"]` — the standard worker
  grant: subscribe on `wild.{tribe}.worker.{worker}.task`, publish on
  `wild.{tribe}.worker.{worker}.result`. Nothing wider.
- `trust.tier: "community"` — what a third-party build is.

## The wire shape

Inbound, the task subject carries the host's `Envelope<WorkerTask>`; the
fields this worker reads are the routing ids, `prompt` (the text to
transform) and `result_subject` (where the outcome goes). Outbound it
publishes a child `Envelope<WorkerResultEvent>` — same `trace_id`,
`parent_id` = the task envelope's `id` — with `status`
(`completed`/`failed`), a one-line `summary`, and the annotation as
`result_json`. A failure it could read is published as a `failed`
*result*, not swallowed and not NACK'd: a failure is an outcome, and the
dispatcher only sees outcomes that arrive on the result subject.

The shapes are hand-mirrored as small serde structs in
[`src/lib.rs`](src/lib.rs) so the example stays self-contained; inside
the development repository a worker uses `common::worker_runtime`
(`decode_worker_task` / `result_for_task` / `encode_worker_result`)
instead.

## Build

```sh
cd examples/workers/annotate-worker
./build.sh
# → target/wasm32-wasip2/release/annotate_worker.wasm
```

`build.sh` is one line: `cargo build --target wasm32-wasip2 --release`.
The produced `.wasm` is a component-model binary the embedded host loads
directly. Every WIT package the world references is `wild:*` text WIT
checked into the repository (`wit/<pkg>/`), reached through the
`wit/deps/` symlinks — nothing to fetch first, no external `wasi:*`
packages involved.

Confirm the surface (if you have `wasm-tools`):

```sh
wasm-tools component wit target/wasm32-wasip2/release/annotate_worker.wasm \
  | grep -E 'export wild:worker/handler|import wild:messaging/consumer'
# import wild:messaging/consumer@0.3.0;
# export wild:worker/handler@0.1.0;
```

## Install + watch it work

```sh
# 1. Install. The loader derives the workload flavor from `provides[]`
#    (`wild:worker/handler`); the positional `.wasm` supplies the bytes,
#    `--sidecar` the manifest verbatim.
wild plugin add target/wasm32-wasip2/release/annotate_worker.wasm \
  --sidecar sidecar.json

wild plugin show annotate-worker   # sidecar + derived primitive roles
```

The subscription template renders per deployed worker instance
(`{tribe}` = the tribe id, `{agent-name}` = the instance's routing
slug). To drive one delivery by hand, publish a task envelope on the
rendered subject and watch the result subject — with a running profile
(`wild up`), `nats` pointed at its bus:

```sh
# 2. Watch the outcome subject first.
nats sub 'wild.demo.worker.annotate.result' &

# 3. Publish a task (an Envelope<WorkerTask>; ids are any UUIDs).
nats pub 'wild.demo.worker.annotate.task' '{
  "v": 1,
  "id": "2e9f2a7e-0000-4000-8000-000000000001",
  "ts": "2026-08-14T12:00:00Z",
  "tribe_id": "demo",
  "trace_id": "2e9f2a7e-0000-4000-8000-000000000002",
  "parent_id": null,
  "payload": {
    "task_id": "2e9f2a7e-0000-4000-8000-000000000003",
    "tribe_id": "demo",
    "cycle_id": "2e9f2a7e-0000-4000-8000-000000000004",
    "worker_name": "annotate",
    "component_type": "annotate-worker",
    "prompt": "the quick brown fox",
    "context_keys": [],
    "result_subject": "wild.demo.worker.annotate.result",
    "timeout_seconds": 30
  }
}'
# → status "completed", summary "annotated 19 chars (4 words)",
#   result_json.annotated = "[annotate-worker] THE QUICK BROWN FOX"
```

The same traffic is visible without `nats`: the Bus pane in `wild watch`
is the firehose, and the daemon log carries the dispatch as
`lifecycle.component.call` lines (`docs/observability.md` in the
development repository has the filter recipes).

## From here to a real worker

Keep the shape, replace the middle:

- **Real work** — swap the `annotate` step for yours. Stay deterministic
  where you can; a worker that needs an LLM imports `wild:ai/chat` and
  declares the `ai.chat` bundle (see `plugins/workers/ai-worker/` in the
  development repository for the full-size reference).
- **More capabilities** — each is one WIT import in `wit/world.wit`, one
  `wit/deps/` symlink, one `[package.metadata.component.target.dependencies]`
  row, one entry in `manifest().requires`, and the matching sidecar
  `capability_bundles` entry. The host grants nothing you did not
  declare.
- **Config** — declare `config_keys` in the manifest + sidecar and parse
  them in `init()`, returning a typed `init-error` so `wild doctor` can
  say why the plugin was skipped. A declared key you never read is a
  contract violation.
- **Failure honesty** — keep the split this example draws: unreadable
  task → `Err` (NACK, host logs it); readable-but-undoable task →
  publish a `failed` result with a summary a non-engineer can act on.
