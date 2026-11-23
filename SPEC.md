# SPEC – mcp-quiver

> Stand: Entwurf 0, 2026-11-23. Jede Festlegung mit Begründung und Preis.
> Offene Punkte stehen in §10 und werden als ADR in `docs/adr/` entschieden.

## 0. MVP (Stand 2026-11-23)

Ziel: das Grundkonstrukt mit schöner Oberfläche und Funktion. **Ohne Azure.**

- Gruppen liegen lokal auf dem Server (Admin-Oberfläche), nicht in Entra ID.
- Geräte binden sich mit einem Einmal-Code ein, den der Admin erzeugt (legt die Gruppen fest).
- Admin meldet sich mit einem Admin-Token an.
- Laufzeiten im MVP: `stdio` (eigene Binaries, `npx`, `uvx`, `docker`) und `remote`. WASM folgt.
- **Abnahmekriterium:** Ein zugewiesenes MCP ist höchstens 5 s später beim Agenten nutzbar
  (Server meldet per SSE, Client gleicht ab und startet, Agenten bekommen `tools/list_changed`).
- Entra ID, Azure-Rollen und Signaturen kommen nach dem MVP; das Datenmodell hält dafür Platz
  (Zuweisungsquelle als Trait, Herkunft und Freigabe am Eintrag).

Umsetzungspakete: `docs/aufgaben/`.

## 1. Zweck

Eigene und fremde MCP-Tools **einmal** bauen und an **jeden** Agenten auf
**jedem** Gerät verteilen, ohne dass sich ein Agent um Installation,
Konfiguration oder Anmeldung kümmern muss.

Zielgruppe: ein Team oder eine Firma mit Microsoft Entra ID und mehreren
Agenten (pi, Hermes, später weitere) auf macOS, Windows und Linux.

## 2. Bausteine

| Baustein | Aufgabe | Technik |
|---|---|---|
| `quiver-server` | Bibliothek, Zuweisung, Verteilung, Admin-API | Rust, ein Container |
| Admin-Oberfläche | Bibliothek pflegen, Sätze zuweisen, Geräte sehen | Vue, wird vom Server ausgeliefert |
| `quiver` (Client) | Dienst auf dem Gerät: installiert, konfiguriert, führt aus, ein MCP-Endpunkt | Rust |
| Client-Fenster | Status, Anmeldung, fehlende Werte ergänzen, Agenten koppeln, Tray | Rust, egui |
| `quiver-agent-sdk` | Verbindung Agent → Client für Plugins | Rust |
| Plugin pi | dünne Hülle um das SDK – pi 1.5 kann MCP selbst (`pi mcp`), das Plugin ist erst für mehr als MCP nötig | Rust, `napi-rs` |
| Plugin Hermes | dünne Hülle um das SDK | Rust, `PyO3`/`maturin` |
| Tool-Satz Microsoft | Microsoft 365 über Graph | Rust |

**Festlegung:** Der Server ist **nur** Bibliothek und Steuerung. Er führt
keine Tools aus.
*Begründung:* einfach, keine Zugangsdaten der User auf dem Server, offline
nutzbar. *Preis:* Systeme, die das Gerät nicht erreicht, sind nicht anbindbar.
Das Datenmodell lässt Platz für `ausfuehrung = zentral` (später, eigenes ADR).

## 3. Ablauf

```
1. Gerät einbinden   Client → Server: Entra-Login (User) + Geräteschlüssel
2. Soll berechnen    Server: Gruppen des Users → Sätze → Einträge + Werte
3. Abgleich          Client: Soll vs. Ist → installieren / aktualisieren / entfernen
4. Vervollständigen  Client fragt nur fehlende client-/geheim-Felder ab
5. Bereitstellen     Client: ein MCP-Endpunkt mit allen erlaubten Tools
6. Aufruf            Agent → Client → Tool (Zugangsdaten fügt der Client ein)
```

Der Client holt den Soll-Zustand beim Start und hält danach eine Verbindung,
über die der Server Änderungen ankündigt (Server-Sent Events). Fällt sie aus,
fragt er in festen Abständen nach. Ohne Server läuft der letzte bekannte
Stand weiter.

## 4. Bibliothek

**Grundlage:** das `server.json`-Format der offiziellen MCP Registry.
*Begründung:* Standard, fremde MCPs lassen sich aus der Registry übernehmen.
*Preis:* eigene Felder müssen unter `_meta` in einem eigenen Namensraum liegen.

```
Eintrag = server.json
 ├─ Standard: name, version, packages/remotes, Variablen, Argumente
 └─ _meta["io.github.patimweb-labs.quiver/eintrag"]
     ├─ herkunft:   eigen | fremd
     ├─ laufzeit:   wasm | nativ | npx | uvx | oci | remote
     ├─ rechte:     Netz-Ziele, Dateipfade, Prozesse   (Sandbox, Freigabe)
     ├─ anmeldung:  keine | entra-delegiert(scopes) | geheimnis(feld)
     └─ felder:     je Konfig-Feld: quelle = server | client | geheim
```

- **eigen:** von uns in Rust gebaut, signiert vom Server.
- **fremd:** übernommen (Registry, npm, PyPI, OCI, Remote-URL). Ein Admin muss
  jede Version ausdrücklich freigeben.
- **Versionen und Kanäle:** jeder Eintrag hat Versionen und optional Kanäle
  (`stabil`, `test`). Ein Satz verweist auf eine Version oder einen Kanal.

## 5. Zuweisung

```
Zuweisungsquelle ──► Satz ──► Einträge (Version/Kanal) + vorbelegte Werte
```

- **Satz:** benannte Sammlung von Einträgen, z. B. „Microsoft 365 Basis“.
- **Zuweisungsquelle:** ein Trait. Die erste Umsetzung sind **Entra-ID-Gruppen**.
  Später kommen App-Rollen und Azure-Rollen dazu.
- **Festlegung:** Firmenspezifische Abbildungen (welche Firmenrolle welchen
  Satz bekommt) liegen in der Server-Konfiguration des Betreibers, **nicht**
  in diesem Repo.
- Ein User bekommt die Vereinigung aller Sätze seiner Gruppen. Widersprechen
  sich vorbelegte Werte, gewinnt der Satz mit der höheren Priorität (Feld am
  Satz). Gleichstand ist ein Fehler in der Admin-Oberfläche, kein Zufall.

## 6. Konfiguration in Schichten

```
Standard des Eintrags  →  Werte des Satzes (Admin)  →  Werte am Gerät (User)
```

| Quelle | Wer füllt | Wo gespeichert |
|---|---|---|
| `server` | Admin | Server, wird mitgepusht |
| `client` | User am Gerät | Client-Konfiguration |
| `geheim` | User am Gerät | Schlüsselbund des Betriebssystems, verlässt das Gerät nie |

## 7. Authentifizierung

1. **Agent → Client:** Der Client lauscht nur lokal. Jeder Agent bekommt beim
   Koppeln im Client-Fenster ein eigenes Token. So weiß der Client, welcher
   Agent fragt, und kann Tools pro Agent filtern.
2. **Client → Server:** Entra-Login des Users (OAuth mit PKCE) plus ein
   Geräteschlüssel (Ed25519), der beim Einbinden registriert wird. Geräte
   lassen sich in der Admin-Oberfläche sperren.
3. **Tool → Dienst:** Für Microsoft holt der Client Tokens für die Scopes des
   Eintrags (`entra-delegiert`) mit dem Login des Users. Microsoft prüft die
   Rechte, nicht wir. Das Tool bekommt das Token pro Aufruf und speichert es
   nicht.

**Festlegung:** Zugangsdaten werden nie als Umgebungsvariable oder Datei
abgelegt, die ein Agent lesen könnte.

## 8. Laufzeiten im Client

| Laufzeit | Einsatz | Abschottung |
|---|---|---|
| `wasm` | bevorzugt für eigene Tools | wasmtime-Sandbox, nur freigegebene Rechte |
| `nativ` | eigene Tools, die mehr brauchen | Freigabe durch Admin, Signatur geprüft |
| `npx` / `uvx` / `oci` | fremde MCP-Server | Freigabe durch Admin, Node/Python/Container nötig |
| `remote` | entfernte MCP-Server über HTTP | Client fügt Zugangsdaten ein |

Gepushte Artefakte sind signiert. Der Client prüft jede Signatur vor dem Start.

## 9. Oberfläche

- **Client:** egui, Bausteine und Themes wie in crosspoint, wledambilight und
  sprechwerk (`dark`, `light`, `mono`).
- **Admin:** Vue mit `weppelmann-ui`.
- **Festlegung: 1:1 gleiches Aussehen.** Beide Oberflächen erzeugen ihre
  Themes aus **denselben** Token-Dateien (`tokens/*.tokens.json`). Bausteine
  gibt es in beiden Welten mit gleichem Namen und gleichem Maß.
  Bildschirmfotos beider Seiten werden in der CI nebeneinander erzeugt.
- **Festlegung: Hand-Cursor auf allem Klickbaren.** In egui ist das nicht
  Standard. Darum setzen die Bausteine `CursorIcon::PointingHand` zentral,
  und ein Oberflächentest prüft es. In Vue gilt `cursor: pointer` im Baustein.

## 10. Offene Punkte

1. Formate für eigene Tools: nur WASM, oder von Anfang an auch native Binaries?
2. Admin-Rechte: Wer darf die Bibliothek pflegen? Eigene Entra-Gruppe oder App-Rolle?
3. Datenbank des Servers: SQLite im Volume (wie sprechwerk) oder Postgres?
4. Soll der Client auch ohne Server nutzbar sein (nur lokale Einträge)?
5. Protokoll: Werden Tool-Aufrufe zentral protokolliert (Audit) oder nur lokal?
6. pi und Hermes: Schnittstellen der Plugins genau prüfen. pi 1.5 spricht MCP (`pi mcp`), für den MVP reicht das.
7. Erster Umfang des Microsoft-Satzes: Mail, Kalender, Teams, SharePoint/OneDrive, Planner?
8. Gemeinsames UI-Crate: aus den bestehenden Apps herauslösen, bevor der Client entsteht?
