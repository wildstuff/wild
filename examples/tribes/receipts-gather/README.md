# receipts-gather — the minimal gather-flow reference

The smallest committed example of the ADR-0128 **gather** step: a folder
source whose per-file fan-out children converge on a `does: gather` barrier,
folding the whole pull into ONE item — and ONE ADR-0125 `notify-operator`
notice for the batch. Zero LLM turns anywhere.

The flow, in the operator's words: *"When the receipts folder is pulled,
gather the per-file results and tell the operator once how the batch went."*

```text
folder pull ──► per-file child runs (fan-out) ──► gather (collect) ──► notify-operator
                     one per CSV file                 N → 1                one notice
```

Load-bearing details:

- The `receipts-inbound` **process extends its source** (same slug — the
  ADR-0118 D14 takeover), so the per-file children walk the process's later
  steps after ingest.
- The `ingest → gather` edge compiles to the explicit `join:gather` arrival
  form (ADR-0128 D5); the fan-out coordinator records `expected: N` and the
  LAST arrival runs the fold.
- Per-file fan-out is host-gated: set `WILD_CONNECTOR_FANOUT_MAX > 0`
  (default `0` keeps the sequential batch path — then the whole pull is ONE
  run and the gather is a single-item fold).
- The notice body renders over the FOLDED item, whose `arrived`/`expected`
  stamps come from the barrier — honest completeness, straight from the run.

This bundle is the fixture behind `tests/e2e/tribes/04-receipts-gather-flow.sh`,
which drives it end to end on a real daemon (see `tests/e2e/tribes/00-fixtures.md`).
