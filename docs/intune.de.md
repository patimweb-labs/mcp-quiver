Englisch: [intune.md](intune.md)

# quiver mit Intune ausrollen

Zwei Schritte: Paket hochladen, Vorgaben ausrollen. Beides ist optional — quiver
läuft auch ohne Geräteverwaltung; dann aktualisiert man die Clients von Hand aus
dem Release.

## Windows

### 1. Das MSI hochladen

1. `quiver-<version>-windows-x86_64.msi` aus dem Release laden.
2. Intune → **Apps** → **Windows** → **Hinzufügen** → **Branchen-App (MSI)**.
3. MSI auswählen, Name `mcp-quiver`, Herausgeber `Patrick Weppelmann`, Version
   wie im Dateinamen.
4. Installationsbefehl: `msiexec /i "quiver-<version>-windows-x86_64.msi" /qn`.
   Deinstallationsbefehl: `msiexec /x "{ProductCode}" /qn` (den ProductCode
   trägt Intune aus dem MSI ein).
5. Voraussetzung: 64-bit Windows 10/11. Erkennungsregel: die Vorgaben des MSI
   stehen lassen (ProductCode und Version).
6. Einer Gruppe zuweisen; ein neueres MSI mit derselben `UpgradeCode` ersetzt
   die alte Fassung.

Das MSI ist **je Benutzer**: es installiert nach
`%LOCALAPPDATA%\Programs\mcp-quiver` und braucht keine Administratorrechte. Das
`MajorUpgrade` ersetzt eine ältere Fassung; der Restart Manager schließt eine
laufende `quiver.exe` (WM_CLOSE, geordnet) und startet sie nach dem Update
wieder. Deinstallieren lässt das Profil (`%APPDATA%\mcp-quiver`) stehen.

### 2. Die Vorgaben ausrollen

Intune → **Geräte** → **Konfiguration** → **Erstellen** → **Benutzerdefiniert**
(Windows 10 und höher) mit ADMX/ADML aus `verteilung/vorgaben/`:

* **quiver.admx** in den ADMX-Speicher, **de-DE/quiver.adml** und
  **en-US/quiver.adml** dazu.
* Dann unter **mcp-quiver** einstellen:
  * *Server-Adresse*: z. B. `https://quiver.example.test` — fest, im Fenster
    nicht änderbar,
  * *Beim Anmelden starten*: an oder aus,
  * *Text des Update-Hinweises*: z. B. `Version {version} kommt über die
    Geräteverwaltung`.

Die Werte landen in `HKLM\Software\Policies\mcp-quiver`; quiver liest sie nur.
Kein ADMX? Eine einzelne Registry-Vorlage mit denselben drei Werten tut es
auch.

## macOS

### 1. Das `.pkg` hochladen

1. `quiver-<version>-macos-<arch>.pkg` aus dem Release laden (`arm64` für Apple
   Silicon, `x86_64` für Intel).
2. Intune → **Apps** → **macOS** → **Hinzufügen** → **macOS-App (PKG)**.
3. Installationsbefehl: `installer -pkg "quiver-<version>-macos-<arch>.pkg"
   -target /` (das ist auch die Vorgabe von Intune).
4. Erkennungsregel: Bündel `de.weppelmann.mcp-quiver` oder die Datei
   `/Applications/quiver.app`.

Das `.pkg` installiert nach `/Applications` und ersetzt eine ältere Fassung. Das
Skript `preinstall` beendet einen laufenden Client (SIGTERM, geordnet) und merkt
sich, dass er lief; `postinstall` startet ihn für den angemeldeten Benutzer
wieder. Das Profil (`~/Library/Application Support/mcp-quiver`) bleibt
unberührt.

Das `.dmg` bleibt für die Installation von Hand.

### 2. Die Vorgaben ausrollen

Intune → **Geräte** → **Konfiguration** → **Erstellen** → **Neue Richtlinie** →
**macOS** → **Vorlagen** → **Benutzerdefiniert**. Die Datei
`verteilung/vorgaben/de.weppelmann.mcp-quiver.mobileconfig` importieren (vorher
`https://quiver.example.test` ersetzen).

Das Profil schreibt die verwalteten Einstellungen für
`de.weppelmann.mcp-quiver` — die einzige Stelle, aus der quiver sie liest. Die
eigenen Einstellungen des Benutzers bleiben unberücksichtigt.

## Linux (ohne Intune)

Root schreibt `/etc/mcp-quiver/vorgaben.json` (siehe
`verteilung/vorgaben/vorgaben.json`) — Eigentümer root, Modus 0644. Eine Datei,
die welt-schreibbar ist oder nicht root gehört, wird ignoriert und protokolliert.

## Was die Geräte zeigen

Der Client zeigt einen ruhigen Hinweis, wenn der Server eine neuere Version
meldet. Liegt eine Vorgabe vor, kommt der Text aus `UpdateHinweis` (Vorgabe:
„Version X kommt über die Geräteverwaltung“). Die Verwaltung listet jedes Gerät,
dessen Client älter ist — siehe [updates.de.md](updates.de.md).

### Selbst-Update oder Geräteverwaltung

Gibt es ein Paket für die Installation, bietet der Client zusätzlich **Jetzt
aktualisieren** an: Er lädt das Paket von seinem eigenen Server, prüft Größe und
SHA-256, startet die neue Fassung als `quiver --selbsttest` und tauscht erst
dann. Das Profil (Einstellungen, Tresor, Geräte-Token, Anmeldungen der
Werkzeuge) bleibt unberührt; laufende Werkzeuge werden vorher beendet, und der
Client startet sich selbst neu.

Für verwaltete Flotten ist das meist **nicht** gewünscht — die Version soll
Intune gehören. Dafür *Update selbst einspielen* auf **aus** stellen
(`UpdateAngebot=false`): Der Client zeigt dann nur den Hinweis, keinen Knopf.
Die Vorgabe ist an.

Das `.pkg` und das MSI passen zu beiden Wegen: Intune kann sie einspielen, und
der Client nimmt dieselben Pakete (MSI je Benutzer, `.dmg` mit dem `.app`),
wenn er es darf.

## Die Pakete vor dem Ausrollen prüfen

```bash
# macOS: hineinsehen, nicht installieren
pkgutil --expand-full quiver-<version>-macos-arm64.pkg /tmp/quiver-pkg
ls /tmp/quiver-pkg/*.pkg/Payload/Applications/quiver.app/Contents/MacOS/quiver
ls /tmp/quiver-pkg/*.pkg/Scripts/

# Windows: stille Installation, Update, Deinstallation — auf einem Testrechner
msiexec /i quiver-<version>-windows-x86_64.msi /qn
msiexec /i quiver-<next>-windows-x86_64.msi /qn
msiexec /x quiver-<next>-windows-x86_64.msi /qn
```

Die CI macht dasselbe auf Pull Requests (Windows-Läufer und macOS-Läufer), ein
kaputtes Paket scheitert also dort zuerst.
