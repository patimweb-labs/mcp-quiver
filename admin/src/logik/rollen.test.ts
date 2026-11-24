import { describe, expect, it } from "vitest";

import {
  ROLLEN,
  ROLLE_TEXT,
  darf_verwalten,
  hoehere_rolle,
  rang,
  rolle_eintrag,
  rolle_fest,
  rollen_text,
} from "./rollen";

describe("Rollen als Daten", () => {
  it("kennt Root, Admin und Mitarbeiter mit Rang", () => {
    expect(ROLLEN.map((eintrag) => eintrag.wert)).toEqual([
      "root",
      "admin",
      "benutzer",
    ]);
    expect(rang("root")).toBeGreaterThan(rang("admin"));
    expect(rang("admin")).toBeGreaterThan(rang("benutzer"));
    for (const eintrag of ROLLEN) {
      expect(eintrag.titel.length).toBeGreaterThan(0);
      expect(eintrag.unter.length).toBeGreaterThan(0);
      expect(ROLLE_TEXT[eintrag.wert]).toBe(eintrag.titel);
    }
  });

  it("der Rollenwechsel steht in der Tabelle", () => {
    expect(rolle_eintrag("admin").wechsel).toBe("benutzer");
    expect(rolle_eintrag("benutzer").wechsel).toBe("admin");
    expect(rolle_eintrag("root").wechsel).toBeNull();
  });

  it("nur Root ist fest", () => {
    expect(rolle_fest("root")).toBe(true);
    expect(rolle_fest("admin")).toBe(false);
    expect(rolle_fest("benutzer")).toBe(false);
  });

  it("nur Root und Admin verwalten", () => {
    expect(darf_verwalten("root")).toBe(true);
    expect(darf_verwalten("admin")).toBe(true);
    expect(darf_verwalten("benutzer")).toBe(false);
  });

  it("die höhere Rolle gilt", () => {
    expect(hoehere_rolle("benutzer", "admin")).toBe("admin");
    expect(hoehere_rolle("admin", "benutzer")).toBe("admin");
    expect(hoehere_rolle("root", "admin")).toBe("root");
  });

  it("eine unbekannte Rolle fällt auf Mitarbeiter zurück", () => {
    expect(rollen_text("erfunden" as never)).toBe("Mitarbeiter");
    expect(rolle_eintrag("erfunden" as never).wert).toBe("benutzer");
  });
});
