import { describe, expect, it } from "vitest";

import { server_json_lesen } from "./serverjson";

describe("server.json lesen", () => {
  it("nimmt ein JSON-Objekt an", () => {
    const ergebnis = server_json_lesen('{"mcpServers":{"mail":{"command":"x"}}}');
    expect(ergebnis.art).toBe("ok");
    if (ergebnis.art === "ok") {
      expect(ergebnis.wert).toHaveProperty("mcpServers");
    }
  });

  it("meldet leeren Inhalt", () => {
    expect(server_json_lesen("   ")).toEqual({
      art: "fehler",
      text: "Der Inhalt ist leer.",
    });
  });

  it("meldet kaputtes JSON mit Ursache", () => {
    const ergebnis = server_json_lesen("{kaputt");
    expect(ergebnis.art).toBe("fehler");
    if (ergebnis.art === "fehler") expect(ergebnis.text).toContain("JSON");
  });

  it("weist Felder und Listen ab", () => {
    for (const text of ["[]", "42", "null", '"text"']) {
      expect(server_json_lesen(text).art).toBe("fehler");
    }
  });
});
