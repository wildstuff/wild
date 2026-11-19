# Used-Car CRM — a dealership tribe

A **deployable** DDD tribe for a used-car dealership: mobile.de inventory
sync, customer + inquiry tracking, viewing appointments on a calendar, and a
price-negotiation flow where every offer goes to the owner. Distilled from a
production system (a German dealer, ~700 vehicles, WhatsApp-first customer
contact) into the platform's native shapes:

| Concept | Where it lives |
|---|---|
| 30-min mobile.de sync | `sources: mobile-de-inventory` (cron `*/30`, folder locator; the live Seller-API binding is a forged-connector swap, ADR-0159) |
| Vehicle stock | `vehicle` — a MIRROR aggregate, feed axes stay open tokens |
| "Never reveal the minimum price" | `pricing_floor` — its own `sensitive` aggregate, bound by no app view |
| Customers + inquiries | `customer` (phone-keyed) + `inquiry` + the `inquiry-triage` ai-worker |
| Viewing appointments | `viewing` (valid-time `date`) + the app's CALENDAR view |
| "Every negotiation goes to me" | `price_offer` + the `price-offer-approval` notify-and-wait process → `deal` |
| WhatsApp customer chat | NOT in the bundle — bound at deploy time (ADR-0167) to the ADR-0160 conversation agent |

> This bundle is also the **reference answer for the goal-statement walk**:
> `specs/goal-statement.md` carries the concept in the operator's own words,
> and the planned selection-eval scenario judges an Elder-built tribe against
> the shapes above. Authored in the **DDD lane** (`authoring_method: ddd`) —
> the ONE `ontology/model.yaml` is the constitution.

## Load it

```bash
wild tribe apply examples/tribes/used-car-crm/ --as my-dealership
```

One command pins the compiled ontology, ingests the seed data (8 vehicles,
5 customers, 5 viewings, 5 price floors) and deploys the chief + the
`inquiry-triage` worker. **No credentials, no LLM adapter needed for the
apply** — the sync reads a folder, the seed is committed CSV.

Then, optionally:

```bash
wild config llm list       # an adapter for the chief + triage worker
# drop a mobile.de CSV export into <profile>/inbox/mobile-de-export/
#   → the */30 cron mirrors it (or fire once via the intake_sync MCP tool)
# "connect our WhatsApp" in the Elder chat → ADR-0167 channel onboarding
```

`specs/onboarding.md` walks all four steps; `specs/domain.md` explains the
funnel and the two-price separation; `specs/goal-statement.md` is the concept
in operator vocabulary.

## What this example deliberately does NOT contain

- **Live mobile.de / WhatsApp credentials or adapters** — the committed
  example is deterministic and credential-free; live wiring is deployment
  configuration (egress grants, forged connector, channel bind), not bundle
  content.
- **The production guard layer.** The system this distils grew seven
  hallucination-guard modules out of real customer chats. Here the two rules
  that survived as STRUCTURE are the price-floor separation and the
  offer-approval flow; conversational guardrails belong to the ADR-0160
  conversation agent, not to a bundle.
- **Email / phone transcription / review management** — out of scope for the
  reference walk, listed in the charter's non-goals where relevant.
