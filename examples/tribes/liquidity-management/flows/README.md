# Operator flows (ADR-0118)

Multi-stage flows exist in TWO lanes. The CONSTITUTION lane: the ddd
compiler's `processes:` section (ADR-0118 D14) lowers "when X → do Y" rules
to `origin: model` flow records — this example's model compiles several
multi-stage flows that way (`dunning-escalation`, `invoice-screening`,
`invoice-dual-check`, and the `invoices-inbound` source takeover with its
ADR-0128 gather). The OPERATOR lane, which this folder feeds: a flow grown at
RUNTIME over MCP with `flow_declare`, exactly as an operator would in Elder
chat — a sovereign `origin: operator` record the model does not own. This
folder replays that authoring so the example's operator-grown flows are
reproducible — and so the dashboard's **Flow card** (the branching-diagram
view) has something rich to draw beyond the compiled ones.

## `invoice-triage.json`

The canonical branching shape (the one the Flow card was designed against):

```
Invoice   ─▶  1 ingest ─▶ 2 normalize ─▶ ? assess (AI · urgency)
                                              │
              ┌───────────────┬──────────────┴───────────────┐
        urgency==overdue   urgency==due_soon             otherwise
              │                 │                            │
            chase (→notify)   remind                       file
              │                 │                            │
           escalate            │                            │
              └───────────────┴──────────────┬──────────────┘
                                          3 record  (⋀ merge)
                                              │
                                         End: triaged
```

- An **exclusive-port** decision (`assess`) fans out three conditioned edges;
  the mandatory `default` edge is the "otherwise → file" branch.
- The `overdue` lane is two steps deep (`chase → escalate`), showing lane depth.
- All three lanes reconverge into `record`, the numbered **merge** step.

## `notice-dispatch.json` — a *flow effect*

A short flow whose job is an **outward action**: `review` (agentic, checks the
dunning level) → on `level >= 2` → `deliver` (a `sink-effect`, `telegram-send`);
otherwise → `terminal:abort(logged only)`. Because it ends in a
`sink-effect`, the atlas colours its node **amber like an Effect** — you can tell
at a glance it *does something outward*, versus the pink processing flows.

## `invoice-collate.json` — a *gather* (ADR-0128 join)

The join/barrier shape: one invoice **fans out** (a broadcast port) into two
independent checks that run in parallel, then **gather** at one node before the
record is written.

```
Invoice   ─▶  1 normalize ─▶  ◇ fan  (broadcast)
                                 ├──────────────┬───────────────┐
                              urgency      cash-discount
                             (urgency)  (cash-discount window)
                                 └──────────────┴───────────────┘
                                        ⋀ 2 collate  (gather · collect)
                                              │
                                        3 record  ─▶  End: collated
```

- `fan` is a **broadcast** port — it fires BOTH branches (unlike the exclusive
  port in `invoice-triage`, where only one branch wins).
- `collate` is a `does: gather` step (`archetype: join`, `mechanism: collect`):
  the two branch findings arrive via explicit `join:collate` edges and fold into
  one `checks` array (ADR-0128 D5 — an arrival names its gather explicitly).
- On the atlas Flow card the gather renders as the numbered **⋀ merge** step,
  then the walk continues to `record` and the `collated` terminal.

## Colour & edge legend (atlas)

A flow is coloured by what it fundamentally **is**, not as a generic pipeline:

| Flow character | Colour | Example |
|---|---|---|
| **Connector / pull** (an intake) | grey — like a Source | `invoices-inbound` |
| **Has a `sink-effect`** (acts outward) | amber — like an Effect | `notice-dispatch` |
| **Pure processing / decision** | the Flow colour (pink) | `invoice-triage`, `dunning-escalation` |

Reactive **trigger** edges (`Type ──triggers──▶ Flow`) are drawn **gold &
dotted with a `triggers` label**, so a "when this data changes" edge reads
differently from a data/produces edge.

## Apply

```bash
MCP=http://127.0.0.1:<mcp_port> \
TOKEN=$(cat <profile_root>/system/token) \
  ./apply-flows.sh
```

Then open the tribe in the dashboard and click the `invoice-triage` flow node
(Domain or Intake layer) to see the Flow card.
