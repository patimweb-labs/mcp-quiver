# 22 – Client: Anmeldung über den Server

Nach 20. Seite Verbindung: Server-Adresse (`https`, `http` nur Loopback) → „Anmelden“. Der Client fragt `GET /api/v1/anmeldearten`; mit Entra öffnet er den Browser (PKCE, `zustand` in konstanter Zeit verglichen, Lauscher nur `127.0.0.1` mit freiem Port, Pfad `/quiver`, 5 min, genau eine Antwort, schlichte Abschlussseite), sonst wie bisher Code-Eingabe. Nie mit Entra selbst sprechen.
Token in den Tresor; `altes_token` mitsenden, damit das Gerät dasselbe bleibt. `grund`: `gesperrt`/`kein_zugriff` → alle Tools aus, klare Meldung; `neu_anmelden` → alle Tools aus, **Geräte-Token und alle Angaben bleiben**, Knopf „Erneut anmelden“, Hinweis im Tray. Unbekannter Grund → wie `neu_anmelden`. 401 wie bisher (neu einbinden), lokale Angaben bleiben. Abmelden löscht das Gerät am Server. Anzeige „Angemeldet als …“. Ein Agent kann nichts davon auslösen.
Tests Ende-zu-Ende mit echtem Server und Entra-Nachbau, Browser als Trait; kittest; Texte de/en; Bilder. Beende nie den laufenden Client des Users.

## Nachtrag: Warum der Client endet

Jedes Ende steht mit Ursache im Log (Tray, Fenster/Cmd+Q, Strg+C, SIGTERM, zweite Instanz, Abmelden des Systems). SIGTERM geordnet wie Strg+C (Unix; Windows: Konsolen-Ereignisse). Cmd+Q ist **kein** Ende, sondern legt das Fenster in den Tray (wie das Schließen); beenden nur über den Tray. Tests.
