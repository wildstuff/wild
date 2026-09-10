# receipts-review-gather — agentic stage inside the gather flow

The committed example tribe behind `tests/e2e/tribes/05-receipts-review-gather-flow.sh`.
It is the first end-to-end proof that a **per-file agentic stage** (the
`review` step, dispatched to a scripted worker stand-in) can **pause, resume
from the bus, and then arrive at a gather barrier** — the join is fed by
resumed walks.

```text
folder pull ──► per-file child runs ──► review (agentic, scripted worker)
                                              │  pause → worker bus → resume
                                              ▼
                                        gather (collect) ──► notify-operator
                                              N → 1
```

Load-bearing details:

- The `receipts-inbound` **process extends its source** (same slug — the
  ADR-0118 D14 takeover), so the per-file children walk the process's later
  steps after ingest.
- The `review` step uses `does: review, using: receipt-review-worker`, where
  `receipt-review-worker` is the scripted `scripted-worker` fixture component
  registered in the daemon's component-type catalog by the e2e script.
- The scripted worker's identity fallback completes the natural-language brief
  successfully, so the child run resumes without an LLM call.
- The `review → gather` edge compiles to the explicit `join:gather` arrival
  form (ADR-0128 D5); the fan-out coordinator records `expected: N` and the
  LAST arrival runs the fold.
- Per-file fan-out is host-gated: set `WILD_CONNECTOR_FANOUT_MAX > 0`
  (default `0` keeps the sequential batch path).
- The notice body renders over the FOLDED item, whose `arrived`/`expected`
  stamps come from the barrier.

This bundle is deterministic and LLM-free throughout.
