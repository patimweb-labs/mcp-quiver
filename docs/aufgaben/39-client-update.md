# 39 – Update des Clients anbieten

**Stand:** 2026-11-25 · **Erledigt** (nach Aufgabe 37) · Entwurf:
[ADR 0005](../adr/0005-client-update.md)

## Vorgabe

- Der **Server** zeigt an, wenn es eine neuere Version gibt.
- Der **Client** prüft, ob es eine neuere Version gibt, und **bietet das Update an**. Der Benutzer entscheidet.
- **Konfiguration und Anmeldungen bleiben vollständig erhalten** (Angaben, Zugänge, Geheimnisse, Geräte-Anmeldung, Anmeldung der Tools).
- **Kein Signaturschlüssel, kein fremdes oder proprietäres Werkzeug.**

## Schon da (Branch feat/versionen-intune)

Versionsprüfung im Server, Hinweis im Client und in der Verwaltung, Vorgaben der Firma, stilles MSI und `.pkg`.
Davon bleibt alles. ADR 0004 („anzeigen statt selbst installieren“) beschreibt die Vorgabe falsch und ist zu ersetzen.

## Vorschlag zur Umsetzung (bei Beginn prüfen)

- Der Server holt das Paket des Releases, prüft es gegen `SHA256SUMS` und gibt es mit Geräte-Token an den Client. Der Client prüft
  Größe und SHA-256 vor dem Tausch. Nur neuere Versionen, nie zurück.
- Tausch je Paketart: Windows-ZIP und MSI (je Benutzer), macOS-App, AppImage; deb/rpm brauchen Rechte, dort Paket an das System geben.
- Laufende Tools sauber beenden, Client tauschen, neu starten. Startet die neue Fassung nicht, kommt die alte zurück.
- Das Profil wird nicht angefasst; vor einer Umstellung des Formats eine Kopie. Test: Angaben, Geheimnisse und Anmeldung vor und nach dem Update gleich.
- Firmen mit Geräteverwaltung können das Angebot per Vorgabe abschalten, dann nur Hinweis.

## Ergebnis

- **Server:** `GET /api/v1/geraet/updates` nennt zu System und Architektur die
  Pakete des Releases (Name, Größe, SHA-256 aus `SHA256SUMS`);
  `GET /api/v1/geraet/updates/paket/{name}` liefert eines davon gestreamt, mit
  `Content-Length`, `ETag` und Drossel. Der Server holt es über `https` von der
  Veröffentlichung, prüft Größe und Prüfsumme und legt es unter
  `<daten>/updates/<version>/` ab. Eine falsche Prüfsumme wird nie
  ausgeliefert.
- **Client:** Hinweis mit **Jetzt aktualisieren** und **Später**. Er prüft die
  Antwort (Größe, SHA-256), beendet die laufenden Werkzeuge, entpackt (ZIP und
  `tar.gz` mit `flate2`, `.dmg` mit `hdiutil`/`ditto`), startet die neue Fassung
  als `quiver --selbsttest` und tauscht erst dann — je Art der Installation,
  die er selbst erkennt. Die alte Fassung bleibt als `<name>.alt`; eine Marke
  im Profil holt sie zurück. Das Profil wird nicht angefasst (Test: vorher und
  nachher gleich).
- **Vorgabe:** `UpdateAngebot` (an/aus, Standard an) in Registry, verwalteten
  Einstellungen und `/etc/mcp-quiver/vorgaben.json`; Vorlagen mitgeliefert.
- **Tests:** Server (Attrappe der Veröffentlichung: holen, prüfen, falsche
  Prüfsumme, fremdes System, Ablage), Client (Angebot, Vorgabe aus, kein
  Rückschritt, „Später“, Tausch mit Attrappe), Tausch im Testordner (ZIP,
  `tar.gz`, AppImage, `.app` auf macOS), Oberfläche (Knöpfe, Fortschritt,
  Fehler) und Bilder in `docs/bilder/fenster-*-update-*.png`.
- **Offen:** siehe `docs/offen.md`, Abschnitt „Update durch den Client“.
