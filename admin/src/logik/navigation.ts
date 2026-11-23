// Die Seiten der Verwaltung und ihr Ort in der Adresse. Die Adresse ist der
// einzige Zustand, der einen Neuladen überlebt (datenorientiert, §2.1).

import type { Seite } from "../api/typen";

export const STANDARD_SEITE: Seite = "ueberblick";

export interface Seiteneintrag {
  id: Seite;
  titel: string;
  /** Für die Navigation: kurze Erklärung, was hier liegt. */
  unter: string;
}

export const SEITEN: Seiteneintrag[] = [
  { id: "ueberblick", titel: "Überblick", unter: "Zähler und Probleme" },
  { id: "bibliothek", titel: "Bibliothek", unter: "Einträge und Vorlagen" },
  { id: "saetze", titel: "Sätze", unter: "Sammlungen mit Werten" },
  { id: "gruppen", titel: "Gruppen", unter: "Sätze für Gerätegruppen" },
  { id: "geraete", titel: "Geräte", unter: "Stand, Gruppen, Zugang" },
];

export function ist_seite(wert: string): wert is Seite {
  return SEITEN.some((eintrag) => eintrag.id === wert);
}

/** `#/geraete` → `geraete`; Unbekanntes wird der Überblick. */
export function seite_aus_hash(hash: string): Seite {
  const wert = hash.replace(/^#\/?/, "").trim();
  return ist_seite(wert) ? wert : STANDARD_SEITE;
}

export function hash_aus_seite(seite: Seite): string {
  return `#/${seite}`;
}

/** Der Titel einer Seite, für Kopf und Dokumenttitel. */
export function seite_titel(seite: Seite): string {
  return SEITEN.find((eintrag) => eintrag.id === seite)?.titel ?? "Überblick";
}
