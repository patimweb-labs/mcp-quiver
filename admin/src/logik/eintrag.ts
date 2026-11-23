// Der Eintrag als Modell: leere Vorlage, Kennung aus dem Titel, Textfelder
// für Argumente und Umgebung, Prüfung vor dem Senden. Reine Funktionen.

import type { Eintrag, Feld, Laufzeit, Quelle, Werte } from "../api/typen";
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
  };
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

/** Vor dem Senden: Ränder weg, leere Standardwerte werden `null`. */
export function eintrag_saeubern(eintrag: Eintrag): Eintrag {
  const sauber = tief_kopie(eintrag);
  sauber.id = sauber.id.trim();
  sauber.titel = sauber.titel.trim();
  sauber.version = sauber.version.trim();
  sauber.beschreibung = sauber.beschreibung.trim();
  sauber.felder = sauber.felder.map((feld) => {
    const standard = feld.standard?.trim() ?? "";
    return {
      ...feld,
      name: feld.name.trim(),
      beschreibung: feld.beschreibung.trim(),
      standard: standard.length > 0 ? standard : null,
    };
  });
  if (sauber.laufzeit.art === "stdio") {
    sauber.laufzeit.befehl = sauber.laufzeit.befehl.trim();
  } else {
    sauber.laufzeit.url = sauber.laufzeit.url.trim();
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
