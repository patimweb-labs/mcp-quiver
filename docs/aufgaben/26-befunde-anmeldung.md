# 26 – Befunde der Abnahme von 20 (zusammen mit 25 umsetzen)

1. **Verwaltungssitzung und Entra:** `verwaltung_wache`/`angemeldet_wache` prüfen bei Entra-Benutzern auch `token_stand == gueltig`; wird ein Benutzer `neu_anmelden`, gesperrt oder gelöscht, enden seine Sitzungen sofort (löschen, nicht nur ablehnen). Ist die letzte Prüfung > 60 min her, prüft die Wache vor dem Durchlassen nach.
2. **Referrer-Policy `same-origin`** statt `no-referrer`: sonst schickt der Browser beim Formular `Origin: null`, und die Bestätigung scheitert. Test über die Kopfzeile **und** im echten Browser (Chrome headless über `admin/scripts`, wie die Bilder): Anmelden → Bestätigen → Rücksprung auf einen Lauscher auf `127.0.0.1` mit Code.
3. **CSP `form-action`:** Chrome prüft auch Weiterleitungen nach dem Absenden. Nur die Bestätigungsseite bekommt `form-action 'self' http://127.0.0.1:<port>` mit genau dem Port des Vorgangs.
4. **Dummy-Hash** beim Start erzeugen (nicht beim ersten Versuch, nicht im Async-Faden).
5. **Seiten des Servers** (Bestätigung, Fehler): eigenes Stylesheet aus den Tokens (`/auth/stil.css`, `style-src 'self'`), Logo, dunkel/hell über `prefers-color-scheme`, gleiche Anmutung wie die Verwaltung, Hand-Cursor; Titel und Texte de/en nach `Accept-Language`.
6. **Discovery** höchstens 24 h alt, danach neu; schlägt das fehl, gilt die alte weiter (Warnung).
7. **Zeitlimit je Test**, der auf Server oder Client wartet: Hängen ist ein Fehlschlag.
8. **Pfade:** `/auth/stand` und `/auth/einrichten` liegen ohne `/api/v1` auf der Wurzel, genau wie die Verwaltung sie aufruft (`admin/src/api/client.ts`).
9. **Voreinstellung:** ohne `QUIVER_OEFFENTLICH` gilt `http://localhost:<Port>`. Test: frischer Server und Server mit heutiger DB ohne Root zeigen die Ersteinrichtung.
10. Die Verwaltung wird gegen den **echten** Server getestet (Chrome headless, gebauter `dist`, echte CSP): einrichten, abmelden, anmelden, falsches Passwort, Übersicht laden, einen Satz ändern, 0 CSP-Verstöße, 0 Konsolenfehler. Als wiederholbares Skript.
