# 33 – Benutzer und Gruppen von Hand zuordnen, Auswahllisten im Dialog

## 1. Auswahllisten ragen aus dem Dialog

Öffnet sich eine Auswahlliste in einem Dialog, bleibt sie heute im Dialog gefangen; man muss scrollen.

- Die Liste liegt **über** dem Dialog (Teleport an `body`, eigene Ebene), unter oder über dem Feld, je
  nachdem wo Platz ist, so breit wie das Feld, höchstens so hoch wie der freie Platz.
- Der Dialog wächst und scrollt dadurch nicht. Beim Scrollen oder Ändern der Fenstergröße folgt die Liste
  dem Feld oder schließt sich.
- Tastatur (Pfeile, Enter, Esc schließt **nur** die Liste), Klick daneben, Fokus zurück, ARIA
  (`combobox`/`listbox`). Pointer-Cursor auf allem Klickbaren.
- **Alle** Auswahllisten der Verwaltung nutzen denselben Baustein. Client (egui): prüfen, ob es dort
  denselben Mangel gibt, und angleichen; Client und Verwaltung sehen gleich aus.

## 2. Benutzer und Gruppen zuordnen

Heute kommen Gruppen eines Benutzers nur aus Entra. Neu, von **beiden Seiten**:

- Gruppe → Reiter/Abschnitt „Mitglieder“: Benutzer suchen, hinzufügen, entfernen.
- Benutzer → Abschnitt „Gruppen“: Gruppen hinzufügen, entfernen.
- Herkunft ist sichtbar: „aus Entra“ (nicht entfernbar, Hinweis: in Entra ändern) oder „von Hand“.
  Beides zugleich ist möglich; „von Hand“ entfernen lässt „aus Entra“ stehen.
- Auch noch nicht angemeldete Benutzer? Nur Benutzer, die der Server kennt. Root bekommt keine Gruppen.

### Regeln

- Eigene Tabelle `benutzer_gruppen (oid, tid, gruppe, von, seit)`, Migration mit Sicherung wie bisher.
  Eine von Hand vergebene Zugehörigkeit gehört zum **Benutzer** und überlebt Entra-Auffrischung,
  Neuanmeldung und neue Geräte. Das alte `manuell_json` am Gerät (Altgeräte ohne Entra) bleibt.
- Wirksame Gruppen = Entra ∪ von Hand ∪ Gerät (alt). Eine Stelle im Code, von Soll, Verwaltung und
  Stromprüfung benutzt.
- Gelöschte Gruppe oder gelöschter/gesperrter Benutzer: Zuordnung verschwindet bzw. wirkt nicht.
- Änderung wirkt sofort (Rundruf), auch der Entzug: verliert ein Benutzer die letzte Gruppe, gilt
  „kein Zugriff“ wie heute.
- Nur Admin/Root (Rollen aus Aufgabe 24 sind noch nicht da); Sitzung, Origin und `X-Quiver` wie alle
  ändernden Anfragen. Prüfspur: wer, wen, welche Gruppe, hinzugefügt/entfernt.
- Wer sich selbst aussperrt, sieht eine Warnung und muss bestätigen.
- Entra-Gruppen mit gleichem Alias dürfen nicht verwechselt werden: Kennung zählt, Name ist Anzeige.

## Prüfen

Servertests (Vereinigung, Entzug sofort, Auffrischung behält „von Hand“, Migration von Altdaten,
Rechte, Prüfspur), Verwaltungstests, Browser-Prüfung beider Seiten **samt** Auswahlliste im Dialog
(Dialoghöhe unverändert, kein Scrollbalken), Bilder für beide Themen mit erfundenen Daten.
fmt, clippy, `cargo test --workspace`, `npm test`, `npm run typecheck`, `npm run build`.

**Nicht committen, nicht pushen.**
