import { describe, expect, it } from "vitest";

import type { Ueberblick } from "../api/typen";
import {
  SEITEN,
  STANDARD_SEITE,
  hash_aus_seite,
  ist_seite,
  seite_aus_hash,
  seite_titel,
} from "./navigation";

describe("Seiten", () => {
  it("kennt sieben Seiten mit Ziel", () => {
    expect(SEITEN.map((seite) => seite.id)).toEqual([
      "ueberblick",
      "bibliothek",
      "saetze",
      "gruppen",
      "benutzer",
      "geraete",
      "pruefspur",
    ]);
    for (const seite of SEITEN) {
      expect(seite.titel.length).toBeGreaterThan(0);
      expect(seite.unter.length).toBeGreaterThan(0);
    }
  });

  it("erkennt nur bekannte Seiten", () => {
    expect(ist_seite("geraete")).toBe(true);
    expect(ist_seite("unbekannt")).toBe(false);
  });
});

describe("Adresse und Seite", () => {
  it("läuft für jede Seite hin und zurück", () => {
    for (const seite of SEITEN) {
      expect(seite_aus_hash(hash_aus_seite(seite.id))).toBe(seite.id);
    }
  });

  it("versteht die Adresse mit und ohne Schrägstrich", () => {
    expect(seite_aus_hash("#/saetze")).toBe("saetze");
    expect(seite_aus_hash("#saetze")).toBe("saetze");
  });

  it("fällt bei Unbekanntem auf den Überblick zurück", () => {
    expect(seite_aus_hash("")).toBe(STANDARD_SEITE);
    expect(seite_aus_hash("#/quatsch")).toBe(STANDARD_SEITE);
    expect(seite_titel("quatsch" as never)).toBe("Überblick");
  });
});

describe("Überblickszähler", () => {
  it("bleibt bei den Zahlen des Servers", () => {
    const ueberblick: Ueberblick = {
      eintraege: 3,
      saetze: 2,
      gruppen: 1,
      geraete: 4,
      geraete_verbunden: 2,
      server_version: "0.1.0",
    };
    expect(ueberblick.geraete_verbunden).toBe(2);
  });
});
