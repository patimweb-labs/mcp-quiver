# 14 – Mehrere Zugänge je Tool, im Fenster gepflegt

Datenmodell steht: `Eintrag.mehrfach: Option<Mehrfach { titel, einzahl, felder }>`, `Zugang { name, werte }`,
`MEHRFACH_UMGEBUNG = "QUIVER_ZUGAENGE"`, `zugangsname_gueltig` in `crates/domain` (lesen!).
Erster Nutzer: quiver-ssh („Verbindungen“: Name, Host, Port, Benutzer, Anmelde-Passwort, sudo-Passwort).

## Client-Kern (`crates/client`)

1. Speicher je Eintrag: Liste der Zugänge (Name + nicht-geheime Werte) in der Client-Konfiguration,
   Geheimes im Tresor unter `zugang/<eintrag>/<name>/<feld>`.
2. `Kern::zugang_speichern(eintrag, name, werte_lokal, werte_geheim)` (anlegen oder ändern; leeres Geheimfeld =
   unverändert), `Kern::zugang_loeschen(eintrag, name)` (auch die Geheimnisse), Namensprüfung mit
   `zugangsname_gueltig`, Pflichtfelder gegen `mehrfach.felder`.
3. Beim Start eines Eintrags mit `mehrfach`: alle Zugänge als JSON (`Vec<Zugang>`, Geheimes eingesetzt) in
   `QUIVER_ZUGAENGE` setzen. Nach jeder Änderung den Eintrag **neu starten**.
4. `EintragSicht.zugaenge: Option<ZugaengeSicht { titel, einzahl, felder, liste: Vec<ZugangSicht { name, werte (ohne Geheimes), geheim_gesetzt: Vec<String> }> }>`.
   Geheimes nie im Modell.
5. Ein Eintrag mit `mehrfach`, aber noch ohne Zugang, läuft trotzdem (das Tool meldet dann selbst „kein Zugang“).

## Fenster (`crates/fenster`)

Tools-Karte mit `mehrfach`: Zeile „<titel> (<Anzahl>)“ und Knopf „<einzahl> hinzufügen“. Aufklappbare Liste der
Zugänge (Name + 1–2 wichtige Werte, z. B. `benutzer@host:port`), je Zeile „Bearbeiten“ und „Löschen“ (Rückfrage).
Dialog wie „Angaben“: Name (beim Bearbeiten fest), die Felder, Geheimfelder als Passwortfelder mit „gesetzt –
leer lassen behält es“ und einem Knopf „Löschen“ je gesetztem Geheimfeld (z. B. Anmelde-Passwort entfernen,
nachdem der Schlüssel eingerichtet ist), Hilfe-Links wie in Aufgabe 13.
Hand-Cursor, Texte de/en, Oberflächentests (leer, zwei Zugänge, anlegen, Geheimfeld löschen), Bild
`fenster-{dark,light}-zugaenge.png`.

## Admin (`admin/`)

`typen.ts` um `mehrfach`; im Eintrag-Formular Abschnitt „Mehrere Zugänge (optional)“: Titel, Einzahl und ein
Felder-Editor wie bei den normalen Feldern (Quelle nur client/geheim). Abzeichen „mehrere Zugänge“.

## Abnahme

- Integrationstest: Eintrag mit `mehrfach` (Programm: `quiver-anmelde-attrappe` um einen Modus erweitern, der
  `QUIVER_ZUGAENGE` in eine Datei schreibt) → zwei Zugänge anlegen → Datei enthält beide samt Geheimnis →
  einen löschen → Neustart, Datei enthält einen; Geheimnis aus dem Tresor entfernt.
- Prüfbefehl grün, `admin/` typecheck, test, build grün.
