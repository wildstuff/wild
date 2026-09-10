---
name: fx-exposure
version: 0.1.0
source: component
component_type: fx-exposure
method: fx-exposure
description: >-
  What a foreign-currency receivable is worth in euro by the time it settles,
  from Procedure `fx_exposure` v1 — one volatility per currency, fitted to
  seven years of daily reference rates (holdout band coverage 0.7595 against
  a 0.80 target). Ask it for "was ist diese Dollar-Rechnung wirklich wert".
  The answer is a BAND, never a single number, and it is only defined
  relative to the rate snapshot it is given.
args_schema:
  type: object
  properties:
    currency:
      type: string
      description: ISO 4217 code (CHF, GBP, USD; EUR is answered as itself).
    amount_foreign:
      type: string
      description: Invoice amount in that currency.
    rate_per_eur:
      type: string
      description: Units of the currency per ONE euro (the ECB convention).
    rate_date:
      type: string
      description: ISO date the rate was quoted.
    days_to_settlement:
      type: integer
      minimum: 0
      description: Calendar days until the money is expected to arrive.
  required: [currency, amount_foreign, rate_per_eur, rate_date, days_to_settlement]
returns_schema:
  type: object
  properties:
    expected_eur:
      type: string
    band_low_eur:
      type: string
    band_high_eur:
      type: string
    band_pct:
      type: string
    rate_date:
      type: string
    basis:
      type: string
    procedure_version:
      type: string
---

# fx-exposure

Answers *"was bringt diese Fremdwährungsrechnung am Ende wirklich?"* with a
range, the rate it was anchored to, and the day that rate was quoted.

## What it is

A **Procedure** — a fitted artifact, not a formula someone typed. Bound as the
Function backing `fx_exposure_valuation`.

The euro amount is `amount / rate`. That part is arithmetic and nothing is
learned. What is **fitted** is one daily volatility per currency — how far
that rate typically moves — over 1.500 business days of daily reference rates.
The 80 % band over d calendar days is the rate times
`exp(±1.2816 × sigma × sqrt(round(d × 5/7)))`.

There is deliberately **no drift term**. A direction fitted on seven years of
one currency pair is indistinguishable from noise, so the Procedure does not
claim to know which way the dollar is going. The band says "I do not know the
direction", which is the true statement.

## Never report the middle without the band

This is the one rule that matters for this tool. `expected_eur` alone reads as
a fact; it is the centre of a range that is 2,7 % wide for a dollar invoice a
month out.

> Die USD-Rechnung RE-2026-0051 über 12.000,00 $ ist bei einem Kurs von
> 1,0850 (Stand 21.07.2026) rund **11.059,91 €** wert — realistisch zwischen
> 10.770,68 € und 11.356,90 €, wenn sie wie erwartet in 30 Tagen zahlt.
> Schätzung aus Verfahren *fx-exposure* (Stand v1).

Four things must survive into whatever you say:

- it is a **Schätzung**, not a booked figure;
- the **band**, always — see above;
- the **version**;
- the **rate and its date**. Which brings us to the part that makes this
  Procedure different from its siblings.

## The answer is only true against one snapshot

`cash-forecast` and `payment-delay` answer from their inputs alone: same
inputs, same answer, for ever. This one is anchored to a market rate that came
from somewhere else, on a particular morning. **The same model, the same
invoice and yesterday's rate is a different number — correctly.**

So `rate_date` comes back with every answer, and it belongs in what you say.
"Rund 11.060 €" without "beim Kurs vom 21.07." is an assertion nobody can
check later, including you.

If the rate you were given is old, say so. The Procedure has no clock and
cannot tell — it will value a receivable against a rate from last year without
complaint, because refusing would require it to know today's date, which is
exactly the kind of hidden input that makes an answer irreproducible.

## Read `basis`

- `fitted` — a fitted volatility carried the band. The normal case.
- `no_fx_risk` — a euro receivable. Value equals amount, band is zero. Say
  "keine Währungsrisiko-Komponente", not "±0 %".

## What it will not answer

- **A currency it has no fit for.** CHF, GBP and USD only; anything else is
  refused. It deliberately does NOT fall back to an average the way
  `payment-delay` falls back to a pooled debtor mean — the franc and the
  dollar differ by a factor of two, so any pooled volatility is wrong for
  both, and a band that is wrong is worse than no band because it will be
  planned against.
- **A rate it cannot read**, or one that is zero or negative.
- **A missing `rate_date`.** Required, never guessed: the date is half the
  audit answer, and a valuation anchored to an unrecorded snapshot cannot be
  re-checked.

## Checking it by hand

```
expected_eur = amount / rate
band         = 1.2816 × sigma × sqrt(round(days × 5/7))
low          = amount / (rate × e^band)      high = amount / (rate × e^-band)
```

The three `sigma` values are on the audit card under
`parameters.currencies`; the rate history ships with the component and its
tests re-derive every volatility from it — so the card can be re-checked with
a spreadsheet rather than taken on trust.
