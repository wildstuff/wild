# `slugify` — GENERATED, do not edit

This tree is unpacked from [`../../../assets/marketplace/templates/slugify-0.1.2.tar.gz`](../../../assets/marketplace/templates/slugify-0.1.2.tar.gz), the signed artefact an
installation actually verifies and builds. **The archive is the truth; this
directory is a convenience for reading it.** Edits here are overwritten by
`cargo run -p xtask -- marketplace-template-tree-write`, and a difference
between the two fails the parity gate.

The archive carries one thing this tree cannot: the compiled
`component.wasm` the sources above were built into. This repository ignores
`*.wasm`, so an unpacked copy here would be invisible to git — download the
archive if you want the artefact as well as the source.

To use it as a starting point, copy this directory somewhere of your own and
change it there. It declares its own `[workspace]`, so it builds where it
lands: `cargo build --release --target wasm32-wasip2`. Build in a copy rather
than in place — a `target/` directory here is an edit the archive does not
carry, and the parity gate says so.

To publish your own version of it, see
[`docs/template-with-a-build.md`](../../../docs/template-with-a-build.md).
