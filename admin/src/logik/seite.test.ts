import { describe, expect, it } from "vitest";

import {
  daten_von,
  fehler,
  fertig,
  ist_fertig,
  laedt,
  type Zustand,
} from "./seite";

describe("Seitenzustand", () => {
  it("kennt genau einen Zustand zur Zeit", () => {
    const zustand: Zustand<number[]> = laedt();
    expect(zustand).toEqual({ art: "laedt" });
    expect(daten_von(zustand)).toBeNull();
    expect(ist_fertig(zustand)).toBe(false);
  });

  it("trägt die Daten im fertigen Zustand", () => {
    const zustand = fertig([1, 2]);
    expect(ist_fertig(zustand)).toBe(true);
    expect(daten_von(zustand)).toEqual([1, 2]);
  });

  it("trägt den Text im Fehlerzustand", () => {
    const zustand = fehler<number[]>("kaputt");
    expect(zustand.art).toBe("fehler");
    if (zustand.art === "fehler") expect(zustand.text).toBe("kaputt");
    expect(daten_von(zustand)).toBeNull();
  });
});
