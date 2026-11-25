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
  hat_mehrfach,
  kennung_aus_titel,
  laufzeit_text,
  leere_anmeldung,
  leere_einrichtung,
  leeres_mehrfach,
  link_saeubern,
  mehrfach_ist_leer,
  mehrfach_von,
  neuer_eintrag,
  neues_feld,
  neues_zugang_feld,
  ist_fester_pfad,
  pfad_warnung,
  server_felder,
  server_werte,
  sortiert_nach_titel,
  umgebung_als_text,
  umgebung_aus_text,
  ZUGANG_QUELLEN,
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
        link: null,
      },
      {
        name: "passwort",
        beschreibung: "Kennwort",
        quelle: "geheim",
        pflicht: true,
        standard: null,
        link: null,
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

describe("Hilfe-Link", () => {
  it("nimmt Text und Adresse durch das Säubern", () => {
    const eintrag = beispiel();
    eintrag.felder[1].link = {
      text: "  Token in Outline erstellen  ",
      url: "  ${url}/settings/api-and-access  ",
    };
    expect(eintrag_saeubern(eintrag).felder[1].link).toEqual({
      text: "Token in Outline erstellen",
      url: "${url}/settings/api-and-access",
    });
  });

  it("macht aus zwei leeren Angaben keinen Link", () => {
    expect(link_saeubern({ text: "  ", url: "" })).toBeNull();
    expect(link_saeubern(null)).toBeNull();
    expect(link_saeubern(undefined)).toBeNull();
    // Ein neues Feld bringt keinen Link mit.
    expect(neues_feld().link).toBeNull();
    const eintrag = beispiel();
    eintrag.felder[0].link = { text: " ", url: "\n" };
    const sauber = eintrag_saeubern(eintrag);
    expect(sauber.felder[0].link).toBeNull();
    // Das ist gültiges JSON für `Option<FeldLink>`.
    expect(JSON.parse(JSON.stringify(sauber)).felder[0].link).toBeNull();
  });

  it("behält eine halbe Angabe, damit nichts unbemerkt verschwindet", () => {
    expect(link_saeubern({ text: "Token", url: " " })).toEqual({
      text: "Token",
      url: "",
    });
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

describe("fester Pfad im Befehl", () => {
  it("erkennt absolute Pfade und keine Namen", () => {
    expect(ist_fester_pfad("/usr/local/bin/quiver-devops")).toBe(true);
    expect(ist_fester_pfad("  /opt/quiver/quiver-jira  ")).toBe(true);
    expect(ist_fester_pfad("C:\\Programme\\mcp-quiver\\tools\\quiver-m365.exe")).toBe(
      true,
    );
    expect(ist_fester_pfad("C:/Programme/quiver.exe")).toBe(true);
    expect(ist_fester_pfad("\\\\server\\freigabe\\quiver.exe")).toBe(true);

    // Ein Name ist kein Pfad — und auch kein halber.
    expect(ist_fester_pfad("quiver-devops")).toBe(false);
    expect(ist_fester_pfad("npx")).toBe(false);
    expect(ist_fester_pfad("")).toBe(false);
    expect(ist_fester_pfad("node_modules/.bin/x")).toBe(false);
  });

  it("warnt beim Speichern, hält aber niemanden auf", () => {
    const warnung = pfad_warnung("/usr/local/bin/quiver-devops");
    expect(warnung).toContain("nur auf Geräten");
    expect(warnung).toContain("quiver-devops");
    expect(pfad_warnung("quiver-devops")).toBeNull();
    expect(pfad_warnung("")).toBeNull();

    // Der Eintrag bleibt gültig: die Warnung ist keine Fehlermeldung.
    const eintrag = beispiel();
    eintrag.laufzeit = {
      art: "stdio",
      befehl: "/usr/local/bin/postfach-mcp",
      argumente: [],
      umgebung: {},
    };
    expect(eintrag_pruefen(eintrag)).toEqual({});
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

  it("nimmt Art und Titel mit und leert leere Titel", () => {
    const eintrag = beispiel();
    eintrag.felder[0].art = {
      auswahl: { optionen: [{ wert: " rot ", anzeige: "  " }] },
    };
    eintrag.felder[0].titel = "  Konto  ";
    eintrag.felder[1].titel = "   ";
    const sauber = eintrag_saeubern(eintrag);
    expect(sauber.felder[0].art).toEqual({
      auswahl: { optionen: [{ wert: "rot", anzeige: "rot" }] },
    });
    expect(sauber.felder[0].titel).toBe("Konto");
    expect(sauber.felder[1].titel).toBeNull();
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

describe("Mehrere Zugänge", () => {
  /** Ein Eintrag mit Zugangsliste wie bei quiver-ssh. */
  function mit_zugaengen() {
    const eintrag = beispiel();
    eintrag.mehrfach = {
      titel: "Verbindungen",
      einzahl: "Verbindung",
      felder: [
        { ...neues_zugang_feld(), name: "benutzer", beschreibung: "Benutzer" },
        { ...neues_zugang_feld(), name: "host", pflicht: true },
        { ...neues_zugang_feld(), name: "passwort", quelle: "geheim" },
      ],
    };
    return eintrag;
  }

  it("beginnt mit Gerät-Feldern und ohne Server-Quelle", () => {
    const feld = neues_zugang_feld();
    expect(feld.quelle).toBe("client");
    expect(ZUGANG_QUELLEN).toEqual(["client", "geheim"]);
    // Das ist der Vertrag: der Client füllt die Liste, nicht der Admin.
    expect(ZUGANG_QUELLEN).not.toContain("server");
  });

  it("erkennt eine leere Zugangsliste", () => {
    expect(mehrfach_ist_leer(leeres_mehrfach())).toBe(true);
    expect(mehrfach_ist_leer(null)).toBe(true);
    expect(mehrfach_ist_leer(undefined)).toBe(true);
    expect(mehrfach_ist_leer(mit_zugaengen().mehrfach)).toBe(false);
    // Titel ohne Feld ist noch keine Liste.
    expect(
      mehrfach_ist_leer({ titel: "Verbindungen", einzahl: "Verbindung", felder: [] }),
    ).toBe(false);
    // Ältere Einträge kennen das Feld gar nicht.
    const alt = beispiel();
    delete alt.mehrfach;
    expect(mehrfach_ist_leer(alt.mehrfach)).toBe(true);
    expect(mehrfach_von(alt)).toEqual(leeres_mehrfach());
  });

  it("zeigt das Abzeichen nur mit Zugangsliste", () => {
    expect(hat_mehrfach(beispiel())).toBe(false);
    expect(hat_mehrfach(neuer_eintrag("neu"))).toBe(false);
    expect(hat_mehrfach(mit_zugaengen())).toBe(true);
  });

  it("verlangt Überschrift, Bezeichnung und ein Feld", () => {
    const eintrag = mit_zugaengen();
    expect(eintrag_pruefen(eintrag)).toEqual({});

    eintrag.mehrfach = { ...eintrag.mehrfach!, titel: "  " };
    expect(eintrag_pruefen(eintrag)["mehrfach.titel"]).toBeTruthy();

    eintrag.mehrfach = { ...mit_zugaengen().mehrfach!, einzahl: "" };
    expect(eintrag_pruefen(eintrag)["mehrfach.einzahl"]).toBeTruthy();

    eintrag.mehrfach = { ...mit_zugaengen().mehrfach!, felder: [] };
    expect(eintrag_pruefen(eintrag).mehrfach).toContain("mindestens ein Feld");
  });

  it("prüft die Felder der Zugangsliste", () => {
    const eintrag = mit_zugaengen();
    eintrag.mehrfach!.felder.push({ ...neues_zugang_feld(), name: "host" });
    eintrag.mehrfach!.felder.push({ ...neues_zugang_feld(), name: "mit leer" });
    const fehler = eintrag_pruefen(eintrag);
    expect(fehler["mehrfach.feld.3.name"]).toContain("schon");
    expect(fehler["mehrfach.feld.4.name"]).toContain("Buchstaben");
  });

  it("weist Server-Felder in der Zugangsliste ab", () => {
    const eintrag = mit_zugaengen();
    eintrag.mehrfach!.felder[0].quelle = "server";
    expect(eintrag_pruefen(eintrag)["mehrfach.feld.0.quelle"]).toContain("Gerät");
  });

  it("gibt eine Zugangsliste nur bei Stdio", () => {
    const eintrag = mit_zugaengen();
    eintrag.laufzeit = { art: "remote", url: "https://example.org", kopfzeilen: {} };
    expect(eintrag_pruefen(eintrag).mehrfach).toContain("Stdio");
  });

  it("säubert die Liste und leert den Abschnitt", () => {
    const eintrag = mit_zugaengen();
    eintrag.mehrfach!.titel = "  Verbindungen  ";
    eintrag.mehrfach!.einzahl = " Verbindung ";
    eintrag.mehrfach!.felder[1].name = " host ";
    eintrag.mehrfach!.felder[1].standard = "   ";
    const sauber = eintrag_saeubern(eintrag);
    expect(sauber.mehrfach?.titel).toBe("Verbindungen");
    expect(sauber.mehrfach?.einzahl).toBe("Verbindung");
    expect(sauber.mehrfach?.felder[1].name).toBe("host");
    expect(sauber.mehrfach?.felder[1].standard).toBeNull();

    // Ohne Titel, Bezeichnung und Felder steht nichts im Vertrag.
    eintrag.mehrfach = leeres_mehrfach();
    const leer = eintrag_saeubern(eintrag);
    expect(leer.mehrfach).toBeNull();
    // Das ist gültiges JSON für `Option<Mehrfach>`.
    expect(JSON.parse(JSON.stringify(leer)).mehrfach).toBeNull();
  });
});
