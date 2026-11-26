Englisch: [updates.md](updates.md)

# Updates: wer fragt, wer zeigt, wer installiert

Der Server erfährt, dass eine neuere Fassung veröffentlicht ist, und zeigt es in
der Verwaltung. Der Client erfährt es **von seinem Server** und bietet das
Update an: „Jetzt aktualisieren“ oder „Später“. Der Client fragt nie das
Internet. Wer keine Geräteverwaltung hat, kann weiter von Hand aus den Paketen
des Releases aktualisieren; den Entwurf begründet
[ADR 0005](adr/0005-client-update.md), Schritt für Schritt für Intune steht in
[intune.de.md](intune.de.md).

## Auf einen Blick

| Stück | Wo |
| --- | --- |
| Server fragt | GitHub-Release des Repositories, höchstens alle 6 Stunden, Streuung bis 30 min |
| Schalter | `QUIVER_UPDATES=an\|aus` (Vorgabe `an`) — schaltet die Abfrage **und** die Paketauslieferung |
| Kennung / Nutzdaten | keine — ein anonymer GET, sonst nichts |
| Gemerkt in | Tabelle `update_stand` (Schema 6), damit ein Neustart nicht sofort wieder fragt |
| Server legt ab | das Paket unter `<daten>/updates/<version>/`, geprüft gegen `SHA256SUMS` |
| Client fragt | nur seinen Server (`GET /api/v1/geraet/updates`, `…/updates/paket/{name}`) |
| Client zeigt | Fenster und Tray: Version, Text des Betreibers, Link auf die Änderungen, „Jetzt aktualisieren“ / „Später“ |
| Client installiert | sich selbst — nach Prüfsumme, Größe und Selbsttest (siehe unten) |
| Verwaltung zeigt | `GET /api/v1/admin/updates`: neueste Client-Version, neuere Server-Version, Geräte mit älterem Client |
| Geräteverwaltung | MSI oder `.pkg` über Intune — [intune.de.md](intune.de.md) |

## Was der Server fragt

`GET https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest` —
ohne Kennung, mit `User-Agent: quiver`. Gelesen wird aus der Antwort:

* `tag_name` → die Version (`v0.3.0` wird `0.3.0`),
* `html_url` → der Link auf die Änderungen,
* `published_at` → nur zur Anzeige,
* `assets` → Dateinamen, Größen und Adressen der Pakete.

Ein Entwurf oder eine Vorabfassung ist **kein** Angebot: er wird abgelehnt.
Werte, die nicht passen, werden verworfen und im Protokoll genannt; eine
gescheiterte Frage lässt den letzten bekannten Stand stehen und zeigt den Grund
in der Verwaltung.

Ob eine Version „neu“ ist, entscheidet ein Versionsvergleich, nie die
Reihenfolge der Antworten: `0.3.0` ist neuer als `0.2.9`, `1.0.0-rc.1` ist
älter als `1.0.0`. Eine unlesbare Version ist nie neuer — damit kann keine
Antwort einen Rückschritt-Hinweis auslösen.

Die Quelle ist im Betrieb fest auf das Repository des Projekts gestellt; Tests
und Entwicklung dürfen `QUIVER_UPDATE_QUELLE` auf eine Loopback-Adresse zeigen
lassen.

## Was der Client zeigt

Der Client fragt seinen Server höchstens alle sechs Stunden (Streuung 30
Minuten) und bekommt die neueste veröffentlichte Version, den Link und die
Pakete für sein System. Ist die Version neuer als der laufende Client,
erscheint über der Seite und in der Tray-Zeile ein Hinweis:

* ohne Geräteverwaltung: „Version X ist verfügbar“,
* mit Geräteverwaltung (irgendeine Vorgabe liegt vor): „Version X kommt über die
  Geräteverwaltung“,
* überschreibbar durch den Text `UpdateHinweis` des Betreibers.

Gibt es ein Paket für **diese** Installation, stehen daneben zwei Knöpfe:

* **Jetzt aktualisieren** — laden, prüfen, Selbsttest, tauschen (siehe unten),
* **Später** — der Hinweis verschwindet für diese Version bis zum nächsten
  Start.

Sonst steht nur der Grund da: vom Betreiber abgeschaltet, Ordner gehört dem
System, `deb`/`rpm` brauchen root, oder kein Paket für diese Installation.
Daneben steht immer ein Link auf die Änderungen; er öffnet den Systembrowser.

## Wie der Client sich selbst tauscht

1. **Werkzeuge beenden.** Laufende Aufrufe bekommen bis zu 10 Sekunden, dann
   werden die Einträge gestoppt. Mitten in einem Aufruf wird nicht getauscht.
2. **Laden** vom eigenen Server (Geräte-Token) nach
   `<profil>/updates/<version>/`. Der Client prüft `Content-Length` und die
   SHA-256 aus der Antwort des Servers schon beim Schreiben; was nicht passt,
   wird gelöscht.
3. **Prüfen und Selbsttest.** Entpacken (ZIP/`tar.gz` mit `flate2`, `.dmg` mit
   `hdiutil`/`ditto`), dann die neue Fassung als `quiver --selbsttest`
   starten: Exit 0, ohne Fenster, ohne Profil. Scheitert das, bleibt alles, wie
   es ist.
4. **Tauschen** je Art der Installation (Tabelle in
   [ADR 0005](adr/0005-client-update.md)): die alte Datei wird zu
   `<name>.alt`, die neue daneben gelegt, eine Marke ins Profil geschrieben.
5. **Neustart.** Das Fenster sagt „Neue Version X ist eingespielt“, schließt,
   und erst nachdem das Profil freigegeben ist, startet die neue Fassung. Beim
   zweiten Start ohne Beweis kommt die alte Fassung zurück (`<name>.alt`).

Das **Profil wird nie angefasst**: Einstellungen, Tresor, Geräte-Token,
Angaben, Zugänge, Anmeldungen der Werkzeuge, Autostart und die geladenen
Programme bleiben, wie sie waren. Ein Test vergleicht das Profil vor und nach
dem Tausch.

## Was die Verwaltung zeigt

Die Übersicht zeigt die neueste veröffentlichte Client-Version, wann zuletzt
gefragt wurde, ob der **Server** selbst älter ist, und welche Geräte einen
älteren Client melden. Die Geräteseite markiert diese Geräte neben ihrer
Client-Version.

Der Server aktualisiert sich nicht selbst; ist er älter, wird er neu gebaut und
ausgerollt (Container-Abbild) — siehe [betrieb.de.md](betrieb.de.md).

## Wenn etwas schiefgeht

| Fall | Verhalten |
| --- | --- |
| GitHub nicht erreichbar | letzter bekannter Stand bleibt, der Fehler steht in der Verwaltung; keine Knöpfe |
| `QUIVER_UPDATES=aus` | niemand fragt und nichts wird ausgeliefert; die Verwaltung sagt es |
| noch keine Antwort | Client zeigt nichts; Verwaltung sagt „noch nicht gefragt“ |
| unlesbare Version | abgelehnt, protokolliert, nichts wird gezeigt |
| Prüfsumme falsch | der Server liefert nicht, der Client tauscht nicht; Meldung im Fenster |
| Selbsttest scheitert | nichts wird getauscht; die Meldung nennt den Grund |
| neue Fassung startet nicht | beim zweiten Start kommt `<name>.alt` zurück |

## Vorgaben

Die vier Einstellungen des Betreibers (`ServerUrl`, `Autostart`,
`UpdateHinweis`, `UpdateAngebot`) werden **nur** aus dem verwalteten Bereich der
Plattform gelesen, nie von quiver geschrieben: `HKLM\Software\Policies\mcp-quiver`
unter Windows, die verwalteten Einstellungen für `de.weppelmann.mcp-quiver`
unter macOS, `/etc/mcp-quiver/vorgaben.json` unter Linux (root-eigen, nicht
welt-schreibbar). `UpdateAngebot=false` schaltet das Angebot ab; die Vorgabe
ist an. Vorlagen mit Platzhalter-Adresse liegen in `verteilung/vorgaben/`; das
Ausrollen steht in [intune.de.md](intune.de.md).
