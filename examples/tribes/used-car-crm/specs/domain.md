# Domain spec — the dealership funnel

## The shape of the business

One inventory, one funnel. The inventory (`vehicle`) is a MIRROR of the
mobile.de listing stock — the dealer maintains listings there, the tribe syncs
them every 30 minutes and never edits them. Everything else is the funnel the
dealership itself owns:

```
inquiry ──► viewing ──► price_offer ──► deal
   │            │             │
customer ◄──────┴─────────────┘   (all three join `customer` by phone)
```

- **inquiry** — the raw demand signal, per contact channel. Reacted to by the
  `inquiry-triage` process (link customer, match vehicle, move funnel state).
- **viewing** — the appointment, the calendar axis of the tribe
  (`valid_time_field: date`). Lifecycle is tribe-owned: requested → confirmed
  → completed / cancelled / no_show.
- **price_offer** — the escalation record. Recording one fires the
  `price-offer-approval` process: the owner answers in the inbox, an accepted
  offer is FILED as a `deal` by the flow (never written by hand), a declined
  one produces a notice.
- **customer** — keyed by phone (the WhatsApp identity). `lead_status` is the
  funnel state; `won`/`lost` are human verdicts.

## The two prices

Every vehicle has TWO prices with different owners:

| Price | Lives on | Owner | Visibility |
|---|---|---|---|
| Asking price (`price_eur`) | `vehicle` (mirror) | the listing | public |
| Price floor (`min_price_eur`) | `pricing_floor` (own aggregate) | the dealer | internal, `sensitive` |

The separation is structural on purpose: the mirror stays pure feed shape, and
the floor — the production system's hardest guard rule ("never reveal the
minimum price") — is unreachable from every customer surface by construction,
not by prompt discipline. The published app binds no `pricing_floor` view; the
chat surfaces see the field masked.

## Demand signals

Two read-models fold the funnel into the numbers the dealer actually asks for:

- `vehicle_interest` — inquiries per vehicle: which cars draw attention.
  Together with `vehicle.days_on_lot` (read-time, `as_of() - listed_on`) it
  answers "what sits quiet and long" — the input to a human repricing
  decision, never to an automatic one.
- `viewings_by_day` — appointment load per day: how full the calendar is.
