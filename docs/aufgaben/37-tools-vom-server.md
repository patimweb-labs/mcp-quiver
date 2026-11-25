# 37 – Der Server liefert die Tools

Der Client bekommt bei der Anmeldung die Tools, die **auf dem Server liegen, konfiguriert sind und für seine Gruppen gelten**.
Er bringt selbst keine mit. Ordner neben dem Programm und Tools im Client-Paket (Aufgabe 35) entfallen.

## Verhalten

- **Anmeldung, Start und jeder Abgleich:** Soll gegen Ist. Fehlt ein Tool oder ist seine Prüfsumme anders, holt der Client es vom
  Server, prüft es und startet es neu. Tools sind damit immer auf dem Stand des Servers.
- **Nicht mehr zugewiesen, aus der Gruppe, gesperrt, kein Zugriff:** Tool stoppen und **Programm löschen. Die Konfiguration
  (Angaben, Zugänge, Geheimnisse, Anmeldung des Tools) bleibt** und gilt wieder, sobald es zurückkommt. Das betrifft auch
  verbliebene Dateien älterer Versionen. Abmelden am Gerät: Tools stoppen, Programme löschen, Konfiguration bleibt.
- Nur die zugewiesenen Tools, nur für das eigene System (Betriebssystem und Architektur). Gibt es ein Tool für dieses System nicht:
  „Für dieses System gibt es das Tool nicht.“, kein Fehler.
- Fortschritt je Karte: „Wird vom Server geladen …“, „Aktualisiert“. Ist der Server nicht erreichbar, laufen die bereits geladenen
  Tools nach den bisherigen Regeln weiter (Frist wie heute).

## Server

- Das Server-Abbild enthält die Programme je Plattform (Windows x64, macOS arm64 und x64, Linux x64 und arm64), komprimiert, aus dem
  festgelegten Release von quiver-tools (Aufgabe 38), geprüft gegen dessen Prüfsummen. Zusätzlich kann der Betreiber weitere Programme
  in `<daten>/tools/<system>/` legen (nur root-Admin am Server, nicht über die Weboberfläche).
- `GET /geraet/tools/{name}?system=…&arch=…` (nur mit Geräte-Token, nur Tools aus dem Soll dieses Geräts), gestreamt, Größengrenze,
  Drossel, ETag. Der Soll-Eintrag nennt je Tool Version, Größe, SHA-256 für das System des Geräts (der Client meldet System und
  Architektur). Änderung der Programme ändert die Revision.
- Verwaltung: je Eintrag, welches Programm, welche Version, für welche Systeme vorhanden; je Gerät, welche Tools in welcher Version laufen.
- Zugriff nur über die Prüfung, die auch für das Soll gilt (Entzug wirkt sofort).

## Client

- Ablage im Profil `tools/<name>/<sha256>/<programm>` (Ordner 0700, Datei 0700; Windows nur Benutzer), Laden in eine Zwischendatei,
  Größe und SHA-256 prüfen, fsync, umbenennen; erst danach ausführbar machen. Vor **jedem** Start erneut prüfen (gleiche Datei, kein
  Verweis, Prüfsumme); stimmt sie nicht, neu laden. Alte Fassungen nach erfolgreichem Start löschen. Windows: laufende Programme
  zuerst beenden, dann ersetzen. Kein Pfad vom Server wird je als Pfad benutzt, nur der Name nach strenger Prüfung.
- TLS wie beim Server (nie ohne, außer Loopback), keine Weiterleitungen.
- Einträge mit altem festem Pfad: es zählt nur der Programmname (Altbestand heilt sich). Einträge mit fremden Programmen
  (`npx`, `uvx`, `docker`, Pfade zu eigenen Programmen) funktionieren unverändert.
- `QUIVER_TOOLS_DIR` bleibt für die Entwicklung.

## Wegfall

Aus `verteilung/`, `release.yml`, `tools.rs`, Texten und Tests alles zum Ordner neben dem Programm und zu Tools im Paket entfernen. Behalten:
Karten, Anmeldung, Meldungen ohne Pfade, Tresor.

## Prüfen

Ende zu Ende mit echtem Server und Attrappen-Programmen: Anmeldung lädt, Änderung aktualisiert, Entzug löscht Programm und behält
Konfiguration, zurück in die Gruppe funktioniert ohne neue Angaben, falsche Prüfsumme, abgeschnitten, manipulierte Datei auf der Platte,
nicht zugewiesenes Tool abrufen (403), fremdes Gerät, Entzug während des Ladens, Platte voll, Windows-Sperre, zwei Abgleiche zugleich.
README: „automatisch installiert, aktualisiert und entfernt“ muss danach stimmen. fmt, clippy, `cargo test --workspace`, admin-Tests.
**Nicht committen, nicht pushen.**

## Anzeige beim Laden (Nachtrag)

- Nach Start und Anmeldung, solange abgeglichen oder geladen wird: ein Spinner mit Text am Kopf der Tool-Seite („Verbinde mit dem
  Server …“, „Tools werden geladen (3 von 12)“, „Wird gestartet …“), am Ende kurz „Alles aktuell“ und weg. Fortschritt gesamt; je Karte
  „Wird geladen …“ (mit Anteil, wenn die Größe bekannt ist) bzw. „Wird aktualisiert …“ mit Spinner. Tray: kurzer Text.
- Die Anzeige steht **nur, wenn es etwas zu tun gibt**; wenn alles aktuell ist, erscheint beim Start nur kurz „Alles aktuell“.
  Kein Flackern unter 300 ms. Der Spinner bremst das Fenster nicht (nur bei laufender Anzeige neu zeichnen); bei „weniger Bewegung“
  des Systems statisch.
- Fehler beim Laden: klarer Satz je Karte mit Knopf „Erneut versuchen“; ein Tool, das nicht lädt, hält die anderen nicht auf.
- Knopf **„Tools aktualisieren“** auf der Tool-Seite (und im Tray): fragt sofort beim Server nach, zeigt den Spinner, ist währenddessen
  gesperrt, meldet „Alles aktuell“ bzw. „2 Tools aktualisiert“. Ohne Verbindung: klarer Hinweis. Laufende Tool-Aufrufe werden zu Ende
  geführt, bevor ein Tool getauscht wird (Frist, danach Meldung).
- Verwaltung: gleiche Wörter. Englisch und Deutsch. Tests für jeden Zustand, Bilder in beiden Themen.
