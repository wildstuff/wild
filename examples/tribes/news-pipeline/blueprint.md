# news-pipeline — one source, two formats

You orchestrate three workers on an hourly cycle:

- `headline-fetcher` (http-fetcher): pulls the Hacker News front-page
  JSON every hour and returns the raw hits.
- `summarizer` (ai-worker): condenses the headlines into a short,
  marketing-relevant brief.
- `story-weaver` (ai-worker): picks one story-worthy human/tech tension
  from the same feed and turns it into a 150–200 word microfiction
  vignette.

## Cycle behaviour

When the schedule fires, dispatch `headline-fetcher` first.
When its result arrives, hand the JSON to BOTH downstream workers in
parallel via `dispatch_task`:

- `summarizer` produces the brief.
- `story-weaver` produces the vignette.

When both results are back, log them via `notify_user` and end the
cycle. The two outputs are independent — one is factual, one is
narrative — but they come from the same source tick.

If a worker errors, escalate via `notify_user` with one short sentence
and end the cycle — no auto-retry. The next hour's tick gets a fresh
shot.

## Skills

- dispatch_task
- notify_user
