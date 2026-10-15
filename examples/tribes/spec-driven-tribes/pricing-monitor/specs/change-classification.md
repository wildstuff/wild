# Change classification

## Purpose

Given a flagged price movement, label it as one of `flash-sale`,
`restock`, `margin-cut`, or `unclear` so the alert message
carries actionable context for the operator.

## Requirements

### Requirement: One label per movement

For every classification request enqueued by
`price-change-detection`, the tribe MUST produce exactly one
classification result.

Capability: `llm.classify` (single-turn LLM call)

#### Scenario: Down-move with high prior volatility

- GIVEN a classification request for `competitor-a/sku-1` with
  `delta_pct = -11.1`
- AND the prior 30 days for this SKU show 4 movements ≥ 5%
- WHEN the classifier runs
- THEN the result MUST be one of `flash-sale` or `unclear`
- AND it MUST NOT be `restock` (restock implies an up-move)

#### Scenario: Up-move classified as restock or margin-correction

- GIVEN a classification request for `competitor-b/sku-3` with
  `delta_pct = +18.0`
- AND `state/raw/<date>/competitor-b-sku-3.html` shows the SKU
  page contains the phrase "Wieder verfügbar" or equivalent
- WHEN the classifier runs
- THEN the result MUST be `restock`

### Requirement: Classification result includes a reason string

Every classification result MUST carry a `reason` field — one
sentence in plain prose explaining the label so the alert
message can quote it directly.

Capability: none — internal field shape

#### Scenario: Reason field present and non-empty

- GIVEN any classification result
- THEN `result.reason` MUST be a string
- AND it MUST be at least 10 characters
- AND it MUST NOT contain "TODO" or "n/a"

### Requirement: Cap on classification per cycle

To prevent runaway LLM cost when many SKUs move at once, the
tribe MUST cap classifications at 10 per cycle. If more than
10 SKUs move, the lowest-magnitude movements (smallest
`|delta_pct|`) MUST be deferred to the next cycle.

Capability: none — internal scheduling

#### Scenario: 12 SKUs move, 10 are classified

- GIVEN a snapshot day with 12 SKUs flagged by detection
- WHEN the chief runs classification
- THEN exactly 10 classification results MUST be produced this
  cycle
- AND the 2 deferred SKUs MUST be the ones with smallest
  `|delta_pct|` among the 12
- AND a `wild.{tribe}.classification.deferred` event MUST be
  published with the slugs of the deferred SKUs
