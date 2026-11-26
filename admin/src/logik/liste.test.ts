import { describe, expect, it } from "vitest";

import type { ListOption } from "@weppelmann/components";

import {
  ABSTAND,
  HOEHE_MAX,
  RAND,
  anker_sichtbar,
  erste_aktive,
  filtern,
  letzte_aktive,
  listenplatz,
  naechste_aktive,
  tippsuche,
} from "./liste";

const fenster = { breite: 1200, hoehe: 800 };

/** Ein Feld an fester Stelle; die Maße bleiben wie im Betrieb. */
function anker(oben: number, hoehe = 32, links = 100, breite = 320) {
  return { oben, unten: oben + hoehe, links, breite };
}

describe("listenplatz", () => {
  it("klappt nach unten auf, wenn dort Platz ist", () => {
    const platz = listenplatz(anker(200), fenster);
    expect(platz.ueber).toBe(false);
    expect(platz.oben).toBe(200 + 32 + ABSTAND);
    expect(platz.links).toBe(100);
    expect(platz.breite).toBe(320);
    expect(platz.hoehe).toBe(HOEHE_MAX);
  });

  it("geht nach oben, wenn unten kein Platz mehr ist", () => {
    const platz = listenplatz(anker(700), fenster);
    expect(platz.ueber).toBe(true);
    // Die Liste endet direkt über dem Feld und bleibt im Fenster.
    expect(platz.oben).toBe(700 - ABSTAND - platz.hoehe);
    expect(platz.oben).toBeGreaterThanOrEqual(RAND);
  });

  it("wird nie höher als der freie Platz", () => {
    const eng = { breite: 1200, hoehe: 400 };
    const platz = listenplatz(anker(100), eng);
    expect(platz.hoehe).toBe(400 - 132 - ABSTAND - RAND);
    expect(platz.hoehe).toBeLessThan(HOEHE_MAX);
    // Unterkante bleibt im Fenster.
    expect(platz.oben + platz.hoehe).toBeLessThanOrEqual(eng.hoehe - RAND);
  });

  it("bleibt beim Aufklappen nach oben unter der Höchsthöhe", () => {
    // Genug Platz über dem Feld: die Wunschhöhe gilt, nicht mehr.
    const platz = listenplatz(anker(700), fenster);
    expect(platz.hoehe).toBe(HOEHE_MAX);
    expect(platz.oben).toBe(700 - ABSTAND - HOEHE_MAX);
  });

  it("klappt nach oben, wenn die Liste unten nicht ganz hinpasst", () => {
    // 150 px frei unter dem Feld, die Liste ist 200 px hoch, oben ist Platz.
    const hoehe = 32;
    const oben = fenster.hoehe - hoehe - ABSTAND - RAND - 150;
    const platz = listenplatz(anker(oben, hoehe), fenster, 200);
    expect(platz.ueber).toBe(true);
    expect(platz.hoehe).toBe(200);
    expect(platz.oben).toBeGreaterThanOrEqual(RAND);
  });

  it("bleibt unten, wenn die Liste dort ganz hinpasst", () => {
    const hoehe = 32;
    const oben = fenster.hoehe - hoehe - ABSTAND - RAND - 150;
    const platz = listenplatz(anker(oben, hoehe), fenster, 120);
    expect(platz.ueber).toBe(false);
    expect(platz.hoehe).toBe(120);
    expect(platz.oben + platz.hoehe).toBeLessThanOrEqual(fenster.hoehe - RAND);
  });

  it("nimmt die größere Seite, wenn sie nirgends ganz hinpasst", () => {
    const eng = { breite: 1200, hoehe: 300 };
    const platz = listenplatz(anker(80), eng, 280);
    expect(platz.ueber).toBe(false);
    expect(platz.oben + platz.hoehe).toBeLessThanOrEqual(eng.hoehe - RAND);
    const tief = listenplatz(anker(200), eng, 280);
    expect(tief.ueber).toBe(true);
    expect(tief.oben).toBeGreaterThanOrEqual(RAND);
  });

  it("bleibt waagerecht im Fenster", () => {
    const rechts = listenplatz(anker(200, 32, 1150), fenster);
    expect(rechts.links + rechts.breite).toBeLessThanOrEqual(fenster.breite - RAND);
    expect(rechts.links).toBeGreaterThanOrEqual(RAND);
    // Ein Feld, das breiter wäre als das Fenster: die Liste wird schmaler.
    const breit = listenplatz(anker(200, 32, 0, 5000), fenster);
    expect(breit.breite).toBe(fenster.breite - 2 * RAND);
    expect(breit.links).toBe(RAND);
  });
});

describe("anker_sichtbar", () => {
  it("erkennt ein Feld außerhalb des Fensters", () => {
    expect(anker_sichtbar(anker(100), fenster)).toBe(true);
    expect(anker_sichtbar(anker(-100), fenster)).toBe(false);
    expect(anker_sichtbar(anker(900), fenster)).toBe(false);
    expect(anker_sichtbar(anker(100, 32, 100, 0), fenster)).toBe(false);
  });
});

const optionen: ListOption[] = [
  { value: "a", label: "Ahorn" },
  { value: "b", label: "Birke", disabled: true },
  { value: "c", label: "Cypresse" },
];

describe("Tastatur", () => {
  it("findet den ersten und letzten wählbaren Eintrag", () => {
    expect(erste_aktive(optionen)).toBe(0);
    expect(letzte_aktive(optionen)).toBe(2);
    expect(erste_aktive([{ value: "x", label: "X", disabled: true }])).toBe(-1);
    expect(letzte_aktive([])).toBe(-1);
  });

  it("überspringt gesperrte Einträge und läuft um", () => {
    expect(naechste_aktive(optionen, 0, 1)).toBe(2);
    expect(naechste_aktive(optionen, 2, 1)).toBe(0);
    expect(naechste_aktive(optionen, 0, -1)).toBe(2);
    expect(naechste_aktive(optionen, 2, -1)).toBe(0);
    expect(naechste_aktive([], 0, 1)).toBe(-1);
  });

  it("sucht mit getippten Buchstaben", () => {
    expect(tippsuche(optionen, "C", 0)).toBe(2);
    expect(tippsuche(optionen, "b", 0)).toBe(-1);
    expect(tippsuche(optionen, "A", 2)).toBe(0);
  });
});

describe("filtern", () => {
  it("zeigt ohne Suche alles und sucht sonst in Text und Hinweis", () => {
    expect(filtern(optionen, "  ")).toHaveLength(3);
    expect(filtern(optionen, "bir").map((o) => o.value)).toEqual(["b"]);
    const mit_hinweis: ListOption[] = [
      { value: "x", label: "Alex", hint: "alex@example.test" },
      { value: "y", label: "Ben", hint: "ben@example.test" },
    ];
    expect(filtern(mit_hinweis, "example").map((o) => o.value)).toEqual(["x", "y"]);
    expect(filtern(mit_hinweis, "ben@").map((o) => o.value)).toEqual(["y"]);
    expect(filtern(mit_hinweis, "niemand")).toEqual([]);
  });
});
