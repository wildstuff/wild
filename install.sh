#!/bin/sh
# Cross-platform installer for `wild`.
#
# Usage:
#
#   curl -fsSL https://wildstuff.com/install | sh
#
# That is a 302 to this file's home in the public distribution repo,
# https://raw.githubusercontent.com/wildstuff/wild/main/install.sh, which works
# just as well if a proxy or an air-gapped network makes the short URL
# unreachable. Verified 2026-08-16: 302 -> 200, `text/plain; charset=utf-8`.
#
# Pulls the prebuilt host binary from the GitHub Release matching
# the user's OS + arch, verifies its sha256, and installs the program tree
# to $WILD_INSTALL_DIR (default: $HOME/.local/lib/wild), then symlinks the
# three binaries into $WILD_BIN_DIR (default: $HOME/.local/bin).
#
# The tree is deliberately NOT under `wild_core::user_dirs::wild_root()`:
# that is where the operator's DATA lives, and `<wild_root>/bin/` is the
# managed slot the first boot seeds with third-party sidecars
# (`llama-server`, `nats-server`). Program and data stay separate, the way
# the macOS bundle (/Applications/Wild.app) and the Windows package
# (%LOCALAPPDATA%\Programs\Wild) already keep them. Until 2026-08-17 this
# script wrote to `$HOME/.wild/bin`, which was the data root's old spelling.
#
# Knobs (env vars):
#
#   WILD_VERSION       — pin to a specific tag (e.g. v0.1.2). Default: the latest
#                        STABLE release. Pre-releases are deliberately not
#                        installed by default (ADR-0225 D7) — naming an `-rc.N`
#                        tag here is how a tester opts in.
#   WILD_INSTALL_DIR   — program tree location. Default: $HOME/.local/lib/wild.
#   WILD_BIN_DIR       — where the binaries are symlinked. Default: $HOME/.local/bin.
#   WILD_NO_MODIFY_PATH — set to 1 to skip the PATH-hint output.
#   WILD_RELEASE_BASE  — where the release assets live. Default: the GitHub
#                        Releases download base. Requires WILD_VERSION, because
#                        an alternate source has no `releases/latest` API to ask.
#
# Supported targets (matches .github/workflows/release.yml's build matrix):
#
#   - macOS arm64  → aarch64-apple-darwin
#   - Linux x86_64  → x86_64-unknown-linux-gnu
#   - Linux aarch64 → aarch64-unknown-linux-gnu
#
# Two hosts are refused UP FRONT rather than handed a tarball that cannot
# work, because after the download the message stops being ours: a musl
# distribution (Alpine), where glibc binaries die with a bare `not found`,
# and an Intel Mac, for which nothing has been published since
# x86_64-apple-darwin left the release matrix on 2026-05-05. Both refusals
# name the way out.
#
# Windows has an installer of its own — `install.ps1`, beside this file:
#
#   powershell -c "irm https://wildstuff.com/install.ps1 | iex"
#
# It fetches a different artefact (the packaged program directory built by
# `xtask package-windows`, not a tarball of host binaries), which is why it is
# a separate script rather than a branch in this one.
#
# Requires: curl OR wget, tar, and one of `shasum` / `sha256sum`.

# ADR-0225 D2 — releases live in the PUBLIC distribution repo, not in the
# development repo this file is authored in. GitHub Release assets inherit
# repo visibility, so an anonymous `releases/latest` against a private repo
# 404s; that is why the installer this repo shipped never worked. `wildstuff/wild`
# is generated from here by `xtask public-sync`, and this script is synced
# into it, so the reference below is to the repo a user actually reaches.
#
# Note that GHCR packages are NOT affected by that rule: their namespace is
# org-scoped with a per-package visibility switch, so no `oci://ghcr.io/wildstuff/…`
# reference anywhere changes.

set -eu

GITHUB_REPO="wildstuff/wild"
GITHUB_API="https://api.github.com/repos/${GITHUB_REPO}"

# Where the `<tag>/<asset>` pair is fetched from. Overridable because the
# default source is the one place this script CANNOT be tested against: a
# release must be published before it can be downloaded, so every assertion
# about "does the tarball install" would arrive after the answer stopped
# being useful. `scripts/ci/smoke-install-linux.sh` points this at the
# freshly built tarball and installs it on a slim base image BEFORE the
# release goes out.
#
# It is an operator surface too, and the same one: an air-gapped server is
# a host that cannot reach github.com either. Download the tarball and its
# `.sha256` sidecar on a machine that can, copy both to the target under
# `<base>/<tag>/`, and install with WILD_RELEASE_BASE + WILD_VERSION set.
# The layout is deliberately the same as the real one so the path this
# exercises is the path operators get — a test that rewrote the URL shape
# would stop testing the shape.
#
# The sha256 verification below is NOT relaxed for an override: a local
# mirror can be stale or truncated just as a download can, and the sidecar
# rides along either way.
RELEASE_BASE="${WILD_RELEASE_BASE:-https://github.com/${GITHUB_REPO}/releases/download}"

# ── helpers ──────────────────────────────────────────────────────────

err() {
    printf 'install: %s\n' "$*" >&2
    exit 1
}

info() {
    printf '%s\n' "$*"
}

# Pick curl or wget. We need one to fetch the API + the tarball.
have() { command -v "$1" >/dev/null 2>&1; }

if have curl; then
    fetch() { curl -fsSL "$1"; }
    fetch_to() { curl -fsSL -o "$2" "$1"; }
elif have wget; then
    fetch() { wget -qO- "$1"; }
    fetch_to() { wget -qO "$2" "$1"; }
else
    err "neither curl nor wget found; install one and re-run"
fi

# Pick the right sha256 tool. macOS ships shasum; Linux usually
# sha256sum. BusyBox's sha256sum also works.
if have sha256sum; then
    sha_check() { sha256sum -c "$1" >/dev/null; }
elif have shasum; then
    sha_check() { shasum -a 256 -c "$1" >/dev/null; }
else
    err "neither sha256sum nor shasum found; install coreutils or use a system with shasum"
fi

# ── OS / arch detection ──────────────────────────────────────────────

# True on a musl-based Linux (Alpine and friends). Two independent
# signals because either one alone can be absent: the loader's file name
# is distinctive, and musl's own `ldd` self-identifies in its first line.
# A glibc box matches neither (`ldd (Ubuntu GLIBC 2.39…)`).
is_musl() {
    for loader in /lib/ld-musl-*.so.1; do
        [ -e "$loader" ] && return 0
    done
    ldd --version 2>&1 | head -n 1 | grep -qi musl
}

# True when this shell is an x86_64 process TRANSLATED by Rosetta 2 on an
# Apple-Silicon Mac. `uname -m` says `x86_64` there, which is the truth about
# the PROCESS and a lie about the machine — and the Intel refusal below would
# otherwise turn away an M-series Mac that can run the arm64 build perfectly,
# for the crime of being reached through a translated terminal.
is_rosetta() {
    [ "$(sysctl -n sysctl.proc_translated 2>/dev/null)" = "1" ]
}

detect_target() {
    os="$(uname -s)"
    arch="$(uname -m)"
    case "$os" in
        Darwin)
            case "$arch" in
                arm64|aarch64) target="aarch64-apple-darwin" ;;
                # A translated shell on Apple Silicon is an arm64 Mac, so it
                # gets the arm64 build — checked BEFORE the Intel refusal,
                # because the refusal would otherwise be right about the
                # process and wrong about the machine.
                x86_64) if is_rosetta; then
                            target="aarch64-apple-darwin"
                        else
                            # Intel macOS left the release matrix on 2026-05-05:
                            # Apple stopped selling Intel Macs in late 2023 and
                            # a macOS runner costs 10x per minute. The tarball
                            # this script would ask for has not been built since,
                            # so without this the install dies on a 404 from a
                            # URL the operator never typed — a message about a
                            # missing file, for a platform decision. Refuse
                            # where the reason is still known.
                            err "this is an Intel Mac, and the published builds are Apple-Silicon only — x86_64-apple-darwin left the release matrix on 2026-05-05. Build from source instead: 'git clone https://github.com/${GITHUB_REPO} && cargo install --path crates/runtime/frontend && cargo install --path crates/runtime/daemon', or 'brew install --HEAD wildstuff/tap/wild'. Nothing was installed."
                        fi
                        ;;
                *) err "unsupported macOS arch: $arch" ;;
            esac
            ;;
        Linux)
            # The published Linux builds are glibc-only — the release
            # matrix targets `*-unknown-linux-gnu`, and so does the Pdfium
            # library that ships beside them. On a musl box the tarball
            # verifies, extracts and installs perfectly, and every binary
            # then dies with a bare `not found`: the loader's notoriously
            # opaque way of saying "there is no ld-linux here". Refuse
            # before downloading rather than after — this is the LAST layer
            # that can still explain itself, because once the binary is on
            # disk the message belongs to the kernel, and no amount of
            # error handling inside a process that never starts can help.
            if is_musl; then
                err "this is a musl-based Linux (Alpine or similar) and the published builds are glibc-only. They would install cleanly here and then fail to start with a bare 'not found'. Run wild on a glibc distribution — Debian, Ubuntu, Fedora, or a 'debian:stable-slim' container base — or build from source against musl yourself. Nothing was installed."
            fi
            case "$arch" in
                x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
                aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
                *) err "unsupported Linux arch: $arch" ;;
            esac
            ;;
        MINGW*|MSYS*|CYGWIN*|Windows_NT)
            err "Windows isn't supported. Use Windows Subsystem for Linux (WSL) and re-run this installer from inside the WSL shell, or build from source: 'git clone https://github.com/${GITHUB_REPO} && cargo build -p wild-frontend -p wild-daemon --release'."
            ;;
        *)
            err "unsupported OS: $os"
            ;;
    esac
    printf '%s' "$target"
}

# ── version resolution ───────────────────────────────────────────────

resolve_version() {
    if [ -n "${WILD_VERSION-}" ]; then
        # Accept both `0.1.2` and `v0.1.2`; normalise to the leading-v
        # tag form the release pipeline produces.
        case "$WILD_VERSION" in
            v*) printf '%s' "$WILD_VERSION" ;;
            *)  printf 'v%s' "$WILD_VERSION" ;;
        esac
        return
    fi
    # An alternate source has no `releases/latest` endpoint to ask, and the
    # API below answers for the PUBLIC repo — so without this the fallback
    # would resolve github.com's newest tag and then look for it inside a
    # local mirror, failing on the asset URL with a message about a version
    # the operator never named.
    if [ -n "${WILD_RELEASE_BASE-}" ]; then
        err "WILD_RELEASE_BASE is set but WILD_VERSION is not. An alternate release source has no 'latest' to resolve — name the version explicitly, e.g. WILD_VERSION=v0.1.2."
    fi
    # GitHub's `releases/latest` returns the newest non-draft **and
    # NON-PRERELEASE** release. That second word is load-bearing, not a
    # detail: ADR-0225 D7 gets two channels out of one public repo from it,
    # because a `-rc.N` published here is invisible to everyone who runs this
    # script and opt-in for a tester via WILD_VERSION. Anything that "fixes" a
    # 404 below by falling back to the newest release of any kind deletes that
    # separation and ships release candidates to ordinary installs.
    #
    # Parse `tag_name` with sed — a JSON parser would need jq, which isn't
    # installed everywhere.
    if ! body="$(fetch "${GITHUB_API}/releases/latest" 2>/dev/null)"; then
        # A 404 here does NOT mean "no releases" — it means no STABLE one. The
        # comment above said "no releases yet" and the message below acted on
        # it, so an operator installing while six pre-releases were published
        # was told the project had none and pointed at a source build. Ask what
        # IS published, and name it: the useful next step is the opt-in, not a
        # toolchain.
        newest="$(fetch "${GITHUB_API}/releases?per_page=1" 2>/dev/null \
            | grep '"tag_name":' \
            | head -n 1 \
            | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
        if [ -n "$newest" ]; then
            err "no STABLE release has been published for ${GITHUB_REPO} yet. Pre-releases are skipped on purpose, so a release candidate never lands on an ordinary install. The newest is ${newest} — install it with 'WILD_VERSION=${newest}', or wait for the first stable cut."
        fi
        # Reached only when BOTH calls failed, so "there are no releases" is
        # one explanation among several — an unreachable API and anonymous
        # rate-limiting (60/hour per address) land here identically. Say what
        # happened rather than pick a cause, which is what sent an operator to
        # a source build for a problem a minute's wait would have cleared.
        err "could not resolve the latest stable release for ${GITHUB_REPO} — GitHub answered neither with one nor with a list of what is published. It may be unreachable from here, or rate-limiting this address. Pin a specific tag with WILD_VERSION=v0.1.2, or build from source: 'git clone https://github.com/${GITHUB_REPO} && cargo build -p wild-frontend -p wild-daemon --release'."
    fi
    tag="$(printf '%s' "$body" \
        | grep '"tag_name":' \
        | head -n 1 \
        | sed -E 's/.*"tag_name"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/')"
    [ -n "$tag" ] || err "couldn't parse tag_name from latest release JSON (got: $(printf '%s' "$body" | head -c 200))"
    printf '%s' "$tag"
}

# ── main ─────────────────────────────────────────────────────────────

main() {
    target="$(detect_target)"
    tag="$(resolve_version)"
    version="${tag#v}"

    install_dir="${WILD_INSTALL_DIR:-$HOME/.local/lib/wild}"
    bin_dir="${WILD_BIN_DIR:-$HOME/.local/bin}"
    asset="wild-${version}-${target}.tar.gz"
    asset_url="${RELEASE_BASE}/${tag}/${asset}"
    sha_url="${asset_url}.sha256"

    info "Installing wild ${tag} for ${target}"
    info "  source:  ${asset_url}"
    info "  program: ${install_dir}"
    info "  on PATH: ${bin_dir}"

    # Stage in a tmp dir we own, then atomically move the binary
    # into place. Trap cleans up on any exit path.
    tmpdir="$(mktemp -d 2>/dev/null || mktemp -d -t wild-install)"
    trap 'rm -rf "$tmpdir"' EXIT INT HUP TERM

    info ""
    info "→ downloading tarball + sha256 sidecar"
    fetch_to "$asset_url" "$tmpdir/$asset" \
        || err "couldn't download $asset_url"
    fetch_to "$sha_url" "$tmpdir/$asset.sha256" \
        || err "couldn't download $sha_url"

    info "→ verifying sha256"
    # The .sha256 sidecar is `<hex>  <filename>` against the bare
    # `wild-<v>-<target>.tar.gz` filename, so cd into the staging
    # dir for the check.
    ( cd "$tmpdir" && sha_check "$asset.sha256" ) \
        || err "sha256 mismatch for $asset — abort. Re-run later, or report at https://github.com/${GITHUB_REPO}/issues."

    info "→ extracting"
    tar -xzf "$tmpdir/$asset" -C "$tmpdir"
    unpacked="$tmpdir/wild-${version}-${target}"

    # Install ALL the binaries the tarball carries, not just the cli.
    # `wild` is only a frontend: `wild up` spawns `wild-hostd`, which it
    # locates as a sibling / in this same dir / on PATH — so a lone `wild`
    # is an install that cannot start anything. `wild-appd` is the same
    # story one level out: the daemon spawns the end-user portal from a
    # sibling path (ADR-0154 D3).
    #
    # `wild-appd` is tolerated as absent so an older WILD_VERSION whose
    # tarball predates it still installs; `wild-hostd` is not, because
    # without it nothing runs.
    mkdir -p "$install_dir"
    installed=""
    for bin in wild wild-hostd wild-appd; do
        src="$unpacked/$bin"
        if [ ! -f "$src" ]; then
            case "$bin" in
                wild|wild-hostd)
                    err "$bin not found inside the tarball at $src — the release asset looks incomplete; report at https://github.com/${GITHUB_REPO}/issues."
                    ;;
                *)
                    info "  · $bin not in this release — skipping (the end-user portal needs it)"
                    continue
                    ;;
            esac
        fi
        # mv across filesystems can fall back to cp+rm — both are fine.
        mv -f "$src" "$install_dir/$bin"
        chmod +x "$install_dir/$bin"
        installed="$installed $bin"
    done

    # Symlink rather than copy: the daemon resolves `wild-appd` as a SIBLING
    # of the running executable (ADR-0154 D3), and a symlink keeps one real
    # tree for that probe to be a sibling within. `ln -sf` on a path that is
    # currently a directory would nest the link inside it, so a stale entry
    # is removed by hand first.
    mkdir -p "$bin_dir"
    for bin in wild wild-hostd wild-appd; do
        [ -f "$install_dir/$bin" ] || continue
        rm -rf "$bin_dir/$bin"
        ln -s "$install_dir/$bin" "$bin_dir/$bin"
    done

    # ── provenance marker (ADR-0210 D16) ────────────────────────────
    #
    # An install records HOW it was made, so `wild upgrade` never has to
    # guess from paths. The rule is one-sided on purpose: a MISSING marker
    # means "not an install this script made" and the verb defers rather
    # than touching a tree it does not own — a Homebrew prefix, a distro
    # package, a developer's `cargo build` output.
    #
    # It sits inside the program tree, not the data root: it describes the
    # BINARIES, it must travel with them, and it must be replaced when they
    # are. Re-running this installer overwrites it, which is what keeps
    # `version` honest after an upgrade.
    #
    # Written last, after every binary is in place: a marker that appears
    # before the tree is complete would claim an install that does not exist
    # yet if the script dies in between.
    cat > "$install_dir/.wild-install.json" <<MARKER
{
  "kind": "tarball",
  "installed_by": "install.sh",
  "version": "${version}",
  "install_dir": "${install_dir}",
  "bin_dir": "${bin_dir}"
}
MARKER


    # The init scripts and the operator-daemon guide ride in the tarball so
    # `bash dist/install/install-systemd.sh` works straight from an unpacked
    # download — release.yml stages them for exactly that. A `curl | sh`
    # install never saw that tree: the staging tmpdir is deleted on exit, so
    # the one documented path to a daemon that survives logout and reboot
    # disappeared the moment the install finished, on the platform where a
    # service-managed daemon is the normal deployment.
    #
    # They land INSIDE `$install_dir` for the same reason the Pdfium library
    # does: that directory is the whole mandate the operator gave us, and it
    # keeps `rm -r` a complete uninstall. The two-level `dist/install` +
    # `dist/{systemd,launchd}` shape is preserved verbatim, because the
    # installer script resolves its unit template relative to its own
    # location — flattening it would break the script this exists to reach.
    for extra in dist docs; do
        if [ -d "$unpacked/$extra" ]; then
            rm -rf "$install_dir/$extra"
            cp -R "$unpacked/$extra" "$install_dir/$extra"
            installed="$installed $extra/"
        fi
    done

    info ""
    info "✓ wild ${tag} installed → ${install_dir} (${installed# })"
    info "  linked into ${bin_dir}"

    # A missing shared library is the one failure `wild doctor` structurally
    # CANNOT report: doctor is a `wild` subcommand, so diagnosing "the
    # binary will not start" would require starting the binary. That makes
    # this the only layer that can catch it, the same argument as the musl
    # refusal above — only softer, because installing the package after the
    # fact fixes it.
    #
    # Ask the binary rather than carrying a list of library names: a
    # hand-written list would drift the moment a dependency changes, and
    # `ldd` already knows the answer for whatever this build actually
    # links. Warn rather than fail — the install itself is fine, and the
    # operator may well be provisioning the host in the next step.
    if have ldd; then
        missing="$(ldd "$install_dir/wild-hostd" 2>/dev/null | awk '/not found/ { print "    · " $1 }')"
        if [ -n "$missing" ]; then
            info ""
            info "⚠ wild-hostd is installed but cannot start on this host yet —"
            info "  these shared libraries are missing:"
            info "$missing"
            info ""
            info "  A slim server or container image typically needs:"
            info "    Debian/Ubuntu:  sudo apt-get install -y libdbus-1-3 libgomp1"
            info "    Fedora/RHEL:    sudo dnf install -y dbus-libs libgomp"
            info "  (libgomp is only needed for the local embed/rerank models.)"
        fi
    fi

    # PATH hint — about the SYMLINK dir, which is what the operator runs
    # from. `~/.local/bin` is already on PATH on most current systems, so
    # this usually prints nothing at all.
    if [ "${WILD_NO_MODIFY_PATH:-0}" != "1" ]; then
        case ":$PATH:" in
            *":$bin_dir:"*) ;;
            *)
                info ""
                info "Add ${bin_dir} to your PATH:"
                info ""
                info "    echo 'export PATH=\"${bin_dir}:\$PATH\"' >> ~/.bashrc"
                info "    echo 'export PATH=\"${bin_dir}:\$PATH\"' >> ~/.zshrc"
                info ""
                ;;
        esac
    fi

    # A pre-2026-08-17 install put the binaries in `$HOME/.wild/bin`. Left
    # alone that copy stays on PATH and may WIN over the one just installed,
    # so the operator would keep running the old build and see none of this.
    # Named, never deleted: the directory is the operator's, this script has
    # no mandate to remove what an earlier version of it created, and a
    # sibling `wild-appd` there is still a working install until they say so.
    if [ -e "$HOME/.wild/bin/wild" ] && [ "$install_dir" != "$HOME/.wild/bin" ]; then
        info ""
        info "! An older install is still at ~/.wild/bin — it may shadow this one."
        info "  Check with:  command -v wild"
        info "  Remove with: rm -r ~/.wild/bin"
        info "  (Your data is elsewhere and is not affected.)"
    fi

    info "Optional runtime add-ons (not pulled by this installer):"
    info "  • nats-server  — embedded host can supervise its own;"
    info "                   install separately if you prefer to point"
    info "                   wild at an existing NATS via WILD_NATS_URL."
    info "  • docker       — only if you build components on THIS machine."
    info "                   The default build service is hosted, and needs none."
    info "  • claude CLI   — for the anthropic-cli LLM adapter."
    info ""
    info "Quick start:"
    info "  wild up                    # boot the embedded host"
    info "  wild doctor                # health snapshot"
    # Only offered when the tree actually landed — an older tarball that
    # predates it must not print a path to a script that isn't there.
    if [ -d "$install_dir/dist/install" ]; then
        info ""
        info "Run it as a service (survives logout + reboot):"
        case "$target" in
            *-apple-darwin)
                info "  bash ${install_dir}/dist/install/install-launchd.sh"
                ;;
            *)
                info "  bash ${install_dir}/dist/install/install-systemd.sh"
                ;;
        esac
    fi
    info ""
    info "Docs: https://github.com/${GITHUB_REPO}"
}

main "$@"
