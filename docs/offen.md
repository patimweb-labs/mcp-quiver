# Offene Punkte (Cheats und Lücken)

Jeder Punkt ist bewusst offen gelassen und wird abgebaut (AGENTS.md §2.5).

## Tests
- [ ] `instanz::tests` (Fenster) hängt an zwei Zeitfenstern (500 ms Annahme,
      2 s Verbindung). Unter schwerer Last des Rechners (viele Testprozesse
      gleichzeitig) ist `zweite_instanz_weckt_die_erste` gelegentlich rot,
      einzeln und in ruhigen Läufen grün. Ein fester Ablauf ohne Netz und Zeit
      (Kanal statt Port) wäre die ehrliche Lösung.

## Server
- [ ] Die Abschlussseite des Browser-Lauschers ist schlicht und nur deutsch;
      die Fenstertexte sind de/en. Die Seite verrät keinen Code, aber sie folgt
      auch nicht der Sprache des Browsers.
- [ ] Die JWKS wird nur bei unbekannter `kid` neu geholt (höchstens 1×/min); die
      Discovery nach 24 Stunden. Ein Endpunktwechsel bei Entra mitten im Betrieb
      erreicht den Server damit erst beim nächsten Auffrischen.
- [ ] Der Einmal-Code der Ersteinrichtung (Aufgabe 25) steht im Log und ist
      128 Bit breit; wer das Log lesen darf, kann den Server in den ersten 15
      Minuten übernehmen. Gewollt, aber es bleibt ein Cheat für den Betrieb:
      Logs der ersten Minuten wegräumen hilft nicht rückwirkend.
- [ ] Zustand der Ersteinrichtung (Code-Hash, Fehlversuche) liegt nur im
      Arbeitsspeicher; ein Neustart erzeugt einen neuen Code und öffnet das
      Fenster erneut. Bei mehreren Instanzen hätte jede ihr eigenes Fenster.
- [ ] Die Ersteinrichtung verlangt den Code, sobald der Server nicht nur auf
      Loopback lauscht – der Docker-Weg braucht ihn also. Nur mit
      `QUIVER_EINRICHTUNG=ohne-code` **und** Loopback-Basis-URL entfällt er;
      dann lässt sich der `Host`-Kopf fälschen, und wer den Port in den ersten
      15 Minuten erreicht, könnte den Root anlegen. Eine Prüfung der
      Aufrufer-IP statt der Lauschadresse wäre strenger, bricht aber den
      üblichen Docker-Weg (`-p 127.0.0.1:…`, im Container kommt die Verbindung
      vom Docker-Netz).
- [ ] Zeitlimit und Größenbegrenzung der Entra-Antworten sind eingebaut, aber
      nicht gegen einen hängenden oder riesigen Nachbau getestet.
- [ ] Die Drossel liegt im Arbeitsspeicher; ein Neustart vergisst die Versuche.
      Bei mehreren Instanzen zählt jede für sich.
- [ ] Zu offene Rechte an `entra-secret` ergeben eine Warnung, keinen Abbruch.
- [ ] Die Sitzungs-Id wird nur bei der Anmeldung neu vergeben, nicht laufend
      rotiert.
- [ ] `admin-setzen --oid` setzt voraus, dass der Benutzer schon einmal gesehen
      wurde; die Liste der Benutzer hat keine Suche und keine Seiten.
- [ ] Der Log-Test deckt den Ablauf Anmeldung/Prüfen/Fehlversuch und die
      Ersteinrichtung ab; ein Geheimnis in einer ganz anderen Meldung wäre
      nicht erfasst.
- [ ] Das Stylesheet der Server-Seiten ist eine erzeugte Datei
      (`crates/server/seiten/system.css`); sie wird von einem Test gegen die
      Tokens geprüft, aber nicht in den Container-Bau eingecheckt – wer die
      Tokens ändert, muss sie neu erzeugen.
- [ ] Die Seiten des Servers nutzen nur Farben und Maße; Bausteine der
      Verwaltung (Knöpfe, Karten) sind nachgebaut, nicht geteilt. Ein
      gemeinsamer Bausatz für Vue und die Server-Seiten fehlt.
- [ ] Ein Gerät, dessen altes Token einem anderen Benutzer gehört, bekommt ein
      neues Gerät; das alte bleibt in der Verwaltung stehen.
- [ ] Die Prüfspur der Kommandozeile nennt als Akteur `root-setzen`, nicht die
      E-Mail des Aufrufers.
- [ ] Abgelaufene Einmal-Codes und OIDC-Zustände werden im Hintergrund
      aufgeräumt (alle 60 s), nicht in einem eigenen Takt je Tabelle.

## Client
- [ ] SIGTERM und Strg+C enden geordnet; das „Abmelden des Systems“ ist auf
      macOS/Windows nicht als eigener Grund erkennbar (dort bleibt es beim
      Fenster-Ende bzw. SIGTERM). Der Signal-Faden ist nicht als
      Integrationstest gegen ein echtes Signal geprüft; die Fahne und die
      Texte sind getestet.
- [ ] Cmd+Q wird im Fenster abgefangen und in den Tray gelegt. Auf Plattformen
      ohne Tray ist das Fenster der einzige Weg; dann bleibt Cmd+Q ein
      Weglege-Befehl ohne sichtbares Symbol.
- [ ] Schnipsel für OpenCode, GitHub Copilot und die Header-Form von Hermes sind nicht gegen die CLI geprüft (nur Doku bzw. Quelltext).
- [ ] Ein-Instanz-Port liest die Zeile ohne Längengrenze (nur lokal erreichbar).
- [ ] Ungetestet: Prozess stirbt → `Fehler` + Neustart mit Backoff; `Remote`-Einträge; Start ohne Server nach Neustart.
- [ ] `tools/list_changed` wird auch ohne echte Änderung der Tool-Liste geschickt.
- [ ] Die Einrichtung (Aufgabe 11) hat bewusst keine Frist; ein hängendes Tool
      blockiert seinen Eintrag, bis der Prozess endet oder der Client beendet wird.
- [ ] `abmelden` räumt lokale Werte, Geheimnisse und Agenten nicht auf — das
      ist seit Aufgabe 22 gewollt: Abmelden verwirft Token und Gerät, nicht die
      Angaben und nicht die Server-Adresse. Wer die Angaben löschen will, löscht
      das Profil.
- [ ] Der Server hat für „Abmelden“ einen Geräteweg (`POST
      /api/v1/geraet/abmelden`) bekommen. Das war für Aufgabe 22 nötig; die
      parallele Arbeit an `crates/server` muss diesen Weg kennen.
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
- [ ] Am Gerät abschalten (Aufgabe 19): die Wahl gilt nur auf diesem Gerät; wer
      sie auf allen Geräten will, schaltet jedes einzeln ab (der Admin-Schalter
      *Aktiv* bleibt die zentrale Steuerung).
- [ ] Am Gerät abschalten: der Schalter steht im Fenster, nicht im Tray und
      nicht auf der Verbindungsseite; aus einem Kopfstand lässt sich ein
      Eintrag also nur im Fenster wieder einschalten.
- [ ] Am Gerät abschalten: ein abgeschalteter Eintrag, den der Server später
      wieder zuweist, startet nicht — das ist gewollt, aber es gibt keinen
      Hinweis in der Verwaltung, *warum* er auf dem Gerät nicht läuft (nur der
      Zustand „am Gerät abgeschaltet“).

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

## Release (Aufgabe 31)
- [ ] Das Windows-Paket (`.msi` mit WiX, portables `.zip`, `.ico`) ist hier
      nicht baubar (macOS) — nur nach Vorlage geschrieben, nicht gelaufen. Die
      PowerShell-Syntax ist nicht geprüft.
- [ ] Die Linux-Pakete (`.deb`, `.rpm` über nfpm, AppImage) und die
      aarch64-Bündel sind lokal nicht gelaufen; nfpm-Vorlage und die
      AppStream-Metadaten haben noch keinen Prüflauf gesehen.
- [ ] Signieren und Notarisieren ist nie mit echten Geheimnissen gelaufen —
      der Weg ist geschrieben, aber unbewiesen. Bis dahin ist jedes macOS-Bündel
      ad hoc signiert und jedes Windows-Paket unsigniert.
- [ ] Die `.icns` entsteht aus `assets/marke/quiver-dock.png` (318 px) und wird
      für 512 und 1024 hochskaliert; ein Bild in 1024 px fehlt (AGENTS.md §8).
- [ ] Das AppImage wird mit `--no-appstream` gebaut; die mitgelieferten
      Metainfo-Daten werden dabei nicht geprüft.
- [ ] Der Text des Releases ist deutsch; englische Release-Notes gibt es nicht.
- [ ] Kein Flatpak — nicht gefordert, wäre ein eigenes Paket.

## Updates (Aufgabe 34)
- [ ] Die Update-Abfrage des Servers ist gegen eine Attrappe auf Loopback
      geprüft, nicht gegen die echte GitHub-API. Der Vertrag (JSON, `tag_name`,
      `html_url`, `draft`/`prerelease`) steht in `quiver_domain::updates`.
- [ ] Die Vorgaben werden je Plattform gelesen (Registry, verwaltete
      Einstellungen, `/etc`), aber nur unter Linux und über den verlegbaren Ort
      wirklich geprüft. Ein Test gegen eine echte Registry oder eine echte
      `.mobileconfig` fehlt — beides schriebe in HKLM bzw. `/Library` und ist
      darum bewusst nicht Teil der Tests.
- [ ] Die macOS-Vorgaben werden über `plutil` in XML gewandelt (Bordmittel);
      eine bereits binäre plist wird damit gelesen, aber nicht selbst geparst.
- [ ] `.mobileconfig`, ADMX/ADML und die Registry-Vorlage sind Vorlagen mit
      Platzhalter-Adresse; gegen ein echtes Intune ausgerollt wurde nichts.
- [ ] Das `.pkg` wird lokal gebaut und mit `pkgutil --expand` geprüft, aber
      nicht installiert. Die Skripte (`preinstall`/`postinstall`) laufen erst im
      CI-Lauf auf einem macOS-Läufer.
- [ ] Die stille MSI-Installation und das Update sind nur im CI-Lauf auf einem
      Windows-Läufer geprüft, nicht auf einem echten verwalteten Rechner.

## Update durch den Client (Aufgabe 39)
- [ ] Der Tausch ist je Art im Testordner geprüft (ZIP, `tar.gz`, AppImage und
      `.app`; macOS-Teil nur auf macOS). Auf einem **echten** Gerät ist er
      nicht gelaufen: kein Windows-Rechner, kein Gerät mit MSI, kein
      AppImage-Start. Der erste Lauf auf einem wirklichen Rechner steht aus.
- [ ] Für das MSI gibt es **keinen Selbsttest** vor dem Tausch: es ist ein
      Installationsprogramm. Windows prüft es selbst (`/qn`,
      `MajorUpgrade`, Restart Manager); die neue Fassung macht ihren
      `--selbsttest` erst beim nächsten Start.
- [ ] Der Rückfall auf die alte Fassung greift beim **zweiten** Start ohne
      Beweis (Marke im Profil). Kommt die neue Fassung über den Start des
      Kerns hinaus und scheitert erst später, bleibt sie stehen; ein
      überwachender Helferprozess fehlt bewusst (ADR 0005, „Verworfene Wege“).
- [ ] Alte Pakete unter `<daten>/updates/<version>/` werden **nicht**
      aufgeräumt. Das kostet Platz und braucht einen Lauf, den es noch nicht
      gibt.
- [ ] Der Selbsttest wird nur gegen eine echte Fassung geprüft, nicht gegen
      eine, die absichtlich hängt. Die Frist (`SELBSTTEST_FRIST`, 60 s) ist
      deshalb nur im Code belegt.
- [ ] Die Client-Integration (`crates/client/tests/update.rs`) läuft nur unter
      Unix: der Selbsttest startet die neue Fassung als eigenes Programm, und
      die Attrappe dafür ist ein Shell-Skript. Unter Windows decken die
      Einheitstests des Tauschs und der CI-Lauf den Weg ab.
- [ ] Ist das Programm-Verzeichnis schreibbar, aber das Umbenennen scheitert
      (etwa weil ein Virenscanner die Datei hält), bleibt die alte Fassung an
      ihrem Platz und die Meldung nennt den Grund. Ein erneuter Versuch ist
      Handarbeit.

## Werkzeuge vom Server (Aufgabe 37)
- [ ] End-to-End **nicht** geprüft: „Platte voll“ (ENOSPC beim Schreiben der
      Zwischendatei).
- [ ] `QUIVER_TOOLS_DIR` zeigt auf einen Entwicklungsordner mit flach
      liegenden Programmen; ein ausgecheckter Stand wird nicht gegen eine
      Prüfsumme geprüft (so gewollt, aber es steht nirgends im Fenster).
- [ ] Die Verwaltung zeigt die Programme und die je Gerät laufenden Fassungen
      im Überblick; ein Filter „welcher Eintrag braucht ein Programm, das
      fehlt?“ fehlt.
- [ ] „Weniger Bewegung“ ist über egui genähert: dreht die Animation nicht
      (`animation_time` = 0), steht der Text ohne Ring. Eine Plattform-Anfrage
      (macOS/Windows-Einstellung) liest der Client nicht.
- [ ] Laufende Werkzeugaufrufe werden vor einem Programmtausch bis zu 10 s
      abgewartet; danach wird getauscht und der Grund steht im Protokoll des
      Eintrags. Ein Hinweis im Fenster („wartet auf 2 laufende Aufrufe“) fehlt,
      und ein Test dafür gibt es nicht (er bräuchte einen hängenden Aufruf).

## Release der Werkzeuge (Aufgabe 38, anderes Repo)
- [ ] Die Fassung der Programme im Abbild steht im `Dockerfile`
      (`QUIVER_TOOLS_VERSION`) und wird von Hand angehoben.

## Anmeldung eines Tools (Aufgabe 35, §5)
- [ ] Der Weg auf einem echten Windows-Gerät (Konsolenfenster, Standardbrowser,
      Loopback hinter der Firewall, Tresor unter `%APPDATA%`) ist hier nicht
      nachstellbar (macOS). Geprüft ist, was prüfbar ist: `CREATE_NO_WINDOW` für
      jeden Kindprozess, Binden nur an `127.0.0.1`, und die Ablage der
      Programme unter `%APPDATA%` (nur der Benutzer). Ein Lauf auf Windows
      steht aus.
- [ ] **Exit 3 („läuft bald ab“) kennt noch kein Werkzeug.** Der Client wertet
      ihn aus und die Karte zeigt den Zustand; `quiver-m365` und `quiver-zep`
      melden ihn bisher nicht. Bis dahin bleibt es bei „angemeldet“ und einem
      Fehlschlag mitten in der Arbeit.
- [ ] Die 10-Sekunden-Frist der Prüfung gilt auch auf einem sehr langsamen Gerät
      (Netz im Status-Befehl). Wird sie zu knapp, ist sie eine Zahl in
      `crates/client/src/anmeldung.rs` (`STATUS_FRIST`), kein Schalter.
- [ ] Der Tresor wartet fair, aber weiter **innerhalb** eines Aufrufs: gibt ein
      fremder Prozess die Sperre 10 s lang nicht frei, meldet der Client
      „gerade gesperrt“. Ein Wiederholen von selbst gibt es nicht — der Benutzer
      stößt es an.
