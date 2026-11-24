# 24 – Rollen: Root, Admin, Mitarbeiter

Nach 23. Die Rechte stehen **am Server**, die Verwaltung zeigt nur, was erlaubt ist.

## Modell

`enum Recht` (Summentyp) und feste Rollen als Menge von Rechten, in `domain`, rein und getestet. Prüfung über **eine** Stelle je Route (`verlangt(Recht::…)`); eine Route ohne Angabe ist gesperrt (Test geht über die gesamte Routentabelle).

| Recht | Root | Admin | Mitarbeiter |
|---|---|---|---|
| eigenes Profil, eigene Gruppen, eigene zugewiesene Tools (Titel, Beschreibung) sehen | – | ja | ja |
| eigene Geräte sehen, umbenennen, abmelden | – | ja | ja |
| Bibliothek, Sätze, Gruppen, alle Geräte sehen und ändern | ja | ja | nein |
| Benutzer sehen, sperren, jetzt prüfen, löschen | ja | ja | nein |
| Rollen vergeben und entziehen | ja | ja | nein |
| Prüfspur sehen | ja | ja | nein |

Mitarbeiter sehen **nie**: Befehle, Umgebung, Werte und Standardwerte der Einträge, andere Benutzer, Geräte und Gruppen. Eigene Antworttypen (`MeinProfil`, `MeinTool`, `MeinGeraet`), kein Weglassen von Feldern aus den Admin-Typen. Fremde Ids antworten wie unbekannte (404).

## Vergabe

- Rolle je Benutzer **und** je Entra-Gruppe (`Gruppe.rolle: Option<Rolle>`); es gilt die höchste. Standard: Mitarbeiter, wenn mindestens eine Entra-Gruppe im Token steht, sonst kein Zugang (verständliche Seite).
- Die Wirkung folgt dem nächsten Nachprüfen bzw. sofort bei einer Änderung in quiver; bei jeder Anfrage frisch aus der DB (kein Recht in der Sitzung).
- Schutz: Root ist nicht löschbar oder herabstufbar. Von selbst angelegte Gruppen (23) haben **nie** eine Rolle. Eine Rolle je Gruppe verlangt eine Rückfrage mit der Zahl der betroffenen Benutzer. Gesperrte haben kein Recht, auch als Admin. Sich selbst die Adminrolle entziehen nur mit Rückfrage.
- Prüfspur: wer wem welche Rolle, auf welchem Weg (Benutzer oder Gruppe).
- Unterbefehl `admin-setzen --oid` bleibt.

## Verwaltung (zusammen mit 21)

Nach der Anmeldung je Rolle: Mitarbeiter sehen nur **Übersicht** (Profil, Gruppen, Tools) und **Meine Geräte**; die übrigen Menüpunkte gibt es nicht, direkte Adressen zeigen „kein Zugriff“. Root und Admin sehen alles. Rolle im Kopf. Rollen in den Dialogen von Benutzer und Gruppe; die Herkunft steht dabei („Admin über Gruppe X“).

## Tests

Matrix Rolle × jede Route (auch Unbekannt, gesperrt, abgelaufen), automatisch aus der Routentabelle. Höchste Rolle gilt. Entzug wirkt in der laufenden Sitzung. Mitarbeiter: fremdes Gerät lesen, umbenennen, löschen → 404 ohne Wirkung; Suchtest, dass kein Befehl, keine Umgebung und kein Wert in irgendeiner Mitarbeiter-Antwort steht. Selbsterhöhung unmöglich (eigene Rolle, eigene Gruppe, Massenzuweisung über fremde Felder). Vue: Menü je Rolle, direkte Adresse gesperrt.
