# `embed-consumer` — reference `wild:ai/embed` caller

The copy-paste starting point for a component that turns text into
embedding vectors. The provider side of `wild:ai/embed` is shipped
(adapter contract → host → shim → `ollama-embed` adapter → conformance →
bootstrap; see `docs/embed-adapters.md` in the development repository);
this is the **first consumer**.

It is the mirror image of an adapter: an adapter *exports*
`wild:embed-adapter` and the host serves `wild:ai/embed` on top of it;
this component *imports* `wild:ai/embed` and calls it.

## What it does

Workload-flavor Tier-2 plugin (the ai-worker shape, minus
chat/files/tools). The host wakes it per NATS message on the
manifest-declared subscription (`wild.{tribe}.embed.req`). Each
invocation:

1. Reads the message body as the text to embed
   (`String::from_utf8_lossy(&msg.body)` — no envelope; a consumer owns
   its own wire shape).
2. Calls `wild:ai/embed.embed(...)` with `model: none` and a
   `caller_context.kind` token. The host resolves the adapter through
   `<profile_root>/embed-adapters.yaml` (`embed_routing.default` or
   `embed_routing.caller_routing`). An explicit `model` is still allowed
   as an author pin.
3. On success publishes `{model, dims, count, preview}` JSON to
   `<subject>.result`; on error publishes the message to
   `<subject>.error`.

The result carries only a **preview** (the first few vector
components) — the full vector belongs in a vector store
(`wild:data`, out of scope here).

It is deliberately generic: it embeds whatever text arrives. The
motivating use is entity-linking (contract text → vector → candidate
match), but nothing here is domain-specific.

## Config keys (via `wasi:config/runtime`)

| Key | Required | Meaning |
|---|---|---|
| `wild/subscribe-subject` | yes | The NATS subject the host dispatches into `handle-message`. Derived from the sidecar's `subscription_template` at deploy time (`wild.{tribe}.embed.req`). |

## Build

```sh
cd examples/consumers/embed-consumer
cargo component build --release
# → target/wasm32-wasip1/release/embed_consumer.wasm
```

Confirm the WIT surface:

```sh
wasm-tools component wit target/wasm32-wasip1/release/embed_consumer.wasm \
  | grep -E 'import wild:ai/embed|export wild:messaging/handler'
# import wild:ai/embed@0.4.0;
# export wild:messaging/handler@0.3.0;
```

> The build resolves `wasi:*` packages out of `wit/external/*.wasm`. In
> the public repo those files are vendored — nothing to do. In the
> development repository they are gitignored; the first build after a
> fresh clone needs them fetched: `cargo run -p xtask -- wit-sync`.

## Deploy

A working embed adapter must be wired first — see
`docs/embed-adapters.md` § Operator install walkthrough in the
development repository (`ollama-embed` + a local `ollama serve`).

```sh
# 1. Install via the shipped sidecar. The loader derives the workload
#    flavor from `provides[]` (`wild:messaging/handler`), so no explicit
#    `kind` or legacy `component_type`/`wit_baseline` fields are needed
#    (ADR-0141 PR0.5). The positional `.wasm` supplies the bytes;
#    `--sidecar` supplies the manifest verbatim.
wild plugin add target/wasm32-wasip1/release/embed_consumer.wasm \
  --sidecar sidecar.template.json

# 2. (Optional) Pin the consumer's caller token in embed-adapters.yaml:
#    embed_routing:
#      default: ollama-nomic-embed
#      caller_routing:
#        embed-consumer-example: ollama-nomic-embed

# 3. Send a message; read the result.
nats pub  'wild.<tribe>.embed.req' 'the quick brown fox'
nats sub  'wild.<tribe>.embed.req.result'
# → {"model":"nomic-embed-text","dims":768,"count":1,"preview":[...]}
```

The `subscription_template` in [`sidecar.template.json`](sidecar.template.json)
is the registration: the host turns it into the `wild/subscribe-subject`
component config at deploy time, and `NatsMessagingPlugin` spawns the
subscribe-loop that routes matching messages into `handle-message`.

## End-to-end test

In the development repository,
`crates/runtime/wild-host/tests/embed_consumer_e2e.rs`
drives this component through a real `wild-host` against a live Ollama:
publishes on `wild.{tribe}.embed.req`, asserts the result on `.result`
carries `dims == ` the model's dimension. `#[ignore]` + gated — skips
cleanly when the wasm isn't built, NATS is down, or Ollama is absent.

## See also

Both in the development repository:

- `docs/embed-adapters.md` § Calling embed from a component — the call
  ergonomics + the `caller-context` umbrella-vs-adapter distinction.
- `plugins/workers/ai-worker/` — the fuller workload-flavor reference
  (chat + tool-loop + files).
