# 0002 – Anmeldung über Entra ID, Root-Benutzer, kein Admin-Schlüssel

Stand: 2026-11-24

## Kontext

Bisher: ein Admin-Schlüssel für die Verwaltung, Einmal-Codes für Geräte, Gruppen von Hand. Gewollt: Anmeldung über Entra (wie bei NetBird), Gruppen automatisch, dazu **ein** Root-Benutzer mit E-Mail und Passwort, kein Admin-Schlüssel mehr.

## Entscheidung

1. **Der Server ist der einzige OIDC-Client** (eine App, ein Tenant, Authorization Code + PKCE, Geheimnis nur am Server). Der Desktop-Client spricht nie mit Entra, sondern öffnet die Anmeldung des Servers im Browser und bekommt über `127.0.0.1` einen Einmal-Code, den er mit seinem PKCE-Verifier gegen ein Geräte-Token tauscht.
2. **Gruppen aus dem ID-Token** (`groups`, nur der App zugewiesene Gruppen), als **Objekt-Id**. Namen sind änderbar und damit keine Grundlage für Rechte; der Admin gibt einer Id einen Anzeigenamen. Kein Verzeichnis-Leserecht.
3. **Nachprüfen statt einmal glauben:** Refresh-Token je User (verschlüsselt am Server), stündlich eingelöst; das neue ID-Token bestimmt die Gruppen. Gesperrt, nicht mehr zugewiesen, widerrufen → alle Geräte des Users verlieren sofort ihre Tools.
4. **Root** (genau einer), Argon2id, der Notzugang, wenn Entra ausfällt. Fehlt er, zeigt der Browser die Ersteinrichtung: nur 15 min nach dem Start, und außer auf Loopback nur mit einem Einmal-Code aus dem Server-Log, damit niemand anders den Server übernimmt (Aufgabe 25). Vergessenes Passwort: Unterbefehl am Server.
5. **Rollen** Root, Admin, Mitarbeiter (Aufgabe 24), vergeben je Benutzer oder je Entra-Gruppe, die höchste gilt. Der Server erzwingt sie bei jeder Anfrage. Wer in keiner Entra-Gruppe ist, hat keinen Zugang.
6. **Kein Admin-Schlüssel.** Automatisierung läuft über Unterbefehle im Container.
7. **Geräte-Codes nur ohne eingerichtetes Entra.**
8. **Ohne zugeordnete Gruppe keine Tools.**

## Folgen

- HTTPS ist Pflicht, außer auf Loopback.
- Bis zu 60 min, bis eine Gruppenänderung wirkt (der Admin kann „jetzt prüfen“ auslösen).
- Ist Entra nicht erreichbar, gilt der letzte Stand höchstens 24 h.

## Ergänzung 2026-11-24 (Aufgabe 23): Zertifikat statt Geheimnis

1. Der Server darf sich statt mit einem Client-Geheimnis mit `private_key_jwt`
   ausweisen (PS256, Kopf `x5t#S256`). Konfiguriert ist genau eins von beidem;
   beides oder eine Lücke verhindert den Start. Grund: viele Mandanten
   verbieten Client-Geheimnisse per Richtlinie.
2. Den Schlüssel erzeugt der Server selbst (`quiver-server entra-schluessel`,
   RSA 3072, selbst signiertes Zertifikat für ein Jahr). Der private Schlüssel
   verlässt den Server nie, auch nicht über Log, Antwort, Prüfspur oder
   `Debug`; nach außen gehen nur Zertifikat, Fingerabdruck und Ablauf.
   `--erneuern` legt einen zweiten Schlüssel an, `--umschalten` macht ihn ohne
   Neustart und ohne Ausfall aktiv. Ab 30 Tagen vor dem Ablauf wird gewarnt.
3. Gruppen aus dem ID-Token legt der Server **selbst** als lokale Gruppen an:
   höchstens 200, je Id einmal, ohne Sätze, Titel „Entra-Gruppe <erste 8>“.
   Titel und Sätze gehören danach dem Admin; Entra ersetzt nur die
   Mitgliedschaften. Eine gelöschte Gruppe bleibt gelöscht, bis der Admin sie
   wieder aufnimmt. Damit entfällt das Zuordnen von Hand, und eine neue Gruppe
   gibt erst Tools, wenn ein Admin einen Satz zuweist.
