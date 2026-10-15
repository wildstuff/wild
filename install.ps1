# Windows installer for Wild.
#
# Usage:
#
#   powershell -c "irm https://wildstuff.com/install.ps1 | iex"
#
# That is a 302 to this file's home in the public distribution repo,
# https://raw.githubusercontent.com/wildstuff/wild/main/install.ps1, which works
# just as well if a proxy or an air-gapped network makes the short URL
# unreachable. Both this file and that redirect target arrive in the public repo
# by the same `public-sync` run, so the short URL starts resolving at the moment
# there is a copy of this script to reach it from.
#
# Downloads the Windows package matching the latest release, verifies its
# sha256, and hands it to the installer INSIDE the package.
#
# Knobs (environment variables -- `irm | iex` passes no arguments, so env vars
# are the whole interface, exactly as they are for `install.sh`):
#
#   WILD_VERSION        - pin to a specific tag (e.g. v0.1.2). Default: the latest
#                         STABLE release. Pre-releases are deliberately not
#                         installed by default (ADR-0225 D7) -- naming an
#                         `-rc.N` tag here is how a tester opts in.
#   WILD_RELEASE_BASE   - where the release assets live. Default: the GitHub
#                         Releases download base. Requires WILD_VERSION, because
#                         an alternate source has no `releases/latest` API to ask.
#   WILD_NO_MODIFY_PATH - set to 1 to skip the PATH-hint output.
#
# Deliberately NOT offered: an install-location override. Where the package
# lands is decided by `Install-Wild.ps1` inside it, and a second spelling here
# would be a fact stored twice -- see "What this script does not own" below.
#
# Requires: PowerShell 5.1 or later (ships with Windows 10/11). No other tools:
# `Expand-Archive` and `Get-FileHash` are built in.
#
# -- The package is NOT SIGNED ------------------------------------------------
#
# There is no Authenticode certificate yet, so Windows SmartScreen warns the
# first time the app is run. Said here because a surprise warning during an
# install reads as "this is malware", and the honest state is "this is
# unsigned". The sha256 check below is what actually protects the bytes in
# transit; a signature would additionally prove who built them.
#
# -- ADR-0225 D2 -------------------------------------------------------------
#
# Releases live in the PUBLIC distribution repo, not in the development repo
# this file is authored in. GitHub Release assets inherit repo visibility, so
# an anonymous lookup against a private repo 404s. `wildstuff/wild` is
# generated from here by `xtask public-sync`, and this script is synced into
# it, so the reference below is to the repo a user actually reaches.

$ErrorActionPreference = 'Stop'

# Windows PowerShell 5.1 negotiates whatever `SecurityProtocol` says, and on an
# unpatched box that still excludes TLS 1.2 -- against which github.com simply
# closes the connection. The failure surfaces as "The request was aborted:
# Could not create SSL/TLS secure channel", which names neither TLS nor the
# fix. PowerShell 7 defaults to the OS setting and needs none of this, hence
# the guard rather than an unconditional assignment.
if ($PSVersionTable.PSEdition -ne 'Core') {
    [Net.ServicePointManager]::SecurityProtocol =
        [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}

# `Invoke-WebRequest` renders a progress bar by repainting the host on every
# chunk, which on 5.1 dominates the transfer: a ~100 MB download that takes
# seconds with this off takes minutes with it on. Restored in `finally`, so a
# user who dot-sources this script keeps their own preference.
$previousProgress = $ProgressPreference
$ProgressPreference = 'SilentlyContinue'

$GithubRepo = 'wildstuff/wild'
$GithubApi = "https://api.github.com/repos/$GithubRepo"

function Fail($message) {
    Write-Host "error: $message" -ForegroundColor Red
    exit 1
}

function Note($message) {
    Write-Host $message
}

# `<base>/<tag>/<asset>` -- the same layout `install.sh` fetches from, so a
# local directory of release assets serves both installers unchanged.
$releaseBase = if ($env:WILD_RELEASE_BASE) {
    $env:WILD_RELEASE_BASE.TrimEnd('/')
} else {
    "https://github.com/$GithubRepo/releases/download"
}

# -- The host -----------------------------------------------------------------

# Only x86_64 is built. An arm64 Windows box runs x64 binaries under emulation
# and this package very likely works there -- but "very likely" is not a
# measurement, and nothing in CI runs on arm64 Windows. So it proceeds and says
# what it does not know, rather than refusing a machine that probably works or
# promising one that might not.
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq 'ARM64') {
    Write-Warning ("this is an arm64 Windows host and no arm64 build exists; " +
        "installing the x86_64 package, which Windows runs under emulation. " +
        "That combination is not tested.")
} elseif ($arch -ne 'AMD64') {
    Fail ("unsupported processor architecture '$arch' -- Wild publishes an " +
        "x86_64 Windows package only.")
}

# -- Which version ------------------------------------------------------------

$tag = $env:WILD_VERSION
if (-not $tag) {
    if ($env:WILD_RELEASE_BASE) {
        Fail ("WILD_RELEASE_BASE is set but WILD_VERSION is not. An alternate " +
            "release source has no 'latest' to resolve -- name the version " +
            "explicitly, e.g. WILD_VERSION=v0.1.2.")
    }
    Note "Resolving the latest stable release..."
    # `releases/latest` returns the newest non-draft AND NON-PRERELEASE
    # release. The second word is load-bearing: ADR-0225 D7 gets two channels
    # out of one public repo from it, because a `-rc.N` published here is
    # invisible to everyone who runs this script and opt-in for a tester via
    # WILD_VERSION. Anything that "fixes" the failure below by falling back to
    # the newest release of any kind deletes that separation and ships release
    # candidates to ordinary installs.
    try {
        $latest = Invoke-RestMethod -Uri "$GithubApi/releases/latest" -UseBasicParsing
    } catch {
        # Captured before the inner call below can overwrite $_.
        $why = $_.Exception.Message

        # A 404 here does NOT mean "no releases" -- it means no STABLE one, and
        # the operator's useful next step is the opt-in rather than a raw HTTP
        # message. Ask what IS published and name it; stay with the original
        # error if that call fails too, since then the problem is reaching
        # GitHub at all.
        $newest = $null
        try {
            $newest = @(Invoke-RestMethod -Uri "$GithubApi/releases?per_page=1" `
                -UseBasicParsing)[0].tag_name
        } catch { }

        if ($newest) {
            Fail ("no STABLE release has been published for $GithubRepo yet. " +
                "Pre-releases are skipped on purpose, so a release candidate " +
                "never lands on an ordinary install. The newest is $newest -- " +
                "install it by setting WILD_VERSION=$newest and running this " +
                "again, or wait for the first stable cut.")
        }
        Fail ("could not ask GitHub for the latest release: $why")
    }
    $tag = $latest.tag_name
    if (-not $tag) { Fail "GitHub returned no tag_name for the latest release." }
}

# The asset carries the cargo version, the tag carries a leading `v`
# (`v0.5.0` -> `Wild-0.5.0-x86_64.zip`). Constructed rather than looked up so
# that `WILD_RELEASE_BASE` -- a plain directory with no API to query -- works
# the same way. The name is therefore spelled BOTH here and in
# `.github/workflows/windows-app.yml`, which is a fact stored twice; the smoke
# step in that same workflow installs with this script, so a drift fails the
# build that introduced it rather than the first user who runs it.
$version = $tag -replace '^v', ''
$asset = "Wild-$version-x86_64.zip"
$assetUrl = "$releaseBase/$tag/$asset"

Note "Installing Wild $tag"

# -- Fetch --------------------------------------------------------------------

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("wild-install-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $work | Out-Null

try {
    $zip = Join-Path $work $asset
    $sum = "$zip.sha256"

    # A `file://` or plain-path base is how the release lane proves this script
    # before publishing anything. `Invoke-WebRequest` handles neither reliably
    # across 5.1 and 7, so the copy is explicit instead of assumed.
    function Fetch($url, $destination) {
        if ($url -match '^https?://') {
            Invoke-WebRequest -Uri $url -OutFile $destination -UseBasicParsing
        } else {
            $local = $url -replace '^file:///?', ''
            if (-not (Test-Path -LiteralPath $local)) {
                Fail "no such file: $local"
            }
            Copy-Item -LiteralPath $local -Destination $destination -Force
        }
    }

    Note "Downloading $asset"
    try {
        Fetch $assetUrl $zip
    } catch {
        Fail ("could not download $assetUrl - $($_.Exception.Message)`n" +
            "  If the release exists, check that its Windows package was attached; " +
            "not every release carries one.")
    }

    try {
        Fetch "$assetUrl.sha256" $sum
    } catch {
        Fail ("could not download the checksum $assetUrl.sha256 - $($_.Exception.Message)`n" +
            "  Refusing to install an unverified package.")
    }

    # -- Verify ---------------------------------------------------------------
    #
    # Fail-closed, and deliberately NOT relaxed when WILD_RELEASE_BASE points
    # somewhere else: the check costs nothing and an alternate source is
    # exactly the case where a wrong file is easiest to serve. `install.sh`
    # makes the same call for the same reason.
    #
    # The sidecar is `shasum -a 256` format (`<hash>  <name>`), which is what
    # the release lane writes for every other asset.
    $expected = ((Get-Content -LiteralPath $sum -Raw) -split '\s+')[0].Trim().ToLower()
    if ($expected -notmatch '^[0-9a-f]{64}$') {
        Fail "the checksum file did not contain a sha256 digest: $expected"
    }
    $actual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLower()
    if ($actual -ne $expected) {
        Fail ("sha256 mismatch for $asset`n" +
            "  expected $expected`n" +
            "  actual   $actual`n" +
            "  The download is corrupt or has been tampered with. Nothing was installed.")
    }
    Note "sha256 verified"

    # -- Install --------------------------------------------------------------
    #
    # ## What this script does not own
    #
    # Where the package lands, what the layout is, and that `wild://` gets
    # registered are all decided by `Install-Wild.ps1`, which
    # `xtask package-windows` writes INTO the package. Re-deciding any of it
    # here would be the same fact in two files with nothing holding them
    # together -- the shape that shipped #5326 and #5706. So this script fetches
    # and verifies, and the package installs itself.
    $unpacked = Join-Path $work 'unpacked'
    Expand-Archive -LiteralPath $zip -DestinationPath $unpacked -Force

    $installer = Get-ChildItem -Path $unpacked -Filter 'Install-Wild.ps1' -Recurse -File |
        Select-Object -First 1
    if (-not $installer) {
        Fail ('the package contains no Install-Wild.ps1 - it was built by a ' +
            'version of `xtask package-windows` this installer does not know ' +
            'how to install, or the archive is not a Wild package.')
    }

    & $installer.FullName

    # A hint, not a mutation -- the same choice `install.sh` makes, and the
    # reason its knob is called NO_MODIFY_PATH rather than NO_SET_PATH.
    #
    # The command below reads and writes the USER PATH specifically. The
    # obvious `setx PATH "$env:PATH;..."` is wrong twice over: `$env:PATH` is
    # the machine and user PATHs already merged, so it copies the machine's
    # entries into the user's, and `setx` silently truncates at 1024
    # characters -- together they can shorten a PATH that took years to grow.
    if ($env:WILD_NO_MODIFY_PATH -ne '1') {
        $installed = Join-Path $env:LOCALAPPDATA 'Programs\Wild'
        Note ""
        Note "To use the CLI from any shell, add this to your PATH:"
        Note "    $installed"
        Note ""
        Note "  [Environment]::SetEnvironmentVariable('PATH',"
        Note "    [Environment]::GetEnvironmentVariable('PATH','User') + ';$installed', 'User')"
        Note ""
        Note "(Set WILD_NO_MODIFY_PATH=1 to silence this hint.)"
    }
} finally {
    $ProgressPreference = $previousProgress
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
