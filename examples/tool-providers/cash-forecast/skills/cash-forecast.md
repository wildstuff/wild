---
name: cash-forecast
version: 0.1.0
source: component
component_type: cash-forecast
method: cash-forecast
description: >-
  Expected cash inflow for a date some days ahead, from Procedure
  `cash_forecast` v4 — a seasonal model fitted to three years of weekly
  payment inflows (holdout MAPE 0.0353). Ask it for "what do we expect to
  come in around <date>". The answer is an ESTIMATE, never a booked
  figure, and it carries the model version that produced it.
args_schema:
  type: object
  properties:
    as_of:
      type: string
      description: ISO date (YYYY-MM-DD) the forecast is made from.
    horizon_days:
      type: integer
      minimum: 0
      maximum: 365
      default: 30
      description: Days ahead to forecast. Defaults to 30.
  required: [as_of]
returns_schema:
  type: object
  properties:
    target_date:
      type: string
    expected_inflow:
      type: string
    week_bucket:
      type: integer
    procedure_version:
      type: string
---

# cash-forecast

Answers *"was erwarten wir an Zahlungseingängen zum <Datum>?"* with one
number and the Procedure that produced it.

## What it is

A **Procedure** — a fitted artifact, not a formula someone typed. It was
trained on 156 weekly inflow totals (2023-01-02 … 2025-12-22) and scored
against the 52 weeks that followed; its holdout MAPE is 0.0353. The
parameters are a baseline (€42,068.98) and one factor per weekly bucket,
all of them in the binary. It reads no clock and no lake, so re-running v4
over the same inputs answers the same number today and in three years.

## Saying it to an operator

Name the Procedure and its standing, not the mechanism:

> Erwarteter Zahlungseingang zum 05.02.2025: **rund 45.100 €** — Schätzung
> aus Verfahren *cash-forecast* (Stand v4, Güte MAPE 0,0353).

Two things must survive into whatever you say:

- it is a **Schätzung**, not a booked figure;
- the **version** — a later refit changes future estimates and does not
  rewrite the ones already reported.

## What it will not answer

- More than **365 days** ahead. A seasonal fit over one year of buckets
  says nothing useful past that, so the call is refused rather than
  answered confidently and wrongly.
- A date it cannot read. `as_of` must be ISO `YYYY-MM-DD`; `07.01.2025` is
  refused rather than guessed at.

## The date it answers about

`target_date` is the date the estimate is **about** (`as_of` plus the
horizon), never the day it ran. That is what lets a dated chart over the
stored estimates answer *"was sagte die Prognose zum 31.03.?"*.

## Checking it by hand

`expected_inflow = 42068.98 × factor[week_bucket]`, rounded to cents. The
factor table and the training extract ship with the component, and its
tests re-derive every factor from that extract — so the audit card can be
re-checked with a spreadsheet rather than taken on trust.
