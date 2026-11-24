import { describe, expect, it } from "vitest";

import type { PruefspurEintrag } from "../api/typen";
import {
  ARTEN,
  OHNE_FILTER,
  PRUEFSPUR_ERSTE,
  PRUEFSPUR_SCHRITT,
  anfrage_limit,
  art_ton,
  art_titel,
  arten_im_bestand,
  filtern,
  hat_mehr,
  naechstes_limit,
  sichtbar,
} from "./pruefspur";

function eintrag(teil: Partial<PruefspurEintrag> = {}): PruefspurEintrag {
  return {
    zeit: "2026-11-24T10:00:00Z",
    art: "anmeldung",
    akteur: "anna@example.test",
    text: "Entra",
    anzahl: 1,
    ...teil,
  };
}

describe("Art der Prüfspur", () => {
  it("benennt die Arten des Servers", () => {
    const arten = ARTEN.map((eintrag) => eintrag.wert);
    for (const wert of [
      "verwaltung",
      "anmeldung",
      "fehlschlag",
      "rolle",
      "aenderung",
      "geraet",
      "nachpruefen",
      "einrichtung",
      "kommandozeile",
    ]) {
      expect(arten).toContain(wert);
    }
    expect(art_titel("rolle")).toBe("Rolle");
    expect(art_titel("verwaltung")).toBe("Verwaltung");
    expect(art_ton("fehlschlag")).toBe("warnung");
  });

  it("lässt eine unbekannte Art sichtbar", () => {
    expect(art_titel("von-morgen")).toBe("von-morgen");
    expect(art_ton("von-morgen")).toBe("blass");
  });

  it("bietet nur die Arten an, die im Bestand vorkommen", () => {
    const liste = [
      eintrag({ art: "anmeldung" }),
      eintrag({ art: "rolle" }),
      eintrag({ art: "rolle" }),
    ];
    expect(arten_im_bestand(liste).map((eintrag) => eintrag.wert)).toEqual([
      "anmeldung",
      "rolle",
    ]);
  });
});

describe("Filtern", () => {
  const liste = [
    eintrag({ art: "anmeldung", akteur: "root@example.test", text: "Passwort" }),
    eintrag({ art: "rolle", akteur: "admin@example.test", text: "Rolle admin" }),
    eintrag({ art: "fehlschlag", akteur: "unbekannt", text: "Root-Anmeldung" }),
  ];

  it("filtert nach Art", () => {
    expect(filtern(liste, { ...OHNE_FILTER, art: "rolle" })).toHaveLength(1);
    expect(filtern(liste, { ...OHNE_FILTER, art: "alle" })).toHaveLength(3);
  });

  it("sucht in Akteur, Text und Art", () => {
    expect(filtern(liste, { ...OHNE_FILTER, suche: "admin@example" })).toHaveLength(
      1,
    );
    expect(filtern(liste, { ...OHNE_FILTER, suche: "fehlschlag" })).toHaveLength(
      1,
    );
    expect(filtern(liste, { ...OHNE_FILTER, suche: "nichts" })).toHaveLength(0);
  });

  it("kombiniert Suche und Art", () => {
    expect(
      filtern(liste, { suche: "Rolle", art: "rolle" }),
    ).toHaveLength(1);
    expect(
      filtern(liste, { suche: "Rolle", art: "anmeldung" }),
    ).toHaveLength(0);
  });
});

describe("Nachladen", () => {
  it("fragt eine Zeile mehr an, als gezeigt wird", () => {
    expect(anfrage_limit(PRUEFSPUR_ERSTE)).toBe(PRUEFSPUR_ERSTE + 1);
    expect(naechstes_limit(PRUEFSPUR_ERSTE)).toBe(
      PRUEFSPUR_ERSTE + PRUEFSPUR_SCHRITT,
    );
  });

  it("erkennt, ob es weitere Einträge gibt", () => {
    expect(hat_mehr(21, 20)).toBe(true);
    expect(hat_mehr(20, 20)).toBe(false);
    expect(hat_mehr(0, 20)).toBe(false);
  });

  it("zeigt nur das Limit, nicht die Probezeile", () => {
    const geladen = Array.from({ length: 21 }, (_, i) => eintrag({ text: `${i}` }));
    expect(sichtbar(geladen, 20)).toHaveLength(20);
    expect(sichtbar(geladen, 20)[19].text).toBe("19");
  });
});
