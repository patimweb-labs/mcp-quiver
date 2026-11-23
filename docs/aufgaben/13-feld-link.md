# 13 – Hilfe-Link neben Feldern („Token in Outline erstellen“)

Datenmodell steht: `Feld.link: Option<FeldLink { text, url }>` und `link_aufloesen(&FeldLink, &Werte)`
in `crates/domain` (lesen – die Funktion prüft auch, dass nur `http(s)://` herauskommt).

**Beispiel:** Outline hat die Felder `url` und `api_key`; `api_key` trägt
`link = { text: "Token in Outline erstellen", url: "${url}/settings/api-and-access" }`.
Der User trägt die Basis-URL ein, klickt den Knopf, erzeugt im Browser seinen Token und fügt ihn ein.

## Fenster (`crates/fenster`)

1. Im Dialog „Angaben für …“: Hat ein Feld einen `link`, steht neben bzw. unter dem Eingabefeld ein Knopf
   mit dem `text` und Icon `external`. Er ist nur aktiv, wenn `link_aufloesen` mit den **aktuellen Eingaben
   des Dialogs** (nicht nur den gespeicherten Werten) plus Server-Werten eine Adresse liefert; sonst gesperrt
   mit Tooltip „Erst <Feld> eintragen“ (welche Felder fehlen, aus dem Platzhalter ableiten).
2. Klick → Aktion `LinkOeffnen(url)` → der Kern bzw. das Fenster öffnet den Standardbrowser
   (`open`-Crate, oder was eframe dafür bietet – keine weitere Abhängigkeit, wenn eframe es kann:
   `ctx.open_url`). Sonst nichts – keine Anmeldung, kein Lauscher.
3. Hand-Cursor, Texte de/en, Oberflächentest (Knopf gesperrt ohne URL, aktiv mit URL, Aktion mit
   richtiger Adresse), Bild `fenster-{dark,light}-angaben.png` mit Link-Knopf.

## Admin (`admin/`)

`typen.ts` um `link`; im Felder-Editor eines Eintrags je Feld zwei optionale Eingaben „Link-Text“ und
„Link-Adresse (Platzhalter ${feld} erlaubt)“. Logik-Test für Säubern (beide leer ⇒ `null`).

## Abnahme

Prüfbefehl grün, `admin/` typecheck, test, build grün.
