# Admin-Oberfläche (Vue)

Die Verwaltung von mcp-quiver: Bibliothek, Sätze, Gruppen, Benutzer, Geräte und
Prüfspur. Sie wird vom Server unter `/` ausgeliefert (`admin/dist`) und muss
**1:1 aussehen wie das Client-Fenster** (AGENTS.md §2.11): gleiche Tokens,
gleiche Schrift, gleiche Abstände, gleiche Bausteine.

## Einmal einrichten

Die Bausteine kommen aus [weppelmann-ui](https://github.com/Smotherer007/weppelmann-ui),
als Git-Submodul unter `vendor/weppelmann-ui`:

```bash
git submodule update --init admin/vendor/weppelmann-ui
cd admin
npm ci
```

## Befehle

```bash
npm run dev        # Vite auf 5173; /api geht an http://127.0.0.1:8740
npm run build      # nach admin/dist (der Server liefert sie unter / aus)
npm run typecheck  # vue-tsc
npm test           # vitest, reine Logik
npm run bilder     # Bildschirmfotos: Muster und die Seiten vor der Anmeldung
npm run bilder:benutzer     # Bibliothek, Gruppen, Benutzer, Prüfspur, Geräte (eigener Server)
npm run pruefung            # Browser-Prüfung gegen den echten Server
npm run pruefung:anmeldung  # Browser-Prüfung der Geräteanmeldung (+ Entra-Nachbau)
```

Zum Ausprobieren im Betrieb: `cargo run -p quiver-server` im Wurzelverzeichnis,
dann `http://127.0.0.1:8740` öffnen. Beim ersten Start zeigt die Verwaltung die
Ersteinrichtung; dort entsteht der eine Root-Benutzer. Einen Admin-Token gibt es
seit ADR 0002 nicht mehr.

Für `npm run dev` (Vite auf 5173) muss der Server dieselbe Herkunft kennen, denn
`/auth/stand` und `/auth/einrichten` prüfen den `Host`-Kopf gegen
`QUIVER_OEFFENTLICH`:

```bash
QUIVER_OEFFENTLICH=http://localhost:5173 cargo run -p quiver-server
```

## Aufbau

```
src/
  api/typen.ts        Vertrag, eins zu eins aus crates/protocol und crates/domain
  api/client.ts       fetch mit Sitzungscookie; 401 führt zur Anmeldung
  logik/              reine Funktionen mit Tests (Zustände, Prüfung, Format,
                      Breitenstufen)
  bausteine/          eigene Bausteine, gleiche Namen und Maße wie in
                      crates/oberflaeche/src/bausteine.rs: Kopf,
                      Seitenleiste, Segmentauswahl, Karte, Status, Feld, …
  ansichten/Muster.vue Musterseite für den Vergleich (nur im Dev-Bau)
  ansichten/          Anmeldebereich (Anmelden, Ersteinrichtung, geschlossen),
                      Überblick, Bibliothek, Sätze, Gruppen, Benutzer, Geräte,
                      Prüfspur
  styles/tokens.css   erzeugt aus tokens/ – nicht von Hand ändern
  styles/schriften.css IBM Plex Sans/Mono aus crates/oberflaeche/schriften
```

Der Zustand einer Seite ist ein Summentyp (`Zustand<T>` in `logik/seite.ts`):
`laedt`, `fertig` mit Daten oder `fehler` mit Text. Die Ansichten entscheiden
nichts; Rechnen, Filtern und Prüfen steht in `logik/` und ist getestet.

## Anmeldung

Ohne Admin-Token (ADR 0002, Aufgaben 20 und 25):

- **Sitzung nur im Cookie.** Der Server setzt `quiver_sitzung` (`HttpOnly`,
  `SameSite=Lax`, `Path=/`, bei https `Secure` und `__Host-`). Der Client
  schickt `credentials: "same-origin"` mit und nirgends ein `Authorization`.
  Weder `localStorage` noch `sessionStorage` enthalten etwas außer dem Theme.
- **Ändernde Anfragen** (POST, PUT, PATCH, DELETE) tragen `X-Quiver: 1`; der
  Server verlangt zusätzlich einen `Origin`, der zu `QUIVER_OEFFENTLICH` passt
  (CSRF). GET braucht beides nicht.
- **`GET /auth/stand`** (offen) sagt, ob ein Root gesetzt ist und ob die
  Ersteinrichtung offen ist. Danach zeigt die Verwaltung genau eine Lage aus
  `logik/anmeldung.ts`: laden, Ersteinrichtung, geschlossen, anmelden, bereit,
  keine Rolle oder Fehler. Ein Server ohne diesen Weg (404) bleibt bei der
  Anmeldung.
- **Anmelden:** Root mit E-Mail und Passwort (`POST /api/v1/auth/root`), dazu
  „Mit Microsoft anmelden“ (`GET /auth/admin/start`) — nur wenn der Server
  Entra kennt. Ein echter Verweis, damit der Browser der Weiterleitung folgt.
- **Ein 401** führt zur Anmeldung mit Begründung. Der Rücksprung danach nimmt
  nur eigene Pfade (`#/geraete` ja, fremde Adresse nein) — `logik/anmeldung.ts`
  prüft das, die Ansicht baut das Ziel nie aus einer fremden Adresse.
- **Abmelden** löscht die Sitzung am Server (`POST /api/v1/auth/abmelden`) und
  zeigt wieder die Anmeldung. Im Kopf stehen Name und Rolle aus
  `GET /api/v1/auth/ich`.
- **Ersteinrichtung** (nur solange kein Root gesetzt ist): E-Mail, Passwort,
  Wiederholung und – außerhalb von Loopback – der Einmal-Code aus dem
  Server-Log. Mindestlänge und Gleichheit sind sofort sichtbar. Ist das
  Fenster zu (15 Minuten nach dem Start), nennt die Seite den Befehl zum
  Neustart. Angelegt wird der Root mit `POST /auth/einrichten`; danach meldet
  der Server gleich an, und die Seite kommt nie wieder (404).
- **Die Seiten des Servers** (Gerätebestätigung, Fehlermeldungen) bringt der
  Server selbst mit: eigenes Stylesheet `/auth/stil.css` aus denselben Tokens,
  Logo, dunkel/hell über `prefers-color-scheme`, Hand-Cursor, Titel und Texte
  deutsch oder englisch nach `Accept-Language`. Sie brauchen kein Vue.

Die Seite hat **kein Inline-JavaScript**: die Sicherheitsköpfe des Servers
erlauben nur `script-src 'self'`. Das Theme setzt deshalb `public/thema.js`
vor dem ersten Aufbau, nicht ein `<script>` im `index.html`.

## Fenstergröße

Die Verwaltung folgt denselben Grenzen wie das Client-Fenster (Aufgabe 18):
`logik/breite.ts` misst das Fenster, liest die Grenzen aus den Tokens
(`--wp-breakpoint-formular`, `--wp-breakpoint-schmal`) und schreibt die Stufe als
`data-breite` an `<html>`. Unter *schmal* (760 px) trägt die Seitenleiste nur
die Zeichen — der Name steht in `title` und `aria-label` —, unter *eng* (700 px)
öffnet ein Zeichen im Kopf das Farbschema als Menü. Der Inhalt endet bei
`size.inhalt-max` (1100 px) und bleibt mittig; Formulare (`Feld.vue`) und
Werkzeugleisten brechen von sich aus um. Die Stufen rechnet `logik/breite.ts`
und ist mit `npm test` geprüft.

In der Bibliothek trägt jeder Eintrag ein `aktiv` (Standard `true`, siehe
`crates/domain`). Ein abgeschalteter Eintrag bleibt in Bibliothek und Sätzen,
läuft aber auf keinem Gerät: die Liste zeigt das Abzeichen *Abgeschaltet* und
der Schalter in der Zeile dreht den Wert ohne den Bearbeiten-Dialog.

Ein Eintrag kann außerdem eine *Anmeldung* und eine *Einrichtung* haben (beide
nur für `stdio`, beide als Argumentliste für dasselbe Programm). Die Liste zeigt
*mit Anmeldung* bzw. *mit Einrichtung*; im Bearbeiten-Dialog stehen dafür zwei
zugeklappte Abschnitte, die sich öffnen, sobald etwas drinsteht.

## Themes und Tokens

Drei Themes, sonst nichts: `dark`, `light`, `mono` (`data-theme` am `<html>`).
Farben, Maße und Schriften kommen ausschließlich aus `src/styles/tokens.css`.
In Ansichten steht kein Hex-Wert und kein Pixelmaß.

`weppelmann-ui` erwartet zusätzlich `--wp-fact-label` (Breite der
Beschriftungsspalte in `WpFactList`), mit Rückfall auf `130px`. Die Variable
fehlt in unseren Token-Dateien; sie gehört nach `tokens/`, nicht in die
erzeugte `tokens.css`. (Nicht von Hand nachtragen.)

## Hand-Cursor

Alles Klickbare zeigt den Hand-Cursor. Das kommt aus
`vendor/weppelmann-ui/packages/components/src/style.css` für native Elemente
und Komponenten und aus den eigenen Bausteinen (`cursor: pointer` im Baustein,
nie in der Ansicht).

## Musterseite

Unter `/muster` steht im Entwicklungsbau (`npm run dev`) dieselbe Seite wie im
Client-Fenster (`crates/oberflaeche/tests/muster.rs`): Kopf, Seitenleiste mit
drei Einträgen, Seitenkopf, Karte mit Hauptknopf und Knopf, drei Statuszeilen,
ein Eingabefeld. Sie braucht keine Anmeldung und zeigt keine Daten — sie ist
der Vergleich der Maße zwischen Vue und egui (AGENTS.md §2.11). Im gebauten
Stand gibt es sie nicht.

## Bildschirmfotos

`npm run bilder` schreibt zwei Sätze Bilder nach `docs/bilder/`:

- `muster-vue-{dark,light}.png` (1200 × 800) — dafür startet das Skript den
  Entwicklungs-Server selbst; es braucht weder Server noch Anmeldung. Das
  Gegenstück erzeugt `QUIVER_BILDER=docs/bilder cargo test -p quiver-oberflaeche
  --test muster`.
- `admin-{dark,light}-{anmeldung,einrichtung,geschlossen}.png` (1440 × 900) —
  die Seiten vor der Anmeldung. Dafür genügt derselbe Entwicklungs-Server: das
  Skript ersetzt im Browser die Antworten von `/auth/stand` und
  `/api/v1/auth/ich`, damit jeder Zustand einmal zu sehen ist. Kein Server,
  keine Zugangsdaten.

Die Seiten **hinter** der Anmeldung schreibt `npm run bilder:benutzer`:

- `admin-{dark,light,mono}-{bibliothek,gruppen,benutzer,pruefspur,geraete}.png`
  (1440 × 900) und je eine schmale Fassung (560 px) — gegen einen **eigenen**
  `quiver-server` auf einem freien Port, mit temporärer Datenbank, gebautem
  `admin/dist` und einem Entra-Nachbau: ein erfundener Katalog (`example.test`),
  zwei Personen („Alex Beispiel“, „Ben Beispiel“), eine Gruppe aus Entra, eine
  lokale. Aus einem echten Profil entsteht bewusst kein Bild.
  `QUIVER_BILD_SEITEN=bibliothek,gruppen` schreibt nur diese Seiten und lässt
  die übrigen unverändert.

## Prüfungen im Browser

Zwei Skripte prüfen die Verwaltung gegen den **echten** Server: gebautes
`dist`, die CSP des Servers, Chrome headless mit eigenem temporären Profil,
Loopback und ein temporärer Datenordner. Kein echter Mandant, kein echter
Tresor, keine Firmendaten; Ports 8740–8742 bleiben unberührt (es wird ein
freier Port gesucht).

```bash
cargo build -p quiver-server
npm run build
npm run pruefung            # Einrichten → Abmelden → Anmelden → falsches
                            # Passwort → Übersicht → einen Satz ändern;
                            # am Ende: 0 CSP-Verstöße, 0 Konsolenfehler
npm run pruefung:anmeldung  # Geräteanmeldung → Bestätigen → Rücksprung auf
                            # http://127.0.0.1:<Port>/quiver mit Code, gegen
                            # einen Entra-Nachbau auf Loopback
```

`pruefung:anmeldung` prüft zusätzlich die Kopfzeile `Referrer-Policy`
(`same-origin`, sonst schickt der Browser beim Formular `Origin: null`), das
`form-action` der Bestätigungsseite für genau den Port des Vorgangs, das
geladene Stylesheet und den Hand-Cursor. Beide Skripte haben ein Zeitlimit;
Hängen ist ein Fehlschlag.

`scripts/server.mjs` startet den Server (`QUIVER_BIN` überschreibt den Pfad),
`scripts/mock-entra.mjs` ist der OIDC-Nachbau (eigener RSA-Schlüssel,
erfundene GUIDs).
