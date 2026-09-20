# Fitness Tracker — a calorie-balance tribe

A **deployable** Atlas-v2 tribe that tracks the two sides of a client's energy
ledger and computes the net:

- **Calories IN** — `food_log` entries over a seeded `food` reference table.
- **Calories OUT** — `workout_log` entries over a seeded `exercise` table.
- **Net** — the `energy_balance` view (`calories in − calories out`, rolling
  7 days) against the client's `goal`.

It also showcases **web-search as the intake loop**: when the client logs a food
or activity not in the reference tables, the chief uses `web-search` (brave) to
look up its energy data and records it before logging — twice over (foods and
exercises). A `profile` (birth year, gender, body measures) makes discovery
**age/gender-aware**: the chief sets profile-aware search queries and matches the
`exercise` / `recipe` suitability fields to the client.

> This bundle is authored in the **DDD lane** (`manifest.yaml` sets
> `authoring_method: ddd`): the ONE `ontology/model.yaml` is the domain model,
> which `wild tribe apply` COMPILES into the ontology + verbs + source bindings.
> Unlike the ontology-only `liquidity-management` showcase, it also ships a
> chief, so `wild tribe apply` ALSO deploys a live coach.

## Load it

```bash
# One command pins the ontology + verbs AND ingests the seed data AND deploys
# the chief. `--as` names the instance (one base → many clients):
wild tribe apply examples/tribes/fitness-tracker/ --as alice-fitness
```

The ontology + seed data load with NO setup — you can log foods/workouts against
the 37 foods / 25 exercises / 15 recipes and read the energy balance immediately.
Live discovery (the web-search intake loop) is an OPTIONAL add-on; arm it in
three steps:

```bash
# 1. Give the brave-search plugin a Brave Search API key (masked → keychain,
#    never the chat). Free tier: https://brave.com/search/api/
wild secret add api-key                 # paste your Brave Search API key
wild secret grant brave-search api-key  # let the brave-search plugin read it

# 2. Allow OUTBOUND egress for the hosts url-fetch will pull (default-deny,
#    ADR-0090/0159). The Brave API host + the nutrition/recipe hosts you trust —
#    the scheduled crawl + the recipe frontier fetch only allowlisted hosts; an
#    off-list fetch is denied + audited, the run continues.
wild egress allow api.search.brave.com --tribe alice-fitness
wild egress allow fdc.nal.usda.gov --tribe alice-fitness   # + any recipe host you crawl

# 3. An LLM adapter for the coach chief (curation + record_food/record_recipe).
wild config llm list                    # ensure one is configured
```

Without step 1–2 the tribe still works fully on the seed data; the scheduled
`fitness-web-search` crawl and the `recipe-crawl` frontier simply stage nothing
(a keyless search returns no hits; an off-allowlist fetch is denied). Arm them to
turn on live discovery. See [`specs/onboarding.md`](specs/onboarding.md)
§ "Scheduled crawling" for how the coach curates the staged hits.

## Structure (the reference layout)

```
fitness-tracker/
├── manifest.yaml                 # deployable tribe (authoring_method: ddd) → coach chief
├── ontology/
│   └── model.yaml                # the DDD domain model — the SINGLE ontology source:
│                                 #   value object `kcal` + closed value-sets
│                                 #   @kcal_from_macros (Atwater cross-check)
│                                 #   aggregates + enforced edges + verbs + projections
│                                 #   the fitness-web-search + recipe-crawl source bindings
│                                 #   the recipe-from-web-source process (Feeder B)
├── data/                         # each CSV beside its <name>.intake.yaml
│   ├── foods.csv · exercises.csv · recipes.csv   (reference tables)
│   └── food-log.csv · workout-log.csv            (~1 week of sample tracking)
├── specs/                        # the chief's blueprint (sketch/domain/onboarding)
├── apps/
│   └── energy-balance.yaml       # the published end-user app (ADR-0154)
└── workers/                      # the two recipe-frontier feeders (ai-worker briefs)
```

## The client app — `apps/energy-balance.yaml`

One published app (ADR-0154), written for the person the data is about rather
than for an operator. `wild tribe apply` copies it into `<profile>/apps/` and
stamps it with the tribe you applied as, so `--as alice-fitness` gives that
client their own surface.

Six pages, following the day: **Balance** (calories in vs. out, each with a
seven-day sparkline), **Food** (the log, a board by meal slot, what you eat
most), **Training** (the log and a calendar, so rest days are visible),
**Recipes**, **Goal** (the two effect forms the client drives themselves), and
**Coach** (search + the chat panel that does the logging).

It is also where this bundle shows the current styling ladder: the app takes the
`comfortable` density preset and a `font_scale` nudge because it is read on a
phone at arm's length — the liquidity cockpit takes `compact` for the opposite
reason. Both presets multiply the same spacing ladder the renderer emits, so the
choice reaches every gap, padding and margin rather than a handful of them.

Validate with `wild app validate apps/energy-balance.yaml`. It is also checked in
CI: `example_bundle_apps_bind_to_their_compiled_model` compiles `ontology/model.yaml`
exactly as `wild tribe apply` does and runs the spec through the publish-strength
gate, so a view naming a projection, measure or effect the model does not declare
fails here rather than in front of a client.

## Recurring intake — Chief-free (ADR-0086) + a two-feeder recipe frontier (ADR-0114)

The scheduled web crawl is a declared `intake_source`
(the `fitness-web-search` source binding in `ontology/model.yaml`), NOT a per-30-min Chief wake. The
scheduler fires it Chief-free every 30 minutes: `search-enumerate` turns the
fixed query into result hits, `url-fetch` pulls each page, and one record per
hit lands in the `web_source` staging type. The chief then CURATES on demand —
it reads the staged hits and promotes the useful ones into the `food` /
`exercise` reference tables via `record_food` / `record_exercise` — and re-wakes
only to change the query (now **profile-aware**: rewritten from `profile`).

Recipes use the **queue-frontier** shape for maximum flexibility. ONE queue
(`recipe_candidate`, `url`-keyed, `pending|done|failed`) is drained by ONE source
(`recipe-crawl`: `queue-enumerate` + `url-fetch`, host flips `pending → done`),
fed by TWO independent producers that both write ordinary `pending` rows:

- **Feeder A — brave-direct** (`workers/recipe-discovery.md`): the chief/worker
  runs `brave-search` (profile-aware query) and enqueues the candidate URLs.
- **Feeder B — from `web_source`** (the `recipe-from-web-source` process): a new
  staged search hit fires a Chief-free `review` step that enqueues the
  recipe-relevant ones. (Needs the ADR-0112 step-1 data-event closure — on
  `develop`.)

The `url` key dedups across both feeders for free; the drain is single-flight;
`discovered_on` is stamped by the feeder at enqueue time. The old tribe
`schedule: "*/30"` and the `deny_tools` list are gone: `url-fetch` is
effect-class, so the effect gate already forces the chief to delegate it to the
intake-runner worker without a tribe-wide deny (which would have blocked that
worker). 48 LLM cycles/day → 0.

> Egress note (ADR-0090): brave-search **snippets** need no per-host egress and
> just work; deep `url-fetch` of arbitrary recipe sites is **default-deny** — an
> off-allowlist host is DENIED + audited. Prefer snippet-driven curation by
> default; add trusted recipe domains to `egress.yaml` before deep-crawling them.

## Atlas v2 features exercised

- **Value Type + directional roles** — `kcal` with `content` (reference) /
  `consumes` (intake) / `burns` (expenditure), so the chief reasons about
  energy direction, not just numbers.
- **Named Function (@ref)** — `@kcal_from_macros` (Atwater) cross-checks a
  food's stated kcal against its macros; materialised in the fold.
- **Read-time aging** — `food_log.days_ago` and `recipe.age_days`
  (`as_of() − date` / `as_of() − recorded_on`), computed per read, never stored.
- **Enforced relations** — `food_log → food` (`eats`) and `workout_log →
  exercise` (`performs`): you can't log calories for an item that isn't in the
  reference table. The loader ingests references BEFORE logs so the seed data
  resolves.
- **Grouped composites** — `daily_intake` / `daily_burn` (per-day sums) and
  `food_frequency` (per-food counts), re-projected on every log write.
- **A multi-source view** — `energy_balance`: `sum_exact(food_log.kcal) −
  sum_exact(workout_log.kcal_burned)` over a rolling 7-day window, the net the
  whole tribe exists to surface.
- **Profile-aware suitability** — `exercise` and `recipe` carry optional
  `min_age`/`max_age`/`gender_suitability`(/`level`) the chief matches against
  `profile` (birth year → age, gender).
- **Presentation + template marks** — icons/colors/categories; `food`,
  `exercise`, and `recipe` are `scaffold`s the onboarding "adapt these"
  checklist surfaces.
- **Customer-visible** — logs + reference tables are exposed to the client
  through the customer-facing Domain MCP (ADR-0083).

## The verbs (all low → auto — tracking is harmless)

| Verb | What |
|------|------|
| `log_food` | Record an eaten food (calories IN) |
| `log_workout` | Record a workout (calories OUT) |
| `record_food` | Add a food to the reference table (from a web lookup) |
| `record_exercise` | Add an exercise to the reference table (from a web lookup) |
| `record_recipe` | Add a recipe to the reference table (from the intake frontier or a lookup) |
| `set_calorie_goal` | Set the daily target the net is judged against |
| `set_profile` | Capture birth year / gender / body measures (drives matching + BMR) |

## Fork it for another use-case

Copy the directory, swap `ontology/` + `data/` + `specs/` for your domain, keep
the structure, and `wild tribe apply <your-bundle> --as <id>`. This bundle is
the reference for "what a populated, deployable tribe looks like".
