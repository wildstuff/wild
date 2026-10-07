# `sticky-note` — minimal `wild:ui/widget` teaching widget

The smallest complete Tier-2 **widget** plugin (ADR-0173). It exports
`wild:ui/widget@0.1.0` and renders a note card — a title and a body text
from the view's `config`, with an optional accent color on the card edge
— as a new view kind for the `wild-view` app renderer. The render is a
pure, deterministic function of its JSON inputs: no network, no secrets,
no host imports, so the whole contract runs on any machine with
`cargo test` alone.

## What a widget is

A widget renders ONE view inside an operator-published app. The host
calls two functions:

- `describe()` — returns the widget's metadata: its stable `kind` (the
  token an app spec's `widget_kind` names), a human label, a version,
  and a JSON Schema for its `config` blob (authoring surfaces use the
  schema; the widget still parses its own config at render time).
- `render(view-json, data-json)` — receives the serialized view spec
  and, for a data-bound view, the already-fetched governed records; it
  returns an **HTML fragment** (or a typed `widget-error`). The carrier
  places the fragment inside the view's card — V1 injects it as
  innerHTML, which is why this sample escapes every interpolated value.

This sample is config-only: `data-json` is ignored (`"null"` for a
bind-less view). The data-bound path is shown by the reference plugin
`plugins/widgets/hello-widget/` in the development repository.

## WIT surface

| Direction | Interface | Used for |
|---|---|---|
| export | `wild:plugin-meta/meta@0.3.0` | `manifest()` / `init()` / `shutdown()` — the Tier-2 lifecycle; `manifest()` is cross-checked against `sidecar.json` at load (slug/version mismatch = hard load error) |
| export | `wild:ui/widget@0.1.0` | `describe()` widget metadata + `render()` HTML fragment |
| import | — | none beyond the standard wasip2 runtime — a widget gets its inputs handed in as JSON strings |

The world ([`wit/world.wit`](wit/world.wit)) is two lines: the
`plugin-base` include and the widget export. Because it imports no
`wasi:*` package, the build needs no `wit-sync` step — a fresh public
clone builds immediately.

## Configuration

The widget reads no profile-level configuration — it is stateless. The
operator-facing settings live in the app spec, inside the `config` of a
`custom` view (ADR-0173 D3) whose `widget_kind` is `sticky-note`:

```yaml
pages:
  - id: notes
    views:
      - id: reminder
        type: custom
        widget_kind: sticky-note
        config:
          title: "Reminder"
          text: "Call the tax office on Monday."
          color: "#f59e0b"
```

| Field | Type | Default | Required | Description |
|---|---|---|---|---|
| `title` | string | `"Note"` | – | Heading of the card. |
| `text` | string | `""` | – | Body text; line breaks are preserved. |
| `color` | string | `var(--wv-accent)` | – | CSS color of the card's accent edge. The default is the app's own accent token, so an unconfigured note follows the operator's theme. |

All three values are HTML-escaped before interpolation — the fragment is
injected as innerHTML, so operator input must never become live markup
(see the `render_escapes_operator_input` test).

## Build

```sh
cd examples/widgets/sticky-note
./build.sh
# → target/wasm32-wasip2/release/sticky_note.wasm
```

`build.sh` is just `cargo build --target wasm32-wasip2 --release`. The
`wasm32-wasip2` target comes from the repo's `rust-toolchain.toml`;
rustup installs it on first use. `cargo test` (host target) exercises
the full render contract without any Wasm tooling.

> In the development repository, `source .shell-env` first — the shared
> build cache redirects `CARGO_TARGET_DIR`, so the artifact lands under
> `$CARGO_TARGET_DIR/wasm32-wasip2/release/sticky_note.wasm` instead of
> the local `target/`.

## Install — what works today, and what waits on a later rung

ADR-0173 shipped the contract, the host adapter and the dashboard
integration, and deferred dynamic discovery/installation to a later
rung. Since then the marketplace chain has shipped for **published**
widgets: a widget published to the OCI index installs through the
governed marketplace flow, which writes its catalog manifest plus a
trust record into the deploy dir (the *curated* ring). A **locally
built** sample like this one does not ride that chain — for it there
are two honest lanes, both manual:

**Lane 1 — server-side render (daemon plugin install).** Install the
component as a Tier-2 plugin; at the next daemon boot every installed
plugin exporting `wild:ui/widget` is wrapped in a server-side render
shim, and `POST /api/v1/tribes/{tribe}/widget-render` can render kind
`sticky-note` (this is the path the browser customer portal uses — it
cannot instantiate wasmtime):

```sh
wild plugin add target/wasm32-wasip2/release/sticky_note.wasm \
  --sidecar sidecar.json
# then restart the daemon (wild down / wild up)
```

**Lane 2 — widget catalog + native dashboard (deploy-dir copy).** The
catalog (`GET /api/v1/widgets`), `widget_search`, and the app-spec
compiler enumerate `<profile_root>/system/widgets/`; the native desktop
dashboard pulls each deployed kind's component bytes over the daemon
socket and registers it in its widget registry. Deploying is a
two-file copy — the `.wasm` and the `<kind>.widget.yaml` catalog
manifest, named by KIND:

```sh
# profile root: <profile_root>/ (or your $WILD_HOME override)
deploy=<profile_root>/system/widgets
mkdir -p "$deploy"
cp target/wasm32-wasip2/release/sticky_note.wasm "$deploy/sticky-note.wasm"
cp sticky-note.widget.yaml "$deploy/sticky-note.widget.yaml"
```

(`cargo run -p xtask -- widgets-deploy` automates exactly this copy, but
only for the in-tree `plugins/widgets/*` crates in the development
repository — it does not scan `examples/`.) A deploy-dir widget with no
`<kind>.trust.json` companion is honestly ranked in the *forged* trust
ring — an unsigned, tribe-local artifact.

What a single `wild plugin add` does **not** yet do is bridge the two
lanes: the plugin install does not populate the deploy dir, so the
catalog and the dashboard do not discover a widget from lane 1 alone.
For a locally built widget, one-command install that lands both lanes
(plus a real trust record) is the part that still waits on the later
discovery rung.

## See it render

- **Native dashboard:** after the lane-2 copy, restart the daemon, then
  `wild up` and `cargo run -p xtask -- dashboard desktop`. Any app whose
  spec carries the `custom` view above renders the note card; the
  dashboard logs and skips widgets that fail to load, so a typo in the
  kind degrades to built-ins, never a crash.
- **Server-side (no dashboard):** after lane 1, bring the daemon up with
  the REST plane bound (`wild up --rest-http 127.0.0.1:8088`) and render
  straight through the daemon door:

  ```sh
  curl -s -X POST "http://127.0.0.1:8088/api/v1/tribes/<tribe>/widget-render" \
    -H 'content-type: application/json' \
    -d '{"type":"custom","widget_kind":"sticky-note","config":{"title":"Reminder","text":"Call the tax office on Monday.","color":"#f59e0b"}}'
  # → {"html":"<div class=\"sticky-note\" …"}
  ```

## Turning it into a real widget

Keep both signatures; grow the render:

1. Parse a richer `config` in `render()` against your own schema, and
   return typed `widget-error`s (`invalid-config` / `invalid-data`) so
   the carrier can show an operator-legible failure instead of a broken
   card.
2. To consume data, have the view declare a `bind` — the host runs the
   bound entity/projection's governed, masked ui-query and passes the
   records as `data-json`; render them, and declare the shape you need
   in the catalog manifest's `data_shapes`.
3. Style with the renderer's `--wv-*` tokens (padding, radius, text
   sizes, colors) so the card follows the app's theme and font scale —
   a widget never hardcodes its own palette.
4. Keep escaping everything you interpolate. The fragment is innerHTML
   until a later rung moves the carrier to shadow DOM or an iframe.
