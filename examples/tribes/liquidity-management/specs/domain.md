# Liquidity Management — Domain Knowledge

## What liquidity is

Liquidity is the ability to meet all due payment obligations at any
point in time. It is distinct from profitability: a business can be
profitable on paper and simultaneously insolvent because invoices
arrive after obligations fall due.

The core tension: receivables come in on their own schedule, payables
must go out on a contractual schedule. The gap between those two
schedules is where liquidity crises originate.

## Legal thresholds (Germany)

### §17 InsO — illiquidity, German Insolvency Code (payment insolvency)
A business is legally insolvent when it cannot meet more than 10% of
its currently due payment obligations. At this point the managing
director has a personal obligation to file for insolvency within
three weeks (§15a InsO). This is the primary threshold you guard.

### §19 InsO — over-indebtedness
The liabilities exceed assets at liquidation values. A separate
insolvency trigger, usually relevant for GmbH entities. Not your
primary domain but worth flagging if the operator mentions balance
sheet issues.

### Practical standard: 13-week rolling forecast
Creditors, auditors, and restructuring advisors universally use a
rolling 13-week (3-month) cash-flow forecast as the documentation
standard for demonstrating ongoing payment capacity. Build toward
this as soon as P2 data is available.

## Core KPIs

| KPI | Definition | Healthy range |
|-----|-----------|---------------|
| DSO (Days Sales Outstanding) | Average days from invoice issue to cash receipt | < 45 days |
| DPO (Days Payable Outstanding) | Average days to pay supplier invoices | 30–60 days |
| Cash Conversion Cycle | DSO − DPO | As low as possible |
| Liquidity Grade I | Cash / current liabilities | > 0.2 |
| Liquidity Grade II | (Cash + receivables) / current liabilities | > 1.0 |
| Liquidity Grade III | Current assets / current liabilities | > 1.2 |

## Key mechanisms the chief must understand

### Cash discount (early-payment discount; from German *Skonto*)
Standard German invoice terms offer 2% discount if paid within 10
days ("2/10 net 30"). Taking the cash discount earns an annualised equivalent
of ~36% — almost always worth it if cash allows. Alert the operator
when overdue payables mean cash-discount windows are being missed.

### Factoring
Selling outstanding receivables to a factoring company at a discount
to convert them to immediate cash. Relevant when DSO is high and
short-term cash is tight. The chief does not execute this but should
mention it as an option when receivables aging shows stress.

### Overdraft line (overdraft facility)
A revolving credit line tied to the current account. It is a
short-term bridge, not a permanent liquidity source. Drawing it
continuously is a warning sign; exhausting it is a crisis signal.

### Seasonal patterns
Most businesses have predictable liquidity troughs (often Q1 and
Q3 in Germany). Knowing the operator's seasonal shape lets the
chief pre-warn rather than react.

## Data model

The tribe's ontology (`ontology/seeds.yaml`) is three layers:

**source_mirror** — mirrored from external feeds, immutable:
- `transaction` — booked bank movements (the cash ground truth)
- `invoice` — receivables (outbound) and payables (inbound), one type
  split by `direction`. **Mixed-origin** (ADR-0082 Inc 1): the mirrored
  feed fields sit beside an authored working-state block (`verified`,
  `effective_status`, `aging_state`, `notes`, …) the verbs write — no second overlay
  type, the fold keeps the two write-paths apart.

**authored** — the tribe's own working records:
- `bank_account` — cash accounts
- `business_partner` — debtors (owe us) and creditors (we owe)
- `payment` — a payment intent with its own lifecycle
  (planned → approved → executed)
- `dunning` — escalating reminders on overdue receivables
- `credit_facility` — overdraft line tracking
- `payment_promise` — a debtor's self-service promise-to-pay, authored
  by the customer over the customer-facing Domain surface (ADR-0083)

**derived** — computed, read-only:
- `cash_flow_13w` — gross movement over the rolling 13-week window

## Relationships

```
transaction      ──booked_on────→ bank_account
invoice          ──with_partner─→ business_partner   (debtor or creditor)
invoice          ──settled_by───→ transaction        (reified link: matched_amount/matched_on)
payment          ──settles──────→ invoice
payment          ──drawn_from───→ bank_account
payment          ──pays─────────→ business_partner   (the creditor)
dunning          ──chases───────→ invoice
dunning          ──reminds──────→ business_partner   (the debtor)
credit_facility  ──tied_to──────→ bank_account
payment_promise  ──promises─────→ invoice
```

A **receivable** is an outbound invoice with no `settled_by`
transaction (a debtor owes us). A **payable** is an inbound invoice
with no `settled_by` transaction (we owe a creditor). The forecast is
the union of expected settlement dates against current and projected
balances.

## The action flow (domain verbs)

Actions are domain VERBS (`ontology/verbs/*.yaml`) — each maps to an
effect with a risk tier. Low-risk verbs auto-admit; medium/high open a
decisions row the operator confirms. Verbs write onto the authored
overlays, never onto the immutable source mirror.

**Payables side — pay a creditor (three deliberately separate steps so
money never moves in one unguarded click):**

```
verify_invoice    (low → auto)   check amount/partner/terms
   ↓
take_cash_discount (low → auto)  optional: take the early-payment discount
   ↓
schedule_payment  (low → auto)   payment: planned
   ↓
approve_payment   (medium → GATE)payment: approved   ← operator confirms
   ↓
pay_invoice       (high → GATE)  payment: executed   ← operator confirms; cash moves
   ↓
reconcile_transaction (low)      match the bank movement back to the invoice
```

`pay_invoice` is HIGH risk — it never auto-admits and never graduates
to autonomy (the ceiling is hard). In production it stages a write-back
intent a banking connector drains, with compensate-on-failure.

**Receivables side — chase a debtor:**

```
dun_invoice       (medium → GATE)level 1 → 2 → 3 (friendly → formal → pre-legal)
write_off_invoice (high → GATE)  give up on an uncollectable receivable
```

Dunning is gated because the notice goes out over the operator's name;
write-off is gated because it has tax and audit consequences.

## What is off-mission

Payroll data, HR headcounts, web analytics, inventory, marketing
spend, fitness metrics — none of these have a direct cash-timing
relationship. They may matter to the business but they do not belong
in this tribe. When the operator offers them: decline clearly, name
the reason, and redirect to the next needed data source.
