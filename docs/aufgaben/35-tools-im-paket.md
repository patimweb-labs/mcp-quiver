# 35 – Die Tools gehören zum Client-Paket

Ein Client aus dem Paket muss für sich funktionieren, auf macOS, Windows und Linux, ohne Pfade eines
Entwicklerrechners.

## 1. Suchen

- Ein Eintrag nennt nur den **Programmnamen** (`quiver-devops`). Der Client sucht bei einem Namen ohne
  Trennzeichen zuerst im mitgelieferten Ordner neben dem eigenen Programm (macOS
  `quiver.app/Contents/Resources/tools`, Windows `<Installation>\tools`, Linux `lib/mcp-quiver/tools` bzw.
  im AppImage), dann im `PATH`. Windows mit `.exe`. `QUIVER_TOOLS_DIR` überschreibt (Entwicklung).
  Kein Ausbruch aus dem Ordner, keine Verweise nach außen. Gilt für Start, Anmeldung, Status, Einrichtung,
  Testen gleichermaßen (eine Stelle).
- Absolute Pfade funktionieren weiter. Zeigt ein absoluter Pfad auf ein Programm, das es hier nicht gibt, heißt
  aber dessen Dateiname wie ein mitgeliefertes Tool, wird das mitgelieferte genommen und im Protokoll
  vermerkt (Altbestand der Server heilt so von selbst).
- Meldungen an den Benutzer nennen nie einen fremden Dateipfad, nur Tool-Name und Grund; der volle Pfad nur
  im lokalen Protokoll.

## 2. Pakete

- release.yml baut die Tools aus `patimweb-labs/quiver-tools` auf demselben Läufer mit und legt sie ins
  Paket. Der Stand ist festgelegt (Commit in einer Datei im Repo, nicht „main“), Version steht im Release-Text.
  Alle `quiver-*`-Programme aus quiver-tools, gestrippt; Paketgröße nennen.
- Signieren (macOS ad hoc) schließt die Tools ein.

## 3. Verwaltung

- Beim Speichern eines Eintrags mit absolutem Pfad (`/`, `C:\`) zeigt die Verwaltung eine Warnung, dass das nur
  auf Geräten mit genau diesem Pfad funktioniert. Erlaubt bleibt es.

## 4. Darstellung im Client

Lange Fehlertexte auf der Tool-Karte brechen um, statt Karten breiter zu machen oder unter Knöpfe zu laufen;
alle Karten gleich breit, Knöpfe überdecken nie Text (Bild vom Nutzer: Karten verschieden breit, Text unter
dem Knopf „…“). Test mit sehr langem Text in drei Breiten.

## Prüfen

Tests für die Suche je Plattform, Altbestand, Meldungen, Karten. Pakete lokal (macOS) bauen und den Client
aus dem Bündel mit verlegtem Profil starten: Tool aus dem Bündel antwortet. fmt, clippy, `cargo test --workspace`,
admin-Tests, actionlint. **Nicht committen, nicht pushen.**

## 5. Anmeldung eines Tools (Microsoft 365, ZEP) – Nachtrag vom Nutzer

Auf einem Windows-Gerät stand dort nur ein Knopf „↻ …“; niemand weiß, was er tut, und die Anmeldung geht nicht.

- Knöpfe tragen immer Text: „Anmelden“, „Abmelden“, „Erneut prüfen“. Während einer Prüfung steht „Prüfe Anmeldung …“
  mit Drehsymbol und ist nicht bedienbar, mit Frist (10 s); danach ein klarer Satz und ein benannter Knopf. Kein Knopf
  besteht je nur aus „…“, auch nicht in schmalen Fenstern.
- Fehlt das Programm: „Dieses Tool ist auf diesem Gerät nicht installiert.“ statt eines technischen Fehlers, kein
  Anmelde-Knopf, nichts doppelt (Fehler stand zweimal auf der Karte). Hinweis an den Admin im Protokoll.
- Nach dem Anmelden: Name/Konto sichtbar („Angemeldet als …“), klarer Zustand bei „nicht angemeldet“, „läuft ab“,
  „fehlgeschlagen“ mit dem Grund in einem Satz (deutsch und englisch).
- Anmelden öffnet auf Windows den Standardbrowser (ohne Konsolenfenster; Kindprozesse ohne Fenster starten:
  CREATE_NO_WINDOW). Der Rückweg auf Loopback funktioniert hinter der Windows-Firewall (nur 127.0.0.1 binden).
- Verwaltung und Client gleiche Texte. Tests für jeden Zustand, Bilder für beide Themen.
- **Nichts doppelt auf der Karte:** „Braucht Angaben“ und der Stift öffnen heute dasselbe. Fehlen Pflichtangaben, gibt es
  nur den Knopf „Angaben ergänzen“ (kein Stift). Sind sie da, nur den Stift, mit Text/Tooltip „Bearbeiten“. Bei Tools mit
  Zugangs-Liste (Organisationen, Systeme, Hosts) führt „… hinzufügen“ zur Liste und der Stift entfällt, wenn es sonst
  nichts zu bearbeiten gibt. Jede Karte einmal durchgehen: jeder Knopf hat eine eigene, benannte Wirkung. Verwaltung
  entsprechend prüfen. Kein doppelter Fehlertext, kein „Fehlt: …“ zusätzlich zu „Braucht Angaben: …“ (eine Zeile).

## 6. Portabel

ZIP (Windows) und `.tar.gz` (Linux) enthalten `quiver` und daneben den Ordner mit den Tools. Der Client findet ihn immer **neben
seiner eigenen Datei**, egal in welchem Ordner das entpackt wurde, welches Arbeitsverzeichnis gilt und ob er über eine
Verknüpfung oder den Autostart gestartet wird (Pfad der eigenen Datei, Verweise aufgelöst). Auch Pfade mit Leerzeichen und
Umlauten. Fehlt der Ordner, sagt das Fenster: „Der Ordner ‚tools‘ neben quiver fehlt. Bitte das ganze ZIP entpacken.“ Test: Paket in einen
Ordner mit Leerzeichen und Umlaut entpacken, aus einem anderen Verzeichnis starten, ein Tool antwortet.
