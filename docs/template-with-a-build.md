# A template that carries its build

A marketplace template normally ships **source**: you install it, your
Forge compiles it, and what runs is a component your own installation
built. A template that carries its build ships that source **and** the
compiled `component.wasm` beside it, plus enough recorded provenance for
you to rebuild it yourself and check that the bytes agree.

The source half is what makes it yours to change. The build half is what
lets it start without waiting on a compile. Neither replaces the other,
and the package is honest about which of the two you are trusting.

## What is in the package

`slugify` is the worked example — a tool provider that turns
`Grüße Über Ärger` into `gruesse-ueber-aerger`. Its package is a signed
`tar.gz` holding:

| Member | Why it travels |
|---|---|
| `template.json` | the manifest — name, version, flavor, WIT baseline, and the artefact's declaration |
| `operator-spec.md` | what the component does, in the words of the person deciding to install it |
| `src/**`, `Cargo.toml`, `Cargo.lock` | the source, and the exact dependency resolution the build used. The packed `Cargo.toml` is given an empty `[workspace]` table it did not have on the author's disk — see below |
| `wit/world.wit`, `wit/deps/**` | the contract it compiles against, frozen as it was at build time |
| `skills/**` | read by the source through `include_str!` — without them it does not compile |
| `golden/**` | the promises the component makes, as recorded input/output cases |
| `component.wasm` | the build itself |

The `golden/` cases are worth a second look. They are not documentation:
when you later evolve this component, the Forge diffs the new run against
these recorded promises and reports each one as kept, improved or
regressed. A package that left them out would hand you a component whose
every proven promise reads as a regression the moment you change it —
the baseline missing rather than failing.

### Why the manifest declares its own workspace

The packed `Cargo.toml` starts with an empty `[workspace]` table. Cargo walks
*upwards* from a manifest looking for one, so a template dropped anywhere
inside another Rust workspace — a monorepo, a scratch directory that happens to
have a `Cargo.toml` two levels up — is folded into that workspace and resolved
against *its* lock file, not the `Cargo.lock` packed beside these sources.
Without the table cargo refuses outright:

```
error: current package believes it's in a workspace when it's not
```

With it, the same tree builds where it lands. This was easy to miss for a
long time, because "standalone" was only ever tested where nothing enclosed
the template: the build sandbox holds one package and nothing above it, so
the case it fails in is the one case the sandbox cannot produce. A component
forged today is seeded with the table already in place; the packer adds it
to older folders that predate that, and leaves one that already declares it
untouched. If you assemble the `tar.gz` by hand, put it in yourself.


### Reading one without downloading it

Every published template is also unpacked into the repository, at
`examples/tool-providers/<name>/`, so the source can be browsed and copied
without pulling an OCI artifact first. Those trees are generated from the
archives and checked against them on every push, so they cannot drift — but
they are a convenience, not the artifact: the `tar.gz` is what carries the
signature, and it carries `component.wasm`, which the tree does not.

## What the manifest says about the build

```json
{
  "name": "slugify",
  "version": "0.1.2",
  "flavor": "tool-provider",
  "wit_baseline": "wild:tool-provider@0.4.0",
  "tools": ["slugify"],
  "artifact": {
    "digest": "sha256:f8329bf0…",
    "built_in": {
      "rustc": "rustc 1.97.0 (2d8144b78 2026-07-07)",
      "target": "wasm32-wasip2",
      "toolchain_reference": "ghcr.io/wildstuff/forge-sandbox:0.3.0",
      "toolchain_digest": "sha256:d50f1f5d…"
    }
  }
}
```

Two of those fields carry more than they look like.

**`digest` is the STRIPPED digest, not the digest of the file.** Every
artefact a Forge hands out gets a `wild.forge.receipt` section stamped
into it, signed under the installation's own key. Two installations
building identical source therefore produce files whose raw sha256
*always* differs — by construction, with nothing wrong. The comparable
digest is the one taken after that section is removed. If you hash
`component.wasm` directly and it does not match, you have measured the
receipt, not the code.

**`toolchain_digest` is something you can fetch.** Same rustc version is
not same toolchain: two sandbox images with rustc identical down to the
commit hash produced different bytes from identical source, because each
resolved dependencies from its own vendored registry and one transitive
crate differed. So the manifest names the image by **digest**, and
`Cargo.lock` travels, so the resolution is not left to chance either.

## The refusals are a guarantee, not a hurdle

It also refuses a `Cargo.toml` that declares no `license` (or
`license-file`). A template travels as source someone else will read, modify
and build, and a package that states no licence leaves them without the one
answer they need before starting. That gap is *named*, not filled in: the
packer supplies the `[workspace]` table because where sources get built is a
fact it can know, but a licence is a claim about who owns the work, and only
the author can make it. Note that a Forge does not write one today, so this
is a line you add to the component's own manifest before packing.


The packer will not put an artefact in a template unless the toolchain
that built it is one **you could obtain and check for yourself**. In
practice:

- a sandbox image pulled from a registry — you can pull the same digest;
- a pinned toolchain bundle whose sha256 the builder verified — you can
  fetch and hash the same tarball.

Anything else is refused **by name**, with the reason and the way out. A
locally built image names nothing outside the host that built it. A
mounted toolchain tree reports a version that is what the tree says of
itself, not evidence about its bytes. A builder's own `cargo` in dev
mode has no identity at all.

The point of refusing rather than recording the weaker case: every
`toolchain_digest` you ever read in a template manifest is a digest you
can act on. That is a property of the set, not a caution you have to
remember. The alternative — a field that sometimes holds an identity
nobody else can resolve — is exactly the kind of thing that looks fine
until someone relies on it.

**Source-only stays fully supported.** A template built in a toolchain
you cannot publish is still a perfectly good template; it just ships its
source and is compiled on installation, the way every template did
before this format existed. What an unobtainable toolchain forfeits is
the artefact, not the template.

## Verifying a package yourself

A template package is an OCI artifact of type
`application/vnd.wild.forge-template.v1` with two layers:

| Layer media type | Content |
|---|---|
| `application/vnd.wild.forge-template.v1+tar+gzip` | the package bytes |
| `application/vnd.wild.forge-template.sig.v1` | a raw 64-byte ed25519 detached signature over exactly those bytes |

The signature covers the **whole payload**, so verifying it is a
signature check over one file — no canonicalisation, no field ordering
to get right. The publisher key ships in this repository as
[`assets/marketplace/wildstuff-index.pub`](../assets/marketplace/wildstuff-index.pub),
64 hex characters — a 32-byte ed25519 public key.

```bash
# Pull both layers of the artifact
oras pull ghcr.io/wildstuff/templates/slugify:0.1.2 -o pkg/

# Verify: signature over the payload, against the shipped key
python3 - <<'PY'
from nacl.signing import VerifyKey        # pip install pynacl
key = bytes.fromhex(open("assets/marketplace/wildstuff-index.pub").read().strip())
sig = open("pkg/slugify-0.1.2.sig", "rb").read()
VerifyKey(key).verify(open("pkg/slugify-0.1.2.tar.gz", "rb").read(), sig)
print("signature OK")
PY
```

An installation does the same thing on your behalf and **refuses on
failure** rather than warning: a template is source you are about to
compile and run, so a signature that cannot be traced to a nameable
publisher stops the install.

## What happens when you install one

The catalog entry marks it `delivery: skeleton` and names
`template_ref` instead of an `oci_ref`, because a skeleton has no
author's binary to install — it is fetched, verified, and then built
through the same approval any component goes through.

Where the package carries an artefact, the signed bytes can start
immediately while the build runs behind them; when the rebuild finishes,
its stripped digest is compared with the one the manifest declared. A
mismatch revokes the running component and says so loudly rather than
leaving two plausible versions in play. The decision that governs this,
including what is and is not yet in force, is
ADR-0319.

## Packing and signing your own

Nothing above is specific to us. The format is a `tar.gz` with a
manifest, the signature is ed25519 over the payload bytes, and the key
that verifies it is whichever key your catalog entry's readers trust.

1. **Build your component through a Forge** so the workspace holds the
   recorded build environment your manifest will quote. Pull the sandbox
   image by digest rather than building it locally — otherwise the packer
   refuses the artefact, correctly, because nobody else could reproduce
   it.
2. **Pack the folder:**
   ```bash
   cargo run -p xtask -- marketplace-template-pack \
       --dir <forged-component-folder> --out my-template-0.1.0.tar.gz
   ```
   The packer reads a closed set of entries and refuses anything it does
   not recognise, naming the file — a template carries source someone
   will compile and run, and a packer that swept the directory would ship
   whatever was lying in it while the signature vouched for the lot.
3. **Sign with your own key** and push both layers under the media types
   above, using whatever registry you publish to.
4. **List it in your own catalog** with `delivery: skeleton` and a
   `template_ref` pointing at your ref.

An operator installs your template by trusting **your** publisher key.
There is no step in this that requires anything of ours.

## Glossary

| Term | One line | More |
|---|---|---|
| Skeleton | A catalog offering delivered as source to build, not a binary to install | ADR-0312 |
| Stripped digest | The artefact's digest with its install-specific receipt section removed — the only one comparable across installations | ADR-0319 |
| Golden case | A recorded input/output pair that states a promise the component makes | ADR-0301 |
| Toolchain source | Where a build's compiler and dependency set came from, as a closed set of cases rather than a string | ADR-0319 |
