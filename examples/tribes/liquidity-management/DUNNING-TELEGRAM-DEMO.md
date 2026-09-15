# Dunning over Telegram — the ADR-0098 WS1 demo (deterministic since ADR-0124)

The end-to-end **outbound customer channel** chain: an operator-gated dunning notice
is authored, and a Chief-free flow delivers it to the debtor over Telegram — all
on the shipped substrate. Since the ADR-0124 proof point the routine delivery is
**deterministic** (no LLM turn): the model's `dunning-escalation` process is a
multi-stage flow of host mechanisms, and only an out-of-scale escalation reaches
the agentic worker.

```
dun_invoice (operator-gated)                          ADR-0066
  → a `dunning` record is written                      ADR-0096 WS2a (entity-change event)
    → the model's `dunning-escalation` FLOW walks it   ADR-0118 PR-12 (reactive multi-stage walk)
      → lookup: business_partner.telegram_chat_id       ADR-0124 (governed read, no agentic worker)
        → template: the notice body from the level       ADR-0124 (deterministic render, no LLM prose)
          → telegram-send tool → api.telegram.org         ADR-0098 WS1 (#2118) over wild:http (#2025)
            → bot token injected into the URL path         ADR-0098 WS1 Step 0 (#2116, path:<sentinel>)
              → on_done: write status=sent                   ADR-0127 (completion-coupled, after the send)
                → one audit.egress row                        ADR-0090 (governed, token never logged)
```

## What this example already ships

- The `dunning` entity (`ontology/model.yaml`, seeded via `data/dunnings.csv`) with a
  `status` field — `DUN-SEED-001` is `open` for partner `BP-004`.
- `business_partner.telegram_chat_id` (ADR-0098) — the recipient address; debtors
  `BP-001/002/004/006` carry one, the rest are unset (opt-in → an audited stop).
- The `dun_invoice` verb that authors a `dunning`.

## Operator setup (one-time)

1. **Deploy the `telegram-send` tool-provider** (ADR-0098 WS1, #2118) — pulled from
   `ghcr.io/wildstuff/plugins/tools/telegram-send` or built with
   `plugins/scripts/build-wasip2-component.sh plugins/tools/telegram-send`.
2. **Store the bot token** (never in a guest): `wild secret add telegram-bot-token`,
   which prompts for the value. The binding below names `telegram_bot_token`, the
   spelling that shipped; the host falls back from the declared name to its
   canonical form (#7307), so both reach the one stored key.
3. **Grant the egress destination** — add to `system/egress.yaml` (the `path:<sentinel>`
   scheme, ADR-0098 WS1 Step 0 / #2116, is what lets Telegram's token-in-path work):
   ```yaml
   destinations:
     - name: telegram
       hosts: ["api.telegram.org"]
       auth:
         telegram_bot: { secret: telegram_bot_token, inject: "path:WILDBOTTOKEN" }
   ```
   The sentinel `WILDBOTTOKEN` matches the one the `telegram-send` component writes into
   the request path; the host substitutes the resolved token. The token never enters the
   guest and never lands in the `audit.egress` row (which carries only the host).

## The deterministic flow — **host mechanisms, config from the model**

The routine notice costs **zero LLM turns**. The model's `dunning-escalation`
process (ADR-0118 D14, in `ontology/model.yaml`) declares the whole delivery as
a flow of stock host mechanisms — the ADR-0124 shape transforms plus the
ADR-0127 completion write:

```yaml
processes:
  - slug: dunning-escalation
    on: { new: dunning }            # the store walks each committed dunning
    steps:
      - { id: route, does: check }  # pass-through router — guards on its edges
      - id: enrich                  # ADR-0124 lookup: a governed read, merged onto the item
        does: check
        using: lookup
        config: { from: business_partner, key: partner_id,
                  select: { chat_id: telegram_chat_id }, on_missing: keep }
      - id: render                  # ADR-0124 template: the notice from the level — no prose
        does: check
        using: template
        config:
          into: text
          render: >-
            {{ if(level == 1, "Friendly reminder", if(level == 2, "Formal notice",
            "FINAL DEMAND (pre-legal)")) }}: invoice {{ invoice_id }} is overdue — …
      - id: send
        does: notify
        using: telegram-send
        on_done: { to: dunning, key: id, set: { status: '"sent"' } }  # ADR-0127
      - { id: judge, does: review, using: dunning-notifier }  # judgement + v1 fallback
    flow:
      - { from: route,  when: 'level > 3',        to: judge }
      - { from: route,  when: 'partner_id != ""', to: enrich }
      - { from: route,  otherwise: true,          to: "stop: no-debtor" }
      - { from: enrich, when: 'chat_id != ""',    to: render }
      - { from: enrich, otherwise: true,          to: "stop: no-recipient" }
      - { from: render, to: send }
      - { from: judge,  to: "done: judged" }
```

Everything here is the one `view` expression grammar — the same language as a
`when:` edge. A debtor without a chat id is an **audited stop**
(`stopped: no-recipient`), never a silent drop and never an invented recipient
(ADR-0098 D4). The `status: sent` write runs **after** the send succeeded
(ADR-0127 `on_done` — completion-coupled): a failed send writes nothing, and a
failed write after a successful send faults the run visibly ("the effect
completed, but the follow-up record write failed") instead of pretending.

## The `dunning-notifier` worker — judgement + fallback, still generic

**`workers/dunning-notifier.md`** remains the generic, domain-agnostic
**`ai-worker`** (ADR-0098 D1 — the platform ships NO `dunning` component; all
specificity is prompt + tool grant). Since the deterministic flow took the
routine notice, it has two jobs:

- **judgement** — the flow routes `level > 3` (outside the modeled 1–3 scale)
  to it: a genuine "look at it" case;
- **v1 fallback** — a runtime without the multi-stage reactive walk dispatches
  the entry binding's single worker for every new dunning (the compiler
  requires one), so the reaction never silently disappears. Its agentic recipe
  (find open → resolve → render → send → mark sent) is exactly the old
  behavior.

> **Why not a hand-coded `dunning-notifier` plugin?** Because that would bake a
> finance-domain worker into the agnostic platform. The right layering keeps the
> domain in the tribe (this model + the prompts) and the platform generic (the
> `ai-worker`, the ADR-0124/0127 host mechanisms, the `telegram-send` tool) —
> the deterministic variant proves it: generic, config-driven, never a
> `dunning`-specific component.

> **Delivery semantics:** the deterministic walk fires once per committed
> dunning write (record-granular, in-process) — **at-most-once** with the
> `on_done` mark after the send, replacing the worker's at-least-once
> scan-and-filter. The agentic fallback path keeps its own idempotency guard
> (`status == open` filter). A "sent but not recorded" fault leaves the record
> `open` — visible to the operator; a manual re-fire could then re-send, which
> is the deliberate, legible trade (ADR-0127 D3).

> **Lean builds:** the shape/store mechanisms ride the `data-ontology` feature.
> On a lean daemon the walk aborts audited at the `enrich` step (mechanism not
> in the catalog) — fail-closed, never a wrong send.

## Run + verify

```sh
# author a dunning (operator-gated) — fires the chain
wild tribe verb dun_invoice --tribe acme-liquidity \
  --input '{"invoice_id":"RE-2026-0032","partner_id":"BP-004","level":2,"amount":"7400.00"}'

# watch the governed send
wild watch --tribe acme-liquidity            # run rows: route → enrich → render → send
nats sub 'wild.acme-liquidity.data.dunning.*'  # the entity-change + the status=sent on_done write
```

A successful run shows: the run rows `route → enrich → render → send` (no LLM
call in the trace), one `audit.egress` row to `api.telegram.org` (no token in
it), the debtor's Telegram receiving the deterministic notice, and the `dunning`
record flipped to `status=sent` by the `on_done` write. A debtor with no
`telegram_chat_id` (e.g. `BP-003`) closes `stopped: no-recipient` with zero
egress; a dunning with no `partner_id` closes `stopped: no-debtor`.

Note: `wild tribe apply` also ingests `data/dunnings.csv`, and each seeded row
fires the process once (pre-existing behavior) — expect one walk per seed row
on a fresh apply.

## Status

Shipped + wired on the substrate:

- WS1 building blocks: the `path:` egress scheme (#2116) + the agnostic
  `telegram-send` tool-provider (#2118).
- ADR-0124 shape mechanisms (`lookup`, `template`) + the ADR-0127 governed
  write-back (`on_done`) — the deterministic delivery path this demo runs.
- B2b worker data facade: the agnostic Atlas read/write tools served by
  `NativeAtlasProvider` — the fallback/judgement worker's surface.
- Deployment wiring: `manifest.yaml` references the worker; the model's
  `dunning-escalation` process compiles to the multi-stage reactive flow the
  store walks per committed dunning (`wild tribe apply` deploys + arms it).
  The compile shape is pinned by `process_lane_compiles_the_liquidity_processes`
  (wild-ddd).

What remains for a LIVE send is operator infra, not code: a running daemon with
the tribe applied, the bot token stored, the egress destination granted, and a
real Telegram chat id on the debtor. No domain-specific platform component is
needed — that is the point.
