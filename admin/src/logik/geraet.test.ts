import { describe, expect, it } from "vitest";

import type { EintragZustand, GeraetStand, Gruppe } from "../api/typen";
import {
  anzahl_mit_problemen,
  besitzer_hinweis,
  besitzer_text,
  geraete_mit_problemen,
  gruppen_namen,
  hat_problem,
  problem_texte,
  revision_kurz,
  stand_text,
  stand_ton,
  wird_aktualisiert,
  zustand_text,
  zustand_ton,
} from "./geraet";

function stand(teil: Partial<GeraetStand> = {}): GeraetStand {
  return {
    geraet: {
      id: "g1",
      name: "Büro",
      gruppen: ["team"],
      gesperrt: false,
    },
    soll_revision: "aaaa",
    meldung: {
      revision: "aaaa",
      client_version: "0.1.0",
      eintraege: [],
    },
    zuletzt_gesehen: null,
    verbunden: true,
    ...teil,
  };
}

describe("Eintrag-Zustand", () => {
  it("gibt jedem Zustand eine Bedeutung", () => {
    const faelle: [EintragZustand, string, string][] = [
      [{ zustand: "laeuft", tools: 1 }, "gut", "Läuft · 1 Tool"],
      [{ zustand: "laeuft", tools: 3 }, "gut", "Läuft · 3 Tools"],
      [
        { zustand: "wartet-auf-werte", felder: ["pfad"] },
        "warnung",
        "Braucht Angaben: pfad",
      ],
      [
        { zustand: "wartet-auf-werte", felder: [] },
        "warnung",
        "Braucht Angaben",
      ],
      [{ zustand: "startet" }, "leise", "Startet …"],
      // Nachtrag zu Aufgabe 37: dieselben Wörter wie auf der Karte im Client.
      [{ zustand: "wird-geladen" }, "leise", "Wird vom Server geladen …"],
      [{ zustand: "aktualisiert" }, "leise", "Aktualisiert"],
      [{ zustand: "fehler", text: "kaputt" }, "fehler", "Fehler: kaputt"],
    ];
    for (const [zustand, ton, text] of faelle) {
      expect(zustand_ton(zustand)).toBe(ton);
      expect(zustand_text(zustand)).toBe(text);
    }
  });

  it("zeigt „am Gerät abgeschaltet“ neutral, nicht als Problem", () => {
    const aus: EintragZustand = { zustand: "am-geraet-aus" };
    expect(zustand_ton(aus)).toBe("leise");
    expect(zustand_text(aus)).toBe("am Gerät abgeschaltet");
    const still = stand({
      meldung: {
        revision: "aaaa",
        client_version: "0.1.0",
        eintraege: [["mail", aus]],
      },
    });
    expect(hat_problem(still)).toBe(false);
    expect(problem_texte(still, (id) => id)).toEqual([]);
  });

  it("liest einen fremden Zustand als neutralen Wert", () => {
    // Ein neuerer Client meldet etwas, das diese Fassung nicht kennt: die
    // Meldung wird nicht abgelehnt, sie ist nur unbekannt (Aufgabe 19).
    const fremd = { zustand: "von-morgen" } as unknown as EintragZustand;
    expect(zustand_ton(fremd)).toBe("leise");
    expect(zustand_text(fremd)).toBe("Unbekannt");
    const still = stand({
      meldung: {
        revision: "aaaa",
        client_version: "0.1.0",
        eintraege: [["mail", fremd]],
      },
    });
    expect(hat_problem(still)).toBe(false);
  });
});

describe("wird_aktualisiert", () => {
  it("vergleicht Soll- und gemeldete Revision", () => {
    expect(wird_aktualisiert(stand())).toBe(false);
    expect(wird_aktualisiert(stand({ soll_revision: "bbbb" }))).toBe(true);
  });

  it("ohne Meldung gibt es nichts zu vergleichen", () => {
    expect(wird_aktualisiert(stand({ meldung: null }))).toBe(false);
  });
});

describe("Probleme", () => {
  it("erkennt Fehler und fehlende Angaben", () => {
    expect(hat_problem(stand())).toBe(false);
    const krank = stand({
      meldung: {
        revision: "aaaa",
        client_version: "0.1.0",
        eintraege: [
          ["mail", { zustand: "fehler", text: "startet nicht" }],
          ["chat", { zustand: "wartet-auf-werte", felder: ["token"] }],
        ],
      },
    });
    expect(hat_problem(krank)).toBe(true);
    expect(problem_texte(krank, (id) => `Titel ${id}`)).toEqual([
      "Titel chat: Braucht Angaben: token",
      "Titel mail: Fehler: startet nicht",
    ]);
  });

  it("sammelt nur die kranken Geräte", () => {
    const gut = stand();
    const krank = stand({
      geraet: { id: "g2", name: "Keller", gruppen: [], gesperrt: false },
      meldung: {
        revision: "aaaa",
        client_version: "0.1.0",
        eintraege: [["mail", { zustand: "fehler", text: "x" }]],
      },
    });
    expect(geraete_mit_problemen([gut, krank]).map((g) => g.geraet.id)).toEqual([
      "g2",
    ]);
    expect(anzahl_mit_problemen([gut, krank])).toBe(1);
  });
});

describe("stand_text", () => {
  it("sagt, warum ein Gerät auffällt", () => {
    expect(stand_text(stand())).toBe("aktuell");
    expect(stand_text(stand({ soll_revision: "bbbb" }))).toBe(
      "wird aktualisiert",
    );
    expect(stand_text(stand({ meldung: null }))).toBe("noch keine Meldung");
    expect(
      stand_text(
        stand({
          geraet: {
            id: "g1",
            name: "Büro",
            gruppen: [],
            gesperrt: true,
          },
        }),
      ),
    ).toBe("gesperrt");
  });
});

describe("stand_ton", () => {
  it("färbt gesperrt, hinkend und ungemeldet", () => {
    expect(stand_ton(stand())).toBe("gut");
    expect(stand_ton(stand({ soll_revision: "bbbb" }))).toBe("warnung");
    expect(stand_ton(stand({ meldung: null }))).toBe("leise");
    expect(
      stand_ton(
        stand({
          geraet: { id: "g1", name: "Büro", gruppen: [], gesperrt: true },
        }),
      ),
    ).toBe("fehler");
  });
});

describe("Gruppen und Revision", () => {
  const gruppen: Gruppe[] = [
    { id: "team", titel: "Team", saetze: [], entra: [] },
    { id: "ops", titel: "Betrieb", saetze: [], entra: [] },
  ];

  it("löst Kennungen in Titel auf, unbekannte bleiben Kennung", () => {
    expect(gruppen_namen(["ops", "fehlt"], gruppen)).toEqual([
      "Betrieb",
      "fehlt",
    ]);
  });

  it("kürzt lange Revisionen", () => {
    expect(revision_kurz("kurz")).toBe("kurz");
    expect(revision_kurz("0123456789abcdef")).toBe("0123456789ab…");
  });
});

describe("Besitzer", () => {
  it("nennt Name, sonst E-Mail, sonst Objekt-Id", () => {
    expect(
      besitzer_text(
        stand({
          besitzer: {
            oid: "11111111-1111-1111-1111-111111111111",
            email: "anna@example.test",
            name: "Anna Admin",
          },
        }),
      ),
    ).toBe("Anna Admin");
    expect(
      besitzer_text(
        stand({
          besitzer: {
            oid: "11111111-1111-1111-1111-111111111111",
            email: "anna@example.test",
            name: null,
          },
        }),
      ),
    ).toBe("anna@example.test");
    expect(
      besitzer_text(
        stand({
          besitzer: {
            oid: "11111111-1111-1111-1111-111111111111",
          },
        }),
      ),
    ).toBe("11111111-1111-1111-1111-111111111111");
  });

  it("nimmt auch einen Besitzer als Namen", () => {
    expect(besitzer_text(stand({ besitzer: "anna@example.test" }))).toBe(
      "anna@example.test",
    );
  });

  it("rät keinen Besitzer, wenn der Server keinen nennt", () => {
    expect(besitzer_text(stand())).toBe("ohne Benutzer (Code)");
    expect(besitzer_hinweis(stand())).toContain("keinen Besitzer");
    expect(besitzer_hinweis(stand({ besitzer: { oid: "x" } }))).toBeNull();
  });
});
