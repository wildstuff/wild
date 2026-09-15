# web-crawler — the ADR-0114 first instance

A crawler with **zero crawler-specific core code**: every piece below is
configuration of the shipped queue-fed intake lane
(ADR-0114).

```
wild tribe apply examples/tribes/web-crawler/ --as acme-crawler
```

## How the loop composes (D1–D6, no bespoke crawler)

1. **The work queue** — `crawl_queue` (ontology/seeds.yaml) is a
   domain-authored instance of the D1 contract: `url` is the record KEY
   (re-enqueueing a known URL is an idempotent upsert), `status`
   (`pending | done | failed`) is the closed work-set vocabulary,
   `depth`/`source`/`parent_url` are open domain payload.
2. **Feeding** needs no new surface — all three producers are ordinary
   governed record writes: the seed list (`data/seed-urls.csv`, loaded at
   apply), any worker's `atlas_put_record`, and the frontier worker below.
3. **Draining** — `ontology/sources/crawl.yaml` declares the D3 explicit
   verb pair (`queue-enumerate` + `url-fetch`) over the
   `queue://crawl_queue?ref=url&limit=200` locator, fired by every
   committed queue write plus a 15-minute sweep (D4 — the queue IS the
   payload). The egress allowlist is the crawl boundary: an enqueued
   off-allowlist link's fetch is denied + audited, the run continues.
4. **Completion** — the host intake-result seam flips each completed
   item `pending → done` (D6), so the next fire re-fetches nothing. A
   crashed run leaves its unreported items `pending`; the next fire
   re-fetches and ingest dedups — at-least-once × idempotent.
5. **The frontier** (D5) — `specs/crawl-frontier.md` arms an on-upsert
   trigger on `page`; `workers/frontier.md` is the GENERIC ai-worker
   configured to filter each new page's `links` (same host, depth
   ceiling 2, skip known rows — never resurrect a `done` row) and
   enqueue the rest as `pending`. The drain picks them up.

## The document-door interim (read before crawling at scale)

Fetched pages are `text/html`, which routes through the **document door**
(LLM extraction) today — the ADR-0114 first-instance text calls this the
expensive-but-working default. The deterministic path (an `html-links`
format-adapter emitting `page` rows directly) is **gated**: the runner's
structured/document fork is decided by a global MIME→door table, so
declaring `text/html → structured` would re-route EVERY web source's
HTML away from the document/search path — a per-source door override is
exactly the ADR-0112 "step-6 collapse" (extractor as door authority),
recorded in ADR-0114 § Deferred. Until then this example is correct but
pays LLM extraction per page — keep the seed list and depth ceiling
small.

## Governance you get for free

- **Crawl boundary**: the ADR-0090 egress allowlist (default-deny) — the
  destination list IS the grant; add your crawl hosts to `egress.yaml`.
- **Politeness floor**: the per-(tribe, destination) rate limits.
- **Budget**: `depth` on the queue rows + the ceiling in the frontier
  worker's brief.
- **Audit**: every fetch, every record write, every denied egress.

## Verifying the loop (the ADR's criteria, hands-on)

1. Apply the bundle, watch the seeded rows drain:
   `wild data query scan crawl_queue` → the two seed rows flip `pending → done`
   after the first fire; re-firing ingests nothing new (idempotent).
2. Enqueue a fourth row by hand (`atlas_put_record` via chat or MCP) —
   the on-event drain fires and only that item is fetched.
3. Watch coverage: `pending` vs `done` counts are the crawl's frontier
   state — first-class, queryable, dashboard-visible.
