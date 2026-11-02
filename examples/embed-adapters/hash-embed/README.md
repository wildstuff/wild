# `hash-embed` — deterministic `wild:embed-adapter` teaching adapter

The copy-paste starting point for a Tier-2 embed-adapter component. It
implements the full `wild:embed-adapter@0.1.0` contract — the meta
lifecycle, the capability profile, and the one-shot batch embed call —
without any external service: the vector is derived from the input text
itself, so every path can be exercised end-to-end, deterministically,
on any machine.

**This is a TEACHING backend.** The vectors are real (unit-length
`f32`, batch index-aligned, stable across machines) but semantically
naive: two texts are close only when they share literal words. Nothing
here understands meaning — the contract mechanics are the point, not
the embeddings. See [Turning it into a real provider
adapter](#turning-it-into-a-real-provider-adapter) for exactly what to
replace — it is one function.

## What it does

The whole "model" is a hashed bag-of-words: lowercase the text, split
on whitespace, FNV-1a-hash every token into one of `dims` buckets
(default 256; the hash's top bit picks +1/−1 so common words don't all
pile onto positive mass), then l2-normalize. Batches map input-for-input
onto `embeddings` (the contract's index-alignment rule), the response
echoes the request's `model` back, and `usage.input-tokens` reports the
genuine token count — this model actually knows it. A text with no
tokens still gets a deterministic non-zero vector (the conformance
suite's non-degeneracy rule). Errors follow the two-bucket split:
malformed input (`dimensions: 0`, empty batch) is `permanent`, never
`retryable` — retrying the same bytes can't succeed.

## Which world — and why

This sample exports **`embed-adapter-plugin`** (via
`include wild:embed-adapter/embed-adapter-plugin@0.1.0;` in
[`wit/world.wit`](wit/world.wit)) — the same world the shipping
adapters (`ollama-embed`, `llama-embed`) build on. Unlike the
LLM-adapter contract there is no streaming sibling and no world choice
to make: embeddings return as one aggregated batch, so `embed` is the
whole export surface, and the crate needs neither
component-model-async nor wit-bindgen's `async-spawn` feature.

## WIT surface

| Direction | Interface | Used for |
|---|---|---|
| export | `wild:plugin-meta/meta@0.3.0` | `manifest()` / `init()` / `shutdown()` — the Tier-2 lifecycle; `manifest()` is cross-checked against `sidecar.json` at load (slug/version mismatch = hard load error) |
| export | `wild:embed-adapter/embed@0.1.0` | `info()` capability profile + the batch `embed()` call |
| import | `wild:secrets/store@0.1.0` | inherited from the shared world — **never called**; tree-shaken out of the built component |
| import | `wild:http/outbound@0.1.0` | inherited from the shared world (the governed egress surface, ADR-0090) — **never called**; tree-shaken out of the built component |

Because both inherited imports are uncalled, the built `.wasm` imports
nothing beyond the standard wasip2 std surface — which is why the
manifest's `requires` list (and the sidecar's) is honestly empty.

## Build

```sh
cd examples/embed-adapters/hash-embed
./build.sh
# → target/wasm32-wasip2/release/hash_embed.wasm
```

`build.sh` is just `cargo build --target wasm32-wasip2 --release`. The
`wasm32-wasip2` target comes from the repo's `rust-toolchain.toml`;
rustup installs it on first use.

> Every WIT dependency here is wild-owned text WIT reached through the
> `wit/deps/` symlinks — no `wasi:*` package, so unlike the `echo-llm`
> sibling there is nothing to fetch (`wit-sync` not needed) even on a
> fresh development-repository clone.

## Install and smoke-test

```sh
# 1. Install. The sidecar supplies slug + version verbatim — they must
#    match the component's manifest() self-report, and they do.
#    (Installing without --sidecar stamps version 0.0.0+local, which
#    then FAILS the load-time cross-check against the component's real
#    0.1.0 — use the sidecar.)
wild plugin add target/wasm32-wasip2/release/hash_embed.wasm \
  --sidecar sidecar.json

# 2. Wire it as an adapter: the yaml entry's `slug` names the installed
#    plugin, the `id` is what embed_routing and callers reference. Add
#    to <profile_root>/embed-adapters.yaml (a separate file from
#    llm-adapters.yaml — one config surface per modality; every embed
#    entry is `kind: component`):
#
#    adapters:
#      - id: hash
#        kind: component
#        slug: hash-embed
#        model: hash-256      # echoed back on every response
#
#    embed_routing:
#      default: hash
#
# 3. Restart the daemon so the post-start pass picks the entry up.
wild up
```

The chat plane's `wild config llm test <id>` has no embed sibling yet — no
verb tests an embed adapter end-to-end — so verification is two honest steps:

1. **Wired?** The boot log prints the subsystem-up line:
   `category: "boot.subsystem-up", subsystem: "wild:ai/embed", "… Tier-2 adapter wired"`
   (a per-entry skip with a reason means the sidecar is missing or the
   manifest cross-check failed).
2. **Embedding?** Drive it through the consumer sample,
   [`examples/consumers/embed-consumer`](../../consumers/embed-consumer/)
   — install it, then round-trip a message over the bus:

   ```sh
   nats sub 'wild.<tribe>.embed.req.result' &
   nats pub 'wild.<tribe>.embed.req' 'the quick brown fox'
   # → {"model":"hash-256","dims":256,"count":1,"preview":[...]}
   ```

   Publishing the same text twice returns the identical preview —
   that determinism is this adapter's signature.

No secret, no endpoint, no egress allowlist entry — nothing leaves the
sandbox.

## Turning it into a real provider adapter

Keep every signature; replace **one function** — `embed_text` in
[`src/lib.rs`](src/lib.rs). That's the entire seam:

1. In `embed()`, build the provider's wire payload from the whole batch
   (one round-trip embeds many strings), send it through the world's
   inherited `wild:http/outbound::fetch` import, and decode the reply
   into `embed-response` (index-aligned vectors, `dims` from the first
   vector, the provider's usage counters into `token-usage`). The
   egress is governed (ADR-0090): the operator's `egress.yaml` must
   allow the destination — a local endpoint needs an `allow_private`
   entry.
2. Read the api-key via `wild:secrets/store::get("api-key")`, declare
   the alias in `manifest().secret_aliases` and the sidecar, and have
   the operator run `wild secret add <slug> api-key`. (A local provider
   like Ollama needs no key — skip this step.)
3. Declare real config (`model`, `endpoint`, …): parse the JSON bundle
   in `init()`, return typed `init-error`s so `wild doctor` can name
   what is missing, and list the keys in `config_keys`.
4. Update `info()` to tell the truth about the upstream:
   `max-dimensions` = the model's native width, `supports-dimensions`
   only if the model really honours dimension truncation, and sort the
   two error buckets by the provider's status codes (connection / 5xx /
   429 → `retryable`; 4xx / bad model → `permanent`).

`plugins/embed/ollama/` in the development repository is the
fully-worked version of exactly these steps, sharing this sample's
layout.
