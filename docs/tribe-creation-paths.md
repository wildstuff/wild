# How a Tribe comes to exist — three paths, three audiences

A Tribe can be born three ways. They are **not** competitors — they sit
at different layers, and the two file-based ones are built on the same
authoring lane. This page exists because it is easy to reach for the
wrong one: "share a domain with someone" and "spin up my example tribe"
feel similar but are different jobs.

The short version:

| I want to… | Use | Audience |
|---|---|---|
| describe an idea and have it shaped into a Tribe | **Elder Genesis** (chat) | operator (the default door) |
| materialize a whole authored Tribe from a bundle on disk | **`wild tribe apply <dir>`** | engineer / dev / dogfood |
| receive a shared domain into a Tribe | **`wild package install <dir>`** | operator receiving from a publisher |
| hand a domain to someone else | **`wild package export <dir>`** | operator publishing |

## 1. Elder Genesis — the guided door

You tell Elder what you want in plain language; the Intake/Mentor
dialogue shapes a charter, and you activate when ready (see
[`how-tribes-live.md`](how-tribes-live.md) § The life of a Tribe). This
is the **operator-facing** way to create a Tribe from nothing — no YAML,
no bundle directory. Prefer it for a Tribe you are authoring yourself.

## 2. `wild tribe apply` — instantiate a whole Tribe from a bundle

`apply` takes a **bundle directory** (`examples/tribes/liquidity-management/`
is the worked example) and materializes the *entire* Tribe from its
FS-canonical source: the compiled DDD ontology, its **workers**, the
**Chief**, the **blueprint**, **skills**, **settings**, enrichment rules,
its **live source bindings (real folder/URL locators)**, and its **seed
data**. `--as <slug>` spins several instances off one base.

This is the **authoring/instantiation primitive** and an
engineer/dev surface (you point at a directory of YAML/Markdown). It is
what the dogfood walkthrough and the e2e suite use to stand up a real,
running Tribe. It is *not* the way to give a domain to a third party —
a bundle carries your real locators, your data, and runtime wiring that
should stay home.

## 3. `wild package export` / `install` — share a domain across installs

A **domain package** is the clean, official way to hand a domain to
someone else. It is a versioned, provenance-stamped bundle of
**declarations only** — entity types, presentation, effect declarations,
optional app templates, and (opt-in) redacted example rows. It carries
**no** code, no named connector, no credential, no live locator, and no
workers or Chief; those are either runtime-wired or origin-bearing, and
the sanitization pass deliberately strips them.

- **`wild package export <out> --tribe <t>`** reads a live Tribe and
  writes a package: it decompiles the ontology back to a `model.yaml`
  and **disarms every source** to an `unbound` template (locator +
  secret handle blanked, cadence kept as a hint). A read — it mutates
  nothing. `--with-example-data` also ships ≤50 redacted sample rows per
  type (ADR-0156 OQ6.3).
- **`wild package install <dir> --tribe <t>`** grafts those declarations
  **additively** into a Tribe, stamped `Package(name@version)`,
  collision-refused, and upgrade-arbitrated (your `local`-edited fields
  ride along). The imported sources arrive `unbound` — the readiness
  signal reads *"waiting for your folder"* until **you** bind your own
  feed. `--with-example-data` (opt-in) ingests the sample rows under a
  deletable `package-sample` source.

See [`cli.md`](cli.md) for the exact flags and
`adr/0156-domain-packages.md` for the
design.

## Why `apply` is not replaced by packages

`package install` is **built on** `apply`'s machinery: it compiles the
model with the unmodified DDD compiler, merge-persists into the same
`ontology/model.yaml` the Tribe owns, and sinks its commands through the
same daemon door `apply` uses. A package presupposes a real Tribe at
both ends — one you exported *from* and one you install *into*, each
created by `apply` or Elder. Remove `apply` and you remove the ground
packages stand on (and the substrate the package e2e tests run on).

So: **author with Elder or `apply`; distribute with `package`.** The
example bundles under `examples/tribes/` stay what they are — developer
and e2e fixtures, and the source a package is exported *from* — never
the channel for handing a domain to a customer.
