# Delta on `briefs/price-change-detection.md`

## APPEND: Operator's local context

- Competitor B's currency-flip pattern is now spec-enforced (see
  `changes/add-currency-aware-classification/specs/price-change-detection.md`).
  This means a `currency-flip` event is silent-by-design — it's a
  filter, not an anomaly. If the operator sees ten currency-flip
  events in a week, that's a structural signal about competitor
  B's pricing engineering team, not noise.

- Reflect cycle observation (2026-05-15): three phantom alerts
  in 30 days were the trigger to land this filter. Track the
  rate of `currency-flip` events the same way; if the rate
  drops to zero, competitor B has stabilised — surface that
  as an "evolve" candidate (operator may want to *re-enable*
  classification on currency flips, since they may now be
  signal rather than noise).
