# 0002 – Anmeldung über Entra ID, Root-Benutzer, kein Admin-Schlüssel

Stand: 2026-11-24

## Kontext

Bisher: ein Admin-Schlüssel für die Verwaltung, Einmal-Codes für Geräte, Gruppen von Hand. Gewollt: Anmeldung über Entra (wie bei NetBird), Gruppen automatisch, dazu **ein** Root-Benutzer mit E-Mail und Passwort, kein Admin-Schlüssel mehr.

## Entscheidung

1. **Der Server ist der einzige OIDC-Client** (eine App, ein Tenant, Authorization Code + PKCE, Geheimnis nur am Server). Der Desktop-Client spricht nie mit Entra, sondern öffnet die Anmeldung des Servers im Browser und bekommt über `127.0.0.1` einen Einmal-Code, den er mit seinem PKCE-Verifier gegen ein Geräte-Token tauscht.
2. **Gruppen aus dem ID-Token** (`groups`, nur der App zugewiesene Gruppen), als **Objekt-Id**. Namen sind änderbar und damit keine Grundlage für Rechte; der Admin gibt einer Id einen Anzeigenamen. Kein Verzeichnis-Leserecht.
3. **Nachprüfen statt einmal glauben:** Refresh-Token je User (verschlüsselt am Server), stündlich eingelöst; das neue ID-Token bestimmt die Gruppen. Gesperrt, nicht mehr zugewiesen, widerrufen → alle Geräte des Users verlieren sofort ihre Tools.
4. **Root** (genau einer) über die Kommandozeile des Servers gesetzt, Argon2id. Der Notzugang, wenn Entra ausfällt. Keine offene Ersteinrichtung im Browser, sie könnte jeder übernehmen.
5. **Rollen** Root, Admin, Mitarbeiter (Aufgabe 24), vergeben je Benutzer oder je Entra-Gruppe, die höchste gilt. Der Server erzwingt sie bei jeder Anfrage. Wer in keiner Entra-Gruppe ist, hat keinen Zugang.
6. **Kein Admin-Schlüssel.** Automatisierung läuft über Unterbefehle im Container.
7. **Geräte-Codes nur ohne eingerichtetes Entra.**
8. **Ohne zugeordnete Gruppe keine Tools.**

## Folgen

- HTTPS ist Pflicht, außer auf Loopback.
- Bis zu 60 min, bis eine Gruppenänderung wirkt (der Admin kann „jetzt prüfen“ auslösen).
- Ist Entra nicht erreichbar, gilt der letzte Stand höchstens 24 h.
