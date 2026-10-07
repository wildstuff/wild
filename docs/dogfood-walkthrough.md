# Dogfood walkthrough — operator's first tribe, end-to-end

A single, copy-paste-ready path from "fresh profile" to "a tribe
that gets better the longer it runs". One example case
(`apple-news-pulse`), one operator. Read it once; the next time
you start a real tribe you reproduce the steps.

The point of this doc is not the example. It is the loop:

> Elder reads the operator's pitch → pins a tribe + writes specs →
> activates → the chief cycles on a schedule → reflect emits
> insights → the operator accepts or rejects a `change_propose` →
> the tribe re-renders with the merged delta.

When that loop runs cleanly for a week, the tribe is observably
better than it was on day one — different headlines on the
filter list, sharpened divergence threshold, fewer false alerts.

> The canonical, Elder-readable version of this path — the steps and
> what to check at each one — lives in the
> `operator-manual.md` under *First Tribe*. That
> manual is the human rendering of the same corpus Elder uses to guide
> you.

## Prerequisites

- `wild` + `wild-hostd` built (`cargo run -p xtask -- dev` or the
  release binary at `~/.cache/cargo-target-the-wild/release/wild`).
- An LLM adapter configured for the `claude-cli-logic` / `chat` /
  `reasoning` strategies — `wild config llm list` should show at
  least one entry whose key resolves at boot.
- Claude Desktop (or another MCP client). The walkthrough talks
  to the daemon's MCP HTTP listener; stdio works the same way.

## Step 0 — Fresh profile

A fresh profile keeps the dogfood loop separate from your
historical test tribes. Existing profiles (`default`,
`feral-thicket`, …) stay untouched.

```sh
WILD_PROFILE=dogfood wild up --daemon --mcp-http 127.0.0.1:7531
```

First boot writes `<profile_root>/` with the embedded
default `bootstrap.yaml`, an empty `tribes/`, a generated
`mcp.token` under `system/`. Re-runs reuse what's on disk.

```sh
WILD_PROFILE=dogfood wild status        # control socket answering?
cat <profile_root>/system/token   # for the MCP client
```

## Step 1 — Operator pitch, Elder takes it from there

Open Claude Desktop (or `wild chat`) and say something close to:

> "Build me a tribe that polls the AAPL news feed every 10 minutes
> during US market hours, classifies each new headline as positive
> / neutral / negative, and pings me when 2+ negative headlines
> land inside a 30-minute window. Call it `apple-news-pulse`."

That single message is the entire operator surface. The Elder
should:

1. `tribe_search` — confirm no existing tribe matches the pitch.
2. `spec_propose` — write the first capability spec; Pin-on-First-Use
   creates `tribes/apple-news-pulse/{blueprint.md, meta.yaml,
   specs/headline-poll.md}`. Blueprint synthesises from the spec's
   `## Purpose` only (operator-prose), not the behavioural body —
   see ADR-0034 §3 amendment.
3. `brief_propose` — optional reasoning prose under `briefs/`
   (the "why this divergence threshold" context).
4. `tribe_update_shape` — trigger spec (recurring cron `*/10
   14-21 * * MON-FRI`), acceptance, cost, failure modes.
5. `tribe_activate` — flip to `active`, publish on the embedded
   deploy bridge, the chief boots.

Verify the Elder hit each step:

```sh
WILD_PROFILE=dogfood wild tribe list      # apple-news-pulse: active
WILD_PROFILE=dogfood wild tribe show apple-news-pulse
```

The `tribe_list` row should carry one `CapabilityOffer` with
`offer_type: event`, `topic: aapl-sentiment-tick` (or whatever the
Elder picked — naming is the Elder's), `subject:
wild.events.<topic>`. That's ADR-0041 Phase 2 working.

## Step 2 — Watch the cycle loop

Open the live console in a second terminal:

```sh
WILD_PROFILE=dogfood wild watch
```

Three panes:

- **Activity** — every `chief.cycle.completed`, every `worker.task`,
  every `chief_reply_to_user`.
- **Tribes** — `apple-news-pulse` row, its last heartbeat, its
  recent runs count.
- **Bus** — raw NATS subjects under `wild.apple-news-pulse.>`.

Wait for the first scheduled fire (10-minute boundary). You
should see:

```
wild.apple-news-pulse.chief.trigger.schedule
wild.apple-news-pulse.chief.cycle.started
wild.apple-news-pulse.worker.http-fetcher.task        # web.fetch
wild.apple-news-pulse.worker.ai-worker.task           # llm.classify
wild.apple-news-pulse.chief.cycle.completed
```

Then re-check from outside the console:

```sh
WILD_PROFILE=dogfood wild loops ls --json | jq .
WILD_PROFILE=dogfood wild traces ls
```

`wild loops ls` shows per-turn telemetry (latency, tokens,
tool_calls). `wild traces ls` shows the chief's reflect output
once the reflect cycle has fired (default cadence: every Nth
cycle, see `prompts/chief-default/reflect.md`).

## Step 3 — Reflect emits insights

After a few hours of cycles (with the 10-minute cadence: ~30
cycles), the chief's reflect mode kicks in. The spec-pinned
reflect (`PROMPT_BASE_REFLECT_SPEC`) surfaces a single tool:
`change_propose`. Watch for it on the bus:

```
wild.apple-news-pulse.change.proposed
```

Inspect the proposal:

```sh
WILD_PROFILE=dogfood wild change show apple-news-pulse <change-id>
# or via MCP:
#   change.show(tribe_slug=apple-news-pulse, change_id=<id>)
```

The proposal carries a structured delta — added / modified /
removed `### Requirement:` blocks — and a `risk` tag (`low`,
`medium`, `high`).

## Step 4 — Accept or reject

Decision time. The chief will not auto-merge a `high`-risk change;
`low`-risk changes auto-accept after a 24h grace per ADR-0034 §4.5.
For everything in between, the operator decides:

```sh
WILD_PROFILE=dogfood wild change accept apple-news-pulse <change-id>
# or  wild change reject apple-news-pulse <change-id>
```

Accept re-renders the tribe, publishes on the deploy bridge, the
chief picks up the new requirements on the next cycle. ADR-0041
Phase 2 also re-derives offers/needs from the merged frontmatter
so the registry stays in sync.

## Step 5 — "Better the longer it runs"

The Day-1 → Day-7 contract:

| Time | What should be observable |
|---|---|
| Day 1 | First cycle completed, first reflect insight. |
| Day 2-3 | First `change_propose` from the chief. |
| Day 4-7 | At least one accepted change has reshaped the requirements. The new shape is visible in `tribes/apple-news-pulse/specs/headline-poll.md` (added requirement, tightened scenario, etc.). |
| Day 7+ | The operator's manual interventions per day trend down. False alerts trend down. |

If the loop doesn't produce these signals, that's the failure
mode worth reporting — not a build error, but a behavioural one.
File against ADR-0034 (the authoring shape) or the chief's
reflect prompt (`prompts/chief-default/reflect.md`).

## Step 6 — Analytics ask, promote, and Herkunft check on `liquidity-management-ddd`

Steps 0-5 run the reflect → change-propose loop on an event tribe. This step
switches to the DDD reference tribe and exercises the accountant-persona
probe from ADR-0138 + ADR-0139 end-to-end: ask Elder for a grouped analysis,
promote the ephemeral answer into a durable projection, then read the
provenance of the resulting node.

### 6.1 — Activate the DDD reference tribe

Apply the `liquidity-management` example as a DDD-stamped tribe named
`liquidity-management-ddd`:

```sh
WILD_PROFILE=dogfood wild tribe apply examples/tribes/liquidity-management/ --as liquidity-management-ddd
```

One command parses `ontology/model.yaml`, compiles it, pins the ontology,
ingests the seed data, and schedules the live chief. Confirm it is active:

```sh
WILD_PROFILE=dogfood wild tribe list
WILD_PROFILE=dogfood wild tribe status --id liquidity-management-ddd
```

### 6.2 — The analytics ask (ephemeral grouped aggregation)

Open root Elder chat (`wild chat`, not scoped to a tribe) and ask the
canonical accountant question:

> "In liquidity-management-ddd, how much input tax — Vorsteuer — do I owe
> this quarter, grouped by supplier?"

Elder switches to Operate mode, reads the tribe's model (`atlas_describe`),
finds that `invoice` carries a computed `vat_amount` and a `direction`
(inbound/outbound), and calls `object_set_aggregate` over the `invoice` type
with `where: direction = inbound` and `group_by: partner_id`. The reply is a
grouped table — an ephemeral answer, no model change, and nothing declared:
asking a question mints no ontology.

You should see a table similar to:

| supplier | input tax (EUR) |
|---|---|
| druckerei | 123.45 |
| logistik | 67.89 |
| techparts | 234.56 |

The exact numbers depend on the seeded inbound invoices.

### 6.3 — The promote step (persistent projection)

If the table is what you want, tell Elder to keep it:

> "Keep this as a permanent report."

Elder promotes the **verbatim** grouping spec that produced the table into a
`projection_declare` call. The new projection — named
`vorsteuer_by_supplier` or a slug Elder confirms with you — is grouped by
`partner_id`, filtered to `direction == inbound`, and sums `vat_amount`. The
spec is pinned live and folded back into `ontology/model.yaml` via
`wild_ddd::reconcile`, so it survives the next `wild tribe apply`.

Verify the projection landed:

```sh
# The model source of record now contains the new projection.
grep -A 20 "vorsteuer_by_supplier" \
  <profile_root>/tribes/liquidity-management-ddd/ontology/model.yaml

# A Domain Decision Record was minted at the ratified seam.
ls <profile_root>/tribes/liquidity-management-ddd/evolution/
```

### 6.4 — The Herkunft check (provenance)

"Herkunft" is the user-facing German label for the provenance section on the
dashboard node detail. Make sure the daemon is running, then open the native
dashboard:

```sh
WILD_PROFILE=dogfood wild up --daemon
# In another terminal:
cargo run -p xtask -- dashboard desktop
```

Navigate to the Atlas view for `liquidity-management-ddd`, select the
`vorsteuer_by_supplier` projection node, and scroll to the **HERKUNFT**
section. It shows the Domain Decision Record: when the projection was added,
who ratified it, and the verbatim operator request ("Keep this as a permanent
report.").

You can also ask the per-tribe Domain-Elder directly:

```sh
WILD_PROFILE=dogfood wild chat --tribe liquidity-management-ddd
```

> "Where does the vorsteuer_by_supplier projection come from?"

It reads the same evolution log and narrates the provenance back to you.

This closes the loop: discover (ask) · run (aggregate) · understand (table) ·
trace (Herkunft) — in the operator's own vocabulary.

## Step 7 — Ask for a capability the tribe does not have

Steps 1–6 all run on what is already installed. This step is the other road:
the operator wants something no component provides, and says so in their own
words. Nothing here needs a component name, a capability tag, or a spec.

Run it against a tribe with an authored domain model — the reference tribe
from step 6 will do — and check that a build service is reachable first:

```sh
WILD_PROFILE=dogfood wild doctor | grep "component build service"
```

Then ask, as an accountant would:

```sh
WILD_PROFILE=dogfood wild chat ask --tribe liquidity-management-ddd \
  "Wir brauchen einen Baustein, der eine Mahnstufen-Übersicht der offenen
   Rechnungen zusammenstellt. Bitte beauftrage den Bau."
```

The Elder answers with what would be built and asks for a yes. **Nothing is
built by that turn** — it stages a commission and waits for a person
(ADR-0318: only an expressed wish mints an order).

Answer on the same session, using the id the reply printed:

```sh
WILD_PROFILE=dogfood wild chat ask --tribe liquidity-management-ddd \
  --session SES-… "ja"
```

`ja` must be the whole answer. A sentence with more in it does not count, and
the reply says so — the decision then keeps waiting rather than being released
by something that only looks like consent.

### What to look at, and what it proves

The commission is a file, not a chat message. Read it:

```sh
ls <profile_root>/decisions/$(date +%Y-%m) | tail -1
```

The row's `brief` carries the operator's own sentence as its **first line** —
that line is what the decision queue renders — followed by a section headed
`## Domain context (orientation, not requirements)`. That section is written by
the host from the tribe's authored model (ADR-0320 D1), so the build is told
the domain's real words: `invoice` rather than a synonym, and the operator's
own labels. When the request names one of the tribe's types, the section is
that type with its fields; when it names none, it is the roster of every type
the model holds.

Once released, the chief writes the build brief to
`tribes/<slug>/briefs/<component>.md` and the build runs for a few minutes.
The component then arrives as a row awaiting approval — nothing installs
itself, and the approval sentence names what was checked.

> **Known gap, measured 2026-09-06:** the domain-context section reaches the
> decision row intact and is then rewritten on its way to the brief file — see
> issue #7441. Read the row, not the file, when you want to see what the host
> composed.

## Inspecting state from the operator side

| Question | Command |
|---|---|
| What tribes do I have? | `wild tribe list` |
| Is this one alive? | `wild tribe status --id <slug>` |
| What did the chief do last hour? | `wild loops ls --tribe <slug> --since 1h` |
| What patterns did reflect emit? | `wild traces ls --tribe <slug>` |
| What changes are pending? | `wild change show <slug>` |
| Why is it not firing? | the daemon log under `<profile>/system/logs/wild-hostd.log.YYYY-MM-DD` |

## Resetting between dogfood runs

Killing the daemon and re-creating the profile is cleaner than
trying to surgically archive tribes:

```sh
WILD_PROFILE=dogfood wild down
rm -rf <profile_root>
WILD_PROFILE=dogfood wild up --daemon          # fresh start
```

Note: this drops NATS-bucket state for the dogfood profile only;
`feral-thicket` and `default` stay untouched.

## Where the scripts fit

`scripts/elder-eval/README.md` lists the five smoke scripts and
their hypotheses. The closest scripted proxy of this walkthrough
is `smoke-recurring.sh` — single Elder-driven message, full setup,
deterministic verification. Run it after touching Elder Intake or
the recurring-execution path to confirm the operator's first-tribe
flow still works.

This walkthrough is the human variant of that script: same loop,
operator-driven, slow enough to let the reflect → change-propose
emergence happen.
