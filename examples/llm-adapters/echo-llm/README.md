# `echo-llm` — deterministic `wild:llm-adapter` teaching adapter

The copy-paste starting point for a Tier-2 LLM-adapter component. It
implements the full `wild:llm-adapter@0.3.0` contract — the meta
lifecycle, the capability profile, the buffered chat call, and both
streaming surfaces — without any external service: the "completion" is
derived from the request itself, so every path can be exercised
end-to-end, deterministically, on any machine.

## What it does

The whole "model" is an echo: find the last `user`-role message, join
its text (and tool-result) blocks, reply `echo: <that text>`. Token
usage is a deterministic estimate (~4 chars per token), the stop-reason
is always `end-turn`, and the response echoes the request's `model`
back — the contract's rule when the upstream did not reroute. Streaming
replays the same reply word-by-word, then a terminal chunk carrying the
final usage / model / stop-reason.

## Which world — and why

This sample exports **`llm-adapter-streaming-plugin`** (via
`include wild:llm-adapter/llm-adapter-streaming-plugin@0.2.0;` in
[`wit/world.wit`](wit/world.wit)), not the smaller `llm-adapter-plugin`
world. Two reasons:

1. **It is the world that loads.** The host shim bindgen's against the
   `llm-adapter-host` world, which resolves all three chat exports
   (`chat`, `chat-stream`, `chat-stream-native`) when it builds its
   typed accessor — a chat-only component fails that lookup before the
   first dispatch. All shipping adapters export the streaming world.
2. **The streaming mechanics are the teaching value.** Because the echo
   reply is known up front, the poll-cursor resource and the native
   `stream<token-chunk>` + `future<chat-response>` producer each cost a
   few dozen readable lines — exactly the scaffolding a real adapter
   needs around its SSE decoder.

The native path (`chat-stream-native`) does use component-model-async
(`async func`, `stream`/`future`), which is why the crate pins
`wit-bindgen = "=0.58.0"` with the `async-spawn` feature — the same pin
the shipping adapters use.

## WIT surface

| Direction | Interface | Used for |
|---|---|---|
| export | `wild:plugin-meta/meta@0.3.0` | `manifest()` / `init()` / `shutdown()` — the Tier-2 lifecycle; `manifest()` is cross-checked against `sidecar.json` at load (slug/version mismatch = hard load error) |
| export | `wild:llm-adapter/chat@0.2.0` | `info()` capability profile + the buffered `chat()` call |
| export | `wild:llm-adapter/chat-stream@0.2.0` | poll-cursor streaming resource (contract shape; the host drives only the native path) |
| export | `wild:llm-adapter/chat-stream-native@0.2.0` | the native token stream the host actually drains |
| import | `wild:secrets/store@0.1.0` | inherited from the shared world — **never called**; tree-shaken out of the built component |
| import | `wasi:http/outgoing-handler@0.2.0` | inherited from the shared world — **never called**; tree-shaken out of the built component |

Because both inherited imports are uncalled, the built `.wasm` imports
nothing beyond the standard wasip2 std surface — which is why the
manifest's `requires` list (and the sidecar's) is honestly empty.

## Build

```sh
cd examples/llm-adapters/echo-llm
./build.sh
# → target/wasm32-wasip2/release/echo_llm.wasm
```

`build.sh` is just `cargo build --target wasm32-wasip2 --release`. The
`wasm32-wasip2` target comes from the repo's `rust-toolchain.toml`;
rustup installs it on first use.

> The build resolves `wasi:*` packages out of `wit/external/*.wasm`
> (via the `wit/deps/` symlinks). In the public repo those files are
> vendored — nothing to do. In the development repository they are
> gitignored; the first build after a fresh clone needs them fetched
> once: `cargo run -p xtask -- wit-sync`.

## Install and smoke-test

```sh
# 1. Install. The sidecar supplies slug + version verbatim — they must
#    match the component's manifest() self-report, and they do. (Plain
#    `--name echo-llm` works too; without either, the slug would
#    default to the file stem `echo_llm` and fail the cross-check.)
wild plugin add target/wasm32-wasip2/release/echo_llm.wasm \
  --sidecar sidecar.json

# 2. Wire it as an adapter: the yaml entry's `slug` names the installed
#    plugin, the `id` is what strategies and workers reference. Add to
#    <profile_root>/llm-adapters.yaml (or the per-profile copy):
#
#    adapters:
#      - id: echo
#        kind: component
#        slug: echo-llm
#        model: echo-1        # echoed back on every response
#
# 3. Restart the daemon so the loader picks the entry up, then ping the
#    adapter directly — no provisioning, no cost:
wild config llm test echo
# → a response whose content starts with "echo: "
```

No secret, no endpoint, no egress allowlist entry — nothing leaves the
sandbox. One slug can be wired under several ids; for a real provider
each id typically pins its own `model` (and, for the `openai` slug, its
own `endpoint`).

## Turning it into a real provider adapter

Keep every signature; replace the reply derivation:

1. In `chat()` / the native producer, build the provider's wire payload
   from the `chat-request`, send it through the world's inherited
   `wasi:http/outgoing-handler` import, and decode the reply into
   `chat-response` (map the provider's finish reason onto
   `stop-reason`, normalise token counters into `token-usage`).
2. Read the api-key via `wild:secrets/store::get("api-key")`, declare
   the alias in `manifest().secret_aliases` and the sidecar, and have
   the operator run `wild secret add <slug> api-key`.
3. Declare real config (`model`, `endpoint`, …): parse the JSON bundle
   in `init()`, return typed `init-error`s so `wild doctor` can name
   what is missing, and list the keys in `config_keys`.
4. Update `info()` to tell the truth about what the upstream supports —
   callers branch on the capability bools, never on the slug.

`plugins/llm/openai/` in the development repository is the fully-worked
version of exactly these steps, sharing this sample's layout.
