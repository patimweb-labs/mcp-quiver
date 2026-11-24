// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

// Die Breitenstufen: dieselben Grenzen wie im Client-Fenster (Aufgabe 18).

import { describe, expect, it } from "vitest";

import { breite_stufe, grenzen_lesen } from "./breite";

const GRENZEN = { formular: 700, schmal: 760 };

describe("breite_stufe", () => {
  it("ist unter der Formulargrenze eng", () => {
    expect(breite_stufe(560, GRENZEN)).toBe("eng");
    expect(breite_stufe(699, GRENZEN)).toBe("eng");
  });

  it("ist ab der Formulargrenze schmal", () => {
    expect(breite_stufe(700, GRENZEN)).toBe("schmal");
    expect(breite_stufe(759, GRENZEN)).toBe("schmal");
  });

  it("ist ab der Seitenleisten-Grenze weit", () => {
    expect(breite_stufe(760, GRENZEN)).toBe("weit");
    expect(breite_stufe(1600, GRENZEN)).toBe("weit");
  });
});

describe("grenzen_lesen", () => {
  it("liest die Tokens aus den CSS-Variablen", () => {
    const werte: Record<string, string> = {
      "--wp-breakpoint-formular": "700px",
      "--wp-breakpoint-schmal": "760px",
    };
    expect(grenzen_lesen((name) => werte[name] ?? "")).toEqual({
      formular: 700,
      schmal: 760,
    });
  });

  it("bleibt weit, wenn ein Token fehlt", () => {
    const grenzen = grenzen_lesen(() => "");
    expect(grenzen.formular).toBe(0);
    expect(breite_stufe(400, grenzen)).toBe("weit");
  });
});
