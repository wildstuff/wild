---
worker_name: receipt-review-worker
component_type: scripted-worker
---

# Scripted receipt-review stand-in

This worker slot is occupied by the deterministic test fixture
`scripted-worker`. The real `review` step hands it a natural-language brief
with the travelling item; the fixture's identity fallback completes
successfully so the per-file child run can resume and arrive at the gather
barrier without an LLM call.
