#!/usr/bin/env python3
"""Generate the fx_exposure training extract and fit its parameters.

Deterministic by construction, like both sibling Procedure: the rate history
is a documented law plus a reproducible pseudo-noise term (a fixed LCG), so
anyone can regenerate the CSV byte-for-byte and re-derive the same
coefficients.

WHAT IS FITTED, AND WHAT IS NOT — this is the Procedure that made the
distinction unavoidable.

The EUR value of a foreign-currency receivable is `amount / rate`. That is
arithmetic; nothing is learned and nothing needs a model. What IS learned is
how far that rate typically MOVES between today and the day the money
actually arrives — one volatility per currency pair. The answer is therefore
not a number but a band, and the band is what a treasurer can act on:
"rund 41.000 €, zwischen 39.800 und 42.300, wenn sie wie erwartet in 26 Tagen
zahlen".

Design:
  - 1970 business days of EUR reference rates for USD, GBP, CHF.
  - Train on the first 1500; hold out the rest.
  - Model: daily log-return volatility σ per currency. The 80 % band over d
    days is ±1.2816 × σ × √d (a random walk with no drift term).
  - Score COVERAGE, not error: the share of actual d-day moves that land
    inside the band. A band model is right when it is CALIBRATED — 0.80 is
    the target, and both a too-wide and a too-narrow band are wrong.

The drift term is deliberately absent. A fitted drift on four years of one
currency pair is indistinguishable from noise, and a Procedure that claims to
know which way the dollar is going is exactly the kind of confident wrongness
ADR-0202 D7 exists to prevent. The band says "I do not know the direction",
which is the true statement.
"""

from datetime import date, timedelta
from decimal import Decimal, ROUND_HALF_UP
from math import exp, log, sqrt
from pathlib import Path

# A Monday, and DELIBERATELY earlier than the sibling fits' 2023 epoch.
#
# A coverage score needs independent observations, and overlapping 21-day
# windows are not independent: 230 holdout days give roughly TEN effective
# samples per currency, which is too few to score anything — a coverage of
# 0.68 and one of 0.80 are the same measurement at that sample size. A rate
# history is also naturally longer than an invoice book, so there is no
# reason to inherit the book's epoch. Whatever this window measures is what
# the card says; the window was widened because the ESTIMATOR was too noisy
# to report, not until the number came out well.
START = date(2019, 1, 2)

FITS = {
    # 1970 business days ends just before the tribe's own cut-off
    # (2026-07-31). A rate history running past that date would be a model
    # scored on quotes it had already read.
    "v1": {"days": 1970, "train": 1500, "snapshot": "eurofx-2019-2026"},
}
LIVE = "v1"

# The synthetic market: a starting rate and a TRUE daily volatility per
# currency, quoted the way the ECB quotes them — units of foreign currency
# per ONE euro. So a RISING rate means the euro strengthened and a euro-based
# receivable in that currency is worth LESS.
#
# The three are the ones a German Mittelstand book actually carries, and the
# spread between them is the point: CHF barely moves against the euro, GBP
# moves noticeably more, USD most of all. A single pooled volatility would
# price a Swiss franc receivable like a dollar one.
MARKETS = {
    "USD": {"start": 1.0850, "sigma": 0.0045},
    "GBP": {"start": 0.8580, "sigma": 0.0035},
    "CHF": {"start": 0.9420, "sigma": 0.0022},
}

# The horizon the holdout is scored at, in calendar days. 30 is the shape of
# the question this Procedure is asked: a receivable due next month.
SCORE_HORIZON_DAYS = 30

# The two-sided 80 % z-score. 80 % rather than 95 % on purpose: a 95 % band on
# a 30-day dollar move is so wide it tells a treasurer nothing they can plan
# with, and a band nobody acts on is a band nobody checks.
Z_80 = 1.2816

# What the baseline does instead: one fixed ±2 % band for every currency at
# every horizon — the "just add a safety margin" rule this replaces.
BASELINE_BAND = 0.02

EXTRACT_SEED = 20260813


class Lcg:
    def __init__(self, seed): self.s = seed
    def next_unit(self):
        self.s = (1664525 * self.s + 1013904223) % (2**32)
        return self.s / 2**32


def normal(rng):
    """A standard normal from twelve uniforms (the CLT trick).

    Deliberately not Box-Muller: this uses exactly twelve draws every time, so
    the sequence a caller sees does not depend on how many values were
    rejected, and the extract stays byte-reproducible across Python versions.
    """
    return sum(rng.next_unit() for _ in range(12)) - 6.0


def business_days(n):
    """`n` weekdays from START. FX reference rates are published on business
    days only, and pretending otherwise would put a rate on a day no market
    ever quoted one."""
    out, d = [], START
    while len(out) < n:
        if d.weekday() < 5:
            out.append(d)
        d += timedelta(days=1)
    return out


def stdev(xs):
    m = sum(xs) / len(xs)
    return sqrt(sum((x - m) ** 2 for x in xs) / (len(xs) - 1))


def rate_6dp(x):
    return Decimal(str(x)).quantize(Decimal("0.000001"), rounding=ROUND_HALF_UP)


def main():
    import sys
    version = sys.argv[1] if len(sys.argv) > 1 else LIVE
    if version not in FITS:
        raise SystemExit(f"unknown fit {version!r}; known: {', '.join(FITS)}")
    fit = FITS[version]
    DAYS, TRAIN, SNAPSHOT = fit["days"], fit["train"], fit["snapshot"]
    print("VERSION", version)
    print("SNAPSHOT", SNAPSHOT)

    dates = business_days(DAYS)
    rng = Lcg(EXTRACT_SEED)
    series = {}
    for ccy in sorted(MARKETS):
        m = MARKETS[ccy]
        r, path = m["start"], []
        for _ in range(DAYS):
            path.append(r)
            r *= exp(m["sigma"] * normal(rng))
        series[ccy] = path

    # ── write the extract: one row per (date, currency) ──
    out = ["rate_date,currency,rate_per_eur"]
    for i, d in enumerate(dates):
        for ccy in sorted(MARKETS):
            out.append(f"{d.isoformat()},{ccy},{rate_6dp(series[ccy][i])}")
    csv = "\n".join(out) + "\n"

    # ── fit: daily log-return volatility per currency, on the train window ──
    params = {}
    for ccy in sorted(MARKETS):
        p = series[ccy][:TRAIN]
        rets = [log(p[i + 1] / p[i]) for i in range(len(p) - 1)]
        params[ccy] = {"sigma": stdev(rets), "n": len(rets)}

    # ── score the holdout: COVERAGE of the d-day band ──
    # `SCORE_HORIZON_DAYS` is calendar days; the series is business days, so
    # the step is the business-day count that spans it. Getting this wrong
    # would inflate the band by ~40 % and the coverage with it.
    step = round(SCORE_HORIZON_DAYS * 5 / 7)
    hits = base_hits = total = 0
    per_ccy = {}
    for ccy in sorted(MARKETS):
        p = series[ccy][TRAIN:]
        band = Z_80 * params[ccy]["sigma"] * sqrt(step)
        h = b = n = 0
        for i in range(len(p) - step):
            move = log(p[i + step] / p[i])
            n += 1
            h += abs(move) <= band
            b += abs(move) <= log(1 + BASELINE_BAND)
        per_ccy[ccy] = (h / n, b / n, n)
        hits, base_hits, total = hits + h, base_hits + b, total + n

    coverage = hits / total
    base_coverage = base_hits / total

    print("CSV_ROWS", len(dates) * len(MARKETS), "TRAIN_DAYS", TRAIN, "HOLDOUT_DAYS", DAYS - TRAIN)
    print("STEP_BUSINESS_DAYS", step)
    print("COVERAGE", round(coverage, 4), "BASELINE_COVERAGE", round(base_coverage, 4),
          "TARGET", 0.80, "WINDOWS", total)
    print("TRAIN_START", dates[0].isoformat(), "TRAIN_END", dates[TRAIN - 1].isoformat())
    print("HOLDOUT_START", dates[TRAIN].isoformat(), "HOLDOUT_END", dates[-1].isoformat())
    for ccy in sorted(MARKETS):
        cov, base, _n = per_ccy[ccy]
        print("CCY", ccy, "sigma", f'{params[ccy]["sigma"]:.6f}',
              "true_sigma", f'{MARKETS[ccy]["sigma"]:.6f}',
              "n", params[ccy]["n"],
              "band_30d_pct", f'{100 * (exp(Z_80 * params[ccy]["sigma"] * sqrt(step)) - 1):.2f}',
              "coverage", f"{cov:.4f}", "baseline_coverage", f"{base:.4f}")

    out_path = Path(__file__).with_name(f"{SNAPSHOT}.csv")
    out_path.write_text(csv)
    print("WROTE", out_path.name)


if __name__ == "__main__":
    main()
