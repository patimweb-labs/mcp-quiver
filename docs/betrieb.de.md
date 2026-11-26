English: [betrieb.md](betrieb.md)

# quiver-server betreiben

Der Server ist Bibliothek und Steuerung; er führt keine Tools aus. Er läuft als
ein Container-Abbild mit einem Port (8740) und einem Volume (`/daten`).

**Inhalt** · [Container](#container) · [Ersteinrichtung](#ersteinrichtung) ·
[Adresse und Host](#adresse-und-host) · [Entra ID](#entra-id) ·
[Programme für die Geräte](#programme-für-die-geräte) ·
[Konfiguration](#konfiguration) · [Reverse-Proxy](#reverse-proxy) ·
[Sicherung](#sicherung) · [Aktualisieren](#aktualisieren) · [Log](#log) ·
[Fehlersuche](#fehlersuche)

## Container

```bash
docker build -t mcp-quiver-server .
docker run -d --name quiver -p 127.0.0.1:8740:8740 \
  -e QUIVER_OEFFENTLICH=http://localhost:8740 \
  --read-only --tmpfs /tmp:size=32m,mode=1777 \
  --cap-drop=ALL --security-opt=no-new-privileges:true \
  --memory 512m --pids-limit 256 \
  --log-driver json-file --log-opt max-size=10m --log-opt max-file=3 \
  -v quiver-daten:/daten mcp-quiver-server
```

`compose.yaml` ist derselbe Aufruf in einer Datei:

```bash
cp .env.example .env     # ausfüllen, keine Geheimnisse ins Repo
docker compose up -d
docker compose logs -f
```

Die Datei nimmt das Abbild aus `QUIVER_IMAGE` (Vorgabe: ein lokaler Bau) und
hält den Port auf Loopback, wo der Proxy mit TLS steht.

Das Abbild ist auf schlanken Betrieb gebaut: Die Laufzeit ist das Programm und
sonst nichts — **keine Shell, keine Paketverwaltung** (distroless), Benutzer
`10001:999`, alle Basis-Abbilder auf einen Digest festgelegt. Im Inneren gibt es
keine Shell, die man ausnutzen könnte.

`tools/container-pruefung.sh` startet genau so einen Container mit eigenem
Namen, eigenem Volume und freiem Port (`quiver-pruefung-*`), prüft die Grenzen,
den Gesundheitsweg, `docker exec`, einen zweiten Start auf demselben Volume und
den Exit-Code nach `docker stop` (0, nicht 137), und räumt ihn wieder ab.

### Was der Container darf

| Eigenschaft | Wert |
| --- | --- |
| Wurzeldateisystem | nicht beschreibbar (`--read-only`), nur `/tmp` ist ein tmpfs |
| Rechte im Kernel | keine (`--cap-drop=ALL`) |
| Aufstieg über setuid | unmöglich (`no-new-privileges`) |
| Speicher / Prozesse | 512 MB / 256 PIDs |
| Log | rotiert (10 MB × 3) |
| Lauschadresse | alle Adressen:8740; die Veröffentlichung hält ihn auf `127.0.0.1` |
| Gesundheit | `quiver-server gesund` fragt den Server nur über Loopback |

### Daten und Rechte

Vorhandene Volumes funktionieren weiter: Der Datenordner gehört `10001:999`
(UID *und* GID), ein Volume aus einer früheren Version öffnet also ohne
`chown` von Hand. Im Volume ist der Datenordner `0700`, und `quiver.db`, `-wal`,
`-shm`, die Sicherungen (`.vor-*`), `entra-*` und `schluessel` sind `0600`; nur
die letzten drei Sicherungen bleiben. Der Prozess setzt seine umask beim Start,
damit gelten diese Rechte ab der ersten Sekunde, und ein Neustart zieht von Hand
verstellte Rechte nach (Symlinks werden nicht verfolgt).

`docker exec -it quiver quiver-server …` geht weiterhin ohne Shell, weil die
Binärdatei in `/usr/local/bin` liegt.

Ohne Docker-Hub-Login bauen mit
`--build-arg BASIS=public.ecr.aws/docker/library`.

## Ersteinrichtung

Die Basis-URL öffnen. Die erste Seite ist die **Ersteinrichtung**: Sie fragt
nach E-Mail und Passwort des einen Root-Benutzers (12 bis 256 Zeichen,
Argon2id) und meldet ihn an. Die Einrichtung ist nur 15 Minuten nach dem Start
des Servers offen, und nur mit einem Einmal-Code, den der Server beim Start
einmal ausgibt — der Code ist Pflicht, sobald der Server auf mehr als Loopback
lauscht (im Container also immer), unabhängig von der Basis-URL:

```
INFO quiver_server::einrichtung: Ersteinrichtung: Code für diesen Start: …
```

Das Fenster folgt der Laufzeit des Servers, nicht der Uhr; ist es zu, den
Server neu starten. Ein vergessenes Passwort wird über die Kommandozeile
ersetzt, und zwar immer über `--stdin`:

```bash
read -rsp 'Neues Passwort: ' P; echo; printf '%s\n' "$P" | \
  docker exec -i quiver quiver-server root-setzen <E-Mail> --stdin; unset P
```

`docker exec -i` ohne `-t`, denn das Abbild ist distroless und hat kein
`stty`: Die Nachfrage kann nicht verbergen, was getippt wird, und statt das
Passwort auf das Terminal zu schreiben, bricht die Binärdatei mit einem
Hinweis auf `--stdin` ab. Die Shell selbst liest das Passwort (`read -rsp`),
so steht es weder in der Kommandozeile noch in der Shell-Historie, und
`unset P` räumt es danach weg. Einen Admin-Token gibt es nicht mehr;
`QUIVER_ADMIN_TOKEN` wird ignoriert und `daten/admin-token` entfernt, sobald
ein Root-Benutzer existiert.

Der Container lauscht immer auf allen Adressen; die Veröffentlichung oben hält
ihn allein auf `127.0.0.1`. Deshalb darf die Basis-URL blankes `http` sein: Mit
allem außer einer https-Basis-URL startet der Server nur, wenn der Host Loopback
ist. Der Einmal-Code bleibt trotzdem Pflicht, denn die Lauschadresse
entscheidet, nicht die Basis-URL. Nur ein ausdrückliches
`QUIVER_EINRICHTUNG=ohne-code` zusammen mit einer Loopback-Basis-URL hebt ihn
auf, und das steht beim Start laut im Log. Wer den Port in den ersten 15
Minuten erreicht, könnte sonst den Root anlegen. Auf `127.0.0.1`
veröffentlichen oder eine `https`-Basis-URL verwenden.

Anmeldung danach: E-Mail und Passwort für den Root und **Mit Microsoft
anmelden**, sobald Entra eingerichtet ist. Die Sitzung liegt in einem
HttpOnly-Cookie (`SameSite=Lax`, `Secure` und `__Host-` bei https, 12 h
absolut, 60 min Leerlauf); im Speicher des Browsers steht nichts.

Ist das Fenster der Ersteinrichtung zu: `docker restart quiver`, dann die Seite
neu laden. Der Code ändert sich mit jedem Start.

## Adresse und Host

`QUIVER_OEFFENTLICH` ist die einzige Adresse, die der Server annimmt. Jede
Anfrage muss einen `Host`-Kopf tragen, der dazu passt — auf allen Wegen, auch
Startseite und Geräte-API. Passt er nicht, antwortet der Server mit **421 ohne
Inhalt**. Einzige Ausnahme ist `GET /gesund`, und nur über Loopback (für den
Gesundheitsweg).

Eine Basis-URL außer `http://localhost…`/`http://127.0.0.1…` muss `https` sein;
sonst startet der Server nicht.

## Entra ID

**Vollständige Anleitung, Klick für Klick und mit der Azure-CLI, samt
Fehlersuche: [entra.de.md](entra.de.md).** Hier die Kurzfassung.

Ohne Entra gibt der Server Einmal-Codes für Geräte in der Verwaltung aus. Mit
Entra melden sich Geräte über den Browser an, und die Gruppen kommen aus dem
Token.

1. In deinem Entra-Mandanten eine Anwendung registrieren (ein Mandant).
   `<Basis-URL>/auth/callback` als Weiterleitungs-URI eintragen (Plattform
   Web). Implizite ID- und Zugriffstoken **aus** lassen.
2. Die App die der Anwendung zugewiesenen Gruppen ausgeben lassen
   (`groups`-Anspruch, nur Objekt-Ids, kein Recht, das Verzeichnis zu lesen).
   Sehr große Gruppenlisten werden absichtlich abgelehnt; dann die Zuweisung
   begrenzen.
3. Eine Identität wählen — **genau eine** von beiden:
   * **Zertifikat (empfohlen, `private_key_jwt`):** `quiver-server
     entra-schluessel` erzeugt einen RSA-3072-Schlüssel und ein selbst
     signiertes Zertifikat, ein Jahr gültig. Es gibt **nur** das öffentliche
     Zertifikat, den Fingerabdruck und das Ablaufdatum aus; dieses Zertifikat
     in Entra bei der Anwendung hochladen (es steht auch unter
     `<Daten>/entra-schluessel.crt.pem`). Der private Schlüssel
     (`<Daten>/entra-schluessel.pem`, Rechte 0600) verlässt den Server nie.
     Kein Client-Geheimnis, das viele Mandanten per Richtlinie verbieten.
   * **Geheimnis:** in `<Daten>/entra-secret` (oder
     `QUIVER_ENTRA_SECRET_DATEI`).
4. Den Server starten mit `QUIVER_OEFFENTLICH=https://quiver.example.test`,
   `QUIVER_ENTRA_TENANT=<Mandanten-GUID>`, `QUIVER_ENTRA_CLIENT_ID=<App-Id>`
   und dem Schlüssel (Vorgabe `<Daten>/entra-schluessel.pem`, oder
   `QUIVER_ENTRA_SCHLUESSEL_DATEI`) **oder** dem Geheimnis. Sind beide da,
   startet der Server nicht; fehlt eines der drei Stücke, auch nicht. Rechte
   offener als 0600 stehen als Warnung im Log.
5. Kein Zuordnungsschritt: Jede im Token gesehene Gruppen-Id legt **einmal**
   eine Gruppe „Entra-Gruppe <erste 8 Zeichen>" an, ohne Sätze – sie gewährt
   also nichts, bis ein Admin in der Verwaltung einen Satz zuweist. Danach
   gehören Titel und Sätze dem Admin; Entra ersetzt nur die Mitglieder. Wer in
   keiner Gruppe ist, bekommt keine Tools. Mit `QUIVER_ENTRA_GRUPPENNAMEN=an`
   holt der Server zusätzlich den Anzeigenamen der Gruppe über Graph
   (Lese-Berechtigung `GroupMember.Read.All`; die App braucht die Zustimmung des
   Mandanten). Der Name ersetzt den automatischen Titel, solange der Admin
   keinen eigenen gesetzt hat, und steht klein unter dem Titel in der
   Verwaltung. Ohne den Schalter ruft der Server Graph nie; so oder so laufen
   die Rechte nur über die Objekt-Id.
6. Root kann Admins ernennen; ein Entra-Benutzer muss sich einmal angemeldet
   haben, bevor `quiver-server admin-setzen --oid <Objekt-Id>` wirkt.

**Zertifikat ohne Ausfallzeit wechseln:** `quiver-server entra-schluessel
--erneuern` legt einen zweiten Schlüssel an (der aktive bleibt unberührt);
`--umschalten` macht ihn aktiv, und der laufende Server nimmt ihn ohne Neustart
auf. Beide Zertifikate sind in Entra registriert, es gibt also keine Lücke. Ab
30 Tagen vor Ablauf warnt der Server im Log, in der Verwaltung und in der
Prüfspur (einmal pro Tag). Vorhandene Schlüssel werden nie überschrieben.

Der Server frischt das Token jedes Benutzers stündlich auf (mit Streuung) und
wenn ein Gerät nach mehr als einer Stunde anfragt. Ist Entra nicht erreichbar,
gilt der letzte Stand höchstens 24 Stunden. Wird ein Benutzer gesperrt, nicht
mehr zugewiesen oder sein Token widerrufen, verlieren seine Geräte beim
nächsten Abgleich die Tools. Löscht ein Admin eine Gruppe, die aus Entra kam,
bleibt sie gelöscht; die Verwaltung kann sie wieder übernehmen. Höchstens 200
Gruppen entstehen so.

## Programme für die Geräte

Der Server führt keine Werkzeuge aus, er hält ihre Programme bereit
(Aufgabe 37). Geräte laden sie mit ihrem Geräte-Token über
`GET /api/v1/geraet/tools/<name>?system=…&arch=…`; ausgeliefert wird nur, was
im Soll **dieses** Geräts steht — ein Entzug wirkt sofort (403).

Zwei Quellen, in dieser Reihenfolge:

| Ordner | Inhalt |
| --- | --- |
| `<Daten>/tools/<system>/` | eigene Programme des Betreibers (gilt für jede Architektur dieses Systems) |
| `<Daten>/tools/<system>/<arch>/` | dasselbe, nur für eine Architektur |
| `/opt/quiver/tools/<system>/<arch>/` | was das Abbild mitbringt (Release von quiver-tools) |

`<system>` ist `windows`, `macos` oder `linux`, `<arch>` ist `x64` oder
`arm64`. Die genauere Angabe geht vor, und was der Betreiber ablegt, geht vor
dem Abbild. In den Ordner gehören die Programme selbst (ausführbar, unter
Windows mit `.exe`) und optional eine `LISTE.json` mit der Fassung:

```json
{"version":"1.2.3","programme":[{"name":"quiver-devops","version":"1.2.3"}]}
```

Größe und SHA-256 rechnet der Server immer selbst aus der Datei — die Liste
liefert nur die Fassung. Ein Programm über 128 MiB wird nicht verteilt. Der
Ordner ist nur für root erreichbar; über die Weboberfläche lässt sich **nichts**
hochladen oder ändern. Was bereitliegt, zeigt `GET /api/v1/admin/programme`
(Verwaltung: je Eintrag das Programm, je Gerät die laufende Fassung aus der
Meldung des Geräts).

Eine geänderte Datei ändert die Revision des Solls; der Server bemerkt sie
innerhalb von 30 Sekunden und stößt die Geräte an. Ein Abbild **ohne**
Programme ist gültig: dann steht beim Start „keine Programme“ im Log, und die
Geräte bekommen nichts (ihre Konfiguration bleibt). Die Fassung steht im
`Dockerfile`; eine andere beim Bauen:

```bash
docker build --build-arg QUIVER_TOOLS_VERSION=0.1.0 -t mcp-quiver-server .
```

Mit leerer Angabe (`QUIVER_TOOLS_VERSION=`) baut es ohne Programme und sagt es.

## Konfiguration

| Variable | Bedeutung |
| --- | --- |
| `QUIVER_OEFFENTLICH` | Basis-URL (`https`, oder `http` nur auf Loopback; Vorgabe `http://localhost:<Port aus QUIVER_ADRESSE>`) |
| `QUIVER_ADRESSE` | Lauschadresse (Vorgabe `0.0.0.0:8740`) |
| `QUIVER_DATEN` | Datenordner (Vorgabe `./daten`) |
| `QUIVER_ADMIN_DIST` | Gebaute Verwaltung (Vorgabe `./admin/dist`) |
| `QUIVER_ENTRA_TENANT` | Mandanten-GUID (nie `common`/`organizations`) |
| `QUIVER_ENTRA_CLIENT_ID` | Anwendungs-Id (Client-Id) |
| `QUIVER_ENTRA_SECRET_DATEI` | Datei mit dem Client-Geheimnis (Vorgabe `<Daten>/entra-secret`); genau eines von Geheimnis und Schlüssel |
| `QUIVER_ENTRA_SCHLUESSEL_DATEI` | Privater Schlüssel für `private_key_jwt` (Vorgabe `<Daten>/entra-schluessel.pem`) |
| `QUIVER_EINRICHTUNG` | `mit-code` (Vorgabe) oder `ohne-code` (nur mit Loopback-Basis-URL) |
| `QUIVER_ENTRA_AUTHORITY` | Andere Authority, nur Loopback (Tests) |
| `QUIVER_ENTRA_GRUPPENNAMEN` | `an` holt Gruppennamen über Graph; Vorgabe `aus`, jeder andere Wert verhindert den Start |
| `QUIVER_ENTRA_GRAPH` | Basis-URL für Graph, nur Loopback (Tests); im Betrieb fest `https://graph.microsoft.com` |
| `QUIVER_PROXIES` | CIDR-Liste, denen `X-Forwarded-For` geglaubt wird |
| `QUIVER_PROGRAMME` | Ordner mit den zu verteilenden Programmen (Vorgabe `<Daten>/tools` und `/opt/quiver/tools`) |
| `QUIVER_ALTGERAETE` | `gesperrt` (Vorgabe, sobald Entra eingerichtet ist, sonst `erlaubt`) für Geräte ohne Benutzer |
| `QUIVER_UPDATES` | `an` (Vorgabe) oder `aus`: darf der Server die GitHub-Veröffentlichungen nach einer neuen Version fragen — und deren Pakete an die Geräte ausliefern? Höchstens alle sechs Stunden, ohne Kennung, ohne Nutzdaten. Siehe [updates.de.md](updates.de.md) |
| `QUIVER_UPDATE_QUELLE` | nur Loopback (Tests): Basis der Abfrage der Veröffentlichung; im Betrieb fest das Repository des Projekts |

`.env.example` ist die Vorlage.

## Reverse-Proxy

TLS am Proxy beenden, `Host` unverändert durchreichen und das Netz des Proxys in
`QUIVER_PROXIES` eintragen (CIDR, mit Komma getrennt), damit der Server
`X-Forwarded-For` für Drossel und Prüfspur glaubt. Die Veröffentlichung bleibt
auf `127.0.0.1`; der Proxy ist der einzige Weg hinein.

## Sicherung

Der ganze Zustand liegt in `/daten` (dem Volume):

* `quiver.db` mit `-wal` und `-shm` — Bibliothek, Sätze, Gruppen, Benutzer,
  Geräte, Prüfspur, Sitzungen;
* `schluessel` — der Schlüssel, der Refresh-Token verschlüsselt. **Sichern, aber
  getrennt von der Datenbank:** ohne ihn sind die Token wertlos, mit ihm
  zusammen ist die Sicherung ein Zugangsdatum;
* `entra-schluessel.pem` und `entra-schluessel.crt.pem`, wenn Entra ein Zertifikat nutzt;
* `.vor-*` — automatische Sicherungen vor einer Migration (die letzten drei
  bleiben).

Container anhalten oder die Sicherungsschnittstelle von SQLite nutzen; eine
laufende `quiver.db` ohne die WAL zu kopieren kann die neuesten Transaktionen
verlieren. Wiederherstellen heißt: anhalten, Datenordner ersetzen, starten.

## Aktualisieren

Abbild tauschen und neu starten:

```bash
docker compose pull
docker compose up -d
```

Vor einer Migration schreibt der Server eine Sicherung und prüft die Integrität
beider Dateien. Datenbank-Migrationen haben keinen Rückweg; zurück geht es über
den Datenordner aus der Sicherung.

## Log

`RUST_LOG` steuert die Stufe (im Abbild `info`). Der Prozess schreibt nach
stdout; Docker rotiert. Jedes Ende des Clients und jedes „in den Tray gelegt“
ist eine Zeile im Client-Protokoll (siehe [client.de.md](client.de.md)), nicht
hier.

In den ersten 15 Minuten steht der Einmal-Code der Ersteinrichtung im Log.
Behandle das Log dieses Fensters wie ein Geheimnis.

## Fehlersuche

| Meldung / Symptom | Ursache und Abhilfe |
| --- | --- |
| `421` auf jeder Seite | Der `Host` passt nicht zu `QUIVER_OEFFENTLICH`. Genau diese Adresse öffnen; `localhost` und `127.0.0.1` sind verschieden. Hinter einem Proxy: `Host` durchreichen. |
| Server startet nicht: Basis-URL nicht https | `https` verwenden, oder auf Loopback veröffentlichen und `http://localhost:…` nutzen. |
| Die Seite sagt, die Ersteinrichtung ist geschlossen | Die ersten 15 Minuten sind vorbei. `docker restart quiver`, neu laden. |
| Die Einrichtung fragt nach einem Code, der nicht bekannt ist | Er steht im Log **dieses** Starts: `docker logs quiver \| grep Einrichtung`. |
| `docker exec … quiver-server` geht nicht | Den absoluten Pfad oder den Namen im `PATH` nehmen (`/usr/local/bin/quiver-server`); eine Shell gibt es nicht. |
| Ein Gerät bekommt keine Tools | Die Gruppe hat keinen Satz, der Satz ist leer, oder der Eintrag ist abgeschaltet. Verwaltung → Gruppen/Sätze/Bibliothek prüfen. |
| Entra-Anmeldung scheitert (`AADSTS…`) | Siehe Tabelle in [entra.de.md](entra.de.md#wenn-es-klemmt). |
| Gruppennamen bleiben „Entra-Gruppe …" | `QUIVER_ENTRA_GRUPPENNAMEN=an` und die Zustimmung für `GroupMember.Read.All` sind beides nötig. |
| Zertifikatswarnung ab 30 Tagen vor Ablauf | Mit `entra-schluessel --erneuern` erneuern, hochladen, dann `--umschalten`; siehe [entra.de.md](entra.de.md). |
| `docker stop` endet mit 137 | SIGTERM hat den Prozess nicht erreicht; das Abbild endet mit 0. Prüfen, ob die Laufzeit den Container abgeschossen hat. |
| Gesundheitsprüfung schlägt fehl | `quiver-server gesund` muss über Loopback antworten; Port und Prozess prüfen. |
