# Operator guide — running `wild-hostd` as a system daemon

ADR-0035 §3.3 split `wild` into a frontend CLI + a long-running daemon
called `wild-hostd`. This guide covers the operator-facing side:
how to install the daemon as a system-managed service so it survives
reboots / logouts and gets restarted on crash.

> **Status:** ADR-0035 §3.3.4 shipped. Init-script artefacts ship in
> the repo under `dist/`; the dual-binary release pipeline ships both
> binaries in the GitHub Release tarball, and the brew-tap formula
> installs them (see `docs/homebrew-tap-setup.md`). For a from-source
> install, `cargo install --path crates/runtime/frontend` (the `wild`
> cli) and `cargo install --path crates/runtime/daemon` (`wild-hostd`),
> or copy the binaries built into your per-worktree `target/release/`.

## What `wild-hostd` is

The same runtime `wild up` boots, minus the TUI. Both binaries share
the bring-up sequence end-to-end
(`wild_bootstrap::bootstrap::run_up_with`). The split:

- **`wild`** — frontend. Talks to a running daemon over the IPC
  control socket (`<profile>/system/runtime.sock`). All of `wild
  chat`, `wild tribe apply`, `wild status`, `wild log-level`, `wild
  reconcile`, `wild metrics`, `wild debug` are thin clients.
- **`wild-hostd`** — daemon. Owns the `wild_host::Host`, the
  reconcile loop, the embedded-deploy NATS subscriber, and the
  control socket. Headless by construction; logs to
  `<profile_root>/system/logs/wild-hostd.log`.

Today (post-3.3.3) you can run them in two shapes:

| Mode | Command | Notes |
|---|---|---|
| In-process | `wild up` | `wild` boots the host inside its own tokio runtime. The TUI exits with the daemon. Default for laptops. |
| Out-of-process | `wild-hostd &` then `wild chat`/`wild watch`/etc. | Daemon survives the TTY closing. The next tranche flips this to the default. |

The init-script artefacts in this PR are for the **out-of-process**
mode — wired into systemd / launchd so the daemon is managed like
any other service.

## macOS — launchd LaunchAgent

```sh
bash dist/install/install-launchd.sh
```

What it does:

1. Resolves your `wild-hostd` path (`command -v wild-hostd`) — works
   for brew (`/opt/homebrew/bin/...` on Apple Silicon, `/usr/local/bin/...`
   on Intel) or cargo (`~/.cargo/bin/...`).
2. Renders `dist/launchd/com.wildstuff.wild-hostd.plist` with that
   path and writes it to `~/Library/LaunchAgents/`.
3. `launchctl bootstrap`-loads + enables + kickstarts the agent.

After install:

```sh
# status
launchctl print gui/$(id -u)/com.wildstuff.wild-hostd

# stop / start manually
launchctl kickstart -k gui/$(id -u)/com.wildstuff.wild-hostd     # restart
launchctl kill TERM gui/$(id -u)/com.wildstuff.wild-hostd        # stop

# logs
tail -f <wild_root>/profiles/$(cat <wild_root>/active-profile)/system/logs/wild-hostd.log
```

Uninstall:

```sh
bash dist/install/install-launchd.sh --uninstall
```

The script `bootout`s the agent and removes the plist. Operator
state under `<wild_root>/` is untouched.

### LaunchAgent vs LaunchDaemon

The shipped plist is a per-user **LaunchAgent**, not a system-wide
LaunchDaemon. Reasons:

- Wild's profile state lives at `<profile_root>/` (per
  ADR-0026). A LaunchDaemon running as root would need a separate
  root-owned profile root + ownership rules.
- LaunchAgents start at user login; LaunchDaemons start at boot. The
  brew/cargo install model matches "start when I log in".
- An operator-side install doesn't need the privilege escalation a
  LaunchDaemon costs.

If you genuinely need a system-wide daemon (multi-user host, headless
CI runner, kiosk), copy the plist into `/Library/LaunchDaemons/`,
adjust `UserName` / `ProgramArguments` / log paths to match a
service account, and `sudo launchctl bootstrap system ...`. That
flavour is not covered by the install helper.

## Linux — systemd user service

```sh
bash dist/install/install-systemd.sh

# …or, to publish end-user apps from this server as well:
bash dist/install/install-systemd.sh --with-appd
```

The paths above are relative to a repo checkout. If you installed with
the `curl … | sh` installer, the same scripts sit beside the binaries —
`$WILD_INSTALL_DIR/dist/install/` (default
`~/.local/lib/wild/dist/install/install-systemd.sh`), and the installer prints
that line when it finishes. They work unchanged from there: the script
resolves its unit template relative to its own location, not to a
checkout.

What it does:

1. Resolves your `wild-hostd` path the same way as the macOS
   script.
2. Renders `dist/systemd/wild-hostd.service` with that path into
   `~/.config/systemd/user/`.
3. With `--with-appd`, writes a drop-in
   (`wild-hostd.service.d/10-appd.conf`) enabling the end-user portal
   — see below.
4. `daemon-reload`s + `enable --now`s the service.

After install:

```sh
# status
systemctl --user status wild-hostd

# stop / start
systemctl --user stop wild-hostd
systemctl --user start wild-hostd

# follow daemon log via journald
journalctl --user -u wild-hostd -f

# the rotated tracing log lives in the profile root regardless
tail -f <wild_root>/profiles/$(cat <wild_root>/active-profile)/system/logs/wild-hostd.log
```

Uninstall:

```sh
bash dist/install/install-systemd.sh --uninstall
```

### The end-user portal on a server

`wild-appd` is the door your people walk through — the LAN-exposed
process that serves published apps while `wild-hostd` stays private
behind it (ADR-0154 D3). On a laptop you get it with
`wild up --with-appd`; under systemd there is no `wild up`, so the
installer's `--with-appd` writes the same intent as a drop-in:

```ini
[Service]
Environment=WILD_WITH_APPD=1
Environment=WILD_CUSTOMER_BIND=127.0.0.1:8090
```

There is deliberately **no second unit**. The daemon spawns the portal
itself and supervises it (restart-on-death, crash-loop breaker), so a
`wild-appd.service` alongside would be a second owner of one process.
Both lines matter: `WILD_CUSTOMER_BIND` opens hostd's private customer
listener (keep it loopback — it is the plane appd proxies to), and appd
binds `WILD_APPD_BIND`, default `0.0.0.0:8091`, for the network. Set
only one and you get a portal that answers but reaches nothing.

The daemon finds the binary as a sibling of itself, then `<profile_root>/bin`,
then `PATH` — it ships in the same release tarball as `wild-hostd`, so
keep the two together. Check it from the box:

```sh
wild app status                       # the published URL, and whether it connects
curl -s localhost:8091/healthz        # "hostd_reachable": true is the one to read
```

Turn it back off by re-running the installer without the flag.

### Headless servers

`systemctl --user` needs a logind session. On a headless box without
a persistent shell, the user manager dies on logout and the service
dies with it. Run `loginctl enable-linger <user>` once to keep the
user manager alive across logout — then the service runs uninterrupted
even when nobody's logged in.

```sh
loginctl enable-linger "$USER"
systemctl --user enable --now wild-hostd
```

### User vs system unit

Same logic as macOS: per-user matches the brew/cargo install model
and avoids the root/profile-root ownership question. For a system
unit (e.g. a CI runner where the daemon should run under a service
account), copy the file into `/etc/systemd/system/`, swap `[Install]
WantedBy=multi-user.target`, set `User=<service-account>` /
`Environment=HOME=...`, and `systemctl daemon-reload && systemctl
enable --now wild-hostd`. Out of scope for the helper.

## Verifying the daemon

Both install paths run `wild-hostd` headless. The cli verbs see it
the same way they'd see an in-process `wild up`:

```sh
$ wild status
host id     : <id>
tribe       : root
state       : Running
uptime      : 47s
…

$ wild metrics
host id        : …
primary tribe  : root
uptime         : 47s
components     : 4
…

$ wild reconcile
✓ kick enqueued for every registered tribe
  reconcile kick enqueued; the loop will tick on its next select cycle
```

If `wild status` reports "no runtime is up" while the OS service
manager says it IS running, the daemon's listener didn't bind — check
the log file in the profile root for the bootstrap error.

## Reaching a remote daemon

The common case is a dashboard on your Mac against a `wild-hostd` on a
Linux box. The zero-infrastructure path is an **SSH tunnel** (the recipe
below). Binding the daemon to a reachable address directly is possible,
but it is a deliberate two-step act with a checked precondition — see
§ Binding the operator plane externally.

### Why the bind is refused by default

Pointing the listener at a reachable address (`WILD_REST_BIND=0.0.0.0:7532`)
without the opt-in is refused, with

```text
rest-http: the operator plane is loopback-only (bind 0.0.0.0:7532 is not a
loopback address); deliberately exposing it takes BOTH
WILD_REST_ALLOW_EXTERNAL=1 AND an inbound TLS identity (WILD_TLS_CERT +
WILD_TLS_KEY) — ADR-0050 §7
```

The reason is **inbound TLS**: on a reachable interface without it, the
bearer token would cross the network in clear. Per ADR-0050 §7 Amendment
(2026-08-18, second) the edge is a *checked* precondition, not advice —
the daemon permits a non-loopback operator bind only when it can construct
a TLS acceptor from the configured identity, and the listener then serves
TLS itself. There is no plaintext non-loopback operator plane, on either
door (REST or MCP). (The *customer* listener — `--customer-http` /
`WILD_CUSTOMER_BIND` — may bind externally: it serves only
`customer_visible`-filtered, tribe-scoped domain routes, no operator
tool. A public webhook — a WhatsApp or other channel provider posting to
`…/inbound/{provider}` — belongs there and needs no tunnel.)

A tunnel needs none of that: the daemon keeps binding loopback, SSH
carries the bytes encrypted, and the connection terminates on loopback at
your end too.

### Binding the operator plane externally

When a tunnel does not fit (a proxy in another container that cannot
reach the daemon's loopback, or a genuinely remote client population),
the direct path is:

```bash
WILD_TLS_CERT=/etc/wild/tls/cert.pem   # PEM chain
WILD_TLS_KEY=/etc/wild/tls/key.pem     # PEM private key
WILD_REST_ALLOW_EXTERNAL=1             # REST door opt-in
WILD_REST_BIND=0.0.0.0:7532
# MCP door: WILD_MCP_ALLOW_EXTERNAL=1 + WILD_MCP_BIND, same identity
```

The listener serves TLS (`https://…`); loopback binds stay plaintext, so
local clients are unaffected. A garbage certificate or key fails the arm
at boot with the file named — the door never comes up half-secured. Both
halves are required on purpose: the opt-in states intent, the identity is
the property, and neither substitutes for the other. `WILD_MCP_ALLOW_EXTERNAL=1`
alone — the pre-amendment behaviour — no longer lifts the refusal.

This door is ordinary HTTP(S) — "TCP" in older prose only ever
distinguished the network port from the Unix socket, never a protocol of
its own. Everything a web upstream can do applies: an nginx/Caddy/Traefik
in front is the *expected* public setup (the proxy holds the
Let's-Encrypt certificate, rate-limits, and forwards), in two flavors:

- **Same host:** point the proxy at `http://127.0.0.1:7532` — the daemon
  keeps a loopback bind, needs neither opt-in nor certificate, and the
  plaintext hop never leaves the machine.
- **Different container / netns** (the proxy cannot reach the daemon's
  loopback): give the daemon the identity above and point the proxy's
  upstream at it over TLS — a self-signed certificate suffices for that
  hop, since the proxy pins it or runs on a private segment.

Certificate lifecycle is deliberately yours today: static PEM paths,
rotation by daemon restart, no ACME.

Prefer a `wild user`-minted token (scoped to a tribe, rotatable,
revocable) over the root `system/token` for anything remote.

### The recipe

**1 — On the server, open the loopback HTTP door.** Fresh profiles ship
`rest_port: null` (no network listener at all; the operator plane travels over
`<profile>/system/api.sock`), so a tunnel has nothing to forward until
you opt one back in:

```yaml
# <profile_root>/profile.yaml
rest_port: 7532
```

or, without editing the profile, `WILD_REST_BIND=127.0.0.1:7532`.
Keep it on `127.0.0.1` — the tunnel is what crosses the network.
Restart the daemon, then confirm the door is up *on the server*:

```console
$ curl -sS -H "Authorization: Bearer $(cat <profile_root>/system/token)" \
    http://127.0.0.1:7532/healthz
```

**2 — From the client, forward the port.**

```console
$ ssh -N -L 7532:127.0.0.1:7532 user@server
```

**3 — Point the client at the local end.**

```console
$ export WILD_API_BASE=http://127.0.0.1:7532
$ export WILD_TOKEN=<contents of the SERVER's system/token>
```

`WILD_API_BASE` is what the desktop dashboard dials instead of its
socket auto-discovery; `WILD_TOKEN` is the bearer both the daemon and
the CLI resolve before falling back to the profile's token file.

### The token is a root-equivalent secret

`<profile>/system/token` on the server is mode `0600` and grants the
whole operator plane. Copy it over a channel you trust, keep it out of
shell history and dotfiles you sync, and treat a leak as you would a
root password. Better: don't copy it at all — `wild user add <id>
--tribe <tribe>` mints a hashed, rotatable, revocable token whose tribe
scope is enforced on the operator plane whether or not RBAC is on
(within its tribe it still writes as freely as it reads; a genuinely
read-only credential is the RBAC rollout's business).

### Two honest caveats

**The dashboard will think the daemon is local.**
`Transport::is_local()` judges by the *shape* of the endpoint: a Unix
socket or a loopback base counts as local, and through a tunnel the
base **is** loopback. So the client believes the daemon runs on this
machine. The visible consequence is the chat drop-zone — it offers to
attach a local file path that the remote daemon cannot read. Upload the
file through the dashboard instead of dropping a path.

**Only the daemon-only verbs run without a local profile, and only on
`WILD_REST_BIND`.** Each verb declares what it needs from the machine it
runs on (`cli::profile_need`); a verb that reaches nothing but the
daemon — `wild connector status`, `wild flows feed`, `wild content
rerun` and their siblings — runs on a client with no profile of its
own, provided `WILD_REST_BIND` names the local end of the tunnel.
Everything that reads or writes profile files still exits 64 ("no
active profile"): `wild traces ls` and `wild data …` are local verbs
no tunnel can serve.

**Ask the CLI, not this page, which is which.** A verb that needs a
profile says so and exits 64; a verb that would have run against a
remote daemon adds a line naming `WILD_REST_BIND` and `WILD_TOKEN`.
That hint is derived from the declaration the verb carries, so it is
right on the day a verb is added — which a list in a document is not.
Two things to know:

- **`WILD_API_BASE` is not enough.** The dashboard dials it; the CLI
  does not. Export `WILD_REST_BIND` as well (same value, minus the
  scheme) or the CLI never sees a door and exits 64.
- **Pass `--tribe` explicitly.** The default is the attach state in
  `<profile>/state/active-tribe`, and a machine with no profile has
  never run `wild attach` — so an unqualified call means `root`.

## Troubleshooting

### "another `wild` daemon is already running"

The IPC control socket is single-binder. If `wild up` and
`wild-hostd` (via launchd/systemd) try to coexist, the second one
bails. Pick one ownership model:

- **Service-managed daemon** (recommended for long-running setups):
  install via the helper, then drive everything through `wild`
  client commands. Don't run `wild up` separately.
- **Interactive `wild up`** (laptops, demos): uninstall the service
  first (`--uninstall` on the helper), then `wild up` as before.

The new "daemon already running" probe (ADR-0035 §3.3.2.B) makes
this collision visible in <1 s instead of letting the second boot
race for resources.

### Service won't start

```sh
# macOS — recent stderr
tail -n 50 ~/Library/Logs/wild-hostd.launchd.err.log

# Linux — recent journal
journalctl --user -u wild-hostd -n 50 --no-pager
```

Common causes: stale `runtime.pid` lockfile (lockfile probe should
clean it; if not, delete it manually), pre-flight tool drift
(`wild doctor`), NATS port already bound by another process.

### Pinning a specific profile

The daemon resolves the active profile from `<wild_root>/active-profile`
unless overridden. To pin a specific profile in the service manager:

- macOS: edit the LaunchAgent plist, add `WILD_PROFILE=<name>` to
  `EnvironmentVariables`, reload via the install helper.
- Linux: drop a `~/.config/systemd/user/wild-hostd.service.d/profile.conf`
  with `[Service]\nEnvironment=WILD_PROFILE=<name>\n`, then
  `systemctl --user daemon-reload && systemctl --user restart
  wild-hostd`.

### Wild.app owns the pointer (macOS)

The packaged app runs against its own profile `wild` (ADR-0120 D13) and
sets `WILD_PROFILE` on the daemon it launches. That env reaches the
daemon and nothing else — a Terminal opened afterwards resolves
`--profile` → `WILD_PROFILE` → `<wild_root>/active-profile` and would find
none of them. So the app **writes the pointer at every launch**: after
Wild.app has run once, a bare `wild connector status` in a Terminal
talks to the app's daemon.

Two consequences worth knowing:

- A `wild up` in a Terminal that coins its own profile moves the pointer;
  the next Wild.app launch moves it back. Pass `--profile <name>` (or
  export `WILD_PROFILE`) for work that must stay on its own profile —
  both outrank the pointer.
- When nothing resolves, the CLI no longer advises `wild up` blindly. It
  reads each profile's `system/runtime.json`, checks the PID, and names
  the daemons that are actually running:

  ```text
  wild: no active profile —
    a daemon IS running:
        wild  (pid 62494, up since 2026-08-12T02:00:11Z)
    → for one command:      wild --profile wild <command>
    → or make it the default: wild profile load wild
  ```

  The old advice created a second profile and a second daemon beside the
  running one, which is how an install accumulated profiles nobody chose.

## What shipped

- **§3.3.4.B** — the release pipeline ships both binaries side-by-side
  in the GitHub Release tarball + the OCI workflow.
- **§3.3.4.C** — the Brew tap formula installs both binaries and (when
  `--with-launchd` is passed) the LaunchAgent automatically.
- **§3.3.2.C default flip** — `wild up`'s default is now
  spawn-and-attach against the daemon; `--in-process` keeps the legacy
  single-binary mode (mainly for tests).

The brew tap (`docs/homebrew-tap-setup.md`) is the operator-facing
install path; the from-source `cargo install` helpers above stay the
developer path.
