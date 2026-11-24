# 10 – Anmeldung von Tools im Client-Fenster

Ein Tool kann eine Anmeldung des Users brauchen (Beispiel: quiver-m365, Microsoft im Browser).
**Die Anmeldung löst der User im quiver-Fenster aus – nie ein Agent.** Das Datenmodell steht schon:
`Eintrag.anmeldung: Option<Anmeldung { anmelden, status, abmelden }>` in `crates/domain` (lesen!).

Jeder Befehl ist eine Argumentliste für **dasselbe** Programm (`Laufzeit::Stdio.befehl`) mit
**derselben** Umgebung und denselben `${feld}`-Ersetzungen wie die Laufzeit
(nutze `konfig_zusammensetzen` und ersetze dann die Argumente). Nur für `Stdio`-Einträge.

## Client-Kern (`crates/client`)

1. `EintragSicht` bekommt `anmeldung: Option<AnmeldeZustand>`:
   ```rust
   pub enum AnmeldeZustand { Unbekannt, Angemeldet { konto: Option<String> }, Abgemeldet, Laeuft, Fehler { text: String } }
   ```
   (`None` = Eintrag braucht keine Anmeldung.)
2. **Status prüfen:** beim Start eines Eintrags, nach jeder An-/Abmeldung und alle 10 min:
   `status`-Befehl ausführen (Timeout 15 s). Exit 0 → `Angemeldet { konto: erste Zeile stdout }`,
   Exit 1 → `Abgemeldet`, sonst `Fehler`.
3. `Kern::anmelden(eintrag) -> anyhow::Result<()>`: Zustand `Laeuft`, `anmelden`-Befehl ausführen
   (Timeout 5 min; der Befehl öffnet selbst den Browser), danach Status prüfen und den Eintrag
   **neu starten**, damit der laufende MCP-Prozess das neue Token sieht.
4. `Kern::abmelden_eintrag(eintrag)`: `abmelden`-Befehl, Status prüfen, Eintrag neu starten.
5. stdout/stderr dieser Befehle ins Log des Clients (wie bei den MCP-Prozessen) – aber **nie** in die
   Meldung an den Server.
6. Meldung an den Server: `EintragZustand` bleibt wie er ist (kein Vertragswechsel). Optional darfst du
   in `Laeuft { tools }` nichts ändern – der Anmeldezustand bleibt lokal.

## Fenster (`crates/fenster`)

Auf der Tools-Karte eines Eintrags mit `anmeldung`:
- `Abgemeldet`/`Unbekannt`: Hinweis „Anmeldung nötig“ (Ton warn) und Hauptknopf **„Anmelden“**.
- `Laeuft`: „Anmeldung im Browser läuft …“ (Ton leise), kein Knopf.
- `Angemeldet`: „Angemeldet als <konto>“ (Ton ok) und Knopf „Abmelden“ (mit Rückfrage).
- `Fehler`: Text (Ton error) und „Erneut versuchen“.
Hand-Cursor, Bausteine, Texte in `texte.json` (de/en). Oberflächentests für alle vier Zustände
(Attrappe), Bildschirmfoto `fenster-{dark,light}-anmeldung.png`.

## Admin (`admin/`)

`typen.ts` um `anmeldung` ergänzen. Im Eintrag-Formular ein aufklappbarer Abschnitt
„Anmeldung (optional)“ mit drei Feldern (Argumente je Befehl, Leerzeichen-getrennt).
Bibliothek-Liste: Abzeichen „mit Anmeldung“.

## Abnahme

- Unit-Tests: Status-Auswertung (Exit 0/1/sonst, Kontoname), Befehlszeile mit ersetzten Platzhaltern.
- Integrationstest in `crates/client/tests/`: ein Test-Programm (neues Bin `quiver-anmelde-attrappe`
  in `crates/client/src/bin/`, merkt sich den Zustand in einer Datei aus `QUIVER_ATTRAPPE_DATEI`)
  → Status `Abgemeldet` → `Kern::anmelden` → `Angemeldet { konto: "test@beispiel.de" }` und Eintrag
  wurde neu gestartet → `abmelden_eintrag` → `Abgemeldet`.
- Prüfbefehl grün, `admin/`: typecheck, test, build grün.
