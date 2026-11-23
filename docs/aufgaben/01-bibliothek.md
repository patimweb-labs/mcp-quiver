# 01 – Bibliothek: server.json ↔ Eintrag

Crate: `crates/bibliothek` (`quiver-bibliothek`). Abhängigkeiten: nur die schon
eingetragenen (`quiver-domain`, `serde`, `serde_json`, `thiserror`).

## Ziel

Ein MCP-Server, beschrieben im `server.json`-Format der offiziellen MCP Registry,
wird zu einem `quiver_domain::Eintrag` – und zurück.

## Schnittstelle (genau so)

```rust
pub const META_SCHLUESSEL: &str = "io.github.patimweb-labs.quiver/eintrag";

#[derive(Debug, thiserror::Error)]
pub enum ImportFehler { /* JSON kaputt, kein Paket und kein Remote, unbekannter Pakettyp, … */ }

/// server.json (als JSON-Wert) → Eintrag.
pub fn importieren(server_json: &serde_json::Value) -> Result<Eintrag, ImportFehler>;

/// Eintrag → server.json. Der vollständige Eintrag steht unter `_meta[META_SCHLUESSEL]`,
/// damit `importieren(exportieren(e)) == e` gilt.
pub fn exportieren(eintrag: &Eintrag) -> serde_json::Value;
```

## Das Format (Auszug, was wir lesen)

```json
{
  "name": "io.github.someone/weather",
  "description": "…",
  "version": "1.2.0",
  "title": "Weather",                       // optional
  "packages": [{
    "registryType": "npm",                  // npm | pypi | oci
    "identifier": "@someone/weather-mcp",
    "version": "1.2.0",
    "transport": { "type": "stdio" },
    "packageArguments": [ { "type": "positional", "value": "--fast" } ],
    "environmentVariables": [
      { "name": "API_KEY", "description": "…", "isRequired": true, "isSecret": true, "default": null }
    ]
  }],
  "remotes": [{
    "type": "streamable-http",
    "url": "https://example.com/mcp",
    "headers": [ { "name": "Authorization", "description": "…", "isRequired": true, "isSecret": true } ]
  }],
  "_meta": { "io.github.patimweb-labs.quiver/eintrag": { /* optional: ganzer Eintrag */ } }
}
```

## Regeln für `importieren`

1. Steht unter `_meta[META_SCHLUESSEL]` ein Eintrag, gilt **nur** dieser
   (deserialisieren, fertig).
2. Sonst `herkunft = Fremd`, und:
   - `id`: letzter Teil von `name` nach `/`, klein, alles außer `a-z0-9-` → `-`,
     doppelte `-` zusammenfassen, `-` am Rand weg. Leer → Fehler.
   - `titel`: `title`, sonst die `id`. `beschreibung`: `description`. `version`: `version`.
   - Gibt es ein Paket mit `transport.type == "stdio"`, nimm das **erste** davon:
     - `npm` → `Stdio { befehl: "npx", argumente: ["-y", "<identifier>@<version>", …paketargumente] }`
     - `pypi` → `Stdio { befehl: "uvx", argumente: ["<identifier>==<version>", …] }`
     - `oci` → `Stdio { befehl: "docker", argumente: ["run", "-i", "--rm", für jede Umgebungsvariable "-e", "NAME", "<identifier>:<version>", …] }`
     - Paketargumente: `positional` → `value`; `named` → `name` und danach `value` (falls vorhanden).
     - Jede Umgebungsvariable wird ein `Feld { name, beschreibung, quelle: Geheim wenn isSecret sonst Client, pflicht: isRequired, standard: default }`
       und in `umgebung` steht `NAME = "${NAME}"`.
   - Sonst: erstes Remote mit `type == "streamable-http"` →
     `Remote { url, kopfzeilen }`; jeder Header wird ein Feld wie oben, in `kopfzeilen`
     steht `Name = "${Name}"`.
   - Weder noch → Fehler.
3. Fehlende optionale Felder sind kein Fehler.

## Abnahme

- Tests in `crates/bibliothek/tests/`, mit Beispieldateien in `crates/bibliothek/tests/beispiele/`:
  - npm mit geheimer Umgebungsvariable und Paketargumenten
  - pypi ohne Variablen
  - oci mit zwei Variablen (Reihenfolge der `-e` stimmt)
  - nur Remote mit Header
  - mit `_meta`-Eintrag (der gewinnt, auch wenn `packages` etwas anderes sagt)
  - Rundlauf `importieren(&exportieren(&e)) == e` für einen eigenen Eintrag
  - Fehlerfälle: weder Paket noch Remote; Name, aus dem eine leere `id` wird
- Prüfbefehl aus `docs/aufgaben/README.md` grün.
