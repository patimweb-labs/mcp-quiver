import { describe, expect, it } from "vitest";

import type { Feld } from "../api/typen";
import {
  FELD_ARTEN,
  feld_art,
  feld_art_aus_name,
  feld_art_name,
  feld_art_text,
  feld_optionen,
  feld_sperrgrund,
  feld_titel,
  feld_wert,
  ist_schalter,
  optionen_als_text,
  optionen_aus_text,
  schalter_an,
  schalter_wert,
  wert_fehler,
} from "./feld";

function feld(teil: Partial<Feld> = {}): Feld {
  return {
    name: "eingabe",
    beschreibung: "",
    quelle: "client",
    pflicht: false,
    standard: null,
    link: null,
    art: "text",
    titel: null,
    ...teil,
  };
}

describe("Beschriftung und Art", () => {
  it("nimmt den Titel, sonst den Namen", () => {
    expect(feld_titel(feld({ name: "benutzer" }))).toBe("benutzer");
    expect(feld_titel(feld({ name: "benutzer", titel: "SAP-Benutzer" }))).toBe(
      "SAP-Benutzer",
    );
    expect(feld_titel(feld({ name: "benutzer", titel: "   " }))).toBe("benutzer");
  });

  it("lässt alte Daten ohne Art als Text gelten", () => {
    expect(feld_art(feld({ art: undefined }))).toBe("text");
  });

  it("behandelt ein Geheimnis immer als Passwort", () => {
    expect(feld_art(feld({ quelle: "geheim", art: "schalter" }))).toBe(
      "passwort",
    );
  });

  it("schaltet die Art um und behält Auswahl-Optionen", () => {
    const auswahl = feld_art_aus_name("auswahl", "text");
    expect(auswahl).toEqual({ auswahl: { optionen: [] } });
    const mitOptionen = {
      auswahl: { optionen: [{ wert: "rot", anzeige: "Rot" }] },
    };
    expect(feld_art_aus_name("auswahl", mitOptionen)).toBe(mitOptionen);
    expect(feld_art_aus_name("zahl", mitOptionen)).toBe("zahl");
    expect(feld_art_aus_name("unbekannt", "text")).toBe("text");
  });

  it("kennt Namen und Worte zu jeder Art", () => {
    expect(FELD_ARTEN.map((art) => art.value)).toEqual([
      "text",
      "passwort",
      "zahl",
      "schalter",
      "url",
      "auswahl",
    ]);
    expect(feld_art_name({ auswahl: { optionen: [] } })).toBe("auswahl");
    expect(feld_art_text("schalter")).toBe("Schalter");
  });
});

describe("Schalter", () => {
  it("erkennt an und aus", () => {
    expect(schalter_an("an")).toBe(true);
    expect(schalter_an(" AN ")).toBe(true);
    expect(schalter_an("aus")).toBe(false);
    expect(ist_schalter(feld({ art: "schalter" }))).toBe(true);
  });

  it("nimmt den Standard, wenn nichts gespeichert ist", () => {
    expect(schalter_wert("", "an")).toBe("an");
    expect(schalter_wert("", null)).toBe("aus");
    expect(schalter_wert("an", "aus")).toBe("an");
    expect(schalter_wert("aus", "an")).toBe("aus");
  });

  it("liefert den passenden Formularwert", () => {
    expect(feld_wert(feld({ name: "pruefen", art: "schalter", standard: "an" }), {})).toBe(
      "an",
    );
    expect(feld_wert(feld({ name: "pfad" }), { pfad: "/tmp" })).toBe("/tmp");
  });
});

describe("Prüfung der Werte", () => {
  it("nimmt nur Ziffern als Zahl", () => {
    expect(wert_fehler("zahl", "4711")).toBeNull();
    expect(wert_fehler("zahl", "47a1")).toBe("zahl");
  });

  it("verlangt http(s) für eine Adresse", () => {
    expect(wert_fehler("url", "https://sap.example")).toBeNull();
    expect(wert_fehler("url", "ftp://sap.example")).toBe("adresse");
  });
});

describe("Sperrgrund neben Speichern", () => {
  const benutzer = feld({
    name: "benutzer",
    pflicht: true,
    titel: "SAP-Benutzer",
  });
  const adresse = feld({
    name: "url",
    pflicht: true,
    art: "url",
    titel: "ADT-Adresse",
  });

  it("nennt die Titel leerer Pflichtfelder", () => {
    expect(feld_sperrgrund([benutzer, adresse], {})).toBe(
      "Es fehlt: SAP-Benutzer, ADT-Adresse",
    );
  });

  it("lässt leere, nicht verpflichtende Felder zu", () => {
    expect(
      feld_sperrgrund([feld({ name: "hinweis", pflicht: false })], {}),
    ).toBeNull();
  });

  it("nennt eine ungültige Zahl und eine ungültige Adresse", () => {
    const zahl = feld({ name: "mandant", art: "zahl", titel: "Mandant" });
    expect(
      feld_sperrgrund([zahl, adresse], { mandant: "12a", url: "/adt" }),
    ).toBe("Ungültige Zahl: Mandant · Ungültige Adresse: ADT-Adresse");
  });

  it("prüft den Standard eines Schalters, nicht ein leeres Feld", () => {
    const schalter = feld({ name: "pruefen", art: "schalter", standard: "an" });
    expect(feld_sperrgrund([schalter], {})).toBeNull();
  });

  it("prüft Geheimnisse nie", () => {
    expect(
      feld_sperrgrund([feld({ name: "token", quelle: "geheim", pflicht: true })], {}),
    ).toBeNull();
  });
});

describe("Optionen einer Auswahl", () => {
  it("schreibt und liest wert und Anzeige", () => {
    const optionen = [
      { wert: "rot", anzeige: "Rot" },
      { wert: "blau", anzeige: "Blau" },
    ];
    expect(optionen_als_text(optionen)).toBe("rot | Rot\nblau | Blau");
    expect(optionen_aus_text("rot | Rot\nblau | Blau")).toEqual(optionen);
  });

  it("nimmt ohne Trenner den Wert als Anzeige", () => {
    expect(optionen_aus_text("rot\nblau |  Blau ")).toEqual([
      { wert: "rot", anzeige: "rot" },
      { wert: "blau", anzeige: "Blau" },
    ]);
  });

  it("verwirft leere Zeilen und leere Werte", () => {
    expect(optionen_aus_text("\n  \n| ohne Wert\nrot")).toEqual([
      { wert: "rot", anzeige: "rot" },
    ]);
  });

  it("findet die Optionen eines Felds", () => {
    const auswahl = feld({
      art: { auswahl: { optionen: [{ wert: "rot", anzeige: "Rot" }] } },
    });
    expect(feld_optionen(auswahl)).toEqual([{ wert: "rot", anzeige: "Rot" }]);
    expect(feld_optionen(feld({ art: "text" }))).toEqual([]);
  });
});
