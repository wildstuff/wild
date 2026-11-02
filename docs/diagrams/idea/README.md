# Figures for `idea.md`

A tunable illustration workspace for [`../../idea.md`](../../idea.md).
Everything here is editable source plus a rendered export.

```
scenes/   8 hero illustrations — editable .svg mockup + .png + an AI prompt (.md)
```

Three of them are shared with the [root README](../../../README.md): the
opening figure is its hero, and the story strip and the cast sheet carry
the same beats as its *Picture this* and *Concepts in 60 seconds*
sections. They live here rather than in a
folder of their own so the whole set stays one family — tune a scene once
and both doors follow.

The `scenes/*.svg` files are flat mockups that are deliberately simple —
correct in composition, palette, and labels, and meant to be tuned. Open
one in any vector editor (Figma, Illustrator, Inkscape, or Excalidraw via
*Import SVG*), or use the matching `*.md` prompt to generate a
higher-fidelity illustration and drop it in beside the mockup.

Palette and the shared style sentence live in
[`../how-tribes-live/scenes/style.md`](../how-tribes-live/scenes/style.md)
— one source for both sets, so the figures stay one family.

## Which figure goes where

| Figure | Where it's embedded | The claim it carries |
|---|---|---|
| `00-the-wild-story` | [README — the hero](../../../README.md) · [`idea.md` — the opening figure](../../idea.md) | The whole story at a glance: two piles that never met are joined into one model, the Tribe builds on it, and what it builds — model, tool, app — is yours from day one; the other way keeps nothing but the bill. |
| `01-read-it-all-vs-model-it-once` | [`idea.md` § Build, don't burn](../../idea.md#first-it-builds-a-model-of-your-world) | The model builds the map once, instead of walking the terrain again on every question. |
| `02-one-entity-many-artefacts` | [`idea.md` § A day in the wild](../../idea.md#a-day-in-the-wild) | An entity is what many artefacts together say about one thing — and every field still points at its page. |
| `03-folder-to-app-strip` | [README § Picture this](../../../README.md#picture-this) | Six steps from pointing at a folder to your partner opening an app. |
| `04-the-cast` | [README § Concepts in 60 seconds](../../../README.md#concepts-in-60-seconds) | Six nouns carry the whole model, with how many of each live where. |
| `05-behind-one-record` | [`idea.md` § Build, don't burn](../../idea.md#first-it-builds-a-model-of-your-world) | A record is an instance of a declared type, and that type is one node in the model everything reads. |
| `06-types-at-work` | [`idea.md` § Build, don't burn](../../idea.md#first-it-builds-a-model-of-your-world) | Named relations make a question a walk; declared effects say which actions the Tribe may take alone. |
| `07-forge-builds-not-burns` | [README § Why forge, when there's an LLM?](../../../README.md#why-forge-when-theres-an-llm) · [`idea.md` § Build, don't burn](../../idea.md#then-it-forges-its-own-tools) | Recurring work per model burns tokens and wobbles; forged code is deterministic and token-free — judgment to the model, repetition to code. |

Figures 2, 5 and 6 are one zoom, and are best tuned together: 2 looks at a
single entity from the outside (what arrived and what it adds up to), 5
steps back to the type that entity instantiates, and 6 steps back once more
to the types working as a domain — relations walked, effects gated.

The worked example across all of them is one household/energy domain —
**Partner · Contract · Invoice · Payment**, the type set `ontology.md`
already uses. Keep the names in step: an entity renamed in one figure and
not the others is the drift these three are most prone to.

## Re-rendering the PNG

The exports were made from the `.svg` with headless Chrome at 2× scale:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --headless --disable-gpu --hide-scrollbars --force-device-scale-factor=2 \
  --window-size=1000,584 --screenshot=scenes/01-read-it-all-vs-model-it-once.png \
  "file://$PWD/scenes/01-read-it-all-vs-model-it-once.svg"
```

Match `--window-size` to the SVG's own `width`/`height` — `1000,620` for
figure 0, `1000,584` for 1, `1000,560` for 2, `1000,648` for 3, `1000,600`
for 4, `1000,560` for 5, `1000,620` for 6, `1000,600` for 7. Any SVG→PNG
tool works; this one just happens to need no install.
