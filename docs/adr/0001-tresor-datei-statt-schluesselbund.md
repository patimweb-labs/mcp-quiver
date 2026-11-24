# 0001 – Geheimnisse in einer eigenen Tresor-Datei statt im Schlüsselbund

Stand: 2026-11-23 · gilt für mcp-quiver (Client), quiver-m365 und quiver-zep

## Kontext

Der Client (Geräte-Token, Werte mit `quelle: geheim`), quiver-m365 (Refresh-Token) und
quiver-zep (Refresh-Token, Client-Registrierung) legten ihre Geheimnisse im Schlüsselbund des
Betriebssystems ab (`keyring`).

- **macOS** fragt nach jedem neuen Build erneut nach („Immer erlauben“). Bis dahin hängt
  der Prozess, und der Client wartet ohne Fehlermeldung. Das hat im Alltag ständig gestört.
- **Linux** hat je nach Distribution keinen Secret Service. Ohne D-Bus oder
  gnome-keyring geht dann nichts, und der Build braucht D-Bus-Header.
- **Windows** hat den Credential Manager, aber ein drittes Verhalten mehr.

## Entscheidung

Jedes Programm hat einen eigenen **`DateiTresor`** in seinem Ordner:

| Programm | Ordner |
|---|---|
| Client | `<profil>/tresor` (`dirs::config_dir()/mcp-quiver`, bzw. `QUIVER_PROFIL`) |
| quiver-m365 | `dirs::data_dir()/quiver-m365/tresor` |
| quiver-zep | `<zustand>/tresor` (`ZEP_ZUSTAND`, sonst `~/.local/state/quiver-zep`) |

- `tresor.schluessel` enthält 32 zufällige Bytes.
- `tresor.json` enthält die Schlüssel/Wert-Liste, verschlüsselt mit XChaCha20-Poly1305 und
  frischer Nonce bei jedem Schreiben.
- Unter Unix gelten die Rechte 0700 für den Ordner und 0600 für die Dateien. Unter Windows
  schützt das Benutzerprofil.
- Mehrere Prozesse dürfen gleichzeitig zugreifen. Jede Änderung liest unter einer
  Dateisperre (`File::lock`, Standardbibliothek) neu und ersetzt die Datei atomar.
- **Kein Umzug, kein Schlüsselbund mehr:** Ein erster Entwurf hat fehlende Werte einmal im
  alten Schlüsselbund gesucht. Weil `quiver-zep status` alle paar Sekunden als eigener
  Prozess läuft, kam der Dialog dadurch immer wieder. Deshalb ist `keyring` ganz entfernt.
  Geheimnisse werden einmal neu eingegeben, Anmeldungen einmal über den Knopf wiederholt.
- `QUIVER_TRESOR=speicher`, `M365_TRESOR=speicher` und `ZEP_TRESOR=speicher` bleiben für
  Tests und Läufe ohne Spuren.

## Folgen

- Dasselbe Verhalten auf macOS, Linux und Windows, ohne Dialoge.
- **Sicherheit, ehrlich:** Wer den Benutzer-Account hat, kann beide Dateien lesen. Das gilt
  praktisch auch für den Schlüsselbund, der für den angemeldeten Benutzer offen ist. Die
  Verschlüsselung verhindert, dass beim versehentlichen Lesen Klartext herauskommt, etwa
  durch `cat`, `grep`, einen Agenten oder eine Sicherung nur von `tresor.json`.
- Später möglich: ein Hauptpasswort beim Start, das den Schlüssel ableitet (Argon2), oder
  wahlweise wieder ein Schlüsselbund als eigener `Tresor`. Beides passt hinter die bestehende
  `Tresor`-Schnittstelle.
