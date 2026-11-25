# 36 – Angaben vom Server vorbelegen

Admins können Angaben eines Tools vorgeben, damit Benutzer nur noch Benutzername und Passwort eintragen
(z. B. die MCP-Adresse von ZEP, die Adresse des Wikis).

- Im Satz (wie heute bei Server-Feldern) und als Standard am Eintrag sind **alle Felder außer Geheimnissen**
  vorbelegbar, auch Felder mit Quelle „Gerät“. Geheimnisse (Passwort, Token) nie, weder in der Oberfläche noch über die
  Schnittstelle (Server lehnt ab, Test).
- Je Wert: **Vorschlag** (Benutzer darf ändern) oder **fest** (im Dialog sichtbar, nicht änderbar, „von deiner Firma vorgegeben“).
- Reihenfolge: fest vom Server → Gerät → Vorschlag vom Server → Standard des Feldes. Bei mehreren Sätzen gewinnt die
  höhere Priorität wie bisher.
- Ist eine Pflichtangabe vorbelegt, gilt sie als gesetzt; der Dialog zeigt sie ausgefüllt. Ändert der Admin eine feste Vorgabe,
  wirkt sie beim nächsten Abgleich, ohne Neuanmeldung.
- Schalter „Schreiben erlauben“ und „Unverschlüsselt erlauben“: vorbelegen darf nur „aus“ fest sein (der Admin kann
  verbieten, nicht heimlich freigeben). Freigeben bleibt Sache des Benutzers.
- Zugangs-Listen (Organisationen, Hosts) sind hier noch nicht dabei; nur Felder.
- Beispieltexte unter Feldern stammen aus dem Tool, nie aus einer Firma.
- Altdaten und ältere Clients vertragen sich (`serde(default)`), Schema mit Migration falls nötig.

Tests (Domäne, Server, Client, Verwaltung), Bilder, SPEC. fmt, clippy, `cargo test --workspace`, `npm test/typecheck/build`.
Nicht committen, nicht pushen.
