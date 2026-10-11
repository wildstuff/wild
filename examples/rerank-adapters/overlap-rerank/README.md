# `overlap-rerank` — deterministic `wild:rerank-adapter` teaching adapter

The copy-paste starting point for a Tier-2 rerank-adapter component. It
implements the full `wild:rerank-adapter@0.1.0` contract — the meta
lifecycle, the capability profile, and the one-shot `rerank()` call —
without any model or external service: relevance is a term-overlap
score computed from the request itself, so the retrieve → rerank second
stage can be exercised end-to-end, deterministically, on any machine.

## What it does

The whole "cross-encoder" is set overlap: query and each document are
tokenized to lowercase word sets (split on non-alphanumeric, Unicode
aware), and each document scores the Jaccard index
`|Q ∩ D| / |Q ∪ D|` against the query. Results come back sorted best
first (ties break on the lower index — the retrieve stage's order),
`top-n` truncates when set, the request's `model` string is echoed
back, and usage is all zeros — the WIT's own convention for local
adapters.

## Why term overlap is a real (if crude) reranker

It is not a toy in shape, only in strength:

- **It scores query and document jointly.** That is the one thing the
  embed-retrieve first stage cannot do — bi-encoder retrieval compares
  two *independently* computed vectors, while a reranker (this one
  included) sees both texts in the same call. Overlap is the smallest
  possible instance of that "look at the pair" property that defines
  the second stage.
- **Exact-term evidence is genuinely informative.** A candidate that
  shares literal query terms is usually more relevant than one that
  merely lands nearby in embedding space; lexical overlap is the
  degenerate, unweighted cousin of BM25. On short queries it will
  often fix the classic embed failure where a same-topic-but-wrong-item
  neighbour outranks the exact match.
- **It is deterministic.** Same request, same scores, same order —
  which makes the full plumbing (loader, shim, registry, resolver,
  fail-soft timeout, the consumer's `mode` labelling) testable without
  GPU, weights, or network.

What it cannot do is semantics: no synonyms, no cross-lingual matches,
no negation. That is exactly the part you swap out below.

## WIT surface

| Direction | Interface | Used for |
|---|---|---|
| export | `wild:plugin-meta/meta@0.3.0` | `manifest()` / `init()` / `shutdown()` — the Tier-2 lifecycle; `manifest()` is cross-checked against `sidecar.json` at load (slug/version mismatch = hard load error) |
| export | `wild:rerank-adapter/rerank@0.1.0` | `info()` capability profile + the single `rerank()` call (rerank has no streaming sibling) |
| import | `wild:secrets/store@0.1.0` | inherited from the shared plugin world — **never called**; tree-shaken out of the built component |
| import | `wild:http/outbound@0.1.0` | inherited from the shared plugin world — **never called**; tree-shaken out of the built component |

The world ([`wit/world.wit`](wit/world.wit)) is one line of substance:
`include wild:rerank-adapter/rerank-adapter-plugin@0.1.0;` — the same
world the shipping `llama-rerank` adapter composes. Because both
inherited imports are uncalled, the built `.wasm` imports nothing
beyond the standard wasip2 std surface — which is why the manifest's
`requires` list (and the sidecar's) is honestly empty.

## Build

```sh
cd examples/rerank-adapters/overlap-rerank
./build.sh
# → target/wasm32-wasip2/release/overlap_rerank.wasm
```

`build.sh` is just `cargo build --target wasm32-wasip2 --release`. The
`wasm32-wasip2` target comes from the repo's `rust-toolchain.toml`;
rustup installs it on first use. Every WIT package the world references
is `wild:*` text WIT checked into the repository (`wit/<pkg>/`),
reached through the `wit/deps/` symlinks — nothing to fetch first, no
external `wasi:*` packages involved.

## Install and wire

```sh
# 1. Install. The sidecar supplies slug + version verbatim — they must
#    match the component's manifest() self-report, and they do. The
#    sidecar's `provides` is the package-form coordinate
#    (`wild:rerank-adapter@0.1.0`); the rerank loader matches that
#    string exactly before wiring.
wild plugin add target/wasm32-wasip2/release/overlap_rerank.wasm \
  --sidecar sidecar.json

# 2. Wire it as a rerank adapter in <profile_root>/rerank-adapters.yaml (the
#    daemon materializes the file with an embedded default on first
#    boot; every rerank entry is `kind: component` — there is no
#    Tier-1 rerank path):
#
#    adapters:
#      - id: overlap
#        kind: component
#        slug: overlap-rerank
#    rerank_routing:
#      default: overlap
#
# 3. Restart the daemon (wild down && wild up) so the post-start
#    Tier-2 pass re-reads the file and warms the adapter pool.
```

How resolution works (ADR-0147, mirroring the embed plane): every
host-internal rerank call carries a caller token, and the adapter id
resolves `rerank_routing.caller_routing[caller]` →
`rerank_routing.default` → first wired adapter. So the block above
makes overlap the default for everything; a per-caller pin looks like
`caller_routing: { tribe-search: overlap }` and leaves other callers
on the default.

## Smoke test — honestly

There is no direct rerank ping verb in the CLI (nothing like
`wild config llm test` exists for this plane), so the smoke test reads
the surfaces the daemon already has:

1. **Boot wiring.** The daemon log names each rerank entry as it wires
   (or skips, with a reason) during the post-start Tier-2 pass — a
   misspelled slug or a missing plugin shows up here, not later.
2. **Drive a consumer.** Every rerank consumer goes through the one
   shared second stage, which is fail-soft: if the adapter is missing
   or broken the caller silently keeps the embed order. The honesty
   signal is the consumer's `mode` field — e.g. tribe search reports
   `semantic+rerank` / `keyword+rerank` only when the cross-encoder
   actually reordered. Run a search from chat (or any tool-search
   surface) and check the mode.
3. **Watch the event.** Each applied rerank logs one INFO event with
   `category: "ai.rerank"` (adapter id, caller, candidate count,
   elapsed ms, reorder delta) — visible in the daemon log and the
   `wild watch` Bus pane. With this adapter wired, that line naming
   `overlap` is the end-to-end proof.

Because the scorer is deterministic you can also assert behaviour, not
just liveness: a query repeating a document's exact words must rank
that document first, every time.

## Swapping in a real cross-encoder

Keep every signature; replace the scoring:

1. In `rerank()`, build a Jina-style `POST /v1/rerank` body
   (`{model, query, documents, top_n}`) and send it through the
   world's inherited `wild:http/outbound` import — the governed egress
   call: the host owns the socket, enforces the default-deny
   `egress.yaml` (a local endpoint needs an `allow_private`
   destination), injects credentials by reference, and audits every
   call. Map the response's `{index, relevance_score}` rows straight
   onto `rerank-result`.
2. Map failures onto the two-bucket `rerank-error`:
   connection-refused / 5xx / rate-limit → `retryable`, bad model /
   malformed input / 4xx → `permanent`.
3. If the provider needs an api-key, read it via
   `wild:secrets/store::get`, declare the alias in
   `manifest().secret_aliases` and the sidecar, and have the operator
   run `wild secret add <slug> <alias>`.
4. Declare real config (`model`, `endpoint`, …): parse the JSON bundle
   in `init()`, return typed `init-error`s so `wild doctor` can name
   what is missing, and list the keys in `config_keys` — a declared
   key you never read is a contract violation.
5. Update `info()` to tell the truth (`provider-family`, real
   `max-documents` / `max-query-chars` limits) — and remember the
   contract's rule that scores are provider-relative: callers must
   never threshold them across models.

`plugins/rerank/llama/` in the development repository is the
fully-worked version of exactly these steps — the bundled llama.cpp
adapter behind the `llama://rerank` endpoint marker — sharing this
sample's layout.
