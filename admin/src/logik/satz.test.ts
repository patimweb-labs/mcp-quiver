import { describe, expect, it } from "vitest";

import type { Feld, FeldArt, Satz, SatzEintrag } from "../api/typen";
import {
  gruppe_pruefen,
  gruppe_saeubern,
  gruppe_satz_setzen,
  neue_gruppe,
  neuer_satz,
  satz_eintrag_entfernen,
  satz_eintrag_hinzufuegen,
  satz_saeubern,
  satz_pruefen,
  satz_wert_setzen,
  satz_wird_benutzt,
  feste_felder,
  vorgabe_setzen,
  vorgabe_von,
  saetze_der_gruppe,
  saetze_sortiert,
} from "./satz";

describe("Vorlagen", () => {
  it("legt leere Sätze und Gruppen an", () => {
    expect(neuer_satz("basis")).toEqual({
      id: "basis",
      titel: "",
      prioritaet: 0,
      eintraege: [],
    });
    expect(neue_gruppe("team")).toEqual({
      id: "team",
      titel: "",
      saetze: [],
      entra: [],
    });
  });
});

describe("Prüfung", () => {
  it("nimmt vollständige Sätze an", () => {
    const satz: Satz = { ...neuer_satz("basis"), titel: "Basis" };
    expect(satz_pruefen(satz)).toEqual({});
  });

  it("meldet Kennung, Titel und Priorität", () => {
    const fehler = satz_pruefen({ ...neuer_satz("Mit Leerzeichen"), prioritaet: 1.5 });
    expect(fehler.id).toBeTruthy();
    expect(fehler.titel).toBeTruthy();
    expect(fehler.prioritaet).toBeTruthy();
  });

  it("prüft Gruppen", () => {
    const fehler = gruppe_pruefen({ ...neue_gruppe("team"), titel: "Team" });
    expect(fehler).toEqual({});
    expect(gruppe_pruefen(neue_gruppe()).titel).toBeTruthy();
  });
});

describe("Säubern", () => {
  it("nimmt Ränder an Kennung und Titel weg", () => {
    const sauber = satz_saeubern({ ...neuer_satz(" basis "), titel: " Basis " });
    expect(sauber.id).toBe("basis");
    expect(sauber.titel).toBe("Basis");
    expect(gruppe_saeubern({ ...neue_gruppe(" team "), titel: " Team " }).id).toBe(
      "team",
    );
  });
});

describe("Einträge im Satz", () => {
  it("nimmt einen Eintrag einmal auf", () => {
    const einmal = satz_eintrag_hinzufuegen(neuer_satz("basis"), "mail");
    const zweimal = satz_eintrag_hinzufuegen(einmal, "mail");
    expect(zweimal.eintraege).toEqual([{ eintrag: "mail", werte: {}, fest: [] }]);
    expect(zweimal).toBe(einmal);
  });

  it("entfernt ihn wieder", () => {
    const satz = satz_eintrag_hinzufuegen(neuer_satz("basis"), "mail");
    expect(satz_eintrag_entfernen(satz, "mail").eintraege).toEqual([]);
  });

  it("setzt Werte und löscht sie mit leerem Text", () => {
    let satz = satz_eintrag_hinzufuegen(neuer_satz("basis"), "mail");
    satz = satz_wert_setzen(satz, "mail", "konto", "ich@example.org");
    expect(satz.eintraege[0].werte).toEqual({ konto: "ich@example.org" });
    satz = satz_wert_setzen(satz, "mail", "konto", "");
    expect(satz.eintraege[0].werte).toEqual({});
  });

  it("rührt andere Einträge nicht an", () => {
    let satz = satz_eintrag_hinzufuegen(neuer_satz("basis"), "mail");
    satz = satz_eintrag_hinzufuegen(satz, "chat");
    const neu = satz_wert_setzen(satz, "chat", "raum", "allgemein");
    expect(neu.eintraege[0]).toBe(satz.eintraege[0]);
    expect(neu.eintraege[0].werte).toEqual({});
  });
});

describe("Reihenfolge", () => {
  const hoch: Satz = { ...neuer_satz("hoch"), titel: "Hoch", prioritaet: 10 };
  const tief: Satz = { ...neuer_satz("tief"), titel: "Alpha", prioritaet: -1 };
  const mitte: Satz = { ...neuer_satz("mitte"), titel: "Mitte", prioritaet: 10 };

  it("sortiert nach Priorität, dann nach Titel", () => {
    expect(saetze_sortiert([tief, hoch, mitte]).map((satz) => satz.id)).toEqual([
      "hoch",
      "mitte",
      "tief",
    ]);
  });

  it("wählt die Sätze einer Gruppe in dieser Reihenfolge", () => {
    const gruppe = { ...neue_gruppe("team"), saetze: ["tief", "hoch", "fehlt"] };
    expect(saetze_der_gruppe(gruppe, [hoch, tief]).map((satz) => satz.id)).toEqual([
      "hoch",
      "tief",
    ]);
  });
});

describe("Zuordnung", () => {
  it("setzt und entfernt Sätze an einer Gruppe", () => {
    let gruppe = gruppe_satz_setzen(neue_gruppe("team"), "basis", true);
    expect(gruppe.saetze).toEqual(["basis"]);
    expect(gruppe_satz_setzen(gruppe, "basis", true)).toBe(gruppe);
    gruppe = gruppe_satz_setzen(gruppe, "basis", false);
    expect(gruppe.saetze).toEqual([]);
  });

  it("zählt, wie viele Gruppen einen Satz benutzen", () => {
    const gruppen = [
      { ...neue_gruppe("a"), saetze: ["basis"] },
      { ...neue_gruppe("b"), saetze: ["basis", "extra"] },
      { ...neue_gruppe("c"), saetze: [] },
    ];
    expect(satz_wird_benutzt("basis", gruppen)).toBe(2);
    expect(satz_wird_benutzt("extra", gruppen)).toBe(1);
    expect(satz_wird_benutzt("fehlt", gruppen)).toBe(0);
  });
});

describe("Vorbelegen (Aufgabe 36)", () => {
  const feld = (name: string, art: FeldArt = "text"): Feld => ({
    name,
    beschreibung: "",
    quelle: "client",
    pflicht: true,
    standard: null,
    link: null,
    art,
  });

  const satz_mit = (): Satz => satz_eintrag_hinzufuegen(neuer_satz("basis"), "mail");

  it("behält nur Vorgaben mit Wert", () => {
    let satz = satz_wert_setzen(satz_mit(), "mail", "adresse", "https://x");
    satz = vorgabe_setzen(satz, "mail", feld("adresse"), "fest");
    satz.eintraege[0].fest = [...feste_felder(satz.eintraege[0]), "ohne-wert"];
    const sauber = satz_saeubern(satz);
    expect(sauber.eintraege[0].fest).toEqual(["adresse"]);
  });

  it("kennt drei Zustände: keine Vorgabe, Vorschlag, fest", () => {
    let satz = satz_mit();
    const adresse = feld("adresse");
    expect(vorgabe_von(satz.eintraege[0], "adresse")).toBeNull();
    satz = satz_wert_setzen(satz, "mail", "adresse", "https://x");
    expect(vorgabe_von(satz.eintraege[0], "adresse")).toBe("vorschlag");
    satz = vorgabe_setzen(satz, "mail", adresse, "fest");
    expect(vorgabe_von(satz.eintraege[0], "adresse")).toBe("fest");
    // Zurück auf Vorschlag: der Wert bleibt.
    satz = vorgabe_setzen(satz, "mail", adresse, "vorschlag");
    expect(vorgabe_von(satz.eintraege[0], "adresse")).toBe("vorschlag");
    expect(satz.eintraege[0].werte).toEqual({ adresse: "https://x" });
    // Ein leerer Wert löscht Wert und Festlegung.
    satz = satz_wert_setzen(satz, "mail", "adresse", "");
    expect(satz.eintraege[0].werte).toEqual({});
    expect(feste_felder(satz.eintraege[0])).toEqual([]);
  });

  it("lässt ein leeres Textfeld ohne Vorgabe", () => {
    const satz = satz_mit();
    const neu = vorgabe_setzen(satz, "mail", feld("adresse"), "fest");
    expect(neu.eintraege[0]).toEqual({ eintrag: "mail", werte: {}, fest: [] });
  });

  it("belegt einen Schalter nur mit seiner sicheren Stellung vor", () => {
    const schreiben = feld("schreiben", "schalter");
    let satz = satz_mit();
    // „Fest“ trägt die sichere Stellung ein – verbieten ist erlaubt.
    satz = vorgabe_setzen(satz, "mail", schreiben, "fest");
    expect(satz.eintraege[0].werte).toEqual({ schreiben: "aus" });
    expect(vorgabe_von(satz.eintraege[0], "schreiben")).toBe("fest");
    // Einschalten ist keine Vorgabe: Wert und Festlegung fallen weg.
    satz = satz_wert_setzen(satz, "mail", "schreiben", "an", schreiben);
    expect(satz.eintraege[0].werte).toEqual({});
    expect(vorgabe_von(satz.eintraege[0], "schreiben")).toBeNull();
    // Ein Schalter, der standardmäßig an ist, wird nur „an“ vorbelegt.
    const pruefen = { ...feld("pruefen", "schalter"), standard: "an" };
    satz = satz_wert_setzen(satz, "mail", "pruefen", "aus", pruefen);
    expect(satz.eintraege[0].werte).toEqual({});
    satz = vorgabe_setzen(satz, "mail", pruefen, "fest");
    expect(satz.eintraege[0].werte).toEqual({ pruefen: "an" });
  });

  it("rührt andere Einträge nicht an", () => {
    let satz = satz_eintrag_hinzufuegen(neuer_satz("basis"), "mail");
    satz = satz_eintrag_hinzufuegen(satz, "chat");
    const neu = vorgabe_setzen(satz, "chat", feld("raum"), "fest");
    expect(neu.eintraege[0]).toBe(satz.eintraege[0]);
    const mit_wert = satz_wert_setzen(satz, "chat", "raum", "allgemein");
    expect(mit_wert.eintraege[0]).toBe(satz.eintraege[0]);
  });

  it("kommt mit älteren Daten ohne fest zurecht", () => {
    const alt: SatzEintrag = { eintrag: "mail", werte: { adresse: "https://x" } };
    expect(feste_felder(alt)).toEqual([]);
    expect(vorgabe_von(alt, "adresse")).toBe("vorschlag");
  });
});
