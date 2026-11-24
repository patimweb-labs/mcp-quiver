// Die Prüfspur lesen: Art benennen, filtern, nachladen (Aufgabe 21).
//
// Die Prüfspur ist nur lesend. Diese Funktionen machen aus den Zeilen des
// Servers eine Ansicht; sie ändern nichts und schreiben nichts.

import type { PruefspurEintrag } from "../api/typen";
import type { Ton } from "./ton";

export interface ArtEintrag {
  wert: string;
  titel: string;
  ton: Ton;
}

/**
 * Die Arten, die der Server schreibt. Eine unbekannte Art bleibt sichtbar –
 * sie bekommt ihren rohen Wert als Titel, statt zu verschwinden.
 */
export const ARTEN: readonly ArtEintrag[] = [
  { wert: "anmeldung", titel: "Anmeldung", ton: "gut" },
  { wert: "fehlschlag", titel: "Fehlschlag", ton: "warnung" },
  { wert: "rolle", titel: "Rolle", ton: "akzent" },
  { wert: "aenderung", titel: "Änderung", ton: "normal" },
  { wert: "geraet", titel: "Gerät", ton: "leise" },
  { wert: "nachpruefen", titel: "Nachprüfen", ton: "leise" },
  { wert: "einrichtung", titel: "Einrichtung", ton: "akzent" },
  { wert: "kommandozeile", titel: "Kommandozeile", ton: "blass" },
];

export function art_eintrag(wert: string): ArtEintrag {
  return (
    ARTEN.find((eintrag) => eintrag.wert === wert) ?? {
      wert,
      titel: wert,
      ton: "blass",
    }
  );
}

export function art_titel(wert: string): string {
  return art_eintrag(wert).titel;
}

export function art_ton(wert: string): Ton {
  return art_eintrag(wert).ton;
}

/** Erste Ladung und jeder Schritt beim Nachladen. */
export const PRUEFSPUR_ERSTE = 20;
export const PRUEFSPUR_SCHRITT = 20;

export function naechstes_limit(limit: number): number {
  return limit + PRUEFSPUR_SCHRITT;
}

/**
 * Der Server bekommt `limit + 1` angefragt: Kommt eine Zeile mehr zurück, gibt
 * es weitere Einträge. So lügt der Knopf „Nachladen“ nicht.
 */
export function anfrage_limit(limit: number): number {
  return limit + 1;
}

export function hat_mehr(geladen: number, limit: number): boolean {
  return geladen > limit;
}

/** Die ersten `limit` Zeilen; die Zusatzzeile ist nur die Probe. */
export function sichtbar(
  geladen: PruefspurEintrag[],
  limit: number,
): PruefspurEintrag[] {
  return geladen.slice(0, limit);
}

// ------------------------------------------------------------------- Filter

export type Artfilter = string | "alle";

export interface Pruefspurfilter {
  suche: string;
  art: Artfilter;
}

export const OHNE_FILTER: Pruefspurfilter = { suche: "", art: "alle" };

/** Die Arten, die in den geladenen Zeilen wirklich vorkommen (plus „alle“). */
export function arten_im_bestand(liste: PruefspurEintrag[]): ArtEintrag[] {
  const vorhanden = new Set(liste.map((eintrag) => eintrag.art));
  return ARTEN.filter((eintrag) => vorhanden.has(eintrag.wert));
}

/** Sucht in Akteur, Text und Art; Groß- und Kleinschreibung egal. */
export function passt(
  eintrag: PruefspurEintrag,
  filter: Pruefspurfilter,
): boolean {
  if (filter.art !== "alle" && eintrag.art !== filter.art) return false;
  const suche = filter.suche.trim().toLowerCase();
  if (suche.length === 0) return true;
  const heu = [
    eintrag.akteur,
    eintrag.text,
    eintrag.art,
    art_titel(eintrag.art),
  ]
    .join(" ")
    .toLowerCase();
  return heu.includes(suche);
}

export function filtern(
  liste: PruefspurEintrag[],
  filter: Pruefspurfilter,
): PruefspurEintrag[] {
  return liste.filter((eintrag) => passt(eintrag, filter));
}

/** „1 Eintrag“ / „12 Einträge“ / „keine Einträge“. */
export function eintraege_text(anzahl: number): string {
  if (anzahl === 0) return "keine Einträge";
  return `${anzahl} ${anzahl === 1 ? "Eintrag" : "Einträge"}`;
}
