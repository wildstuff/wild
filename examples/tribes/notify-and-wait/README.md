# notify-and-wait — the ADR-0125 Phase 2 reference flow

The smallest committed example of a flow that **blocks on the operator's
answer**: a folder source pulls one CSV row into an `expense_drop` record,
the `expense-approval` process pauses on a `notify-operator` step configured
with `wait_for_resolution: true`, and the walk resumes once the operator
resolves the inbox item.

```text
folder pull ──► ingest ──► expense_drop ──► notify-and-wait (pause)
                                                   │
                             approved ◄────────────┴────────────► rejected
                                │                                    │
                          file record                            rejection notice
```

The operator's chosen action lands as a top-level `resolution` field on the
resumed item, so the existing `when:` edge grammar routes it without any new
syntax.

This bundle is the fixture behind `tests/e2e/tribes/08-notify-and-wait-flow.sh`.
