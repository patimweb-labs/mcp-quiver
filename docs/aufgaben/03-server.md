# 03 – Server: Bibliothek, Zuweisung, Verteilung

Crate: `crates/server` (`quiver-server`), Bibliothek (`src/lib.rs`) + Programm
(`src/main.rs`, Binärname `quiver-server`). Der Vertrag steht in
`crates/protocol/src/lib.rs` (Tabelle oben in der Datei) – **genau den** umsetzen.
Die Regeln (Soll berechnen usw.) stehen in `crates/domain` – benutzen, nicht nachbauen.

## Abhängigkeiten (nur diese)

`axum = "0.8"`, `tokio` (workspace), `tower-http = { version = "0.6", features = ["fs", "trace"] }`,
`rusqlite = { version = "0.37", features = ["bundled"] }` (nimm die neueste 0.x, die baut),
`serde`, `serde_json`, `sha2`, `hex`, `rand = "0.9"`, `time = { version = "0.3", features = ["formatting", "parsing"] }`,
`tracing`, `tracing-subscriber`, `anyhow`, `thiserror`, `futures-util`, `tokio-stream = { version = "0.1", features = ["sync"] }`,
`quiver-domain`, `quiver-protocol`, `quiver-bibliothek`.
Dev: `tower = { version = "0.5", features = ["util"] }`, `http-body-util`.

## Konfiguration (Umgebung)

| Variable | Standard | Bedeutung |
|---|---|---|
| `QUIVER_ADRESSE` | `0.0.0.0:8740` | Adresse |
| `QUIVER_DATEN` | `./daten` | Ordner für `quiver.db` und `admin-token` |
| `QUIVER_ADMIN_TOKEN` | – | Fehlt er, wird beim ersten Start einer erzeugt, in `<daten>/admin-token` (Rechte 0600) gespeichert und **einmal** ins Log geschrieben. |
| `QUIVER_ADMIN_DIST` | `./admin/dist` | Gebaute Admin-Oberfläche, ausgeliefert unter `/` (SPA: unbekannte Pfade außerhalb `/api` → `index.html`). Fehlt der Ordner, nur die API. |

## Speicherung

SQLite in `<daten>/quiver.db`. Tabellen mit JSON-Spalte (einfach, das Modell liegt in `domain`):
`eintraege(id TEXT PRIMARY KEY, json TEXT)`, `saetze(…)`, `gruppen(…)`,
`geraete(id, json, token_hash, zuletzt_gesehen, meldung_json)`,
`codes(code PRIMARY KEY, gruppen_json, gueltig_bis)`.
Zugriff aus async-Code über `tokio::task::spawn_blocking` oder eine `Mutex<Connection>` –
deine Wahl, aber nie die Verbindung über ein `.await` halten.

## Verhalten

- **Auth:** `Authorization: Bearer …`. Admin-Pfade → Admin-Token (Vergleich in konstanter Zeit).
  Geräte-Pfade → sha256(Token) muss zu einem Gerät passen; gesperrte Geräte → 403.
  Falsches/fehlendes Token → 401. Fehler immer als `quiver_protocol::Fehler`.
- **PUT** `/admin/<art>/{id}`: Kennung im Pfad ≠ Kennung im Rumpf → 400.
  Eintrag löschen, den ein Satz noch nutzt → 409. Satz löschen, den eine Gruppe nutzt → 409.
  Satz speichern, der auf einen unbekannten Eintrag zeigt → 400.
- **Codes:** 8 Zeichen aus `ABCDEFGHJKLMNPQRSTUVWXYZ23456789`, Anzeige `XXXX-XXXX`
  (Eingabe ohne Bindestrich und in Kleinbuchstaben auch gültig). Einmal benutzbar,
  abgelaufen → 410.
- **Anmelden:** legt ein `Geraet` an (`id` zufällig, 12 Zeichen hex), Gruppen aus dem Code,
  Token 32 Byte zufällig als hex; gespeichert wird nur der Hash.
- **Soll:** `quiver_domain::soll_berechnen` mit dem aktuellen Stand. Fehler daraus → 500 mit Text.
- **Ereignisse (SSE):** `/geraet/ereignisse` schickt sofort ein Ereignis `soll` mit der
  aktuellen Revision, danach **jedes Mal, wenn sich die Revision dieses Geräts ändert**
  (nach jeder Admin-Änderung neu berechnen; gleiche Revision → nichts schicken).
  Keep-Alive alle 15 s. Solange mindestens ein Strom offen ist, gilt das Gerät als `verbunden`.
  Umsetzung: ein `tokio::sync::broadcast`-Kanal „geändert“, den jede Admin-Schreiboperation auslöst.
- **Meldung:** speichert die letzte `Meldung` und `zuletzt_gesehen` (auch bei GET `/soll`).
- **Import:** Rumpf ist server.json → `quiver_bibliothek::importieren` → speichern, Antwort der Eintrag.
  Existiert die `id` schon → 409.
- **Überblick:** Zähler und `env!("CARGO_PKG_VERSION")`.
- Log mit `tracing`; Token und Codes **nie** ins Log (außer dem einmaligen Admin-Token).

## Aufbau

```rust
pub struct Konfig { pub daten: PathBuf, pub admin_token: String, pub admin_dist: Option<PathBuf> }
pub async fn app(konfig: Konfig) -> anyhow::Result<axum::Router>;
```
`main.rs` liest die Umgebung, ruft `app`, bindet, fährt bei Strg+C sauber herunter.

## Abnahme

Integrationstests in `crates/server/tests/` gegen `app(...)` mit `tower::ServiceExt::oneshot`
und einem temporären Datenordner (`std::env::temp_dir()` + zufälliger Name, am Ende löschen):

1. Ohne/mit falschem Token → 401; Admin-Pfad mit Geräte-Token → 401.
2. Ablauf: Eintrag (PUT) → Satz (PUT) → Gruppe (PUT) → Code → Anmelden → `/geraet/soll`
   enthält den Eintrag mit den Server-Werten aus dem Satz.
3. Code zweimal benutzen → beim zweiten Mal Fehler. Abgelaufener Code → 410.
4. 409 beim Löschen eines benutzten Eintrags/Satzes; 400 bei Kennung Pfad ≠ Rumpf.
5. SSE: Strom öffnen, erstes Ereignis lesen (Revision A), Satz ändern, nächstes Ereignis
   hat Revision B ≠ A. Änderung, die dieses Gerät nicht betrifft → kein Ereignis
   (mit kurzem Timeout prüfen).
6. Gesperrtes Gerät → 403 auf `/geraet/soll`.
7. Import einer npm-server.json → Eintrag mit `befehl == "npx"`.

Prüfbefehl grün. `cargo run -p quiver-server` startet und gibt die Adresse aus.
