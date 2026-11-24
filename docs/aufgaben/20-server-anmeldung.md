# 20 – Server: Entra-Anmeldung, Root, Sitzungen

Zuerst `docs/adr/0002-anmeldung-entra-und-root.md`. Vorhandene Wege ohne Entra (Codes) bleiben für Server ohne Entra erhalten.

## Konfiguration

`QUIVER_OEFFENTLICH` (Basis-URL; `https`, `http` nur bei `localhost`/`127.0.0.1`/`[::1]`, sonst startet der Server nicht), `QUIVER_ENTRA_TENANT` (GUID, nie `common`/`organizations`), `QUIVER_ENTRA_CLIENT_ID`, Geheimnis aus `QUIVER_ENTRA_SECRET_DATEI` (Standard `<daten>/entra-secret`; Rechte zu offen → Warnung). Teilweise gesetzt → Start scheitert mit klarer Meldung. `QUIVER_ENTRA_AUTHORITY` nur Loopback (Tests). `QUIVER_PROXIES` (CIDR): nur von dort gilt `X-Forwarded-For`. `QUIVER_ALTGERAETE=erlaubt|gesperrt` (Standard erlaubt).

## OIDC (Modul `oidc.rs`)

- Discovery; `issuer` exakt; Endpunkte nur `https` und gleicher Host wie die Authority (Loopback im Test). JWKS zwischengespeichert, bei unbekannter `kid` höchstens 1×/min neu.
- ID-Token: nur `RS256`, Signatur, `iss`, `aud` = Client-Id, `tid` = Tenant, `nonce`, `exp`/`nbf`/`iat` (60 s Toleranz), `oid` Pflicht. Bewährte Crates (`openidconnect` oder `jsonwebtoken`), keine eigene Kryptografie. Keine Weiterleitungen folgen, Zeitlimit, Antwortgröße begrenzt.
- Identität ist `(tid, oid)`; E-Mail/Name nur Anzeige, **nie** Schlüssel.
- `state`, `nonce`, PKCE-Verifier: 256 Bit, am Server, 10 min, einmalig, per HttpOnly-Cookie an den Browser gebunden; Fehler von Entra nur als Code/kurzer Text anzeigen (escaped).
- `groups`: Objekt-Ids (GUID geprüft). `_claim_names`/`hasgroups` (zu viele Gruppen) → Anmeldung scheitert mit Hinweis, nie „keine Gruppen“. Fehlt der Claim ganz → keine Gruppen.

## Gerät (ersetzt Codes, wenn Entra an ist)

1. `GET /auth/geraet/start?challenge&port&zustand&name` – `challenge` S256 (43 Zeichen), Port 1024–65535, `zustand` ≤ 128 sichere Zeichen, Name ≤ 80. Dann Entra.
2. Nach der Rückkehr eine **Bestätigungsseite** (Konto, Gerätename, Server) mit POST-Knopf (CSRF-Token); erst dann Weiterleitung auf **genau** `http://127.0.0.1:<port>/quiver` mit Einmal-Code und `zustand`. Abbrechen → Fehler dorthin.
3. `POST /auth/geraet/tausch {code, verifier, altes_token?}`: Code gehasht gespeichert, 60 s, einmalig (auch bei falschem Verifier verbraucht). Antwort wie `AnmeldenAntwort`. Mit gültigem `altes_token` wird **dasselbe** Gerät weitergeführt, wenn es keinem oder demselben User gehört.
4. Gerät gehört `(tid, oid)`. Ist der User nicht gültig: `403` mit maschinenlesbarem `grund` (`gesperrt` | `neu_anmelden` | `kein_zugriff`), Ereignis-Strom endet. `Fehler` bekommt `grund: Option<…>` (verträglich).

## Nachprüfen

Je User Refresh-Token, XChaCha20-Poly1305 mit `<daten>/schluessel` (0600, beim ersten Start erzeugt), AAD = `(tid,oid)`. Stündlich (mit Streuung), beim Verbinden eines Geräts nach > 60 min und auf Knopf. Rotation sofort gespeichert. Neues ID-Token muss dieselbe `(tid,oid)` haben; Gruppen ersetzen → Soll-Änderung an die Geräte. `invalid_grant`/`interaction_required`/Zuweisung fehlt → `neu_anmelden`. Netz-/5xx-Fehler → letzter Stand, höchstens 24 h, danach `neu_anmelden`. Kein gleichzeitiges Einlösen je User. Die Löschung des Users löscht Token und Geräte.

## Verwaltung

- Sitzung: 256 Bit, gehasht in der DB, Cookie `HttpOnly; SameSite=Lax; Path=/`, bei https `Secure` und `__Host-`. 12 h absolut, 60 min ohne Nutzung; neue Id bei jeder Anmeldung; Abmelden löscht am Server. Ändernde Anfragen: `Origin` muss `QUIVER_OEFFENTLICH` entsprechen **und** Kopf `X-Quiver: 1`; keine CORS-Freigabe.
- Root: `POST /auth/root`. Argon2id (64 MiB, t=3, p=1; höchstens 2 gleichzeitig, sonst 429), E-Mail normalisiert, unbekannte E-Mail gleiche Dauer/Antwort. Drossel je IP **und** je Konto (5 frei, dann wachsend bis 15 min, nie dauerhaft sperren), vor der Prüfung. Passwort 12–256 Zeichen. Nie im Log.
- Entra-Admin: User mit `rolle = admin`. Rechte bei **jeder** Anfrage frisch aus der DB. Root ist nicht löschbar/herabstufbar; ein Admin kann Admins ernennen und entziehen; sich selbst entziehen nur mit Rückfrage.
- Neue Pfade: `/auth/ich`, `/auth/abmelden`, `/admin/benutzer` (Liste, Rolle, sperren, jetzt prüfen, löschen), `/admin/entra-gruppen` (gesehene Ids, Anzeigename), `Gruppe.entra: Vec<Objekt-Id>`; wirksame Gruppen eines Geräts = Zuordnung des Users ∪ am Gerät gesetzte. Prüfspur (`/admin/pruefspur`): Anmeldungen, Fehlschläge, Rollen, Änderungen – wer, wann, was, ohne Geheimnisse; 180 Tage.
- `admin_token` ganz entfernen: `QUIVER_ADMIN_TOKEN` wird ignoriert (Warnung), `<daten>/admin-token` nach dem ersten gesetzten Root gelöscht.
- Sicherheits-Köpfe überall (CSP ohne inline, `frame-ancestors 'none'`, `nosniff`, `no-referrer`, HSTS bei https), `Cache-Control: no-store` bei `/auth` und `/api`.
- Einmal-Codes (ohne Entra) ebenfalls gedrosselt; mit Entra antworten `/anmelden` und `/admin/codes` 404 mit Hinweis.

## Unterbefehle (`quiver-server …`, im Container)

`root-setzen` (E-Mail, Passwort verdeckt zweimal; `--stdin` für Skripte; beendet alle Root-Sitzungen), `admin-setzen --oid`, `benutzer`, `eintrag-setzen <datei|->`, `satz-setzen <datei|->`, `eintraege`, `saetze`. Gleicher Prüfweg wie die API, Prüfspur „Kommandozeile“. Der laufende Server bemerkt Änderungen über `PRAGMA data_version` (≤ 2 s) und stößt die Geräte an.

## Datenbank

Versionierte Migrationen, vorher Sicherung `<db>.vor-<version>`; Test mit einer Datei im heutigen Schema: alle Daten bleiben. Altgeräte ohne User: erlaubt, bis `QUIVER_ALTGERAETE=gesperrt`, dann 403 `neu_anmelden`.

## Tests

Eigener Entra-Nachbau (axum, eigener RSA-Schlüssel). Jede Regel oben einzeln, Angriffe ausdrücklich: falsche Signatur/`alg=none`/HS256 mit öffentlichem Schlüssel/fremder `iss`/`aud`/`tid`, vertauschte `nonce`, `state` aus anderem Browser, Code doppelt, falscher Verifier, Weiterleitungsziel verbogen (`127.0.0.1@evil`, `localhost`, Pfad), CSRF ohne `Origin`/fremde `Origin`, Sitzung nach Rollenentzug, gestohlene alte Sitzung nach Passwortwechsel, gefälschtes `X-Forwarded-For`, Gruppen-Überlauf, Refresh mit anderer `oid`, gleichzeitiges Nachprüfen, Ausfall 23 h/25 h (Uhr einstellbar), zu offene Rechte, alte DB. Kein Geheimnis in Log, Antwort, Prüfspur (Suchtest über alle Ausgaben).

Doku: README (en/de) mit **allgemeiner** Entra-Anleitung (keine Firmendaten), SPEC. Prüfbefehl grün. Nicht committen.
