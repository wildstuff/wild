#!/usr/bin/env python3
"""Generate the payment_delay training extract and fit its parameters.

Deterministic by construction, exactly like the sibling `cash-forecast` fit:
the settlement history is a documented law plus a reproducible pseudo-noise
term (a fixed LCG), so anyone can regenerate the CSV byte-for-byte and
re-derive the same coefficients.

Design:
  - 186 weekly cohorts of settled receivables (2023-01-02 .. 2026-07-27),
    6 invoices per week, rotated over the debtors active that week.
  - Train on the first 174 weeks; hold out the final 12.
  - Model: expected delay = the debtor's own mean, SHRUNK toward the pooled
    mean by `n / (n + K)`.
  - Score MAE in days, against the baseline of one pooled mean for everyone.

`HABITS`, `POOLED_HABIT` and `FIRST_ACTIVE` are the GENERATIVE LAW of the
synthetic debtors — what the extract is drawn from, as opposed to the fitted
`parameters` the Procedure carries, which are this law estimated from a
sample. The liquidity example's seed generator IMPORTS them from here
(`examples/tribes/liquidity-management/data/regenerate-seed.py`) rather than
restating the numbers, so the debtors whose invoices the estimate is shown
against behave like the ones the model was fitted to. Two copies of a law
would drift apart silently — the chart would still render, just against
different debtors.

WHY THE FIT IS NOT DRAWN FROM THE TRIBE'S OWN INVOICES: it is the same law
with different luck, not the same rows. A Procedure is fitted on the book as
it stood at a cut-off and then applied to invoices it has never seen; fitting
on the very rows it is scored against would make every holdout number a
memory rather than a prediction.
"""

from datetime import date, timedelta
from decimal import Decimal, ROUND_HALF_UP
from pathlib import Path

START = date(2023, 1, 2)  # a Monday — the same epoch the inflow fit uses

# One entry per REGISTERED Procedure version, and they never leave: a stored
# estimate claiming v1 produced it has to stay re-derivable for as long as it
# stands. Same promise the re-run door makes at the artifact level
# (ADR-0202 D7.3) — this is its training-time half.
FITS = {
    # 186 weeks to 2026-07-27, the last Monday before the tribe's own cut-off
    # (2026-07-31). A model fitted on data past that date would be predicting
    # invoices it had already read.
    "v1": {"weeks": 186, "train": 174, "snapshot": "settlements-2023-2026"},
}
LIVE = "v1"

# How many settled receivables a week produces. Six is what makes BP-007 thin
# rather than absent: it starts in week 157, so it reaches the training window
# with ~14 observations — few enough that its own mean does not carry, which
# is the case the shrinkage exists for and the case the card has to show.
INVOICES_PER_WEEK = 6

# How each debtor pays, as `(mean delay in days, half-spread)`. Positive = pays
# after the due date. Every row is the `notes` column of the partner table
# stated as a number:
#
#   BP-001  "Key account - largest revenue contributor"
#   BP-002  "Short payment terms - reliable payer"
#   BP-004  "Irregular payment behaviour - watch DSO"
#   BP-006  "Reliable - always pays within terms"
#   BP-007  "New client since Q1 2026"
#
# The spread matters as much as the mean: BP-004 is not merely late, it is
# UNPREDICTABLE, and a model reporting only a mean would hide precisely the
# thing that note warns about. The card therefore carries the observed spread
# beside each mean, so an operator can see which estimate to trust.
HABITS = {
    "BP-001": (4, 3),    # key account — takes a few comfortable extra days
    "BP-002": (-2, 2),   # reliable, short terms — early and tight
    "BP-003": (2, 3),    # mid-tier, unremarkable
    "BP-004": (12, 8),   # the DSO problem: late AND wide
    "BP-005": (7, 5),    # project-based, large singles, slower to clear
    "BP-006": (-3, 2),   # always within terms
    "BP-007": (1, 3),    # too new to have a trustworthy mean of its own
}
# What a debtor without a habit of its own is assumed to do — and the value a
# per-debtor estimate is shrunk toward when its history is too thin.
POOLED_HABIT = (3, 4)

# Debtors that did not exist for the whole window, and the Monday they start.
# Their thin history is the point, not an inconvenience: a Procedure that
# reports a confident mean for a client it has seen fourteen times is lying
# with a number, which is harder to catch than lying with a word.
FIRST_ACTIVE = {"BP-007": date(2026, 1, 5)}

# The shrinkage weight is `n / (n + K)`: with K invoices of history a debtor's
# own mean carries half the estimate. K = 10 is a JUDGEMENT, not a fit — it
# says "ten settled invoices is when I start believing a debtor's own
# average", which is a sentence an accountant can agree or disagree with. It
# is on the card for exactly that reason.
SHRINKAGE_K = 10

# Above this weight the estimate is essentially the debtor's own mean, and the
# answer says so (`own_history`); below it the answer says `blended`. A
# reporting threshold only — it never changes a number.
OWN_HISTORY_MIN_WEIGHT = 0.8

# Seed of the extract's noise draw. The tribe's seed generator uses its own, so
# the two histories are the same law with different luck rather than the same
# numbers.
EXTRACT_SEED = 20260812


# Deterministic pseudo-noise: a plain LCG, seeded once.
class Lcg:
    def __init__(self, seed): self.s = seed
    def next_unit(self):
        # Numerical Recipes constants; returns [0,1)
        self.s = (1664525 * self.s + 1013904223) % (2**32)
        return self.s / 2**32


def draw_delay(pid, unit):
    """The law: the debtor's mean plus a symmetric draw over its half-spread,
    in whole days.

    `unit` is a [0,1) draw — passed in rather than taken from a module-level
    generator so a caller walking the book in a different order (the tribe's
    seed generator does) gets the same behaviour without inheriting this
    file's sequence.
    """
    mean, spread = HABITS.get(pid, POOLED_HABIT)
    return mean + round((unit * 2 - 1) * spread)


def active(pid, monday):
    """Whether a debtor exists yet on that Monday."""
    return pid not in FIRST_ACTIVE or monday >= FIRST_ACTIVE[pid]


def shrink(own_mean, n, pooled):
    """The estimate: the debtor's own mean pulled toward the pooled one in
    proportion to how much history stands behind it."""
    w = n / (n + SHRINKAGE_K)
    return w * own_mean + (1 - w) * pooled, w


def mean(xs):
    return sum(xs) / len(xs)


def stdev(xs):
    if len(xs) < 2:
        return 0.0
    m = mean(xs)
    return (sum((x - m) ** 2 for x in xs) / (len(xs) - 1)) ** 0.5


def money(x):
    return Decimal(str(x)).quantize(Decimal("0.01"), rounding=ROUND_HALF_UP)


def main():
    import sys
    version = sys.argv[1] if len(sys.argv) > 1 else LIVE
    if version not in FITS:
        raise SystemExit(f"unknown fit {version!r}; known: {', '.join(FITS)}")
    fit = FITS[version]
    WEEKS, TRAIN, SNAPSHOT = fit["weeks"], fit["train"], fit["snapshot"]
    print("VERSION", version)
    print("SNAPSHOT", SNAPSHOT)

    debtors = sorted(HABITS)
    rng = Lcg(EXTRACT_SEED)
    rows = []
    for i in range(WEEKS):
        monday = START + timedelta(weeks=i)
        eligible = [p for p in debtors if active(p, monday)]
        for j in range(INVOICES_PER_WEEK):
            pid = eligible[(i * 3 + j) % len(eligible)]
            delay = draw_delay(pid, rng.next_unit())
            # The due date walks through the week so a cohort is not one date.
            due = monday + timedelta(days=j % 5)
            rows.append((pid, due, due + timedelta(days=delay), delay, i))

    # ── write the extract ──
    out = ["partner_id,due_date,paid_date,delay_days"]
    for pid, due, paid, delay, _w in rows:
        out.append(f"{pid},{due.isoformat()},{paid.isoformat()},{delay}")
    csv = "\n".join(out) + "\n"

    # ── fit on the training window ──
    train = [r for r in rows if r[4] < TRAIN]
    holdout = [r for r in rows if r[4] >= TRAIN]
    pooled = mean([r[3] for r in train])

    by_debtor = {}
    for pid, _due, _paid, delay, _w in train:
        by_debtor.setdefault(pid, []).append(delay)

    params = {}
    for pid in debtors:
        obs = by_debtor.get(pid, [])
        if not obs:
            continue
        est, w = shrink(mean(obs), len(obs), pooled)
        params[pid] = {
            "n": len(obs),
            "own_mean": mean(obs),
            "observed_stdev": stdev(obs),
            "estimate": est,
            "weight": w,
        }

    # ── score the holdout: the model, and the baseline it has to beat ──
    def predict(pid):
        p = params.get(pid)
        return p["estimate"] if p else pooled

    errs = [abs(predict(r[0]) - r[3]) for r in holdout]
    base = [abs(pooled - r[3]) for r in holdout]
    mae, mae_pooled = mean(errs), mean(base)

    print("CSV_ROWS", len(rows), "TRAIN", len(train), "HOLDOUT", len(holdout))
    print("POOLED", round(pooled, 4), "POOLED_STDEV", round(stdev([r[3] for r in train]), 4))
    print("MAE", round(mae, 4), "MAE_POOLED_BASELINE", round(mae_pooled, 4))
    print("TRAIN_START", rows[0][1].isoformat(), "TRAIN_END", train[-1][1].isoformat())
    print("HOLDOUT_START", holdout[0][1].isoformat(), "HOLDOUT_END", rows[-1][1].isoformat())
    for pid in debtors:
        p = params.get(pid)
        if not p:
            print("DEBTOR", pid, "ABSENT_FROM_TRAINING")
            continue
        print("DEBTOR", pid, "n", p["n"],
              "own_mean", f"{p['own_mean']:.4f}",
              "stdev", f"{p['observed_stdev']:.4f}",
              "estimate", f"{p['estimate']:.4f}",
              "weight", f"{p['weight']:.4f}")

    # Written under the snapshot's own name, in this directory: the audit card
    # NAMES the snapshot, so a hand copy from a generic `extract.csv` was one
    # step where the card and the file it points at could drift apart.
    out_path = Path(__file__).with_name(f"{SNAPSHOT}.csv")
    out_path.write_text(csv)
    print("WROTE", out_path.name)


if __name__ == "__main__":
    main()
