---
name: payment-delay
version: 0.1.0
source: component
component_type: payment-delay
method: payment-delay
description: >-
  When a receivable will actually be paid, from Procedure `payment_delay`
  v1 — one mean delay per debtor, fitted to three and a half years of
  settled invoices and shrunk toward the house average where the history is
  thin (holdout MAE 1.9014 days, against 4.2987 for a single pooled mean). Ask it
  for "wann kommt diese Rechnung wirklich rein". The answer is an ESTIMATE,
  never a promise, and it carries the spread it was drawn from.
args_schema:
  type: object
  properties:
    partner_id:
      type: string
      description: The debtor the invoice is owed by.
    due_date:
      type: string
      description: ISO date (YYYY-MM-DD) the invoice is contractually due.
  required: [partner_id, due_date]
returns_schema:
  type: object
  properties:
    expected_payment_date:
      type: string
    expected_delay_days:
      type: integer
    delay_stdev_days:
      type: string
    basis:
      type: string
    procedure_version:
      type: string
---

# payment-delay

Answers *"wann zahlt dieser Kunde wirklich?"* with a date, the spread behind
it, and how much of it rests on that customer's own record.

## What it is

A **Procedure** — a fitted artifact, not a formula someone typed. Bound as
the Function backing `settlement_delay_forecast`, so a declared field can
name it with `backed_by:` and the boundary write path fills the estimate.

It learned one number per debtor: the mean days between an invoice's due
date and the day it was actually paid, over 1.044 settled receivables
(2023-01-02 … 2026-04-27). That mean is **shrunk** toward the pooled mean of
all debtors with weight `n / (n + 10)` — a debtor seen fifteen times gets
pulled most of the way back to the house average, one seen a hundred and
seventy is trusted on its own. Scored on the 72 receivables that followed:
MAE **1.9014 days**, against **4.2987** for the baseline of one pooled mean
for everybody — the comparison is what makes the first number mean anything,
and both are measured in the same test.

It reads no clock and no lake, so re-running v1 over the same inputs answers
the same date today and in three years.

## Saying it to an operator

Name the Procedure and its standing, not the mechanism — and never drop the
spread:

> Voraussichtlicher Zahlungseingang für RE-2026-0032: **28.06.2026**, also
> rund 11 Tage nach Fälligkeit (17.06.) — Schätzung aus Verfahren
> *payment-delay* (Stand v1). Dieser Kunde streut allerdings stark
> (±4,8 Tage).

Three things must survive into whatever you say:

- it is a **Schätzung**, not a promise;
- the **version** — a later refit changes future estimates and does not
  rewrite the ones already reported;
- the **spread**, whenever it is wide. A debtor with a well-fitted mean of
  +12 days and a spread of ±4,8 is not "pays on the 14th", it is "pays
  somewhere in that week, probably".

## Read `basis` before you trust the number

Every answer says what carries it:

- `own_history` — this debtor's own record dominates the estimate. Report it
  plainly.
- `blended` — too little history, so the estimate is pulled toward the house
  average. Say so: *"noch wenig Zahlungshistorie, daher am Hausdurchschnitt
  orientiert"*.
- `no_history` — the training window never saw this partner, and the answer
  IS the pooled mean. It is a placeholder with a date on it. Never present
  it as a customer-specific expectation.

`no_history` is the normal answer for a client acquired after the fit, and
for every creditor — the model learned about debtors only.

## What it will not answer

- An invoice with no partner. An empty `partner_id` is refused rather than
  answered with the house average, because a number attached to nobody
  cannot be checked by anybody.
- A date it cannot read. `due_date` must be ISO `YYYY-MM-DD`; `03.08.2026`
  is refused rather than guessed at.

## The date it answers about

`expected_payment_date` is the date the estimate is **about** (the due date
plus the fitted delay), never the day it ran. It is a **calendar** date: no
weekend or holiday snapping, so an estimate can land on a Sunday. Treat it
as "around then", not as a bank-value date.

## Checking it by hand

```
expected_delay_days = round( w × own_mean + (1 − w) × 3.3132 ),  w = n / (n + 10)
expected_payment_date = due_date + expected_delay_days
```

`own_mean`, `n` and the pooled 3,3132 are on the audit card under
`parameters`; the training extract ships with the component and its tests
re-derive every fitted number from that extract — so the card can be
re-checked with a spreadsheet rather than taken on trust.
