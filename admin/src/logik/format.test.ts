import { describe, expect, it } from "vitest";

import { abgelaufen, datum, kuerzen, restzeit, zeitpunkt } from "./format";

const jetzt = new Date(2026, 1, 5, 14, 7, 0);

function vor(sekunden: number): string {
  return new Date(jetzt.getTime() - sekunden * 1000).toISOString();
}

function in_zukunft(sekunden: number): string {
  return new Date(jetzt.getTime() + sekunden * 1000).toISOString();
}

describe("datum", () => {
  it("schreibt Ortszeit mit führenden Nullen", () => {
    expect(datum(jetzt.toISOString())).toBe("05.02.2026, 14:07");
  });

  it("bleibt bei Unbrauchbarem höflich", () => {
    expect(datum("keine Zeit")).toBe("—");
  });
});

describe("zeitpunkt", () => {
  it("sagt „gerade eben“ für die letzte Minute", () => {
    expect(zeitpunkt(vor(10), jetzt)).toBe("gerade eben");
  });

  it("zählt Minuten und Stunden", () => {
    expect(zeitpunkt(vor(60), jetzt)).toBe("vor 1 Minute");
    expect(zeitpunkt(vor(4 * 60), jetzt)).toBe("vor 4 Minuten");
    expect(zeitpunkt(vor(3600), jetzt)).toBe("vor 1 Stunde");
    expect(zeitpunkt(vor(3 * 3600), jetzt)).toBe("vor 3 Stunden");
  });

  it("zählt Tage, danach kommt das Datum", () => {
    expect(zeitpunkt(vor(2 * 86400), jetzt)).toBe("vor 2 Tagen");
    expect(zeitpunkt(vor(90 * 86400), jetzt)).toBe(
      datum(vor(90 * 86400)),
    );
  });

  it("kennt auch die Zukunft", () => {
    expect(zeitpunkt(in_zukunft(120), jetzt)).toBe("in 2 Minuten");
    expect(zeitpunkt(in_zukunft(3600), jetzt)).toBe("in 1 Stunde");
  });

  it("meldet fehlende Zeit", () => {
    expect(zeitpunkt(null, jetzt)).toBe("noch nie");
    expect(zeitpunkt("unsinn", jetzt)).toBe("—");
  });
});

describe("abgelaufen und restzeit", () => {
  it("vergleicht mit dem übergebenen Jetzt", () => {
    expect(abgelaufen(in_zukunft(60), jetzt)).toBe(false);
    expect(abgelaufen(vor(60), jetzt)).toBe(true);
    expect(abgelaufen("unsinn", jetzt)).toBe(true);
  });

  it("sagt die Restzeit an", () => {
    expect(restzeit(in_zukunft(42 * 60), jetzt)).toBe("noch 42 Minuten gültig");
    expect(restzeit(in_zukunft(60), jetzt)).toBe("noch 1 Minute gültig");
    expect(restzeit(in_zukunft(2 * 3600), jetzt)).toBe("noch 2 Stunden gültig");
    expect(restzeit(vor(1), jetzt)).toBe("abgelaufen");
  });
});

describe("kuerzen", () => {
  it("lässt kurze Texte stehen", () => {
    expect(kuerzen("kurz", 10)).toBe("kurz");
  });

  it("kürzt mit Auslassungszeichen", () => {
    expect(kuerzen("abcdefghij", 5)).toBe("abcd…");
  });
});
