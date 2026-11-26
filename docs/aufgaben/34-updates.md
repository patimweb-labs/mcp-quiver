# 34 – Neue Versionen anzeigen, Verteilung über Intune

quiver läuft für sich: keine fremden Werkzeuge, keine Signaturschlüssel, kein Selbst-Installieren.
Installiert, aktualisiert und entfernt wird durch die Geräteverwaltung (Intune) oder von Hand mit den
Paketen aus dem Release. Die Konfiguration des Benutzers bleibt bei jedem Update erhalten.

## 1. Anzeigen

- Server fragt die Releases höchstens alle 6 h ab (`QUIVER_UPDATES=an|aus`, Standard an, ohne Kennung, ohne
  Nutzdaten) und zeigt in der Verwaltung: neuere Server-Version, neueste Client-Version, welche Geräte
  ältere Clients haben.
- Der Client fragt nur den Server. Ist er älter, zeigt er einen ruhigen Hinweis in Fenster und Tray mit
  Text aus der Vorgabe (Standard: „Version X ist verfügbar“; mit Verwaltung: „kommt über die
  Geräteverwaltung“) und Link auf die Änderungen. Kein Herunterladen, kein Installieren.

## 2. Pakete für Intune

- Windows: MSI (je Benutzer, wie heute) muss still laufen (`msiexec /i … /qn`), Upgrade ersetzt die alte
  Version, beendet einen laufenden Client vorher geordnet (RestartManager) und startet ihn danach wieder;
  Deinstallation lässt das Profil stehen. Geeignet als „Branchen-App (MSI)“ in Intune, ohne weiteres Werkzeug.
- macOS: zusätzlich `.pkg` (pkgbuild/productbuild, Bordmittel), installiert nach /Applications, ersetzt die
  alte Version, beendet den laufenden Client vorher und startet ihn danach wieder. `.dmg` bleibt.
- Linux bleibt wie es ist.

## 3. Vorgaben der Firma

Nur lesbar für den Benutzer, gesetzt durch die Verwaltung; Vorrang vor dem Profil:
- Windows: `HKLM\Software\Policies\mcp-quiver`
- macOS: verwaltete Einstellungen für `de.weppelmann.mcp-quiver` (nur der verwaltete Bereich)
- Linux: `/etc/mcp-quiver/vorgaben.json` (nur root-eigen und nicht weltschreibbar)
Felder: `ServerUrl` (dann fest, im Fenster nicht änderbar, Hinweis „von deiner Firma vorgegeben“),
`Autostart` (an/aus), `UpdateHinweis` (Text). Ungültige Werte: ignorieren, im Protokoll nennen. Ohne
Vorgaben alles wie bisher.
- Vorlagen mitliefern: `.mobileconfig` (macOS) und ADMX/ADML bzw. Registry-Vorlage (Windows), mit
  Platzhalter-Adresse.

## Prüfen

Tests für die Anzeige, Vorgaben (je Plattform), stille Installation und Upgrade so weit die CI es
erlaubt (Windows-Läufer: `msiexec /qn` installieren, Version 2 darüber, deinstallieren, Profil noch da;
macOS-Läufer: `installer -pkg`). README/SECURITY: die Versionsabfrage des Servers erwähnen.
`docs/intune.md` und `docs/intune.de.md`: Schritt für Schritt für Windows und macOS, kurz.

**Nicht committen, nicht pushen.**
