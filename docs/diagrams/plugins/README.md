# Figures — plugins.md

Mermaid sources for [`../../plugins.md`](../../plugins.md). Edit a
`.mmd`, run `./render.sh`, and the exports under `rendered/`
regenerate; the doc embeds the same diagrams inline so GitHub renders
them without the export step.

| Figure | Section in the doc |
|---|---|
| `01-plugin-anatomy.mmd` | *What a plugin is* — component + signed sidecar → cache → trust gate → host |
| `02-delivery-tiers.mmd` | *Where the bytes come from* — native · embedded · installed |
| `03-install-flow.mmd` | *Installing a plugin* — pull, signature check, tier, hot-load |
| `04-trust-allowance.mmd` | *Trust — what a tier may do* — the three rungs and operator grants |
