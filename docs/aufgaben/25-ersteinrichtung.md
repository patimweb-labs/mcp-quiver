# 25 – Ersteinrichtung im Browser

Direkt nach 20, **zusammen mit der neuen Anmeldeseite** (aus 21 vorgezogen). Gibt es keinen Root, zeigt die Verwaltung statt der Anmeldung die Ersteinrichtung. Der Admin-Schlüssel kommt nirgends mehr vor.

## Server

- `GET /auth/stand` (offen): `{ eingerichtet, einrichtung: "offen"|"geschlossen"|null, code_noetig, entra }`, sonst nichts.
- `POST /auth/einrichten {email, passwort, code?}`: legt **genau einen** Root an (atomar, eine Transaktion mit Prüfung „kein Root“; gleichzeitige Aufrufe: einer gewinnt, die anderen 409), meldet ihn an und löscht `<daten>/admin-token`. Gibt es einen Root: 404, auch die Seite.
- **Zeitfenster:** nur 15 min nach dem Start des Servers offen, danach `geschlossen` (403, „Server neu starten“). Uhr: Laufzeit des Prozesses, nicht die Wanduhr.
- **Code:** Ist `QUIVER_OEFFENTLICH` Loopback, braucht es keinen. Sonst ist ein Einmal-Code Pflicht, den der Server bei jedem Start erzeugt (128 Bit, gehasht im Speicher) und **nur** ins eigene Log schreibt; konstante Zeit, gedrosselt, nach 10 Fehlversuchen geschlossen.
- `Host` und `Origin` müssen exakt zu `QUIVER_OEFFENTLICH` passen (gegen DNS-Rebinding und fremde Seiten), dazu `X-Quiver: 1`. Passwortregeln und Argon2id wie Root. Prüfspur.
- Vor der Einrichtung ist die Admin-API ganz gesperrt; Geräte laufen weiter.
- Steht der Server auf Loopback, lauscht er im Container trotzdem auf allen Adressen: die Doku zeigt `-p 127.0.0.1:8740:8740`.

## Seite (Vue)

Logo, „Willkommen bei mcp-quiver“, kurzer Text. E-Mail, Passwort, Passwort wiederholen (Mindestlänge und Gleichheit sofort sichtbar, Anzeigen-Knopf), Code nur wenn nötig mit Hinweis, wo er steht. Danach direkt angemeldet. `geschlossen`: verständliche Seite mit dem Befehl zum Neustart. Drei Themes, Tastatur, Passwortmanager (`autocomplete="username"`/`new-password"`), Bilder.

Anmeldeseite: E-Mail und Passwort, „Mit Microsoft anmelden“ nur wenn eingerichtet; Sitzung nur im Cookie, nichts im Speicher des Browsers.

## Tests

Einrichten, danach 404; zehn gleichzeitige Versuche → genau ein Root; Fenster abgelaufen (Uhr einstellbar); Code fehlt, falsch, zehnmal falsch; fremder `Host`/`Origin`; kein `X-Quiver`; Admin-API vorher gesperrt; `admin-token` gelöscht und wirkungslos; vorhandene Daten (Einträge, Geräte, Gruppen) bleiben; Passwort und Code nie im Log außer der einen Codezeile. Vue: alle Zustände, Weg vom Einrichten bis in die Verwaltung.
