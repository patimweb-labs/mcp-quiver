# Changelog

Alle nennenswerten Änderungen an mcp-quiver. Das Format folgt
[Keep a Changelog](https://keepachangelog.com/de/1.1.0/), die Versionen
[Semantic Versioning](https://semver.org/lang/de/).

Pflegt `release-please` aus den Conventional Commits; von Hand kommt nur der
Eintrag zur ersten Version hierher. Der Tag ist `v<Version>` und muss zur
Version in `[workspace.package]` passen.

## [0.3.0](https://github.com/patimweb-labs/mcp-quiver/compare/v0.2.0...v0.3.0) (2026-11-25)


### Neu

* Benutzer und Gruppen von Hand zuordnen, Auswahllisten über dem Dialog ([f2d1815](https://github.com/patimweb-labs/mcp-quiver/commit/f2d181509a58848d06b2e942b9857a83a7c9c168))
* Benutzer und Gruppen von Hand zuordnen, Auswahllisten über dem Dialog ([dde6b9c](https://github.com/patimweb-labs/mcp-quiver/commit/dde6b9c63d67c28a2c327655334417a2f81a9777))


### Behoben

* Autostart-Test fasst die Windows-Registry nicht an, Zeilenenden LF ([0218a30](https://github.com/patimweb-labs/mcp-quiver/commit/0218a30cc2493ac2a5f576c052dbe9b47b9446f7))
* Entzug wirkt sofort ([f052966](https://github.com/patimweb-labs/mcp-quiver/commit/f0529665992e23530839d02f2c672b941a26ad55))
* Entzug wirkt sofort, auch beim Öffnen des Ereignis-Stroms ([65a5b5d](https://github.com/patimweb-labs/mcp-quiver/commit/65a5b5d47a67ff3ee1326671ee9b28f2efad2ad5))

## [0.2.0](https://github.com/patimweb-labs/mcp-quiver/compare/v0.1.0...v0.2.0) (2026-11-25)


### Neu

* **admin:** Admin-Oberfläche in Vue – Bibliothek, Sätze, Gruppen, Geräte, Einmal-Codes ([18c5b52](https://github.com/patimweb-labs/mcp-quiver/commit/18c5b52be8f57aeb3b8f8fc593f5593727796a80))
* Agenten nur einmal koppeln, Token erneuern, Schnipsel je Agent; Einträge im Admin abschalten ([a48789a](https://github.com/patimweb-labs/mcp-quiver/commit/a48789a49f44e11667fb9dace5630ee846cb7c25))
* Anmeldung von Tools im Client-Fenster (Anmelden/Abmelden/Status), Feld im Admin ([dc9110c](https://github.com/patimweb-labs/mcp-quiver/commit/dc9110c8ae42dfcca3606a859a67e60fc8e8a84d))
* **bibliothek:** server.json importieren und exportieren ([d5bef20](https://github.com/patimweb-labs/mcp-quiver/commit/d5bef2083eea15260a5a19a20c44f96022416a3d))
* Client – Anmeldung mit Microsoft über den Server ([814bb6c](https://github.com/patimweb-labs/mcp-quiver/commit/814bb6caf987d22ba27754b170784f78423b9c8d))
* **client:** Client-Kern – Abgleich per SSE, Laufzeiten stdio/remote, MCP-Endpunkt für Agenten ([a7abb2e](https://github.com/patimweb-labs/mcp-quiver/commit/a7abb2e2ff2b48a02380aaa04579fcf5c2055b3c))
* **client:** Demo-MCP, das Aufrufe nur protokolliert; docs: Aufgabe 08 (Tray, Autostart) ([cff9af3](https://github.com/patimweb-labs/mcp-quiver/commit/cff9af3f3a3386dd13ed7a48044a8ce04658dad9))
* **client:** Einstellungen der Tools exportieren und importieren ([92f3675](https://github.com/patimweb-labs/mcp-quiver/commit/92f3675b38a456e1a5338768df3a913efdfd2684))
* Codes ohne Ablauf, Tools erst nach Anmeldung am Server, entferntes Gerät stoppt alles ([6ea5a02](https://github.com/patimweb-labs/mcp-quiver/commit/6ea5a028320c54d9243c2338f6d1420a3a90ab63))
* **domain:** Einträge abschalten – bleiben in Bibliothek und Sätzen, laufen auf keinem Gerät ([af93600](https://github.com/patimweb-labs/mcp-quiver/commit/af93600f28f01c24722b3f447bc643f7b9d62dbb))
* **domain:** Einträge können eine Anmeldung beschreiben (anmelden/status/abmelden) ([d88978a](https://github.com/patimweb-labs/mcp-quiver/commit/d88978a4d1edabac174c4169fee9bf758122458b))
* **domain:** Einträge können eine Einrichtungsseite anbieten; docs: Aufgabe 11 ([901bb88](https://github.com/patimweb-labs/mcp-quiver/commit/901bb8860f8ab00459f1df2941694166587e1f9c))
* **domain:** Felder können einen Hilfe-Link mitbringen (FeldLink, nur http/https); docs: Aufgabe 13 ([eea1192](https://github.com/patimweb-labs/mcp-quiver/commit/eea1192f808a687c63ac077b1dfc963a9da279d6))
* **domain:** mehrere vom User gepflegte Zugänge je Tool (Mehrfach, QUIVER_ZUGAENGE); docs: Aufgabe 14 ([f0696e9](https://github.com/patimweb-labs/mcp-quiver/commit/f0696e9175170f1e4395e8db4f7352047513d9ef))
* egui und Vue 1:1 – gemeinsame Maße aus den Tokens, Musterseite zum Vergleich ([eefff6d](https://github.com/patimweb-labs/mcp-quiver/commit/eefff6d753e8707c12dce9b1c643eafffa6639d5))
* Ersteinrichtung im Browser, Befunde der Abnahme ([842f88a](https://github.com/patimweb-labs/mcp-quiver/commit/842f88adb9bed30579643641acd041b096293189))
* Feldart Ordner mit Ordner-Dialog im Client ([c9b5c22](https://github.com/patimweb-labs/mcp-quiver/commit/c9b5c22c9a0b513286526005134071029fb0b28c))
* Feldarten – Schalter als Checkbox, Titel, sichtbares Scrollen, Sperrgrund ([fc6ce93](https://github.com/patimweb-labs/mcp-quiver/commit/fc6ce93c15a7acb7d8fadc06b5d3127a10c2617c))
* **fenster:** Client-Fenster in egui – Verbindung, Tools, Agenten koppeln ([6e24fb7](https://github.com/patimweb-labs/mcp-quiver/commit/6e24fb7d2ad65bb61da0add24a790160cb628005))
* **fenster:** Hintergrundbetrieb – Tray, Fenster nur auf Wunsch, eine Instanz, Autostart ([0626904](https://github.com/patimweb-labs/mcp-quiver/commit/0626904d7aa0f483457509b4b31844ed6db18785))
* **fenster:** responsives Layout, Protokoll aufgeräumt ([be10c3f](https://github.com/patimweb-labs/mcp-quiver/commit/be10c3f07e347f7bd52b04f9a87ef4f012c598f9))
* Geheimnisse in eigener Tresor-Datei statt im Schlüsselbund ([f217265](https://github.com/patimweb-labs/mcp-quiver/commit/f217265dcccd1b10dedc23a09a96afa67cc1cb57))
* Grundgerüst – Workspace, Datenmodell, Protokoll, Bausatz aus sprechwerk ([b8f367e](https://github.com/patimweb-labs/mcp-quiver/commit/b8f367e2e7ee7c869bd68a6f0d251576b6af21de))
* Hilfe-Link neben Feldern im Angaben-Dialog (öffnet nur den Browser), Link-Felder im Admin ([a955c46](https://github.com/patimweb-labs/mcp-quiver/commit/a955c462ebf0441da1d6e9273a522c3aba6367fc))
* Knopf „Einrichten“ im Client-Fenster, Einrichtung im Client-Kern und im Admin ([6691063](https://github.com/patimweb-labs/mcp-quiver/commit/66910639ef170929c38913a5c5bc5965c3920ddf))
* Logo als App-, Tray- und Favicon ([2e3fc52](https://github.com/patimweb-labs/mcp-quiver/commit/2e3fc5205426848bab08313c49dbe0ad0d9a29dc))
* Logo oben links im Client und in der Verwaltung ([e7806d1](https://github.com/patimweb-labs/mcp-quiver/commit/e7806d120ce005fba6776e3cc8d6ff645ea42c3d))
* mehrere Zugänge je Tool im Client-Fenster pflegen (Liste, Geheimfelder einzeln löschen), Editor im Admin ([efad114](https://github.com/patimweb-labs/mcp-quiver/commit/efad11483dd750c920bd3dafc3d020f8fc7b06ec))
* Namen der Entra-Gruppen über Graph (nur Anzeige) ([3c649d1](https://github.com/patimweb-labs/mcp-quiver/commit/3c649d11f5d2c5952fd11cee3ced1363863e28e3))
* **oberflaeche:** Bausatz für mcp-quiver, Tokens als einzige Quelle für egui und Vue ([bb1c99b](https://github.com/patimweb-labs/mcp-quiver/commit/bb1c99b1dc0a0d98f444904a5001949e4be955d2))
* Protokoll der Tool-Aufrufe und Tool-Prozesse, quiver__protokoll/quiver__tool_log ([ae5df61](https://github.com/patimweb-labs/mcp-quiver/commit/ae5df6197c7e3c2fe50e3e0697cc48bacddf384d))
* Server – Anmeldung über Entra, Root, Sitzungen, Prüfspur ([9fe9205](https://github.com/patimweb-labs/mcp-quiver/commit/9fe92054da077609d4b165a98f193ce54814ad84))
* Server – Entra mit Zertifikat, Gruppen aus Entra ([b87e532](https://github.com/patimweb-labs/mcp-quiver/commit/b87e532b9c33e213d295debb8372d4cea82b80d8))
* **server:** Bibliothek, Zuweisung, Geräte mit Einmal-Code, Soll per SSE, Admin-API ([3fdc7f7](https://github.com/patimweb-labs/mcp-quiver/commit/3fdc7f762a35dbbcece5167551cb18117d551b39))
* Tools am Gerät abschalten ([94b343b](https://github.com/patimweb-labs/mcp-quiver/commit/94b343bea00d3c8cc33eddc13c62e2dfa6ff5aad))
* Verwaltung – Benutzer, Prüfspur und Besitzer der Geräte ([5b0f19a](https://github.com/patimweb-labs/mcp-quiver/commit/5b0f19ac6d1c0267498f485e16628de02e93ecdb))
* Verwaltung – Ersteinrichtung und Anmeldung über Sitzung ([ddb2589](https://github.com/patimweb-labs/mcp-quiver/commit/ddb258972ea5adf873890ab4d69a7919f841b4b8))


### Behoben

* Beenden per Signal auch bei weggelegtem Fenster ([e845a58](https://github.com/patimweb-labs/mcp-quiver/commit/e845a58788d3ca4deea6d624f6075ec639bdd99c))
* Benutzerliste übersichtlich, kleine Merkmale, Aktionen im Menü ([38274dd](https://github.com/patimweb-labs/mcp-quiver/commit/38274dd5c84dd1fefded25a45235fc3e4e9c38f3))
* Client – Grenzen, Fristen, Tresor und Protokoll gehärtet ([add7bf3](https://github.com/patimweb-labs/mcp-quiver/commit/add7bf3eea48eabd075d8a4a6b929ee2b7aa6524))
* Client kompiliert auf Linux und Windows, Port der Instanz in eigener Datei ([80a9bb0](https://github.com/patimweb-labs/mcp-quiver/commit/80a9bb0005da066e6197072288fe19bedb1d44bf))
* **client:** Doppelstart verliert keine Tool-Liste mehr; Einmal-Token und Geheimnisse im Log maskiert ([9697aa6](https://github.com/patimweb-labs/mcp-quiver/commit/9697aa6f6f6f9b31fe93a70d0443015dc8259d91))
* **client:** TLS für entfernte MCP-Server (rmcp-Feature reqwest) ([60b26ae](https://github.com/patimweb-labs/mcp-quiver/commit/60b26ae0ecddaefc286884fe5171534a8071ce11))
* Dock-Icon auf macOS so groß wie andere Apps ([ffcf2ac](https://github.com/patimweb-labs/mcp-quiver/commit/ffcf2ac308a0b2c5a73f0a224ec79cf39f8f9739))
* Eingabeschema immer als Objekt (Hermes lehnte die Tool-Liste ab) ([075f555](https://github.com/patimweb-labs/mcp-quiver/commit/075f5554617d0720540d1f679a57b590ff153489))
* **fenster:** Feldliste der Dialoge scrollt, Dialog bleibt im Fenster ([0d4251e](https://github.com/patimweb-labs/mcp-quiver/commit/0d4251e169d7ee3e4c8c71f8c9019798a3999f4f))
* **fenster:** Hermes über 'hermes mcp add --auth header' koppeln ([927c353](https://github.com/patimweb-labs/mcp-quiver/commit/927c35362675c860b0cf0f6d1d0b3c1ae1b6cfda))
* **fenster:** Hilfe-Link öffnet den Browser (eframe-Merkmal links) ([34827dd](https://github.com/patimweb-labs/mcp-quiver/commit/34827dd28ae1318f41c8c8d44bf5b50c47e06429))
* **oberflaeche:** Kopier-Knopf und Hinweis-Icon vertikal mittig zum Text ([a001dc4](https://github.com/patimweb-labs/mcp-quiver/commit/a001dc49d1d0a25f4bc7e68b0ba8d559c2d388fc))
* Rückstand im Ereignis-Strom, Drossel ohne unterscheidbare Adresse, Einrichtung im Container ([1665ddf](https://github.com/patimweb-labs/mcp-quiver/commit/1665ddfc5bfdbed2f1684a741beb6ffa7d2bf542))
* Schlüsselbund ganz entfernt – kein Umzug, keine Dialoge mehr ([7a05929](https://github.com/patimweb-labs/mcp-quiver/commit/7a05929671444fe8bbe8d12bff506469e1a705d0))
* Server – Grenzen, Drossel, Ereignis-Strom und Einrichtung gehärtet ([4691f5c](https://github.com/patimweb-labs/mcp-quiver/commit/4691f5cd7155a201a33031cda2fd1c05eb40e873))
* Server, Image und Dateirechte gehärtet ([78f0c02](https://github.com/patimweb-labs/mcp-quiver/commit/78f0c02e8c4d1824caa546850634a85ee6e81b22))
* Zugangsnamen mit Großbuchstaben (z. B. SAP-System BL4_900) ([4e460a0](https://github.com/patimweb-labs/mcp-quiver/commit/4e460a0fcc744fcbbd1285ddb0829c72f8251711))


### Dokumentation

* ADR 0002 und Aufgaben 20–22 – Anmeldung über Entra, Root, kein Admin-Schlüssel ([23a91a2](https://github.com/patimweb-labs/mcp-quiver/commit/23a91a2a1ab806bfcb6680b43dd6170444aaa947))
* Aufgabe 07 – egui und Vue angleichen ([eeec61f](https://github.com/patimweb-labs/mcp-quiver/commit/eeec61fc9cfaa5409f6efbb451d37c36cb8f7dfa))
* Aufgabe 09 – Agenten nur einmal koppeln, Schnipsel je Agent, Einträge abschalten ([ab1c6ef](https://github.com/patimweb-labs/mcp-quiver/commit/ab1c6ef8cf49f41283966614ac645f7a5263570d))
* Aufgabe 10 – Anmeldung von Tools im Client-Fenster ([6cba990](https://github.com/patimweb-labs/mcp-quiver/commit/6cba990ed4db1af69eb1ee2f5c163ed4594a2a63))
* Aufgabe 12 – Doppelstart verliert Tool-Liste, Einmal-Token im Log ([2e272c8](https://github.com/patimweb-labs/mcp-quiver/commit/2e272c88e5a6b3d0ca33beea8186dba8ca2998d8))
* Aufgabe 19 – Einträge am Gerät abschalten ([434a05a](https://github.com/patimweb-labs/mcp-quiver/commit/434a05ac7be87c4b903f83c61836fef473b9e1b1))
* Aufgabe 22 – Ende des Clients mit Ursache protokollieren ([2616bf7](https://github.com/patimweb-labs/mcp-quiver/commit/2616bf77269fa36dda9762d2d40217b182629da8))
* Aufgabe 22 – Seite Verbindung mit Entra-Anmeldung ([95cc019](https://github.com/patimweb-labs/mcp-quiver/commit/95cc019b0f4919045679e117f1f4c09327d8d407))
* Aufgabe 23 – Entra-Anmeldung mit Zertifikat ([c2c1982](https://github.com/patimweb-labs/mcp-quiver/commit/c2c198256bef59bc286a41f7a7432cbcc730705c))
* Aufgabe 23 – Entra-Gruppen von selbst anlegen ([1b6dfcf](https://github.com/patimweb-labs/mcp-quiver/commit/1b6dfcf7c3d669f9baeab660caa1b9420c4651fc))
* Aufgabe 24 – Rollen Root, Admin, Mitarbeiter ([fb9625c](https://github.com/patimweb-labs/mcp-quiver/commit/fb9625cc9767f3f2dfc4c100b073d4a4f281a5f4))
* Aufgabe 25 – Ersteinrichtung im Browser ([b146e9e](https://github.com/patimweb-labs/mcp-quiver/commit/b146e9e3203da24aaadcf913c85a3a4e005ceecf))
* Aufgabe 26 – Befunde der Abnahme ([6278263](https://github.com/patimweb-labs/mcp-quiver/commit/6278263adc7c107335bb0fafd29848b0b85c6cd2))
* Aufgabe 27 – Konsolen-Client mit eigenem Setup und Betrieb ([03bbad9](https://github.com/patimweb-labs/mcp-quiver/commit/03bbad9fdc57da1194d9e0d6024fa47be7648820))
* Aufgabe 27 – quiver-cli nur als Docker-Image ([9bf4925](https://github.com/patimweb-labs/mcp-quiver/commit/9bf4925a67b71f1db9a6519061c8e1197ff28b6c))
* Aufgabe 27 – zusätzlicher Konsolen-Client, Fenster bleibt ([25f44ac](https://github.com/patimweb-labs/mcp-quiver/commit/25f44ac1846ac3f77b0f763d1367fd85b8ed2c89))
* Aufgabe 27 (Entwurf) – Client für Konsole und Docker ([3694bf3](https://github.com/patimweb-labs/mcp-quiver/commit/3694bf3a9d6a911c8b7b3f1e98acf201fd87f848))
* Aufgabe 29 – Härtung und Prüfung der Oberfläche nach Entra ([0a71c42](https://github.com/patimweb-labs/mcp-quiver/commit/0a71c42e5ad488fe8336636bc6f9e8d5d260237b))
* Aufgabe 29 – Security-Durchgang als Pflichtteil ([4b6a460](https://github.com/patimweb-labs/mcp-quiver/commit/4b6a46065237478db8bbe21bc08a26b22004e8c2))
* Aufgabe 30 – Namen der Entra-Gruppen über Graph ([7bde785](https://github.com/patimweb-labs/mcp-quiver/commit/7bde785c7a737eae3e17ef0617fc3174c6089ad8))
* Entra einrichten – ausführliche Anleitung de/en, Fehlerhilfe, CLI ([de0c8f0](https://github.com/patimweb-labs/mcp-quiver/commit/de0c8f0ca3aa82b8383e49ddcd695ad46978f018))
* Konzept, Spezifikation und Arbeitsanweisung für mcp-quiver ([3b88648](https://github.com/patimweb-labs/mcp-quiver/commit/3b886486f75c7bc21ba70d71ac3f4369adc98d98))
* MVP-Umfang in der SPEC, Aufgaben 02–06; ci: Prüfen auf drei Systemen ([ccc7b33](https://github.com/patimweb-labs/mcp-quiver/commit/ccc7b33009fde0ecf4039da11e3c295f290c807c))
* Nachtrag zu Aufgabe 05 ([19131aa](https://github.com/patimweb-labs/mcp-quiver/commit/19131aa450335a2b9d47afb560f96e167d32f4d9))
* offene Punkte ergänzt ([0b834d8](https://github.com/patimweb-labs/mcp-quiver/commit/0b834d8af024c00c2eab5c65d9515630a9507a2f))
* Produktseite, release-please und sichere Passworteingabe ([65ba3e4](https://github.com/patimweb-labs/mcp-quiver/commit/65ba3e4a95692c757d58026b2eac4e9c18e8b844))
* Server per Docker starten, Projektstand ([6b1d028](https://github.com/patimweb-labs/mcp-quiver/commit/6b1d02853dcf786a53a100a037a023e76f7549d6))


### Bau und Pakete

* Argon2 und RSA auch in Tests optimiert ([7ad196f](https://github.com/patimweb-labs/mcp-quiver/commit/7ad196f75dc0321ed4b89090655e3608971ea983))
* Container-Image für den Server (Admin-Oberfläche eingebaut) ([90f5cf5](https://github.com/patimweb-labs/mcp-quiver/commit/90f5cf5e9102ec086583f5376924b00bf94802ca))

## 0.1.0 (2026-11-25)

### Neu

- **Server** `quiver-server`: Bibliothek auf Basis des `server.json`-Formats,
  Sätze mit Priorität, Gruppen (lokal und aus Entra ID), Geräteabgleich über
  SSE, Admin-API, Prüfspur, verschlüsselte Refresh-Token. Ein Container, ein
  Port, ein Volume; distroless, ohne Shell, mit Grenzen für Rumpf, Zeit,
  Verbindungen und Anmeldeversuche.
- **Verwaltung** (Vue): Überblick, Bibliothek mit Import, Sätze, Gruppen,
  Benutzer, Geräte und Prüfspur; Ersteinrichtung und Anmeldung über den Root
  oder Microsoft Entra ID; dieselben Tokens und Bausteine wie das
  Client-Fenster.
- **Client** `quiver`: Anmeldung des Geräts über Entra oder Einmal-Code,
  Abgleich des Soll-Zustands, stdio- und entfernte MCP-Tools, ein MCP-Endpunkt
  unter `127.0.0.1:8741/mcp`, Koppeln von pi, Claude Code, OpenCode, GitHub
  Copilot und Hermes, Tresor-Datei für Geheimnisse, Export und Import der
  Angaben, Protokoll lokal mit Maskierung.
- **Client-Fenster** (egui): Verbindung, Tools, Agenten und Protokoll; Tray und
  Autostart; Einrichten und Anmelden je Tool; mehrere Zugänge je Eintrag;
  Abschalten auf einem Gerät; drei Farbschemata, Deutsch und Englisch.
- **Microsoft Entra ID** (optional): Zertifikat (`private_key_jwt`) oder
  Geheimnis, Gruppen aus dem Token, Gruppennamen über Graph auf Wunsch,
  Zertifikatswechsel ohne Ausfallzeit.
- **Pakete** für macOS (arm64 und Intel), Windows (MSI und portables ZIP) und
  Linux (deb, rpm, AppImage, tar.gz), dazu `SHA256SUMS` und
  Herkunftsnachweis je Release; Signieren und Notarisieren, wenn die Geheimnisse
  gesetzt sind.
- **Release-Weg** über release-please: Tag und Bau im selben Workflow, weil ein
  von `GITHUB_TOKEN` erzeugter Tag keine weiteren Workflows auslöst; der Tag
  von Hand und der reine Testbau bleiben möglich.
