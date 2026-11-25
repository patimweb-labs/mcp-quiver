Deutsch: [release.de.md](release.de.md)

# Releasing quiver

A release is prepared by release-please. Merging the release pull request
creates the tag `vX.Y.Z`, and the **same** workflow builds the client for macOS
(Apple Silicon and Intel), Windows and Linux (x86_64 and aarch64), creates the
GitHub release and attaches every package plus `SHA256SUMS`. The server image is
built by `abbild.yml`: for a tag this workflow created itself, it starts that
workflow explicitly; a tag pushed by hand triggers it directly.

| File | Watches over |
| --- | --- |
| `.github/workflows/release.yml` | release-please, version check, five build jobs, checksums, provenance, release |
| `release-please-config.json` | which files a release bumps (`Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`) |
| `.release-please-manifest.json` | the last released version |
| `verteilung/macos/paket.sh` | `quiver.app`, `.dmg`, signing and notarisation |
| `verteilung/linux/paket.sh` | `.tar.gz`, `.deb`, `.rpm`, AppImage |
| `verteilung/windows/paket.ps1` | `.msi` (per user) and portable `.zip` |
| `verteilung/version.sh` | the version from `Cargo.toml` |
| `verteilung/tools/stand.txt` | the commit of the bundled tools (never `main`) |
| `verteilung/tools/bauen.sh`, `bauen.ps1` | build the tools from quiver-tools, stripped, into `dist/tools` |
| `verteilung/tools/einbauen.sh`, `macos-signieren.sh`, `wix-werkzeuge.ps1` | put them into the package and (macOS) sign each of them |

## Making a release

Releases are prepared by [release-please](https://github.com/googleapis/release-please)
from Conventional Commits. The version lives in one place,
`[workspace.package] version` in `Cargo.toml`; the crates inherit it, and
`release-please-config.json` keeps `Cargo.toml`, `Cargo.lock` and
`CHANGELOG.md` in step.

1. **Merge work into `main`** through a pull request, with commit messages
   release-please understands (`feat:`, `fix:`, `perf:`, …). Every push to
   `main` updates or opens the release pull request `chore: Release <version>`.
2. **Merge the release pull request.** It only raises the version, the lock
   file and the changelog; a pull request opened by `GITHUB_TOKEN` does not
   start other workflows, so run the check locally before merging.
3. **The same push builds and publishes.** release-please creates the tag
   `v<version>` and the release; the rest of `release.yml` builds the five
   packages, writes `SHA256SUMS` and the provenance, appends the package block
   to the release notes and starts `abbild.yml` for the same tag. Tag and build
   are one workflow because a tag created with `GITHUB_TOKEN` does not trigger
   another workflow. `workflow_dispatch` is the one exception, which is how the
   image workflow is started.
4. **Watch the run.** *Actions → Release*. When it is green, the release is at
   *Releases* with all packages, `SHA256SUMS`, the provenance attestations and
   the image tags `X.Y.Z`, `X.Y`, `latest`, `sha-<commit>`.

### Which job runs when

GitHub silently checks `success()` over **all** ancestors in a job that has no
status function. A skipped job would therefore take the following ones with
it — for a tag pushed by hand, a pull request or *Run workflow*, `release_please`
is skipped and `pakete`, `veroeffentlichen` and `abbild` would be skipped too.
That is why every condition spells out
`!cancelled() && needs.<job>.result == 'success' && …`; `!cancelled()` instead
of `always()`, so that a cancelled run does not keep building either.

| Trigger | `release_please` | `version` | `pakete` | `veroeffentlichen` | `abbild` |
| --- | --- | --- | --- | --- | --- |
| Tag `vX.Y.Z` pushed by hand | skipped | checks the tag | builds five packages | creates the release or extends it | skipped — the tag started `abbild.yml` itself |
| Release PR merged (push to `main`) | creates tag and release | checks the tag, `selbst_erzeugt=ja` | builds five packages | attaches packages, checksums and provenance | starts `abbild.yml` for the tag |
| Push to `main` without a new release | only maintains the release PR | no tag, `veroeffentlichen=nein` | skipped | skipped | skipped |
| Pull request | skipped (only runs on `main`) | no tag, `veroeffentlichen=nein` | builds, without secrets | skipped | skipped |
| *Run workflow* | skipped | no tag, `veroeffentlichen=nein` | builds | skipped | skipped |

**A pull request and *Run workflow* never publish.** `version` only sets
`veroeffentlichen=ja` when a tag is involved: a tag push, or a tag that
release-please created in this run. Every other path ends at `nein`, and
`veroeffentlichen` additionally requires `needs.pakete.result == 'success'`.
Nothing is created, changed or uploaded there.

### Tag by hand (fallback)

Without release-please the manual way still works:

1. Raise `[workspace.package] version`, refresh `Cargo.lock`
   (`cargo check --workspace`), run the full check (AGENTS.md §4) and merge
   through a pull request.
2. Tag and push:

   ```bash
   git tag -a v0.2.0 -m "quiver 0.2.0"
   git push origin v0.2.0
   ```

3. The tag push runs `release.yml` (build and release) and `abbild.yml` (image)
   by itself. No release-please involved.

The first job compares the tag with the version from `Cargo.toml`. `v0.2.0`
next to `0.1.0` stops the run before anything is built — the release name and
the version the program reports to the server must not drift apart.

### When something went wrong

* *Run workflow* builds the packages again and leaves them as artifacts of the
  run; it does **not** touch a release. Good for trying a fix.
* A push of the same tag is not possible; delete and re-push only if the
  release is to be replaced.
* One platform failed, the others worked: the run is red and **nothing is
  published** — `veroeffentlichen` only starts when all five build jobs
  succeeded. The packages of the successful legs stay as artifacts of the run.
  Fix the cause and tag again (`v0.2.1`); as long as the tag is not yet out,
  delete release and tag, fix, tag again.
* The image workflow was not started: start it by hand (*Actions → Abbild →
  Run workflow*) on the tag. It checks out the tag and tags the image with its
  version.

## What is built where

| Runner | Architecture | Packages |
| --- | --- | --- |
| `macos-latest` (arm64) | arm64 | `.dmg` with `quiver.app` |
| `macos-15-intel` | x86_64 | `.dmg` with `quiver.app` |
| `windows-2025` | x86_64 | `.msi` and portable `.zip` |
| `ubuntu-22.04` | x86_64 (glibc 2.35) | `.deb`, `.rpm`, AppImage, `.tar.gz` |
| `ubuntu-22.04-arm` | aarch64 | `.deb`, `.rpm`, AppImage, `.tar.gz` |

Everything is built **natively on its runner** (`cargo build --release
--locked`), never cross-compiled: Rust under emulation takes an hour instead of
minutes. The crates come from `rust-toolchain.toml`, so the CI uses the same
compiler as the developers.

`permissions: {}` is the default; only the last job may write
(`contents: write` for the release, `attestations: write` and `id-token: write`
for the provenance). Every `checkout` uses `persist-credentials: false` and
`submodules: recursive`. There is no `pull_request_target`, and no user input
takes the direct path into a `run:` block — values go through `env:`.

## Secrets (all optional)

Without them the macOS bundle is **ad hoc** signed and Windows stays
unsigned — everything still builds and runs. Set as repository secrets:

| Secret | What it is |
| --- | --- |
| `MACOS_CERTIFICATE` | Developer ID Application certificate, `.p12`, base64 encoded |
| `MACOS_CERTIFICATE_PASSWORD` | password of that `.p12` |
| `MACOS_SIGNING_IDENTITY` | e.g. `Developer ID Application: Firma GmbH (TEAMID)` |
| `APPLE_ID` | Apple ID for the notary service |
| `APPLE_TEAM_ID` | team id (ten characters) |
| `APPLE_APP_PASSWORD` | app-specific password for `notarytool` |

```bash
base64 -i DeveloperID.p12 | pbcopy          # -> MACOS_CERTIFICATE
security find-identity -v -p codesigning    # -> MACOS_SIGNING_IDENTITY
```

All six must be there for signing **and** notarisation: with the certificate
but without the Apple credentials the bundle is signed but not notarised, and
Gatekeeper still asks. What actually happened is written to
`dist/SIGNIERT.txt` during the build and ends up in the release text
(`signiert-und-notarisiert`, `signiert`, `unsigniert`).

Pull requests **never** touch secrets: the macOS package step for pull requests
is a different step without any `secrets.*` in its environment. The keychain
and the decoded certificate are deleted at the end of the script, also when it
fails (`trap … EXIT`).

## What "unsigned" means

* **macOS:** the bundle is ad hoc signed (`codesign -s -`). It starts on the
  machine that built it; elsewhere Gatekeeper blocks the first start.
  Right click → *Open* or *System settings → Privacy & Security → Open anyway*.
  With notarisation this step disappears.
* **Windows:** the `.msi` and the `.zip` carry no signature. SmartScreen says
  *Windows protected your PC*: *More information → Run anyway*. The `.msi`
  installs per user and needs no administrator.

None of this is a security hole — it is the absence of a paid certificate. The
checksums and the provenance still prove where the files came from.

## What is inside the packages

* **macOS:** `quiver.app` with `Contents/MacOS/quiver`, `Contents/Info.plist`
  and `Contents/Resources/AppIcon.icns`. Bundle id
  `de.weppelmann.mcp-quiver` — the same id the client announces as its app id.
  `LSUIElement` is **false**: quiver shows a window, sits in the Dock and has a
  menu bar; the tray icon is the second way to it, not the only one. Minimum
  system version 11.0, matching `MACOSX_DEPLOYMENT_TARGET`. The icon is built
  from `assets/marke/quiver-dock.png` — no new logo.
* **Linux:** the tarball is the plain tree (`bin/quiver`, `share/…`); `.deb`
  and `.rpm` carry the same files under `/usr`. The dependencies are what
  eframe and the tray load at start-up: libGL/libEGL, libxkbcommon (with its
  X11 part), Wayland and X11. The tray is **not** GTK — the client speaks
  D-Bus (StatusNotifierItem) itself — but it needs something that displays
  those icons, hence `Recommends: gnome-shell-extension-appindicator` (KDE,
  Xfce and Cinnamon bring it along).
* **Windows:** `.msi` for the current user only, installing to
  `%LOCALAPPDATA%\Programs\mcp-quiver`, with a start menu shortcut, an icon in
  *Apps & features* and an uninstall without administrator rights; plus the
  portable `.zip` with `quiver.exe` next to `LICENSE` and `README.md`.
* **The tools** are part of every package (task 35): the programs from
  [`quiver-tools`](https://github.com/patimweb-labs/quiver-tools), built on the
  same runner from the commit pinned in `verteilung/tools/stand.txt` — not from
  `main`. They sit where the client looks for them:
  `quiver.app/Contents/Resources/tools`, `<install dir>\tools` and
  `/usr/lib/mcp-quiver/tools` (inside the AppImage below `usr/lib/…`). On macOS
  each of them is signed **before** the bundle, because the bundle's signature
  seals its contents and signing afterwards would break it. Which programs
  they are, their size and the commit are named in the release notes (from
  `dist/TOOLS.txt`); the size of every file is in the *Sizes* table.
* Autostart is not part of any package: the client sets it up itself (launch
  agent, `HKCU\…\Run`, XDG autostart) when the user switches it on.

## Building a package locally

The scripts are the same ones the CI calls:

The tools first, then the package — the same order as in CI:

```bash
verteilung/tools/bauen.sh                     # dist/tools + dist/TOOLS.txt (from the network)
verteilung/macos/paket.sh                     # macOS: dist/quiver.app, dist/*.dmg
verteilung/linux/paket.sh                     # Linux: tar.gz, deb, rpm, AppImage
pwsh -File verteilung/windows/paket.ps1       # Windows: msi + zip
verteilung/version.sh                         # prints the version
```

Everything lands in `dist/` (git-ignored). `ARCH=arm64` (macOS) or
`ARCH=aarch64` (Linux) builds for the other architecture when the machine can
do it. `CARGO_TARGET_DIR` is honoured, so a separate target directory works.

`verteilung/linux/paket.sh` fetches `nfpm` and `appimagetool` into
`dist/werkzeuge/`, with a pinned version **and** a pinned SHA-256; nothing else
is downloaded.

To try the macOS bundle without touching the real profile:

```bash
profil=$(mktemp -d)
QUIVER_PROFIL="$profil" QUIVER_MCP_ADRESSE=127.0.0.1:0 QUIVER_TRESOR=speicher \
  ./dist/quiver.app/Contents/MacOS/quiver --headless
# Ctrl+C ends it; "rm -rf $profil" afterwards
```

`--headless` runs the core only. Without it the window and the tray come up;
`--hintergrund` brings the tray alone. Port `0` lets the system pick a free
port, so a second instance never fights with the one in use.

## Verifying a download

```bash
sha256sum -c SHA256SUMS                       # macOS: shasum -a 256 -c SHA256SUMS
gh attestation verify quiver-0.2.0-macos-arm64.dmg --repo patimweb-labs/mcp-quiver
```

`SHA256SUMS` covers every package in the release. The attestation says in
which run, from which commit and by which workflow the file was built; it is
signed by GitHub's OIDC identity and needs no key of ours.

## What only the CI can check

Locally testable: the workflow syntax (`actionlint`), the shell scripts
(`bash -n`), the macOS bundle including its start-up, the `.icns`, the `.dmg`.
Everything else — the Intel bundle, the MSI, `.deb`/`.rpm`/AppImage, signing
and notarisation, the release itself — only happens in the CI. A dry run
without a release: *Run workflow* on the branch.
