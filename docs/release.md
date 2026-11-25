Deutsch: [release.de.md](release.de.md)

# Releasing quiver

A tag `vX.Y.Z` builds the client for macOS (Apple Silicon and Intel), Windows
and Linux (x86_64 and aarch64), creates the GitHub release and attaches every
package plus `SHA256SUMS`. The server image is not built here — `abbild.yml`
does that, with the same tags.

| File | Watches over |
| --- | --- |
| `.github/workflows/release.yml` | version check, five build jobs, checksums, provenance, release |
| `verteilung/macos/paket.sh` | `quiver.app`, `.dmg`, signing and notarisation |
| `verteilung/linux/paket.sh` | `.tar.gz`, `.deb`, `.rpm`, AppImage |
| `verteilung/windows/paket.ps1` | `.msi` (per user) and portable `.zip` |
| `verteilung/version.sh` | the version from `Cargo.toml` |

## Making a release

1. **Raise the version.** It lives in one place, `[workspace.package] version`
   in `Cargo.toml`; the crates inherit it (`version.workspace = true`).
2. **Refresh `Cargo.lock`.** The lock file carries the crates' versions, so
   `cargo build --locked` in the CI fails if it is stale:

   ```bash
   cargo check --workspace --locked || cargo check --workspace
   git add Cargo.toml Cargo.lock
   ```

3. **Run the full check** (AGENTS.md §4) and merge through a pull request.
4. **Tag and push the tag:**

   ```bash
   git tag -a v0.2.0 -m "quiver 0.2.0"
   git push origin v0.2.0
   ```

5. **Watch the run.** *Actions → Release*. When it is green, the release is at
   *Releases* with all packages, `SHA256SUMS` and the provenance attestations.

The first job compares the tag with the version from `Cargo.toml`. `v0.2.0`
next to `0.1.0` stops the run before anything is built — the release name and
the version the program reports to the server must not drift apart.

### When something went wrong

* *Run workflow* builds the packages again and leaves them as artifacts of the
  run; it does **not** touch a release. Good for trying a fix.
* A push of the same tag is not possible; delete and re-push only if the
  release is to be replaced.
* One platform failed, the others worked: the run is red and the release
  carries what was built. Fixing means a new tag (`v0.2.1`) — or, as long as
  the tag is not yet out, delete the release and the tag, fix, tag again.

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
* Autostart is not part of any package: the client sets it up itself (launch
  agent, `HKCU\…\Run`, XDG autostart) when the user switches it on.

## Building a package locally

The scripts are the same ones the CI calls:

```bash
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
