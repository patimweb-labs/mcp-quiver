# 34 – Updates erkennen und installieren

Client und Verwaltung erkennen neue Versionen. Die Verwaltung zeigt sie an, der Client kann sich selbst
aktualisieren: neue Version installieren, alte entfernen, **Konfiguration behalten**.

## Grundsätze

- **Keine Abfrage ohne Wissen des Betreibers.** Der Server fragt die Releases (GitHub) ab, Schalter
  `QUIVER_UPDATES=an|aus` (Standard an), höchstens alle 6 Stunden mit Streuung, ohne Kennung, ohne
  Nutzdaten. Der Client fragt **nur den Server**, nie das Internet. README/SECURITY anpassen („kein Ruf nach
  Hause“ stimmt sonst nicht mehr).
- **Installiert wird nur signiert.** Jede Datei eines Releases hat eine Ed25519-Signatur (minisign-Format)
  über Dateiname, Version, Plattform und Prüfsumme. Der öffentliche Schlüssel steckt im Client. Ohne oder
  mit falscher Signatur wird nichts installiert. Der private Schlüssel liegt nur als GitHub-Secret
  `UPDATE_SIGNATUR_SCHLUESSEL` vor; ihn erzeugt und hinterlegt der Betreiber (Anleitung mitliefern).
- **Kein Rückschritt:** nur höhere Versionen; Verkettung mit signierter Liste (`updates.json`).
- Der Download kommt vom Server als Spiegel (geprüft gegen Signatur) oder direkt vom Release; die
  Signatur entscheidet, nicht die Quelle. Feste Größengrenzen, Ablage nur im eigenen Ordner (0700).

## Verwaltung

Hinweis, wenn der Server älter ist (Version, Link auf Änderungen, Anleitung zum Aktualisieren des
Containers; der Server tauscht sich nicht selbst aus). Liste der Geräte zeigt die Client-Version, veraltete
sind markiert; Admin kann je Gruppe festlegen: nur anzeigen / automatisch installieren / Mindestversion.

## Client

- Hinweis im Fenster und im Tray („Version X verfügbar“), Knopf „Jetzt installieren“, automatisches
  Installieren nach Vorgabe, aber nie während ein Tool-Aufruf läuft; Aufschieben möglich.
- Ablauf: herunterladen → Signatur prüfen → **Profil sichern** → alte Instanz geordnet beenden →
  ersetzen → neue starten → Gesundheit (startet, MCP-Endpunkt antwortet) → alte Version entfernen.
  Klappt der Start nicht: alte Version wieder einsetzen, Meldung, nicht erneut versuchen.
- Je Paketart: macOS `.app` (atomar tauschen, Signatur/Quarantäne beachten), Windows MSI je Benutzer
  (Upgrade, alte Version wird entfernt) und portables ZIP, Linux AppImage/`.tar.gz`; `.deb`/`.rpm` brauchen
  Adminrechte und werden **nur angezeigt**. Nicht beschreibbare Orte: nur anzeigen.
- Konfiguration, Tresor, Protokoll und Anmeldung bleiben; Profil-Migrationen sind abwärts verträglich
  oder werden gesichert.

## Prüfen

Attrappe-Release (Server und Datei lokal), Fälle: neuer, gleich, älter, falsche Signatur, vertauschte
Plattform, abgeschnittene Datei, voller Datenträger, Start der neuen Version scheitert → Rückfall,
Aufruf läuft gerade, Nutzer schiebt auf, Konfiguration danach unverändert. Auf allen drei Systemen in der CI.

Nicht committen, nicht pushen, keine Schlüssel erzeugen oder in Dateien schreiben.
