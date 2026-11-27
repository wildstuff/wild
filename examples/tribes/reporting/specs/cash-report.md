---
offer:
  type: document
  topic: cash-report
  aliases: [auswertung, weekly-cash-report]
---
# Cash Report

## Purpose
The weekly cash report: an evaluation over the area's cash position —
inflows vs. outflows, receivables aging, and the coverage outlook —
published as a document other tribes of the `finanzen` area (and the
operator) can consume.

## Requirements

### Requirement: weekly cash report
The system MUST publish a weekly cash report document summarising the
area's cash position.

Capability: files.read

#### Scenario: A week of movements is reported
- GIVEN a week's inflows, outflows and aging receivables in the `finanzen` area
- WHEN the weekly cycle fires
- THEN a cash report document is published for the area's tribes and the operator
