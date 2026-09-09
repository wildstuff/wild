#!/usr/bin/env python3
"""Regenerate this bundle's seed CSVs, anchored at TODAY.

The sample data is DERIVED, not typed. Every fact that has to agree with
another one is computed here: a settled invoice gets its bank booking (right
counterparty, right IBAN, right amount, a date after issuance), each account's
`balance` folds from its `opening_balance` plus the movements booked on it, and
every minted IBAN carries a valid mod-97 check digit. Hand-editing the CSVs is
how the previous cut drifted into payments that predated their invoice and
debtors who paid someone else's bill.

Transaction `amount` is UNSIGNED here because the model declares it so —
`direction` carries the sign. A signed amount would make
`cash_flow_13w.gross_movement` (a plain sum) quietly report the net.

    python3 examples/tribes/liquidity-management/data/regenerate-seed.py

Writes the CSVs in place, beside this file. Moving TODAY forward is the whole
point of keeping this around: re-cut the data when `example_seed_freshness`
(wild-ddd) goes red.

The report's forecast window needs nothing after a re-cut — it anchors on
`today` and follows the clock. One thing DOES read the resulting figures and
must be updated in the same commit:

    assets/evals/selection/open-receivables-is-the-declared-measure.yaml
    expects the open-receivables total (at this cut: 135.800,00 EUR).

That figure is deliberately UNCHANGED by the settled history below: everything
generated here is already paid, so the open book is exactly the hand-authored
tail it always was.

── The settled history, and why it is drawn from the Procedure's own law ──

The tribe carries `cash_inflow_forecast`, a fitted Procedure (ADR-0202 D5)
whose estimates are charted against what actually came in. Two years of
settled receivables therefore have to exist, and they have to be the same
ORDER OF MAGNITUDE as the model — a forecast of ~42.000 EUR/week drawn beside
an actual line of ~5.000 EUR/week does not demonstrate a forecast, it
demonstrates a mistake.

So the weekly inflow here is drawn from the generative law the training
extract was drawn from, IMPORTED from `fit.py` rather than restated:
`BASE_EUR × true_factor(week_bucket) × noise`. The Procedure's parameters are
that law estimated from a sample, so the gap between the two curves is the fit
error (holdout MAPE 0.038) and nothing else. Restating the constants here
would let the two worlds drift apart with both charts still rendering.

The noise is seeded PER ABSOLUTE WEEK, not from a running generator, so moving
TODAY forward re-cuts the window without rewriting the weeks that stay in it.
"""
import calendar
import csv
import importlib.util
import os
from datetime import date, timedelta

TODAY = date(2026, 8, 22)
OUT = os.path.dirname(os.path.abspath(__file__))

# Each Procedure's generative law — ONE source per model, imported, never
# restated here.
#
# Loaded by PATH under explicit names rather than by `sys.path` + `import fit`:
# both components call their training module `fit.py` (it is the convention,
# and each is self-describing in its own directory), so two directories on
# `sys.path` would make `import fit` resolve by insertion order. That is a
# shadowing bug waiting for whichever path is inserted second — and it would
# not fail, it would silently fit the seed to the wrong law.
def _law(component):
    path = os.path.normpath(
        os.path.join(OUT, "..", "..", "..", "tool-providers", component, "training", "fit.py")
    )
    spec = importlib.util.spec_from_file_location(f"{component}_law", path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


fit = _law("cash-forecast")
delay = _law("payment-delay")
fx = _law("fx-exposure")

# Two full seasonal years before the cut: enough that every one of the model's
# 52 weekly buckets is observed twice, which is what makes a seasonal claim
# checkable rather than a single year's coincidence.
HISTORY_WEEKS = 104

# Payables as a share of receivables. Sized so the operating result is a small
# surplus once the recurring costs below are added (~1% of turnover), instead
# of a book that either drains or hoards over two years.
COGS_RATIO = 0.89

# Fixed monthly operating costs, extended across the whole history.
RENT_EUR, TELECOM_EUR, PAYROLL_EUR, POWER_EUR = "1850.00", "950.00", "12000.00", "3200.00"

# Seed of the seed. Distinct from `fit.EXTRACT_SEED`: the tribe's book and the
# training extract are the same business in different years, not the same
# numbers twice.
SEED = 20260731

# The Monday of the week TODAY falls in — every weekly bucket anchors here.
LAST_MONDAY = TODAY - timedelta(days=TODAY.weekday())
OPENING = LAST_MONDAY - timedelta(weeks=HISTORY_WEEKS)

# ── The weekly forecast seed (ADR-0202 D5) ──────────────────────────────────
# One estimate per past Monday for the week that followed (the accuracy trail),
# plus ONE run from the current Monday writing the whole forward curve at 7,
# 14 … 91 days. `expected_inflow` is deliberately NOT a column: the Procedure
# fills it at the ingest boundary, so an empty column here becoming a curve on
# the surface is the proof that the chain ran.
ESTIMATE_TRAIL_WEEKS = 52
FORECAST_WEEKS = 13
# DERIVED, not typed. Each training module names the version it is the live fit
# for, so a refit that forgets the seed is not possible from this side — one
# fewer hand-kept copy of a number that already lives in too many places (see
# the README's "the five places a version lives"). What this cannot check is
# that `LIVE` agrees with the component's audit card; the
# `the_forecast_surfaces_name_the_live_verfahren` suite compares the seed to
# the CARD, which closes that gap from the other end.
PROCEDURE_VERSION = fit.LIVE
DELAY_VERSION = delay.LIVE
FX_VERSION = fx.LIVE


def iban_de(bban: str) -> str:
    """Mint a German IBAN with a correct mod-97 check digit."""
    assert len(bban) == 18, bban
    rearranged = bban + "DE00"
    digits = "".join(str(int(c, 36)) for c in rearranged)
    check = 98 - (int(digits) % 97)
    return f"DE{check:02d}{bban}"


def mod97_ok(i: str) -> bool:
    s = i[4:] + i[:4]
    return int("".join(str(int(c, 36)) for c in s)) % 97 == 1


def d(s):
    return s.isoformat()


# ── Accounts ────────────────────────────────────────────────────────────────
MAIN = "DE89370400440532013000"
RESERVE = "DE35370400440532013002"
TAX = "DE08370400440532013003"
PAYROLL = "DE62370400440532013001"  # counterparty only (the payroll clearing)

ACCOUNTS = [
    # iban, name, bank, opening balance on OPENING
    (MAIN, "Main Business Account", "Commerce Bank Cologne", "60000.00"),
    (RESERVE, "Reserve Account", "Commerce Bank Cologne", "75000.00"),
    (TAX, "Tax Account", "Cologne-Bonn Savings Bank", "3200.00"),
]

# ── Partners ────────────────────────────────────────────────────────────────
# BP-005 and BP-007 used to duplicate BP-002 / BP-003 (same name, same IBAN);
# they are distinct companies now.
BP5_IBAN = iban_de("200505500000778899")
BP7_IBAN = iban_de("701500000123456780")

PARTNERS = [
    # id, name, role, iban, terms, disc%, disc_days, city, notes, chat
    ("BP-001", "Smith Corp", "debtor", "DE44500105175407324931", 30, "", "", "Berlin",
     "Key account - largest revenue contributor", "100200301"),
    ("BP-002", "Brewer & Sons Ltd", "debtor", "DE62200700240000279500", 14, "", "", "Hamburg",
     "Short payment terms - reliable payer", "100200302"),
    ("BP-003", "Little & Partners", "debtor", "DE26200400600087654300", 30, "", "", "Munich",
     "Mid-tier consulting client", ""),
    ("BP-004", "Hoffman Trading Co", "debtor", "DE79345678901234567890", 14, "", "", "Cologne",
     "Irregular payment behaviour - watch DSO", "100200304"),
    ("BP-005", "Nordwind Maritime GmbH", "debtor", BP5_IBAN, 30, "", "", "Bremen",
     "Project-based - large single invoices", "100200305"),
    ("BP-006", "Weaver Consulting", "debtor", "DE78200400600012312300", 30, "", "", "Frankfurt",
     "Reliable - always pays within terms", "100200306"),
    ("BP-007", "Aurora Labs Ltd", "debtor", BP7_IBAN, 60, "", "", "Stuttgart",
     "New client since Q1 2026 - 60-day terms agreed", ""),
    ("SUP-001", "TechParts Inc", "creditor", "DE02120300000000202051", 30, 2, 10, "Stuttgart",
     "Cash discount 2/10 net 30 - take it when possible", ""),
    ("SUP-002", "Logistics Express Ltd", "creditor", "DE80200400600004444400", 30, "", "", "Dortmund",
     "No cash discount - negotiate terms in Q3", ""),
    ("SUP-003", "Miller Office", "creditor", "DE32700100800012345000", 30, "", "", "Berlin",
     "Office supplies and rent - fixed monthly", ""),
    ("SUP-004", "Berlin City Utilities", "creditor", "DE58200400600971586400", 30, "", "", "Berlin",
     "Utilities - fixed schedule", ""),
    ("SUP-005", "Hoffman Printing", "creditor", "DE03200400600022222200", 30, 2, 10, "Leipzig",
     "Cash discount 2/10 - new supplier since May", ""),
]
P = {p[0]: p for p in PARTNERS}

# ── Receivables (outbound) ──────────────────────────────────────────────────
# (id, partner, issued, due, amount, paid_date or None)
OUTBOUND = [
    ("RE-2026-0031", "BP-001", date(2026, 5, 4), date(2026, 6, 3), "15000.00", date(2026, 6, 1)),
    ("RE-2026-0033", "BP-005", date(2026, 5, 11), date(2026, 6, 10), "8900.00", date(2026, 6, 8)),
    ("RE-2026-0038", "BP-007", date(2026, 6, 1), date(2026, 7, 31), "9200.00", date(2026, 6, 26)),
    ("RE-2026-0039", "BP-003", date(2026, 6, 8), date(2026, 7, 8), "4500.00", date(2026, 7, 3)),
    ("RE-2026-0045", "BP-002", date(2026, 7, 6), date(2026, 7, 20), "6300.00", date(2026, 7, 17)),
    # overdue as of TODAY - the dunning chain works on these
    ("RE-2026-0032", "BP-004", date(2026, 5, 18), date(2026, 6, 17), "7400.00", None),
    ("RE-2026-0035", "BP-001", date(2026, 6, 5), date(2026, 7, 5), "11500.00", None),
    ("RE-2026-0036", "BP-006", date(2026, 6, 12), date(2026, 7, 12), "6800.00", None),
    ("RE-2026-0037", "BP-004", date(2026, 6, 22), date(2026, 7, 22), "4100.00", None),
    # open, falling due inside the 13-week forecast window.
    #
    # Anchored on TODAY, not written as absolute dates. They used to be
    # absolute, and the clock walked five of them past their due date into the
    # OVERDUE set — `example_seed_freshness` went red on a tree nobody had
    # touched, and bumping TODAY did not help, because these rows never read
    # it. A row whose comment says "falls due inside the window" has to be
    # relative to the window, or the comment is only true on the day it was
    # written.
    ("RE-2026-0040", "BP-002", TODAY - timedelta(days=15), TODAY + timedelta(days=6), "8800.00", None),
    ("RE-2026-0041", "BP-001", TODAY - timedelta(days=40), TODAY + timedelta(days=13), "16500.00", None),
    ("RE-2026-0042", "BP-005", TODAY - timedelta(days=33), TODAY + timedelta(days=20), "24000.00", None),
    ("RE-2026-0043", "BP-006", TODAY - timedelta(days=26), TODAY + timedelta(days=27), "5600.00", None),
    ("RE-2026-0044", "BP-004", TODAY - timedelta(days=15), TODAY + timedelta(days=34), "3900.00", None),
    ("RE-2026-0046", "BP-003", TODAY - timedelta(days=23), TODAY + timedelta(days=41), "7200.00", None),
    ("RE-2026-0049", "BP-001", TODAY - timedelta(days=31), TODAY + timedelta(days=55), "9600.00", None),
    ("RE-2026-0048", "BP-005", TODAY - timedelta(days=38), TODAY + timedelta(days=69), "18000.00", None),
    ("RE-2026-0047", "BP-007", TODAY - timedelta(days=29), TODAY + timedelta(days=62), "12400.00", None),
    # ── FOREIGN-CURRENCY receivables (ADR-0202 D5, the fx-exposure Procedure).
    # Three open invoices, one per fitted currency. `amount` is in the
    # invoice's OWN currency — which is exactly why every money measure in the
    # model now carries `currency == EUR`, and why these do not disturb the
    # 135.800,00 EUR open book the selection eval pins.
    ("RE-2026-0050", "BP-005", TODAY - timedelta(days=33), TODAY + timedelta(days=20), "24000.00", None),
    ("RE-2026-0051", "BP-001", date(2026, 7, 23), date(2026, 8, 22), "12000.00", None),
    ("RE-2026-0052", "BP-004", date(2026, 7, 27), date(2026, 8, 26), "9500.00", None),
]

# Which of the hand-authored receivables are NOT in euro. A lookup rather than
# a seventh tuple element: three exceptions in a table of thirteen, and the
# generated history is euro throughout.
FOREIGN = {"RE-2026-0050": "USD", "RE-2026-0051": "GBP", "RE-2026-0052": "CHF"}

# ── Payables (inbound) ──────────────────────────────────────────────────────
INBOUND = [
    ("EK-2026-0091", "SUP-001", date(2026, 5, 4), date(2026, 6, 3), "8500.00", date(2026, 5, 29)),
    ("EK-2026-0098", "SUP-001", date(2026, 6, 8), date(2026, 7, 8), "3800.00", date(2026, 7, 2)),
    # overdue as of TODAY
    ("EK-2026-0092", "SUP-002", date(2026, 6, 1), date(2026, 7, 1), "3100.00", None),
    ("EK-2026-0093", "SUP-003", date(2026, 6, 10), date(2026, 7, 10), "1200.00", None),
    # open, due inside the window; the last two still have a cash-discount window
    ("EK-2026-0094", "SUP-001", date(2026, 7, 6), date(2026, 8, 5), "5800.00", None),
    ("EK-2026-0095", "SUP-004", date(2026, 7, 8), date(2026, 8, 7), "2400.00", None),
    ("EK-2026-0096", "SUP-002", date(2026, 7, 13), date(2026, 8, 12), "7200.00", None),
    ("EK-2026-0097", "SUP-003", date(2026, 7, 15), date(2026, 8, 14), "980.00", None),
    ("EK-2026-0099", "SUP-005", date(2026, 7, 20), date(2026, 8, 19), "6500.00", None),
    ("EK-2026-0100", "SUP-002", date(2026, 7, 22), date(2026, 8, 21), "4100.00", None),
    ("EK-2026-0101", "SUP-001", date(2026, 7, 24), date(2026, 8, 23), "9300.00", None),
    ("EK-2026-0102", "SUP-004", date(2026, 7, 27), date(2026, 8, 26), "2200.00", None),
    ("EK-2026-0103", "SUP-005", date(2026, 7, 29), date(2026, 8, 28), "3400.00", None),
]

# ── The settled history ─────────────────────────────────────────────────────
# Generated, never typed: two years of receivables that were issued, fell due
# and were paid, drawn week by week from the Procedure's own law. Each week's
# settlements sum EXACTLY to that week's draw, so "what the model expects" and
# "what the book records" are comparable figures rather than two guesses.

WEEKS = [LAST_MONDAY - timedelta(weeks=k) for k in range(HISTORY_WEEKS, -1, -1)]

DEBTORS = [p for p in PARTNERS if p[2] == "debtor"]
CREDITORS = [p for p in PARTNERS if p[2] == "creditor"]

# ── Payment behaviour, per debtor ───────────────────────────────────────────
# `(mean days late, half-spread)`, IMPORTED from the Procedure that is fitted
# to it — exactly as `fit.weekly_inflow` is imported rather than restated.
#
# It used to live here, which was one table too many: the component learns
# this law from its own extract, this generator draws the tribe's book from it,
# and two copies would drift apart with nothing failing. The chart would still
# render, just against debtors who no longer behave the way the model was
# taught they do.
#
# The law itself is not invented anywhere — every line of it is the behaviour
# the partner's own `notes` column already claimed while the generator drew one
# pooled distribution for all of them ("Irregular payment behaviour - watch
# DSO", "Reliable - always pays within terms"). Read
# `tool-providers/payment-delay/training/fit.py` for the table.
HABITS = delay.HABITS
POOLED_HABIT = delay.POOLED_HABIT

# OUR side of the ledger, and deliberately NOT taken from that table.
# A debtor's lateness is a habit we OBSERVE and can only learn; how we pay our
# own suppliers is a policy we SET — and with 2/10-net-30 discounts standing on
# two of them, a treasury that drifts days late is losing money on purpose.
# Slightly early, tight spread. It stays here because it belongs to the tribe,
# not to a model: no Procedure learns it.
OUR_PAYMENT_HABIT = (-1, 2)

# ── ADR-0202 D5 — the fx snapshot the valuations are anchored to ────────────
# REAL ECB euro reference rates for the cut-off date, fetched from the data
# API rather than invented. They are quoted as units per ONE euro, which is
# why a rising number means the euro strengthened.
#
# Note the decimals: four for USD and CHF, five for GBP. That is the ECB's own
# per-series precision and the reason `fx_rate.rate_per_eur` is TEXT — one
# fixed decimal scale would round the pound on the way in.
#
# 2026-07-31 is a Friday, so it is the newest quote that exists at the tribe's
# cut. Rates publish around 16:00 CET; there is systematically no rate for
# "today" until the afternoon, and none at all on weekends.
FX_RATE_DATE = date(2026, 7, 31)
FX_RATES = {"CHF": "0.9304", "GBP": "0.85573", "USD": "1.1485"}

# Partners that did not exist for the whole window, and the Monday they start.
# Imported for the same reason as the habits: the component's extract has to be
# thin for BP-007 in the same window the tribe's book is, or its `blended`
# basis would be an artefact of two disagreeing timelines.
NEWCOMERS = delay.FIRST_ACTIVE


def week_index(monday):
    """Weeks since the model's epoch Monday — the law's own time axis."""
    return (monday - fit.START).days // 7


def week_rng(monday, salt):
    """A generator seeded by the ABSOLUTE week, so a later re-cut leaves every
    week that stays in the window byte-identical.

    The first draw is discarded: an LCG's first output is close to linear in
    its seed, so consecutive weeks would otherwise share a visible ramp."""
    r = fit.Lcg((SEED + salt + week_index(monday) * 7919) % (2**32))
    r.next_unit()
    return r


def monday_of(d):
    return d - timedelta(days=d.weekday())


def split(total, n, rng):
    """Split a weekly total into `n` invoice amounts that sum back to it
    exactly — the last part absorbs the rounding, so the week's sum is the
    law's figure and not the law's figure ± a few cents."""
    weights = [0.65 + 0.7 * rng.next_unit() for _ in range(n)]
    scale = sum(weights)
    parts = [round(total * w / scale, 2) for w in weights[:-1]]
    parts.append(round(total - sum(parts), 2))
    return parts


def parts_for(amount):
    """How many invoices a week's money arrives in. Fewer when the hand-authored
    tail already accounts for most of it."""
    if amount > 9000:
        return 3
    if amount > 3000:
        return 2
    return 1 if amount > 500 else 0


def booked_by_week(rows):
    """Σ settled amount per week-Monday, over an existing invoice list."""
    out = {}
    for _, _, _, _, amount, paid in rows:
        if paid:
            m = monday_of(paid)
            out[m] = out.get(m, 0.0) + float(amount)
    return out


def generate(rows, partners, prefix, ratio, salt, existing, habits):
    """Extend `rows` with settled invoices so that each week's settled total
    hits the law. Returns nothing; appends in place."""
    seq = {}
    for monday in WEEKS:
        target = fit.weekly_inflow(
            fit.week_of_year(week_index(monday)), week_rng(monday, 0).next_unit()
        ) * ratio
        remaining = target - existing.get(monday, 0.0)
        rng = week_rng(monday, salt)
        n = parts_for(remaining)
        if n == 0:
            continue
        for j, amount in enumerate(split(remaining, n, rng)):
            if amount <= 0:
                continue
            # Money lands Mon-Wed, so the newest week never books after TODAY.
            paid = monday + timedelta(days=j % 3)
            # BP-007's note says "new client since Q1 2026", and the generator
            # used to hand it two years of history anyway. It is now genuinely
            # new — which is what gives the payment-delay Procedure its honest
            # degradation case: too few settled invoices to trust a per-debtor
            # mean, so the estimate falls back toward the pooled one.
            eligible = [p for p in partners if p[0] not in NEWCOMERS or monday >= NEWCOMERS[p[0]]]
            partner = eligible[(week_index(monday) * 3 + j) % len(eligible)]
            terms = partner[4]
            # How LATE this debtor pays, drawn from its own habit rather than
            # from one pooled distribution. The notes column has always claimed
            # these behaviours ("watch DSO", "always pays within terms"); until
            # the payment-delay Procedure needed them to be true, the data said
            # every debtor was the same.
            mean, spread = habits(partner[0])
            delay = mean + round((rng.next_unit() * 2 - 1) * spread)
            due = paid - timedelta(days=delay)
            issued = due - timedelta(days=terms)
            year = paid.year
            seq[year] = seq.get(year, 1000) + 1
            rows.append(
                (f"{prefix}-{year}-{seq[year]:04d}", partner[0], issued, due,
                 f"{amount:.2f}", paid)
            )


generate(OUTBOUND, DEBTORS, "RE", 1.0, 101, booked_by_week(OUTBOUND),
         habits=lambda pid: HABITS.get(pid, POOLED_HABIT))
generate(INBOUND, CREDITORS, "EK", COGS_RATIO, 202, booked_by_week(INBOUND),
         habits=lambda _pid: OUR_PAYMENT_HABIT)

# ── Transactions ────────────────────────────────────────────────────────────
# Amounts are ALWAYS POSITIVE - `direction` carries the sign, exactly as the
# model declares. Settlements are generated from the invoices above so the
# counterparty, the IBAN, the amount and the date can never drift apart.
tx = []


def book(dt, amount, direction, counterparty, cp_iban, description, reference, account=MAIN):
    tx.append({
        "date": d(dt), "amount": amount, "direction": direction,
        "counterparty": counterparty, "counterparty_iban": cp_iban,
        "description": description, "reference": reference, "account_iban": account,
    })


for inv_id, pid, issued, due, amount, paid in OUTBOUND:
    if paid:
        name, cp_iban = P[pid][1], P[pid][3]
        book(paid, amount, "credit", name, cp_iban, f"Payment {inv_id}", inv_id)

for inv_id, pid, issued, due, amount, paid in INBOUND:
    if paid:
        name, cp_iban = P[pid][1], P[pid][3]
        book(paid, amount, "debit", name, cp_iban, f"Payment {inv_id}", inv_id)


# The same fixed costs across the generated history — a business carries rent
# and payroll in the weeks it invoices nothing, and a book without them reads
# as pure margin. Stops before the three hand-authored tail months below, which
# keep their own bookings so the recent window stays exactly as it was.
def months_from(start, end):
    y, m = start.year, start.month
    while (y, m) <= (end.year, end.month):
        yield y, m
        y, m = (y + 1, 1) if m == 12 else (y, m + 1)


for y, m in months_from(OPENING, date(2026, 4, 1)):
    mname = calendar.month_name[m]
    for day, amount, name, cp_iban, label, ref in (
        (2, PAYROLL_EUR, f"Salaries {mname}", PAYROLL,
         f"Wages and salaries {mname} {y}", f"LG-{y}-{m:02d}"),
        (3, TELECOM_EUR, "Telecom Business Customers", "DE20200700240012345601",
         f"Telecommunications {mname}", f"T-{y}-{m:02d}"),
        (15, RENT_EUR, "Miller Office", P["SUP-003"][3],
         f"Rent {mname} {y}", f"RENT-{y}-{m:02d}"),
    ):
        when = date(y, m, day)
        if when >= OPENING:
            book(when, amount, "debit", name, cp_iban, label, ref)
    # Power is billed a month in arrears and booked on the 10th, which is why
    # the reference names the month BEFORE the booking date.
    billed = date(y, m, 1) - timedelta(days=1)
    when = date(y, m, 10)
    if when >= OPENING:
        book(when, POWER_EUR, "debit", "Berlin City Utilities", P["SUP-004"][3],
             f"Electricity {calendar.month_name[billed.month]} {billed.year}",
             f"RG-STW-{billed.month:02d}{billed.year}")

# Recurring operating movements - rent, utilities, telecom, salaries, insurance.
for month, mname in ((5, "May"), (6, "June"), (7, "July")):
    book(date(2026, month, 15), "1850.00", "debit", "Miller Office",
         P["SUP-003"][3], f"Rent {mname} 2026", f"RENT-2026-{month:02d}")
    book(date(2026, month, 3), "950.00", "debit", "Telecom Business Customers",
         "DE20200700240012345601", f"Telecommunications {mname}", f"T-2026-{month:02d}")
    book(date(2026, month, 2), "12000.00", "debit", f"Salaries {mname}",
         PAYROLL, f"Wages and salaries {mname} 2026", f"LG-2026-{month:02d}")
book(date(2026, 5, 8), "3200.00", "debit", "Berlin City Utilities",
     P["SUP-004"][3], "Electricity April 2026", "RG-STW-042026")
book(date(2026, 6, 10), "3200.00", "debit", "Berlin City Utilities",
     P["SUP-004"][3], "Electricity May 2026", "RG-STW-052026")
book(date(2026, 7, 10), "3400.00", "debit", "Berlin City Utilities",
     P["SUP-004"][3], "Electricity June 2026", "RG-STW-062026")
book(date(2026, 5, 20), "6700.00", "debit", "Logistics Express Ltd",
     P["SUP-002"][3], "Freight April", "LOG-2026-04")
book(date(2026, 6, 18), "5500.00", "debit", "Logistics Express Ltd",
     P["SUP-002"][3], "Freight May", "LOG-2026-05")
book(date(2026, 7, 21), "4200.00", "debit", "Allianz Insurance",
     "DE73200400600000200000", "Business insurance Q3", "VS-Q3-2026")
# Down payment on a project - cash in that is not an invoice settlement.
book(date(2026, 6, 15), "22000.00", "credit", "Nordwind Maritime GmbH",
     BP5_IBAN, "Down payment Project Gamma", "PRJ-GAMMA-01")

# Cross-account movements, so no account sits without a single booking.
book(date(2026, 6, 30), "10000.00", "debit", "Reserve Account", RESERVE,
     "Transfer to reserve", "TRF-2026-06", account=MAIN)
book(date(2026, 6, 30), "10000.00", "credit", "Main Business Account", MAIN,
     "Transfer from main account", "TRF-2026-06", account=RESERVE)
book(date(2026, 7, 6), "9000.00", "debit", "Tax Account", TAX,
     "Transfer for VAT prepayment", "TRF-2026-07", account=MAIN)
book(date(2026, 7, 6), "9000.00", "credit", "Main Business Account", MAIN,
     "Transfer from main account", "TRF-2026-07", account=TAX)
book(date(2026, 7, 10), "14800.00", "debit", "Cologne Tax Office",
     "DE02120300000000202051", "VAT prepayment Q2 2026", "UST-Q2-2026", account=TAX)

tx.sort(key=lambda r: (r["date"], r["account_iban"], r["reference"]))


def balance_of(acct, opening):
    bal = float(opening)
    for r in tx:
        if r["account_iban"] != acct:
            continue
        bal += float(r["amount"]) if r["direction"] == "credit" else -float(r["amount"])
    return f"{bal:.2f}"


# ── Write ───────────────────────────────────────────────────────────────────
def write(name, header, rows):
    with open(os.path.join(OUT, name), "w", newline="") as f:
        w = csv.writer(f, lineterminator="\n")
        w.writerow(header)
        w.writerows(rows)


write("business-partners.csv",
      ["partner_id", "name", "role", "iban", "payment_terms_days", "cash_discount_percent",
       "cash_discount_days", "city", "notes", "telegram_chat_id"],
      [list(p) for p in PARTNERS])

write("bank-accounts.csv",
      ["iban", "account_name", "bank", "currency", "opening_balance", "opening_date",
       "balance", "balance_date"],
      [[i, n, b, "EUR", o, d(OPENING), balance_of(i, o), d(TODAY)] for i, n, b, o in ACCOUNTS])

write("bank-transactions.csv",
      ["date", "amount", "direction", "counterparty", "counterparty_iban", "description",
       "reference", "account_iban"],
      [[r["date"], r["amount"], r["direction"], r["counterparty"], r["counterparty_iban"],
        r["description"], r["reference"], r["account_iban"]] for r in tx])

write("invoices-outbound.csv",
      ["invoice_id", "issued_date", "due_date", "paid_date", "amount", "currency",
       "customer_id", "status", "payment_terms_days"],
      [[i, d(iss), d(due), d(paid) if paid else "", amt, FOREIGN.get(i, "EUR"), pid,
        "paid" if paid else "open", (due - iss).days]
       for i, pid, iss, due, amt, paid in sorted(OUTBOUND, key=lambda r: r[0])])

write("invoices-inbound.csv",
      ["invoice_id", "issued_date", "due_date", "paid_date", "amount", "currency",
       "supplier_id", "status", "payment_terms_days", "cash_discount_percent",
       "cash_discount_days"],
      [[i, d(iss), d(due), d(paid) if paid else "", amt, "EUR", pid,
        "paid" if paid else "open", (due - iss).days, P[pid][5], P[pid][6]]
       for i, pid, iss, due, amt, paid in sorted(INBOUND, key=lambda r: r[0])])

write("credit-facilities.csv",
      ["facility_id", "bank", "account_iban", "limit", "drawn", "review_date"],
      [["KK-2026-001", "Commerce Bank Cologne", MAIN, "50000.00", "0.00", "2026-12-31"],
       ["KK-2026-002", "Cologne-Bonn Savings Bank", TAX, "25000.00",
        f"{max(0.0, -float(balance_of(TAX, '3200.00'))):.2f}", "2026-09-30"]])

write("dunnings.csv",
      ["id", "invoice_id", "partner_id", "level", "sent_date", "amount", "status"],
      [["DUN-SEED-001", "RE-2026-0032", "BP-004", 1, "2026-06-25", "7400.00", "open"],
       ["DUN-SEED-002", "RE-2026-0035", "BP-001", 1, "2026-07-13", "11500.00", "open"],
       ["DUN-SEED-003", "RE-2026-0036", "BP-006", 2, "2026-07-27", "6800.00", "escalated"]])

write("payment-promises.csv",
      ["id", "invoice_id", "promised_date", "promised_amount", "status", "note"],
      [["PROM-SEED-001", "RE-2026-0032", "2026-08-07", "7400.00", "open",
        "Payment promised once the end customer pays"],
       ["PROM-SEED-002", "RE-2026-0037", "2026-08-14", "4100.00", "open",
        "Requested instalment payment in two tranches"],
       ["PROM-SEED-003", "RE-2026-0035", "2026-07-20", "11500.00", "open",
        "Promised for 20 July - the date has passed and the aging tick breaks it"]])

write("payments.csv",
      ["id", "invoice_id", "partner_id", "account_iban", "amount", "scheduled_date",
       "executed_date", "method", "status"],
      [["PAY-SEED-001", "EK-2026-0091", "SUP-001", MAIN, "8500.00", "2026-05-28",
        "2026-05-29", "sepa", "executed"],
       ["PAY-SEED-002", "EK-2026-0098", "SUP-001", MAIN, "3800.00", "2026-07-02",
        "2026-07-02", "sepa", "executed"],
       ["PAY-SEED-003", "EK-2026-0101", "SUP-001", MAIN, "9114.00", "2026-08-01",
        "", "sepa", "approved"],
       ["PAY-SEED-004", "EK-2026-0094", "SUP-001", MAIN, "5800.00", "2026-08-04",
        "", "sepa", "planned"],
       ["PAY-SEED-005", "EK-2026-0093", "SUP-003", MAIN, "1200.00", "2026-08-03",
        "", "sepa", "planned"]])

# ── The weekly forecast ask (ADR-0202 D5) ───────────────────────────────────
# NO `expected_inflow` column, on purpose. These rows are the QUESTION; the
# Procedure answers it at the ingest boundary and the answer is written back
# into the field. If the component is not installed, the column stays empty and
# the readiness surface says why — which is the honest state, not a broken one.
#
# `target_date` is computed here as `as_of + horizon_days`, the same arithmetic
# the component does internally. It has to be: only `expected_inflow` travels
# back, so a row whose stored target disagreed with the one the model answered
# for would plot the right number on the wrong week, and nothing would report it.
estimates = []
for k in range(ESTIMATE_TRAIL_WEEKS, 0, -1):
    as_of = LAST_MONDAY - timedelta(weeks=k)
    estimates.append((as_of, as_of + timedelta(days=7), 7))
# One run, today, writing the whole forward curve — what a weekly cadence
# produces each Monday.
for w in range(1, FORECAST_WEEKS + 1):
    horizon = 7 * w
    estimates.append((LAST_MONDAY, LAST_MONDAY + timedelta(days=horizon), horizon))

keys = {(d(t), PROCEDURE_VERSION) for _, t, _ in estimates}
assert len(keys) == len(estimates), "two estimates collide on [target_date, procedure_version]"

write("cash-inflow-estimates.csv",
      ["as_of", "target_date", "horizon_days", "procedure_version"],
      [[d(a), d(t), h, PROCEDURE_VERSION] for a, t, h in estimates])

# ── The per-invoice settlement ASK (ADR-0202 D5, the second Procedure) ──────
# One row per OPEN receivable, and only open ones: an invoice that has already
# settled has an answer, and asking a model to estimate a date the book
# records would be a forecast of the past.
#
# Like the inflow seed, these rows carry the QUESTION and no answer. The
# `payment-delay` Procedure fills `expected_payment_date`, the spread and the
# basis at the ingest boundary. Loading them with the component absent is a
# legal state: the rows stand with an empty estimate and `wild tribe readiness`
# names the missing Procedure.
open_receivables = [r for r in OUTBOUND if r[5] is None]
assert open_receivables, "the seed carries open receivables to estimate"
settlement_keys = {(r[0], DELAY_VERSION) for r in open_receivables}
assert len(settlement_keys) == len(open_receivables), (
    "two settlement estimates collide on [invoice_id, procedure_version]"
)

write("settlement-estimates.csv",
      ["invoice_id", "partner_id", "due_date", "amount", "currency", "procedure_version"],
      [[i, pid, d(due), amt, FOREIGN.get(i, "EUR"), DELAY_VERSION]
       for i, pid, _iss, due, amt, _paid in sorted(open_receivables, key=lambda r: r[0])])

# ── The fx snapshot, and the valuation ASK (ADR-0202 D5) ───────────────────
# The rates are a MIRROR of what the ECB published; in a live tribe the
# `ecb-fx` source refreshes them each afternoon. Seeding them means the
# example values its foreign book without waiting for an egress grant.
write("fx-rates.csv",
      ["currency", "rate_date", "rate_per_eur"],
      [[c, d(FX_RATE_DATE), FX_RATES[c]] for c in sorted(FX_RATES)])

# One row per open FOREIGN receivable, carrying the QUESTION and no answer:
# which invoice, in what currency, how much, against which rate of which day,
# and how far ahead to draw the band. The Procedure fills the euro figure, the
# band and the basis at the ingest boundary.
#
# `days_to_settlement` is the debtor's mean delay from the shared law. A live
# tribe copies the `payment-delay` Procedure's own answer for the invoice —
# that is what makes the two compose — but the seed cannot, because that answer
# does not exist until the component has run. Using the LAW keeps the two
# consistent without pretending a Procedure ran.
fx_open = [r for r in open_receivables if r[0] in FOREIGN]
assert fx_open, "the seed carries foreign-currency receivables to value"
write("fx-valuations.csv",
      ["invoice_id", "currency", "amount_foreign", "rate_per_eur", "rate_date",
       "days_to_settlement", "procedure_version"],
      [[i, FOREIGN[i], amt, FX_RATES[FOREIGN[i]], d(FX_RATE_DATE),
        HABITS.get(pid, POOLED_HABIT)[0], FX_VERSION]
       for i, pid, _iss, _due, amt, _paid in sorted(fx_open, key=lambda r: r[0])])

# The seed's whole claim is that forecast and actuals are the same order of
# magnitude. Check it here rather than discovering it on a chart.
inflow_by_week = booked_by_week([r for r in OUTBOUND])
for monday in WEEKS[:-1]:  # the newest week is still filling
    law = fit.weekly_inflow(
        fit.week_of_year(week_index(monday)), week_rng(monday, 0).next_unit()
    )
    got = inflow_by_week.get(monday, 0.0)
    assert abs(got - law) < 0.02 or got > law, (
        f"week {monday}: settled {got:.2f} but the law says {law:.2f}"
    )

assert mod97_ok(BP5_IBAN) and mod97_ok(BP7_IBAN)
print("BP-005", BP5_IBAN, " BP-007", BP7_IBAN)
print(f"{len(estimates)} weekly estimates, {ESTIMATE_TRAIL_WEEKS} back + "
      f"{FORECAST_WEEKS} forward from {LAST_MONDAY}")
print(f"history opens {OPENING} ({HISTORY_WEEKS} weeks), "
      f"{len(OUTBOUND)} receivables / {len(INBOUND)} payables")
for i, n, b, o in ACCOUNTS:
    print(f"{n:24} opening {o:>10}  ->  balance {balance_of(i, o):>10}")
print(f"{len(tx)} transactions written to {OUT}")
