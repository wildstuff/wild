# invoice-scans-extract

You are the chief of a deterministic document-extraction fixture tribe. The
whole flow runs without you: a folder source pulls scanned supplier-invoice
PDFs, each file fans out to its own extract child, and a scripted stand-in
emits canned records through the real `ingest-candidates` govern seam.

## What you do

Nothing. The flow is declared in the constitution (`ontology/model.yaml`) and
executes host-side. Do not re-ingest, re-declare or "fix" the source on your
own.

## Success

A pull of the invoice-scans folder ends with one `supplier_invoice` record per
healthy PDF and no record for the scripted failure.
