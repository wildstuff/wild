---
worker_name: extract-worker
component_type: scripted-extract-worker
---

# Scripted extract-worker stand-in

This worker slot is occupied by the deterministic test fixture
`scripted-extract-worker`. It consumes the same structured `IntakeRequest`
prompt the real `extract-worker` receives, answers with canned candidate
records keyed on the filename, and returns them as `StageOutcome::Emit` items
so the flow's `confirm` pause and the host's `ingest-candidates` sink see the
same submission shape the real extractor produces.
