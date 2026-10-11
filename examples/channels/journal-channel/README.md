# `journal-channel` — minimal channel-flavor operator channel

The copy-paste starting point for a **channel** (the transport axis): a
Tier-2 plugin the host calls once per operator notification, that
renders the payload, and that hands it to a transport. The transport
here is deliberately trivial — journal the notification as one bus
event — because the point is the **host-calls-deliver → render →
hand-to-transport** shape, not the transport. No external service, no
account, no token, no host workspace membership: the whole plugin is
this directory.

The direction is the part that trips people up. A tool-provider plugin
exports tools the host calls *on demand*; a channel plugin exports a
surface the host pushes *to*. The host holds no channel names — any
deployed plugin exporting `wild:operator-channel/channel` is unioned
into the operator notifier at daemon post-start, keyed by its slug.
When an escalation opens anywhere (a fault route, a stuck forge build,
a chief escalation), the notifier fans out to every accepting channel's
`deliver()`.

## The contract

| Surface | WIT | Role |
|---|---|---|
| lifecycle | `include wild:plugin-meta/plugin-base@0.3.0` | `manifest()` / `init()` / `shutdown()` — the minimum every Tier-2 plugin exports. The host cross-checks `manifest()` against the sidecar at load time; a `slug`/`version` mismatch is a hard load error. |
| channel | `export wild:operator-channel/channel@0.1.0` | `deliver(notification, config)` — once per fanned-out notification; `capabilities()` — polled once at load, cached by the host. The slug is the channel token a `system/channels.yaml` binding row names. |
| journal sink | `import wild:messaging/consumer@0.3.0` | One fire-and-forget publish per delivery. The only capability this channel needs. |

Per delivery the guest publishes one JSON journal entry on
`wild.{tribe}.user.journal.notification` (the existing
`wild.{tribe}.user.{channel}.notification` subject shape — no new
subject root) carrying the full rendered notification: id, severity,
kind, pii-class, title, body (truncated at the declared
`max-body-chars`, with the cut reported honestly in the receipt's
`rendered-truncated`), the action list, and the deeplink. Config VALUES
are never journaled — only the key names — because a binding's config
can name secret aliases.

## Why the bus is the sink

Three candidate sinks were considered for a dependency-light sample:

- **`wild:messaging/consumer` (chosen).** Open at the `community` trust
  tier with no grant command, directly observable in `wild watch`'s Bus
  pane and via `nats sub`, and the same import the worker example
  (`examples/workers/annotate-worker/`) already teaches — one publish
  call, no paths, no filesystem layout to explain.
- **`wild:files/write`.** Works, but its community-tier policy is
  explicitly `open-untriaged` (granted today, not security-reviewed),
  and a file sink needs a workspace root and path conventions that are
  noise next to the channel contract.
- **WASI stdout/stderr.** Not an operator-documented surface for
  channel plugins — nothing in `docs/observability.md` promises where
  it lands.

## Build

```sh
cd examples/channels/journal-channel
./build.sh
# → target/wasm32-wasip2/release/journal_channel.wasm
```

`build.sh` is one line: `cargo build --target wasm32-wasip2 --release`.
The produced `.wasm` is a component-model binary the embedded host
loads directly. Every WIT package the world references is `wild:*` text
WIT checked into the repository (`wit/<pkg>/`), reached through the
`wit/deps/` symlinks — nothing to fetch first, no external `wasi:*`
packages involved.

Confirm the surface (if you have `wasm-tools`):

```sh
wasm-tools component wit target/wasm32-wasip2/release/journal_channel.wasm \
  | grep -E 'export wild:operator-channel/channel|import wild:messaging/consumer'
# import wild:messaging/consumer@0.3.0;
# export wild:operator-channel/channel@0.1.0;
```

## Install + watch a delivery end-to-end

```sh
# 1. Install. The loader derives the channel role from `provides[]`
#    (`wild:operator-channel/channel`); the positional `.wasm` supplies
#    the bytes, `--sidecar` the manifest verbatim.
wild plugin add target/wasm32-wasip2/release/journal_channel.wasm \
  --sidecar sidecar.json

# 2. Restart the profile. The operator notifier is assembled ONCE per
#    daemon run at post-start (a write-once slot) — a channel added
#    while the daemon runs joins on the next boot.
wild down && wild up
# daemon log: "operator push notifier installed (channels: …, journal-channel)"

# 3. Watch the journal subject, then open an escalation by hand. The
#    escalation subscriber listens on wild.system.escalation.>, persists
#    the row, and the store's fresh-open hook fans out to every channel:
nats sub 'wild.demo.user.journal.notification' &

nats pub 'wild.system.escalation.demo.warn' '{
  "escalation_id": "esc-journal-demo-1",
  "tribe_id": "demo",
  "severity": "warn",
  "summary": "journal-channel smoke test",
  "context_json": null,
  "created_at": null
}'
# → one JSON journal entry: severity "info" (escalation warn maps down),
#   kind "audit-finding", title "journal-channel smoke test",
#   deeplink "wild://inbox/esc-journal-demo-1"
```

The same traffic is visible without `nats`: the Bus pane in `wild
watch` is the firehose, and the daemon log carries one INFO
`notify.pushed` line per delivered channel (`channel=journal-channel`)
— filter recipes in `docs/observability.md` in the development
repository. Re-publishing the same escalation id while the row is open
does NOT re-deliver: the fresh-open hook pushes once per opening, which
is the idempotency half of the contract.

Deliveries need no `system/channels.yaml` row — an unbound channel
accepts everything. Adding a binding row is how an operator narrows it,
and it takes effect live (re-read per delivery, no restart):

```yaml
channels:
  - channel: journal-channel        # this plugin's slug = its token
    tribe: demo
    min_severity: warning           # journal only warning and up
```

## What works today, and what waits

Everything above is exercisable now: load, capability seed, fan-out,
`deliver()`, receipt, `notify.pushed`, binding filters. Two honest
edges:

- **Outbound-only.** `wild:operator-channel` (ADR-0044) is the
  host→operator direction. The inbound half — an operator *replying*
  through a channel and the reply resolving the escalation — is
  ADR-0159 (bidirectional channels), proposed but not accepted; a
  channel that wants it today ships a separate `wild:channel/inbound`
  parser. This sample records the `actions` list in the journal so a
  reader sees what a reply *would* pick, but nothing routes back.
- **Boot-time union.** The notifier is a write-once, per-process slot:
  `wild plugin add` stages the plugin, the next daemon start wires it.
  There is no live channel hot-add today.

- **`pull` vs `push` is metadata, not routing.** This channel declares
  `pull` (the operator inspects the journal; nothing pings an external
  endpoint). The notifier currently fans out to every accepting channel
  regardless of kind — the declaration matters for honesty and for
  future severity-keyed routing, not for whether `deliver()` runs.

## From here to a real channel

Keep the shape, replace the transport:

- **Real push** — swap the journal publish for the vendor call. The
  full-size reference is `plugins/channels/telegram-channel/` in the
  development repository: same export, but `deliver()` renders to the
  vendor format, sends over the governed `wild:http/outbound` egress
  (the credential is injected host-side and never enters the guest),
  and returns the vendor's message id in the receipt.
- **A recipient** — a push channel reads its recipient (chat id,
  address) from the `config` map the host resolves per delivery from
  `system/channels.yaml`. An empty map means the tribe has no binding:
  report a skip (`permanent`), never guess a recipient.
- **Honest capabilities** — declare what the surface really renders
  (`supports-rich-markdown`, `supports-inline-actions`,
  `max-body-chars`); Elder pre-filters notifications that would degrade
  poorly, and `rendered-truncated` in the receipt is the per-delivery
  truth.
- **Error semantics drive the distributor** — `unavailable` invites a
  fallback channel, `permanent` gives up, `rate-limited(n)` backs off
  `n` seconds. Map transport failures onto these deliberately.
