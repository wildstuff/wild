# Onboarding — from apply to a working dealership CRM

## Step 0 — apply

```bash
wild tribe apply examples/tribes/used-car-crm/ --as my-dealership
```

One command: compiles the model, pins the ontology + verbs, ingests the seed
stock (8 vehicles, 5 customers, 5 viewings, 5 price floors), deploys the chief
and the `inquiry-triage` worker. The seed works with NO credentials and no LLM
adapter; the chief needs one configured (`wild config llm list`) before its
first cycle.

## Step 1 — the inventory sync

The `mobile-de-inventory` source watches the profile-relative folder
`inbox/mobile-de-export` on a `*/30 * * * *` cron. Drop the mobile.de CSV
export there (columns: `ad_id, make, model, first_registration, mileage_km,
price_eur, fuel, gearbox, color, status, listed_on`) and the mirror follows —
Chief-free, no token spend.

Going LIVE against the mobile.de Seller API later is a connector swap
(ADR-0159 forged binding + `wild egress allow services.mobile.de`), not a
model change: same target type, same cadence, different locator.

## Step 2 — the WhatsApp channel

The customer conversation surface is NOT part of this bundle by design —
channels are bound per deployment, by chat (ADR-0167):

> "connect our WhatsApp"

and the Elder walks the onboarding (token, webhook, self-test). The bound
channel + the ADR-0160 customer conversation agent are what turn a customer
message into `record_customer` + `log_inquiry` + `schedule_viewing` calls.
Until a channel is bound, the same verbs work from the operator chat and the
app's forms — the tribe is complete without the channel, just quieter.

## Step 3 — publish the app

`apps/showroom-cockpit.yaml` ships ready: overview (demand tiles + interest
chart), the stock table, the viewing CALENDAR + kanban, and search. Publish it
to give the sales floor a surface without any operator tooling.

## Step 4 — live operation, in one sentence each

- New inquiry → `inquiry-triage` links customer + vehicle, funnel stays honest.
- Viewing wanted → `schedule_viewing` (status "requested"), a human confirms.
- Price offer → `record_price_offer` → the OWNER's inbox decides; unanswered
  for a day means declined.
- 18:00 reflect → the chief summarises inquiries, tomorrow's viewings, open
  offers, and quiet long-standing listings.
