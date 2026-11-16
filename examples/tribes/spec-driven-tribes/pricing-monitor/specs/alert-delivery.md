# Alert delivery

## Purpose

Push one alert per classified movement to the operator's
configured Telegram channel within 60 seconds of the
classification result landing.

## Requirements

### Requirement: One alert per classified movement

The tribe MUST publish one Telegram message per
classification result, identified uniquely by
`(snapshot_date, competitor_id, sku_id)`.

Capability: `notify.operator` (Telegram channel adapter)

#### Scenario: Classified movement triggers alert

- GIVEN a classification result for `competitor-a/sku-1` with
  `label: "flash-sale"` and `delta_pct: -11.1`
- WHEN the chief publishes the alert
- THEN exactly one Telegram message MUST be sent to the
  configured chat-id
- AND the message body MUST include the SKU name, the delta
  percent, the classification label, and the reason string
- AND a `wild.{tribe}.alert.sent` event MUST be published
  with the alert id

#### Scenario: Same SKU same day, no second alert

- GIVEN an alert for `competitor-a/sku-1` was already sent
  today (2026-05-08)
- AND a second classification result for the same SKU lands
  the same day
- WHEN the chief considers the second alert
- THEN NO second Telegram message MUST be sent
- AND a `wild.{tribe}.alert.dedup` event MUST be published
  with the alert id that was suppressed

### Requirement: 60-second SLA

The latency from classification-result-landed to
Telegram-publish MUST be ≤ 60 seconds at p99.

Capability: none — operational SLO

#### Scenario: SLA reflected in telemetry

- GIVEN a successful alert delivery
- THEN `wild.{tribe}.alert.sent` MUST carry an
  `elapsed_ms` field measuring the gap from
  classification-result write to Telegram publish

### Requirement: Severity gating

The tribe MUST honour the operator's severity-filter
configuration: only `critical` movements trigger
notifications when configured to `critical-only`.

Capability: none — config read

#### Scenario: Below-critical movements do not alert

- GIVEN the operator has set `severity_filter: "critical-only"`
- AND a classification result lands with `label: "unclear"`
- WHEN the chief considers the alert
- THEN NO Telegram message MUST be sent
- AND a `wild.{tribe}.alert.dropped-by-severity` event MUST
  be published
