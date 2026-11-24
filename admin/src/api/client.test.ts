import { describe, expect, it } from "vitest";

import {
  API_BASIS,
  AUTH_BASIS,
  MICROSOFT_ANMELDUNG,
  ist_aendernd,
  kopfzeilen,
} from "./client";

describe("Köpfe einer Anfrage", () => {
  it("setzt kein Authorization und kein Token", () => {
    for (const methode of ["GET", "POST", "PUT", "DELETE"] as const) {
      const koepfe = kopfzeilen(methode, true);
      expect(Object.keys(koepfe).map((name) => name.toLowerCase())).not.toContain(
        "authorization",
      );
    }
  });

  it("markiert ändernde Anfragen mit X-Quiver", () => {
    for (const methode of ["POST", "PUT", "PATCH", "DELETE"] as const) {
      expect(ist_aendernd(methode)).toBe(true);
      expect(kopfzeilen(methode, false)["X-Quiver"]).toBe("1");
    }
  });

  it("lässt GET ohne X-Quiver", () => {
    expect(ist_aendernd("GET")).toBe(false);
    expect(kopfzeilen("GET", false)).toEqual({ Accept: "application/json" });
  });

  it("setzt Content-Type nur mit Rumpf", () => {
    expect(kopfzeilen("POST", true)["Content-Type"]).toBe("application/json");
    expect(kopfzeilen("POST", false)["Content-Type"]).toBeUndefined();
  });
});

describe("Pfade", () => {
  it("hält die Verwaltung unter /api/v1", () => {
    expect(API_BASIS).toBe("/api/v1");
  });

  it("legt die Anmeldung auf dieselbe Wurzel wie die Seite", () => {
    expect(AUTH_BASIS).toBe("/auth");
    expect(MICROSOFT_ANMELDUNG).toBe("/auth/admin/start");
  });
});
