# Sicherheit

## Eine Lücke melden

**Nicht als öffentliches Issue.** Bitte über den Reiter **Security** dieses
Repositories → **„Report a vulnerability“** (GitHub Private Vulnerability
Reporting). Der Bericht ist dann nur für die Maintainer sichtbar, und wir können
gemeinsam einen Termin für die Veröffentlichung festlegen.

Hilfreich im Bericht:

* was ein Angreifer erreicht (an Zugangsdaten kommen, Tools eines anderen
  Geräts aufrufen, sich als Gerät oder Administrator ausgeben, den Dienst zum
  Stehen bringen),
* die Version (Tag oder Paketname, z. B. `0.1.0`) und das System,
* Schritte zum Nachstellen, wenn es geht mit Protokoll (`RUST_LOG=debug`).
  **Keine** Tresor-Dateien, Passwörter, Entra-Geheimnisse oder echten
  Tool-Aufrufe.

Wir antworten innerhalb von 7 Tagen. Ist die Lücke bestätigt, gibt es einen Fix
und — je nach Schwere — ein Release; im CHANGELOG steht dann, was war, ohne
dass eine offene Lücke beschrieben wird, bevor der Fix draußen ist.

## Unterstützte Versionen

Sicherheitsfixes bekommt die jeweils **letzte** veröffentlichte Version
(Stand `0.1.x`). Ältere Stände nicht: ein Update ist ein Pakettausch oder ein
neues Container-Abbild, und eine Lücke wird nicht rückportiert.

## Was als Lücke gilt

Alles, was eine der Zusagen aus der README bricht:

* **Zugangsdaten** — ein Tool-Passwort oder ein Microsoft-Token erreicht den
  Server, einen Agenten oder eine Datei außerhalb des Profils; die
  Tresor-Datei steht im Klartext; das Protokoll gibt ein Geheimnis preis.
* **Anmeldung** — ein Gerät oder eine Verwaltungssitzung ohne gültigen Beweis;
  ein Einmal-Code, der mehr als einmal gilt; ein CSRF- oder `Host`-Fehler, der
  eine ändernde Anfrage durchlässt; ein Agent, der die Geräteanmeldung
  auslösen kann.
* **Rechte** — ein Gerät bekommt Tools, die seine Gruppen nicht erlauben; ein
  Benutzer wird Admin, ohne dass der Root es erlaubt; ein Agent schaltet
  Einträge ab, meldet sich an oder ändert Angaben.
* **Container** — ein Weg aus dem Abbild heraus, ein Schreibzugriff auf das
  Wurzeldateisystem, ein Aufstieg über Capabilities oder `setuid`.
* **Verwaltung** — eine CSP-Umgehung, ein XSS, ein Pfad außerhalb der
  Basis-URL, der eine Sitzung übernimmt.

Nichts davon ist eine Lücke: der Betreiber sieht Metadaten (Bibliothek,
Zuweisung, Geräte), wer den Server umbaut, kann Einträge erfinden, und wer das
Benutzerkonto hat, kann die Schlüsseldatei des Tresors lesen — das steht offen
in [ADR 0001](docs/adr/0001-tresor-datei-statt-schluesselbund.md) und in der
README unter „Sicherheit und Daten“. Eine fehlende Signatur der Pakete ist
ebenfalls keine Lücke, sondern ein fehlendes Zertifikat
([docs/release.de.md](docs/release.de.md#was-unsigniert-bedeutet)).

## Was wir schon wissen

Die offenen Punkte stehen in [`docs/offen.md`](docs/offen.md) und in
[`docs/adr/`](docs/adr/). Ein Blick dorthin spart einen Bericht.
