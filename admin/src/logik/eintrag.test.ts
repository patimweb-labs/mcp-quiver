import { describe, expect, it } from "vitest";

import type { Eintrag, Werte } from "../api/typen";
import {
  anmeldung_als_text,
  anmeldung_aus_text,
  anmeldung_ist_leer,
  anmeldung_von,
  argumente_als_text,
  argumente_aus_text,
  einrichtung_als_text,
  einrichtung_aus_text,
  einrichtung_ist_leer,
  einrichtung_von,
  eintrag_pruefen,
  eintrag_saeubern,
  filter_eintraege,
  gueltige_kennung,
  hat_anmeldung,
  hat_einrichtung,
  kennung_aus_titel,
  laufzeit_text,
  leere_anmeldung,
  leere_einrichtung,
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

describe("Anmeldung", () => {
  /** Ein Eintrag mit vollständiger Anmeldung. */
  function mit_anmeldung() {
    const eintrag = beispiel();
    eintrag.anmeldung = {
      anmelden: ["anmelden", "--konto", "${KONTO}"],
      status: ["status"],
      abmelden: ["abmelden"],
    };
    return eintrag;
  }

  it("schreibt Argumente durch Leerzeichen getrennt", () => {
    expect(anmeldung_als_text(["anmelden", "--konto"])).toBe(
      "anmelden --konto",
    );
    expect(anmeldung_als_text([])).toBe("");
    // Und zurück: mehrere Leerzeichen und Ränder fallen weg.
    expect(anmeldung_aus_text("  anmelden   --konto  ")).toEqual([
      "anmelden",
      "--konto",
    ]);
    expect(anmeldung_aus_text("")).toEqual([]);
    // Platzhalter bleiben ein Argument.
    expect(anmeldung_aus_text("anmelden ${KONTO}")).toEqual([
      "anmelden",
      "${KONTO}",
    ]);
  });

  it("erkennt eine leere Anmeldung", () => {
    expect(anmeldung_ist_leer(leere_anmeldung())).toBe(true);
    expect(anmeldung_ist_leer(null)).toBe(true);
    expect(anmeldung_ist_leer(undefined)).toBe(true);
    expect(anmeldung_ist_leer(mit_anmeldung().anmeldung)).toBe(false);
    // Ältere Einträge kennen das Feld gar nicht.
    const alt = beispiel();
    delete alt.anmeldung;
    expect(anmeldung_ist_leer(alt.anmeldung)).toBe(true);
    expect(anmeldung_von(alt)).toEqual(leere_anmeldung());
  });

  it("zeigt das Abzeichen nur mit Anmeldung", () => {
    expect(hat_anmeldung(beispiel())).toBe(false);
    expect(hat_anmeldung(neuer_eintrag("neu"))).toBe(false);
    expect(hat_anmeldung(mit_anmeldung())).toBe(true);
  });

  it("verlangt alle drei Befehle", () => {
    const eintrag = mit_anmeldung();
    expect(eintrag_pruefen(eintrag)).toEqual({});

    eintrag.anmeldung = { ...eintrag.anmeldung!, status: [] };
    expect(eintrag_pruefen(eintrag)).toHaveProperty("anmeldung.status");

    eintrag.anmeldung = leere_anmeldung();
    expect(Object.keys(eintrag_pruefen(eintrag))).toEqual(
      expect.arrayContaining(["anmeldung.anmelden", "anmeldung.abmelden"]),
    );
  });

  it("gibt eine Anmeldung nur bei Stdio", () => {
    const eintrag = beispiel();
    eintrag.anmeldung = mit_anmeldung().anmeldung;
    eintrag.laufzeit = { art: "remote", url: "https://example.org", kopfzeilen: {} };
    expect(eintrag_pruefen(eintrag).anmeldung).toContain("Stdio");
  });

  it("säubert die Argumente und leert den Abschnitt", () => {
    const eintrag = mit_anmeldung();
    eintrag.anmeldung = {
      anmelden: [" anmelden ", "", "  "],
      status: ["status"],
      abmelden: ["abmelden"],
    };
    const sauber = eintrag_saeubern(eintrag);
    expect(sauber.anmeldung?.anmelden).toEqual(["anmelden"]);

    // Alles leer: dann steht im Vertrag keine Anmeldung.
    eintrag.anmeldung = { anmelden: [" "], status: [], abmelden: [] };
    const leer = eintrag_saeubern(eintrag);
    expect(leer.anmeldung).toBeNull();
    // Das ist gültiges JSON für `Option<Anmeldung>`.
    expect(JSON.parse(JSON.stringify(leer)).anmeldung).toBeNull();
  });

  it("schreibt die Anmeldung unverändert weiter", () => {
    const sauber = eintrag_saeubern(mit_anmeldung());
    expect(sauber.anmeldung).toEqual({
      anmelden: ["anmelden", "--konto", "${KONTO}"],
      status: ["status"],
      abmelden: ["abmelden"],
    });
  });
});

describe("Einrichtung", () => {
  /** Ein Eintrag mit Einrichtung. */
  function mit_einrichtung() {
    const eintrag = beispiel();
    eintrag.einrichtung = { einrichten: ["einrichten", "${ABLAGE}"] };
    return eintrag;
  }

  it("schreibt Argumente durch Leerzeichen getrennt und liest sie zurück", () => {
    expect(einrichtung_als_text(["einrichten", "${ABLAGE}"])).toBe(
      "einrichten ${ABLAGE}",
    );
    expect(einrichtung_als_text([])).toBe("");
    expect(einrichtung_aus_text("  einrichten   --still ")).toEqual([
      "einrichten",
      "--still",
    ]);
    expect(einrichtung_aus_text("")).toEqual([]);
    // Platzhalter bleiben ein Argument.
    expect(einrichtung_aus_text("einrichten ${ABLAGE}")).toEqual([
      "einrichten",
      "${ABLAGE}",
    ]);
  });

  it("erkennt eine leere Einrichtung", () => {
    expect(einrichtung_ist_leer(leere_einrichtung())).toBe(true);
    expect(einrichtung_ist_leer(null)).toBe(true);
    expect(einrichtung_ist_leer(undefined)).toBe(true);
    expect(einrichtung_ist_leer(mit_einrichtung().einrichtung)).toBe(false);
    // Ältere Einträge kennen das Feld gar nicht.
    const alt = beispiel();
    delete alt.einrichtung;
    expect(einrichtung_ist_leer(alt.einrichtung)).toBe(true);
    expect(einrichtung_von(alt)).toEqual(leere_einrichtung());
  });

  it("zeigt das Abzeichen nur mit Einrichtung", () => {
    expect(hat_einrichtung(beispiel())).toBe(false);
    expect(hat_einrichtung(neuer_eintrag("neu"))).toBe(false);
    expect(hat_einrichtung(mit_einrichtung())).toBe(true);
  });

  it("verlangt einen Befehl und nur bei Stdio", () => {
    const eintrag = mit_einrichtung();
    expect(eintrag_pruefen(eintrag)).toEqual({});

    eintrag.einrichtung = leere_einrichtung();
    expect(eintrag_pruefen(eintrag)).toHaveProperty("einrichtung.einrichten");

    eintrag.einrichtung = mit_einrichtung().einrichtung;
    eintrag.laufzeit = { art: "remote", url: "https://example.org", kopfzeilen: {} };
    expect(eintrag_pruefen(eintrag).einrichtung).toContain("Stdio");
  });

  it("säubert die Argumente und leert den Abschnitt", () => {
    const eintrag = mit_einrichtung();
    eintrag.einrichtung = { einrichten: [" einrichten ", "", "  "] };
    expect(eintrag_saeubern(eintrag).einrichtung).toEqual({
      einrichten: ["einrichten"],
    });

    // Alles leer: dann steht im Vertrag keine Einrichtung.
    eintrag.einrichtung = { einrichten: [" "] };
    const leer = eintrag_saeubern(eintrag);
    expect(leer.einrichtung).toBeNull();
    // Das ist gültiges JSON für `Option<Einrichtung>`.
    expect(JSON.parse(JSON.stringify(leer)).einrichtung).toBeNull();
  });
});
