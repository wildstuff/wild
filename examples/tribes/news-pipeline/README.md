# news-pipeline — one source, two formats

Hourly Hacker-News front-page scan that produces **two independent
artefacts** from the same raw feed:

1. A concise **B2B marketing brief**.
2. A 150–200 word **microfiction vignette**.

Useful as a compact example of how one data source can feed multiple
workers with different personas and output shapes.

```
┌─────────────────┐  schedule "0 * * * *"
│  orchestrator   │  triggers each hour
└───────┬─────────┘
        │
        ▼
┌─────────────────┐  GET hn.algolia.com/api/v1/search?tags=front_page
│ headline-fetcher│  → raw JSON hits
└───────┬─────────┘
        │
   ┌────┴────┐
   ▼         ▼
┌────────┐ ┌─────────────┐
│summarizer│ │ story-weaver │
│ brief  │ │  vignette   │
└────┬───┘ └──────┬──────┘
     │            │
     └─────┬──────┘
           ▼
      user log channel
```

## Bundle layout

```
news-pipeline/
├── manifest.yaml              # tribe schedule, default model, workers
├── blueprint.md               # chief persona + cycle behaviour
├── workers/
│   ├── headline-fetcher.md    # http-fetcher → HN Algolia
│   ├── summarizer.md          # ai-worker → marketing brief
│   └── story-weaver.md        # ai-worker → microfiction vignette
└── README.md                  # this file
```

## Prerequisites

- A wild profile, running:
  ```
  wild profile new demo
  wild --profile demo up &
  ```
- An LLM adapter named `claude` in `<profile_root>/llm-adapters.yaml`.
  If you use a different adapter, edit `manifest.yaml::default_model` (or set
  the frontmatter `model:` on each worker file).
  ```
  wild --profile demo llm add claude
  ```

## Deploy

```
wild --profile demo tribe apply examples/tribes/news-pipeline/
```

The first cycle fires within the next hour. To trigger immediately:

```
wild --profile demo tribe trigger news-pipeline
```

## Watch it run

```
wild --profile demo tui                           # full live view
wild --profile demo logs summarizer --tail 50     # the brief
wild --profile demo logs story-weaver --tail 50   # the vignette
```

## Cost control

Each cycle hits the model twice (summarizer + story-weaver). With Claude
Haiku that's roughly $0.0002 per cycle (≈$2/year); with Claude Opus it's a
few cents per day. Pin cheaper models per worker via the frontmatter
`model:` key if you want to run this continuously.

To pause without deleting:

```
wild --profile demo tribe stop news-pipeline
```

To remove entirely:

```
wild --profile demo tribe delete news-pipeline
```

## Customising

- **Different feed.** Swap `workers/headline-fetcher.md` frontmatter
  `config.url` for any single-call public JSON endpoint.
- **Different angle.** Edit the persona sections in
  `workers/summarizer.md` or `workers/story-weaver.md`.
- **Different cadence.** Change `manifest.yaml::schedule`. Standard
  five-field cron; e.g. `0 9,17 * * MON-FRI` for twice-daily on weekdays.
