# Härtung im Betrieb

Was der Server und der Client von sich aus einhalten — nachlesbar, prüfbar,
ohne Zusatzsoftware. Die Tests dazu liegen bei den jeweiligen Modulen
(`crates/server/src/*`, `crates/server/tests/*`, `crates/client/src/rechte.rs`).

## Server

| Eigenschaft | Wert | Wo |
| --- | --- | --- |
| `Host`-Kopf | muss zu `QUIVER_OEFFENTLICH` passen, sonst **421 ohne Inhalt** — auf allen Wegen, auch Startseite und Geräte-API | `src/host.rs` |
| Ausnahme | `GET /gesund`, nur über Loopback (für den `HEALTHCHECK`) | `src/host.rs`, `src/betrieb.rs` |
| Kopfzeilen | CSP ohne Inline, `X-Content-Type-Options: nosniff`, `Referrer-Policy: same-origin`, `X-Frame-Options: DENY`, `Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Resource-Policy: same-origin`, `Permissions-Policy` alles aus, HSTS bei https | `src/kopfzeilen.rs` |
| Rumpfgrenze | 64 KB je Anfrage, 256 KB für Einträge; darüber **413** | `src/lib.rs`, `src/fehler.rs` |
| Zeitlimits | Kopfzeilen 10 s, Rumpf 30 s (je Anfrage); danach wird die Verbindung geschlossen bzw. **408** geantwortet | `src/betrieb.rs`, `src/lib.rs` |
| Verbindungen | höchstens 256 gleichzeitig; ohne freien Platz wird die neue Verbindung sofort geschlossen | `src/betrieb.rs` |
| Ereignis-Ströme | höchstens 4 je Gerät und 200 insgesamt; darüber **429** | `src/zustand.rs`, `src/geraet.rs` |
| Prüfspur | jede ändernde Verwaltungsroute schreibt wer/was/Ziel (Methode, Pfadmuster, Aktion) — ohne Werte und Geheimnisse | `src/spur.rs` |
| Dateirechte | Datenordner 0700; `quiver.db`, `-wal`, `-shm`, Sicherungen (`.vor-*`), `entra-*`, `schluessel` 0600 — beim Anlegen (umask am Prozessanfang) und beim Start nachgezogen, Symlinks nicht verfolgt | `src/rechte.rs` |
| Sicherungen | die letzten drei bleiben | `src/rechte.rs`, `src/speicher.rs` |
| Lieferkette | `cargo deny check` (Sicherheitslücken auch für eingestellte Kisten, Lizenzen, nur crates.io, keine doppelten Krypto-Kisten); Regeln und jede Ausnahme mit Grund in `deny.toml` | `deny.toml`, `.github/workflows/pruefen.yml` |

Die Prüfspur entsteht in einer Wache und nicht in den Ansichten: eine Route
kann sie nicht vergessen, auch keine, die später dazukommt. Eine abgelehnte
Änderung (400, 403, 404, 409) schreibt nichts — die Spur ist ein Verzeichnis
der Änderungen, kein Zugriffsprotokoll.

## Container

Die Laufzeit besteht aus dem Programm und sonst nichts (distroless, ohne Shell
und ohne Paketverwaltung), Benutzer `10001:999`, alle Basis-Abbilder auf einen
Digest festgelegt. `compose.yaml` und die README nennen denselben Aufruf:

```
--read-only --tmpfs /tmp --cap-drop=ALL --security-opt=no-new-privileges
--memory --pids-limit --log-rotation
```

`tools/container-pruefung.sh` startet genau so einen Container mit eigenem
Namen, eigenem Volume und freiem Port (`quiver-pruefung-*`), prüft die Grenzen,
den Gesundheitsweg, `docker exec` und einen zweiten Start auf demselben Volume
und räumt ihn wieder ab.

## Client

| Eigenschaft | Wert | Wo |
| --- | --- | --- |
| Profilordner | 0700 | `crates/client/src/rechte.rs` |
| Dateien darin | 0600 (`zustand.json`, `oberflaeche.json`, Protokolle, Tresor, Sperrdatei) | `rechte.rs`, `profil.rs`, `protokoll.rs`, `dateitresor.rs` |
| Schreiben | atomar (Zwischendatei, dann umbenennen); ein Symlink am Ziel wird ersetzt, nicht verfolgt | `rechte.rs` |
| Beim Start | von Hand verstellte Rechte werden nachgezogen | `Kern::starten` → `rechte::profil_pruefen` |
| Windows | diese Bits gibt es nicht; die Vererbung der ACL des Benutzerprofils genügt (Profil liegt in `%APPDATA%`) | README |

Die Protokolle bleiben zusätzlich maskiert (siehe README, „Protokoll und
Datenschutz“): Geheimschlüssel und alle Werte aus dem Tresor werden ersetzt,
bevor eine Zeile auf die Platte geht.
