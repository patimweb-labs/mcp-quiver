// Feldarten (Aufgabe 16): Beschriftung, Darstellung, Werte und der Grund,
// warum „Speichern“ gesperrt ist. Reine Funktionen – die Ansichten rufen nur
// hier auf (AGENTS.md §2.4).

import type { Feld, FeldArt, FeldOption, Werte } from "../api/typen";

/** Die wählbaren Arten im Feldeditor, in der Reihenfolge der Anzeige. */
export const FELD_ARTEN: { value: string; label: string }[] = [
  { value: "text", label: "Text" },
  { value: "passwort", label: "Passwort" },
  { value: "zahl", label: "Zahl" },
  { value: "schalter", label: "Schalter" },
  { value: "url", label: "Adresse" },
  { value: "auswahl", label: "Auswahl" },
];

/** Die Beschriftung eines Felds: `titel`, sonst der Name. */
export function feld_titel(feld: Feld): string {
  const titel = feld.titel?.trim() ?? "";
  return titel.length > 0 ? titel : feld.name;
}

/**
 * Die dargestellte Art. Ein Geheimnis bleibt ein Passwort, egal welche Art
 * gespeichert ist – wie in Rust (`logik::feld_art`).
 */
export function feld_art(feld: Feld): FeldArt {
  if (feld.quelle === "geheim") return "passwort";
  return feld.art ?? "text";
}

/** Der Name der Art, wie er in der Auswahl steht. */
export function feld_art_name(art: FeldArt): string {
  return typeof art === "string" ? art : "auswahl";
}

/** Die Art in Worten, für die Feldkarte. */
export function feld_art_text(art: FeldArt): string {
  const name = feld_art_name(art);
  return FELD_ARTEN.find((eintrag) => eintrag.value === name)?.label ?? name;
}

/**
 * Die Art aus der Auswahl setzen. Beim Wechsel auf „auswahl“ bleiben schon
 * eingetragene Optionen erhalten; sonst beginnt die Liste leer.
 */
export function feld_art_aus_name(name: string, alt: FeldArt): FeldArt {
  if (name === "auswahl") {
    if (typeof alt !== "string" && alt.auswahl) return alt;
    return { auswahl: { optionen: [] } };
  }
  switch (name) {
    case "passwort":
    case "zahl":
    case "schalter":
    case "url":
      return name;
    default:
      return "text";
  }
}

/** Die Optionen einer Auswahl; andere Arten haben keine. */
export function feld_optionen(feld: Feld): FeldOption[] {
  const art = feld_art(feld);
  return typeof art === "string" ? [] : art.auswahl.optionen;
}

/** Ist das Feld ein Schalter? */
export function ist_schalter(feld: Feld): boolean {
  return feld_art(feld) === "schalter";
}

/** Ist ein Schalterwert „an“? `an`/`aus` sind die gespeicherten Werte. */
export function schalter_an(wert: string): boolean {
  return ["an", "true", "1", "ja", "yes"].includes(wert.trim().toLowerCase());
}

/** Der Wert eines Schalters: gespeichert, sonst Standard, sonst `aus`. */
export function schalter_wert(gespeichert: string, standard: string | null): string {
  if (gespeichert.trim().length > 0) return schalter_an(gespeichert) ? "an" : "aus";
  return standard && schalter_an(standard) ? "an" : "aus";
}

/** Der Wert eines Felds, wie ihn das Formular zeigt. */
export function feld_wert(feld: Feld, werte: Werte): string {
  const gespeichert = werte[feld.name] ?? "";
  if (ist_schalter(feld)) return schalter_wert(gespeichert, feld.standard);
  return gespeichert;
}

/** Was an einem eingegebenen Wert nicht stimmt. */
export type WertFehler = "zahl" | "adresse" | null;

/** Prüft einen nicht-leeren Wert gegen seine Art. */
export function wert_fehler(art: FeldArt, wert: string): WertFehler {
  if (art === "zahl" && !/^\d+$/.test(wert)) return "zahl";
  if (art === "url" && !/^https?:\/\//.test(wert)) return "adresse";
  return null;
}

/**
 * Warum „Speichern“ gesperrt ist: fehlende Pflichtfelder und ungültige Werte.
 * `null` heißt: alles in Ordnung. Leere, nicht verpflichtende Felder sind
 * immer erlaubt; Geheimnisse werden hier nie geprüft (der Admin kennt sie
 * nicht).
 *
 * `pruefe_pflicht` lässt sich abschalten, wo leere Pflichtfelder erlaubt sind;
 * die Dialoge mit Werten nutzen die Vorgabe `true`.
 */
export function feld_sperrgrund(
  felder: Feld[],
  werte: Werte,
  pruefe_pflicht = true,
): string | null {
  const fehlend: string[] = [];
  const zahlen: string[] = [];
  const adressen: string[] = [];
  for (const feld of felder) {
    if (feld.quelle === "geheim") continue;
    const art = feld_art(feld);
    const wert = feld_wert(feld, werte).trim();
    if (wert.length === 0) {
      if (pruefe_pflicht && feld.pflicht) fehlend.push(feld_titel(feld));
      continue;
    }
    const fehler = wert_fehler(art, wert);
    if (fehler === "zahl") zahlen.push(feld_titel(feld));
    else if (fehler === "adresse") adressen.push(feld_titel(feld));
  }
  const teile: string[] = [];
  if (fehlend.length > 0) teile.push(`Es fehlt: ${fehlend.join(", ")}`);
  if (zahlen.length > 0) teile.push(`Ungültige Zahl: ${zahlen.join(", ")}`);
  if (adressen.length > 0) teile.push(`Ungültige Adresse: ${adressen.join(", ")}`);
  return teile.length > 0 ? teile.join(" · ") : null;
}

/** Optionen als Text: `wert | Anzeige`, eine je Zeile. */
export function optionen_als_text(optionen: FeldOption[]): string {
  return optionen
    .map((option) =>
      option.wert === option.anzeige
        ? option.wert
        : `${option.wert} | ${option.anzeige}`,
    )
    .join("\n");
}

/** Umkehrung: ohne `|` ist der Wert zugleich die Anzeige. */
export function optionen_aus_text(text: string): FeldOption[] {
  const optionen: FeldOption[] = [];
  for (const roh of text.split("\n")) {
    const zeile = roh.trim();
    if (zeile.length === 0) continue;
    const trenner = zeile.indexOf("|");
    const wert = (trenner < 0 ? zeile : zeile.slice(0, trenner)).trim();
    const anzeige = (trenner < 0 ? wert : zeile.slice(trenner + 1)).trim();
    if (wert.length === 0) continue;
    optionen.push({ wert, anzeige: anzeige.length > 0 ? anzeige : wert });
  }
  return optionen;
}
