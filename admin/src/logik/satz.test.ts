import { describe, expect, it } from "vitest";

import type { Satz } from "../api/typen";
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
    expect(zweimal.eintraege).toEqual([{ eintrag: "mail", werte: {} }]);
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
