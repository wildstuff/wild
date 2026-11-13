# Reporting

The financial reporting tribe of the `finanzen` area.

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

## Success

The yardstick is the report itself: the operator and the other tribes of the
area can rely on it. Each cycle's output is scored against these checks:

- report-published: judge The weekly cash report is published as a document covering inflows against outflows, receivables aging and the coverage outlook.
- numbers-from-the-sibling: judge The figures come from what `liquidity-management` maintains, and this tribe changes no account, receivable or forecast.
- threshold: 0.7
