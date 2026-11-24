// Drei Themes und nichts sonst (AGENTS.md §2.7). Die Namen sind die Werte am
// `<html data-theme="…">`; die Token-Datei entscheidet, welche Farben das sind.

import type { Thema } from "../api/typen";

export const THEMEN: Thema[] = ["dark", "light", "mono"];

export const THEMA_TEXT: Record<Thema, string> = {
  dark: "Dunkel",
  light: "Hell",
  mono: "Mono",
};

export const THEMA_SPEICHER = "quiver-thema";

export function ist_thema(wert: string): wert is Thema {
  return (THEMEN as string[]).includes(wert);
}

/** Unbekanntes (oder Fehlendes) wird `dark` – wie `Thema::aus_name`. */
export function thema_aus_wert(wert: string | null | undefined): Thema {
  return wert && ist_thema(wert) ? wert : "dark";
}
