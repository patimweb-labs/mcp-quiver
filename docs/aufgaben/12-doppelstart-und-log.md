# 12 – Fehler: Doppelstart verliert die Tool-Liste; Einmal-Token im Log

## A – Doppelstart (crates/client)

**Beobachtet (2026-11-23):** Nach dem Start des Clients wurde der Eintrag `outline` zweimal im Abstand von
~30 ms gestartet (zwei „Service initialized“ im Log). Danach lief der Prozess, aber `Innen::werkzeuge()`
lieferte für `outline` **keine** Tools – Agenten sahen das Tool nicht. Ein späterer Neustart (Versionswechsel)
behob es. Vermutete Ursache: Start aus dem gespeicherten Soll und gleich danach Abgleich mit dem Server-Soll
überlappen; der zweite Start ersetzt den Läufer, während die Aufgabe des ersten noch schreibt (oder umgekehrt).

1. **Reproduzieren** mit einem Test (zuerst rot!), z. B. Kern mit gespeichertem Soll starten und sofort
   denselben Soll vom Server bekommen, oder zwei `neu_starten` desselben Eintrags direkt hintereinander.
2. Beheben: pro Eintrag höchstens **eine** Start-Aufgabe; jede Aufgabe trägt eine Generation/Kennung, und
   `innen.aendern` schreibt nur, wenn der Läufer noch zu dieser Generation gehört. Gleicher Fingerabdruck wie
   der laufende → gar nicht neu starten.
3. Test grün, auch unter Wiederholung (z. B. 50 Durchläufe).

## B – Einmal-Token im Log (crates/client)

`anmeldung.rs` schreibt stdout/stderr der Befehle ins Log. `quiver-<tool> einrichten` gibt die Adresse der
Einrichtungsseite aus, inklusive Einmal-Token (`/e/<token>/`). Im Log maskieren: alles, was wie
`/e/<hex>/` aussieht, als `/e/<verborgen>/` schreiben (reine Funktion mit Test). Allgemeiner: Zeilen mit
`token`, `password`, `passwort`, `secret`, `Bearer` nach dem Schlüsselwort maskieren.

## Abnahme

Prüfbefehl grün. In der Antwort: die Ursache von A in zwei Sätzen.
