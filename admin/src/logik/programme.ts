// Die Programme, die der Server an die Geräte verteilt (Aufgabe 37) – als
// Zeilen für die Ansicht. Reine Funktionen, damit sie ohne Server testbar
// sind.

import type { GeraetStand, Programm } from "../api/typen";

/** Eine Zeile der Programmliste: ein Name, seine Fassungen und Systeme. */
export interface ProgrammZeile {
  name: string;
  /** Die Fassungen, die es gibt („1.2.3“, „unbekannt“). */
  fassungen: string[];
  /** Die Systeme, für die das Programm bereitliegt, z. B. `linux/x64`. */
  systeme: string[];
  /** Die größte Fassung in MiB, für die Anzeige. */
  groesse: string;
}

/** Wie groß ist das Programm? (MiB mit einer Nachkommastelle) */
export function groesse_text(bytes: number): string {
  const mib = bytes / (1024 * 1024);
  if (mib < 0.1) return `${Math.max(1, Math.round(bytes / 1024))} KiB`;
  return `${mib.toFixed(1)} MiB`;
}

/**
 * Die Programme zu Zeilen zusammenfassen: je Name eine Zeile mit allen
 * Fassungen und Systemen. Sortiert nach Namen.
 */
export function programm_zeilen(programme: Programm[]): ProgrammZeile[] {
  const nach = new Map<string, ProgrammZeile & { groesste: number }>();
  for (const programm of programme) {
    const zeile = nach.get(programm.name) ?? {
      name: programm.name,
      fassungen: [],
      systeme: [],
      groesse: "",
      groesste: 0,
    };
    if (!zeile.fassungen.includes(programm.version)) {
      zeile.fassungen.push(programm.version);
    }
    const system = `${programm.system}/${programm.arch}`;
    if (!zeile.systeme.includes(system)) zeile.systeme.push(system);
    if (programm.groesse > zeile.groesste) {
      zeile.groesste = programm.groesse;
      zeile.groesse = groesse_text(programm.groesse);
    }
    nach.set(programm.name, zeile);
  }
  return [...nach.values()]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map(({ groesste: _groesste, ...zeile }) => ({
      ...zeile,
      fassungen: zeile.fassungen.sort(),
      systeme: zeile.systeme.sort(),
    }));
}

/**
 * Welche Programme auf einem Gerät laufen – „quiver-devops 1.2.3“.
 *
 * Die Liste kommt aus der letzten Meldung des Geräts; ältere Clients melden
 * keine, dann bleibt sie leer.
 */
export function laufende_programme(stand: GeraetStand): string[] {
  const programme = stand.meldung?.programme ?? [];
  return programme
    .map((programm) => `${programm.name} ${programm.version}`)
    .sort();
}
