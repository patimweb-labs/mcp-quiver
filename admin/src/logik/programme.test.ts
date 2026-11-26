import { describe, expect, it } from "vitest";

import type { GeraetStand, Programm } from "../api/typen";
import { groesse_text, laufende_programme, programm_zeilen } from "./programme";

function programm(teil: Partial<Programm> = {}): Programm {
  return {
    name: "quiver-devops",
    version: "1.2.3",
    groesse: 8 * 1024 * 1024,
    sha256: "a".repeat(64),
    system: "linux",
    arch: "x64",
    ...teil,
  };
}

describe("programm_zeilen", () => {
  it("fasst die Systeme eines Namens in einer Zeile zusammen", () => {
    const zeilen = programm_zeilen([
      programm({ system: "linux", arch: "x64" }),
      programm({ system: "macos", arch: "arm64" }),
      programm({ system: "linux", arch: "arm64" }),
    ]);
    expect(zeilen).toHaveLength(1);
    expect(zeilen[0].name).toBe("quiver-devops");
    expect(zeilen[0].systeme).toEqual(["linux/arm64", "linux/x64", "macos/arm64"]);
    expect(zeilen[0].fassungen).toEqual(["1.2.3"]);
  });

  it("nennt verschiedene Fassungen und nimmt die größte zur Anzeige", () => {
    const zeilen = programm_zeilen([
      programm({ version: "1.2.3", groesse: 1024 * 1024 }),
      programm({ version: "1.3.0", groesse: 3 * 1024 * 1024, system: "macos", arch: "x64" }),
      programm({ name: "quiver-m365", version: "unbekannt", groesse: 512 }),
    ]);
    expect(zeilen.map((zeile) => zeile.name)).toEqual([
      "quiver-devops",
      "quiver-m365",
    ]);
    expect(zeilen[0].fassungen).toEqual(["1.2.3", "1.3.0"]);
    expect(zeilen[0].groesse).toBe("3.0 MiB");
    expect(zeilen[1].groesse).toBe("1 KiB");
  });

  it("bleibt ohne Programme leer", () => {
    expect(programm_zeilen([])).toEqual([]);
  });
});

describe("groesse_text", () => {
  it("rechnet in MiB und KiB", () => {
    expect(groesse_text(8 * 1024 * 1024)).toBe("8.0 MiB");
    expect(groesse_text(1024 * 1024 * 1.5)).toBe("1.5 MiB");
    expect(groesse_text(2048)).toBe("2 KiB");
  });
});

describe("laufende_programme", () => {
  it("liest die gemeldeten Fassungen", () => {
    const stand = {
      meldung: {
        revision: "r",
        client_version: "0.3.0",
        eintraege: [],
        programme: [programm(), programm({ name: "quiver-m365", version: "2.0.0" })],
      },
    } as unknown as GeraetStand;
    expect(laufende_programme(stand)).toEqual([
      "quiver-devops 1.2.3",
      "quiver-m365 2.0.0",
    ]);
  });

  it("bleibt bei einem älteren Client leer", () => {
    const stand = {
      meldung: { revision: "r", client_version: "0.1.0", eintraege: [] },
    } as unknown as GeraetStand;
    expect(laufende_programme(stand)).toEqual([]);
  });
});
