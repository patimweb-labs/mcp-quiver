# 08 – Hintergrundbetrieb: Tray, Fenster nur auf Wunsch, Autostart

Crate: `crates/fenster` (Binär `quiver`). Der Client muss **im Hintergrund weiterlaufen**:
Der MCP-Endpunkt und der Abgleich dürfen nie stoppen, nur weil das Fenster zu ist.

## Verhalten

1. **Fenster schließen** (rotes X / Strg+W / Cmd+W) beendet **nicht** das Programm: das Fenster
   verschwindet, der Kern läuft weiter, das Tray-Symbol bleibt.
2. **Tray-Symbol** auf macOS (Menüleiste), Windows (Infobereich), Linux (StatusNotifierItem).
   Menü:
   - Zustandszeile, nicht klickbar: „Verbunden · 4 Tools · 1 Agent“ bzw. „Nicht verbunden“
   - „Fenster öffnen“
   - „Beim Anmelden starten“ (Häkchen, schaltet den Autostart)
   - Trenner, „Beenden“ (hält den Kern sauber an, `kern.beenden()`)
   Klick auf das Symbol (Windows/Linux, wo üblich) öffnet das Fenster.
3. **Start im Hintergrund:** `quiver --hintergrund` startet ohne Fenster, nur Tray + Kern.
   Ohne Argument öffnet sich das Fenster wie bisher.
4. **Nur eine Instanz:** Ein zweiter Start (z. B. Doppelklick, während es im Tray läuft) öffnet
   das Fenster der laufenden Instanz und beendet sich. Umsetzung z. B. über eine kleine lokale
   Datei/Socket im Profil – der MCP-Port 8741 ist ohnehin belegt, das ist das Erkennungszeichen;
   wie die laufende Instanz das Fenster öffnet, ist deine Wahl (z. B. ein lokaler Endpunkt
   `POST /fenster` auf demselben Server **hinter derselben Token-Wache ist falsch** – nimm einen
   eigenen Weg ohne Token, der nur „Fenster zeigen“ kann und sonst nichts).
5. **Autostart beim Anmelden** – schaltbar im Tray **und** im Fenster (Seite „Verbindung“ oder
   eine neue Seite „Einstellungen“, Baustein `haken`). Startet immer mit `--hintergrund`.
   - macOS: LaunchAgent `~/Library/LaunchAgents/io.github.patimweb-labs.quiver.plist`
     (`RunAtLoad`, Programm = aktueller Pfad von `quiver`)
   - Windows: `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, Wert `mcp-quiver`
   - Linux: `~/.config/autostart/io.github.patimweb-labs.quiver.desktop`
   Der Zustand wird aus dem System gelesen (gibt es den Eintrag?), nicht nur gemerkt.
   Umgebungsvariable `QUIVER_AUTOSTART_DIR` verlegt das Ziel (für Tests, wie in crosspoint).

## Vorlagen (vom selben Autor, dürfen übernommen werden; Kopfzeile auf MIT ändern)

- Tray: wledambilight `crates/fenster/src/tray/{mod,native,linux}.rs`
  (`gh api repos/patimweb-labs/wledambilight/contents/<pfad> -q .content | base64 -d`) –
  dort ist auch gelöst, dass macOS/Windows eine laufende Ereignisschleife brauchen und Wayland
  Fenster nicht verstecken kann (`MINIMIEREN`).
- Autostart: crosspoint `crates/core/src/autostart.rs` (Linux-Teil).
- Abhängigkeiten wie dort: `tray-icon` (macOS/Windows), `ksni` (Linux). Für den Windows-Registry-Eintrag
  `winreg` (nur `cfg(windows)`). Keine weiteren.

## Tray-Symbol

Es gibt noch kein Logo (kommt vom Nutzer). Bis dahin: Lucide-Icon `package` als Symbol, mit
`resvg` (steckt über `egui_extras` schon im Baum) in 32 und 64 px gerastert. Auf macOS als
Template-Bild (passt sich hell/dunkel an). Später nur Bilddatei tauschen.

## Datenorientiert

- Zustand des Trays (Text, Häkchen) wird aus dem `Modell` abgeleitet – reine Funktion in `logik.rs`
  mit Test (z. B. `tray_zeile(&Modell) -> String`).
- Autostart als eigenes Modul `autostart.rs` mit `fn aktiv() -> bool`, `fn setzen(bool)`;
  Tests für Linux-Datei und macOS-plist-Inhalt (Inhalt erzeugen ist eine reine Funktion,
  Schreiben in `QUIVER_AUTOSTART_DIR`).

## Abnahme

- Prüfbefehl grün, neue Unit-Tests für `tray_zeile`, Autostart-Inhalte (plist, .desktop), Argument `--hintergrund`.
- Von Hand auf macOS (dieser Rechner): `cargo run --release -p quiver-fenster`, Fenster schließen →
  Tray bleibt, `curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8741/mcp` antwortet weiter (401);
  Tray „Fenster öffnen“ → Fenster da; zweiter Start → kein zweiter Prozess.
  Beschreibe in deiner Antwort, was du von Hand geprüft hast und was nicht.
- Autostart: Schalter an → plist existiert mit `--hintergrund`; aus → plist weg.
  **Teste das nur mit `QUIVER_AUTOSTART_DIR` auf einen temporären Ordner**, nicht im echten
  `~/Library/LaunchAgents`.
