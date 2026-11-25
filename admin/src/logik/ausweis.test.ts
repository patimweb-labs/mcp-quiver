import { describe, expect, it } from "vitest";

import type { EntraAusweis, EntraGruppe, Gruppe } from "../api/typen";
import {
  entra_name,
  entra_name_hinweis,
  gruppe_hinweis,
  ist_entra_gruppe,
  ohne_satz,
  zertifikat_warnung,
} from "./ausweis";
import { neue_gruppe as gruppe } from "./satz";

function mit(
  aenderung: Partial<Gruppe> & { entra?: string[] },
): Gruppe {
  return { ...gruppe("g"), entra: [], ...aenderung };
}

function stand(aenderung: Partial<EntraAusweis>): EntraAusweis {
  return { art: "schluessel", warnung: false, ...aenderung };
}

describe("Gruppen aus Entra", () => {
  it("erkennt die Gruppen mit Entra-Id", () => {
    expect(ist_entra_gruppe(mit({ entra: ["id-1"] }))).toBe(true);
    expect(ist_entra_gruppe(gruppe("team"))).toBe(false);
    expect(ist_entra_gruppe({ ...gruppe("team"), entra: [] })).toBe(false);
  });

  it("markiert eine Gruppe ohne Satz", () => {
    expect(ohne_satz(gruppe("team"))).toBe(true);
    expect(ohne_satz(mit({ saetze: ["basis"] }))).toBe(false);
  });

  it("nennt nur den fehlenden Satz", () => {
    expect(gruppe_hinweis(mit({ entra: ["id-1"] }))).toBe(
      "Noch kein Satz – gibt noch keine Tools",
    );
    expect(gruppe_hinweis(mit({ entra: ["id-1"], saetze: ["basis"] }))).toBeNull();
    expect(gruppe_hinweis(gruppe("team"))).toBe(
      "Noch kein Satz – gibt noch keine Tools",
    );
    expect(gruppe_hinweis(mit({ saetze: ["basis"] }))).toBeNull();
  });
});

describe("Name aus Entra", () => {
  const ID = "12345678-1234-1234-1234-123456789012";

  function gesehen(name: string | null): EntraGruppe[] {
    return [
      {
        id: ID,
        anzeigename: "Team intern",
        zuletzt: "2026-11-24T10:00:00Z",
        gruppe: ID,
        geloescht: false,
        name_entra: name,
        name_entra_am: "2026-11-24T10:00:00Z",
      },
    ];
  }

  it("nimmt den Namen zur Id, auch in anderer Schreibweise", () => {
    expect(entra_name(ID, gesehen("Team Alpha"))).toBe("Team Alpha");
    expect(entra_name(ID.toUpperCase(), gesehen("Team Alpha"))).toBe("Team Alpha");
    expect(entra_name(ID, gesehen("  "))).toBeNull();
    expect(entra_name(ID, gesehen(null))).toBeNull();
    expect(entra_name(ID, [])).toBeNull();
  });

  it("zeigt „Entra: …“ nur, wenn der Name vom Titel abweicht", () => {
    const gruppe = mit({ entra: [ID], titel: "Team intern" });
    expect(entra_name_hinweis(gruppe, gesehen("Team Alpha"))).toBe(
      "Entra: Team Alpha",
    );
    // Gleich dem Titel: nichts zu sagen.
    expect(
      entra_name_hinweis(mit({ entra: [ID], titel: "Team Alpha" }), gesehen("Team Alpha")),
    ).toBeNull();
    // Ohne Entra-Id oder ohne Namen: nichts.
    expect(entra_name_hinweis(gruppe, gesehen(null))).toBeNull();
    expect(entra_name_hinweis(mit({ entra: [], titel: "Team" }), gesehen("Team Alpha"))).toBeNull();
  });
});

describe("Warnung zum Zertifikat", () => {
  it("warnt nicht ohne Warnung, ohne Schlüssel oder ohne Stand", () => {
    expect(zertifikat_warnung(null)).toBeNull();
    expect(zertifikat_warnung(stand({ warnung: false }))).toBeNull();
    expect(zertifikat_warnung(stand({ art: "geheimnis", warnung: true }))).toBeNull();
    expect(zertifikat_warnung(stand({ art: "keine", warnung: true }))).toBeNull();
  });

  it("nimmt den Text des Servers, wenn er da ist", () => {
    expect(
      zertifikat_warnung(
        stand({ warnung: true, tage_uebrig: 9, hinweis: "Bitte erneuern" }),
      ),
    ).toBe("Bitte erneuern");
  });

  it("fällt auf einen eigenen Satz zurück", () => {
    expect(
      zertifikat_warnung(
        stand({ warnung: true, tage_uebrig: 9, ablauf: "2026-12-03T00:00:00Z" }),
      ),
    ).toBe("Das Entra-Zertifikat gilt noch 9 Tage (bis 2026-12-03T00:00:00Z).");
    expect(
      zertifikat_warnung(stand({ warnung: true, tage_uebrig: -2 })),
    ).toBe("Das Entra-Zertifikat ist seit 2 Tagen abgelaufen.");
  });
});
