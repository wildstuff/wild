# CONTRACTS — looked-up facts for `slugify`

Derived by the host at seed time from the live component catalog (`wild:registry/component-discovery.list-types`), the vendored WIT under `wit/deps/`, and the tribe's pinned record types this spec declares it reads (`wild:data/ontology.get-type`). Never hand-edited; re-derived on every seed. **Read this before you write code that calls a tool, reads a record type, or calls a host import.** A shape that is not in this file is not knowable from this workspace: emit `NEEDS_CLARIFICATION` naming the tool or import — do not guess it, and do not hedge across candidate shapes.

## Catalog tools this spec names

The spec names no tool or component present in the catalog (matched by whole token against the spec text). If the spec means you to call one, its name is not in the catalog as written — ask, do not pick a similar name.

## Tribe record types this spec reads

The spec declares no `- READS:` line, so no record type is rendered here. A tool that reads the tribe's records without declaring what it reads has no vocabulary to build against: ask for the `- READS:` line rather than inventing a type slug.

## Host imports this component may call

The seeded world imports no capability-gated host interface: the spec declares no `Capability:` tag this flavor may wire. Whatever the component does, it does without host calls.

## Tool policy — not readable here

The tribe's tool policy (`system/tool-policy.yaml`, a per-tribe deny-list) has no guest-readable door, so this file cannot say whether `invoke("<tool>")` will be admitted for this tribe. If a call comes back as a `tool-error` (`not-granted`, `unknown-tool`), that is a structural refusal, not a code defect (ADR-0297 D5): stop and emit `NEEDS_CLARIFICATION` quoting it. Do not retry under another tool name, and do not write code that tolerates both a refusal and a success as if they were two shapes of one answer.
