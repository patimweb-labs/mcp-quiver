# 23 – Server: Anmeldung an Entra mit Zertifikat statt Geheimnis

Nach 20. Viele Tenants verbieten Client-Geheimnisse per Richtlinie. Der Server weist sich mit `private_key_jwt` aus (`client_assertion`), Geheimnis bleibt möglich.

- Unterbefehl `quiver-server entra-schluessel`: erzeugt am Server RSA 3072 und ein selbst signiertes Zertifikat (1 Jahr), legt den privaten Schlüssel in `<daten>/entra-schluessel.pem` (0600, erzeugt ohne Zeitfenster mit offenen Rechten) und gibt **nur** das öffentliche Zertifikat (PEM), Fingerabdruck und Ablauf aus. Der private Schlüssel verlässt den Server nie. Vorhandenes wird nie überschrieben; `--erneuern` legt einen zweiten an, `--umschalten` macht ihn aktiv (Wechsel ohne Ausfall).
- Assertion: `PS256`, Kopf `x5t#S256`, `aud` = Token-Endpunkt aus der Discovery, `iss`=`sub`=Client-Id, `jti` zufällig, `nbf`, `exp` ≤ 5 min. Bei Code-Einlösung und Refresh. Je Anfrage neu.
- Konfiguration: genau eins von Schlüssel oder Geheimnis; beides → Start scheitert mit klarer Meldung.
- Ablauf: ab 30 Tagen Warnung im Log, in der Verwaltung und in der Prüfspur (je Tag einmal).
- Der Schlüssel erscheint nie in Log, Fehler, Antwort oder `Debug`.
- Tests: der Entra-Nachbau prüft die Assertion streng (Signatur gegen das Zertifikat, Algorithmus, alle Felder, `jti` nur einmal, Ablauf). Rechte der Datei, kein Überschreiben, Wechsel, Suchtest auf den Schlüssel in allen Ausgaben.
- Doku (en/de): Anleitung allgemein, ohne Firmendaten. ADR 0002 ergänzen.

Dazu ein Befund aus 20: Ein Test, der hängt, muss nach einer Frist scheitern (Zeitlimit um jeden Test, der auf Server oder Client wartet).

## Entra-Gruppen von selbst in quiver

- Jede Gruppen-Id, die in einem geprüften ID-Token steht (Anmeldung, Nachprüfen), legt der Server **einmal** als quiver-Gruppe an, falls noch keine Gruppe diese Id trägt: `entra: [id]`, Titel „Entra-Gruppe <erste 8 Zeichen>“, **ohne Sätze**. Sie gibt also erst Tools, wenn ein Admin einen Satz zuordnet. Prüfspur „angelegt durch Anmeldung“.
- Titel und Sätze gehören danach dem Admin, kein Abgleich überschreibt sie. Löscht der Admin die Gruppe, bleibt sie gelöscht (Id gemerkt); die Verwaltung kann sie wieder aufnehmen.
- Mitgliedschaften pflegt **nur** Entra: ersetzt bei Anmeldung und bei jedem Nachprüfen, auch das Entfernen. Änderungen wirken sofort auf die verbundenen Geräte. Von Hand sind sie für Entra-Gruppen nicht änderbar.
- Höchstens 200 so angelegte Gruppen. Tests: erste Anmeldung, zweite ohne Doppel, Umbenennen bleibt, Entzug nimmt Tools am verbundenen Gerät weg, gelöscht bleibt gelöscht, neue Gruppe ohne Satz gibt nichts, gleichzeitige Anmeldungen legen genau eine an.
- Verwaltung (Aufgabe 21): Gruppen aus Entra gekennzeichnet, Titel änderbar, Hinweis „noch kein Satz“.
