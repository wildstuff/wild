# Installation & first boot

> Companion to the [README](../README.md). Walks from a clean
> machine to a running Tribe.

## Prerequisites

- **Rust 1.97.0** — only needed for path (c) below (build from
  source). Pinned via `rust-toolchain.toml` at the repo root.
  `rustup` reads the toolchain file automatically; no manual
  version selection needed.
- **Apple-Silicon macOS, Linux or Windows** are the supported target
  surfaces for prebuilt binaries. Intel Macs get no published build (see
  below) and must build from source. Windows gets a packaged program
  directory rather than the tarball of host binaries the other two use,
  installed by `install.ps1` — see [Windows](#windows) below.
- **On Linux, a glibc distribution** — Debian, Ubuntu, Fedora and the
  like. The published builds are glibc-only, so Alpine and other musl
  systems cannot run them; the installer refuses such a host up front
  rather than leaving you with binaries that fail to start.
- **On a minimal Linux host, two system libraries** that a desktop
  distribution already has but a slim container image does not:
  `libdbus-1` (both `wild` and `wild-hostd` link it) and, if you want
  the local embed/rerank models, `libgomp1` (needed by `llama-server`,
  which `wild install-deps` fetches). On a `debian:stable-slim` base:

  ```bash
  apt-get install -y --no-install-recommends libdbus-1-3 ca-certificates libgomp1
  ```

  Without `libdbus-1` the daemon does not start at all —
  `error while loading shared libraries: libdbus-1.so.3`. Fonts, X and
  fontconfig are **not** required: PDF rendering carries its own fonts
  and runs on a host that has none.
- **NATS** — the embedded host can supervise its own NATS server
  (default), or you can point it at an existing one via
  `WILD_NATS_URL`. `wild doctor` reports which path you're on.
- **Docker** — only for a LOCAL build service, which is not the
  default. Components are built by a hosted build service
  (`WILD_FORGE_BACKEND=remote`, the default), and that needs
  nothing container-shaped on this machine. Set
  `WILD_FORGE_BACKEND=docker` to build here instead, and then
  Docker is required: without it `forge_build` requests fail with
  an explicit `forge-not-available` message saying whether Docker
  is missing or merely not running.

  `wild doctor` reports the build service either way — it asks
  whether components can be BUILT, not whether Docker is present,
  and names Docker only when you have chosen to build locally.
- **`cargo-component`, `wkg`** — only needed when building Wasm
  components from source (development workflow). Skipped for
  pure consumers; see `docs/development.md`.

## Install paths

**Start here: which are you installing?**

The **desktop app** is the product an operator uses — `Wild.app` on macOS, a
program directory on Windows. Everything else on this page installs the
**host binaries** (`wild`, `wild-hostd`, `wild-appd`) for a shell, a server or
a development machine. They are different artefacts, and one is not a way to
get the other: Homebrew and the curl installer never place `Wild.app`.

| You want | Go to |
|---|---|
| the macOS desktop app | [The desktop app](#the-desktop-app-macos) below |
| the Windows package (app + CLI + daemon together) | [Windows](#windows) |
| the CLI / daemon on macOS or Linux | (a), (b) or (c) below |

### The desktop app (macOS)

Download **`Wild-<version>-aarch64.dmg`** from the Assets list of a release on
the public distribution repo — <https://github.com/wildstuff/wild/releases> —
then open it and drag **Wild** onto **Applications**.

The steps, and whether a Gatekeeper prompt is involved, are written on the
release page itself and ship with each release
(`scripts/ci/render-release-notes.sh`). They are deliberately **not** repeated
here: that text has to change in the same breath as the signing lane, and a
second copy is how a user ends up following instructions for a dialog they do
not get, or missing one they do.

Two things worth knowing before you go looking for files:

- **Apple Silicon only.** No Intel build is published (see (a) below for why).
- **The bundle is self-contained** — the CLI, the daemon and the NATS server
  all ship inside it, so a `.app` install needs none of the paths below.
  The CLI is `Contents/Helpers/wild.app/Contents/MacOS/wild` if you want it on
  your `PATH` — or use the app's **"Install 'wild' command line tool"** action,
  which finds it for you.

  It moved there in ADR-0287 D1: reaching the macOS keychain needs an
  entitlement, an entitlement needs a provisioning profile, and a profile lives
  in a bundle. A bare executable in `Contents/MacOS/` cannot carry one.
- Your data does not live in the app. It is under
  `~/Library/Application Support/Wild`, which is why replacing the app is safe
  (see `upgrading.md`).

**Do not use a release marked *Pre-release* unless you mean to.** Every public
release so far is one, and they are the tester channel by decision (ADR-0225
D7).

### (a) curl-pipe installer (macOS / Linux)

```bash
curl -fsSL https://wildstuff.com/install | sh
```

Pulls the prebuilt host binaries from the latest GitHub Release
that matches your OS + arch, verifies the sha256, unpacks the program
tree into `~/.local/lib/wild`, and symlinks `wild`, `wild-hostd` and
`wild-appd` into `~/.local/bin` — already on `PATH` on most current
systems.

The program tree is kept out of the data root on purpose. Your profiles,
declarations and decisions live under
`wild_core::user_dirs::wild_root()` ([§ Where the root
is](profile-layout.md)), and `<wild_root>/bin/` is the managed slot the
first boot seeds with third-party sidecars. Program and data separate,
the same split the macOS bundle and the Windows package already make.
Before 2026-08-17 the installer wrote to `$HOME/.wild/bin`; if you
installed then, that copy is still there and may shadow the new one —
the installer says so and prints the `rm` for it.

Knobs (env vars):

| Var | Default | Effect |
|---|---|---|
| `WILD_VERSION` | latest **stable** GitHub Release | Pin to a tag (`v0.1.2`). Pre-releases are never the default — `releases/latest` skips them by definition, and ADR-0225 D7 turns that into the tester channel: name an `-rc.N` tag here to opt in. |
| `WILD_INSTALL_DIR` | `~/.local/lib/wild` | Where the program tree lands (binaries + `dist/` + `docs/`). `rm -r` on it is a complete uninstall of the program. |
| `WILD_BIN_DIR` | `~/.local/bin` | Where the three binaries are symlinked. |
| `WILD_NO_MODIFY_PATH` | `0` | Set to `1` to suppress the PATH-hint output. |
| `WILD_RELEASE_BASE` | GitHub Releases download base | Install from somewhere other than github.com — a local mirror or a copied-over tarball. Requires `WILD_VERSION`. |

**Air-gapped install.** A server that cannot reach github.com can still
be installed from a tarball you carried over. Download
`wild-<version>-<target>.tar.gz` **and** its `.sha256` sidecar on a
machine that has network, put both on the target host under
`<some-dir>/v<version>/`, then:

```bash
WILD_VERSION=v0.5.0 \
WILD_RELEASE_BASE=file:///srv/wild-assets \
  sh install.sh
```

The sha256 is still verified — a copied file can be truncated exactly
like a download can. This is the same path CI uses to prove a release
installs before it is published (`scripts/ci/smoke-install-linux.sh`).

Targets the script can install today (matches
`.github/workflows/release.yml`'s build matrix):

- macOS arm64 → `aarch64-apple-darwin`
- Linux x86_64 → `x86_64-unknown-linux-gnu`
- Linux aarch64 → `aarch64-unknown-linux-gnu`

**Intel Macs are not a published target.** `x86_64-apple-darwin` left
the release matrix on 2026-05-05: Apple stopped selling Intel Macs in
late 2023, and a macOS runner costs 10× per minute. Nothing has been
built for it since, so **both** doors refuse an Intel Mac up front:
`install.sh` by target detection, and the brew formula through
`depends_on arch: :arm64` inside its `on_macos` block. The
alternative is worse than a refusal in each case — a 404 on a URL the
operator never typed for the installer, and for brew a *successful*
install of Apple Silicon binaries that fail at the first exec,
because a Homebrew platform with no `on_*` block does not fail, it
falls through to the formula's default url. The way on is a source
build:

```bash
brew install --HEAD wildstuff/tap/wild
# or, from a clone:
cargo install --path crates/runtime/frontend   # wild
cargo install --path crates/runtime/daemon     # wild-hostd
```

A Mac running Apple Silicon is **not** affected by that refusal even
when `uname -m` says `x86_64` — a shell translated by Rosetta 2 reports
the process, not the machine, and the installer checks
`sysctl.proc_translated` before deciding.

Windows has an installer of its own — see [Windows](#windows).

### (b) Homebrew

```bash
brew install wildstuff/tap/wild
```

Custom tap (not homebrew-core yet). Currently a source build via
`cargo install` (3-5 min); will switch to a binary Formula once
release.yml's tarballs have a stable track record. Tap setup
notes: `docs/homebrew-tap-setup.md`.

### (c) Build from source

For any platform Rust supports — including the ones the release
matrix does not build (Intel macOS, musl distributions).

```bash
# Clone the repo
git clone https://github.com/wildstuff/the-wild
cd the-wild

# First-time setup: verify the pinned external tools are present
cargo run -p xtask -- check-tools     # `install-tools` fetches any missing

# Build the host binaries (wild + wild-hostd)
cargo build -p wild-frontend -p wild-daemon --release
# → ./target/release/wild and ./target/release/wild-hostd
```

Symlink the binaries into your `$PATH` (or `cargo install --path
crates/runtime/frontend && cargo install --path crates/runtime/daemon`
if you prefer):

```bash
ln -sf "$(pwd)/target/release/wild" /usr/local/bin/wild
wild --version
```

### Windows

```powershell
powershell -c "irm https://wildstuff.com/install.ps1 | iex"
```

Windows gets a different artefact from the other platforms, and the
difference is worth knowing before you go looking for files. Paths (a)
and (b) above install a handful of **host binaries** out of a tarball.
Windows installs a **packaged program directory** — the CLI, the daemon,
the desktop app, the NATS server and the default plugin set, assembled
together and versioned as one thing (ADR-0262).

So there are two directories, not one:

| | |
|---|---|
| `%LOCALAPPDATA%\Programs\Wild` | the **program** — what the installer writes |
| `%LOCALAPPDATA%\Wild` | your **data** — `<wild_root>`, what Wild stores |

An update replaces the first and leaves the second alone.

Afterwards **Wild is in your Start menu** — that is how you open it. You
do not need the path above, and you do not need a terminal again.

Wild can also **start when you log in** — off by default. The switch is
in the Wild icon's menu in the notification area ("Start Wild at
login"). Once on, Wild appears in **Settings → Apps → Startup** with a
switch of its own, so you can turn it off where Windows expects you to
look. It starts the app, which brings the daemon up; it does not
resurrect anything you closed on purpose.

Wild also appears in **Settings → Apps → Installed apps**, with its
version and publisher, and the Uninstall button there works. It removes
the program directory, the Start-menu entry and the `wild://`
registration — and deliberately keeps `%LOCALAPPDATA%\Wild`, your data.
It says so when it finishes, so deleting that too stays a separate,
deliberate step. Anything you put in the program directory yourself is
left alone as well; the uninstaller removes the files it installed by
name rather than clearing the folder.

The install is **per-user**: nothing is written to `%ProgramFiles%` and
nothing touches `HKLM`, so no step asks for elevation — not the first
install and not any update. The Start-menu entry is the per-user one
under `%APPDATA%` for the same reason; the all-users menu would need
elevation. `wild://` is registered under `HKCU` so a desktop
notification can be clicked open.

The CLI is a separate matter: `wild.exe` is **not** on your `PATH`
afterwards, and the installer prints the one command that adds it. You
only need this if you want to drive Wild from a terminal — the app
itself does not.

Knobs, set as environment variables before running the script (`irm |
iex` passes no arguments):

| | |
|---|---|
| `WILD_VERSION` | pin a tag, e.g. `v0.1.2`. Default: the latest **stable** release — pre-releases are never installed by default, and naming an `-rc.N` tag here is how a tester opts in (ADR-0225 D7) |
| `WILD_RELEASE_BASE` | fetch from somewhere else. Requires `WILD_VERSION` — an alternate source has no "latest" to resolve |
| `WILD_NO_MODIFY_PATH` | `1` silences the PATH hint |

**One thing it does not do.** It does not install a signed package —
there is no Authenticode certificate yet, so Windows SmartScreen warns
on first run. What it does do is verify the download's sha256 before
unpacking anything, and refuse the install outright on a mismatch.

## First boot — `wild up`

The runtime is **headless by default** (ADR-0010). `wild up`
spawns `wild-hostd` as a child process (ADR-0035 §3.3.2.C),
supervises it for the lifetime of the invocation, and drains it
gracefully on Ctrl-C / `wild down`.

```bash
wild up
# → wild up: starting wild-hostd...
# → boot sequence (daemon's stderr surfaces in your terminal):
#   ✓ NATS supervised (or external)
#   ✓ JetStream up
#   ✓ tribes.db migrated
#   ✓ embedded host ready
#   ✓ default chief (Tier-1.5) registered
#   ✓ ai-worker / http-fetcher / pdf-parser pulled (first run)
# → wild-hostd ready. Control socket answering.
# → wild up: attached. Ctrl-C drains the daemon.
```

To spawn the host as a detached `wild-hostd` daemon that keeps
running for later `wild` invocations to attach (instead of blocking
until Ctrl-C), pass `--daemon`; `wild down` shuts it down. (The
legacy `--in-process` flag was retired with ADR-0036 Phase H.)

For a daemon that survives shell exit + reboots (the recommended
shape for ongoing use), register the LaunchAgent / systemd unit
that ships in `dist/` — see
[`docs/operator-daemon.md`](operator-daemon.md).

What happens on first boot:

- **`<wild_root>/`** is materialised (profile dir, OCI cache,
  bootstrap.yaml seed copy if missing). That is the directory below —
  everything Wild stores lives under it, and `wild doctor` prints the
  resolved path for every file it checks.
- **`bootstrap.yaml`** drives which Tier-2 plugins get pulled —
  see `docs/bootstrap-and-default-inventory.md`
  for the full schema. The embedded default ships
  `ai-worker / http-fetcher / pdf-parser / anthropic-cli /
  openrouter / math-tools` enabled.
- **`bootstrap.lock`** pins each plugin to an OCI digest. Commit
  it for reproducible deploys, leave it gitignored on personal
  devboxes.
- **`tribes.db`** is the per-profile SQLite mirror. Schema
  migrations apply automatically on each boot.

### Where your files actually live

Two names appear throughout the docs. Both are directories on your own
machine; neither is a literal path, because each platform keeps
application data in its own place:

| | `<wild_root>` — everything Wild stores |
|---|---|
| macOS | `~/Library/Application Support/Wild` |
| Linux | `$XDG_DATA_HOME/wild`, else `~/.local/share/wild` |
| Windows | `%LOCALAPPDATA%\Wild` |
| any platform | `$WILD_HOME`, when set — it overrides all three |

`<profile_root>` is one profile inside it: `<wild_root>/profiles/<name>`.
It holds the files you edit (`profile.yaml`, `ELDER.md`,
`llm-adapters.yaml`, `tribes/`), while `<profile_root>/system/` holds the
daemon's own state — sockets, logs, the NATS config. That split is what
lets you copy or back up a profile without dragging opaque process state
along.

Never type the placeholder: run `wild doctor`, which prints the resolved
path for every file it checks. The full per-file matrix is
`docs/profile-layout.md`.

> The unix answer used to be `~/.wild` on both macOS and Linux. It moved
> so each platform gets its native location; older prose and older chat
> threads still use the dotfile spelling.

If you're laptop-developing and want one terminal with the watch
console wired in:

```bash
wild up --watch
# → starts the runtime AND opens the watch console in-process
```

For the conversational REPL, run `wild chat` in a second terminal:

```bash
wild chat
# → "What's your challenge?"
```

## Verify with `wild doctor`

`wild doctor` walks every layer the runtime touches and prints a
structured pass/fail report. Run it after `wild up` from another
terminal:

```bash
wild doctor
# wild doctor — runtime health checks
#
#   · preflight tools          (no PINS configured)
#   · claude CLI               /opt/homebrew/bin/claude (1.2.3)
#   ✓ nats-server binary       /opt/homebrew/bin/nats-server
#   ✓ <profile_root>/system/nats.conf
#   ✓ llama-server binary      <wild_root>/bin/llama/llama-server
#   ✓ llama embed model (vec)  bge-m3-embed @ http://127.0.0.1:11435 — healthy, 1024-dim vector
#   ✓ llama rerank model       bge-reranker-v2-m3 @ http://127.0.0.1:11436 — healthy, top score 0.912
#   ✓ NATS @ nats://127.0.0.1:4222
#   ✓ JetStream                ($JS.API.INFO ok)
#   ✓ tribe-state KV           bucket present
#   ✓ tribe-registry KV        bucket present
#   ✓ embedded runtime         host ready, 0 workloads
```

The two `llama …` rows fire a real embed / rerank request at the
loopback sidecars, so a green row means vec + rerank actually
answer — not merely that the binary and model files are present.
Before `wild up` has started the sidecars they soften to `·`
(`… sidecar not answering … — is wild up running?`); a missing
binary or model points at `wild install-deps` / `wild models
pull`.

A blocking failure (NATS unreachable, JetStream off) returns
non-zero. Soft signals (no runtime running yet, no demo data
seeded) print a hint but exit zero so `wild doctor` doubles as
"tell me what's running."

### PDF rendering needs nothing installed (ADR-0251)

Rendering a PDF — page previews, `render-document` in a content flow —
needs **no native library and no separate step**. Rasterisation runs
inside the `pdf-parser` Wasm component that ships with the daemon, the
same way CSV, JSON, XLSX and DOCX are read.

This is a change: until ADR-0251 it needed the native **Pdfium** shared
library, `wild install-deps` fetched it, a `wild doctor` row reported
whether it had bound, and a host without it had previews switched off.
All of that is gone — there is no library to install, no probe order to
learn, no row to read, and `PDFIUM_LIB_PATH` no longer does anything.

If you are upgrading, a `libpdfium` left behind by an older install is
inert. Deleting it is safe and reclaims about 7 MB.

## Connect — chat REPL or watch console

Bare `wild` (no subcommand) opens an inline chat REPL with native
scrollback / selection / Cmd-C — Claude-Code-style shell:

```bash
wild
# → "What's your challenge?"
```

Connection details (NATS URL, credentials, default profile) live
in `<wild_root>/cli.toml`. `wild config show` prints the resolved
chain. To target a remote daemon:

```bash
wild --host=ops.internal:4222
```

For live multi-tribe monitoring (Activity · Tribes · Bus panes),
run the watch console in a second terminal tab:

```bash
wild watch
```

The full surface map is in [`docs/cli.md`](cli.md). Slash-command
reference + streaming details: `docs/inline-chat-design.md`.
Watch console layout + Forge-pane extension: `docs/watch-dashboard-design.md`.

## First conversation — talking to Elder

When you hit "What's your challenge?", you're talking to Elder
(the system-tribe orchestrator, ADR-0001). Elder's job is to:

- **Onboard** — figure out what you want, ask clarifying
  questions, search prior conversations for similar topics.
- **Spawn** — when you're ready, deploy a Tribe with the right
  Chief + initial worker roster.
- **Route** — route subsequent user input to the right Tribe (or
  back to Elder for cross-tribe ops).
- **Operate** — apply blueprint changes, swap a worker image,
  stop a tribe.

Try:

```
> I want to monitor competitor pricing weekly and alert me when
  something changes by more than 5%.

Elder: That sounds like a recurring crawl-and-diff job. Before I
  spawn a tribe, let me ask a few things:
  - Which competitors? (URLs or names?)
  - "Pricing" — listed prices, or do we need to handle login /
    quote forms?
  - Where do you want the alert? Email, Slack, file, MCP-callback?
```

Elder will iterate with you, then offer to deploy. When you
confirm, a fresh Tribe materialises with the Chief running its
first cycle. From then on, the Tribe is the addressee — type
`/tribe <id>` in the chat REPL to switch the conversation to its
Chief.

## Bootstrap reproducibility

For multi-machine / production deploys, commit `bootstrap.lock`
alongside `bootstrap.yaml`. The lock pins each plugin to an OCI
digest:

```bash
wild bootstrap sync       # refresh lock from current manifest
wild bootstrap list       # show resolved digests
git add bootstrap.lock
git commit -m "pin bootstrap to verified digests"
```

The boot path refuses to start when the manifest and the lock
have drifted (`WILD_OFFLINE=1` overrides this for air-gapped
re-runs against a populated cache). Full schema + drift handling
in `docs/bootstrap-and-default-inventory.md`.

## Optional: sharpen recall with local embeddings

Everything above works with **exact-shape** recall: a Tribe (and the
Elder, when designing one) surfaces past lessons whose goal *shape*
matches the one at hand. You can optionally upgrade this to
**semantic recall** — matching *similar* shapes, not just identical
ones — by running a local embedding model. It is **off by default**;
without it, recall simply falls back to exact-shape matching, and
nothing else changes.

**Why turn it on**

- A Tribe recalls lessons from *adjacent* problems, not only ones
  shaped the same way — so it reuses experience more often.
- The Elder, while designing a new Tribe, draws on what the **whole
  fleet** has already learned about similar challenges (the knowledge
  commons — ADR-0058).
- It runs **fully local and zero-cost**: an Ollama model on your own
  machine — no data leaves the box, no per-token bill.

**Enable it**

```bash
# 1. Run Ollama locally and pull an embedding model
ollama serve
ollama pull nomic-embed-text          # 137M, 768-dim — the default

# 2. Build + install the embed-adapter sidecar into your profile
cd plugins/embed/ollama && cargo component build --release
wild plugin add target/wasm32-wasip1/release/ollama_embed.wasm \
  --name ollama-embed --kind provider \
  --provides wild:embed-adapter@0.1.0 \
  --config-key model --config-key endpoint --no-pull

# 3. Restart the runtime — the shipped default wires the adapter
wild up
```

The embedded `embed-adapters.yaml` already points `ollama-embed` at
`nomic-embed-text` on `http://127.0.0.1:11434`, so no further config
is needed for the default model. Confirm it wired in the boot log:

```
category: "boot.subsystem-up", subsystem: "wild:ai/embed", "… Tier-2 adapter wired"
```

**Ollama on a non-default port?** It's pure config — no rebuild. Edit
`<profile_root>/embed-adapters.yaml` and change the `endpoint`
field (e.g. `http://127.0.0.1:11500` if your Ollama listens
elsewhere), then `wild up`. The `--config-key` flags on `wild plugin
add` only *allow* those keys; the actual values are read from this
YAML, never from the install command. Swapping the `model` works the
same way **only for another 768-dim model** — the vector width is
fixed in the `learning` schema, so a different-dimension model is a
schema change that needs the learnings re-seeded (see
`embed-adapters.md`).

Once live, the daemon backfills embeddings for existing learnings on
its own (a periodic host pass) — **there is no command to run**. New
Tribes are born ready; Tribes created *before* you enabled it keep
working on exact-shape recall until their learnings are re-seeded.

> **Note (local build).** A from-source `wild plugin add` stamps the
> sidecar version `0.0.0+local`, which trips the version-integrity
> gate at warm-up. Set `version` in
> `<profile_root>/system/plugin-cache/ollama-embed.json` to match
> the component (`0.1.0`). An OCI-published adapter carries the right
> version and skips this.

> 🔍 **Dig deeper — embeddings & semantic recall.**
>
> - **Concept:** [`self-modeling.md`](self-modeling.md) (the relational learning layer; semantic recall in full) · ADR-0058 (knowledge commons + outcome history).
> - **Reference:** `embed-adapters.md` — the full `wild:ai/embed` adapter contract, the install walkthrough, and the conformance + live-E2E notes.
> - **On disk:** the registry is `<profile_root>/embed-adapters.yaml` (shipped default entry: `ollama-nomic-embed`).

## What's next

- **CLI cheatsheet:** [`docs/cli.md`](cli.md)
- **Inline chat REPL design:** `docs/inline-chat-design.md`
- **Watch console design:** `docs/watch-dashboard-design.md`
- **Elder walkthrough:** `docs/elder.md`
- **Configure LLM adapters:** `docs/llm-adapters.md`
- **Add a plugin:** [`docs/plugin-concept.md`](plugin-concept.md)
- **Develop on the host:** `docs/development.md`
