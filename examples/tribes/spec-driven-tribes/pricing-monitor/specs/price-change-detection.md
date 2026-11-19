# Price-change detection

## Purpose

Snapshot competitor SKU prices on a daily cadence and surface
SKUs whose price moved by ≥10% versus the most recent prior
snapshot.

## Requirements

### Requirement: Daily snapshot

The tribe MUST capture a price snapshot for every configured
competitor SKU once per 24-hour window starting at the cadence
configured by the operator.

Capability: `schedule.cron` + `web.fetch`

#### Scenario: Cron fires at configured time

- GIVEN the tribe has been activated with `trigger_spec: "daily 08:00 UTC"`
- AND the operator has configured 4 SKUs across 3 competitors
- WHEN the daily cron fires
- THEN the chief MUST issue exactly one fetch per configured SKU
- AND each fetched price MUST be persisted under
  `<profile>/tribes/pricing-monitor/state/snapshots/<date>.json`
- AND a `wild.{tribe}.snapshot.taken` event MUST be published
  with the snapshot id

### Requirement: Snapshot is structured

Each snapshot row MUST contain `competitor_id`, `sku_id`,
`price_minor_units`, `currency`, `fetched_at_iso8601`. Free-text
HTML or unparsed bytes MUST NOT be persisted in the snapshot
row (raw fetch goes to `state/raw/<date>/<seq>.html` instead).

Capability: `storage.write`

#### Scenario: Schema-conforming row

- GIVEN a successful fetch of `competitor-a/sku-1` at 08:01:23 UTC
- AND the page lists "EUR 89.90"
- WHEN the chief writes the snapshot
- THEN the row MUST equal
  `{competitor_id: "competitor-a", sku_id: "sku-1",
    price_minor_units: 8990, currency: "EUR",
    fetched_at_iso8601: "2026-05-08T08:01:23Z"}`

#### Scenario: Refuse to persist malformed row

- GIVEN a fetch returned a page where the price extractor
  returned `null`
- WHEN the chief tries to write the snapshot row
- THEN the row MUST NOT be written to `state/snapshots/`
- AND a `wild.{tribe}.fetch.extraction-failed` event MUST be
  published with `competitor_id`, `sku_id`, and the raw
  fetch's `state/raw/` path
- AND the chief MUST continue with the remaining SKUs (one
  failed extraction does not block the snapshot of the others)

### Requirement: Detect ≥10% movement

The tribe MUST identify SKUs whose `price_minor_units` differs
from the prior snapshot by ≥10% (in either direction) and emit
exactly one classification request per affected SKU per
snapshot day.

Capability: none — internal computation

#### Scenario: Movement above threshold triggers classification

- GIVEN yesterday's snapshot has `competitor-a/sku-1` at 8990
- AND today's snapshot has `competitor-a/sku-1` at 7990
- WHEN the chief compares the two snapshots
- THEN one classification request MUST be enqueued for
  `competitor-a/sku-1` with `delta_pct = -11.1`

#### Scenario: Movement below threshold is ignored

- GIVEN yesterday's snapshot has `competitor-a/sku-1` at 8990
- AND today's snapshot has `competitor-a/sku-1` at 8500
- WHEN the chief compares the two snapshots
- THEN NO classification request MUST be enqueued
- AND a `wild.{tribe}.no-change` event MAY be published once
  per cycle for telemetry

#### Scenario: First-day snapshot has no prior to compare against

- GIVEN this is the first snapshot ever taken for `sku-1`
- WHEN the chief tries to compute movement
- THEN NO classification request MUST be enqueued
- AND the snapshot row MUST still be written
- AND a `wild.{tribe}.snapshot.first` event MUST be published
  for telemetry
