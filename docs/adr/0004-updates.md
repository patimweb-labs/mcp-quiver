# ADR 0004 — Updates: anzeigen statt selbst installieren

**Stand:** 2026-12-02 · **Aufgabe:** 34

## Kontext

Ein Client auf einem Gerät soll erfahren, dass eine neuere Fassung
veröffentlicht ist, ohne dass quiver dafür ein eigenes Verteil- und
Signatursystem mitbringt. Ein früherer Entwurf ließ den Client Pakete laden,
Signaturschlüssel prüfen und sich selbst ersetzen (minisign, `updates.json`,
Spiegel, Helferprozess, Rückfall auf die alte Fassung). Das braucht eigenen
Schlüsselbetrieb, eigene Abläufe zum Tauschen eines laufenden Programms und
eigene Ausfälle, die niemand mehr nachvollziehen kann — und es ist genau die
Aufgabe einer Geräteverwaltung, die im Zielumfeld (Microsoft 365, Intune)
ohnehin vorhanden ist.

## Entscheidung

1. **quiver installiert sich nicht selbst.** Es gibt keinen Tausch, keinen
   Helferprozess, keine Signatur der Pakete, keine Schlüssel im Programm. Die
   einzige Quelle für Versionen ist die Veröffentlichung des Repositories; die
   einzige Handlung ist ein Hinweis.
2. **Der Server fragt, sonst niemand.** Der Server fragt die
   GitHub-Veröffentlichung höchstens alle sechs Stunden ab (Streuung 30
   Minuten, Zustand in der Datenbank, damit ein Neustart nicht sofort wieder
   fragt), ohne Kennung und ohne Nutzdaten. `QUIVER_UPDATES=aus` schaltet das
   ab. Der Client fragt **nur** seinen Server, nie das Internet.
3. **Anzeigen heißt anzeigen.** Der Client zeigt einen ruhigen Hinweis mit der
   Version, dem Text der Firma und einem Link auf die Änderungen. Ein
   Rückschritt wird nie angezeigt (kein „neu“ für eine ältere Fassung), ein
   Entwurf oder eine Vorabfassung nie als Angebot. Die Verwaltung zeigt die
   neueste Client-Version, eine neuere Server-Version und die Geräte, deren
   Client älter ist.
4. **Eingespielt wird durch die Geräteverwaltung oder von Hand.** Dafür gibt es
   Pakete, die still installieren, eine alte Fassung ersetzen und das Profil
   stehen lassen: das MSI (je Benutzer) für Intune als „Branchen-App (MSI)“ und
   ein `.pkg` für macOS (Bordmittel `pkgbuild`/`productbuild`). Beide beenden
   einen laufenden Client vor dem Austausch und starten ihn danach wieder.
5. **Die Firma kann drei Dinge vorgeben** (`ServerUrl`, `Autostart`,
   `UpdateHinweis`), nur lesbar, aus dem verwalteten Bereich der Plattform —
   Vorrang vor dem Profil. Ohne Vorgaben gilt alles wie bisher.

## Preis

* Wer keine Geräteverwaltung hat, aktualisiert von Hand aus dem Release. Das
  ist der bewusste Tausch: kein eigener Schlüsselbetrieb, dafür ein
  Installationsschritt.
* Die Abfrage der Veröffentlichung ist eine Netzverbindung des Servers nach
  außen. Sie ist abschaltbar und überträgt nichts außer der Anfrage selbst.
* Der Hinweis erscheint erst, wenn der Client mit dem Server verbunden war —
  ohne Server keine Version.

## Verworfene Wege

* **Selbst-Update mit minisign** (der frühere Entwurf): eigener Schlüssel,
  eigener Spiegel, eigener Tausch eines laufenden Programms. Verworfen, weil es
  die Aufgabe der Geräteverwaltung doppelt und mehr Angriffsfläche schafft.
* **Eigene `updates.json` im Release**: eine zweite Wahrheit neben den
  Release-Daten. Die GitHub-Antwort (`tag_name`, `html_url`) genügt für die
  Anzeige.
* **Client fragt GitHub selbst**: widerspricht „der Client spricht nur mit dem
  Server“ und verrät jedem Client-Nutzer die Release-Abfrage.
