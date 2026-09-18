# Reporting

The financial reporting area of the `finanzen` domain.

## Mission

Produce the recurring cash reports and evaluations over the numbers the
`liquidity-management` sibling maintains: the weekly cash report
(inflows vs. outflows, receivables aging, coverage outlook) as a
document other areas and the operator can consume.

## Scope

- Owns the `cash-report` capability offer (see `specs/cash-report.md`).
- Reads its sibling's numbers; it does not maintain accounts,
  receivables, or forecasts itself — that is `liquidity-management`'s
  duty.
