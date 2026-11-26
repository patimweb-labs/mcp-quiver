# 39 – Update des Clients anbieten

**Stand:** 2026-11-25 · **Zurückgestellt**, kommt nach Aufgabe 37/38 (Tools vom Server).

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
