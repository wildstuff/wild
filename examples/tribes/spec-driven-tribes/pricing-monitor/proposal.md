# Pricing Monitor

> **Status:** dormant tribe — handwritten in spec-driven format
> for the spec-driven exploration (see `docs/adr/0034-spec-driven-tribes.md`).

## What

Sarah runs a niche e-commerce business and resells specialty
hardware. Three competitors anchor the price floor on her four
top SKUs. When any of them moves a price by ≥10% (up or down),
she wants to know within a few hours so she can react.

## Why a tribe (not a notebook)

This is process-shaped: the comparison must run on a cadence,
the trigger is "competitor changed price", and the output is
an alert Sarah reads on her phone. A notebook entry captures
the intent but doesn't run; a tribe runs.

## Walking the day

```
Morning:    cron fires at 08:00 UTC.
            chief reads competitor pages, extracts price per SKU.
Comparison: chief diffs against yesterday's snapshot in storage.
Decision:   if any |Δ| ≥ 10%, classify as flash-sale / restock /
            margin-cut and emit one alert per affected SKU.
Quiet day:  no movement → no message; chief writes "no-change"
            row to storage and ends the cycle.
```

## Key constraints from the operator's pitch

- Budget cap: ≤ $5/month at current claude-cli rates.
- Three competitor URLs (one is JS-rendered; cookie banner blocks
  direct fetches on competitor B).
- Alerts to Telegram (Sarah's primary channel); critical only.
- Don't alert on hour-boundary form-spam waves: those resolve
  within 60s and pollute downstream models.
