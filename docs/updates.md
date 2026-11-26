Deutsch: [updates.de.md](updates.de.md)

# Updates: who asks, who shows, who installs

quiver does **not** install itself. A device learns that a newer version exists
and shows a quiet hint; installed and updated is done by device management
(Intune) or by hand from the packages in the release. This page describes who
asks whom and what the administration shows. The step-by-step for Intune is in
[intune.md](intune.md); the design is in [ADR 0004](adr/0004-updates.md).

## At a glance

| Piece | Where |
| --- | --- |
| Server asks | GitHub release of the repository, at most every 6 hours, jitter up to 30 min |
| Switch | `QUIVER_UPDATES=an\|aus` (default `an`) |
| Identifier / payload | none — one anonymous GET, nothing else |
| Remembered in | table `update_stand` (schema 6), so a restart does not ask again |
| Client asks | only its own server (`GET /api/v1/geraet/updates`) |
| Client shows | window and tray: version, operator's text, link to the changes |
| Client installs | nothing |
| Administration shows | `GET /api/v1/admin/updates`: newest client version, newer server version, devices with older clients |
| Install / update | MSI or `.pkg` via Intune, or by hand — [intune.md](intune.md) |

## What the server asks

`GET https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest` —
unauthenticated, with `User-Agent: quiver`. Read from the answer:

* `tag_name` → the version (`v0.3.0` becomes `0.3.0`),
* `html_url` → the link to the changes,
* `published_at` → for display only.

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
the newest published version plus the link. If it is newer than the running
client, a hint appears above the page and in the tray line:

* without device management: `Version X is available`,
* with device management (any policy present): `Version X comes via device
  management`,
* always overridable by the operator's `UpdateHinweis` text.

The hint is display only: no download, no button that installs anything. There
is a link to the changes; it opens in the system browser.

## What the administration shows

The overview page shows the newest published client version, when it was last
asked, whether the **server** itself is older, and which devices report an older
client. The devices page marks those devices next to their client version.

The server never updates itself; if it is older, it is rebuilt and redeployed
(container image) — see [betrieb.md](betrieb.md).

## Failures

| Case | Behaviour |
| --- | --- |
| GitHub not reachable | last known state stays, the error appears in the administration |
| `QUIVER_UPDATES=aus` | nobody asks; the administration says so |
| no answer yet | client shows nothing; administration says "not asked yet" |
| unreadable version | rejected, logged, nothing shown |

## Policies

The three operator settings (`ServerUrl`, `Autostart`, `UpdateHinweis`) are read
only from the managed area of the platform, never written by quiver:
`HKLM\Software\Policies\mcp-quiver` on Windows, the managed preferences for
`de.weppelmann.mcp-quiver` on macOS, `/etc/mcp-quiver/vorgaben.json` on Linux
(root-owned, not world-writable). Templates with a placeholder address are in
`verteilung/vorgaben/`; how to roll them out is in [intune.md](intune.md).
