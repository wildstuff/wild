# Self-modeling — relational self-knowledge (ADR-0034 dogfood)

A tribe accumulates knowledge about its own work — heuristics that paid
off, failure-modes to avoid. Historically that lived only in flat
`briefs/<cap>.md` files the chief had to **re-read every cycle**: prose,
not queryable, no relationships. Self-modeling puts that accumulated
learning into `wild:data` + the graph so the chief can ask its
understanding **relationally** ("what do I know about capability X")
instead of re-loading every brief.

This is eat-your-own-dogfood: Wild manages its own intelligence with its
own primitives — and validates the platform on itself. It is an
**extension** of existing pieces, not a new store: model-first born-types
(#928), the graph interface (#867), `reflect`, and the decisions trail.

## The boundary (load-bearing)

Only the **accumulated, relational learning layer** goes to
`wild:data` + graph. Everything else stays where it is:

- **Declarations** (mission, specs, blueprint) stay **FS-canonical** —
  two-entrances, human authority. They are NOT moved into `wild:data`.
- **Decisions** stay in their trail; a learning only **references** a
  `DEC-…` id (`decision_ref`), it does not re-model the decision.

## The three moving parts

1. **Born-with ontology.** Every tribe is born with two empty types
   (`crates/runtime/data-cli/src/self_modeling.rs`), seeded through the same
   idempotent, human-edit-wins `declare_tribe_ontology` path as a
   model-first `entity_model`, FS-canonical under `types/<slug>.yaml`:
   - `learning` — `{id, summary, kind, severity, cap_ref, decision_ref,
     model_ref, created_at}`, `origin: authored`, with a graph relation
     `about_capability` (`via_field: cap_ref`) to… (`model_ref` is
     ADR-0108 §4's domain-model anchor — a Type/Verb slug, an indexed
     reference like `decision_ref`, recalled by `find-by-field`.)
   - `capability` — a thin anchor node so a graph walk that *starts* from
     a capability has a pinned start type. The capability *declaration*
     still lives in specs; this is only a graph anchor.

   **Coverage.** Born-with fires on the `tribe.create` path (exactly
   where model-first #928 seeds an `entity_model`). The root tribe
   (bootstrapped) and tribes created before this feature don't get the
   types retroactively; they acquire them on (re)creation or via an
   explicit `wild data type apply types/learning.yaml`. The chief
   degrades cleanly until then — `record_learning` against an unpinned
   type returns a `backend disabled`/`unknown type` error, never a crash.

2. **`reflect` writes relationally.** When a reflect tick surfaces a
   reasoning learning (behaviour already correct, but a heuristic /
   failure-mode emerged), the chief calls **`record_learning`** →
   `wild:data/store.upsert` against the `learning` type. The `cap_ref`
   value materialises the `about_capability` graph edge automatically on
   write (host-side projection). The id is a content hash so re-recording
   the same insight is an idempotent overwrite. The flat brief-delta path
   (`propose_change`) stays available; the structured path is additive and
   is the new value.

3. **Relational recall.** The chief calls **`recall_learnings`** to read
   its self-knowledge without re-reading briefs:
   - by capability → `query.find-by-field` on `cap_ref`, or
   - `via_graph` → `graph.descendants(capability, X, about_capability)`
     (the learnings reach the capability backward along the edge), then
     hydrate via `store.get`.
   - by `goal_form` → `query.find-by-field` on the indexed goal-shape key
     (prior `kind=outcome` learnings for exactly that shape).
   - by `model` (ADR-0108 §4) → `query.find-by-field` on `model_ref`
     ("what have I learned about the `dunning` verb / the `invoice`
     type"); own lake or the fleet commons.
   - no selector → most-recent learnings (`store.list-records`).

4. **Injected recall (deterministic).** Outcome recall does not depend on
   the model deciding to call the tool. The standard runner re-keys the
   `goal_form` off a scored Result wake's `quality` verdict (the same
   verdict the outcome recorder uses), queries the indexed column, and
   appends the rows to the system prompt as `## Recalled outcomes for
   this goal-shape (auto-injected)`. The root Elder's design prompt gets
   the analog `## Recalled outcome ledger (auto-injected)` — one line per
   goal-shape pursued (root-scoped; learnings are tribe-scoped by
   design). Empty lake or fresh shape ⇒ no section, zero prompt-token
   cost. Each injection logs `loop.recall_injected` /
   `elder.design_recall_injected`, so recall is observable instead of
   inferred from the model's tool use. The `recall_learnings` tool stays
   for capability recall and for digging beyond the injected digest.

5. **Outcome history (ADR-0058 Strand 0).** Outcome records are
   **attempt-keyed**: the id hashes `(goal_form, target, strategy,
   cycle_ref)`, so re-running a strategy appends a row instead of
   overwriting the score, and `recorded_at` (full RFC-3339) totally
   orders same-day attempts. The injected digest renders one **score
   series per strategy** (`0.65 → 1.00 (2 attempt(s))`, freshest
   strategy first, dropped counts named, a one-strategy evidence base
   called out explicitly); the design ledger renders the **trend per
   shape** (`first → best`, attempt + strategy counts). Outcome rows
   optionally carry `strategy_caps` (the worker's capability slugs —
   the globally-shared vocabulary that makes a row readable
   cross-tribe) and `cited` (the prior-outcome ids that were injected
   around the attempt — the Strand C efficacy seam). `record_learning`
   gates `kind`/`severity` to the closed vocabulary writer-side (the
   engine has no record-value enums) and returns the existing
   learnings for the same capability, so duplicates and contradictions
   surface at the moment of writing; capability recall is ranked
   failure-modes first, severity before recency.

6. **Knowledge commons (ADR-0058 Strand A).** The ROOT tribe's lake
   is the fleet's commons: a host reconciler (boot-flush + 60s
   convergence loop, `wild-daemon::knowledge_commons`) mirrors every
   tribe's shareable learnings — `kind=outcome` rows and
   capability-ref'd heuristics/failure-modes — into the root lake.
   Mirrored rows are re-keyed per source tribe and carry
   `source_tribe`; rendering shows foreign evidence as
   `(from <tribe>)` and never merges it into a local series. The
   reconciler also seeds the `capability` anchor records from the
   component catalog (tool slug + description), so `about_capability`
   graph edges land on real nodes. Derived state (ADR-0041 pattern):
   tribe lakes stay canonical, value-equal rows skip (no version
   churn), deleting the commons rows converges back. Writers are
   HOST-only (`host:knowledge-commons`) — a guest can never inject
   rows into another tribe's evidence. The host Elder runner reaches the
   commons too (Strand A parity): a `recall_learnings` MCP tool over
   the root lake in every mode, plus the outcome ledger injected into
   its Intake design prompt through the same shared renderer. Net
   effect: the F4 design ledger is fleet-wide on both Elder surfaces. Chiefs read the
   commons too (ADR-0058 Strand A2): `recall_learnings scope: commons`
   routes the same `goal_form` / `capability` keys through the
   host-fixed `wild:data@0.5.0 query.recall-commons` verb — the ONE
   read-only, root-fixed cross-tribe read (no guest write path into
   another tribe's lake). The chief reads the commons deterministically at
   three points (A2b/A2c): a **capability-lessons** section (own +
   fleet heuristics/failure-modes, failure-modes first) injected at the
   top of every standard cycle before it dispatches; a **commons
   outcomes** sub-section beside the F4 goal-shape digest on a scored
   Result wake; and the same capability lessons injected into the
   reflect prompt before it records a new learning (so it revises
   instead of duplicating). Each is keyed by its concern field —
   `cap_ref` for lessons, `goal_form` for outcomes — so `target=self`
   autonomy rows (no `cap_ref`) never pollute them (ADR-0057 fit). The
   per-tribe OPERATOR CHAT (Mentor/Operate, both surfaces) gets the same
   push (issue #4712 Slice A): a `## What <tribe> has learned
   (auto-injected)` grounding section renders the tribe's own
   heuristics/failure-modes — cap-less and model-anchored lessons
   included — failure-modes first, through the shared
   `common::self_modeling` rank/line renderers; `recall_learnings` stays
   for depth. Observability: `loop.commons_mirror` +
   `capability.anchors_seeded` + one `audit.effect` row per changing
   sweep.

7. **Semantic recall (ADR-0058 Strand B).** Exact-match keys
   (`goal_form` check-ids) never collide across freely-authored tribes,
   so the `learning` type carries an optional `summary_vec` embedding
   (dim `SUMMARY_VEC_DIM`, born-with for NEW tribes). A host embed
   reconciler (`knowledge_embed`, `data`-gated, fail-soft) backfills it
   via `wild:ai/embed` — no adapter ⇒ no embeddings ⇒ recall degrades
   to exact-match; pre-B tribes (incl. root) keep exact-match until
   re-seeded (no auto-migration). The Elder's Intake design turn embeds
   the OPERATOR'S request (the proto-goal — the draft `## Success`
   doesn't exist yet at compose time) and KNN-searches the commons,
   injecting a `## Near goal-shapes` section behind a HARD distance
   cutoff (honest-empty over plausible-noise; the per-row distance is
   rendered so the reader judges). The cutoff is **live-calibrated**
   against `nomic-embed-text` (L2-normalised vectors): same-shape pairs
   sit at L2 ≤ 0.89, unrelated ≥ 0.91, so `NEAR_SHAPES_MAX_DISTANCE =
   0.90` (≈ cosine 0.60); a `near_shapes` probe in the `ollama_embed_live`
   conformance suite guards the separation against embed-model drift.
   Host-side, no WIT bump; the chief-guest semantic mode is a later
   `recall-commons-knn` verb.

8. **The earned graph edge (ADR-0058 Strand C).** Until now the
   `about_capability` edge was decoration (depth-1, scalar, anchors
   unpopulated). Strand C lands the first relation the graph genuinely
   earns: `cites` (`learning → learning`, self), materialised ONE edge
   per element of the `cited` array via the array-`via_field` projection
   generalisation. Today `cited` carries the prior outcomes in context
   when an attempt was produced, so `cites` is an attempt LINEAGE;
   `recall_learnings lineage_of=<outcome-id>` traces it both ways —
   `built_on` (outbound `neighbors`: what it built on) and `cited_by`
   (inbound `descendants`: later attempts that cited it — the read no
   single `cited` field can give, which is what earns the edge). New
   tribes get the relation; pre-C tribes stay edgeless until recreated
   (no auto-migration). **Strand C-B (SHIPPED):** `record_outcome` also
   cites the capability lessons that existed for the worker's capability
   at production time (re-derived at record time — the record-before-
   inject mirror), so the same edge becomes a lesson's *windowed
   efficacy*: its `cited_by` outcomes are exactly those produced AFTER
   it was recorded (its post-recording window). The reflect tick gets a
   deterministic `## Learning efficacy` digest surfacing lessons whose
   window scores systematically low (mean < 0.5 over ≥ 3) as
   RECONSIDERATION candidates. Two honest bounds: it is ABSOLUTE not
   counterfactual (the "before" outcomes don't cite the lesson and
   aren't compared — so it never proves causation, hence reconsider not
   auto-demote), and per-lesson only to the degree lessons on one
   capability were recorded at different times (co-recorded lessons
   collapse toward a per-capability signal). Reach is bounded by the
   worker→capability map (`primary_tool_name_for`) that populates
   `cited` — a worker with no known capability cites no lesson, so a
   richer roster is a follow-up.

9. **Operator-feedback candidates (issue #4712 Slice B).** A decision
   the operator REJECTS with a stated reason
   (`decision_resolve outcome=rejected note=…`) no longer feeds only the
   autonomy statistics: the hook-carrying resolve seam
   (`daemon-lib::manager::decisions_backend`) records the reason
   deterministically as ONE `target=self kind=outcome` learning on the
   `self:operator-feedback` goal-form
   (`common::self_modeling::operator_rejection_candidate`) — the FACT
   ("operator rejected `<rule_key>` because …"), with `decision_ref`
   provenance back to the trail row and `judge_score 0.0` (a rejection
   IS the operator's zero score). The same note also lands as the
   DDR `rejection_reason` (previously the edited blob was misused as
   the reason channel). Deterministic here means the EVENT reliably
   produces the record — attempt-keyed on the decision id, so a
   retried resolve collapses onto one row (upsert, never a duplicate);
   a reason-less rejection records nothing (no lake litter). Whether
   the reason GENERALISES into a `heuristic` / `failure_mode` stays
   with the reflect cycle's model judgement — the quality filter is
   not bypassed. Recall works through the existing surfaces
   (`recall_learnings goal_form=self:operator-feedback`, plus semantic
   recall over the `searchable` note summary); a deterministic reflect
   PUSH (an `autonomy_history_digest`-style injection) is the named
   follow-up. Observability: `loop.operator_feedback`. Guests can never
   mint these rows — the WIT resolve shape carries no `note`, so the
   plugin conversion pins it to `None`.

The chief world imports `wild:data/{ontology, store, query, graph}`; the
vocabulary (slug / field / relation names) is shared host↔chief via
`common::self_modeling` so the writer and the schema can't drift.

## Versioning (living)

Self-knowledge accumulates and evolves: the ontology is content-addressed
by `schema_hash` (version-independent), so an edited `types/learning.yaml`
bumps to a new version while identical content is a no-op. Learnings
themselves are upserts (revisable), not write-once.

## Toggle

`WILD_SELF_MODELING` (`cfg.tribe.self_modeling`, default **on** —
additive). Off → exact pre-feature behaviour: no born-with types, the
`record_learning` / `recall_learnings` tools are not advertised, the flat
brief path is untouched. Read host-side at `tribe.create` and **stamped
into the chief component's `wasi:config/runtime`** (`wild-self-modeling`)
so the wasm runners gate their own tool surface. Born-with seeding is a
no-op on a SQL-free (`--no-default-features`) daemon. See
[`config-vars.md`](config-vars.md).

## Forgetting — the learning-hygiene sweep

Learnings accumulate unbounded and silently crowd the recall windows
(every digest scans a fixed window; the oldest rows push the newest
out). The records already carry their own evidence — `recorded_at` /
`created_at` is the clock, an outcome's `cited` array is the usage
testimony — so forgetting needs no counters. A host sweep (riding the
DDD-reconciler tick, `WILD_TRIBE_DDD_RECONCILE_SECS`, plus one
boot-flush; **all** tribes, not lane-gated) asks the one shared verdict
`common::self_modeling::stale_learnings` which rows fell out of use:

- **Outcome-history compression** — per `goal_form` group past
  `OUTCOME_PRUNE_MIN_GROUP` (8), retain the newest
  `OUTCOME_KEEP_RECENT` (5) plus the single best-scoring witness; the
  rest is superseded history.
- **Uncited-lesson aging** — a `heuristic`/`failure_mode` older than
  `LEARNING_STALE_AFTER_DAYS` (45) that no retained outcome cites, with
  at least one outcome recorded since (proof the loop ran and had the
  chance to cite it). A tribe with no outcome traffic never forgets —
  zero evidence is not evidence of disuse.

**The commons follows the origin.** Mirrored commons rows carry
`source_tribe` + `source_key`; the 60-second mirror reconciler retires a
mirror whose origin row was tombstoned (both stamps present, the origin
lake answered this sweep, definite `NotFound`) — so an accepted prune
never leaves an orphan in the fleet recall, and there is no second
policy or gate at root: the origin accept WAS the decision. The inverse
stays convergent (a mirror deleted while its origin lives is re-created);
a deleted tribe's fleet evidence stays; pre-stamp legacy mirrors are kept.

The sweep only ever **proposes**: one curation change-folder per tribe
(`learning-hygiene-<12hex>`, content-addressed over the prune KEYS so a
pending proposal dedups while day counters tick), risk `low`, no
auto-accept stamp — the operator accepts, or the `data.curate` autonomy
rule merges it if armed. Accept tombstones the keys via `store.delete`
(append-only; the event log keeps full history). A pruned insight that
proves needed again is simply re-recorded — content-hash ids make that
idempotent.

## Deferred (not P0)

FTS5/full-text + a pluggable `wild:learner` · replacing `briefs/<cap>.md`
entirely (P0 is additive — ADR-0058 pins the flip-off gate) · modelling
decisions themselves as `wild:data` · automatic relation inference
(ask-don't-hallucinate).
