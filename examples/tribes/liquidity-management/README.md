# Liquidity Management — DDD Reference Tribe

The **domain-model-first** reference example (ADR-0108). The liquidity
domain (§17 InsO, 13-week forecast, gated payment + dunning verbs),
authored the DDD way: one `ontology/model.yaml` is the tribe's **constitution**,
and `wild tribe apply` *compiles* it — deterministically, no translation — into
the full ontology.

> **This is the canonical authoring example.** If you are building a new tribe
> today, start here.

## Apply it

```bash
wild tribe apply examples/tribes/liquidity-management/ --as acme-liquidity
```

One command: parse + gate the model, compile it, pin the ontology, ingest the
seed data, register the recurring feeds, stamp the tribe `ddd`, and schedule a
live chief. Then talk to it:

```bash
wild --profile <name> elder      # > "Which debtors are overdue?"
```

### The sample data has a fixed "today"

The seed is cut around **2026-07-31**: four receivables already overdue (the
dunning chain works on those), the rest falling due across the following
thirteen weeks, and bank movements running to two days before. That shape is
what makes the demo readable — a forecast with something in it, an overdue
bucket that is not everything.

Behind that tail sit **two years of settled history** — ~330 receivables,
~330 payables and the fixed costs a business carries in between. They exist
for the Procedure: a learned forecast has nothing to be judged against
without a record of what actually came in, and a single quarter cannot show
a seasonal claim at all. Every generated invoice is already **paid**, so the
open book — the 135.800,00 EUR the declared measure reports, the overdue
four, the dunning chain — is exactly the hand-authored tail it always was.

The history is not typed either: each week's settlements are drawn from the
same generative law the Procedure's training extract was drawn from,
imported from
[`fit.py`](../../tool-providers/cash-forecast/training/fit.py) rather than
restated. That is what makes the two curves comparable — over the seeded
book the model tracks the actuals to about **3,4 % MAPE**, against the
3,8 % its audit card claims on its own holdout. Two copies of the law would
drift apart with both charts still rendering.

It is also perishable. Run this bundle a year later and every open item is
overdue, the forecast window is empty, and the aging tick has nothing left to
demonstrate. Re-cut it: move `TODAY` in
[`data/regenerate-seed.py`](./data/regenerate-seed.py) to the current date and
run it. That is the whole chore — the report's forecast window anchors on
`today` and follows the clock by itself, and the noise is seeded per absolute
week, so a re-cut leaves every week that stays in the window untouched.

You will not have to remember: `example_seed_freshness` (in `wild-ddd`) reads
these CSVs against the real clock and reddens once too few open invoices fall
inside the next 13 weeks, once the overdue set stops being a few and starts
being the whole book, or once the Procedure's curve stops looking forward.

Edit the CSVs by hand and you inherit the drift the generator exists to
prevent — the previous cut had payments dated before their invoice, three
debtors paying someone else's bill, and a balance that tied to nothing.

## What makes it the DDD lane

The switch is one line in `manifest.yaml`:

```yaml
authoring_method: ddd
```

At that key, `wild tribe apply` routes through the domain-model compiler instead
of the hand-authored loader:

1. **Parse + gate up front** — `compile_ddd_source(ontology/model.yaml)` parses
   the `DomainModelDoc` and runs the cross-layer gate (every source
   `target_type` must name a declared aggregate) **before any side effect**. A
   broken model fails loudly with zero partial state.
2. **Compile, don't translate** — the model lowers mechanically to the full
   `DataCommand` stream (types + value objects + functions + verbs + sources).
   An LLM (or you) authors the model; the transform is deterministic.
3. **Persist + stamp** — the model is written as the tribe's source of record
   (`tribes/<slug>/ontology/model.yaml`) and the tribe is stamped `ddd` in its
   `meta.yaml`. From then on the tribe is a *living* DDD tribe: model changes go
   through the `ddd_declare_*` chat tools (fold → gate → compile → persist), not
   hand-edits.
4. **Strip the hand-authored ontology** — the model owns types · value objects ·
   functions · verbs · sources · intake, so any stray hand-authored ontology
   file is ignored (this bundle ships none).

## The model — `ontology/model.yaml`

The single constitution, in two layers (ADR-0108 §3):

| Layer | Section | Lowers to |
|---|---|---|
| **A — Domain** | `value_objects`, `value_sets` | `money_amount` (EUR), `iban` (mod-97), `partner_role` (closed set) |
| | `functions` | the reusable pure expressions (`@vat_amount`, `@cash_discount_amount`, …) |
| | `aggregates` | the type vocabulary — mirror/state/derived/enriched fields, relations |
| | `aggregates[].lifecycle` | the closed `status` enum **and** each command's `requires_state` guard, from ONE source (the transitions) |
| | `aggregates[].commands` | the gated domain verbs (risk tiers) + per-verb `requires_state` preconditions (object-state guards, enforced by the gate, advertised on `describe`) |
| | `projections` | the CQRS `debtor` read-model (grouped over `invoice`) |
| **B — ACL** | `sources` | the recurring `intake_source` bindings + column→field `intake_config` |

Presentation and template marks (icons, colours, categories, `title_key`,
scaffold hints, field descriptions, `sensitive`/`customer_visible`) are authored
intent the compiler cannot infer — they ride explicitly on the model and land on
the compiled types.

## Settled and late are two facts

An invoice carries two verdicts of its own, and keeping them apart is
deliberate:

| field | answers | members | who writes it |
|---|---|---|---|
| **`settlement`** | **is it settled?** | `open` · `paid` · `written_off` | derived — read THIS one |
| `status` | what the feed says | `open` · `paid` · `cancelled` | the accounting export |
| `effective_status` | did the tribe override the feed? | `from_source` · `paid` · `written_off` | the tribe's verbs |
| `aging_state` | is it late? | `current` · `overdue` | the daily aging rule |

`settlement: open` means **still owed, past-due included** — which is what an
operator means by "offene Forderungen". Every measure and every grouped read
binds `settlement`, never one of its two sources: the feed alone misses a
write-off the tribe decided, and the override alone reads `from_source` on
everything nobody has touched.

Two corrections got the model here, both found by measuring rather than
reasoning, and both worth knowing before editing this file:

**The axes were one enum.** `effective_status` used to carry an `overdue`
member, so ageing moved a receivable OUT of `open` — every reading of the field
was correct while the label lied, and a grouped read returned an `open` bucket
that silently excluded exactly the invoices that need action.

**The override claimed a state it was never given.** Its initial member was
`open`. An ingest may never write an authored field, so the fold seeds the first
member on *every* imported invoice — and that seeded `open` then beat the feed's
own `paid`. Naming it `from_source` is what makes the field honest: it says the
tribe passed, not that the invoice is unpaid.

Together these produced the wrong total about one turn in five against a live
Elder (`open-receivables-is-the-declared-measure`); grouping outbound invoices
by `settlement` now returns `{open: 13, paid: 5}` summing to the same
135.800,00 EUR the declared measure reports.

Two consequences worth knowing:

- A measure that wants the LATE unsettled ones names **both** axes — a
  `filter:` list ANDs. `aging_state` alone would keep counting an invoice that
  was paid late.
- Settling does **not** reset `aging_state`, on purpose: "which invoices were
  paid late?" is a question the merged enum destroyed and this one answers.

Upgrading an already-deployed copy: `effective_status` retired both `overdue`
and `open`, so old rows keep values the schema no longer declares. This example
ships no migration — re-apply onto a fresh profile.

## Conditional effects — `requires_state` on the invoice verbs

The invoice verbs demonstrate **conditional effects**: a verb only fires when
the invoice's CURRENT record satisfies a guard. The guard is declared on the
verb (`requires_state: {field: predicate}`), enforced **centrally** by the
effect gate before the handler runs, and advertised on `describe(invoice)` —
so the condition lives in the API layer, not inside the effect (which stays
reusable across record shapes):

| Verb | Fires only when | Refused when |
|---|---|---|
| `take_cash_discount` | `direction == inbound` **and** `status == open` | a receivable (outbound) or an already-settled invoice |
| `reconcile_transaction` | `status != paid` | the invoice is already settled (no double-settle) |
| `write_off_invoice` | `status != paid` | the invoice is already settled |
| `verify_invoice` | *(any state)* | — guards are opt-in, not blanket |

The bounded predicate DSL carries one predicate per field (`==`/`!=`/`nonempty`
/ numeric); "still owed" reads honestly as `!= paid`. Create verbs
(`dun_invoice`, `schedule_payment`) mint a new subject, so `requires_state`
cannot read the *invoice* there — their conditions live with the caller (the
`receivables-overdue` flow reacting to the daily aging write, or the chief's
judged backstop) and on the risk gate, not on the verb.

The `payment` lifecycle guards (`approve_payment`: planned → approved;
`pay_invoice`: approved → executed) need a record in the state they demand, so
`data/payments.csv` seeds all three: two `executed` payments matching the
settled payables and their bank debits, one `approved` awaiting execution, two
`planned`. A verb whose only demonstrable behaviour is refusing on an empty set
teaches the refusal, not the verb.

## ADR-0118 flows — the `processes:` section

> ADR-0118 compiled flow records
> (the engine substrate — the subject graph with stages, ports, terminals) are
> surfaced as **flows** at the authoring and operator layer — the word the
> `flow_declare` / `flow_feed` verbs and the dashboard use. You author a
> `process`; it compiles to a flow. This section speaks *flow*.

The model's `sources:` compile to *length-1* flows. The model's
**`processes:`** section (ADR-0118 **D14**) compiles each constitutional
"when X → do Y" rule to a flow — a reactive (`new`/`change`) process to a
length-1 reactive flow the store fires on the write, a `source`/`handoff`
process to a multi-stage flow. The seven processes here exercise all four
trigger kinds, the D10 branching flow, the ADR-0126 Phase 3 document
connector, AND two ADR-0128 Join/Barrier gathers (one per lane: the connector
batch-collapse and a reactive fan-out fold), authored the DDD way
(`ddd_declare_process`, or hand-written in `model.yaml`) and lowered
deterministically by the compiler, not scripted at runtime:

- **intake, gathered (connector lane)** — `invoices-inbound` extends its own
  folder feed (`on: {source: …}`, the D14 takeover: process slug == source
  slug, so the multi-stage record replaces the length-1 ingest fold). After
  the auto `ingest` step, every partial run of one pull arrives at an
  ADR-0128 **`gather`** barrier (`join:gather` edges) and ONE
  `notify-operator` notice goes out per pull — N per-file runs fold to one
  with fan-out on (`WILD_CONNECTOR_FANOUT_MAX > 0`), the single batch run
  folds alone on the sequential path; the notice body renders the barrier's
  own `arrived/expected` stamp either way.
- **intake, extracted + enriched (document connector)** — `invoices-inbound-pdf`
  (ADR-0126 **Phase 3**) is the same D14 takeover, but over a DOCUMENT source
  that accepts both PDF and DOCX scans from `documents/inbound-invoices`:
  3 PDF samples and 5 DOCX samples ship with the tribe.
  An `extractor:` block makes the source fold `[ingest, extract,
  ingest-candidates]`, and the same-slug `source` process inserts an ENRICH
  step, so each scanned file walks its own run `ingest → extract → enrich →
  ingest-candidates`. The `extract` step is the built-in agentic `extract-worker`
  (one candidate invoice per file); the `enrich` step is a deterministic
  `lookup` that merges the creditor's payment terms from the partner master
  (zero LLM turns); the candidate is then admitted through the confirm gate.
  The operator authors the per-connector intent; the extraction complexity
  stays under the waterline.

  ADR-0089 full-text indexing is turned on for this source:
  `index_corpora: [documents]` feeds every PDF/DOCX into the search corpus, and
  `index_options.per_page: true` makes the `search-indexer` emit one corpus entry
  per page rather than one per file. That lets an operator search inside
  multi-page scans and land on the exact page — e.g. "show me invoices
  mentioning 'Net 14' on page 2".
- **external data, two acquisition shapes (fetch vs. intake)** — a debtor's
  external creditworthiness (`credit_assessment`) is acquired two ways, side by
  side, to make the point honest: `partner-credit-lookup` (credit inquiry) is
  the CLEAN structured read (`does: fetch` a cross-tribe `pipeline:credit-bureau/
  partner-credit-check` offer), but that only works when the bureau is itself a
  Wild tribe — the EXCEPTION. `partner-credit-report-pdf` (credit report) is
  the REALISTIC path: a Schufa/Creditreform result arrives as a PDF or DOCX and
  the rating is pulled out of it — a DOCUMENT source whose `extractor:` concretely
  lands the `credit_assessment` datum through the confirm gate (the fetch is the
  clean-case alternative that would supply the same datum). Document intake, not
  a fetch, is where operators actually spend their effort (ADR-0176 Context). No
  sample credit-report documents ship (a real one is a third-party document); the
  source is declared and armed, empty until one lands. It uses the same ADR-0089
  full-text indexing as the invoice scan, so any incoming report becomes
  searchable per page.
- **screening, branching (reactive lane)** — `invoice-screening`
  (`on: {new: invoice}`) is the record-granular branching flow: each FILED
  invoice walks its own run (ADR-0118 PR-12), so a deterministic `check`
  step (`screen`, no worker) routes on the invoice's OWN fields over an
  **exclusive port** (D10, first match wins): a cancelled re-ingest halts at
  a `stop:` terminal; a cash-discount-window-open payable is flagged
  (`done: flagged-cash-discount`); everything else is filed straight away — all
  deterministic, no worker spent on routing. (No overdue edge: an arriving
  invoice's source status is never `overdue` — aging is time, and time
  enters as the daily aging WRITE; ADR-0156 OQ5 / ADR-0157.) The two lanes
  are the deliberate granularity split: per-PULL/per-FILE work on the
  source process, per-RECORD routing on the reactive process.
- **worker (payables→receivables loop)** — the chase is TRIGGERED since
  ADR-0157: the invoice's declared `aging:` rule makes the daily host tick
  write `aging_state: overdue` on each past-due open receivable (a
  governed, actor-stamped write), and the `receivables-overdue` process
  (`on: {change: {aggregate: invoice, field: aging_state, becomes:
  overdue}}`) dispatches `receivables-chaser` (one agentic `review` step)
  on that write. A `change:` trigger names an aggregate and a field, never a
  predicate on the record — and `invoice` is BOTH sides of the book, aged on
  both — so the process opens with a deterministic `check` port that routes
  on `direction`: a receivable goes to the chase, an overdue PAYABLE (our own
  late payment — a real fact, and none of this process's business) halts at
  `stop: not-a-receivable`. On the shipped seed that is 5 chased and 4 halted,
  and the 4 cost no LLM turn. The blueprint's `receivables-chased` judge
  (ADR-0049) stays as the agentic BACKSTOP over this deterministic lane. The new
  dunning then fires `dunning-escalation`
  (`on: {new: dunning}`) — since the ADR-0124 proof point a **deterministic
  multi-stage flow**: `lookup` merges the debtor's `telegram_chat_id` onto the
  dunning, `template` renders the notice from the level (no LLM prose),
  `telegram-send` delivers it, and the ADR-0127 `on_done` write records
  `status: sent` — zero LLM turns for the routine notice. A missing debtor or
  chat id stops audited (`no-debtor` / `no-recipient`); only an out-of-scale
  `level > 3` reaches the agentic `dunning-notifier` (which is also the v1
  fallback on a runtime without the multi-stage walk). The store fires both
  processes on the write (replacing the old `on-upsert: dunning` spec).
- **logic-first decision (ADR-0144 dogfood)** — `invoice-dual-check`
  (`on: {new: invoice}`) decides an invoice's validity from its OWN fields with
  **zero workers and zero LLM turns**: a deterministic `map` step (`validate`)
  adds `reference_ok` / `amount_ok`, then a `does: decide` step (`triage`) runs
  an ordered rule table over those fields and stamps a top-level `resolution`
  the edges route on — a bad reference or amount halts at `stop:
  invalid-invoice`, a clean invoice folds to `done: checked`. The routing/triage
  decision that used to be wired to an agentic worker is a rule over fields, so
  it is logic; the flow arms with no deployed component (the `decision` host
  built-in resolves directly at D16). (The ADR-0128 join/barrier fold is shown
  by `invoices-inbound`'s `does: gather` batch-collapse.)
- **effect** — `dunning-notice`: `on: {handoff: …}` → a length-1 `Sink-Effect`
  over the Telegram sender, offered to a partner tribe with a cross-tribe Grant.

These are Layer B of the one constitution — read them in
[`ontology/model.yaml`](./ontology/model.yaml) under `processes:`. `wild tribe
apply` compiles them alongside the ontology; the compiled records carry
`origin: model` (the reconciler owns them, the same as the source folds).

## ADR-0185 flows — the `flows:` section

> ADR-0185 adds a second Layer-B
> authoring lane for flows. A `processes:` entry (above) uses the `does/using/
> steps` sugar — a business rule that *reacts* to a domain event. A **`flows:`**
> entry is a `FlowDecl` written **directly** — the exact shape an operator grows
> in Elder chat with `flow_declare` / `flow_edit`, or edits in the dashboard Flow
> editor. Both compile to `origin: model` pipeline records and share the
> pipeline-record slug namespace (a collision fails the gate). `wild flows ls`
> lists the flows authored in THIS section — not the ones compiled from
> `sources:` / `processes:`, which are flows too but are not what an operator
> means when they ask what they wrote. In this bundle that is 3 rows out of 18
> pipeline records; the dashboard draws each as a Flow node with a "▸ n steps"
> badge. (`wild flows ls <tribe>` takes the tribe as a positional, not
> `--tribe`.)

The three flows here exercise the ADR-0185 **D3 presets** end to end:

- **`payable-cash-discount-review`** — *Decide → Act*. `map` (compute the discount) →
  `decision` (an exclusive-port rule table: is taking the cash discount worthwhile?) →
  `notify-operator` only when it is; otherwise an audited `terminal:abort`. Zero
  workers, zero LLM.
- **`sepa-payment-export`** — the ADR-0184 **`invoke-tool`** envelope. A
  deterministic Transform addresses an external `sepa_xml_export` tool by name,
  `args` validated against the tool's *own* schema (opaque to the core; an
  unknown tool aborts audited at run time, never at compile), then notifies the
  operator that the file is ready to sign.
- **`multi-invoice-scan-split`** — the full *Capture → Extract → Confirm → File*
  chain: `render-document` → `detect-document-boundaries` → a `join`/`collect`
  gather → an agentic `extract-worker` → a `confirm` pause (human in the loop) →
  a `store` sink-data. The manual sibling of the `invoices-inbound-pdf`
  connector takeover, but with an explicit operator confirm before filing.

## Demo status — what's live vs. stubbed

This example teaches the *shapes*; a few stages reach for a real integration the
example does not ship, so they are **demo stubs**. Each stub carries a `note:`
that the dashboard renders as an amber "⚠ Stub: …" line **under the flow node**,
so an operator reads what *would* happen instead of guessing at a dead end. What
each stub needs to become live:

| Flow / door | Stub stage | Needs to run for real |
|---|---|---|
| `partner-credit-lookup` (credit inquiry) | `assess` — cross-tribe `fetch` | a deployed `credit-bureau` tribe exposing the `partner-credit-check` offer. The realistic alternative — `partner-credit-report-pdf` — is live once a PDF lands. |
| `partner-credit-report-pdf` (credit report) | the document extractor | a credit-report PDF in `documents/credit-reports` (none ship — a real one is third-party) + an LLM for extraction. |
| `dunning-escalation` / `dunning-notice` | `send` — `telegram-send` | a Telegram bot token + egress binding (see `manifest.yaml`). Without it the send is a no-op in the run rows. |
| `dunning-escalation` / `receivables-overdue` | `judge` / `chase` — agentic workers | an LLM model pinned (`workers[i].model` or `manifest.default_model`; `wild tribe apply` warns when none is set). |
| `sepa-payment-export` | `build-file` — `invoke-tool sepa_xml_export` | a tool provider that ships the `sepa_xml_export` tool. Until then the stage aborts audited at run time. |

Everything else — the CSV intakes, the deterministic `map`/`template`/`lookup`/
`decision`/`store`/`gather` stages, the aging tick, the confirm gate — runs with
no external dependency.

## The operator apps — `apps/`

Two published end-user apps (ADR-0154), because they answer different
questions. `wild tribe apply` installs both into the profile's app plane
(`<profile>/apps/`) and stamps each copy with the tribe you applied as.

Neither spec in the BUNDLE declares a `default_tribe` — absent, a view binds
the carrier, so the bundle stays portable under `--as <any-id>`. The installed
copy does carry it, because a profile-level app sits under no tribe and would
otherwise bind nothing. Re-applying refreshes an app you have not touched and
keeps one you have edited (saying so either way).

**`liquidity-report.yaml` — the accountant's page.** Cash on hand, how many
invoices are open, how much is already overdue, what is still expected in —
then the next 13 weeks in and out, and the accounts and bank movements those
figures fold from. One page, no schema tour. Start here when you want to see
what the tribe *knows*.

The "expected in" tile carries an ADR-0189 D7 **sparkline**: the same measure
folded into 13 weekly buckets along `due_date`. It is the only tile on the page
that can, and that is an ontology fact — a trend buckets its measure over a date
field OF THE SAME PROJECTION, and of the four read-models the report binds only
`cash_forecast` has one. Read the halves apart: the figure sums every row of the
projection, the sparkline shows only the window.

Its 13-week curve is the ADR-0154 PR3d **`window:`** block: the chart's read
becomes a `TimeBucket` fold over 13 consecutive 7-day windows along the
invoices' `due_date`. `window.start` is `today` — an INTENT, not a date: the
renderer stays clock-free and the HOST resolves the token where the read is
served (`wild_core::window_start`), so the window is always the next 13 weeks
and never needs re-cutting. An absolute ISO date still works, and is the right
choice when a window names a real period (a fiscal year, an audit range).

**`liquidity-cockpit.yaml` — the working surface, and the schema showcase.** It
exercises **eight of the ten** built-in view kinds — `kpi-tile`, `chart`,
`table`, `detail-card` (with the D11 change-history trail + D16 cross-view
record wiring), `effect-form`, `chat-panel`, plus the ADR-0175 G13 `search` and
`timeline`/kanban. The two it does not: `diagram` (ADR-0189 D8 — authored
structure, bound to no data) and `custom` (ADR-0161 — needs a widget plugin the
`WidgetCatalog` does not carry here). It also shows the ADR-0175 feature
surface: a responsive **grid layout** (G9), **audience** gates (G6),
**onboarding** / **empty-state** cards (G14), design-token **branding** (G11),
and the time-driven **gantt** / **calendar** table representations (D11).

Its two debtor charts are deliberately two, not one two-measure chart: the read
plan folds `y[0]` and only `y[0]`, and the renderer draws a single series — a
second measure parses and then never reaches a pixel. Open exposure and the
overdue slice therefore get a chart each, in their own ADR-0209 S1 palette
token. The cockpit also takes the `compact` density preset where the report
stays `normal` and the fitness sample's client app takes `comfortable`: one
preset per surface, each multiplying the same spacing ladder.

Validate either with `wild app validate apps/<name>.yaml`. Both are also
checked in CI: `example_bundle_apps_bind_to_their_compiled_model`
(`wild-mcp-tools`) compiles this bundle's `model.yaml` and runs every app spec
in `apps/` through the publish-strength gate, so a view naming a projection,
measure or effect the model does not declare reddens a test instead of
surfacing at an operator's `app_publish`.

### The read-models a report needs

A `kpi-tile` and a `chart` bind a **projection**, never an entity, and a tile's
read is a SUM of the measure across the projection's rows. So a headline figure
must exist as a measure in `model.yaml` before any app can show it — four do,
beside the older `debtor` and `cash_flow_13w`:

| Projection | Grouped over | Answers |
|---|---|---|
| `cash_position` | `bank_account` by currency | how much cash is actually on hand |
| `invoice_book` | `invoice` by direction | how many invoices, how much open, how much overdue — both sides |
| `cash_movement` | `transaction` by direction | money in vs. money out (the filtered sum `cash_flow_13w` cannot express) |
| `cash_forecast` | open `invoice` by due date | the forward curve the 13-week window folds |
| `cash_inflow_outlook` | `cash_inflow_estimate` by target date | what the Procedure expects, per week it is about |
| `cash_inflow_actual` | settled outbound `invoice` by paid date | what actually landed — the trail the estimate is judged against |

## The weekly cash forecast — the ADR-0202 Procedure

This tribe carries the platform's one **learned** Function. Everything else in
`functions:` is an expression over fields; `cash_inflow_forecast` is a
**Procedure** — a fitted artifact with a version, an audit card and a holdout
score, bound through `wild:function/backing` rather than a tool slug. Its
source is
[`examples/tool-providers/cash-forecast`](../../tool-providers/cash-forecast).

### Why it is here at all, next to a forecast that already exists

The tribe has TWO forward curves, and the difference between them is the
whole argument for a Procedure:

| | reads | assumes |
|---|---|---|
| `cash_forecast` (projection) | open invoices on their due dates | every debtor pays exactly on the due date |
| `cash_inflow_outlook` (Procedure) | a seasonal model over two years of settlements | what usually happens — including that nobody pays in the second week of August |

The first knows only what it was told. The second is the learned correction
to its one assumption, and the gap between the bars is the tribe's payment
behaviour stated as a number. Both are drawn in
[`apps/liquidity-report.yaml`](./apps/liquidity-report.yaml), with the
`actual-inflow` chart running the same 13-week width **backwards**
(`start: today-13w`) so an operator can check the estimate rather than trust
it.

### The horizon is a record field, and that is a fix

ADR-0202 D10.6 offers two homes for the horizon — a static `config` value or
a per-run record field. Only one of them is wired: `FunctionDef.config` is
read by the `llm-prompt` backing and by nothing else, and the synthesized
enrich trigger carries `inputs` + `outputs` with no config at all. A
`config: {horizon_days: 30}` therefore never reached the component, which
fell back to its OWN default of 30 and answered plausibly for a horizon
nobody had declared. It read as working because the two 30s agreed.

`horizon_days` is now a field on `cash_inflow_estimate` and an input of the
Function. That removes an unread declared key — and it is what lets ONE
weekly run write a whole curve (7, 14 … 91 days out) instead of the single
point a static horizon allows.

### What the seed contains, and what it deliberately does not

[`data/cash-inflow-estimates.csv`](./data/cash-inflow-estimates.csv) carries
65 rows: one per past Monday asking for the week that followed (the accuracy
trail), plus one run from the current Monday asking 7, 14 … 91 days ahead.

**It has no `expected_inflow` column.** The rows are the *question*; the
Procedure answers at the ingest boundary and the answer is written into the
field. An empty column in the file becoming a curve on the surface is the
proof that the whole chain ran — build → pin → declare → gate → store →
describe.

Beside it sits
[`cash-inflow-estimates.intake.yaml`](./data/cash-inflow-estimates.intake.yaml),
and it is not optional: a seed CSV is matched to its type by the
**fingerprint of its header**, and that fingerprint is registered only by an
`.intake.yaml`. Without the mapping the file is skipped — named, with its
reason — and the apply **fails** instead of deploying green around the hole
(`--allow-holes` is the deliberate opt-in to a partial seed).

### Supplying the Procedure — two doors, and they are not equal

`wild tribe apply` pins the ontology and loads the rows; it does not build a
Wasm component. Applied with no Procedure installed, the estimates stand with
an **empty** `expected_inflow`, and the Elder's `domain_readiness` names the
gap — the same door § *Exporting this domain* uses below. That is a legal
state, not a broken one, but it is also the state you get by default, so
pick a door:

```bash
# Door 1 — the number. Build and install the component directly.
bash plugins/scripts/build-wasip2-component.sh examples/tool-providers/cash-forecast
wild plugin add <the built .wasm>
```

Door 1 fills `expected_inflow` and nothing else. The manifest reaches a
catalog row on **one** path only — `forge.meta.json` → `ForgeBuildComplete` →
`ComponentTypeYaml.model` (ADR-0202 D10.2) — and only the forge lane can set
it. So a hand-installed component is an ordinary tool that happens to answer:
no Procedure node, no audit card, no `backing_version`, and `procedure_rerun`
has no pinned version to replay.

**Door 2 — the number AND its provenance: forge it in-profile.** This is not
a one-liner. The Procedure is authored through the ADR-0202 D8 lane (the
`evolve.forge.capability` decision carries the brief; `wild forge brief
<tribe> <capability>` renders what the Wright works from), and the committed
source under `examples/tool-providers/cash-forecast` is the reference of what
that lane should produce — `model.yaml` beside it is the authored half of the
`forge.meta.json` a build attaches.

Door 2 is the one ADR-0202 D7 is about: *"wie haben Sie das damals
gerechnet?"* is answerable only when the version, the method sentence, the
fitted parameters and the source digest are on the catalog row. Use door 1 to
see the chart work; use door 2 when you want to show the audit story.

### …and one more step, or the curve stays empty

A Procedure-backed Function is **effecting**, so every invocation goes
through the ADR-0048 effect gate. Ungraduated, the gate does not refuse it —
it opens a **decision row per call and waits**. Apply this bundle with the
component installed and nothing else done, and you get: 65 estimates
ingested, 65 pending decisions (`rule_key: data.enrich.cash_inflow_forecast`,
risk `medium`), and a forecast chart that is empty. Nothing errors. That is
the gate working as designed — and it is also the single most likely reason
someone concludes the dogfood is broken.

Graduate the rule once, in the operator-intent file — `system/` has no
Wasm-guest write path, so a bundle structurally cannot do this for you and a
chief cannot graduate itself:

```yaml
# <profile_root>/system/autonomy.yaml
tribes:
  <your-tribe-id>:
    data.enrich.cash_inflow_forecast: autonomous
```

The file is mtime-cached, so the next admit picks it up with no restart.
**Set it BEFORE `tribe apply`** and all 65 estimates fill during the apply;
set it after and the already-pending decisions still need resolving, because
each carries its own call.

Measured on a live profile: with the rule graduated up front, `tribe apply`
ingests 1496 records, all 65 estimates carry an `expected_inflow`, and the
`cash_inflow_outlook` projection materialises 65 rows — `estimate_count: 1`
on every one, which is the healthy reading: two estimates for one date at the
same version would mean a re-run wrote a second row instead of replacing one.

**Known gap, named rather than implied:** nothing produces the weekly rows on
a cadence. A flow entry is `connector` / `trigger` / `subject` / `manual` /
`channel` — none of them is "every Monday, write a row", so the recurring
production of estimates is a seeded fact here, not a running one. The rows
age exactly like the rest of the seed, and `example_seed_freshness` reddens
when the curve stops looking forward.

### Refitting — v3 → v4, and the five places a version lives

The shipped Procedure is **v4**, and it did not start there. v3 was fitted on
104 weeks — two samples per week-of-year bucket — so each of its 52 seasonal
factors was a mean of two draws from a ±6 % noise band. Most of its 3,8 %
holdout error was estimation noise in the factors, not anything about the
business. v4 is the *same method* on 156 training weeks, three samples per
bucket, still holding out a year:

| | training weeks | baseline | holdout MAPE |
|---|---|---|---|
| v3 | 104 | 42308.60 | 0.038 |
| v4 | 156 | 42068.98 | 0.0353 |

The baseline moving *toward* the synthetic business's true 42000 is the check
that this is a better estimate and not merely a different one. The method
paragraph on the card is untouched on purpose — D7.1 asks for a model that
fits in one human sentence, and a refit that also changed the method would
make the v3-vs-v4 gap say two things at once.

**v3 does not become unreproducible.** `training/fit.py` carries one entry per
registered version and they never leave: `python3 fit.py v3` rewrites the old
extract byte-for-byte and prints 42308.60 / 0.038. A stored estimate claiming
v3 has to stay checkable for as long as it stands — the training-time half of
the re-run door. The series is one law walked with one seed in one order, so
v4's extract is a **superset** of v3's: the first 156 rows are identical.

The interesting part is what a refit touches. The version is not one fact
stored once — it lives in five places, and it has to:

1. **half the estimate's identity key**, so v3's and v4's answers for one date
   stand side by side rather than overwriting each other;
2. **the forward chart's filter**, because a `sum` over that key sums *across*
   versions and would double the curve the moment a re-run lands;
3. **the Function's description**, the sentence telling an operator how good
   the estimate is;
4. **the component's audit card**, which is what it actually is;
5. **the component's `skills/cash-forecast.md`** — the file an LLM reads before
   it speaks, which quotes the version, the Güte *and* the baseline in prose.

Nothing compared them. `the_forecast_surfaces_name_the_live_procedure` does
now, and the failures it prevents are not errors. A filter left at `v3` renders
an **empty chart**, which reads exactly like a tribe that has no forecast yet.
A skill file left at v3 is worse: every stored row is correct, no chart is
empty, nothing disagrees — Elder simply *tells* the operator the superseded
fit, with a number and a Güte, in a tone of complete confidence. The fifth
place was found by reading, after the refit had already updated the other four
and left this one at v3, MAPE 0,038 and a baseline of €42.308,60.

### The row says who computed it

`cash_inflow_estimate` carries both `procedure_version` (what the row is *for*)
and `computed_by_version` (what actually answered). That is not redundancy —
it is the only way to make the pair checkable.

The key part must be authored: a row needs its key before it exists, and an
*effecting* enrichment runs at the boundary, after keying. The computed half
arrives from the Procedure's own answer — the component states its version,
and the field binds that answer key by name:

```yaml
enriched:
  - { name: expected_inflow,     type: decimal, function: cash_inflow_forecast }
  - { name: computed_by_version, type: text,    function: cash_inflow_forecast,
      enrich_output: procedure_version }
```

Before that binding existed in the DDD lane the component's claim was simply
dropped, and a seed left at v3 after a refit produced rows claiming v3 that
carried v4's numbers — with nothing erroring. Now the contradiction is a
visible pair on the row.

### Nachrechnen — the re-run door

`procedure_rerun` is the strongest audit answer: not *"here is what v3
claimed"* but *"here is v3 recomputing it in front of you"*. It resolves the
build recorded for that version, instantiates it outside the registered row
and calls it, so the platform never merely **says** a number was right.

Both doors are **Elder tools, not CLI verbs**, and both live in the *operator*
chat — `procedure_rerun` is reachable in `mentor` and `operate`, and is
deliberately absent from a tribe's own Domain-Elder catalog. Recomputing with
a superseded artifact is an operator act, not something a tribe does to
itself:

```
# in the operator chat
"zeig mir die Verfahrensstände von cash_forecast"     → atlas_query(op="procedures")
"rechne die Prognose für den 04.01.2027 mit v3 nach"  → procedure_rerun
```

Everything else is derived — the component, the retained build digest and the
backing export all come from declarations already in the lake. You name the
Procedure and the version, which is what the card actually shows you.

It refuses rather than approximating, and the refusals are distinct on
purpose: a card that recorded **no build digest** (registered before builds
were retained) can be read but not recomputed; a **swept blob** points at
evidence that is gone. Neither falls through to the currently registered
build — answering with today's code under an old version's name would answer
a different question, invisibly.

**Two versions in the lake is where the surfaces earn their shape.** The
forward chart stays scoped to v4; `cash_inflow_by_version` — deliberately dull
while one version is live — is where the comparison shows up. Note it sums
over *different date ranges* per version unless the re-run covered the same
stretch, which is what `estimate_count` is there to reveal.

**The seed ships one version, on purpose.** Seeding v3 rows beside v4 to make
the comparison chart look populated would be fabricated history: rows are
enriched by whatever component is installed, so those v3 rows would carry v4's
numbers. The v3 rows come from an actual re-run, or they do not exist.

### The second Procedure — when each debtor actually pays

`cash_forecast` (the projection) sums open invoices on the day they fall
**due**, and its own description admits what that assumes: *"it assumes every
debtor pays on the due date, which is exactly what the DSO and the dunning
lane exist to correct"*. The `payment-delay` Procedure is that correction, per
invoice.

It learned one mean delay per debtor from 1.044 settled receivables, shrunk
toward the pooled mean by `n / (n + 10)`. On the 72 receivables held out:

| | holdout MAE |
|---|---|
| per-debtor, shrunk | **1.9014 days** |
| one pooled mean for everybody | 4.2987 days |

Both figures are measured in the same test, which also asserts the first is
less than half the second. A score with no baseline beside it is a number
nobody can judge.

**The two curves are the point.** `forecast-incoming` (teal) is what the
contract says; `verfahren-settlement` (amber) is the same money on the day it
is expected to arrive. Where an amber bar sits to the right of a teal one,
that money is late, and how far right says by how much — BP-004's receivables
shift about eleven days, BP-006's move three days *left*, because it pays
early. No arithmetic required.

**Why it is its own type and not four fields on `invoice`.** The reason is
that `invoice` is one type in two directions, and a debtor model has no
business running over payables — enriching it would call the Procedure for
every creditor invoice too.

An earlier draft gave a privacy reason first: `invoice` is
`customer_visible: true`, the customer read masks exactly the fields marked
`sensitive`, and the DDD lane's `EnrichedField` had no such mark, so an
enriched estimate on `invoice` would have been readable *by the debtor it is
about*. **That is no longer true** — `EnrichedField` carries `sensitive`
(ADR-0083), added for precisely this case. The privacy argument no longer
decides anything here; the direction argument still does.

**The row says how much it knows.** Beyond the date, every estimate carries
`delay_spread_days` and `delay_basis`:

- `own_history` — this debtor's own record carries the estimate;
- `blended` — too little history, pulled toward the house average. BP-007, the
  client that arrived in January, sits here with 15 invoices and weight 0,60;
- `no_history` — the fit never saw this partner, so the answer *is* the pooled
  mean, together with the much wider pooled spread.

Without those two fields a client seen fifteen times would look exactly like
one seen a hundred and seventy, and BP-004 — mean +11,6 days, spread ±4,8, the
one whose partner note says "irregular payment behaviour" — would present a
week-wide guess as a day.

### The third Procedure — what the foreign book is worth

Three receivables in this example are not in euro: a dollar, a pound and a
franc invoice. They are the reason every money measure in the model now carries
`currency == EUR`, and they are what the `fx-exposure` Procedure prices.

**The platform forced the architecture here, and that is the lesson.** A
Procedure may not fetch its own market data — ADR-0202 D10.3's import
allow-list carries no sockets and no http — so the impure half is a *source*
and the rate arrives as a stored field. What looks like a limitation is what
makes a stored valuation auditable: the rate and the day it was quoted are on
the row, so "why did this say 20.897 €" still has an answer in three years.

No connector was written for it. An `https` locator resolves to the bundled
`web-connector`, and the **host** owns the socket: it parses the URL, enforces
the default-deny `system/egress.yaml`, applies the timeout and writes the
`audit.egress` row. The operator grants `data-api.ecb.europa.eu` once.

**Why the data API and not the familiar `eurofxref` files.** The ECB's classic
download page offers CSV only inside a `.zip`, and its unpacked form is XML.
Neither reaches the deterministic intake door, which reads csv / json / xlsx —
XML falls through to the **document** door, i.e. an LLM reading exchange rates
out of markup. For a figure that anchors an audited valuation that is exactly
the wrong tool. The data API answers `text/csv` uncompressed, no key, no
registration.

#### Two axes, and both live on the row

`cash_forecast` and `payment_delay` answer from their inputs alone, so their
audit cards are complete on their own. This one is anchored to a market rate
from a particular morning: **the same model, the same invoice and yesterday's
rate is a different number — correctly.**

So `fx_valuation` keys on `[invoice_id, procedure_version, rate_date]`. The
snapshot is part of the identity, which is what makes a re-valuation tomorrow a
*new* row instead of an overwrite — and what keeps "what was it worth when we
reported it" answerable. `anchored_to_rate_date` rides beside the authored
`rate_date`, the same authored/computed pairing the inflow estimate carries for
its version.

#### The rates are real, and the decimals decided a type

The seeded snapshot is the actual ECB publication for 2026-07-31:

| | rate per EUR |
|---|---|
| CHF | 0,9304 |
| GBP | 0,85573 |
| USD | 1,1485 |

Four decimals for the franc and the dollar, **five** for the pound. That is the
ECB's own per-series precision and the reason `rate_per_eur` is `text` and not
`decimal`: one fixed scale would have rounded the pound on the way in.

#### Never the middle without the band

`fx_exposure_outlook` groups by currency and the chart draws **three** series —
low, expected, high. `expected_eur` alone reads as a fact; it is the centre of a
range 2,7 % wide for a dollar invoice a month out. The widget cannot show one
without the others, which is the skill file's rule expressed as a shape.

Reading the euro read-models and this one together is the whole open book.
Reading either alone is a partial answer that does not say it is partial.

### Exporting this domain — what travels, and what cannot

A Procedure is also the sharpest case for what a **domain package** is and is
not, which is why this tribe is the one the export machinery names. Export
the domain and the model travels: aggregates, verbs, projections, apps, and
the Function `cash_inflow_forecast` **with its backing intact**.

What does not travel is the Procedure itself. Its substance — ADR-0202 D7's
audit triangle of `method` + fitted `parameters` + `source_ref` — lives on
the PROFILE's component-catalog row, and a domain package is TRIBE-scoped: it
is a bundle of declarations, never code (ADR-0156 D1). So the export reports

```
Procedure required: cash_inflow_forecast → cash-forecast/cash_forecast
```

and the recipient installs a domain with a named requirement rather than a
Function that silently produces nothing. Stripping the backing instead is not
an option — a Function with no backing at all fails `FunctionDef::validate`,
so the package would refuse to install rather than install honestly
incomplete.

Two publisher decisions ride the same door, and they are deliberately
different words:

| `procedure_basis` | What the recipient gets | What they are told |
|---|---|---|
| `carry` (default) | `procedure/cash-forecast.json` — the audit card travels, so the method can be re-weighted rather than inherited frozen | the basis is here |
| `withhold` | nothing | *"the publisher kept the method"* — a decision, said out loud |

The default is `carry` because a publisher who said nothing has not decided
to keep their method — they were not asked. A near-miss spelling
(`withold`) is **refused**, never defaulted: reading it as `carry` publishes
the method irreversibly, and refusing costs a question.

Both forms leave the same empty `procedure/` directory on the receiving side,
which is exactly why the install preview has to name which one happened —
a kept method must never read like a fetch that failed. Pinned by
`a_withheld_basis_reads_as_a_decision` (`wild-tribe-ops`), and that this
domain still declares a Procedure at all by
`the_flagship_domain_exports_its_procedure`.

On the receiving profile, `domain_readiness` derives the gap against the
**recipient's** component catalog, so `ready` is a conjunction: the domain is
not ready until the Procedure is supplied there — by the same two doors as
above.

## CRUD effects — the `crud/` script (ADR-0083)

Generic **create / read / update / delete** can be exposed on a type as
governed, entity-scoped effects that act exactly like the bespoke verbs. They
are **materialised, never authored** (ADR-0083) — the compiler emits none from
`model.yaml` — so, like the operator flows, the reproducible artifact is a
post-apply bind: [`crud/apply-crud.sh`](./crud/apply-crud.sh) binds
create/update/delete on **`dunning`** (`crud_bind`), giving the dashboard detail
card a **"+ Add new"** action and the domain surface `dunning_create` /
`dunning_update` / `dunning_delete`. `read` is auto-bound (reads stay free);
`crud_unbind` retires an effect (→ internal-only).

`dunning` is an **authored** aggregate, so a hand create/delete is meaningful.
`invoice` deliberately gets **no** create effect — it is a `source_mirror` whose
records the ingest feed owns, so "create an invoice" is correctly refused (the
feed, not an operator verb, mints invoices). Who may invoke a CRUD effect derives
from the bound type's live `customer_visible`. Since ADR-0178 the operator can
also drive this in Elder chat ("enable creating dunning notices").

## Correctness — the compiler gate

The claim "the model compiles to a correct ontology" is not a promise, it is a
**test**: the compiler tests in `wild-ddd` (`type_lane_*`, `verb_lane_*`, and
their value-object / function / source / mapping / flow / process siblings)
compile this model and assert the emitted records against committed goldens — so
a compiler regression reddens a test rather than shipping a broken tribe.

## Lane-independent assets

The domain knowledge is carried by the model; the rest of the bundle is
lane-independent (copy the whole folder as a template):

- `blueprint.md` — the deployed chief's system prompt (mission).
- `specs/` — `domain.md` (framework knowledge, §17 InsO, KPIs), `onboarding.md`
  (priority stack), `sketch.md` (mission narrative). The dunning reaction is no
  longer a spec trigger — it is the `dunning-escalation` **process** in the
  model. There is **no** `specs/domain-model.yaml` — the real
  `ontology/model.yaml` *is* the model.
- `data/` — the CSV starter seed + intake mappings (one-shot, lane-independent).
  Generated, not typed: bank balances fold from an `opening_balance` plus the
  movements, every settled invoice has a matching bank booking with the right
  counterparty and IBAN, and every IBAN carries a valid mod-97 check digit —
  so the reconciliation story the verbs teach actually reconciles. `amount` on
  a transaction is UNSIGNED, as the model declares; `direction` carries the
  sign. The dates are cut around a fixed "today" (see below).
- `documents/` — sample inbound-invoice scans (3 PDF, 5 DOCX) for the document
  door. `wild tribe apply` DELIVERS them into the tribe's file store, so the
  connector has something to read the first time it ticks; a per-tribe ledger
  makes that a one-shot, so a scan you processed and deleted does not come
  back on the next apply. Because the whole subtree is delivered, only real
  documents belong here — a test holds that line
  (`example_documents_are_reachable`).
- `scripts/` — the PDF generator for those scans. It sits outside `documents/`
  precisely because that tree is delivered: parked beside the samples, the
  generator arrived in the operator's inbox and was counted as one of them.
- `workers/dunning-notifier.md` — the Telegram dunning worker (ADR-0098), now
  the judgement (`level > 3`) + v1-fallback step of `dunning-escalation` (the
  routine notice is deterministic since ADR-0124); `workers/receivables-chaser.md` — raises
  the first-level dunning when the `receivables-overdue` flow (or the chief's
  judged backstop) dispatches it (atlas-only, no egress). The `invoice-dual-check` triage is a `does: decide` rule step
  (logic, no worker), so it ships no `workers/*.md` — see ADR-0144.
- `ontology/enrichment_rules/` — the ingest-boundary enrich rule (a pass-through
  lane the compiler does not emit).

## Try it in a probe session

1. **P1 guidance** — the chief opens with a bank-data ask, not a blank prompt.
2. **Off-mission rejection** — hand it fitness data → it explains why it doesn't
   fit and names what it needs.
3. **Grow the model in chat** — describe a rule the model doesn't carry (a new
   invoice state, a new source) → the chief offers a `ddd_declare_*` and grows
   the constitution through the gate, never a raw schema write.
