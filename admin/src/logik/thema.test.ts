import { describe, expect, it } from "vitest";

import { THEMEN, THEMA_TEXT, ist_thema, thema_aus_wert } from "./thema";

describe("Themes", () => {
  it("kennt genau drei", () => {
    expect(THEMEN).toEqual(["dark", "light", "mono"]);
    for (const thema of THEMEN) {
      expect(THEMA_TEXT[thema].length).toBeGreaterThan(0);
      expect(ist_thema(thema)).toBe(true);
    }
  });

  it("fällt bei Unbekanntem auf dunkel zurück", () => {
    expect(thema_aus_wert("rosa")).toBe("dark");
    expect(thema_aus_wert(null)).toBe("dark");
    expect(thema_aus_wert("mono")).toBe("mono");
  });
});
