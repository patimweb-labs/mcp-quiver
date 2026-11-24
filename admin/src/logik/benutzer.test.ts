import { describe, expect, it } from "vitest";

import type { BenutzerSicht, EntraGruppe } from "../api/typen";
import {
  OHNE_FILTER,
  aktionen,
  benutzer_name,
  entra_gruppen,
  filtern,
  geraete_text,
  lage_passt,
  loesch_frage,
  pruef_frage,
  pruefstand,
  rollen_aendern,
  rollen_frage,
  rollenknopf,
  sortiert,
  sperre_aendern,
  sperre_frage,
  zielrolle,
} from "./benutzer";

function benutzer(teil: Partial<BenutzerSicht> = {}): BenutzerSicht {
  return {
    oid: "11111111-1111-1111-1111-111111111111",
    email: "anna@example.test",
    name: "Anna Admin",
    rolle: "admin",
    gesperrt: false,
    entra: [],
    zuletzt: null,
    token_stand: "gueltig",
    geraete: 0,
    ...teil,
  };
}

const GRUPPE_A = "aaaaaaaa-1111-1111-1111-111111111111";
const GRUPPE_B = "bbbbbbbb-2222-2222-2222-222222222222";

describe("Filtern und Suchen", () => {
  const liste = [
    benutzer({
      oid: "1",
      name: "Rita Root",
      email: "root@example.test",
      rolle: "root",
    }),
    benutzer({
      oid: "2",
      name: "Anna Admin",
      email: "anna@example.test",
      rolle: "admin",
      gesperrt: true,
    }),
    benutzer({
      oid: "3",
      name: "Bea Benutzer",
      email: "bea@example.test",
      rolle: "benutzer",
      token_stand: "neu_anmelden",
    }),
  ];

  it("findet in Name, E-Mail und Objekt-Id", () => {
    const namen = (suche: string) =>
      filtern(liste, { ...OHNE_FILTER, suche }).map(benutzer_name);
    expect(namen("bea")).toEqual(["Bea Benutzer"]);
    expect(namen("anna@example")).toEqual(["Anna Admin"]);
    expect(namen("3")).toEqual(["Bea Benutzer"]);
    expect(namen("gibtsnicht")).toEqual([]);
  });

  it("filtert nach Rolle aus der Rollentabelle", () => {
    expect(
      filtern(liste, { ...OHNE_FILTER, rolle: "admin" }).map(benutzer_name),
    ).toEqual(["Anna Admin"]);
    expect(
      filtern(liste, { ...OHNE_FILTER, rolle: "benutzer" }).map(benutzer_name),
    ).toEqual(["Bea Benutzer"]);
  });

  it("filtert nach Lage: aktiv, gesperrt, neu anmelden", () => {
    expect(
      filtern(liste, { ...OHNE_FILTER, lage: "gesperrt" }).map(benutzer_name),
    ).toEqual(["Anna Admin"]);
    expect(
      filtern(liste, { ...OHNE_FILTER, lage: "neu_anmelden" }).map(
        benutzer_name,
      ),
    ).toEqual(["Bea Benutzer"]);
    expect(
      filtern(liste, { ...OHNE_FILTER, lage: "aktiv" }).map(benutzer_name),
    ).toEqual(["Rita Root", "Bea Benutzer"]);
  });

  it("kombiniert Suche und Filter", () => {
    expect(
      filtern(liste, { suche: "example", rolle: "benutzer", lage: "alle" }).map(
        benutzer_name,
      ),
    ).toEqual(["Bea Benutzer"]);
  });

  it("lässt Gesperrte nicht als „neu anmelden“ gelten", () => {
    expect(
      lage_passt(benutzer({ gesperrt: true, token_stand: "neu_anmelden" }), "neu_anmelden"),
    ).toBe(false);
  });
});

describe("Reihenfolge", () => {
  it("zeigt Root zuerst, dann den Rang, dann den Namen", () => {
    const liste = [
      benutzer({ oid: "b", name: "Zoe", rolle: "benutzer" }),
      benutzer({ oid: "a", name: "Anna", rolle: "benutzer" }),
      benutzer({ oid: "r", name: "Rita", rolle: "root" }),
      benutzer({ oid: "m", name: "Mia", rolle: "admin" }),
    ];
    expect(sortiert(liste).map(benutzer_name)).toEqual([
      "Rita",
      "Mia",
      "Anna",
      "Zoe",
    ]);
  });
});

describe("Entra-Gruppen mit Anzeigename", () => {
  const gesehen: EntraGruppe[] = [
    {
      id: GRUPPE_A,
      anzeigename: "Entwicklung",
      zuletzt: "2026-11-24T10:00:00Z",
    },
    {
      id: GRUPPE_B,
      anzeigename: "  ",
      zuletzt: "2026-11-24T10:00:00Z",
    },
  ];

  it("nimmt den Anzeigenamen, sonst die Objekt-Id", () => {
    const zeilen = entra_gruppen(
      benutzer({ entra: [GRUPPE_A, GRUPPE_B] }),
      gesehen,
    );
    expect(zeilen).toEqual([
      { id: GRUPPE_B, name: GRUPPE_B },
      { id: GRUPPE_A, name: "Entwicklung" },
    ]);
  });

  it("kommt mit einer Id ohne gesehenen Namen zurecht", () => {
    const unbekannt = "cccccccc-3333-3333-3333-333333333333";
    expect(entra_gruppen(benutzer({ entra: [unbekannt] }), [])).toEqual([
      { id: unbekannt, name: unbekannt },
    ]);
    expect(entra_gruppen(benutzer({ entra: [] }), gesehen)).toEqual([]);
  });
});

describe("Stand der Prüfung", () => {
  it("Root und Server ohne Entra brauchen kein Token", () => {
    expect(pruefstand(benutzer({ rolle: "root" }), true).text).toBe("Notzugang");
    expect(pruefstand(benutzer(), false).text).toBe("ohne Entra");
  });

  it("gültig heißt geprüft, sonst neu anmelden", () => {
    const gueltig = pruefstand(benutzer(), true);
    expect(gueltig.text).toBe("geprüft");
    expect(gueltig.ton).toBe("gut");
    const neu = pruefstand(benutzer({ token_stand: "neu_anmelden" }), true);
    expect(neu.text).toBe("neu anmelden nötig");
    expect(neu.ton).toBe("warnung");
  });
});

describe("Aktionen", () => {
  it("lässt den Root unangetastet", () => {
    expect(aktionen(benutzer({ rolle: "root" }), true)).toEqual({
      rolle: false,
      sperre: false,
      pruefen: false,
      loeschen: false,
    });
  });

  it("bietet ohne Entra kein Nachprüfen an", () => {
    expect(aktionen(benutzer(), false).pruefen).toBe(false);
    expect(aktionen(benutzer(), true).pruefen).toBe(true);
  });

  it("bietet Gesperrten kein Nachprüfen an", () => {
    expect(aktionen(benutzer({ gesperrt: true }), true).pruefen).toBe(false);
    expect(aktionen(benutzer({ gesperrt: true }), true).sperre).toBe(true);
  });

  it("wechselt zwischen Admin und Mitarbeiter", () => {
    expect(zielrolle(benutzer({ rolle: "admin" }))).toBe("benutzer");
    expect(zielrolle(benutzer({ rolle: "benutzer" }))).toBe("admin");
    expect(zielrolle(benutzer({ rolle: "root" }))).toBeNull();
    expect(rollenknopf(benutzer({ rolle: "admin" }))).toBe(
      "Zum Mitarbeiter",
    );
    expect(rollenknopf(benutzer({ rolle: "benutzer" }))).toBe(
      "Zum Admin",
    );
    expect(rollenknopf(benutzer({ rolle: "root" }))).toBe("Rolle fest");
  });

  it("baut die Rümpfe, die der Vertrag erwartet", () => {
    expect(rollen_aendern(benutzer({ rolle: "benutzer" }), true)).toEqual({
      rolle: "admin",
      selbst_bestaetigt: true,
    });
    expect(rollen_aendern(benutzer({ rolle: "root" }), false)).toBeNull();
    expect(sperre_aendern(benutzer(), false)).toEqual({
      gesperrt: true,
      selbst_bestaetigt: false,
    });
    expect(sperre_aendern(benutzer({ gesperrt: true }), true)).toEqual({
      gesperrt: false,
      selbst_bestaetigt: true,
    });
  });
});

describe("Rückfragen", () => {
  it("erklärt jede Handlung mit Namen und Folge", () => {
    expect(rollen_frage(benutzer({ rolle: "benutzer" }), false)).toContain(
      "Anna Admin wird Admin",
    );
    expect(rollen_frage(benutzer({ rolle: "admin" }), false)).toContain(
      "wieder Mitarbeiter",
    );
    expect(rollen_frage(benutzer({ rolle: "admin" }), true)).toContain(
      "nimmst dir selbst",
    );
    expect(sperre_frage(benutzer(), false)).toContain("wird gesperrt");
    expect(sperre_frage(benutzer({ gesperrt: true }), false)).toContain(
      "wird entsperrt",
    );
    expect(pruef_frage(benutzer())).toContain("nachgeprüft");
    expect(loesch_frage(benutzer(), false)).toContain("anna@example.test");
    expect(loesch_frage(benutzer(), true)).toContain("dein eigenes Konto");
  });

  it("zählt Geräte richtig", () => {
    expect(geraete_text(0)).toBe("0 Geräte");
    expect(geraete_text(1)).toBe("1 Gerät");
    expect(geraete_text(2)).toBe("2 Geräte");
  });

  it("fällt auf E-Mail und Objekt-Id zurück, wenn der Name fehlt", () => {
    expect(benutzer_name(benutzer({ name: null }))).toBe("anna@example.test");
    expect(benutzer_name(benutzer({ name: null, email: null }))).toBe(
      "11111111-1111-1111-1111-111111111111",
    );
  });
});
