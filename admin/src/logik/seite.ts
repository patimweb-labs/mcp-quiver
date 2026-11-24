// Der Zustand einer Seite als Summentyp (AGENTS.md §2.2): laden, fertig,
// Fehler schließen einander aus. Kein Fächer aus `laedt`/`fehler`/`daten`.
//
// Die Ansichten entscheiden nichts – sie zeigen, was hier steht.

export type Zustand<T> =
  | { art: "laedt" }
  | { art: "fertig"; daten: T }
  | { art: "fehler"; text: string };

export function laedt<T>(): Zustand<T> {
  return { art: "laedt" };
}

export function fertig<T>(daten: T): Zustand<T> {
  return { art: "fertig", daten };
}

export function fehler<T>(text: string): Zustand<T> {
  return { art: "fehler", text };
}

/** Die Daten, wenn es welche gibt – sonst `null`. */
export function daten_von<T>(zustand: Zustand<T>): T | null {
  return zustand.art === "fertig" ? zustand.daten : null;
}

/** Ist der Zustand fertig? Verengt den Typ für die Ansicht. */
export function ist_fertig<T>(
  zustand: Zustand<T>,
): zustand is { art: "fertig"; daten: T } {
  return zustand.art === "fertig";
}
