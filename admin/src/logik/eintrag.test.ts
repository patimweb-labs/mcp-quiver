import { describe, expect, it } from "vitest";

import type { Eintrag, Werte } from "../api/typen";
import {
  argumente_als_text,
  argumente_aus_text,
  eintrag_pruefen,
  eintrag_saeubern,
  filter_eintraege,
  gueltige_kennung,
  kennung_aus_titel,
  laufzeit_text,
  neuer_eintrag,
  neues_feld,
  server_felder,
  server_werte,
  sortiert_nach_titel,
  umgebung_als_text,
  umgebung_aus_text,
} from "./eintrag";

function beispiel(): Eintrag {
  return {
    id: "postfach",
    version: "1.0.0",
    titel: "Postfach",
    beschreibung: "Mail lesen",
    herkunft: "eigen",
    laufzeit: {
      art: "stdio",
      befehl: "postfach-mcp",
      argumente: ["--still"],
      umgebung: { PFAD: "/tmp", ART: "abruf" },
    },
    felder: [
      {
        name: "konto",
        beschreibung: "Postfach",
        quelle: "server",
        pflicht: true,
        standard: null,
      },
      {
        name: "passwort",
        beschreibung: "Kennwort",
        quelle: "geheim",
        pflicht: true,
        standard: null,
      },
    ],
    aktiv: true,
  };
}

describe("kennung_aus_titel", () => {
  it("bildet eine kleingeschriebene Kennung", () => {
    expect(kennung_aus_titel("Postfach Abruf")).toBe("postfach-abruf");
  });

  it("löst Umlaute und ß auf", () => {
    expect(kennung_aus_titel("Größe & Maße")).toBe("groesse-masse");
    expect(kennung_aus_titel("Überweisung")).toBe("ueberweisung");
  });

  it("lässt keine Ränder stehen", () => {
    expect(kennung_aus_titel("  ...Microsoft 365!  ")).toBe("microsoft-365");
    expect(kennung_aus_titel("")).toBe("");
  });
});

describe("gueltige_kennung", () => {
  it("nimmt kleine Kennungen mit Bindestrichen", () => {
    expect(gueltige_kennung("microsoft-365")).toBe(true);
    expect(gueltige_kennung("a")).toBe(true);
  });

  it("weist Großschreibung, Leerzeichen und Ränder ab", () => {
    expect(gueltige_kennung("Microsoft")).toBe(false);
    expect(gueltige_kennung("mit leer")).toBe(false);
    expect(gueltige_kennung("-vorne")).toBe(false);
    expect(gueltige_kennung("hinten-")).toBe(false);
    expect(gueltige_kennung("")).toBe(false);
  });
});

describe("Argumente und Umgebung als Text", () => {
  it("schreibt je Zeile ein Argument und liest sie zurück", () => {
    const text = argumente_als_text(["--still", "--pfad", "/tmp"]);
    expect(argumente_aus_text(text)).toEqual(["--still", "--pfad", "/tmp"]);
  });

  it("verwirft leere Zeilen", () => {
    expect(argumente_aus_text("a\n\n  \nb")).toEqual(["a", "b"]);
  });

  it("sortiert die Umgebung, damit sie stabil bleibt", () => {
    expect(umgebung_als_text({ ZWEI: "2", EINS: "1" })).toBe("EINS=1\nZWEI=2");
  });

  it("liest die Umgebung zurück und verwirft Unbrauchbares", () => {
    const werte: Werte = umgebung_aus_text(
      "A=1\n# Kommentar\nohne Gleichheitszeichen\n=leer\nB = zwei = drei\n",
    );
    expect(werte).toEqual({ A: "1", B: "zwei = drei" });
  });

  it("läuft einmal im Kreis", () => {
    const werte = { PFAD: "/tmp", ART: "abruf" };
    expect(umgebung_aus_text(umgebung_als_text(werte))).toEqual(werte);
  });
});

describe("Server-Felder", () => {
  it("wählt nur, was der Admin füllt", () => {
    const eintrag = beispiel();
    expect(server_felder(eintrag).map((feld) => feld.name)).toEqual(["konto"]);
  });

  it("übernimmt nur Server-Werte in einen Satz", () => {
    expect(
      server_werte(beispiel(), {
        konto: "ich@example.org",
        passwort: "geheim",
        unbekannt: "x",
      }),
    ).toEqual({ konto: "ich@example.org" });
  });
});

describe("eintrag_pruefen", () => {
  it("nimmt einen vollständigen Eintrag an", () => {
    expect(eintrag_pruefen(beispiel())).toEqual({});
  });

  it("meldet fehlende Pflichtangaben", () => {
    const eintrag = neuer_eintrag();
    const fehler = eintrag_pruefen(eintrag);
    expect(fehler.id).toBeTruthy();
    expect(fehler.titel).toBeTruthy();
    expect(fehler.befehl).toBeTruthy();
  });

  it("prüft die Adresse eines entfernten Servers", () => {
    const eintrag = neuer_eintrag("mail");
    eintrag.laufzeit = { art: "remote", url: "ftp://example.org", kopfzeilen: {} };
    expect(eintrag_pruefen(eintrag).url).toContain("http");
    eintrag.laufzeit.url = "https://example.org/mcp";
    expect(eintrag_pruefen(eintrag).url).toBeUndefined();
  });

  it("meldet doppelte und unbrauchbare Feldnamen", () => {
    const eintrag = beispiel();
    eintrag.felder.push({ ...neues_feld(), name: "konto" });
    eintrag.felder.push({ ...neues_feld(), name: "mit leer" });
    const fehler = eintrag_pruefen(eintrag);
    expect(fehler["feld.2.name"]).toContain("schon");
    expect(fehler["feld.3.name"]).toContain("Buchstaben");
  });
});

describe("Laufzeit in Worten", () => {
  it("nennt Befehl oder Adresse", () => {
    expect(laufzeit_text(beispiel().laufzeit)).toBe("Stdio · postfach-mcp");
    expect(
      laufzeit_text({ art: "remote", url: "https://example.org", kopfzeilen: {} }),
    ).toBe("Remote · https://example.org");
    expect(laufzeit_text({ art: "stdio", befehl: "", argumente: [], umgebung: {} })).toBe(
      "Stdio",
    );
    expect(laufzeit_text({ art: "remote", url: "", kopfzeilen: {} })).toBe(
      "Remote",
    );
  });
});

describe("eintrag_saeubern", () => {
  it("nimmt Ränder weg und leert leere Standardwerte", () => {
    const eintrag = beispiel();
    eintrag.id = "  postfach  ";
    eintrag.titel = "  Postfach  ";
    eintrag.version = "  1.0.0 ";
    eintrag.laufzeit = {
      art: "stdio",
      befehl: "  postfach-mcp  ",
      argumente: [],
      umgebung: {},
    };
    eintrag.felder[0].name = "  konto  ";
    eintrag.felder[0].standard = "   ";
    const sauber = eintrag_saeubern(eintrag);
    expect(sauber.id).toBe("postfach");
    expect(sauber.titel).toBe("Postfach");
    expect(sauber.version).toBe("1.0.0");
    if (sauber.laufzeit.art === "stdio") {
      expect(sauber.laufzeit.befehl).toBe("postfach-mcp");
    }
    expect(sauber.felder[0].name).toBe("konto");
    expect(sauber.felder[0].standard).toBeNull();
  });
});

describe("Listen", () => {
  const a = { ...beispiel(), id: "a", titel: "Zebra" };
  const b = { ...beispiel(), id: "b", titel: "Alpha" };

  it("sortiert ohne Rücksicht auf Groß- und Kleinschreibung", () => {
    expect(sortiert_nach_titel([a, b]).map((eintrag) => eintrag.id)).toEqual([
      "b",
      "a",
    ]);
  });

  it("sucht in Kennung, Titel und Beschreibung", () => {
    expect(filter_eintraege([a, b], "mail").map((eintrag) => eintrag.id)).toEqual([
      "a",
      "b",
    ]);
    expect(filter_eintraege([a, b], "AlPhA").map((eintrag) => eintrag.id)).toEqual(
      ["b"],
    );
    expect(filter_eintraege([a, b], "   ")).toHaveLength(2);
    expect(filter_eintraege([a, b], "nichts")).toHaveLength(0);
  });
});
