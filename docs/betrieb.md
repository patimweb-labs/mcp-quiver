Deutsch: [betrieb.de.md](betrieb.de.md)

# Running quiver-server

The server is a library and a control plane; it runs no tools. It ships as one
container image with one port (8740) and one volume (`/daten`).

**Inhalt** · [Container](#container) · [First setup](#first-setup) ·
[Address and Host](#address-and-host) · [Entra ID](#entra-id) ·
[Configuration](#configuration) · [Reverse proxy](#reverse-proxy) ·
[Backup](#backup) · [Upgrading](#upgrading) · [Logs](#logs) ·
[Troubleshooting](#troubleshooting)

## Container

```bash
docker build -t mcp-quiver-server .
docker run -d --name quiver -p 127.0.0.1:8740:8740 \
  -e QUIVER_OEFFENTLICH=http://localhost:8740 \
  --read-only --tmpfs /tmp:size=32m,mode=1777 \
  --cap-drop=ALL --security-opt=no-new-privileges:true \
  --memory 512m --pids-limit 256 \
  --log-driver json-file --log-opt max-size=10m --log-opt max-file=3 \
  -v quiver-daten:/daten mcp-quiver-server
```

`compose.yaml` is the same call in a file:

```bash
cp .env.example .env     # fill in, never put secrets in the repository
docker compose up -d
docker compose logs -f
```

The file uses the image from `QUIVER_IMAGE` (default: a local build) and keeps
the port on loopback, where the TLS-terminating proxy sits.

The image is built for lean operation: the runtime is the program and nothing
else — **no shell, no package manager** (distroless), user `10001:999`, all base
images pinned by digest. There is no shell inside to be exploited.

`tools/container-pruefung.sh` starts exactly such a container under its own
name and volume (`quiver-pruefung-*`) on a free port, checks the limits, the
health route, `docker exec`, a second start on the same volume and the exit code
after `docker stop` (0, not 137), and removes it again.

### What the container may do

| Property | Value |
| --- | --- |
| Root file system | read-only (`--read-only`), only `/tmp` is a tmpfs |
| Capabilities | none (`--cap-drop=ALL`) |
| Privilege escalation | impossible (`no-new-privileges`) |
| Memory / processes | 512 MB / 256 PIDs |
| Log | rotated (10 MB × 3) |
| Listen address | all addresses:8740; the publish keeps it on `127.0.0.1` |
| Health | `quiver-server gesund` asks the server over loopback only |

### Data and permissions

Existing volumes keep working: the data directory belongs to `10001:999` (UID
*and* GID), so a volume created by an earlier version opens without a manual
`chown`. Inside the volume the data directory is `0700` and `quiver.db`,
`-wal`, `-shm`, the backups (`.vor-*`), `entra-*` and `schluessel` are `0600`;
only the last three backups are kept. The process sets its umask at start-up,
so these permissions apply from the first second, and a restart corrects files
that were changed by hand (symlinks are not followed).

`docker exec -it quiver quiver-server …` keeps working without a shell, because
the binary sits in `/usr/local/bin`.

Without a Docker Hub login, build with
`--build-arg BASIS=public.ecr.aws/docker/library`.

## First setup

Open the base URL. The first page is the **initial setup**: it asks for the
e-mail and password of the one root user (12 to 256 characters, Argon2id) and
signs that user in. The setup is only available for 15 minutes after the server
starts, and only with a one-time code that the server prints once at start-up —
the code is required whenever the server listens on more than loopback (so
always in the container), regardless of the base URL:

```
INFO quiver_server::einrichtung: Ersteinrichtung: Code für diesen Start: …
```

The window follows the server's uptime, not the wall clock; after it closes,
restart the server. A forgotten password is replaced through the command line,
and always through `--stdin`:

```bash
read -rsp 'New password: ' P; echo; printf '%s\n' "$P" | \
  docker exec -i quiver quiver-server root-setzen <e-mail> --stdin; unset P
```

`docker exec -i` without `-t`, because the image is distroless and has no
`stty`: the prompt cannot hide what is typed, and instead of echoing the
password onto the terminal the binary stops with a message pointing at
`--stdin`. The shell reads the password itself (`read -rsp`), keeps it out of
the command line and out of the shell history, and `unset P` clears it
afterwards. There is no admin token any more; `QUIVER_ADMIN_TOKEN` is ignored
and `daten/admin-token` is removed as soon as a root user exists.

The container always listens on all addresses; the publish above keeps it on
`127.0.0.1` only. That is why the base URL may be plain `http`: with anything
but an https base URL the server refuses to start unless the host is
loopback. The one-time code is still required, because the listen address
decides, not the base URL. Only an explicit `QUIVER_EINRICHTUNG=ohne-code`
together with a loopback base URL lifts it, and that is logged loudly at
start-up. Whoever can reach the port during the first 15 minutes could
otherwise create the root. Publish on `127.0.0.1` or use an `https` base URL.

Sign-in afterwards: e-mail and password for the root, and **Sign in with
Microsoft** once Entra is configured. The session lives in an HttpOnly cookie
(`SameSite=Lax`, `Secure` and `__Host-` on https, 12 h absolute, 60 min idle);
nothing is kept in the browser's storage.

If the setup window is closed: `docker restart quiver`, then reload the page.
The code changes with every start.

## Address and Host

`QUIVER_OEFFENTLICH` is the only address the server accepts. Every request must
carry a `Host` header that matches it — on all routes, including the start page
and the device API. A mismatch is answered with **421 without a body**. The one
exception is `GET /gesund`, and only over loopback (for the health check).

A base URL other than `http://localhost…`/`http://127.0.0.1…` must be `https`;
the server refuses to start otherwise.

## Entra ID

**Full guide, click by click and with the Azure CLI, including troubleshooting:
[entra.md](entra.md).** This is the short version.

Without Entra the server issues one-time device codes from the admin UI. With
Entra, devices sign in through the browser and groups come from the token.

1. Register an application in your Entra tenant (single tenant). Add
   `<base URL>/auth/callback` as a redirect URI (web platform). Leave
   implicit ID and access tokens **off**.
2. Make the app emit the groups assigned to the application (`groups` claim,
   object ids only, no directory read permission). Very large group lists are
   rejected on purpose; limit the assignment instead.
3. Choose an identity — **exactly one** of the two:
   * **Certificate (recommended, `private_key_jwt`):** `quiver-server
     entra-schluessel` creates an RSA 3072 key and a self-signed certificate
     valid for one year. It prints **only** the public certificate, the
     thumbprint and the expiry; upload that certificate to the application in
     Entra (the certificate is also written to
     `<data>/entra-schluessel.crt.pem`). The private key
     (`<data>/entra-schluessel.pem`, mode 0600) never leaves the server. No
     client secret, which many tenants forbid by policy.
   * **Secret:** in `<data>/entra-secret` (or `QUIVER_ENTRA_SECRET_DATEI`).
4. Start the server with `QUIVER_OEFFENTLICH=https://quiver.example.test`,
   `QUIVER_ENTRA_TENANT=<tenant GUID>`, `QUIVER_ENTRA_CLIENT_ID=<app id>` and
   the key (default `<data>/entra-schluessel.pem`, or
   `QUIVER_ENTRA_SCHLUESSEL_DATEI`) **or** the secret. If both are present the
   server refuses to start; if one of the three pieces is missing, it does not
   start either. Permissions more open than 0600 are reported in the log.
5. No mapping step: every group id seen in a token creates a group
   "Entra-Gruppe <first 8 characters>" **once**, without tool sets – so it
   grants nothing until an admin assigns a set in the admin UI. After that the
   title and sets belong to the admin; Entra only replaces the members. Users
   without a group get no tools. With `QUIVER_ENTRA_GRUPPENNAMEN=an` the server
   also fetches the group's display name via Graph (read scope
   `GroupMember.Read.All`; the app needs the tenant's consent). The name
   replaces the automatic title as long as the admin has not set one of their
   own, and is shown small below the title in the admin UI. Without the switch
   the server never calls Graph; either way, permissions run on the object id
   only.
6. Root can appoint admins; an Entra user must sign in once before
   `quiver-server admin-setzen --oid <object id>` works.

**Rotating the certificate without downtime:** `quiver-server entra-schluessel
--erneuern` creates a second key (the active one stays untouched);
`--umschalten` makes it active, and the running server picks it up without a
restart. Both certificates are registered in Entra, so there is no gap. From 30
days before expiry the server warns in the log, in the admin UI and in the audit
trail (once per day). Existing keys are never overwritten.

The server refreshes each user's token hourly (with jitter) and when a device
connects after more than an hour. If Entra is unreachable, the last state stays
valid for at most 24 hours. If a user is blocked, no longer assigned, or their
token is revoked, their devices lose their tools on the next check. If an admin
deletes a group that came from Entra, it stays deleted; the admin UI can adopt
it again. At most 200 groups are created this way.

## Configuration

| Variable | Meaning |
| --- | --- |
| `QUIVER_OEFFENTLICH` | Base URL (`https`, or `http` only on loopback; default `http://localhost:<port of QUIVER_ADRESSE>`) |
| `QUIVER_ADRESSE` | Listen address (default `0.0.0.0:8740`) |
| `QUIVER_DATEN` | Data folder (default `./daten`) |
| `QUIVER_ADMIN_DIST` | Built admin UI (default `./admin/dist`) |
| `QUIVER_ENTRA_TENANT` | Tenant GUID (never `common`/`organizations`) |
| `QUIVER_ENTRA_CLIENT_ID` | Application (client) id |
| `QUIVER_ENTRA_SECRET_DATEI` | Client secret file (default `<data>/entra-secret`); exactly one of secret and key |
| `QUIVER_ENTRA_SCHLUESSEL_DATEI` | Private key for `private_key_jwt` (default `<data>/entra-schluessel.pem`) |
| `QUIVER_EINRICHTUNG` | `mit-code` (default) or `ohne-code` (only with a loopback base URL) |
| `QUIVER_ENTRA_AUTHORITY` | Alternative authority, loopback only (tests) |
| `QUIVER_ENTRA_GRUPPENNAMEN` | `an` fetches group names via Graph; default `aus`, any other value prevents startup |
| `QUIVER_ENTRA_GRAPH` | Base URL for Graph, loopback only (tests); fixed to `https://graph.microsoft.com` in production |
| `QUIVER_PROXIES` | CIDR list trusted for `X-Forwarded-For` |
| `QUIVER_ALTGERAETE` | `gesperrt` (default once Entra is configured, otherwise `erlaubt`) for devices without a user |
| `QUIVER_UPDATES` | `an` (default) or `aus`: may the server ask the GitHub releases for a new version? At most every six hours, no identifier, no payload. See [updates.md](updates.md) |
| `QUIVER_UPDATE_QUELLE` | loopback only (tests): base of the release query; fixed to the project repository in production |

`.env.example` is the template.

## Reverse proxy

Terminate TLS at the proxy, pass `Host` through unchanged, and list the proxy's
network in `QUIVER_PROXIES` (CIDR, comma separated) so the server trusts
`X-Forwarded-For` for throttling and the audit trail. The publish stays on
`127.0.0.1`; the proxy is the only way in.

## Backup

All state lives in `/daten` (the volume):

* `quiver.db` with `-wal` and `-shm` — library, sets, groups, users, devices,
  audit trail, sessions;
* `schluessel` — the key that encrypts refresh tokens. **Back it up, but
  separately from the database:** without it the tokens are useless, with it
  together the backup is a credential;
* `entra-schluessel.pem` and `entra-schluessel.crt.pem` if Entra uses a certificate;
* `.vor-*` — automatic backups before a migration (the last three are kept).

Stop the container or use SQLite's backup API; copying a live `quiver.db`
without the WAL can lose the newest transactions. A restore is: stop, replace
the data folder, start.

## Upgrading

Replace the image and restart:

```bash
docker compose pull
docker compose up -d
```

Before a migration the server writes a backup and checks the integrity of both
files. Database migrations have no way back; to go back, restore the data
folder from the backup.

## Logs

`RUST_LOG` controls the level (`info` in the image). The process logs to
stdout; Docker rotates it. Every end of the client and every "left in the
tray" is a client-side log line (see [client.md](client.md)), not here.

During the first 15 minutes the log contains the one-time setup code. Treat the
log of that window as a secret.

## Troubleshooting

| Message / symptom | Cause and remedy |
| --- | --- |
| `421` on every page | `Host` does not match `QUIVER_OEFFENTLICH`. Open exactly that address; `localhost` and `127.0.0.1` are different. Behind a proxy: pass `Host` through. |
| Server refuses to start: base URL not https | Use `https`, or publish on loopback and use `http://localhost:…`. |
| Setup page says the window is closed | The first 15 minutes are over. `docker restart quiver`, reload. |
| Setup asks for a code that is not known | It is in the log of **this** start: `docker logs quiver \| grep Einrichtung`. |
| `docker exec … quiver-server` does not work | Use the absolute path or the name in `PATH` (`/usr/local/bin/quiver-server`); there is no shell. |
| Device gets no tools | The group has no set, the set is empty, or the entry is inactive. Check admin → Groups/Sets/Library. |
| Entra sign-in fails (`AADSTS…`) | See the table in [entra.md](entra.md#when-it-gets-stuck). |
| Group names stay "Entra-Gruppe …" | `QUIVER_ENTRA_GRUPPENNAMEN=an` and consent for `GroupMember.Read.All` are both required. |
| Certificate warning from 30 days before expiry | Renew with `entra-schluessel --erneuern`, upload, then `--umschalten`; see [entra.md](entra.md#renew-the-certificate-annually-without-downtime). |
| `docker stop` ends with 137 | SIGTERM did not reach the process; the image ends with 0. Check that the container was not killed by the runtime. |
| Health check fails | `quiver-server gesund` must answer over loopback; check the port and that the process is up. |
