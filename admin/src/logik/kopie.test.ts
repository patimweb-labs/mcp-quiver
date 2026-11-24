import { describe, expect, it } from "vitest";

import { tief_kopie } from "./kopie";

describe("tief_kopie", () => {
  it("teilt nichts mehr mit dem Original", () => {
    const original = { a: 1, innen: { b: [1, 2] } };
    const kopie = tief_kopie(original);
    kopie.innen.b.push(3);
    expect(original.innen.b).toEqual([1, 2]);
  });

  it("bleibt bei JSON-Werten gleich", () => {
    const original = { id: "x", felder: [{ name: "a", standard: null }] };
    expect(tief_kopie(original)).toEqual(original);
  });
});
