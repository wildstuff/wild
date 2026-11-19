# math-tribe

A reproducible, multi-path arithmetic-agent example. The smallest
useful tribe: one chief, one worker, one user channel.

## What it shows

- **Bundle layout** — `manifest.yaml` + `workers/*.yaml` +
  `blueprint.md`. The minimum a `wild tribe apply` needs.
- **Multi-path tool-use** — the blueprint tells the chief "either
  answer directly or delegate". The chief picks. Both paths reach
  `cycle.completed` the same way; useful as a stable end-to-end
  smoke fixture (the test asserts on cycle shape, not on which
  branch ran).
- **Worker prompt override** — `workers/math.yaml`'s
  `prompt_overrides.system` shapes the worker's reply format
  without rebuilding the ai-worker component.

## Deploy

```sh
# From the repo root, against a running daemon
# (`wild up --offline` or `wild up`):
wild tribe apply examples/tribes/math-tribe/   # applies AND starts it

# Or hold it back — pinned, nothing running:
wild tribe apply examples/tribes/math-tribe/ --dormant
wild tribe activate math-tribe                 # start it later
```

`wild tribe apply` starts the tribe (ADR-0031 amendment): it persists
the manifest + blueprint + ontology, then materialises chief + worker
components, NATS subscriptions and the cron — through the same
readiness gate `wild tribe activate` uses, so a missing secret or a
full active-tribe cap leaves a dormant tribe with the reason named.
`--dormant` keeps the old two-step when you want to inspect first (no
compute / LLM until you say so). The manifest's workers are preserved
across activate via the apply-time RenderInput cache.

Once started, send the tribe a math question:

```sh
wild chat --tribe math-tribe "what is 17 + 25?"
```

(Or any other message — the chief's blueprint tells it how to
respond.)

## Verify

After a cycle runs, you can read what landed:

```sh
wild traces ls --tribe math-tribe
wild traces export --tribe math-tribe --out math-corpus.jsonl
```

## Fork it

Copy the directory anywhere you like, edit `blueprint.md` and
the worker file, change the `name:` in `manifest.yaml`, deploy:

```sh
cp -r examples/tribes/math-tribe ~/my-tribes/research-cell
$EDITOR ~/my-tribes/research-cell/blueprint.md
$EDITOR ~/my-tribes/research-cell/manifest.yaml   # rename
wild tribe apply ~/my-tribes/research-cell/
```

Tribes are config — there's no separate "Examples" registry. A
git repo of `~/my-tribes/*` is the natural way to share + version
your own tribes inside a team.

## See also

- `examples/README.md` — the broader pattern.
- `crates/common/src/bundle.rs` — full bundle schema.
- `docs/cycle-schedules.md` — how to add an autonomous cron tick.
