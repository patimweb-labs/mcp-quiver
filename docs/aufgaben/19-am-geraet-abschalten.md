# 19 – Einträge am Gerät abschalten

Der User will einen ganzen MCP (Eintrag) auf **seinem** Gerät ausschalten, ohne dass der Admin etwas ändert.

## Verhalten

- Schalter je Eintrag auf der Seite Tools, Standard **an**. Auch neu zugewiesene Einträge starten an.
- **Aus:** laufender Prozess wird beendet, Start-Aufgaben abgebrochen (Generation!), seine Tools verschwinden sofort aus `tools/list`, `list_changed` geht an alle Agenten. Ein noch laufender Aufruf endet mit einer klaren Meldung. Aufrufe danach: „<Titel> ist auf diesem Gerät abgeschaltet – im quiver-Fenster unter Tools einschalten.“ (nie „unbekannt“).
- Aus bleibt aus: über Neustart, Abgleich, Änderung des Eintrags am Server, `werte_setzen`, Zugang speichern, Anmelden/Abmelden, Import. **Kein** Weg startet einen abgeschalteten Eintrag, auch nicht die regelmäßige Status-Prüfung (`anmeldung_lauf` überspringt ihn). Angaben, Zugänge, Geheimnisse und Anmeldung bleiben erhalten und lassen sich weiter bearbeiten.
- **An:** startet sofort mit den aktuellen Werten, wie nach einer Zuweisung – aber nur, wenn der Server das Gerät in diesem Lauf angenommen hat.
- Server-Vorrang: am Server entfernt oder deaktiviert → verschwindet wie bisher. Kommt derselbe Eintrag später wieder, gilt die alte Wahl. Gerät ausbinden/abgelehnt: Wahl bleibt.
- Agenten können **nicht** schalten; es gibt kein MCP-Tool dafür.

## Umsetzung

- `zustand.json`: Menge abgeschalteter Eintrags-Ids (`serde(default)`, alte Dateien lesbar). `Kern::eintrag_schalten(id, an)`; speichern **vor** dem Schalten, scheitert es → Fehler, nichts geschaltet. Eine Stelle entscheidet, ob ein Eintrag laufen darf; alle Start-Wege gehen hindurch. Gleichzeitiges Schalten und Abgleichen darf keinen Prozess übrig lassen.
- `EintragSicht.am_geraet_aus`, Zustand `Abgeschaltet`. Meldung an den Server: neuer `EintragZustand::AmGeraetAus`; Verwaltung zeigt „am Gerät abgeschaltet“ neutral (kein Fehler, zählt nicht als Problem). **Verträglichkeit:** Ein Server, der den Wert nicht kennt, darf die Meldung nicht ablehnen und umgekehrt – beide Seiten lesen unbekannte Zustände als neutralen Wert (Vertragstest mit fremdem Wert).
- Export/Import (Aufgabe 15): Wahl mitnehmen, Vorschau zeigt sie.
- Fenster: Checkbox/Schalter rechts in der Karte, Titel „Auf diesem Gerät aktiv“, Hand-Cursor, Tastatur, abgeschaltete Karte gedämpft mit Abzeichen „Abgeschaltet“; Anmelden-/Einrichten-Knopf gesperrt mit Hinweis. Filter „nur aktive“. Tray zählt nur aktive. Texte de/en, drei Themes, Breiten 560/800/1200.

## Tests

Ende-zu-Ende mit echtem Server und Demo-MCP: aus → Tools in < 1 s weg, Prozess beendet (PID prüfen), `list_changed` kommt; Neustart bleibt aus; Server ändert den Eintrag → bleibt aus; alle oben genannten Wege starten ihn nicht; an → läuft wieder; schnelles An/Aus/An (50×) mit Abgleich dazwischen → genau ein Prozess; Speicherfehler → unverändert; Aufruf auf abgeschaltetes Tool → Meldung; zwei Agenten sehen dasselbe. kittest (Schalter, Hand-Cursor, gesperrter Knopf, schmal), vitest für die Verwaltung. Bilder unter `docs/bilder` aktualisieren und selbst ansehen.

Doku: README (en/de), SPEC. Prüfbefehl grün (Rust und admin). Nicht committen.
