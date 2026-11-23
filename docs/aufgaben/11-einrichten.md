# 11 – Knopf „Einrichten“ im Client-Fenster

Tools aus `quiver-tools` (Outline, Email, SSH, ABAP) verwalten Zugänge in **Profilen** (mehrere Wikis,
Postfächer, Hosts, SAP-Systeme). Eingerichtet wird auf einer Seite, die das Tool selbst im Browser öffnet –
**nie** über einen Agenten. Das Datenmodell steht: `Eintrag.einrichtung: Option<Einrichtung { einrichten }>`
in `crates/domain` (lesen!). Vorbild ist Aufgabe 10 (Anmeldung) – gleiche Mechanik, gleiche Befehlszeile
(selbes Programm, selbe Umgebung, `${feld}` ersetzt).

## Client-Kern

1. `EintragSicht.einrichtung: Option<EinrichtZustand>` mit `Bereit`, `Laeuft`, `Fehler { text }`
   (`None` = keine Einrichtung).
2. `Kern::einrichten(eintrag)`: Zustand `Laeuft`, Befehl ausführen (kein Timeout – der User arbeitet auf der
   Seite; der Befehl endet selbst), Exit 0 → `Bereit` und **Eintrag neu starten** (neue Profile wirken sofort),
   Exit 1 → `Bereit` ohne Neustart (abgebrochen), sonst `Fehler`.
3. Nur **eine** Einrichtung je Eintrag gleichzeitig; ein zweiter Klick tut nichts.

## Fenster

Tools-Karte mit `einrichtung`: Knopf **„Einrichten“** (Icon `settings`, normaler Knopf, nicht Hauptknopf),
während `Laeuft` ersetzt durch „Einrichtung im Browser geöffnet …“ (Ton leise). Bei `Fehler` Text + „Erneut versuchen“.
Hand-Cursor, Texte de/en, Oberflächentests für die drei Zustände, Bild `fenster-{dark,light}-einrichten.png`.

## Admin

`typen.ts` um `einrichtung` ergänzen; im Eintrag-Formular Abschnitt „Einrichtung (optional)“ mit einem Feld
(Argumente, Leerzeichen-getrennt); Abzeichen „mit Einrichtung“ in der Liste.

## Abnahme

- Integrationstest mit einem Test-Programm (`quiver-anmelde-attrappe` um einen Modus `einrichten` erweitern,
  der eine Datei schreibt und mit 0 endet) → Zustand `Laeuft` → `Bereit`, Eintrag neu gestartet.
- Prüfbefehl grün, `admin/`: typecheck, test, build grün.
