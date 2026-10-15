# `docs/assets/` — diagram sources

This directory holds the editable diagram sources used by the
docs. Two formats live side by side:

- **`*.excalidraw`** — the editable source (JSON; opens in
  [excalidraw.com](https://excalidraw.com)).
- **`*.png`** — exported render that the markdown docs embed
  inline (`<img src="assets/<n>.png">`). GitHub renders PNG
  inline; it does not render `.excalidraw` files inline.

`wild-deployment.svg` is the one hand-authored SVG source (open it in
any vector editor); its PNG re-renders with headless Chrome at 2×:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
  --window-size=1140,700 --screenshot=wild-deployment.png \
  "file://$PWD/wild-deployment.svg"
```

When you change a diagram, refresh both:

1. Open the `.excalidraw` file at https://excalidraw.com (drag-
   and-drop or **File → Open**).
2. Edit.
3. **Save back to source** — File → Save to → choose the same
   `.excalidraw` path. This updates the JSON.
4. **Export PNG** — File → Export image → PNG → save to the same
   directory with the same stem (e.g. `wild-overview.png`).
   Recommended export settings:
   - Background: `with background`
   - Scale: `2x` (sharper at GitHub's default rendering)
   - Embed scene: `on` (so the PNG itself can be re-imported into
     Excalidraw later — round-trip-safe).

## Current diagrams

| File | What it shows | Used in |
|---|---|---|
| `wild-overview.excalidraw` / `.png` | Control plane — how a user steers The Wild from their Desktop AI (Claude / Gemini) via MCP into Elder, who spawns and operates Tribes | [README](../../README.md), [docs/idea.md](../idea.md) |
| `wild-internal.excalidraw` / `.png` | System overview — Elder + three Tribes + Workers + Forge + foundation bar (NATS / JetStream / Wasm sandbox / SQLite) | [README](../../README.md), [docs/architecture.md](../architecture.md) |
| `wild-deployment.svg` / `.png` | What runs where — Wild.app + `wild` over `wild-hostd` (sandbox, host plugins, bus, store), `wild-appd` to the LAN, and the remote `wild-forged` builder with its three homes (ADR-0260) | [docs/architecture.md](../architecture.md) |
| `wild_organism_logo.svg` | Project logo (legacy "Organism" name; pre-Tribe rename) | currently unused |
