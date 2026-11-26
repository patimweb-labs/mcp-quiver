Deutsch: [intune.de.md](intune.de.md)

# Rolling out quiver with Intune

Two steps: upload the package, roll out the policies. Both are optional — quiver
runs without device management; the clients then update by hand from the
release.

## Windows

### 1. Upload the MSI

1. Download `quiver-<version>-windows-x86_64.msi` from the release.
2. Intune → **Apps** → **Windows** → **Add** → **Line-of-business app**.
3. Select the MSI, name it `mcp-quiver`, publisher `Patrick Weppelmann`,
   version as in the file name.
4. Install command: `msiexec /i "quiver-<version>-windows-x86_64.msi" /qn`.
   Uninstall command: `msiexec /x "{ProductCode}" /qn` (Intune fills in the
   product code from the MSI).
5. Requirement: 64-bit Windows 10/11. Detection rule: leave the MSI defaults
   (product code and version).
6. Assign to a group; a newer MSI with the same `UpgradeCode` replaces the old
   version.

The MSI is **per user**: it installs into `%LOCALAPPDATA%\Programs\mcp-quiver`
and needs no administrator rights. Its `MajorUpgrade` replaces an older version;
the Restart Manager closes a running `quiver.exe` (WM_CLOSE, orderly) and starts
it again after the update. Uninstalling leaves the profile
(`%APPDATA%\mcp-quiver`) alone.

### 2. Roll out the policies

Intune → **Devices** → **Configuration** → **Create** → **Custom** (Windows
10 and later) with the ADMX/ADML from `verteilung/vorgaben/`:

* **quiver.admx** goes into the ADMX store, **de-DE/quiver.adml** and
  **en-US/quiver.adml** with it.
* Then set under **mcp-quiver**:
  * *Server address*: e.g. `https://quiver.example.test` — fixed, not editable
    in the window,
  * *Start at sign-in*: on or off,
  * *Update hint text*: e.g. `Version {version} is available via device
    management`,
  * *Install updates itself*: off, if the client must **not** offer the update
    (then only the hint; Intune installs it).

The values land in `HKLM\Software\Policies\mcp-quiver`; quiver only reads them.
No ADMX? A single registry template with the same four values does the same.

## macOS

### 1. Upload the `.pkg`

1. Download `quiver-<version>-macos-<arch>.pkg` from the release (`arm64` for
   Apple Silicon, `x86_64` for Intel).
2. Intune → **Apps** → **macOS** → **Add** → **macOS app (PKG)**.
3. Install command: `installer -pkg "quiver-<version>-macos-<arch>.pkg" -target /`
   (Intune defaults to this).
4. Detection rule: bundle `de.weppelmann.mcp-quiver` or the file
   `/Applications/quiver.app`.

The `.pkg` installs into `/Applications` and replaces an older version. Its
`preinstall` script quits a running client (SIGTERM, orderly) and notes that it
was running; `postinstall` starts it again for the signed-in user. The profile
(`~/Library/Application Support/mcp-quiver`) stays untouched.

The `.dmg` remains for manual installation.

### 2. Roll out the policies

Intune → **Devices** → **Configuration** → **Create** → **New policy** →
**macOS** → **Templates** → **Custom**. Import
`verteilung/vorgaben/de.weppelmann.mcp-quiver.mobileconfig` (replace
`https://quiver.example.test` first).

The profile writes the managed preferences for `de.weppelmann.mcp-quiver` — the
only place quiver reads them from. The user's own preferences are ignored.

## Linux (without Intune)

Root writes `/etc/mcp-quiver/vorgaben.json` (see
`verteilung/vorgaben/vorgaben.json`) — owner root, mode 0644. A file that is
world-writable or not owned by root is ignored and logged.

## What the devices show

The client shows a quiet hint when the server reports a newer version. With a
policy in place the text comes from `UpdateHinweis` (default: "Version X comes
via device management"). The administration lists every device whose client is
older — see [updates.md](updates.md).

### Self-update or device management

If a package for the installation is available, the client also offers **Update
now**: it downloads the package from its own server, checks size and SHA-256,
starts the new version as `quiver --selbsttest` and only then swaps it. The
profile (settings, vault, device token, tool sign-ins) stays untouched; the
running tools are stopped first, and the client restarts itself.

For managed fleets that is usually **not** wanted — Intune should own the
version. Set *Install updates itself* to **off** (`UpdateAngebot=false`): the
client then shows the hint only, no button. The default is on.

The `.pkg` and the MSI fit both ways: Intune can install them, and the client
uses the same packages (MSI per user, `.dmg` with the `.app`) when it is allowed
to.

## Checking the packages before rollout

```bash
# macOS: look inside, do not install
pkgutil --expand-full quiver-<version>-macos-arm64.pkg /tmp/quiver-pkg
ls /tmp/quiver-pkg/*.pkg/Payload/Applications/quiver.app/Contents/MacOS/quiver
ls /tmp/quiver-pkg/*.pkg/Scripts/

# Windows: silent install, upgrade, uninstall — on a test machine
msiexec /i quiver-<version>-windows-x86_64.msi /qn
msiexec /i quiver-<next>-windows-x86_64.msi /qn
msiexec /x quiver-<next>-windows-x86_64.msi /qn
```

The CI does the same on pull requests (Windows runner and macOS runner), so a
broken package fails there first.
