# Offene Punkte (Cheats und Lücken)

Jeder Punkt ist bewusst offen gelassen und wird abgebaut (AGENTS.md §2.5).

## Server
- [ ] Keine Begrenzung der Versuche bei `POST /anmelden` (Codes laufen nach 60 min ab, 32⁸ Möglichkeiten).
- [ ] Abgelaufene Codes werden nur beim Einlösen entfernt, kein Aufräumen im Hintergrund.

## Client
- [ ] Ungetestet: Prozess stirbt → `Fehler` + Neustart mit Backoff; `Remote`-Einträge; Start ohne Server nach Neustart.
- [ ] `tools/list_changed` wird auch ohne echte Änderung der Tool-Liste geschickt.
- [ ] `abmelden` räumt lokale Werte, Geheimnisse und Agenten nicht auf.
- [ ] Tools pro Agent filtern (SPEC §7) – die Agent-Kennung liegt in der Sitzung schon vor.
- [ ] stderr der MCP-Prozesse geht ungefiltert ins Log; ein Tool, das Geheimnisse ausgibt, landet dort.

## Allgemein
- [ ] `docs/architektur.md` und `docs/datenfluss.md` fehlen noch.
- [ ] Nach dem MVP: Entra ID, Signaturen, WASM-Laufzeit (SPEC §0).
