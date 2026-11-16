---
# ADR-0096 on-upsert sugar: compiled to the durable consumer on
# `wild.{tribe}.data.page.created`. When a crawled page lands, this
# spec's worker fires Chief-free and feeds the page's discovered links
# back into the crawl queue (ADR-0114 D5 — the feedback producer).
# `created`-only by design: a re-crawled page's UPDATE never re-runs
# the frontier, so the loop is driven by NEW coverage only.
triggers:
  - on-upsert: page
---

# Crawl frontier dispatch

When a crawled page lands, filter its discovered links and enqueue the new
ones as `pending` crawl_queue rows. The enqueueing itself is the generic
`frontier` ai-worker (`workers/frontier.md`); this spec only declares WHEN
it runs. The queue's own drain trigger (`ontology/sources/crawl.yaml`)
picks the new rows up — the loop is composed, never a bespoke crawler.

## Requirements

### Requirement: Extend the frontier when a page lands

On a new `page` record the tribe MUST dispatch the `frontier` worker to
enqueue the page's in-scope, not-yet-known links as `pending`
`crawl_queue` rows, bounded by the declared depth ceiling.

Capability: ai.chat

#### Scenario: A crawled page is ingested
- GIVEN a `page` record is created
- WHEN trigger.kind = data-event
- THEN chief MUST `chief_dispatch_task(worker_name="frontier", prompt="Enqueue the new page's in-scope links as pending crawl_queue rows, per your worker definition.")`
