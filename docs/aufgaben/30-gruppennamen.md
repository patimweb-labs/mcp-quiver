# 30 – Namen der Entra-Gruppen über Graph

Rechte laufen weiter **nur** über die Id. Der Name ist Anzeige.

- Schalter `QUIVER_ENTRA_GRUPPENNAMEN=an` (Standard aus, sonst Startfehler bei anderem Wert). Nur dann kommt `https://graph.microsoft.com/GroupMember.Read.All` zu den Scopes (Anmeldung **und** Refresh), denn ohne Zustimmung des Tenants scheiterte sonst jede Anmeldung.
- Nach geprüfter Anmeldung und beim Nachprüfen: für die Gruppen **aus dem ID-Token**, deren Name fehlt oder älter als 24 h ist, `GET {graph}/v1.0/groups/{id}?$select=id,displayName` mit dem Access-Token dieses Benutzers. Höchstens 20 je Lauf, 4 gleichzeitig, 5 s je Anfrage, 64 KB, keine Weiterleitungen, Host fest (`https://graph.microsoft.com`; Loopback nur in Tests über `QUIVER_ENTRA_GRAPH`). Id der Antwort muss passen; Name getrimmt, ohne Steuer- und Richtungszeichen, höchstens 120 Zeichen.
- **Im Hintergrund**: hält Anmeldung und Nachprüfen nie auf, ein Fehler ändert nichts (Log ohne Token, je Stunde einmal). 403 → einen Tag Pause.
- Das Access-Token lebt nur im Speicher dieses Laufs, nie in DB, Log, `Debug`, Prüfspur.
- `EntraGruppe.name_entra` + Zeit. Titel der quiver-Gruppe: wird nur gesetzt, solange er der automatische ist („Entra-Gruppe xxxxxxxx“) oder gleich dem zuletzt von Entra übernommenen. Einen Titel des Admins überschreibt der Server nie. Prüfspur bei Änderung.
- Verwaltung: Gruppen zeigen den Titel, klein darunter „Entra: <Name>“ wenn abweichend; Benutzerliste zeigt den Titel der quiver-Gruppe. Die Id steht nur noch im Hinweis und in den Einzelheiten.
- Tests: aus → kein Scope, keine Anfrage; an → Name kommt, Umbenennen in Entra folgt, Admin-Titel bleibt; 403/404/Zeitlimit/Weiterleitung/fremde Id/riesig/böser Name; Anmeldung klappt trotz totem Graph und wird nicht langsamer; Token nirgends in Ausgaben (Suchtest). Migration.
