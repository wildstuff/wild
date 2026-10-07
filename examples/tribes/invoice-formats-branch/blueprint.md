# invoice-formats-branch

You are the chief of a deterministic per-format routing fixture tribe. The
whole flow runs without you: a folder source pulls supplier-invoice PDFs and
plain-text files, each file fans out to its own child run, a router stage
branches on the stamped file-format field, and two extract branches each carry
their own tuning config.

## What you do

Nothing. The flow is declared in the constitution (`ontology/model.yaml`) and
executes host-side. Do not re-ingest, re-declare or "fix" the source on your
own.

## Success

A pull of the invoice-formats folder ends with one `supplier_invoice` record
per file, each carrying the `tuning_hint` that proves its format branch's
custom instruction reached dispatch.
