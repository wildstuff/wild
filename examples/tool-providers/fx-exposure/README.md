# `fx-exposure` — the two-axis ADR-0202 Procedure

What a foreign-currency receivable is worth in euro **by the time it settles**.
The euro amount is `amount / rate`, which is arithmetic; what is *learned* is
how far that rate typically moves in the meantime. So the answer is a **band**,
and the band is the part a treasurer can plan against.

**Vocabulary (ADR-0202 D4).** The machinery word is `procedure`; to an
operator it is a **Procedure** (German: *Verfahren*), and its output is a
**Prognose** / **Schätzung**. `method` is the sentence on the card. An LLM is
an **Adapter** and never appears here.

## What it answers

> Die USD-Rechnung RE-2026-0051 über 12.000,00 $ ist bei einem Kurs von
> 1,0850 (Stand 21.07.2026) rund **11.059,91 €** wert — realistisch zwischen
> 10.770,68 € und 11.356,90 €, wenn sie wie erwartet in 30 Tagen zahlt.
> Schätzung aus Verfahren *fx-exposure* (Stand v1).

Two doors onto one model:

| Door | Name | Who uses it |
|---|---|---|
| `wild:function/backing` | `fx_exposure_valuation` | a declared field, via `backed_by:` (ADR-0082) |
| `wild:tool-provider/tools` | `fx-exposure` | an operator or Elder, directly |

Both call the same `procedure::value`, so a hand check and a stored valuation
cannot disagree.

## Why this one has two axes

Its siblings answer from their inputs alone. `cash_forecast` and
`payment_delay` are functions of what you hand them, so **version +
parameters** is the whole story and their audit cards are complete on their
own.

This one is anchored to a market rate that came from outside, on a particular
morning. **The same model, the same invoice and yesterday's rate is a
different number — correctly.** Reproducing a stored valuation therefore needs
two things, and only one of them lives on the card:

1. **the model** — `version` + the three volatilities, re-derivable from the
   extract the card names;
2. **the anchor** — the rate and the date it was quoted, which arrive per call
   and are recorded on the **row**.

Which is why `rate_date` rides back **out** of every answer even though the
caller supplied it. Without that echo the second axis exists only in whatever
called the model, and the stored row cannot say what it was anchored to.
`the_answer_carries_the_snapshot_it_was_anchored_to` values the same invoice at
two rates and requires the answers to differ — if they ever matched, the second
axis would be decoration.

## It may not fetch its own rate, and that is the feature

This is the component in the family with the most obvious reason to open a
socket. It cannot: ADR-0202 **D10.3**'s import allow-list carries no
`wasi:sockets`, no `wasi:http` and no `wild:http`. The impure half has to be a
source, and the rate has to arrive as a field.

A model that fetched its own inputs could answer differently on two runs with
nothing recorded to explain the difference. Because the rate arrives as data,
the snapshot is on the row where an auditor can find it three years later.
`the_procedure_that_wants_the_network_most_still_imports_nothing` asserts the
empty `requires` where it can actually fail.

In the liquidity example the impure half is the `ecb-fx` source, which needs no
connector of its own: an `https` locator resolves to the bundled
`web-connector`, and the **host** owns the socket — it parses the URL, enforces
the default-deny `system/egress.yaml`, applies the timeout and writes the
`audit.egress` row (ADR-0090).

## The method, and the two judgements in it

```
band = 1.2816 × sigma × sqrt(round(days × 5/7))
expected_eur = amount / rate
low = amount / (rate × e^band)      high = amount / (rate × e^-band)
```

`sigma` is fitted. Two things are **not**, and both sit on the card as
parameters so they can be argued with:

- **80 %, not 95 %.** A 95 % band on a 30-day dollar move is too wide to plan
  against, and a band nobody acts on is a band nobody checks. 80 % means "one
  month in five will land outside this".
- **five trading days per seven calendar days.** Rates are quoted on business
  days, so a 30-day horizon spans 21 moves. Getting this wrong inflates every
  band by ~19 % and the coverage with it.

There is deliberately **no drift term**. A direction fitted on seven years of
one currency pair is indistinguishable from noise; the band says "I do not know
which way", which is the true statement.

The euro band is **not symmetric** about the middle — the rate moves
multiplicatively and the conversion inverts it, so the upside is wider. That is
asserted, because it is the easiest property to get wrong by hand.

## COVERAGE, not error — and the aggregate is deliberately unflattering

A band model is right when it is **calibrated**, so both directions are wrong:
a band covering 95 % is not better than one covering 80 %, it is useless.

| | holdout coverage |
|---|---|
| fitted, per currency | **0.7595** |
| one flat ±2 % band | 0.7491 |

Read alone that says the Procedure is barely worth having. The per-currency
split is what it actually says:

| | fitted | flat ±2 % |
|---|---|---|
| CHF | 0.7884 | 0.9577 — a margin so wide it states nothing |
| GBP | 0.7817 | 0.7751 |
| USD | 0.7082 | 0.5145 — a treasurer surprised half the time |

**The aggregate cannot tell them apart, and the test asserts that it cannot.**
If the two ever separate in aggregate, the argument for reading per currency is
gone and this table is wrong. Same shape as `payment_delay`: the pooled rule is
right on average and wrong for every individual case.

USD at 0.7082 is a real shortfall against the 0.80 target, and the card says so
in as many words. One realisation of a random walk can produce it; hiding it
behind the aggregate would make the card a story.

**The holdout window was widened once**, from 230 business days to 470, and the
reason was principled rather than cosmetic: overlapping 21-day windows on 230
days give roughly *ten* effective samples per currency, at which sample size a
coverage of 0.68 and one of 0.80 are the same measurement. The estimator was
too noisy to report. It was not re-tuned afterwards — the numbers above are
what the wider window gave.

## Refusing rather than averaging

An unfitted currency is **refused**. That is deliberately unlike
`payment-delay`, whose unknown debtor gets the pooled mean under `no_history`:
there, a pooled answer is a real statement about a real population. Here the
franc and the dollar differ by a factor of two, so any pooled volatility is
wrong for both — and a band that is wrong is worse than no band, because it
will be planned against.

A euro receivable is answered as itself: value equals amount, band zero,
`basis: no_fx_risk`. A missing `rate_date` is refused rather than carried
through blank, because the date **is** the second audit axis.

Money crosses the boundary as **text** and a JSON number is refused rather than
coerced — the tempting alternative loses cents silently through an `f64`.

## The audit card is measured, not claimed

- `the_baked_parameters_are_the_fit_of_the_shipped_extract` — re-derives every
  volatility from `training/eurofx-2019-2026.csv`;
- `the_manifest_holdout_score_is_the_measured_one` — recomputes the coverage,
  the flat-band baseline, and every per-currency figure in the table above;
- `the_audit_card_states_what_the_component_uses` — holds the card and the
  constants to each other, per currency, reading the card block by block.

### The training extract

1.970 business days × 3 currencies (2019-01-02 … 2026-07-21). The first 1.500
days are the training window; the last 470 are held out. It is **synthetic**,
generated by `training/fit.py` from a documented per-currency law plus a
fixed-seed LCG, so anyone can regenerate it byte-for-byte and re-derive the
same coefficients. Its sha256 is what `procedure.yaml`'s `source_ref.digest`
pins.

It starts earlier than its siblings' 2023 epoch on purpose — see the widened
window above — and stops before the tribe's own cut-off, because a rate history
running past that date would be a model scored on quotes it had already read.

## Building it

```
./build.sh          # cargo build --target wasm32-wasip2 --release
```

`sidecar.json` describes the artifact for a local install. The component is
**not** a member of the host workspace; a bare root `cargo build` never touches
it.
