// Der Eintrag als Modell: leere Vorlage, Kennung aus dem Titel, Textfelder
// für Argumente und Umgebung, Prüfung vor dem Senden. Reine Funktionen.

import type {
  Anmeldung,
  Eintrag,
  Einrichtung,
  Feld,
  FeldLink,
  Laufzeit,
  Mehrfach,
  Quelle,
  Werte,
} from "../api/typen";
import { tief_kopie } from "./kopie";

/** Leere Vorlage für „Neu"; `id` wird beim Anlegen aus dem Titel gebildet. */
export function neuer_eintrag(id = ""): Eintrag {
  return {
    id,
    version: "1.0.0",
    titel: "",
    beschreibung: "",
    herkunft: "eigen",
    laufzeit: {
      art: "stdio",
      befehl: "",
      argumente: [],
      umgebung: {},
    },
    felder: [],
    aktiv: true,
    anmeldung: null,
    einrichtung: null,
    mehrfach: null,
  };
}

/** Ein leeres Feld in der Feldliste. */
export function neues_feld(): Feld {
  return {
    name: "",
    beschreibung: "",
    quelle: "server",
    pflicht: true,
    standard: null,
    link: null,
  };
}

/**
 * Ein leeres Feld in einer Zugangsliste (Aufgabe 14). Die Liste gehört dem
 * Gerät, deshalb beginnt sie mit „Gerät (Angaben)“ und nie mit dem Server.
 */
export function neues_zugang_feld(): Feld {
  return { ...neues_feld(), quelle: "client" };
}

/**
 * Ein Hilfe-Link beim Säubern: Ränder weg, und beide Teile leer heißt „kein
 * Link“ (`null`, wie `Option<FeldLink>` in Rust).
 */
export function link_saeubern(
  link: FeldLink | null | undefined,
): FeldLink | null {
  const text = link?.text.trim() ?? "";
  const url = link?.url.trim() ?? "";
  if (text.length === 0 && url.length === 0) return null;
  return { text, url };
}

/**
 * Kennung aus einem Titel: klein, ohne Umlaute, nur a–z, 0–9 und
 * Bindestriche. `Postfach Abruf` → `postfach-abruf`.
 */
export function kennung_aus_titel(titel: string): string {
  const ersetzt = titel
    .toLowerCase()
    .replaceAll("ä", "ae")
    .replaceAll("ö", "oe")
    .replaceAll("ü", "ue")
    .replaceAll("ß", "ss");
  return ersetzt
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+/, "")
    .replace(/-+$/, "");
}

/** Gültige Kennung: klein, mit Bindestrichen, nicht leer. */
export function gueltige_kennung(kennung: string): boolean {
  return /^[a-z0-9]+(?:-[a-z0-9]+)*$/.test(kennung);
}

const FELDNAME = /^[A-Za-z0-9_.-]+$/;

/** Argumente als ein Text, ein Argument je Zeile. */
export function argumente_als_text(argumente: string[]): string {
  return argumente.join("\n");
}

/** Umkehrung: leere Zeilen fallen weg. */
export function argumente_aus_text(text: string): string[] {
  return text
    .split("\n")
    .map((zeile) => zeile.trim())
    .filter((zeile) => zeile.length > 0);
}

/** Umgebung als `NAME=WERT`, alphabetisch – so ist sie stabil. */
export function umgebung_als_text(umgebung: Werte): string {
  return Object.keys(umgebung)
    .sort()
    .map((name) => `${name}=${umgebung[name]}`)
    .join("\n");
}

/** Umkehrung; Zeilen ohne `=` und leere Namen werden verworfen. */
export function umgebung_aus_text(text: string): Werte {
  const werte: Werte = {};
  for (const roh of text.split("\n")) {
    const zeile = roh.trim();
    if (zeile.length === 0 || zeile.startsWith("#")) continue;
    const trenner = zeile.indexOf("=");
    if (trenner <= 0) continue;
    const name = zeile.slice(0, trenner).trim();
    if (name.length === 0) continue;
    werte[name] = zeile.slice(trenner + 1).trim();
  }
  return werte;
}

/** Die drei Befehle einer Anmeldung. */
export type Anmeldebefehl = "anmelden" | "status" | "abmelden";

/** Die Befehle in der Reihenfolge, in der sie im Formular stehen. */
export const ANMELDEBEFEHLE: Anmeldebefehl[] = [
  "anmelden",
  "status",
  "abmelden",
];

/** Eine leere Anmeldung: drei Befehle, noch ohne Argumente. */
export function leere_anmeldung(): Anmeldung {
  return { anmelden: [], status: [], abmelden: [] };
}

/** Die Anmeldung eines Eintrags; ältere Daten kennen das Feld nicht. */
export function anmeldung_von(eintrag: Eintrag): Anmeldung {
  return eintrag.anmeldung ?? leere_anmeldung();
}

/** Drei leere Befehle heißen: keine Anmeldung. */
export function anmeldung_ist_leer(
  anmeldung: Anmeldung | null | undefined,
): boolean {
  if (!anmeldung) return true;
  return ANMELDEBEFEHLE.every((befehl) => anmeldung[befehl].length === 0);
}

/** Braucht der Eintrag eine Anmeldung? Für das Abzeichen in der Liste. */
export function hat_anmeldung(eintrag: Eintrag): boolean {
  return !anmeldung_ist_leer(eintrag.anmeldung);
}

/** Argumente eines Anmelde-Befehls als ein Text, durch Leerzeichen getrennt. */
export function anmeldung_als_text(argumente: string[]): string {
  return argumente.join(" ");
}

/**
 * Umkehrung: ein Argument je Wort. Leerzeichen trennen, leere Wörter fallen
 * weg (Aufgabe 10: „Argumente je Befehl, Leerzeichen-getrennt“).
 */
export function anmeldung_aus_text(text: string): string[] {
  return text
    .split(/\s+/)
    .map((wort) => wort.trim())
    .filter((wort) => wort.length > 0);
}

/** Ein leerer Eintrag zum Einrichten: ein Befehl, noch ohne Argumente. */
export function leere_einrichtung(): Einrichtung {
  return { einrichten: [] };
}

/** Die Einrichtung eines Eintrags; ältere Daten kennen das Feld nicht. */
export function einrichtung_von(eintrag: Eintrag): Einrichtung {
  return eintrag.einrichtung ?? leere_einrichtung();
}

/** Ohne Argumente gibt es keine Einrichtung. */
export function einrichtung_ist_leer(
  einrichtung: Einrichtung | null | undefined,
): boolean {
  if (!einrichtung) return true;
  return einrichtung.einrichten.length === 0;
}

/** Bietet der Eintrag eine Einrichtung an? Für das Abzeichen in der Liste. */
export function hat_einrichtung(eintrag: Eintrag): boolean {
  return !einrichtung_ist_leer(eintrag.einrichtung);
}

/**
 * Die Argumente der Einrichtung als ein Text, durch Leerzeichen getrennt
 * (Aufgabe 11: „Argumente, Leerzeichen-getrennt“).
 */
export function einrichtung_als_text(argumente: string[]): string {
  return argumente.join(" ");
}

/** Umkehrung: ein Argument je Wort, leere Wörter fallen weg. */
export function einrichtung_aus_text(text: string): string[] {
  return text
    .split(/\s+/)
    .map((wort) => wort.trim())
    .filter((wort) => wort.length > 0);
}

/**
 * Eine leere Zugangsliste (Aufgabe 14): Titel, Einzahl und noch kein Feld.
 * Erst mit einem Feld wird daraus ein `mehrfach` im Vertrag.
 */
export function leeres_mehrfach(): Mehrfach {
  return { titel: "", einzahl: "", felder: [] };
}

/** Die Zugangsliste eines Eintrags; ältere Daten kennen das Feld nicht. */
export function mehrfach_von(eintrag: Eintrag): Mehrfach {
  return eintrag.mehrfach ?? leeres_mehrfach();
}

/** Ohne Überschrift, Bezeichnung und Felder gibt es keine Zugangsliste. */
export function mehrfach_ist_leer(
  mehrfach: Mehrfach | null | undefined,
): boolean {
  if (!mehrfach) return true;
  return (
    mehrfach.titel.trim().length === 0 &&
    mehrfach.einzahl.trim().length === 0 &&
    mehrfach.felder.length === 0
  );
}

/** Führt der Eintrag eine Zugangsliste? Für das Abzeichen in der Liste. */
export function hat_mehrfach(eintrag: Eintrag): boolean {
  return !mehrfach_ist_leer(eintrag.mehrfach);
}

/** Die Quellen, die eine Zugangsliste kennt: das Gerät, offen oder geheim. */
export const ZUGANG_QUELLEN: Quelle[] = ["client", "geheim"];

/** Nur die Felder, die der Admin (Server) füllt. */
export function server_felder(eintrag: Eintrag): Feld[] {
  return eintrag.felder.filter((feld) => feld.quelle === "server");
}

/** Hat der Eintrag Felder, die ein Satz vorbelegen kann? */
export function hat_server_felder(eintrag: Eintrag): boolean {
  return server_felder(eintrag).length > 0;
}

/** Nur Server-Werte übernehmen – Client- und Geheimfelder gehören ans Gerät. */
export function server_werte(eintrag: Eintrag, werte: Werte): Werte {
  const erlaubt = new Set(server_felder(eintrag).map((feld) => feld.name));
  const aus: Werte = {};
  for (const [name, wert] of Object.entries(werte)) {
    if (erlaubt.has(name)) aus[name] = wert;
  }
  return aus;
}

/** Quelle als Text für die Auswahl. */
export function quelle_text(quelle: Quelle): string {
  switch (quelle) {
    case "server":
      return "Server (Admin)";
    case "client":
      return "Gerät (Angaben)";
    case "geheim":
      return "Gerät (geheim)";
  }
}

export function herkunft_text(herkunft: Eintrag["herkunft"]): string {
  return herkunft === "eigen" ? "Eigen" : "Fremd";
}

/** Die Laufzeit in einem Halbsatz für Listen. */
export function laufzeit_text(laufzeit: Laufzeit): string {
  if (laufzeit.art === "stdio") {
    return laufzeit.befehl.trim().length > 0
      ? `Stdio · ${laufzeit.befehl}`
      : "Stdio";
  }
  return laufzeit.url.trim().length > 0
    ? `Remote · ${laufzeit.url}`
    : "Remote";
}

/** Ein Feld für den Versand vorbereiten: Ränder weg, leerer Standard `null`. */
function feld_saeubern(feld: Feld): Feld {
  const standard = feld.standard?.trim() ?? "";
  return {
    ...feld,
    name: feld.name.trim(),
    beschreibung: feld.beschreibung.trim(),
    standard: standard.length > 0 ? standard : null,
    link: link_saeubern(feld.link),
  };
}

/** Vor dem Senden: Ränder weg, leere Standardwerte werden `null`. */
export function eintrag_saeubern(eintrag: Eintrag): Eintrag {
  const sauber = tief_kopie(eintrag);
  sauber.id = sauber.id.trim();
  sauber.titel = sauber.titel.trim();
  sauber.version = sauber.version.trim();
  sauber.beschreibung = sauber.beschreibung.trim();
  sauber.felder = sauber.felder.map(feld_saeubern);
  if (sauber.laufzeit.art === "stdio") {
    sauber.laufzeit.befehl = sauber.laufzeit.befehl.trim();
  } else {
    sauber.laufzeit.url = sauber.laufzeit.url.trim();
  }
  if (sauber.anmeldung) {
    const anmeldung = anmeldung_von(sauber);
    sauber.anmeldung = {
      anmelden: anmeldung.anmelden.map((teil) => teil.trim()).filter((teil) => teil.length > 0),
      status: anmeldung.status.map((teil) => teil.trim()).filter((teil) => teil.length > 0),
      abmelden: anmeldung.abmelden.map((teil) => teil.trim()).filter((teil) => teil.length > 0),
    };
    // Drei leere Befehle sind keine Anmeldung – dann steht auch nichts im Vertrag.
    if (anmeldung_ist_leer(sauber.anmeldung)) sauber.anmeldung = null;
  }
  if (sauber.einrichtung) {
    const einrichtung = einrichtung_von(sauber);
    sauber.einrichtung = {
      einrichten: einrichtung.einrichten
        .map((teil) => teil.trim())
        .filter((teil) => teil.length > 0),
    };
    // Ohne Argumente gibt es keine Einrichtung.
    if (einrichtung_ist_leer(sauber.einrichtung)) sauber.einrichtung = null;
  }
  if (sauber.mehrfach) {
    const mehrfach = mehrfach_von(sauber);
    sauber.mehrfach = {
      titel: mehrfach.titel.trim(),
      einzahl: mehrfach.einzahl.trim(),
      felder: mehrfach.felder.map(feld_saeubern),
    };
    // Ohne Titel, Bezeichnung und Felder ist es keine Zugangsliste.
    if (mehrfach_ist_leer(sauber.mehrfach)) sauber.mehrfach = null;
  }
  return sauber;
}

/**
 * Prüfung vor dem Senden. Ergebnis: Feld → Meldung; leer heißt in Ordnung.
 * Die Schlüssel passen zu den Eingaben, damit die Ansicht sie markieren kann.
 */
export function eintrag_pruefen(eintrag: Eintrag): Record<string, string> {
  const fehler: Record<string, string> = {};
  if (eintrag.id.trim().length === 0) {
    fehler.id = "Kennung fehlt.";
  } else if (!gueltige_kennung(eintrag.id)) {
    fehler.id = "Nur a–z, 0–9 und Bindestriche, nicht mit einem Bindestrich beginnen.";
  }
  if (eintrag.titel.trim().length === 0) {
    fehler.titel = "Titel fehlt.";
  }
  if (eintrag.version.trim().length === 0) {
    fehler.version = "Version fehlt.";
  }

  if (eintrag.laufzeit.art === "stdio") {
    if (eintrag.laufzeit.befehl.trim().length === 0) {
      fehler.befehl = "Befehl fehlt.";
    }
  } else {
    const url = eintrag.laufzeit.url.trim();
    if (url.length === 0) {
      fehler.url = "Adresse fehlt.";
    } else if (!/^https?:\/\//i.test(url)) {
      fehler.url = "Die Adresse muss mit http:// oder https:// beginnen.";
    }
  }

  const namen = new Set<string>();
  eintrag.felder.forEach((feld, i) => {
    const name = feld.name.trim();
    if (name.length === 0) {
      fehler[`feld.${i}.name`] = "Name fehlt.";
    } else if (!FELDNAME.test(name)) {
      fehler[`feld.${i}.name`] =
        "Nur Buchstaben, Ziffern, Punkt, Unterstrich und Bindestrich.";
    } else if (namen.has(name)) {
      fehler[`feld.${i}.name`] = "Diesen Namen gibt es im Eintrag schon.";
    } else {
      namen.add(name);
    }
  });

  if (eintrag.anmeldung) {
    if (eintrag.laufzeit.art !== "stdio") {
      fehler.anmeldung = "Eine Anmeldung gibt es nur für Stdio-Einträge.";
    } else {
      for (const befehl of ANMELDEBEFEHLE) {
        if (eintrag.anmeldung[befehl].length === 0) {
          fehler[`anmeldung.${befehl}`] = "Befehl fehlt.";
        }
      }
    }
  }

  if (eintrag.einrichtung) {
    if (eintrag.laufzeit.art !== "stdio") {
      fehler.einrichtung = "Eine Einrichtung gibt es nur für Stdio-Einträge.";
    } else if (eintrag.einrichtung.einrichten.length === 0) {
      fehler["einrichtung.einrichten"] = "Befehl fehlt.";
    }
  }

  if (eintrag.mehrfach) {
    const mehrfach = mehrfach_von(eintrag);
    if (eintrag.laufzeit.art !== "stdio") {
      fehler.mehrfach = "Eine Zugangsliste gibt es nur für Stdio-Einträge.";
    }
    if (mehrfach.titel.trim().length === 0) {
      fehler["mehrfach.titel"] = "Überschrift fehlt.";
    }
    if (mehrfach.einzahl.trim().length === 0) {
      fehler["mehrfach.einzahl"] = "Bezeichnung fehlt.";
    }
    if (mehrfach.felder.length === 0) {
      fehler.mehrfach = "Die Zugangsliste braucht mindestens ein Feld.";
    }
    const zugangsnamen = new Set<string>();
    mehrfach.felder.forEach((feld, i) => {
      const name = feld.name.trim();
      if (name.length === 0) {
        fehler[`mehrfach.feld.${i}.name`] = "Name fehlt.";
      } else if (!FELDNAME.test(name)) {
        fehler[`mehrfach.feld.${i}.name`] =
          "Nur Buchstaben, Ziffern, Punkt, Unterstrich und Bindestrich.";
      } else if (zugangsnamen.has(name)) {
        fehler[`mehrfach.feld.${i}.name`] =
          "Diesen Namen gibt es in der Zugangsliste schon.";
      } else {
        zugangsnamen.add(name);
      }
      if (!ZUGANG_QUELLEN.includes(feld.quelle)) {
        fehler[`mehrfach.feld.${i}.quelle`] =
          "Eine Zugangsliste füllt das Gerät: nur Gerät (Angaben) oder Gerät (geheim).";
      }
    });
  }

  return fehler;
}

/** Einträge alphabetisch nach Titel, für alle Listen. */
export function sortiert_nach_titel(eintraege: Eintrag[]): Eintrag[] {
  return [...eintraege].sort((a, b) =>
    a.titel.localeCompare(b.titel, "de", { sensitivity: "base" }),
  );
}

/** Suche über Kennung, Titel und Beschreibung, ohne Rücksicht auf Groß-/Kleinschreibung. */
export function filter_eintraege(eintraege: Eintrag[], suche: string): Eintrag[] {
  const wort = suche.trim().toLowerCase();
  if (wort.length === 0) return eintraege;
  return eintraege.filter((eintrag) =>
    [eintrag.id, eintrag.titel, eintrag.beschreibung]
      .join(" ")
      .toLowerCase()
      .includes(wort),
  );
}

/** Kennung → Eintrag, für Nachschlagen in Sätzen und Geräten. */
export function nach_kennung(eintraege: Eintrag[]): Map<string, Eintrag> {
  return new Map(eintraege.map((eintrag) => [eintrag.id, eintrag]));
}
