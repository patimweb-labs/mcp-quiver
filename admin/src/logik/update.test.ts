import { describe, expect, it } from "vitest";

import type { UpdateStand } from "../api/typen";
import {
  abfrage_text,
  neueste_version,
  stand_hinweis,
  veraltet,
  veraltete_text,
  version_neuer,
} from "./update";

function stand(teil: Partial<UpdateStand> = {}): UpdateStand {
  return {
    server_version: "0.2.0",
    abfrage_an: true,
    zuletzt_geprueft: "2026-12-01T00:00:00Z",
    neueste_version: "0.3.0",
    aenderungen: "https://github.com/patimweb-labs/mcp-quiver/releases/tag/v0.3.0",
    server_veraltet: true,
    veraltete_clients: [],
    ...teil,
  };
}

describe("Versionen", () => {
  it("vergleicht Zahlen und Vorabfassungen", () => {
    expect(version_neuer("0.3.0", "0.2.9")).toBe(true);
    expect(version_neuer("v0.3.0", "0.2.0")).toBe(true);
    expect(version_neuer("1.0.0", "1.0.0-rc.1")).toBe(true);
    expect(version_neuer("0.2.0", "0.2.0")).toBe(false);
    expect(version_neuer("0.2.0", "0.3.0")).toBe(false);
  });

  it("hält Unlesbares für nicht neuer", () => {
    expect(version_neuer("kaputt", "0.1.0")).toBe(false);
    expect(version_neuer("0.3.0", "")).toBe(false);
  });
});

describe("Stand", () => {
  it("nennt die neueste Version oder lässt sie leer", () => {
    expect(neueste_version(stand())).toBe("0.3.0");
    expect(neueste_version(null)).toBe("");
    expect(neueste_version(stand({ neueste_version: null }))).toBe("");
  });

  it("erkennt einen veralteten Client", () => {
    expect(veraltet("0.2.0", stand())).toBe(true);
    expect(veraltet("0.3.0", stand())).toBe(false);
    expect(veraltet(undefined, stand())).toBe(false);
    expect(veraltet("0.2.0", stand({ neueste_version: null }))).toBe(false);
  });

  it("baut den Hinweis der Übersicht", () => {
    expect(stand_hinweis(stand())).toContain("veröffentlicht ist 0.3.0");
    expect(stand_hinweis(stand({ server_veraltet: false }))).toBe("");
    expect(stand_hinweis(null)).toBe("");
    expect(stand_hinweis(stand({ fehler: "nicht erreichbar" }))).toBe(
      "nicht erreichbar",
    );
    expect(stand_hinweis(stand({ neueste_version: null }))).toContain(
      "noch nicht abgefragt",
    );
  });

  it("sagt, ob die Abfrage läuft", () => {
    expect(abfrage_text(stand())).toContain("höchstens alle 6 Stunden");
    expect(abfrage_text(stand({ abfrage_an: false }))).toContain("QUIVER_UPDATES");
    expect(abfrage_text(null)).toBe("—");
  });

  it("zählt die Geräte mit älterem Client", () => {
    expect(veraltete_text(stand())).toBe("");
    const einer = stand({
      veraltete_clients: [{ geraet: "g1", name: "rechner", version: "0.2.0" }],
    });
    expect(veraltete_text(einer)).toBe("Ein Gerät meldet einen älteren Client.");
    const zwei = stand({
      veraltete_clients: [
        { geraet: "g1", name: "rechner", version: "0.2.0" },
        { geraet: "g2", name: "notebook", version: "0.2.0" },
      ],
    });
    expect(veraltete_text(zwei)).toBe("2 Geräte melden einen älteren Client.");
  });
});
