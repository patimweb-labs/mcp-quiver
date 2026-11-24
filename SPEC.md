# SPEC – mcp-quiver

> Stand: Entwurf 0, 2026-11-23. Jede Festlegung mit Begründung und Preis.
> Offene Punkte stehen in §11 und werden als ADR in `docs/adr/` entschieden.

## 0. MVP (Stand 2026-11-23)

Ziel: das Grundkonstrukt mit schöner Oberfläche und Funktion. **Ohne Azure.**

- Der Weg ohne Azure bleibt vollständig: Gruppen liegen lokal auf dem Server,
  Geräte binden sich mit Einmal-Codes ein. Aufgabe 20 ergänzt den zweiten Weg
  über Entra ID; er ist optional und ändert den Code-Weg nicht.
- Gruppen liegen lokal auf dem Server (Admin-Oberfläche), nicht in Entra ID.
- Geräte binden sich mit einem Einmal-Code ein, den der Admin erzeugt (legt die Gruppen fest).
  Der Code gilt einmal – befristet (Minuten) oder ohne Ablauf, bis ein Gerät ihn einlöst.
- **Ohne Anmeldung kein Tool:** Der Client startet Tools erst, nachdem der Server das Gerät
  in diesem Lauf angenommen hat (erster erfolgreicher Abgleich); der gespeicherte Stand
  läuft beim Start nicht an. Lehnt der Server das Gerät ab (401/403 – entfernt oder
  gesperrt), stoppt der Client alle Tools, verwirft das Geräte-Token und bittet ums
  Neu-Einbinden. Der Ereignis-Strom eines entfernten Geräts endet sofort.
- Die Verwaltung meldet sich mit einer Sitzung an (Root per E-Mail/Passwort oder
  Entra mit Rolle `admin`); einen Admin-Token gibt es seit Aufgabe 20 nicht mehr.
- Laufzeiten im MVP: `stdio` (eigene Binaries, `npx`, `uvx`, `docker`) und `remote`. WASM folgt.
- **Abnahmekriterium:** Ein zugewiesenes MCP ist höchstens 5 s später beim Agenten nutzbar
  (Server meldet per SSE, Client gleicht ab und startet, Agenten bekommen `tools/list_changed`).
- Entra ID, Azure-Rollen und Signaturen kommen nach dem MVP; das Datenmodell hält dafür Platz
  (Zuweisungsquelle als Trait, Herkunft und Freigabe am Eintrag).

Umsetzungspakete: `docs/aufgaben/`.

## 1. Zweck

Eigene und fremde MCP-Tools **einmal** bauen und an **jeden** Agenten auf
**jedem** Gerät verteilen, ohne dass sich ein Agent um Installation,
Konfiguration oder Anmeldung kümmern muss.

Zielgruppe: ein Team oder eine Firma mit Microsoft Entra ID und mehreren
Agenten (pi, Hermes, später weitere) auf macOS, Windows und Linux.

## 2. Bausteine

| Baustein | Aufgabe | Technik |
|---|---|---|
| `quiver-server` | Bibliothek, Zuweisung, Verteilung, Admin-API | Rust, ein Container |
| Admin-Oberfläche | Bibliothek pflegen, Sätze zuweisen, Geräte sehen | Vue, wird vom Server ausgeliefert |
| `quiver` (Client) | Dienst auf dem Gerät: installiert, konfiguriert, führt aus, ein MCP-Endpunkt | Rust |
| Client-Fenster | Status, Anmeldung, fehlende Werte ergänzen, Agenten koppeln, Tray | Rust, egui |
| `quiver-agent-sdk` | Verbindung Agent → Client für Plugins | Rust |
| Plugin pi | dünne Hülle um das SDK – pi 1.5 kann MCP selbst (`pi mcp`), das Plugin ist erst für mehr als MCP nötig | Rust, `napi-rs` |
| Plugin Hermes | dünne Hülle um das SDK | Rust, `PyO3`/`maturin` |
| Tool-Satz Microsoft | Microsoft 365 über Graph | Rust |

**Festlegung:** Der Server ist **nur** Bibliothek und Steuerung. Er führt
keine Tools aus.
*Begründung:* einfach, keine Zugangsdaten der User auf dem Server; fällt der
Server **nach** der Anmeldung aus, laufen die Tools weiter. *Preis:* Systeme, die das Gerät nicht erreicht, sind nicht anbindbar.
Das Datenmodell lässt Platz für `ausfuehrung = zentral` (später, eigenes ADR).

## 3. Ablauf

```
1. Gerät einbinden   Client → Browser → Server: Entra-Login, Einmal-Code → Geräte-Token
2. Soll berechnen    Server: Gruppen des Users → Sätze → Einträge + Werte
3. Abgleich          Client: Soll vs. Ist → installieren / aktualisieren / entfernen
4. Vervollständigen  Client fragt nur fehlende client-/geheim-Felder ab
5. Bereitstellen     Client: ein MCP-Endpunkt mit allen erlaubten Tools
6. Aufruf            Agent → Client → Tool (Zugangsdaten fügt der Client ein)
```

Der Client holt den Soll-Zustand beim Start und hält danach eine Verbindung,
über die der Server Änderungen ankündigt (Server-Sent Events). Fällt sie aus,
fragt er in festen Abständen nach. Ohne Server läuft der letzte bekannte
Stand weiter.

## 4. Bibliothek

**Grundlage:** das `server.json`-Format der offiziellen MCP Registry.
*Begründung:* Standard, fremde MCPs lassen sich aus der Registry übernehmen.
*Preis:* eigene Felder müssen unter `_meta` in einem eigenen Namensraum liegen.

```
Eintrag = server.json
 ├─ Standard: name, version, packages/remotes, Variablen, Argumente
 └─ _meta["io.github.patimweb-labs.quiver/eintrag"]
     ├─ herkunft:   eigen | fremd
     ├─ laufzeit:   wasm | nativ | npx | uvx | oci | remote
     ├─ rechte:     Netz-Ziele, Dateipfade, Prozesse   (Sandbox, Freigabe)
     ├─ anmeldung:  keine | entra-delegiert(scopes) | geheimnis(feld)
     └─ felder:     je Konfig-Feld: quelle = server | client | geheim
```

- **eigen:** von uns in Rust gebaut, signiert vom Server.
- **fremd:** übernommen (Registry, npm, PyPI, OCI, Remote-URL). Ein Admin muss
  jede Version ausdrücklich freigeben.
- **Versionen und Kanäle:** jeder Eintrag hat Versionen und optional Kanäle
  (`stabil`, `test`). Ein Satz verweist auf eine Version oder einen Kanal.

## 5. Zuweisung

```
Zuweisungsquelle ──► Satz ──► Einträge (Version/Kanal) + vorbelegte Werte
```

- **Satz:** benannte Sammlung von Einträgen, z. B. „Microsoft 365 Basis“.
- **Zuweisungsquelle:** ein Trait. Die erste Umsetzung sind **Entra-ID-Gruppen**.
  Später kommen App-Rollen und Azure-Rollen dazu.
- **Festlegung:** Firmenspezifische Abbildungen (welche Firmenrolle welchen
  Satz bekommt) liegen in der Server-Konfiguration des Betreibers, **nicht**
  in diesem Repo.
- Ein User bekommt die Vereinigung aller Sätze seiner Gruppen. Widersprechen
  sich vorbelegte Werte, gewinnt der Satz mit der höheren Priorität (Feld am
  Satz). Gleichstand ist ein Fehler in der Admin-Oberfläche, kein Zufall.

## 6. Konfiguration in Schichten

```
Standard des Eintrags  →  Werte des Satzes (Admin)  →  Werte am Gerät (User)
```

| Quelle | Wer füllt | Wo gespeichert |
|---|---|---|
| `server` | Admin | Server, wird mitgepusht |
| `client` | User am Gerät | Client-Konfiguration |
| `geheim` | User am Gerät | Tresor-Datei des Clients, verlässt das Gerät nie |

**Festlegung: Sicherung der Geräte-Angaben.** Export und Import schreiben eine
JSON-Datei (`format: mcp-quiver/einstellungen`, `version: 1`) mit den Werten und
Zugangslisten je Eintrag; mit Passwort zusätzlich die Geheimnisse aus dem
Tresor, verschlüsselt mit XChaCha20-Poly1305 unter einem Schlüssel aus Argon2id
(m = 64 MiB, t = 3, p = 1, Salz 16 Byte, Passwort mindestens 8 Zeichen). Die
Datei wird mit den Rechten 0600 geschrieben. Geräte-Token, Agent-Tokens und die
Anmeldungen der Tools selbst sind nie enthalten — sie gehören zum Gerät bzw. zur
Anmeldung. Der Import mischt statt zu ersetzen, überspringt Einträge, die das
Gerät nicht kennt, und startet die betroffenen Einträge neu. Die Wahl „auf
diesem Gerät abgeschaltet“ (`am_geraet_aus`, Abschnitt 9) steht mit in der Datei
und in der Vorschau; ein Import schaltet damit aus, aber nie ein.

*Preis:* eine zusätzliche Abhängigkeit (`argon2`) und ein Dateiformat, das
versioniert bleibt. Wer das Passwort vergisst, kommt an die Geheimnisse der
Sicherung nicht mehr heran; ohne Passwort ist die Datei offen lesbar.

## 7. Authentifizierung

1. **Agent → Client:** Der Client lauscht nur lokal. Jeder Agent bekommt beim
   Koppeln im Client-Fenster ein eigenes Token. So weiß der Client, welcher
   Agent fragt, und kann Tools pro Agent filtern.
2. **Client → Server, mit Entra:** Der **Server** ist der einzige OIDC-Client
   (eine Anwendung, ein Mandant, Authorization Code + PKCE; Client-Geheimnis
   **oder** Zertifikat nur am Server, Aufgabe 23). Der Client spricht nie mit
   Entra: er öffnet die Anmeldung des Servers im Browser, bekommt über
   `127.0.0.1` einen Einmal-Code und tauscht ihn mit seinem PKCE-Verifier
   gegen ein Geräte-Token. Identität ist `(tid, oid)`; E-Mail und Name sind
   nur Anzeige. Gruppen kommen als Objekt-Ids aus dem ID-Token; jede gesehene
   Id legt **einmal** eine lokale Gruppe an (ohne Sätze), Titel und Sätze
   gehören dem Admin.
3. **Client → Server, ohne Entra:** Einmal-Code aus der Verwaltung. Beide Wege
   enden in derselben Gerätebindung; Geräte lassen sich sperren.
   **Client-Anmeldung (Aufgabe 22):** Die Verbindungsseite fragt zuerst nur die
   Adresse (`https`; `http` nur Loopback), merkt sie sich über das Abmelden
   hinaus und fragt hinter „Weiter“ `GET /api/v1/anmeldearten`. Danach zeigt sie
   **entweder** „Mit Microsoft anmelden“ **oder** das Code-Feld, nie beides.
   Der Browser-Lauscher liegt nur auf `127.0.0.1`, Pfad `/quiver`, freier Port,
   fünf Minuten, genau eine Antwort; `zustand` wird in konstanter Zeit
   verglichen, der Code mit dem eigenen PKCE-Verifier getauscht. Ein vorhandenes
   Geräte-Token geht als `altes_token` mit (dasselbe Gerät bleibt).
4. **Verwaltung:** Root (genau einer, E-Mail und Passwort, Argon2id) wird bei
   der **Ersteinrichtung im Browser** angelegt (`POST /auth/einrichten`), sonst
   über die Kommandozeile; Entra-Benutzer mit Rolle `admin`. Sitzung als
   HttpOnly-Cookie, 12 h absolut, 60 min ohne Nutzung. Ändernde Anfragen
   brauchen `Origin` = Basis-URL **und** `X-Quiver: 1`; keine CORS-Freigabe.
   Rechte werden bei jeder Anfrage frisch aus der Datenbank gelesen. Einen
   Admin-Schlüssel gibt es nicht mehr.
5. **Nachprüfen statt einmal glauben:** Das Refresh-Token jedes Benutzers liegt
   verschlüsselt am Server (XChaCha20-Poly1305, Schlüssel `<daten>/schluessel`,
   AAD `(tid, oid)`) und wird stündlich mit Streuung eingelöst, beim Verbinden
   eines Geräts nach mehr als 60 Minuten und auf Knopf. Rotation wird sofort
   gespeichert. `invalid_grant`/`interaction_required`/fehlende Zuweisung →
   neu anmelden; Netz-/5xx-Fehler → letzter Stand, höchstens 24 h.
6. **Tool → Dienst:** Für Microsoft holt der Client Tokens für die Scopes des
   Eintrags (`entra-delegiert`) mit dem Login des Users. Microsoft prüft die
   Rechte, nicht wir. Das Tool bekommt das Token pro Aufruf und speichert es
   nicht.
7. **Ersteinrichtung (Aufgabe 25):** Gibt es keinen Root, zeigt die Verwaltung
   statt der Anmeldung die Ersteinrichtung. `GET /auth/stand` (offen) sagt nur
   `{ eingerichtet, einrichtung, code_noetig, entra }`. `POST /auth/einrichten`
   legt **genau einen** Root an (eine Transaktion mit Prüfung „kein Root“:
   gleichzeitige Aufrufe ergeben einmal 200 und sonst 409), meldet ihn an und
   löscht `<daten>/admin-token`; danach antwortet der Weg mit 404. Das Fenster
   ist **15 Minuten nach dem Start** offen und hängt an der Laufzeit des
   Prozesses, nicht an der Wanduhr. Ist die Basis-URL nicht Loopback, ist ein
   Einmal-Code Pflicht (128 Bit, nur gehasht im Speicher, genau einmal im
   eigenen Log, konstante Zeit, nach zehn Fehlversuchen geschlossen). `Host`
   und `Origin` müssen exakt die Basis-URL sein, dazu `X-Quiver: 1`. Vor der
   Einrichtung ist die Admin-API ganz gesperrt; Geräte laufen weiter. Ohne
   `QUIVER_OEFFENTLICH` gilt `http://localhost:<Port>`.
8. **Sitzungen folgen dem Benutzer (Aufgabe 26):** Die Wachen prüfen bei
   Entra-Benutzern auch `token_stand`; `neu_anmelden`, gesperrt oder gelöscht
   beendet **alle** Sitzungen des Benutzers sofort (löschen, nicht nur
   ablehnen). Ist die letzte Nachprüfung länger als 60 Minuten her, prüft die
   Wache vor dem Durchlassen nach. Die Discovery gilt höchstens 24 Stunden;
   schlägt das Neuholen fehl, gilt der alte Stand weiter (Warnung im Log).
9. **Seiten des Servers (Aufgabe 26):** Bestätigung und Fehler sind eigene
   Seiten mit eigenem Stylesheet (`/auth/stil.css` aus denselben Tokens, dunkel
   als Vorgabe, hell über `prefers-color-scheme`), Logo und Hand-Cursor; Titel
   und Texte deutsch oder englisch nach `Accept-Language`. `Referrer-Policy`
   ist `same-origin` – mit `no-referrer` schickt der Browser beim Formular
   `Origin: null`, und die Bestätigung scheitert. Nur die Bestätigungsseite
   bekommt `form-action 'self' http://127.0.0.1:<Port>` mit genau dem Port des
   Vorgangs, weil Chrome auch die Weiterleitung nach dem Absenden prüft.

10. **Zertifikat statt Geheimnis (Aufgabe 23):** Der Server weist sich bei
    Entra wahlweise mit `private_key_jwt` aus: PS256, Kopf `x5t#S256`, `aud` =
    Token-Endpunkt, `iss` = `sub` = Client-Id, zufällige `jti`, `nbf`/`exp`
    höchstens fünf Minuten; je Anfrage neu, bei Code-Einlösung und Refresh.
    Konfiguriert ist **genau eins** von Schlüssel und Geheimnis; beides oder
    eine Lücke verhindert den Start. Der Unterbefehl `entra-schluessel` erzeugt
    RSA 3072 und ein selbst signiertes Zertifikat (ein Jahr), legt den privaten
    Schlüssel mit 0600 an, überschreibt nie und gibt nur das öffentliche
    Zertifikat, Fingerabdruck und Ablauf aus. `--erneuern` legt einen zweiten
    an, `--umschalten` macht ihn ohne Neustart und ohne Ausfall aktiv. Ab 30
    Tagen vor dem Ablauf wird im Log, in der Verwaltung und in der Prüfspur
    gewarnt (je Tag einmal). Der Schlüssel erscheint nie in Log, Fehler,
    Antwort, Prüfspur oder `Debug`.
11. **Gruppen aus Entra (Aufgabe 23):** Höchstens 200; jede Id einmal, ohne
    Sätze; Titel und Sätze gehören danach dem Admin und werden nie
    überschrieben. Löscht der Admin eine solche Gruppe, bleibt sie gelöscht
    (Id gemerkt), bis er sie wieder aufnimmt. Mitgliedschaften pflegt nur
    Entra: Anmeldung und jede Nachprüfung ersetzen sie, auch das Entfernen,
    und wirken sofort auf verbundene Geräte.
12. **Namen der Entra-Gruppen (Aufgabe 30):** Rechte laufen weiter **nur** über
    die Objekt-Id; der Name ist Anzeige. Nur mit `QUIVER_ENTRA_GRUPPENNAMEN=an`
    (Standard `aus`, jeder andere Wert ist ein Startfehler) kommt
    `https://graph.microsoft.com/GroupMember.Read.All` zu den Bereichen von
    Anmeldung **und** Refresh – ohne Zustimmung des Mandanten scheiterte sonst
    jede Anmeldung. Für die Gruppen aus dem ID-Token, deren Name fehlt oder
    älter als 24 h ist, holt der Server im Hintergrund `displayName`
    (`GET /v1.0/groups/{id}`; höchstens 20 je Lauf, 4 gleichzeitig, 5 s je
    Anfrage, 64 KB, keine Weiterleitungen, Host fest – Loopback nur in Tests
    über `QUIVER_ENTRA_GRAPH`) mit dem Zugriffstoken **dieses** Benutzers. Das
    Zugriffstoken lebt nur in diesem Lauf: nie in der Datenbank, im Log, in
    `Debug` oder in der Prüfspur. Anmeldung und Nachprüfen warten nie darauf,
    ein Fehler ändert nichts (Log höchstens stündlich), ein 403 pausiert einen
    Tag. Der Titel der quiver-Gruppe folgt dem Namen nur, solange er der
    automatische („Entra-Gruppe xxxxxxxx“) oder der zuletzt übernommene ist;
    einen Titel des Admins überschreibt der Server nie, und jede Übernahme
    steht in der Prüfspur. Die Verwaltung zeigt den Titel, klein darunter
    „Entra: <Name>“, wenn beide abweichen; die Id steht nur im Hinweis und in
    den Einzelheiten.

**Festlegung:** Zugangsdaten werden nie als Umgebungsvariable oder Datei
abgelegt, die ein Agent lesen könnte. Fehler an ein Gerät tragen einen
maschinenlesbaren `grund` (`gesperrt`, `neu-anmelden`, `kein-zugriff`);
unbekannte Gründe gelten als `neu-anmelden`. Daraus folgt im Client:
`gesperrt`/`kein-zugriff` → alle Tools aus, Token weg, klare Meldung;
`neu-anmelden`/unbekannt → alle Tools aus, **Geräte-Token und alle Angaben
bleiben**, Knopf „Erneut anmelden“ und Hinweis im Tray; 401 (Token ungültig
oder Gerät entfernt) → wie bisher neu einbinden, lokale Angaben bleiben.
„Abmelden“ löscht das Gerät **am Server** (`POST /api/v1/geraet/abmelden`) und
behält Adresse und Angaben; ist der Server nicht erreichbar, sagt der Client
das, meldet lokal aber trotzdem ab. Jedes Ende des Clients steht mit Ursache im
Log (Tray, Fenster, Cmd+Q, Strg+C, SIGTERM, zweite Instanz, Systemabmelden);
Cmd+Q und das Schließen legen nur in den Tray, beendet wird über das Tray.
Agenten können keine Anmeldung, kein Abmelden und kein Beenden auslösen.

*Preis:* Die Anmeldung ist ein zusätzlicher Zustand im Client (Adresse,
Anmeldewege, Browser warten, neu anmelden, Fehler), und ein laufender
Browser-Vorgang hält einen Port auf `127.0.0.1` fünf Minuten offen. Der Client
muss den Server nach `neu-anmelden` fragen, bevor er das Token wegwirft — das
kostet einen Weg, verhindert aber, dass eine Gruppe oder ein Zertifikat
versehentlich alle lokalen Angaben mitnimmt. Der Server ist nun auch
Identity-Provider-Abhängigkeit: ohne Entra
gilt der letzte Stand höchstens 24 h. Ein Gruppen-Überlauf im Token wird
abgelehnt statt als „keine Gruppen“ gelesen. Ein selbst signiertes Zertifikat
muss der Betreiber vor dem Ablauf erneuern; der Server warnt 30 Tage vorher,
aber erneuern kann er es nicht von selbst. Ein zweiter Leserecht-Grad
(Graph) ist eine zusätzliche Zustimmung des Mandanten; ohne sie bleibt der
Schalter aus, und in der Verwaltung steht dann nur der automatische Titel. Die
Seiten des Servers sind
schmucklos (kein Inline-Skript, keine Inline-Stile), damit die CSP ohne
Ausnahme gilt; sie tragen dieselben Tokens wie Verwaltung und Client, aber
nicht deren Bausteine. Die Ersteinrichtung ist ein Weg, der nur in den ersten
15 Minuten eines Serverlaufs offen ist – wer sie verpasst, startet neu. Der
Einmal-Code für Nicht-Loopback steht im Log; wer das Log lesen darf, kann den
Server übernehmen, und genau das ist gewollt (Aufgabe 25).

## 8. Laufzeiten im Client

| Laufzeit | Einsatz | Abschottung |
|---|---|---|
| `wasm` | bevorzugt für eigene Tools | wasmtime-Sandbox, nur freigegebene Rechte |
| `nativ` | eigene Tools, die mehr brauchen | Freigabe durch Admin, Signatur geprüft |
| `npx` / `uvx` / `oci` | fremde MCP-Server | Freigabe durch Admin, Node/Python/Container nötig |
| `remote` | entfernte MCP-Server über HTTP | Client fügt Zugangsdaten ein |

Gepushte Artefakte sind signiert. Der Client prüft jede Signatur vor dem Start.

## 9. Oberfläche

- **Client:** egui, Bausteine und Themes wie in crosspoint, wledambilight und
  sprechwerk (`dark`, `light`, `mono`).
- **Admin:** Vue mit `weppelmann-ui`.
- **Festlegung: 1:1 gleiches Aussehen.** Beide Oberflächen erzeugen ihre
  Themes aus **denselben** Token-Dateien (`tokens/*.tokens.json`). Bausteine
  gibt es in beiden Welten mit gleichem Namen und gleichem Maß.
  Bildschirmfotos beider Seiten werden in der CI nebeneinander erzeugt.
- **Festlegung: Hand-Cursor auf allem Klickbaren.** In egui ist das nicht
  Standard. Darum setzen die Bausteine `CursorIcon::PointingHand` zentral,
  und ein Oberflächentest prüft es. In Vue gilt `cursor: pointer` im Baustein.
- **Festlegung: drei Breitengrenzen als Tokens** (Aufgabe 18). Unter
  `breakpoint.schmal` (760 px) trägt die Seitenleiste nur Zeichen mit Hinweis,
  unter `breakpoint.formular` (700 px) steht die Beschriftung über dem Feld,
  die Kopfzeile legt Farbschema und Sprache in ein Menü und die
  Protokolltabelle lässt *Agent* und *Dauer* weg (beide im Detail). Der Inhalt
  endet bei `size.inhalt-max` (1100 px) und bleibt mittig; Leisten brechen um.
  In einem flachen Fenster (`breakpoint.flach`) ist der Seitenkopf eine Stufe
  kleiner, damit der Inhalt Platz behält. Beide Oberflächen lesen dieselben
  Tokens — egui als Konstanten, Vue über `--wp-*` und `data-breite`.

  *Preis:* Die Grenzen sind fest und nicht einstellbar; zwischen 700 und 760 px
  ist die Seitenleiste breit, obwohl der Inhalt schon knapp ist. Die Stufe einer
  Tool-Log-Zeile wird aus dem Text geraten, weil die Zeile kein Feld dafür hat.
- **Festlegung: Einträge lassen sich am Gerät abschalten** (Aufgabe 19). Jede
  Tools-Karte trägt rechts den Schalter *Auf diesem Gerät aktiv*, Standard an.
  Aus beendet den Prozess, nimmt die Werkzeuge aus `tools/list`, meldet allen
  Agenten `list_changed` und sagt dem nächsten Aufruf, dass der Eintrag
  abgeschaltet ist – nie „unbekannt“. Angaben, Zugänge, Geheimnisse und
  Anmeldung bleiben erhalten; nur *Anmelden* und *Einrichten* sind gesperrt,
  solange er aus ist.

  Die Wahl steht als Menge von Kennungen in `zustand.json`
  (`am_geraet_aus`, `serde(default)`) und geht als neuer Zustand
  `EintragZustand::AmGeraetAus` an den Server, der sie neutral zeigt. Eine
  einzige Stelle (`darf_laufen`) entscheidet, ob ein Eintrag starten darf; alle
  Start-Wege gehen hindurch, auch Abgleich, `werte_setzen`, Anmeldung,
  Einrichtung und Import. Gespeichert wird **vor** dem Schalten: scheitert das
  Schreiben, ist nichts geschaltet. *Aus* bleibt aus, auch über Neustart,
  Serveränderung und Import; eine Sicherung nimmt die Wahl mit und hebt sie nie
  auf. Agenten können nicht schalten; ein MCP-Werkzeug dafür gibt es nicht.
  Der Server-Vorrang bleibt: was am Server entfernt oder deaktiviert wird,
  verschwindet; kommt der Eintrag zurück, gilt die alte Wahl.

  *Preis:* Ein zweiter Zustand neben dem Laufzustand — Modell, Meldung und
  Verwaltung müssen beide lesen (unbekannte Zustände gelten als neutral). Wer
  einen Eintrag auf vielen Geräten abschalten will, muss es auf jedem Gerät tun;
  der Admin-Schalter (*Aktiv*) bleibt die einzige zentrale Steuerung.

## 10. Protokoll und Datenschutz

Der Client führt ein Protokoll der Werkzeug-Aufrufe (Aufgabe 17): Zeit, Agent,
Eintrag, Werkzeug, Dauer, Ergebnis, gekürzte Argumente und eine Vorschau der
Antwort. Dazu kommen die stderr-Zeilen jedes stdio-Eintrags, je Eintrag.

- **Festlegung: lokal, nicht zentral.** Das Protokoll bleibt auf dem Gerät
  (`<profil>/protokoll/*.jsonl`, Rechte 0600, 7 Tage, höchstens 50 MB). Der
  Server bekommt keine Aufrufe; er sieht nur den Zustand der Einträge (Abschnitt
  3). Eine zentrale Auswertung wäre ein eigener Entwurf (Zustimmung, Aufbewahrung,
  Zugriff) und ist nicht Teil des MVP.
- **Festlegung: maskiert.** Schlüssel mit `pass`, `token`, `secret`, `key`,
  `pat`, `auth` oder `cookie` im Namen bekommen `<verborgen>`; jeder Wert aus dem
  Tresor wird ersetzt, auch als Teilstring. Kürzen: 200 Zeichen je Wert, 4 KB
  Argumente, 500 Zeichen Vorschau, 2000 Zeichen Fehlertext.

*Preis:* Wer die Dateien liest, sieht Tool-Namen, Zeiten und Fehlertexte; nach
7 Tagen ist das meiste weg. Ein Geheimnis, das ein Tool **selbst** in eine
stderr-Zeile schreibt und das nicht im Tresor steht, kann nur über den
Schlüsselnamen erkannt werden.

### Was Agenten sehen

Zwei eigene Werkzeuge, `quiver__protokoll` und `quiver__tool_log`, sind lesend
und stehen in `tools/list`. Standardmäßig liefern sie **nur Metadaten** (Zeit,
Agent, Werkzeug, Dauer, Status, Fehlertext). Argumente, Antwort-Vorschauen und
Prozesszeilen gibt es erst, wenn der User im Fenster unter „Agenten“
„Agenten dürfen Inhalte im Protokoll sehen“ einschaltet (in `zustand.json`
gespeichert). Fehlertexte sind auch dann maskiert. Die Aufrufe der beiden
eigenen Werkzeuge landen nicht im Protokoll (keine Rückkopplung).

## 11. Offene Punkte

1. Formate für eigene Tools: nur WASM, oder von Anfang an auch native Binaries?
2. ~~Admin-Rechte: Wer darf die Bibliothek pflegen?~~ Entschieden in Aufgabe 20:
   einzelne Entra-Benutzer mit Rolle `admin`, Root als Notzugang.
3. Datenbank des Servers: SQLite im Volume (wie sprechwerk) oder Postgres?
4. Soll der Client auch ohne Server nutzbar sein (nur lokale Einträge)?
5. pi und Hermes: Schnittstellen der Plugins genau prüfen. pi 1.5 spricht MCP (`pi mcp`), für den MVP reicht das.
6. Erster Umfang des Microsoft-Satzes: Mail, Kalender, Teams, SharePoint/OneDrive, Planner?
7. Gemeinsames UI-Crate: aus den bestehenden Apps herauslösen, bevor der Client entsteht?
