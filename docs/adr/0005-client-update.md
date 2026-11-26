# ADR 0005 — Der Client bietet das Update an

**Stand:** 2026-11-25 · **Aufgabe:** 39 · **Ersetzt:** [ADR 0004](0004-updates.md)

## Kontext

ADR 0004 hat festgelegt, dass quiver sich **nicht** selbst aktualisiert: der
Client zeigt einen Hinweis, eingespielt wird durch die Geräteverwaltung
(Intune) oder von Hand aus den Paketen des Releases. Das war die richtige
Antwort auf den damaligen Entwurf — einen eigenen Signaturschlüssel, einen
eigenen Spiegel, einen Helferprozess, der ein laufendes Programm tauscht.

In der Praxis bleibt damit eine Lücke: wer **keine** Geräteverwaltung hat,
muss bei jeder Version von Hand ein Paket herunterladen, installieren und
darauf hoffen, dass Profil und Anmeldungen heil bleiben. Der Aufwand liegt
beim Benutzer, und die Versionen auf den Geräten laufen auseinander.

Zwei Dinge haben sich seit 0004 geändert:

1. **Es gibt jetzt einen Server, der die Veröffentlichung ohnehin kennt.** Er
   fragt die Releases ab (Aufgabe 34), prüft sie und liefert den Clients
   bereits Programme aus (`GET /geraet/tools/{name}`, gestreamt, mit Größe,
   `ETag` und Drossel, nur an das zugewiesene Gerät).
2. **Es gibt `SHA256SUMS` im Release.** Damit lässt sich ein Paket prüfen,
   ohne einen eigenen Schlüssel zu betreiben: die Prüfsumme entsteht im selben
   Release wie das Paket, und der Client prüft sie erneut, bevor er tauscht.

## Entscheidung

1. **Der Client erfährt die Version von seinem Server und bietet sie an.** Kein
   Client fragt GitHub, je. Der Server nennt die neueste Version (nur neuere,
   nie Vorabfassungen, nie zurück) und die Pakete, die zu **seinem** System
   gehören — mit Größe und SHA-256. Der Hinweis bekommt zwei Knöpfe: „Jetzt
   aktualisieren“ und „Später“.
2. **Der Server holt und prüft das Paket.** Er lädt es einmal über `https` von
   der Veröffentlichung, prüft Größe und SHA-256 gegen `SHA256SUMS`, legt es
   unter `<daten>/updates/<version>/` ab und liefert es dem Gerät mit dessen
   Token aus — gestreamt, mit `Content-Length`, `ETag` und Drossel, wie die
   Programme. Ohne Prüfsumme gibt es kein Paket. Der Server ist damit der
   einzige Weg nach draußen; die Geräte sprechen weiter nur mit ihm.
3. **Kein Signaturschlüssel, kein fremdes Werkzeug.** Geprüft wird mit
   SHA-256 aus derselben Veröffentlichung; entpackt wird mit `flate2` (ZIP,
   tar.gz) und auf macOS mit den Bordmitteln `hdiutil` und `ditto`. Kein
   `unzip`, kein `tar`, kein Installer-Framework, kein Schlüsselbund.
4. **Getauscht wird je Art der Installation, vom Client selbst erkannt** (aus
   seinem eigenen Pfad):

   | So liegt quiver | Tausch |
   | --- | --- |
   | `…\quiver.exe` (portabel, ZIP) | laufende Datei umbenennen, neue daneben, Neustart |
   | `%LOCALAPPDATA%\Programs\mcp-quiver\quiver.exe` (MSI je Benutzer) | `msiexec /i … /qn`, Client beendet sich, das MSI startet ihn wieder |
   | `….app/Contents/MacOS/quiver` | `.app` aus dem `.dmg` daneben, Bündel tauschen, Neustart |
   | `…/quiver-*.AppImage` | Datei ersetzen, Neustart |
   | `…/bin/quiver` (aus `.tar.gz`) | Datei ersetzen, Neustart |
   | `/usr/bin/quiver` aus `.deb`/`.rpm` | **nur Hinweis** (braucht root) |
   | Ordner ohne Schreibrecht | **nur Hinweis** |

5. **Erst prüfen, dann tauschen.** Die neue Fassung wird entpackt und mit
   `quiver --selbsttest` gestartet (Exit 0, ohne Fenster, ohne Profil).
   Scheitert das, bleibt alles, wie es ist, mit einer klaren Meldung. Die alte
   Fassung bleibt als `<name>.alt` liegen; eine Marke im Profil sorgt dafür,
   dass ein zweiter Start ohne Beweis die alte Fassung zurückholt.
6. **Das Profil wird nicht angefasst.** Getauscht wird nur das Programm. Profil,
   Tresor, Geräte-Token, Angaben, Zugänge, Anmeldungen der Werkzeuge, Autostart
   und die vom Server geladenen Programme bleiben, wo sie sind. Ein Test
   vergleicht das Profil vor und nach dem Tausch.
7. **Die Firma kann das Angebot abschalten** (`UpdateAngebot` in den Vorgaben,
   Standard: an). Dann bleibt es beim Hinweis; eingespielt wird durch die
   Geräteverwaltung.

## Preis

* Der Server lädt Pakete (bis 512 MiB) auf die Platte. Das ist Speicher im
  Datenordner und eine Verbindung nach außen, die er ohnehin schon hat. Alte
  Versionen werden nicht automatisch gelöscht — sie bleiben liegen, bis jemand
  aufräumt.
* Der Tausch eines laufenden Programms ist der heikelste Teil. Er ist deshalb
  klein gehalten (umbenennen, daneben legen), für jede Art einzeln geprüft und
  mit einer Sicherung versehen. Für das MSI macht es Windows selbst (Restart
  Manager); dort gibt es keinen Selbsttest vorher, weil das MSI ein
  Installationsprogramm und kein Programm ist.
* Der Selbsttest ist die einzige Zusicherung vor dem Tausch. Eine Fassung, die
  erst später scheitert, tauscht trotzdem; die Sicherung bleibt dann liegen.
* Wer `deb`/`rpm` installiert hat, bekommt weiter nur einen Hinweis. Das ist
  Absicht: root-Rechte gehören nicht in einen Client.

## Verworfene Wege

* **Signaturschlüssel und eigener Spiegel** (der Entwurf vor 0004): eigener
  Schlüsselbetrieb, eigene Ausfälle, mehr Angriffsfläche — und kein Gewinn
  gegenüber einer Prüfsumme, die im selben Release liegt.
* **Client fragt GitHub selbst**: widerspricht „der Client spricht nur mit dem
  Server“ und verrät jedem Client-Nutzer die Release-Abfrage.
* **Fremdes Werkzeug zum Entpacken** (`unzip`, `tar`, PowerShell): auf dem Gerät
  des Benutzers nicht garantiert vorhanden und selbst wieder zu prüfen.
* **Ein Helferprozess, der den Tausch überwacht**: mehr bewegliche Teile als
  Nutzen. Die Marke im Profil plus Sicherung deckt den einfachen Fall ab; was
  darüber hinausgeht, steht in `docs/offen.md`.
