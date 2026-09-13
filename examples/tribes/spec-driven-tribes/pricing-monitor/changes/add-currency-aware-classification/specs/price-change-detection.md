# Delta on `specs/price-change-detection.md`

## ADDED Requirements

### Requirement: Suppress classification on currency flip

The tribe MUST NOT enqueue a classification request when the
prior snapshot's `currency` differs from the current
snapshot's `currency` for the same `(competitor_id, sku_id)`.

Capability: none — internal computation

#### Scenario: Currency flip suppresses classification

- GIVEN yesterday's snapshot has
  `competitor-b/sku-3 = {price_minor_units: 8990, currency: "EUR"}`
- AND today's snapshot has
  `competitor-b/sku-3 = {price_minor_units: 8990, currency: "USD"}`
- WHEN the chief compares the two snapshots
- THEN NO classification request MUST be enqueued
- AND a `wild.{tribe}.detection.currency-flip` event MUST be
  published with `competitor_id`, `sku_id`, `prior_currency`,
  `current_currency`

#### Scenario: Same currency, real movement still triggers

- GIVEN yesterday's snapshot has
  `competitor-b/sku-3 = {price_minor_units: 8990, currency: "EUR"}`
- AND today's snapshot has
  `competitor-b/sku-3 = {price_minor_units: 4490, currency: "EUR"}`
- WHEN the chief compares the two snapshots
- THEN one classification request MUST be enqueued for
  `competitor-b/sku-3` with `delta_pct = -50.0`
- AND NO `currency-flip` event MUST be published
