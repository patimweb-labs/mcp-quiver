// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

// Die Breitenstufen der Oberfläche. Dieselben Grenzen trägt das Client-Fenster
// (`bausteine::schmal`, `bausteine::eng`): sie stehen als Tokens in
// `tokens/primitives.tokens.json` und kommen über `tokens.css` hierher —
// eine Wahrheit für beide Oberflächen (AGENTS.md §2.11).

import { ref } from "vue";

/** `eng` unter `breakpoint.formular`, `schmal` unter `breakpoint.schmal`. */
export type Breite = "eng" | "schmal" | "weit";

export interface Grenzen {
  formular: number;
  schmal: number;
}

/**
 * Die Stufe zu einer Fensterbreite.
 *
 * Die Grenzen sind Weiten in Pixeln; sie kommen aus den Tokens. Fehlt ein
 * Token, gilt `0` — dann bleibt die Oberfläche weit, statt mit einer
 * geratenen Zahl zu brechen.
 */
export function breite_stufe(px: number, grenzen: Grenzen): Breite {
  if (px < grenzen.formular) return "eng";
  if (px < grenzen.schmal) return "schmal";
  return "weit";
}

/** Die Grenzen aus den CSS-Variablen lesen (`--wp-breakpoint-*`). */
export function grenzen_lesen(wert: (name: string) => string): Grenzen {
  const zahl = (name: string): number => {
    const roh = Number.parseFloat(wert(name));
    return Number.isFinite(roh) ? roh : 0;
  };
  return {
    formular: zahl("--wp-breakpoint-formular"),
    schmal: zahl("--wp-breakpoint-schmal"),
  };
}

/** Die aktuelle Stufe — Kopf und Seitenleiste lesen sie. */
export const breite = ref<Breite>("weit");

/**
 * Die Stufe messen und am Wurzelelement vermerken (`data-breite`). Die
 * Ansichten entscheiden danach, was sie zeigen; kein `v-if` in einer
 * Geschäftslogik (AGENTS.md §2.3).
 */
export function messe_breite(): void {
  if (typeof window === "undefined" || typeof document === "undefined") return;
  const grenzen = grenzen_lesen((name) =>
    getComputedStyle(document.documentElement).getPropertyValue(name),
  );
  breite.value = breite_stufe(window.innerWidth, grenzen);
  document.documentElement.dataset.breite = breite.value;
}
