import { describe, expect, it } from "vitest";

import type { AuthStand, BenutzerSicht, Ich, Rolle } from "../api/typen";
import {
  HINWEIS_ABGELAUFEN,
  PASSWORT_MAX,
  PASSWORT_MIN,
  anmeldung_fehler,
  anmeldung_lage,
  anzeige_name,
  code_fehler,
  darf_verwalten,
  einrichtung_fehler,
  email_fehler,
  email_normalisieren,
  ist_eigener_pfad,
  lagetitel,
  lage_aus_ich,
  lage_aus_stand,
  lage_nach_401,
  lage_ohne_stand,
  passwort_fehler,
  rollen_text,
  weiter_nach_anmeldung,
  wiederholung_fehler,
  zeichen,
} from "./anmeldung";

function stand(teil: Partial<AuthStand> = {}): AuthStand {
  return {
    eingerichtet: true,
    einrichtung: null,
    code_noetig: false,
    entra: true,
    ...teil,
  };
}

function benutzer(teil: Partial<BenutzerSicht> = {}): BenutzerSicht {
  return {
    oid: "11111111-1111-1111-1111-111111111111",
    email: "root@example.test",
    name: "Rita Root",
    rolle: "root",
    gesperrt: false,
    entra: [],
    zuletzt: null,
    token_stand: "gueltig",
    geraete: 0,
    ...teil,
  };
}

function ich(rolle: Rolle, teil: Partial<BenutzerSicht> = {}): Ich {
  return { benutzer: benutzer({ rolle, ...teil }), art: "root" };
}

describe("Lage aus dem Stand des Servers", () => {
  it("führt mit Root zur Anmeldung, mit Microsoft nur wenn eingerichtet", () => {
    expect(lage_aus_stand(stand({ entra: true }))).toEqual(
      anmeldung_lage(true),
    );
    expect(lage_aus_stand(stand({ entra: false }))).toEqual(
      anmeldung_lage(false),
    );
  });

  it("zeigt ohne Root die Ersteinrichtung, wenn das Fenster offen ist", () => {
    expect(
      lage_aus_stand(
        stand({ eingerichtet: false, einrichtung: "offen", code_noetig: false }),
      ),
    ).toEqual({ art: "einrichtung", code_noetig: false });
    expect(
      lage_aus_stand(
        stand({ eingerichtet: false, einrichtung: "offen", code_noetig: true }),
      ),
    ).toEqual({ art: "einrichtung", code_noetig: true });
  });

  it("zeigt die geschlossene Seite, wenn das Fenster zu ist", () => {
    expect(
      lage_aus_stand(stand({ eingerichtet: false, einrichtung: "geschlossen" })),
    ).toEqual({ art: "geschlossen" });
  });

  it("bietet ohne anwendbare Einrichtung keinen Weg hinein", () => {
    expect(
      lage_aus_stand(stand({ eingerichtet: false, einrichtung: null })),
    ).toEqual({ art: "geschlossen" });
  });

  it("kommt ohne `/auth/stand` mit den Anmeldearten aus", () => {
    expect(lage_ohne_stand({ entra: true, root: true })).toEqual(
      anmeldung_lage(true),
    );
    expect(lage_ohne_stand({ entra: false, root: true })).toEqual(
      anmeldung_lage(false),
    );
  });

  it("erklärt einen 401 und behält den Microsoft-Weg", () => {
    expect(lage_nach_401(true)).toEqual({
      art: "anmeldung",
      entra: true,
      hinweis: HINWEIS_ABGELAUFEN,
    });
    expect(lage_nach_401(false)).toEqual({
      art: "anmeldung",
      entra: false,
      hinweis: HINWEIS_ABGELAUFEN,
    });
  });
});

describe("Lage nach der Anmeldung", () => {
  it("lässt Root und Admin in die Verwaltung", () => {
    expect(darf_verwalten("root")).toBe(true);
    expect(darf_verwalten("admin")).toBe(true);
    expect(lage_aus_ich(ich("admin")).art).toBe("bereit");
    expect(lage_aus_ich(ich("root")).art).toBe("bereit");
  });

  it("sperrt die Rolle Mitarbeiter aus", () => {
    expect(darf_verwalten("benutzer")).toBe(false);
    expect(lage_aus_ich(ich("benutzer"))).toEqual({
      art: "kein_zugang",
      ich: ich("benutzer"),
    });
  });
});

describe("Rücksprung nach der Anmeldung", () => {
  it("nimmt eigene Seiten", () => {
    expect(weiter_nach_anmeldung("#/geraete")).toBe("geraete");
    expect(weiter_nach_anmeldung("#geraete")).toBe("geraete");
    expect(weiter_nach_anmeldung("bibliothek")).toBe("bibliothek");
  });

  it("kennt nur die eigenen Pfade", () => {
    expect(ist_eigener_pfad("#/saetze")).toBe(true);
    expect(ist_eigener_pfad("/saetze")).toBe(false);
    expect(ist_eigener_pfad("#/saetze/1")).toBe(false);
  });

  it("fällt bei fremden Zielen auf den Überblick zurück", () => {
    for (const ziel of [
      "https://fremd.example/#/geraete",
      "//fremd.example/geraete",
      "javascript:alert(1)",
      "#/quatsch",
      "",
      null,
      undefined,
    ]) {
      expect(weiter_nach_anmeldung(ziel)).toBe("ueberblick");
    }
  });
});

describe("E-Mail", () => {
  it("normalisiert wie der Server", () => {
    expect(email_normalisieren("  Rita@Example.Test ")).toBe(
      "rita@example.test",
    );
  });

  it("verlangt etwas mit @", () => {
    expect(email_fehler("")).toMatch(/eintragen/);
    expect(email_fehler("   ")).toMatch(/eintragen/);
    expect(email_fehler("rita.example.test")).toMatch(/@/);
    expect(email_fehler("rita@example.test")).toBeNull();
  });

  it("begrenzt die Länge auf 254 Zeichen", () => {
    expect(email_fehler(`${"a".repeat(250)}@b.de`)).not.toBeNull();
  });
});

describe("Passwort", () => {
  it("zählt Zeichen, nicht Bytes", () => {
    expect(zeichen("😀😀")).toBe(2);
    expect(passwort_fehler("😀".repeat(PASSWORT_MIN))).toBeNull();
  });

  it("hält die Grenzen des Servers ein", () => {
    expect(passwort_fehler("")).toMatch(/eintragen/);
    expect(passwort_fehler("a".repeat(PASSWORT_MIN - 1))).toBe(
      `Mindestens ${PASSWORT_MIN} Zeichen.`,
    );
    expect(passwort_fehler("a".repeat(PASSWORT_MIN))).toBeNull();
    expect(passwort_fehler("a".repeat(PASSWORT_MAX))).toBeNull();
    expect(passwort_fehler("a".repeat(PASSWORT_MAX + 1))).toBe(
      `Höchstens ${PASSWORT_MAX} Zeichen.`,
    );
  });

  it("verlangt die Wiederholung", () => {
    expect(wiederholung_fehler("geheim-genug-123", "")).toMatch(/wiederholen/);
    expect(wiederholung_fehler("geheim-genug-123", "geheim-genug-124")).toBe(
      "Die Passwörter sind nicht gleich.",
    );
    expect(wiederholung_fehler("geheim-genug-123", "geheim-genug-123")).toBeNull();
  });

  it("prüft den Code nur, wenn er nötig ist", () => {
    expect(code_fehler("", false)).toBeNull();
    expect(code_fehler("  ", true)).toMatch(/Code/);
    expect(code_fehler("abc", true)).toBeNull();
  });
});

describe("Formulare", () => {
  const gut = {
    email: "rita@example.test",
    passwort: "geheim-genug-123",
    wiederholung: "geheim-genug-123",
    code: "",
  };

  it("lässt ein gutes Formular durch", () => {
    expect(einrichtung_fehler(gut, false)).toBeNull();
    expect(einrichtung_fehler({ ...gut, code: "abc" }, true)).toBeNull();
  });

  it("meldet den ersten Fehler mit seinem Feld", () => {
    expect(einrichtung_fehler({ ...gut, email: "" }, false)?.feld).toBe("email");
    expect(einrichtung_fehler({ ...gut, passwort: "kurz" }, false)?.feld).toBe(
      "passwort",
    );
    expect(
      einrichtung_fehler({ ...gut, wiederholung: "anders-genug-123" }, false)
        ?.feld,
    ).toBe("wiederholung");
    expect(einrichtung_fehler(gut, true)?.feld).toBe("code");
  });

  it("nimmt beim Anmelden jedes nicht-leere Passwort an", () => {
    expect(anmeldung_fehler("", "x")?.feld).toBe("email");
    expect(anmeldung_fehler("rita@example.test", "")?.feld).toBe("passwort");
    expect(anmeldung_fehler("rita@example.test", "kurz")).toBeNull();
  });
});

describe("Titel der Lage", () => {
  it("gibt jeder Lage einen Namen", () => {
    const titel = [
      lagetitel({ art: "laedt" }),
      lagetitel({ art: "einrichtung", code_noetig: true }),
      lagetitel({ art: "geschlossen" }),
      lagetitel(anmeldung_lage(true)),
      lagetitel({ art: "bereit", ich: ich("root") }),
      lagetitel({ art: "kein_zugang", ich: ich("benutzer") }),
      lagetitel({ art: "fehler", text: "kaputt" }),
    ];
    for (const wert of titel) expect(wert.length).toBeGreaterThan(0);
    expect(lagetitel(anmeldung_lage(false))).toBe("Anmelden");
    expect(lagetitel({ art: "einrichtung", code_noetig: false })).toBe(
      "Ersteinrichtung",
    );
  });
});

describe("Anzeige im Kopf", () => {
  it("nimmt den Namen, sonst die E-Mail, sonst die Id", () => {
    expect(anzeige_name(benutzer())).toBe("Rita Root");
    expect(anzeige_name(benutzer({ name: "  " }))).toBe("root@example.test");
    expect(anzeige_name(benutzer({ name: null, email: null }))).toBe(
      "11111111-1111-1111-1111-111111111111",
    );
  });

  it("nennt die Rollen wie die Aufgabe", () => {
    expect(rollen_text("root")).toBe("Root");
    expect(rollen_text("admin")).toBe("Admin");
    expect(rollen_text("benutzer")).toBe("Mitarbeiter");
  });
});
