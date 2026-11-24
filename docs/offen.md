# Offene Punkte (Cheats und Lücken)

Jeder Punkt ist bewusst offen gelassen und wird abgebaut (AGENTS.md §2.5).

## Server
- [ ] Keine Begrenzung der Versuche bei `POST /anmelden` (Codes laufen nach 60 min ab, 32⁸ Möglichkeiten).
- [ ] Abgelaufene Codes werden nur beim Einlösen entfernt, kein Aufräumen im Hintergrund.

## Client
- [ ] Im Fenster-Modus reagiert der Client nicht auf SIGINT (Strg+C); nur SIGTERM beendet ihn.
- [ ] Schnipsel für OpenCode, GitHub Copilot und die Header-Form von Hermes sind nicht gegen die CLI geprüft (nur Doku bzw. Quelltext).
- [ ] Ein-Instanz-Port liest die Zeile ohne Längengrenze (nur lokal erreichbar).
- [ ] Ungetestet: Prozess stirbt → `Fehler` + Neustart mit Backoff; `Remote`-Einträge; Start ohne Server nach Neustart.
- [ ] `tools/list_changed` wird auch ohne echte Änderung der Tool-Liste geschickt.
- [ ] Die Einrichtung (Aufgabe 11) hat bewusst keine Frist; ein hängendes Tool
      blockiert seinen Eintrag, bis der Prozess endet oder der Client beendet wird.
- [ ] `abmelden` räumt lokale Werte, Geheimnisse und Agenten nicht auf.
- [ ] Tools pro Agent filtern (SPEC §7) – die Agent-Kennung liegt in der Sitzung schon vor.
- [ ] stderr der MCP-Prozesse geht ungefiltert ins Log (`laufzeit::stderr_ins_log`); `anmeldung::maskieren` dort ebenfalls anwenden.
- [ ] Mehrere Zugänge (Aufgabe 14): die Liste ist nach Namen sortiert (BTreeMap), der User kann sie nicht umsortieren.
- [ ] Mehrere Zugänge: `QUIVER_ZUGAENGE` steht nur in der Laufzeit eines `Stdio`-Eintrags; Anmelde- und Einrichtungsbefehle bekommen die Variable nicht mit.
- [ ] Mehrere Zugänge: ein Pflicht-Geheimnis lässt sich einzeln entfernen, ohne dass die Pflichtprüfung anschlägt (gewollt: Passwort weg, Schlüssel steht).
- [ ] Mehrere Zugänge: das Entfernen der letzten Geheimnisse räumt nur die Felder der aktuellen Liste und die gesetzten Werte auf, keine verwaisten Schlüssel aus früheren Feldnamen.
- [ ] Protokoll (Aufgabe 17): einen **Export** gibt es nicht — wer die Aufrufe
      mitnehmen will, liest die JSONL-Dateien unter `<profil>/protokoll/` von Hand.
- [ ] Protokoll: Fehlerzahlen werden **nicht** an den Server gemeldet; der Server
      sieht nur den Zustand der Einträge (`laeuft`, `fehler`), keine Zähler.
- [ ] Protokoll: die Maske erkennt Geheimnisse an Schlüsselnamen und am Tresor;
      schreibt ein Tool einen Wert, der ihm selbst bekannt ist, unstrukturiert auf
      stderr, bleibt er stehen, solange er nicht im Tresor liegt.
- [ ] Protokoll: beim Scrollen hält die Liste an (keine neuen Zeilen oben), damit
      die Position nicht springt; zurück nach oben lädt sie wieder live.
- [ ] Protokoll: die Suche im Reiter „Tool-Logs“ wirkt nur auf den gewählten
      Eintrag; über alle Einträge zu suchen gibt es nicht.
- [ ] Protokoll: `quiver__protokoll` und `quiver__tool_log` filtern nicht nach
      Agent — jeder Agent mit gültigem Token sieht, was alle Agenten aufriefen.

## Oberfläche
- [ ] Hilfe-Link: Platzhalter auf reine Server-Felder bleiben gesperrt (`EintragSicht` kennt keine Server-Werte).
- [ ] Hilfe-Link: halb ausgefüllte Links (nur Text oder nur Adresse) werden gespeichert.
- [ ] „1 Tools“ – Einzahl fehlt bei Anzahl 1.
- [ ] Zugangsliste: die zwei „wichtigen Werte“ sind die ersten zwei nicht-geheimen Felder in Feldreihenfolge, mit `@` verbunden – keine feldbezogene Formatierung (`benutzer@host:port`).
- [ ] Segmentauswahl: gewählter Text ist in egui fett, in Vue normal.
- [ ] egui-Zeilen 1–2 px flacher als im Browser (kein line-height-Modell); Karten bis 4 px kürzer.
- [ ] `image` als Dev-Abhängigkeit in `oberflaeche`, nur zum Zuschneiden der Musterbilder.
- [ ] Keine Saat-Daten für die Admin-Bildschirmfotos (werden aus einer temporären DB erzeugt).
- [ ] weppelmann-ui: `--wp-fact-label` fehlt in `tokens/`; Bausteine dort haben intern noch px-/Hex-Werte.
- [ ] Layout (Aufgabe 18): Die Stufe einer Tool-Log-Zeile wird aus dem Text geraten
      (`ERROR`, `WARN`, `INFO`) — `quiver_client::protokoll::Zeile` trägt kein Feld dafür.
      Schreibt ein Tool etwas anderes, bleibt die Stufe leer.
- [ ] Layout: Die Tool-Logs zeichnen alle Zeilen statt nur der sichtbaren
      (`show_rows`): mit umbrechenden Zeilen sind die Höhen nicht mehr gleich.
      Bei 5000 Zeilen kostet das Layout je Bild spürbar.
- [ ] Layout: In der schmalen Seitenleiste (unter 760 px) fällt der
      Abschnittsname („Gerät“) weg — es gibt keinen Hinweis dafür.
- [ ] Layout: Passt „Protokoll leeren“ nicht mehr in die letzte Filterzeile,
      steht es allein in einer eigenen Zeile (rechts).

## Allgemein
- [ ] `docs/architektur.md` und `docs/datenfluss.md` fehlen noch.
- [ ] Nach dem MVP: Entra ID, Signaturen, WASM-Laufzeit (SPEC §0).

## Hintergrundbetrieb (Aufgabe 08)
- [ ] Der Weg für die zweite Instanz (`127.0.0.1:<MCP-Port + 1>`) ist ein
      einfacher TCP-Port ohne Verschlüsselung. Er kann nur „Fenster zeigen",
      aber jeder lokale Prozess kann das Fenster damit nach vorn holen.
- [ ] Ist der Port von etwas anderem belegt, das nicht antwortet, startet
      quiver ohne Wächter — dann gäbe es zwei Instanzen.
- [ ] `Waechter` beendet seinen Faden nicht beim Beenden; der Prozess endet
      ohnehin, deshalb steht es offen.
- [ ] Start im Hintergrund unter Wayland: `ViewportBuilder::with_visible(false)`
      wird dort nicht überall beachtet; das Fenster kann kurz erscheinen.
      (Weglegen beim Schließen geht dort über `Minimized`.)
- [ ] Tray-Symbol ist das Lucide-Zeichen `package` in Weiß — auf einer hellen
      Windows-Taskleiste schlecht sichtbar. Wird mit dem Logo ersetzt
      (AGENTS.md §8).
- [ ] Linux-Teil (`ksni`) ist hier nicht baubar (macOS) — nur nach Vorlage
      geschrieben, nicht gelaufen.
- [ ] `QUIVER_TRESOR=speicher` ist ein Entwicklerweg (Geheimnisse gehen beim
      Beenden verloren); gedacht für Läufe ohne Spuren.

- [ ] Tresor-Datei (ADR mcp-quiver 0001): optional Hauptpasswort (Argon2) beim Start.
