// Wo eine Auswahlliste liegt und wie sie sich mit der Tastatur bedienen lässt
// (Aufgabe 33). Reine Rechnung: kein DOM, keine Vue-Abhängigkeit, damit die
// Regeln prüfbar sind (AGENTS.md §2.4).
//
// Die Liste liegt **über** allem anderen (eigene Ebene am `body`), unter dem
// Feld, wenn dort Platz ist, sonst darüber. Sie wird nie höher als der freie
// Platz, damit der Dialog nicht wächst und nicht scrollt.

import type { ListOption } from "@weppelmann/components";

/** Abstand zum Fensterrand in Pixeln. */
export const RAND = 8;
/** Luft zwischen Feld und Liste. */
export const ABSTAND = 2;
/** Höchsthöhe einer Liste; mehr Einträge scrollen. */
export const HOEHE_MAX = 280;
/** Unter dieser Höhe ist „unten“ kein Platz mehr, dann geht die Liste nach oben. */
export const HOEHE_MIN = 96;

/** Das Feld, an dem die Liste hängt (Viewport-Koordinaten). */
export interface Anker {
  oben: number;
  unten: number;
  links: number;
  breite: number;
}

/** Das Fenster (Viewport), in dem die Liste Platz finden muss. */
export interface Fenster {
  breite: number;
  hoehe: number;
}

/** Wo die Liste steht: Position, Breite und Höchsthöhe in Pixeln. */
export interface Listenplatz {
  oben: number;
  links: number;
  breite: number;
  hoehe: number;
  /** `true`, wenn die Liste über dem Feld aufgeklappt ist. */
  ueber: boolean;
}

/**
 * Die Liste so breit wie das Feld, höchstens so hoch wie der freie Platz,
 * und vollständig im Fenster.
 */
export function listenplatz(
  anker: Anker,
  fenster: Fenster,
  wunsch: number = HOEHE_MAX,
): Listenplatz {
  const platz_unten = fenster.hoehe - anker.unten - ABSTAND - RAND;
  const platz_oben = anker.oben - ABSTAND - RAND;
  // Nach unten, solange dort wenigstens ein paar Einträge Platz haben; sonst
  // nach oben, wenn dort mehr Platz ist (das Feld steht am unteren Rand).
  const ueber = platz_unten < Math.min(wunsch, HOEHE_MIN) && platz_oben > platz_unten;
  const verfuegbar = Math.max(0, ueber ? platz_oben : platz_unten);
  const hoehe = Math.max(0, Math.min(wunsch, verfuegbar));
  const breite = Math.max(0, Math.min(anker.breite, fenster.breite - 2 * RAND));
  const links = Math.min(
    Math.max(RAND, anker.links),
    Math.max(RAND, fenster.breite - RAND - breite),
  );
  const oben = ueber ? anker.oben - ABSTAND - hoehe : anker.unten + ABSTAND;
  return { oben, links, breite, hoehe, ueber };
}

/** Ist das Feld (noch) zu sehen? Sonst schließt sich die Liste. */
export function anker_sichtbar(anker: Anker, fenster: Fenster): boolean {
  return anker.unten > 0 && anker.oben < fenster.hoehe && anker.breite > 0;
}

// --------------------------------------------------------------- Tastatur

/** Erster auswählbarer Eintrag; `-1`, wenn keiner wählbar ist. */
export function erste_aktive(optionen: ListOption[]): number {
  return optionen.findIndex((option) => !option.disabled);
}

/** Letzter auswählbarer Eintrag; `-1`, wenn keiner wählbar ist. */
export function letzte_aktive(optionen: ListOption[]): number {
  for (let i = optionen.length - 1; i >= 0; i -= 1) {
    if (!optionen[i].disabled) return i;
  }
  return -1;
}

/**
 * Nächster auswählbarer Eintrag in eine Richtung; die Liste läuft um. Ohne
 * wählbaren Eintrag bleibt der Stand.
 */
export function naechste_aktive(
  optionen: ListOption[],
  ab: number,
  richtung: 1 | -1,
): number {
  const anzahl = optionen.length;
  if (anzahl === 0) return -1;
  for (let schritt = 1; schritt <= anzahl; schritt += 1) {
    const i = (ab + richtung * schritt + anzahl * anzahl) % anzahl;
    if (!optionen[i].disabled) return i;
  }
  return ab;
}

/** Erster Eintrag, der mit dem getippten Zeichen beginnt (Tippsuche). */
export function tippsuche(
  optionen: ListOption[],
  zeichen: string,
  ab: number,
): number {
  const gesucht = zeichen.toLowerCase();
  for (let schritt = 1; schritt <= optionen.length; schritt += 1) {
    const i = (ab + schritt) % optionen.length;
    if (!optionen[i].disabled && optionen[i].label.toLowerCase().startsWith(gesucht)) {
      return i;
    }
  }
  return -1;
}

/**
 * Einträge für eine Liste mit Suche: gesucht wird in Beschriftung und Hinweis.
 * Leere Suche zeigt alles.
 */
export function filtern(optionen: ListOption[], suche: string): ListOption[] {
  const gesucht = suche.trim().toLowerCase();
  if (gesucht.length === 0) return optionen;
  return optionen.filter((option) =>
    `${option.label} ${option.hint ?? ""}`.toLowerCase().includes(gesucht),
  );
}
