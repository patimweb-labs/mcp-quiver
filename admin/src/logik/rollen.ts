// Rollen als Daten (Aufgabe 24). Die Ansichten zählen keine Rollen auf und
// prüfen keine Namen: Sie lesen diese Tabelle. Kommt eine Rolle dazu, wird
// hier ein Eintrag ergänzt – Filter, Anzeige und Rang folgen von selbst.
//
// Der Server bleibt die Wahrheit über Rechte (`Rolle::darf_verwalten` in
// `crates/protocol`). Hier steht nur, wie die Verwaltung sie zeigt.

import type { Rolle } from "../api/typen";
import type { Ton } from "./ton";

export interface RollenEintrag {
  wert: Rolle;
  /** Was im Kopf und in der Tabelle steht. */
  titel: string;
  /** Kurze Erklärung für Filter und Rückfragen. */
  unter: string;
  /** Höher gilt: Bei mehreren Wegen gewinnt die höchste Rolle. */
  rang: number;
  /** Notzugang: nicht änderbar, nicht löschbar (ADR 0002). */
  fest: boolean;
  /** Bedeutung der Farbe in der Tabelle. */
  ton: Ton;
  /** Rolle, die der Knopf setzt; `null` oder weggelassen heißt: kein Wechsel. */
  wechsel?: Rolle | null;
}

export const ROLLEN: readonly RollenEintrag[] = [
  {
    wert: "root",
    titel: "Root",
    unter: "Notzugang ohne Entra; nicht änderbar",
    rang: 2,
    fest: true,
    ton: "akzent",
    wechsel: null,
  },
  {
    wert: "admin",
    titel: "Admin",
    unter: "darf Bibliothek, Gruppen, Geräte und Benutzer verwalten",
    rang: 1,
    fest: false,
    ton: "normal",
    wechsel: "benutzer",
  },
  {
    wert: "benutzer",
    titel: "Mitarbeiter",
    unter: "sieht eigene Geräte und zugewiesene Tools",
    rang: 0,
    fest: false,
    ton: "leise",
    wechsel: "admin",
  },
];

export const ROLLE_TEXT = Object.fromEntries(
  ROLLEN.map((eintrag) => [eintrag.wert, eintrag.titel]),
) as Record<Rolle, string>;

export function rolle_eintrag(rolle: Rolle): RollenEintrag {
  return (
    ROLLEN.find((eintrag) => eintrag.wert === rolle) ??
    ROLLEN[ROLLEN.length - 1]
  );
}

export function rollen_text(rolle: Rolle): string {
  return rolle_eintrag(rolle).titel;
}

export function rang(rolle: Rolle): number {
  return rolle_eintrag(rolle).rang;
}

/** Ist die Rolle fest (Root)? Dann gibt es keine Aktionen. */
export function rolle_fest(rolle: Rolle): boolean {
  return rolle_eintrag(rolle).fest;
}

/** Darf diese Rolle die Verwaltung benutzen? (Rust: `Rolle::darf_verwalten`.) */
export function darf_verwalten(rolle: Rolle): boolean {
  return rang(rolle) > 0;
}

/** Die höhere von zwei Rollen (der Server nimmt die höchste). */
export function hoehere_rolle(a: Rolle, b: Rolle): Rolle {
  return rang(a) >= rang(b) ? a : b;
}
