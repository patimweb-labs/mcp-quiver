import { describe, expect, it } from "vitest";

import type { Ueberblick } from "../api/typen";
import { kennzahlen } from "./ueberblick";

describe("kennzahlen", () => {
  it("gibt jedem Zähler Titel, Wert und Ziel", () => {
    const ueberblick: Ueberblick = {
      eintraege: 7,
      saetze: 3,
      gruppen: 2,
      geraete: 5,
      geraete_verbunden: 4,
      server_version: "0.1.0",
    };
    const zahlen = kennzahlen(ueberblick);
    expect(zahlen.map((zahl) => zahl.schluessel)).toEqual([
      "eintraege",
      "saetze",
      "gruppen",
      "geraete",
      "verbunden",
      "server",
    ]);
    expect(zahlen.find((zahl) => zahl.schluessel === "eintraege")).toEqual({
      schluessel: "eintraege",
      titel: "Einträge",
      wert: "7",
      seite: "bibliothek",
    });
    expect(zahlen.find((zahl) => zahl.schluessel === "verbunden")?.wert).toBe(
      "4 / 5",
    );
  });
});
