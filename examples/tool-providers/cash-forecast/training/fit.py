#!/usr/bin/env python3
"""Generate the cash_forecast training extract and fit its parameters.

Deterministic by construction: the series is a documented formula plus a
reproducible pseudo-noise term (a fixed LCG), so anyone can regenerate the
CSV byte-for-byte and re-derive the same coefficients.

Design:
  - 156 weekly inflow totals (3 years), ISO-ish week buckets 1..52.
  - Train on the first 104 weeks (2 samples per week-of-year).
  - Hold out the final 52 weeks (year 3) and score MAPE.
  - Model: expected = baseline * seasonal_factor[week_of_year]

`BASE_EUR` + `true_factor` + `week_of_year` + `Lcg` are the GENERATIVE LAW of
the synthetic business — what the extract is drawn from, as opposed to the
fitted `parameters` the Procedure carries, which are this law estimated from a
sample. The liquidity example's seed generator IMPORTS them from here
(`examples/tribes/liquidity-management/data/regenerate-seed.py`) rather than
restating the numbers, so the tribe whose inflows the forecast is shown against
is drawn from the same business the model was fitted to. Two copies of a law
would drift apart silently — the chart would still render, just against a
different world.
"""

from datetime import date, timedelta
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

START = date(2023, 1, 2)  # a Monday

# One entry per REGISTERED Procedure version, and they never leave: v3's
# parameters have to stay re-derivable for as long as a stored estimate claims
# v3 produced it. That is the same promise the re-run door makes at the
# artifact level (ADR-0202 D7.3) — this is its training-time half.
#
# The series is one law with one seed walked in one order, so a longer window
# is a SUPERSET of a shorter one: the first 156 rows of v4's extract are v3's
# extract, byte for byte. The refit is genuinely "a year more of the same
# business", not a different world drawn under a new name.
FITS = {
    # 3 years; 2 samples per week-of-year bucket, 1 year held out.
    "v3": {"weeks": 156, "train": 104, "snapshot": "inflows-2023-2025"},
    # 4 years; 3 samples per bucket, still 1 year held out. Same method — only
    # the estimation noise in each seasonal factor shrinks.
    "v4": {"weeks": 208, "train": 156, "snapshot": "inflows-2023-2026"},
}
LIVE = "v4"

# The synthetic business's mean weekly inflow, before seasonality and noise.
# NOT the same number as the fitted `baseline_eur` in `procedure.yaml` (42308.60):
# that one is the MEAN OF A SAMPLE of this law, and the small gap between them
# is the whole reason a holdout score exists.
BASE_EUR = 42000.0

# Seed of the extract's noise draw. The seed generator uses its own, so the two
# series are the same law with different luck rather than the same numbers.
EXTRACT_SEED = 20260807

# Deterministic pseudo-noise: a plain LCG, seeded once.
class Lcg:
    def __init__(self, seed): self.s = seed
    def next_unit(self):
        # Numerical Recipes constants; returns [0,1)
        self.s = (1664525 * self.s + 1013904223) % (2**32)
        return self.s / 2**32

def week_of_year(i):
    """1..52 bucket for the i-th week since START."""
    return (i % 52) + 1

def true_factor(w):
    """The seasonal shape the synthetic business actually has: a mild
    year-end peak and a summer trough."""
    import math
    return 1.0 + 0.18 * math.cos(2 * math.pi * (w - 50) / 52.0)

def money(x):
    return Decimal(str(x)).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)

def weekly_inflow(w, unit):
    """The law: baseline × seasonal shape × a ±6% multiplicative draw.

    `unit` is a [0,1) draw — passed in rather than taken from a module-level
    generator so a caller walking weeks in a different order (the seed
    generator does) gets the same shape without inheriting this file's
    sequence.
    """
    return BASE_EUR * true_factor(w) * (0.94 + 0.12 * unit)


def main():
    import sys
    version = sys.argv[1] if len(sys.argv) > 1 else LIVE
    if version not in FITS:
        raise SystemExit(f"unknown fit {version!r}; known: {', '.join(FITS)}")
    fit = FITS[version]
    WEEKS, TRAIN, SNAPSHOT = fit["weeks"], fit["train"], fit["snapshot"]
    print("VERSION", version)
    print("SNAPSHOT", SNAPSHOT)

    rng = Lcg(EXTRACT_SEED)
    rows = []
    for i in range(WEEKS):
        d = START + timedelta(weeks=i)
        w = week_of_year(i)
        amount = weekly_inflow(w, rng.next_unit())
        rows.append((d, w, money(amount)))

    # ── write the extract ──
    out = ["week_start,week_of_year,inflow_eur"]
    for d, w, a in rows:
        out.append(f"{d.isoformat()},{w},{a}")
    csv = "\n".join(out) + "\n"

    # ── fit on the training window ──
    train = rows[:TRAIN]
    baseline = sum(float(a) for _, _, a in train) / len(train)

    by_week = {}
    for _, w, a in train:
        by_week.setdefault(w, []).append(float(a))
    factors = {w: (sum(v) / len(v)) / baseline for w, v in sorted(by_week.items())}

    # ── score the holdout ──
    holdout = rows[TRAIN:]
    errs = []
    for _, w, a in holdout:
        pred = baseline * factors[w]
        errs.append(abs(pred - float(a)) / float(a))
    mape = sum(errs) / len(errs)

    print("CSV_ROWS", len(rows))
    print("BASELINE", money(baseline))
    print("MAPE", round(mape, 4))
    print("TRAIN_START", rows[0][0].isoformat(), "TRAIN_END", rows[TRAIN - 1][0].isoformat())
    print("HOLDOUT_START", rows[TRAIN][0].isoformat(), "HOLDOUT_END", rows[-1][0].isoformat())
    print("FACTORS", ",".join(f"{factors[w]:.6f}" for w in range(1, 53)))

    # Written under the snapshot's own name, in this directory: the audit card
    # NAMES the snapshot, so a hand copy from a generic `extract.csv` was one
    # step where the card and the file it points at could drift apart.
    out_path = Path(__file__).with_name(f"{SNAPSHOT}.csv")
    out_path.write_text(csv)
    print("WROTE", out_path.name)

if __name__ == "__main__":
    main()
