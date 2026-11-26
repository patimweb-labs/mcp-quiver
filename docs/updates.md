Deutsch: [updates.de.md](updates.de.md)

# Updates: who asks, who shows, who installs

The server learns that a newer version exists and shows it in the
administration. The client learns it **from its server** and offers the update:
"Update now" or "Later". The client never asks the internet. Whoever runs no
device management can still update by hand from the release packages; the design
is in [ADR 0005](adr/0005-client-update.md), the step-by-step for Intune is in
[intune.md](intune.md).

## At a glance

| Piece | Where |
| --- | --- |
| Server asks | GitHub release of the repository, at most every 6 hours, jitter up to 30 min |
| Switch | `QUIVER_UPDATES=an\|aus` (default `an`) — switches the query **and** the package delivery |
| Identifier / payload | none — one anonymous GET, nothing else |
| Remembered in | table `update_stand` (schema 6), so a restart does not ask again |
| Server keeps | the package under `<data>/updates/<version>/`, checked against `SHA256SUMS` |
| Client asks | only its own server (`GET /api/v1/geraet/updates`, `…/updates/paket/{name}`) |
| Client shows | window and tray: version, operator's text, link to the changes, "Update now" / "Later" |
| Client installs | itself — after checksum, size and self-test (see below) |
| Administration shows | `GET /api/v1/admin/updates`: newest client version, newer server version, devices with older clients |
| Device management | MSI or `.pkg` via Intune — [intune.md](intune.md) |

## What the server asks

`GET https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest` —
unauthenticated, with `User-Agent: quiver`. Read from the answer:

* `tag_name` → the version (`v0.3.0` becomes `0.3.0`),
* `html_url` → the link to the changes,
* `published_at` → for display only,
* `assets` → file names, sizes and download URLs of the packages.

A draft or a pre-release is **not** an offer: it is rejected. Values that do not
fit are ignored and logged; a failed question leaves the last known state
untouched and shows the reason in the administration.

Whether a version is "new" is decided by a version comparison, never by the
order of the answers: `0.3.0` is newer than `0.2.9`, `1.0.0-rc.1` is older than
`1.0.0`. An unreadable version is never newer — so no answer can cause a
downgrade hint.

The source is fixed to the project's repository in production; tests and
development may point `QUIVER_UPDATE_QUELLE` at a loopback address.

## What the client shows

The client asks its server at most every six hours (jitter 30 minutes) and gets
the newest published version, the link and the packages for its system. If the
version is newer than the running client, a hint appears above the page and in
the tray line:

* without device management: `Version X is available`,
* with device management (any policy present): `Version X comes via device
  management`,
* always overridable by the operator's `UpdateHinweis` text.

If a package for **this** installation exists, two buttons appear next to it:

* **Update now** — download, check, self-test, swap (below),
* **Later** — the hint disappears for this version until the next start.

Otherwise only the reason is shown: updates switched off by the operator,
folder belongs to the system, `deb`/`rpm` need root, or no package for this
installation. There is always a link to the changes; it opens in the system
browser.

## How the client swaps itself

1. **Stop the tools.** Running calls get up to 10 seconds to finish, then the
   entries are stopped. Nothing is swapped in the middle of a call.
2. **Download** the package from its own server (device token) into
   `<profile>/updates/<version>/`. The client checks `Content-Length` and the
   SHA-256 from the server's answer while streaming; a mismatch deletes the file.
3. **Check and self-test.** Unpack (ZIP/`tar.gz` with `flate2`, `.dmg` with
   `hdiutil`/`ditto`), then run the new binary as `quiver --selbsttest`: exit 0,
   no window, no profile. If it fails, nothing changes.
4. **Swap** according to the installation type (see the table in
   [ADR 0005](adr/0005-client-update.md)): rename the old file to `<name>.alt`,
   put the new one next to it, write a marker into the profile.
5. **Restart.** The window says "Version X is installed", closes, and only
   after the profile has been released does the new version start. On the second
   start without proof the old version comes back (`<name>.alt`).

The **profile is never touched**: settings, vault, device token, entries,
access data, tool sign-ins, autostart and the downloaded tools stay exactly as
they were. A test compares the profile before and after the swap.

## What the administration shows

The overview page shows the newest published client version, when it was last
asked, whether the **server** itself is older, and which devices report an older
client. The devices page marks those devices next to their client version.

The server never updates itself; if it is older, it is rebuilt and redeployed
(container image) — see [betrieb.md](betrieb.md).

## Failures

| Case | Behaviour |
| --- | --- |
| GitHub not reachable | last known state stays, the error appears in the administration; no buttons |
| `QUIVER_UPDATES=aus` | nobody asks and nothing is delivered; the administration says so |
| no answer yet | client shows nothing; administration says "not asked yet" |
| unreadable version | rejected, logged, nothing shown |
| checksum wrong | server does not deliver, client does not swap; message in the window |
| self-test fails | nothing is swapped; the message names the reason |
| new version does not start | second start restores `<name>.alt` |

## Policies

The four operator settings (`ServerUrl`, `Autostart`, `UpdateHinweis`,
`UpdateAngebot`) are read only from the managed area of the platform, never
written by quiver: `HKLM\Software\Policies\mcp-quiver` on Windows, the managed
preferences for `de.weppelmann.mcp-quiver` on macOS, `/etc/mcp-quiver/vorgaben.json`
on Linux (root-owned, not world-writable). `UpdateAngebot=false` switches the
offer off; the default is on. Templates with a placeholder address are in
`verteilung/vorgaben/`; how to roll them out is in [intune.md](intune.md).
