# 0003 – Grenzen im Client

Stand: 2026-12-01 · gilt für `quiver-client` und das Client-Fenster

## Kontext

Der Client führt fremden Code aus: Werkzeuge aus der Bibliothek (`npx`, `uvx`,
eigene Binaries, entfernte MCP-Server) und Aufrufe von Agenten über einen
lokalen HTTP-Endpunkt. Beides kann hängen, riesig antworten, beliebig viel auf
stderr schreiben oder den Speicher füllen. Ein Eintrag kann außerdem eine
Kennung tragen, die als Präfix der Werkzeugnamen nicht taugt.

Bisher gab es nur Punktgrenzen (Startfrist, Protokollring, Kürzen beim
Aufzeichnen). Was fehlte, war eine Grenze für den **Aufruf**, für die
**Ausgabe** und für das, was ein Agent vom Protokoll sieht.

## Entscheidung

Die Grenzen stehen an je einer Stelle und sind getestet (SPEC §8 „Grenzen im
Client“):

- **Aufruf** – Frist ohne Lebenszeichen 120 s (je Eintrag 1–900 s in
  `zustand.json`), Fortschrittsmeldungen verlängern sie, hart bei 15 min; der
  Abbruch des Agenten bricht auch den Aufruf beim Tool ab (`notifications/cancelled`).
- **Auslastung** – 32 gleichzeitig, 8 je Eintrag, darüber sofort „ausgelastet“.
- **Größen** – Antwort 8 MB (ohne Kopie gezählt), Nachricht über stdio 16 MB
  (darüber Neustart mit Pause), Ereigniszeile 64 KB, Netzantwort 4 MB (Soll
  8 MB).
- **Prozessausgaben** – Zeile mit 8-KB-Puffer, Protokollzeile 2000 Zeichen,
  200 Zeilen je Minute und Eintrag auf die Platte, Tagesdatei 10 MB, alles
  **vor** dem Schreiben geprüft.
- **Werkzeugliste** – 500 Werkzeuge, Name ≤ 128 aus `[A-Za-z0-9_.-]`,
  Beschreibung ≤ 4000, Schema ≤ 64 KB, ohne Steuer- und Richtungszeichen;
  `list_changed` höchstens einmal je Sekunde.
- **Zuordnung** – Werkzeugname → (Eintrag, Werkzeug) über eine **Tabelle** aus
  den laufenden Einträgen, nicht über eine Zerlegung am `__`. Doppelte Namen
  werden verworfen (Log). Kennungen: `[a-z0-9-]{1,40}`, kein `__`, nicht
  `quiver`; unbrauchbare Einträge starten nicht und werden im Fenster benannt.
- **Herkunft** – Anfragen mit `Origin` nur von Loopback, `Host` nur der eigene
  Port; ohne `Origin` erlaubt (CLI-Agent). Keine Weiterleitungen im HTTP-Client:
  ein Geräte-Token gehört zu genau einer Adresse.
- **Sitzungen** – höchstens 64, Ablauf nach 10 Minuten ohne Nutzung; „verbunden“
  im Fenster folgt daraus.
- **Tresor und Profil** – fehlt bei vorhandenem Tresor der Schlüssel, entsteht
  **kein** neuer; ein kaputter Tresor verhindert den Start nicht (Fenster zeigt
  „Tresor nicht lesbar“, „Neu beginnen“ mit Rückfrage, alte Dateien bleiben als
  `.kaputt-<zeit>`). Der gespeicherte Stand wird mit PID und Zufall
  zwischengespeichert und mit `fsync` für Datei und Ordner geschrieben; eine
  Sperrdatei hält einen zweiten Kern vom selben Profil fern.
- **Umgebung** – ein Werkzeug erbt nicht die Umgebung des Clients, sondern eine
  Whitelist (PATH, HOME, USER, LANG, `LC_*`, TZ, TMPDIR, Proxy, Plattform-
  nötiges) plus die Werte des Eintrags. `QUIVER_*` gehört **nicht** dazu.
- **Blockierendes** – der Tresor wird einmal je Abgleich gelesen
  (`spawn_blocking`), die Dateisperre hat eine Frist von 5 s.

## Preis

- Ein Werkzeug, das länger als 15 Minuten arbeitet, wird abgebrochen; es muss
  seine Arbeit in Stücke teilen oder Fortschritt melden.
- Ein Werkzeug, das keine Werkzeugliste liefert, die diesen Regeln entspricht,
  verliert einzelne Werkzeuge (mit Log-Zeile) statt zu scheitern.
- Zwei Kerne auf demselben Profil sind nicht mehr möglich – für einen Neustart
  muss der alte Kern beendet sein (er gibt die Sperre mit `beenden` frei).
- Die zusätzliche Nachfrage alle 5 Minuten kostet eine Anfrage je Gerät und
  Intervall.

## Bewusste Grenze

Ein Tool läuft mit dem Konto des Benutzers und könnte den Tresor lesen. Das
kann der Client nicht verhindern. Schutz sind geprüfte, signierte Pakete und die
Freigabe durch den Admin (SPEC §4).
