import { describe, expect, it } from "vitest";

import type { BenutzerGruppe, BenutzerSicht, Gruppe, Gruppenmitglied } from "../api/typen";
import {
  aussperr_warnung,
  entfernbar,
  entra_hinweis,
  fehlende_gruppen,
  gesperrt_hinweis,
  gruppen_text,
  herkuenfte,
  kandidaten,
  letzte_gruppe,
  mitglied_name,
  mitglied_zeile,
  mitglieder_text,
  von_hand_anzahl,
} from "./mitglieder";

function zuordnung(teil: Partial<BenutzerGruppe> & { gruppe: string }): BenutzerGruppe {
  return {
    titel: teil.gruppe,
    aus_entra: false,
    von_hand: false,
    aus_geraet: false,
    von: null,
    seit: null,
    entra_ids: [],
    ...teil,
  };
}

function person(teil: Partial<BenutzerSicht> & { oid: string }): BenutzerSicht {
  return {
    email: null,
    name: null,
    rolle: "benutzer",
    gesperrt: false,
    entra: [],
    zuletzt: null,
    token_stand: "gueltig",
    geraete: 0,
    gruppen: [],
    ...teil,
  };
}

function mitglied(teil: Partial<Gruppenmitglied> & { oid: string }): Gruppenmitglied {
  return {
    email: null,
    name: null,
    gesperrt: false,
    aus_entra: false,
    von_hand: false,
    von: null,
    seit: null,
    ...teil,
  };
}

describe("Herkunft", () => {
  it("nennt Entra, Hand und Gerät – auch beides zugleich", () => {
    expect(herkuenfte({ aus_entra: true, von_hand: false }).map((h) => h.text)).toEqual([
      "aus Entra",
    ]);
    expect(herkuenfte({ aus_entra: false, von_hand: true }).map((h) => h.text)).toEqual([
      "von Hand",
    ]);
    expect(
      herkuenfte({ aus_entra: true, von_hand: true, aus_geraet: true }).map((h) => h.text),
    ).toEqual(["aus Entra", "von Hand", "am Gerät"]);
    // Jede Herkunft erklärt sich.
    for (const eintrag of herkuenfte({ aus_entra: true, von_hand: true, aus_geraet: true })) {
      expect(eintrag.hinweis.length).toBeGreaterThan(10);
    }
  });

  it("entfernbar ist nur, was von Hand vergeben wurde", () => {
    expect(entfernbar({ von_hand: true })).toBe(true);
    expect(entfernbar({ von_hand: false })).toBe(false);
  });
});

describe("letzte Gruppe", () => {
  const nur_hand = [zuordnung({ gruppe: "team", von_hand: true })];
  it("warnt nur beim Entfernen der letzten wirksamen Gruppe", () => {
    expect(letzte_gruppe(nur_hand, "team")).toBe(true);
    expect(aussperr_warnung(nur_hand, true)).toContain("letzte Gruppe");
    // Für andere ist es keine Warnung, die man bestätigen muss.
    expect(aussperr_warnung(nur_hand, false)).toBe("");
  });

  it("bleibt wirksam, wenn Entra oder das Gerät sie auch gibt", () => {
    const beides = [zuordnung({ gruppe: "team", von_hand: true, aus_entra: true })];
    expect(letzte_gruppe(beides, "team")).toBe(false);
    expect(aussperr_warnung(beides, true)).toBe("");
    const am_geraet = [zuordnung({ gruppe: "team", von_hand: true, aus_geraet: true })];
    expect(letzte_gruppe(am_geraet, "team")).toBe(false);
  });

  it("waehnt nicht, wenn es noch eine zweite Gruppe gibt", () => {
    const zwei = [
      zuordnung({ gruppe: "team", von_hand: true }),
      zuordnung({ gruppe: "andere", aus_entra: true }),
    ];
    expect(letzte_gruppe(zwei, "team")).toBe(false);
    expect(aussperr_warnung(zwei, true)).toBe("");
  });

  it("warnt bei einer Entra-Gruppe nie", () => {
    const nur_entra = [zuordnung({ gruppe: "team", aus_entra: true })];
    expect(letzte_gruppe(nur_entra, "team")).toBe(false);
  });
});

describe("fehlende Gruppen", () => {
  it("lässt weg, was schon da ist, und sortiert nach Titel", () => {
    const gruppen: Gruppe[] = [
      { id: "z", titel: "Zebra", saetze: [], entra: [] },
      { id: "a", titel: "Alpha", saetze: [], entra: [] },
      { id: "m", titel: "Mitte", saetze: [], entra: [] },
    ];
    const vorhanden = [zuordnung({ gruppe: "m" })];
    expect(fehlende_gruppen(gruppen, vorhanden).map((g) => g.id)).toEqual(["a", "z"]);
  });
});

describe("Namen und Zeilen", () => {
  it("nimmt Name, dann E-Mail, dann Objekt-Id", () => {
    expect(mitglied_name({ oid: "o", name: "Alex", email: "a@example.test" })).toBe("Alex");
    expect(mitglied_name({ oid: "o", name: "  ", email: "a@example.test" })).toBe(
      "a@example.test",
    );
    expect(mitglied_name({ oid: "o", name: null, email: null })).toBe("o");
    expect(mitglied_zeile({ oid: "o", name: "Alex", email: "a@example.test" })).toBe(
      "a@example.test",
    );
    // Ohne Namen steht die Objekt-Id klein darunter.
    expect(mitglied_zeile({ oid: "o", name: null, email: null })).toBe("o");
  });

  it("zählt und beugt", () => {
    expect(mitglieder_text(1)).toBe("1 Mitglied");
    expect(mitglieder_text(0)).toBe("0 Mitglieder");
    expect(gruppen_text(2)).toBe("2 Gruppen");
    expect(gruppen_text(1)).toBe("1 Gruppe");
    expect(
      von_hand_anzahl([
        mitglied({ oid: "a", von_hand: true }),
        mitglied({ oid: "b", aus_entra: true }),
      ]),
    ).toBe(1);
  });
});

describe("Kandidaten", () => {
  const liste = [
    person({ oid: "a", name: "Alex Beispiel", email: "alex@example.test" }),
    person({ oid: "b", name: "Ben Beispiel", email: "ben@example.test" }),
    person({ oid: "root", name: "Root", rolle: "root" }),
    person({ oid: "c", name: null, email: null }),
  ];

  it("lässt Root und vorhandene Mitglieder weg", () => {
    const gefunden = kandidaten(liste, [mitglied({ oid: "a" })], "");
    expect(gefunden.map((eintrag) => eintrag.oid)).toEqual(["b", "c"]);
  });

  it("sucht in Name, E-Mail und Objekt-Id", () => {
    expect(kandidaten(liste, [], "ben@").map((e) => e.oid)).toEqual(["b"]);
    expect(kandidaten(liste, [], "BEN B").map((e) => e.oid)).toEqual(["b"]);
    expect(kandidaten(liste, [], "c").map((e) => e.oid)).toEqual(["c"]);
    expect(kandidaten(liste, [], "niemand")).toEqual([]);
  });
});

describe("Hinweise", () => {
  it("sagt, was aus Entra kommt und was gesperrt ist", () => {
    expect(entra_hinweis(mitglied({ oid: "a", von_hand: true }))).toBe("");
    expect(entra_hinweis(mitglied({ oid: "a", aus_entra: true }))).toContain("nur in Entra");
    expect(
      entra_hinweis(mitglied({ oid: "a", aus_entra: true, von_hand: true })),
    ).toContain("zusätzlich");
    expect(gesperrt_hinweis(mitglied({ oid: "a", gesperrt: true }))).toContain("wirkt aber nicht");
    expect(gesperrt_hinweis(mitglied({ oid: "a" }))).toBe("");
  });
});
