# Mitentwickeln

Die verbindlichen Regeln stehen in [`AGENTS.md`](AGENTS.md) — für Menschen wie
für Werkzeuge. Hier das, was man zum ersten Pull Request braucht.

## Weg

1. `git switch -c fix/kurzbeschreibung origin/main`
2. ändern, prüfen, committen
3. `git push -u origin HEAD`, `gh pr create`
4. CI abwarten (dieselben Schritte wie unten), mergen

`main` ist immer lauffähig: kein direkter Push, kein Force-Push, kein Löschen.
Reviews sind nicht erzwungen (Solo-Betrieb), der Pull Request ist es.

## Commits

Conventional Commits, ein Gedanke pro Commit, deutschsprachige Betreffzeile
erlaubt: `fix: Sperrliste hebt eine Adresssperre wieder auf`. Releases macht
`release-please` aus diesen Zeilen — `feat:`, `fix:`, `perf:`, `refactor:`,
`docs:`, `build:`, `ci:`, `test:`.

Der Release-PR heißt `chore: Release <version>` und wird nicht von Hand
angefasst: er hebt `Cargo.toml`, `Cargo.lock` und `CHANGELOG.md` an. Wird er
gemergt, legt derselbe Workflow Tag, Pakete und Server-Abbild an
([docs/release.de.md](docs/release.de.md)).

## Prüfen

Das ist der Befehl vor jedem Commit:

```bash
cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
cargo deny check
```

`cargo test` **mit** `--workspace`: ohne das prüft Cargo nur das Wurzelpaket
und meldet „0 passed".

Für die Verwaltung:

```bash
cd admin
npm ci
npm run typecheck
npm test
npm run build
```

Beide Browser-Prüfungen (`npm run pruefung`, `npm run pruefung:anmeldung`)
starten einen echten Server auf Loopback mit temporärem Datenordner; sie
brauchen weder einen Mandanten noch einen echten Tresor.

## Was zu einem PR gehört

Die Liste steht in [`AGENTS.md`](AGENTS.md) §5 und noch einmal in der
[Vorlage](.github/pull_request_template.md). Der kurze Kern: Test dabei (ein Fix
ohne Test ist ein Fix auf Probe), `clippy -D warnings` sauber, alle drei
Farbschemata geprüft, Tastaturbedienung und Fokus sichtbar, keine Hex-Werte und
keine Pixelmaße im Code — nur Tokens (AGENTS.md §2.6), keine neue Abhängigkeit
ohne Begründung, Dokumentation nachgezogen. Bei Änderungen an der Oberfläche
gehört ein Screenshot dazu; er wird erzeugt, nicht von Hand abgelegt
([docs/entwicklung.de.md](docs/entwicklung.de.md#bilder)).

## Sicherheitslücken

Bitte **nicht** als Issue, sondern über GitHub „Private vulnerability
reporting“ — siehe [`SECURITY.md`](SECURITY.md).
