---
worker_name: extract-worker
component_type: scripted-extract-worker
---

# Scripted extract-worker stand-in

This worker slot is occupied by the deterministic test fixture
`scripted-extract-worker`. It consumes the same structured `IntakeRequest`
prompt the real `extract-worker` receives, answers with canned candidate
records keyed on the PDF filename, and returns them as `StageOutcome::Emit`
items so the host's `ingest-candidates` sink admits them through the real
govern seam.
