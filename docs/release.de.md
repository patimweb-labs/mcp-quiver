English: [release.md](release.md)

# quiver ausliefern

Ein Release bereitet release-please vor. Wird der Release-Pull-Request gemergt,
entsteht der Tag `vX.Y.Z`, und **derselbe** Workflow baut den Client für macOS
(Apple Silicon und Intel), Windows und Linux (x86_64 und aarch64), legt das
GitHub-Release an und hängt alle Pakete samt `SHA256SUMS` daran. Das
Server-Abbild baut `abbild.yml`: Bei einem Tag, den dieser Workflow selbst
erzeugt hat, stößt er den Workflow ausdrücklich an; ein von Hand geschobener Tag
löst ihn direkt aus.

| Datei | Wofür |
| --- | --- |
| `.github/workflows/release.yml` | release-please, Versionsprüfung, fünf Baujobs, Prüfsummen, Herkunft, Release |
| `release-please-config.json` | welche Dateien ein Release anhebt (`Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`) |
| `.release-please-manifest.json` | die zuletzt veröffentlichte Version |
| `verteilung/macos/paket.sh` | `quiver.app`, `.dmg`, Signieren und Notarisieren |
| `verteilung/linux/paket.sh` | `.tar.gz`, `.deb`, `.rpm`, AppImage |
| `verteilung/windows/paket.ps1` | `.msi` (je Benutzer) und portables `.zip` |
| `verteilung/version.sh` | die Version aus `Cargo.toml` |

## Ein Release machen

Releases bereitet [release-please](https://github.com/googleapis/release-please)
aus den Conventional Commits vor. Die Version steht an einer Stelle,
`[workspace.package] version` in `Cargo.toml`; die Kisten erben sie, und
`release-please-config.json` hält `Cargo.toml`, `Cargo.lock` und
`CHANGELOG.md` zusammen.

1. **Arbeit nach `main` mergen** — über einen Pull Request, mit
   Commit-Zeilen, die release-please versteht (`feat:`, `fix:`, `perf:`, …).
   Jeder Push auf `main` aktualisiert oder öffnet den Release-Pull-Request
   `chore: Release <version>`.
2. **Den Release-Pull-Request mergen.** Er hebt nur Version, Sperrdatei und
   Changelog an; ein von `GITHUB_TOKEN` geöffneter Pull Request startet keine
   weiteren Workflows, deshalb vorher lokal prüfen.
3. **Derselbe Push baut und veröffentlicht.** release-please legt den Tag
   `v<version>` und das Release an; der Rest von `release.yml` baut die fünf
   Pakete, schreibt `SHA256SUMS` und den Herkunftsnachweis, hängt den
   Paketblock an die Release-Notizen und stößt `abbild.yml` für denselben Tag
   an. Tag und Bau sind ein Workflow, weil ein von `GITHUB_TOKEN` erzeugter Tag
   keinen weiteren Workflow auslöst. `workflow_dispatch` ist die eine Ausnahme,
   über die das Abbild angestoßen wird.
4. **Dem Lauf zusehen.** *Actions → Release*. Ist er grün, steht das Release
   unter *Releases* — mit allen Paketen, `SHA256SUMS`, den Herkunftsnachweisen
   und den Abbild-Namen `X.Y.Z`, `X.Y`, `latest`, `sha-<commit>`.

### Welcher Job wann läuft

GitHub prüft in einem Job **ohne** Statusfunktion stillschweigend `success()`
über **alle** Vorfahren. Ein übersprungener Job würde damit die folgenden
mitreißen — bei einem Tag von Hand, einem Pull Request oder *Run workflow*
bliebe `release_please` leer, und `pakete`, `veroeffentlichen` und `abbild`
fielen mit aus. Deshalb steht in jeder Bedingung ausdrücklich
`!cancelled() && needs.<job>.result == 'success' && …`; `!cancelled()` statt
`always()`, damit ein abgebrochener Lauf nicht weiterbaut.

| Auslöser | `release_please` | `version` | `pakete` | `veroeffentlichen` | `abbild` |
| --- | --- | --- | --- | --- | --- |
| Tag `vX.Y.Z` von Hand geschoben | übersprungen | prüft den Tag | baut fünf Pakete | legt das Release an oder ergänzt es | übersprungen — der Tag hat `abbild.yml` selbst gestartet |
| Release-PR gemergt (Push nach `main`) | legt Tag und Release an | prüft den Tag, `selbst_erzeugt=ja` | baut fünf Pakete | hängt Pakete, Prüfsummen und Herkunft an | stößt `abbild.yml` für den Tag an |
| Push nach `main` ohne neues Release | führt nur den Release-PR | kein Tag, `veroeffentlichen=nein` | übersprungen | übersprungen | übersprungen |
| Pull Request | übersprungen (läuft nur auf `main`) | kein Tag, `veroeffentlichen=nein` | baut, ohne Geheimnisse | übersprungen | übersprungen |
| *Run workflow* | übersprungen | kein Tag, `veroeffentlichen=nein` | baut | übersprungen | übersprungen |

**Bei einem Pull Request und bei *Run workflow* wird nie veröffentlicht.**
`version` setzt `veroeffentlichen=ja` nur, wenn ein Tag im Spiel ist: ein
Tag-Push oder ein Tag, den release-please in diesem Lauf angelegt hat. Jeder
andere Weg endet bei `nein`, und `veroeffentlichen` verlangt zusätzlich
`needs.pakete.result == 'success'`. Angelegt, geändert oder hochgeladen wird
dort also nichts.

### Tag von Hand (Rückweg)

Ohne release-please geht der Weg von Hand weiterhin:

1. `[workspace.package] version` anheben, `Cargo.lock` auffrischen
   (`cargo check --workspace`), alles prüfen (AGENTS.md §4) und über einen
   Pull Request mergen.
2. Tag setzen und schieben:

   ```bash
   git tag -a v0.2.0 -m "quiver 0.2.0"
   git push origin v0.2.0
   ```

3. Der Tag-Push löst `release.yml` (Bau und Release) und `abbild.yml` (Abbild)
   selbst aus. release-please ist nicht beteiligt.

Der erste Job vergleicht den Tag mit der Version aus `Cargo.toml`. `v0.2.0`
neben `0.1.0` bricht ab, **bevor** etwas gebaut wird: der Name des Releases
und die Version, die das Programm dem Server meldet, dürfen nicht
auseinanderlaufen.

### Wenn etwas schiefging

* *Run workflow* baut die Pakete erneut und legt sie als Artefakte an den Lauf;
  ein Release wird dabei **nicht** angefasst. Gut zum Ausprobieren eines Fixes.
* Denselben Tag noch einmal schieben geht nicht; löschen und neu schieben nur,
  wenn das Release ersetzt werden soll.
* Eine Plattform ist gescheitert, die anderen liefen: der Lauf ist rot und es
  wird **nichts veröffentlicht** — `veroeffentlichen` startet nur, wenn alle
  fünf Baujobs erfolgreich waren. Die Pakete der gelungenen Läufe bleiben als
  Artefakte am Lauf. Reparieren und neu taggen (`v0.2.1`) — oder, solange der
  Tag noch nicht draußen ist, Release und Tag löschen, reparieren, neu taggen.
* Das Abbild wurde nicht angestoßen: von Hand *Actions → Abbild → Run
  workflow* auf dem Tag starten. Der Lauf checkt den Tag aus und benennt das
  Abbild mit seiner Version.

## Was wo gebaut wird

| Läufer | Architektur | Pakete |
| --- | --- | --- |
| `macos-latest` (arm64) | arm64 | `.dmg` mit `quiver.app` |
| `macos-15-intel` | x86_64 | `.dmg` mit `quiver.app` |
| `windows-2025` | x86_64 | `.msi` und portables `.zip` |
| `ubuntu-22.04` | x86_64 (glibc 2.35) | `.deb`, `.rpm`, AppImage, `.tar.gz` |
| `ubuntu-22.04-arm` | aarch64 | `.deb`, `.rpm`, AppImage, `.tar.gz` |

Gebaut wird **nativ auf dem jeweiligen Läufer** (`cargo build --release
--locked`), nie gekreuzt: Rust unter Emulation braucht eine Stunde statt
Minuten. Die Kisten kommen aus `rust-toolchain.toml`, die CI nimmt also
denselben Compiler wie die Entwicklung.

`permissions: {}` ist der Standard; nur der letzte Job darf schreiben
(`contents: write` für das Release, `attestations: write` und `id-token: write`
für die Herkunft). Jedes `checkout` nutzt `persist-credentials: false` und
`submodules: recursive`. Es gibt kein `pull_request_target`, und keine
Nutzereingabe geht direkt in ein `run:` — Werte laufen über `env:`.

## Geheimnisse (alle optional)

Ohne sie wird das macOS-Bündel **ad hoc** signiert und Windows bleibt
unsigniert — gebaut und lauffähig ist trotzdem alles. Als Repo-Secrets:

| Secret | Was es ist |
| --- | --- |
| `MACOS_CERTIFICATE` | Developer-ID-Application-Zertifikat, `.p12`, base64-kodiert |
| `MACOS_CERTIFICATE_PASSWORD` | Passwort dieser `.p12` |
| `MACOS_SIGNING_IDENTITY` | z. B. `Developer ID Application: Firma GmbH (TEAMID)` |
| `APPLE_ID` | Apple-ID für den Notardienst |
| `APPLE_TEAM_ID` | Team-Id (zehn Zeichen) |
| `APPLE_APP_PASSWORD` | app-spezifisches Passwort für `notarytool` |

```bash
base64 -i DeveloperID.p12 | pbcopy          # -> MACOS_CERTIFICATE
security find-identity -v -p codesigning    # -> MACOS_SIGNING_IDENTITY
```

Alle sechs müssen stehen, damit signiert **und** notarisiert wird: mit
Zertifikat, aber ohne die Apple-Angaben, ist das Bündel signiert, aber nicht
notarisiert — Gatekeeper fragt dann trotzdem. Was tatsächlich passiert ist,
schreibt der Bau nach `dist/SIGNIERT.txt`, und das landet im Release-Text
(`signiert-und-notarisiert`, `signiert`, `unsigniert`).

Pull Requests **fassen keine Geheimnisse an**: für sie gibt es einen eigenen
macOS-Schritt, in dessen Umgebung kein `secrets.*` steht. Schlüsselbund und
entschlüsseltes Zertifikat werden am Ende des Skripts gelöscht, auch wenn es
scheitert (`trap … EXIT`).

## Was „unsigniert“ bedeutet

* **macOS:** das Bündel ist ad hoc signiert (`codesign -s -`). Auf dem Rechner,
  der es gebaut hat, startet es; woanders blockiert Gatekeeper den ersten
  Start. Rechtsklick → *Öffnen* oder *Systemeinstellungen → Datenschutz &
  Sicherheit → „Dennoch öffnen“*. Mit Notarisierung entfällt der Schritt.
* **Windows:** `.msi` und `.zip` tragen keine Signatur. SmartScreen meldet
  *Der Computer wurde durch Windows geschützt*: *Weitere Informationen →
  Trotzdem ausführen*. Das `.msi` installiert je Benutzer und braucht keine
  Administratorrechte.

Beides ist kein Sicherheitsloch, sondern das Fehlen eines bezahlten
Zertifikats. Prüfsummen und Herkunftsnachweis zeigen weiterhin, woher die
Dateien kommen.

## Was in den Paketen steckt

* **macOS:** `quiver.app` mit `Contents/MacOS/quiver`, `Contents/Info.plist`
  und `Contents/Resources/AppIcon.icns`. Bündel-Kennung
  `de.weppelmann.mcp-quiver` — dieselbe Kennung, die der Client als App-Id
  meldet. `LSUIElement` ist **false**: quiver zeigt ein Fenster, steht im Dock
  und hat eine Menüleiste; das Tray-Symbol ist der zweite Weg dorthin, nicht
  der einzige. Mindestsystem 11.0, passend zu `MACOSX_DEPLOYMENT_TARGET`. Das
  Symbol entsteht aus `assets/marke/quiver-dock.png` — kein neues Logo.
* **Linux:** das `.tar.gz` ist der schlichte Baum (`bin/quiver`, `share/…`);
  `.deb` und `.rpm` legen dieselben Dateien unter `/usr` ab. Die
  Abhängigkeiten sind das, was eframe und das Tray beim Start laden:
  libGL/libEGL, libxkbcommon (samt X11-Teil), Wayland und X11. Das Tray ist
  **kein** GTK — der Client spricht D-Bus (StatusNotifierItem) selbst —, aber
  es braucht etwas, das die Symbole anzeigt; daher
  `Recommends: gnome-shell-extension-appindicator` (KDE, Xfce und Cinnamon
  bringen es mit).
* **Windows:** `.msi` nur für den angemeldeten Benutzer, Installation nach
  `%LOCALAPPDATA%\Programs\mcp-quiver`, mit Startmenü-Verknüpfung, Symbol unter
  *Apps und Features* und Deinstallation ohne Administratorrechte; dazu das
  portable `.zip` mit `quiver.exe` neben `LICENSE` und `README.md`.
* **In den Paketen stecken keine Werkzeuge mehr.** Der Client bekommt sie bei
  der Anmeldung vom Server und hält sie auf dessen Stand (Aufgabe 37); die
  Programme je Plattform liefert das Release von
  [`quiver-tools`](https://github.com/patimweb-labs/quiver-tools) (Aufgabe 38).
* Autostart steckt in keinem Paket: den richtet der Client selbst ein
  (LaunchAgent, `HKCU\…\Run`, XDG-Autostart), wenn der Benutzer ihn einschaltet.

## Ein Paket lokal bauen

Die Skripte sind dieselben, die die CI aufruft:

```bash
verteilung/macos/paket.sh                     # macOS: dist/quiver.app, dist/*.dmg
verteilung/linux/paket.sh                     # Linux: tar.gz, deb, rpm, AppImage
pwsh -File verteilung/windows/paket.ps1       # Windows: msi + zip
verteilung/version.sh                         # gibt die Version aus
```

Alles landet in `dist/` (nicht eingecheckt). `ARCH=arm64` (macOS) oder
`ARCH=aarch64` (Linux) baut die andere Architektur, wenn der Rechner das kann.
`CARGO_TARGET_DIR` wird beachtet, ein eigenes Zielverzeichnis funktioniert
also.

`verteilung/linux/paket.sh` holt `nfpm` und `appimagetool` nach
`dist/werkzeuge/`, mit fester Version **und** fester SHA-256; sonst wird
nichts geladen.

Das macOS-Bündel ausprobieren, ohne das echte Profil zu berühren:

```bash
profil=$(mktemp -d)
QUIVER_PROFIL="$profil" QUIVER_MCP_ADRESSE=127.0.0.1:0 QUIVER_TRESOR=speicher \
  ./dist/quiver.app/Contents/MacOS/quiver --headless
# Strg+C beendet es; danach "rm -rf $profil"
```

`--headless` startet nur den Kern. Ohne das Argument kommen Fenster und Tray,
mit `--hintergrund` nur das Tray. Port `0` lässt das System einen freien Port
wählen, damit eine zweite Instanz nie mit der laufenden kollidiert.

## Einen Download prüfen

```bash
sha256sum -c SHA256SUMS                       # macOS: shasum -a 256 -c SHA256SUMS
gh attestation verify quiver-0.2.0-macos-arm64.dmg --repo patimweb-labs/mcp-quiver
```

`SHA256SUMS` deckt jedes Paket des Releases ab. Der Herkunftsnachweis sagt, in
welchem Lauf, aus welchem Commit und von welchem Workflow die Datei gebaut
wurde; er ist von GitHubs OIDC-Identität signiert und braucht keinen eigenen
Schlüssel.

## Was nur die CI prüfen kann

Lokal prüfbar: die Workflow-Syntax (`actionlint`), die Skripte (`bash -n`), das
macOS-Bündel samt Start, die `.icns`, das `.dmg`. Alles andere — das
Intel-Bündel, das MSI, `.deb`/`.rpm`/AppImage, Signieren und Notarisieren, das
Release selbst — passiert nur in der CI. Ein Probelauf ohne Release:
*Run workflow* auf dem Branch.
