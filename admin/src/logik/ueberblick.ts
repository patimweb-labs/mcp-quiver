// Die Zähler der Überblicksseite als Daten. Die Ansicht zeichnet nur.

import type { Seite, Ueberblick } from "../api/typen";

export interface Kennzahl {
  schluessel: string;
  titel: string;
  wert: string;
  /** Wohin der Zähler führt – ein Zähler ohne Ziel ist eine Sackgasse. */
  seite: Seite;
}

export function kennzahlen(ueberblick: Ueberblick): Kennzahl[] {
  return [
    {
      schluessel: "eintraege",
      titel: "Einträge",
      wert: String(ueberblick.eintraege),
      seite: "bibliothek",
    },
    {
      schluessel: "saetze",
      titel: "Sätze",
      wert: String(ueberblick.saetze),
      seite: "saetze",
    },
    {
      schluessel: "gruppen",
      titel: "Gruppen",
      wert: String(ueberblick.gruppen),
      seite: "gruppen",
    },
    {
      schluessel: "geraete",
      titel: "Geräte",
      wert: String(ueberblick.geraete),
      seite: "geraete",
    },
    {
      schluessel: "verbunden",
      titel: "Verbunden",
      wert: `${ueberblick.geraete_verbunden} / ${ueberblick.geraete}`,
      seite: "geraete",
    },
    {
      schluessel: "server",
      titel: "Server-Version",
      wert: ueberblick.server_version,
      seite: "ueberblick",
    },
  ];
}
