# `payment-delay` — the second ADR-0202 Procedure

Per-debtor settlement dates: the learned correction to the assumption that
everyone pays on the due date. Where the sibling `cash-forecast` estimates
the *aggregate* inflow from seasonality alone, this one takes the book you
already have and moves each receivable to the day it is expected to actually
arrive.

**Vocabulary (ADR-0202 D4).** The machinery word is `procedure`; to an
operator it is a **Procedure** (German: *Verfahren*), and its output is a
**Prognose** / **Schätzung**. `method` is the sentence on the card. An LLM
is an **Adapter** and never appears here.

## What it answers

> Voraussichtlicher Zahlungseingang für RE-2026-0032: **28.06.2026**, also
> rund 11 Tage nach Fälligkeit (17.06.) — Schätzung aus Verfahren
> *payment-delay* (Stand v1). Dieser Kunde streut allerdings stark
> (±4,8 Tage).

Two doors onto one model:

| Door | Name | Who uses it |
|---|---|---|
| `wild:function/backing` | `settlement_delay_forecast` | a declared field, via `backed_by:` (ADR-0082) |
| `wild:tool-provider/tools` | `payment-delay` | an operator or Elder, directly |

Both call the same `procedure::estimate`, so a hand check and a stored estimate
cannot disagree.

## The method, and the one judgement in it

Each debtor gets one number: the mean days between an invoice's due date and
the day it was actually paid. That mean is **shrunk** toward the pooled mean
of all debtors:

```
w = n / (n + 10)
expected_delay_days = round( w × own_mean + (1 − w) × 3.3132 )
expected_payment_date = due_date + expected_delay_days
```

`K = 10` is the only choice in the model, and it is on the audit card as a
parameter for exactly that reason. It says *"ten settled invoices is when I
start believing a debtor's own average"* — a sentence an accountant can agree
or disagree with. Everything else is a measurement.

Nothing else is in the model: no trend, no seasonality, no invoice-size term,
and no weekend or holiday snapping. An estimate can land on a Sunday; it is a
calendar date meaning "around then", not a bank value date.

## What it beats, and why that is on the card

| | holdout MAE |
|---|---|
| per-debtor, shrunk | **1.9014 days** |
| one pooled mean for everybody | 4.2987 days |

Both are measured in `the_manifest_holdout_score_is_the_measured_one`, over
the same 72 receivables, and the test asserts the first is less than *half*
the second. A holdout score with no baseline beside it is a number nobody can
judge: "1,9 Tage" sounds precise until you learn what guessing the house
average scores.

Most of the pooled model's error is not noise — it is the spread *between*
debtors. The pooled delays have a standard deviation of 5.70 days while no
individual debtor exceeds 4.8, and that gap is the whole model.

## The answer carries its own honesty

Every estimate ships two fields beyond the date:

- **`delay_stdev_days`** — this debtor's observed spread. BP-004's fitted mean
  is +11.6 days with ±4.8 around it; the partner note calls it "irregular
  payment behaviour", and a row showing only a date would present a week-wide
  guess as a day.
- **`basis`** — `own_history`, `blended`, or `no_history`. Without it a client
  seen fifteen times looks exactly like one seen a hundred and seventy.

`no_history` is the honest answer for every creditor and for any client
acquired after the fit: it returns the pooled mean and **says so**, along with
the much wider pooled spread. The alternative — the same number under
`own_history` — is confident wrongness with no error anywhere, which is what
D7 exists to prevent.

An empty `partner_id` is refused instead of answered, because a number
attached to nobody cannot be checked by anybody.

## Why it imports nothing

A fitted model carries its parameters, so it needs no clock, no socket and no
lake read. That is what keeps it inside the **ADR-0202 D10.3 determinism
profile**: re-running v1 over the same inputs has to answer the same date, or
the audit trail is a story rather than a record.

`wild:function/backing` and `wild:tool-provider/tools` are **exports**, not
imports; the profile constrains what a Procedure may reach for, not what it
offers.

## The audit card is measured, not claimed

`procedure.yaml` is the ADR-0202 D4 manifest. Every number in it is re-derived by
tests from the training extract it names:

- `the_baked_parameters_are_the_fit_of_the_shipped_extract` — re-fits every
  debtor's `n`, mean and spread, plus the pooled pair, from
  `training/settlements-2023-2026.csv`, and fails if one drifts;
- `the_manifest_holdout_score_is_the_measured_one` — recomputes both the MAE
  the card claims and the baseline it is claimed against;
- `the_audit_card_states_what_the_component_computes` — holds the card and
  the constants to EACH OTHER, per debtor.

That last one reads the card **block by block** rather than searching the
file. Six of the seven debtors share an `n`, so a whole-file `contains` would
pass with every number sitting under the wrong partner — a gate whose unit is
the file checks one thing per file.

### One entry is derived, on purpose

`expected_delay_days` is the only card value the component does not read. It
computes it from `n`, `own_mean_days` and `pooled_mean_delay_days`, because
storing a value beside its own inputs is a second hand-maintained copy of one
number. The card carries it anyway — *"shrink 4,0819 toward 3,3132 with weight
171/181"* is not a number an accountant can read — and the test above is the
join that keeps the two from drifting apart.

### Checking it by hand

`own_mean`, `n` and the pooled 3,3132 are on the card under `parameters`;
apply the two lines under **The method** above. The training extract ships
with the component, so the card can be re-checked with a spreadsheet rather
than taken on trust.

## The training extract

`training/settlements-2023-2026.csv` — 1.116 settled receivables over 186
weekly cohorts (2023-01-02 … 2026-07-20). The first 174 weeks (1.044 rows) are
the training window; the final 12 (72 rows) are held out for scoring. It is
**synthetic**, generated by `training/fit.py` from a documented per-debtor law
plus a fixed-seed LCG, so anyone can regenerate it byte-for-byte and re-derive
the same coefficients. That script also prints the parameters baked into
`src/procedure.rs`.

Its sha256 is what `procedure.yaml`'s `source_ref.digest` pins.

### The law lives here, and the tribe imports it

`HABITS`, `POOLED_HABIT` and `FIRST_ACTIVE` in `fit.py` are the generative law
of the synthetic debtors. The liquidity example's seed generator **imports**
them rather than restating them, exactly as it imports `cash-forecast`'s
inflow law — so the debtors whose invoices the estimate is shown against
behave like the ones the model was fitted to. Two copies would drift apart
silently: the chart would still render, just against different debtors.

The fit is drawn with its **own seed**. Same law, different luck. Fitting on
the very rows the estimate is later shown against would make every holdout
number a memory rather than a prediction.

### BP-007 is thin on purpose

It starts in week 157 and reaches the training window with 15 invoices, so its
weight is 0.60 and its estimate is pulled most of the way back toward the
house average. Its sample mean is +0.40 against a true habit of +1: the sample
does not carry, which is precisely what shrinkage is for. A Procedure that
reported a confident mean for a client it has seen fifteen times is lying with
a number, and that is harder to catch than lying with a word.

### Superseded versions stay re-derivable

`FITS` carries one entry per REGISTERED version and they never leave. There is
only v1 today, and the table is in place from the start for that reason: an
estimate claiming v1 produced it has to stay checkable for as long as it
stands — the training-time half of the re-run door (ADR-0202 D7.3), which is
cheap to keep and impossible to reconstruct after the fact.

## Building it

```
./build.sh          # cargo build --target wasm32-wasip2 --release
```

`sidecar.json` describes the artifact for a local install. The component is
**not** a member of the host workspace; a bare root `cargo build` never
touches it.
