---
name: liquidity-domain-glossary
version: 0.1.0
source: prose
scope: operator-judgement
tribe_id: liquidity-management
description: "Domain glossary for the liquidity-management example tribe. Bridges operator vocabulary (accounting tax terms) to the tribe's model fields and values."
---

# liquidity-domain-glossary

**When to use:** Whenever the operator of the `liquidity-management` tribe uses
plain-language accounting terms that must be mapped to the tribe's data model.
This skill lives in the tribe, not the platform — the mapping below is
tribe-owned knowledge. The canonical terms are English; the German original is
noted once as etymology where an operator may still say it.

## Operator vocabulary → model mapping

- **Input tax** (VAT paid; from German *Vorsteuer*) → `vat_amount` on records where `direction == inbound`.
- **Output tax** (VAT charged; from German *Umsatzsteuer*) → `vat_amount` on records where `direction == outbound`.
- **Liquidity** / cash position → derived from `invoice.amount`, `invoice.direction`, and `invoice.due_date`.
- **Open items** (from German *Offene Posten*) → `invoice` records whose `status` is not `paid`.
- **Creditor** (supplier we owe; from German *Kreditor*) → counterparty on `direction == inbound` (payable) invoices.
- **Debtor** (customer who owes us; from German *Debitor*) → counterparty on `direction == outbound` (receivable) invoices.

## Usage discipline

- Do NOT hard-code any of the above mappings in platform crates.
- When the operator asks for "input tax", translate to the model query
  (`direction == inbound` and aggregate `vat_amount`) before answering.
- If a term is missing from this glossary, ask the operator how they define it
  and propose an addition to the tribe's skill file rather than guessing.
