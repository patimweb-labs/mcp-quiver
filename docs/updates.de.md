Englisch: [updates.md](updates.md)

# Updates: wer fragt, wer zeigt, wer installiert

quiver installiert sich **nicht selbst**. Ein Gerät erfährt, dass eine neuere
Fassung veröffentlicht ist, und zeigt einen ruhigen Hinweis; installiert und
aktualisiert wird durch die Geräteverwaltung (Intune) oder von Hand aus den
Paketen des Releases. Diese Seite beschreibt, wer wen fragt und was die
Verwaltung zeigt. Schritt für Schritt für Intune steht in
[intune.de.md](intune.de.md); den Entwurf begründet
[ADR 0004](adr/0004-updates.md).

## Auf einen Blick

| Stück | Wo |
| --- | --- |
| Server fragt | GitHub-Release des Repositories, höchstens alle 6 Stunden, Streuung bis 30 min |
| Schalter | `QUIVER_UPDATES=an\|aus` (Vorgabe `an`) |
| Kennung / Nutzdaten | keine — ein anonymer GET, sonst nichts |
| Gemerkt in | Tabelle `update_stand` (Schema 6), damit ein Neustart nicht sofort wieder fragt |
| Client fragt | nur seinen Server (`GET /api/v1/geraet/updates`) |
| Client zeigt | Fenster und Tray: Version, Text des Betreibers, Link auf die Änderungen |
| Client installiert | nichts |
| Verwaltung zeigt | `GET /api/v1/admin/updates`: neueste Client-Version, neuere Server-Version, Geräte mit älterem Client |
| Installieren / Aktualisieren | MSI oder `.pkg` über Intune oder von Hand — [intune.de.md](intune.de.md) |

## Was der Server fragt

`GET https://api.github.com/repos/patimweb-labs/mcp-quiver/releases/latest` —
ohne Kennung, mit `User-Agent: quiver`. Gelesen wird aus der Antwort:

* `tag_name` → die Version (`v0.3.0` wird `0.3.0`),
* `html_url` → der Link auf die Änderungen,
* `published_at` → nur zur Anzeige.

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
Minuten) und bekommt die neueste veröffentlichte Version und den Link. Ist sie
neuer als der laufende Client, erscheint über der Seite und in der Tray-Zeile
ein Hinweis:

* ohne Geräteverwaltung: „Version X ist verfügbar“,
* mit Geräteverwaltung (irgendeine Vorgabe liegt vor): „Version X kommt über die
  Geräteverwaltung“,
* überschreibbar durch den Text `UpdateHinweis` des Betreibers.

Der Hinweis ist reine Anzeige: kein Herunterladen, kein Knopf, der etwas
installiert. Daneben steht ein Link auf die Änderungen; er öffnet den
Systembrowser.

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
| GitHub nicht erreichbar | letzter bekannter Stand bleibt, der Fehler steht in der Verwaltung |
| `QUIVER_UPDATES=aus` | niemand fragt; die Verwaltung sagt es |
| noch keine Antwort | Client zeigt nichts; Verwaltung sagt „noch nicht gefragt“ |
| unlesbare Version | abgelehnt, protokolliert, nichts wird gezeigt |

## Vorgaben

Die drei Einstellungen des Betreibers (`ServerUrl`, `Autostart`,
`UpdateHinweis`) werden **nur** aus dem verwalteten Bereich der Plattform
gelesen, nie von quiver geschrieben: `HKLM\Software\Policies\mcp-quiver` unter
Windows, die verwalteten Einstellungen für `de.weppelmann.mcp-quiver` unter
macOS, `/etc/mcp-quiver/vorgaben.json` unter Linux (root-eigen, nicht
welt-schreibbar). Vorlagen mit Platzhalter-Adresse liegen in
`verteilung/vorgaben/`; das Ausrollen steht in [intune.de.md](intune.de.md).
