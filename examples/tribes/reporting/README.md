# reporting — the minimal domain-sibling example

The smallest tribe that participates meaningfully in a **domain**
(ADR-0118 / ADR-0194): it declares `domain: finanzen` in its manifest
and advertises one capability offer (`cash-report`) via spec
frontmatter. Together with `liquidity-management` (which declares the
same domain) it forms the two-tribe `finanzen` domain fixture.

## Try it

```sh
wild tribe apply examples/tribes/liquidity-management --as liquidity-management
wild tribe apply examples/tribes/reporting --as reporting
```

Both tribes' `meta.yaml` now carry `domain: finanzen`, the
`tribe-registry` mirrors it, and the Elder's per-tribe grounding gains an
"Area context" section — ask either tribe's Elder *"which domain in the
finanzen area does the evaluations?"* and it answers from grounding.

## Layout

- `manifest.yaml` — name, `domain: finanzen`, no workers.
- `blueprint.md` — the mission.
- `specs/cash-report.md` — the `cash-report` offer (frontmatter drives
  the registry derivation, ADR-0034 §2.1 / ADR-0041).
