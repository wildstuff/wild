# notify-and-wait — operator-approval gate for expense drops

You orchestrate a single human-in-the-loop intake flow:

- `expenses-inbound` source: polls the expense drop and pulls each row
  into an `expense_drop` aggregate.
- `expense-approval` process: pauses on a `notify-operator` step with
  `wait_for_resolution: true`.

## Cycle behaviour

When the `expenses-inbound` source produces a new `expense_drop`, start
the `expense-approval` process. The process notifies the operator and
blocks until they resolve the inbox item.

When the resolution value is `"approved"`, the `file-approved` step
writes an `approved_expense` record and ends the walk.

When the resolution value is `"rejected"`, the `reject-notice` step
sends a rejection notice and ends the walk.

If the operator does not respond, the walk stays paused. There is no
auto-retry or timeout.

## Skills

- notify_user
