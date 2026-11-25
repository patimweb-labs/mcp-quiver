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
| Prüfspur, Aufräumen | Einträge bleiben 180 Tage; aufgeräumt wird höchstens stündlich im Hintergrund, nicht beim Schreiben. Fehlversuche je Adresse und Stunde stehen als **ein** Eintrag mit Zähler (Index auf `zeit`) | `src/speicher.rs`, `src/refresh.rs` |
| Anmelde-Start | `/auth/geraet/start` und `/auth/admin/start` sind je Adresse gedrosselt; höchstens 2000 offene Vorgänge, darüber **503** mit Hinweis. Aufgeräumt wird im Hintergrund (Index auf `gueltig_bis`) | `src/anmeldung.rs`, `src/speicher.rs` |
| Drossel-Karte | höchstens 50 000 Einträge; der älteste ohne Sperre weicht zuerst. Aufgeräumt in Schritten zu 1000, damit die Sperre nicht alles aufhält | `src/drossel.rs` |
| Root-Drossel | Eine Adresse, von der in 30 Tagen eine Root-Anmeldung gelang, unterliegt nur der IP-Drossel; für andere gilt zusätzlich die Konto-Drossel. Ist die Adresse nicht unterscheidbar, sperrt das Konto höchstens 60 s | `src/benutzer.rs` |
| Ersteinrichtung | Code ist Pflicht, sobald der Server nicht nur auf Loopback lauscht (also auch im Container), unabhängig von der Basis-URL. Ausnahme nur `QUIVER_EINRICHTUNG=ohne-code` **und** Loopback-Basis-URL, mit deutlicher Warnung im Log. Fehlversuche zählen je Adresse (10), global erst nach 100 | `src/einrichtung.rs`, `src/config.rs` |
| Geräte-Token | Alter und letzte Nutzung werden gespeichert; 60 Tage ungenutzt oder 365 Tage alt → `neu-anmelden`. Vorhandene Geräte beginnen bei der Migration bei Null | `src/auth.rs`, `src/speicher.rs` |
| Gleichzeitiges Bearbeiten | `ETag` auf Eintrag, Satz, Gruppe und Gerät; ein abweichender `If-Match` ergibt **409** („inzwischen geändert“), die Verwaltung lädt dann neu | `src/admin.rs`, `admin/src/logik/etag.ts` |
| Grenzen der Verwaltung | Namen 80, Titel 120, Beschreibung 1000, Felder 50, Argumente 50 (je 4000), Umgebung 50 (je 4000); darüber **422** mit benanntem Feld. Eine Meldung wird gekürzt (200 Einträge, Text 500) statt abgelehnt | `src/grenzen.rs` |
| Gruppen und Sätze | Verweise auf unbekannte Sätze oder Einträge ergeben **422** mit den Namen | `src/admin.rs` |
| Fehler nach außen | Ein 500 trägt nur eine kurze Meldung mit Kennung; die Einzelheiten stehen unter derselben Kennung im Log. Ebenso nennt eine abgelehnte Rumpf-Form keine Feldpfade | `src/fehler.rs` |
| Sicherung vor Migration | über die Sicherungsschnittstelle von SQLite (mit WAL), danach `integrity_check` auf Sicherung und Datenbank | `src/speicher.rs` |
| Altgeräte | mit eingerichtetem Entra ohne Angabe `gesperrt`; ohne Entra bleibt der Weg offen | `src/config.rs` |
| Beenden | SIGTERM wirkt wie Strg+C (PID 1 im Container): Ereignis-Ströme enden sofort, offene Anfragen laufen höchstens 5 s aus, dann WAL-Checkpoint und Ende mit 0. `docker stop` endet damit nicht mit 137 | `src/main.rs`, `src/betrieb.rs` |
| Ereignis-Strom | erst abonnieren, dann den Zugang prüfen, dann die Revision rechnen — eine Änderung dazwischen geht nicht verloren. Ein entzogener Zugang endet sofort (403 mit Grund beim Öffnen, sonst Ende des Stroms), auch wenn der Grund erst beim Rechnen auffällt. `Cache-Control: no-store, no-transform` und `X-Accel-Buffering: no`, damit kein Vermittler den Rumpf zurückhält | `src/geraet.rs`, `src/auth.rs` |
| Soll-Abgleich | Der Client fragt den Soll **direkt nach dem Öffnen** und **am Ende** des Ereignis-Stroms nach, zusätzlich alle 5 Minuten (± Streuung). Ein Entzug hängt damit an keinem einzelnen Ereignis | `crates/client/src/abgleich.rs` |
| Dateirechte | Datenordner 0700; `quiver.db`, `-wal`, `-shm`, Sicherungen (`.vor-*`), `entra-*`, `schluessel` 0600 — beim Anlegen (umask am Prozessanfang) und beim Start nachgezogen, Symlinks nicht verfolgt | `src/rechte.rs` |
| Sicherungen | die letzten drei bleiben | `src/rechte.rs`, `src/speicher.rs` |
| Lieferkette | `cargo deny check` (Sicherheitslücken auch für eingestellte Kisten, Lizenzen, nur crates.io, keine doppelten Krypto-Kisten); Regeln und jede Ausnahme mit Grund in `deny.toml` | `deny.toml`, `.github/workflows/pruefen.yml` |

Die Prüfspur entsteht in einer Wache und nicht in den Ansichten: eine Route
kann sie nicht vergessen, auch keine, die später dazukommt. Eine abgelehnte
Änderung (400, 403, 404, 409, 422) schreibt nichts — die Spur ist ein Verzeichnis
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
den Gesundheitsweg, `docker exec`, einen zweiten Start auf demselben Volume und
den Exit-Code nach `docker stop` (0, nicht 137), und räumt ihn wieder ab.

## Client

| Eigenschaft | Wert | Wo |
| --- | --- | --- |
| Profilordner | 0700 | `crates/client/src/rechte.rs` |
| Dateien darin | 0600 (`zustand.json`, `oberflaeche.json`, Protokolle, Tresor, Sperrdatei) | `rechte.rs`, `profil.rs`, `protokoll.rs`, `dateitresor.rs` |
| Schreiben | atomar (Zwischendatei, dann umbenennen); ein Symlink am Ziel wird ersetzt, nicht verfolgt | `rechte.rs` |
| Beim Start | von Hand verstellte Rechte werden nachgezogen | `Kern::starten` → `rechte::profil_pruefen` |
| Windows | diese Bits gibt es nicht; die Vererbung der ACL des Benutzerprofils genügt (Profil liegt in `%APPDATA%`) | README |
| Zwischendateien | Name `<ziel>.<pid>.<zufall>.neu`; was nach einem Absturz liegen blieb, wird beim Start entfernt, wenn es älter als eine Stunde ist | `rechte.rs` |

Die Protokolle bleiben zusätzlich maskiert (siehe README, „Protokoll und
Datenschutz“): Geheimschlüssel und alle Werte aus dem Tresor werden ersetzt,
bevor eine Zeile auf die Platte geht.
