// Benutzer lesen, filtern und in Aktionen übersetzen (Aufgabe 21).
//
// Alles hier ist reine Rechnung: Die Ansicht zeigt, was diese Funktionen
// liefern (AGENTS.md §2.4). Rollen kommen aus `rollen.ts`; die Aktionen
// entsprechen genau den Feldern von `BenutzerAendern` im Vertrag.

import type {
  BenutzerAendern,
  BenutzerSicht,
  EntraGruppe,
  Rolle,
} from "../api/typen";
import { anzeige_name } from "./anmeldung";
import { rang, rolle_eintrag, rolle_fest, rollen_text } from "./rollen";
import type { Ton } from "./ton";

export function benutzer_name(benutzer: BenutzerSicht): string {
  return anzeige_name(benutzer);
}

/** Root zuerst, dann nach Rang, dann nach Namen. */
export function sortiert(liste: BenutzerSicht[]): BenutzerSicht[] {
  return [...liste].sort((a, b) => {
    const rang_abstand = rang(b.rolle) - rang(a.rolle);
    if (rang_abstand !== 0) return rang_abstand;
    return benutzer_name(a).localeCompare(benutzer_name(b), "de", {
      sensitivity: "base",
    });
  });
}

// ------------------------------------------------------------------- Filter

/** Alle Rollen zur Auswahl – die Liste wächst mit `ROLLEN`, nicht hier. */
export type Rollenfilter = Rolle | "alle";
export type Lagefilter = "alle" | "aktiv" | "gesperrt" | "neu_anmelden";

export interface Benutzerfilter {
  suche: string;
  rolle: Rollenfilter;
  lage: Lagefilter;
}

export const OHNE_FILTER: Benutzerfilter = {
  suche: "",
  rolle: "alle",
  lage: "alle",
};

/** Passt der Stand der Prüfung zu „aktiv“/„gesperrt“/„neu anmelden“? */
export function lage_passt(benutzer: BenutzerSicht, lage: Lagefilter): boolean {
  switch (lage) {
    case "alle":
      return true;
    case "aktiv":
      return !benutzer.gesperrt;
    case "gesperrt":
      return benutzer.gesperrt;
    case "neu_anmelden":
      return !benutzer.gesperrt && benutzer.token_stand !== "gueltig";
  }
}

/** Sucht in Name, E-Mail und Objekt-Id; Groß- und Kleinschreibung egal. */
export function passt(benutzer: BenutzerSicht, filter: Benutzerfilter): boolean {
  const suche = filter.suche.trim().toLowerCase();
  if (suche.length > 0) {
    const heu = [
      benutzer_name(benutzer),
      benutzer.email ?? "",
      benutzer.oid,
    ]
      .join(" ")
      .toLowerCase();
    if (!heu.includes(suche)) return false;
  }
  if (filter.rolle !== "alle" && benutzer.rolle !== filter.rolle) return false;
  return lage_passt(benutzer, filter.lage);
}

export function filtern(
  liste: BenutzerSicht[],
  filter: Benutzerfilter,
): BenutzerSicht[] {
  return sortiert(liste.filter((benutzer) => passt(benutzer, filter)));
}

// -------------------------------------------------------- Entra-Gruppen

export interface Gruppenzeile {
  id: string;
  /** Anzeigename, sonst die Objekt-Id (mehr weiß der Server nicht). */
  name: string;
}

/**
 * Die Entra-Gruppen eines Benutzers mit Anzeigename. Ids bleiben der
 * Schlüssel (ADR 0002, Punkt 2); der Name ist nur Anzeige.
 */
export function entra_gruppen(
  benutzer: BenutzerSicht,
  gesehen: EntraGruppe[],
): Gruppenzeile[] {
  // Der Titel der quiver-Gruppe; fehlt die Gruppe (gelöscht), der Name aus
  // Entra. Die Id steht nur im Hinweis, nie als Text (Aufgabe 30).
  const namen = new Map(
    gesehen.map((gruppe) => [
      gruppe.id.toLowerCase(),
      gruppe.anzeigename?.trim() || gruppe.name_entra?.trim() || "",
    ]),
  );
  return benutzer.entra
    .map((id) => ({
      id,
      name: namen.get(id.toLowerCase()) || "unbenannt",
    }))
    .sort((a, b) => a.name.localeCompare(b.name, "de", { sensitivity: "base" }));
}

// --------------------------------------------------------- Stand der Prüfung

export interface Pruefstand {
  text: string;
  ton: Ton;
  /** Für den Hinweis über der Tabelle: was heißt das? */
  hinweis: string;
}

/**
 * Der Stand des Refresh-Tokens. Der Root hat keines (er ist der Notzugang);
 * ohne Entra gibt es nichts nachzuprüfen.
 */
export function pruefstand(
  benutzer: BenutzerSicht,
  entra: boolean,
): Pruefstand {
  if (benutzer.rolle === "root") {
    return {
      text: "Notzugang",
      ton: "leise",
      hinweis: "Der Root meldet sich mit Passwort an; es gibt kein Entra-Token.",
    };
  }
  if (!entra) {
    return {
      text: "ohne Entra",
      ton: "leise",
      hinweis: "Dieser Server nutzt kein Entra; es gibt nichts nachzuprüfen.",
    };
  }
  if (benutzer.token_stand === "gueltig") {
    return {
      text: "geprüft",
      ton: "gut",
      hinweis:
        "Das letzte Nachprüfen war erfolgreich. Gruppenänderungen wirken bis zur nächsten Prüfung.",
    };
  }
  return {
    text: "neu anmelden nötig",
    ton: "warnung",
    hinweis:
      "Das Refresh-Token gilt nicht mehr. Der Benutzer muss sich am Gerät neu anmelden.",
  };
}

// ---------------------------------------------------------------- Aktionen

export interface Aktionen {
  rolle: boolean;
  sperre: boolean;
  pruefen: boolean;
  loeschen: boolean;
}

/** Was darf an diesem Benutzer geändert werden? Root: nichts. */
export function aktionen(benutzer: BenutzerSicht, entra: boolean): Aktionen {
  if (rolle_fest(benutzer.rolle)) {
    return { rolle: false, sperre: false, pruefen: false, loeschen: false };
  }
  return {
    // Eine Rolle, für die es keinen Wechsel gibt (etwa eine spätere Rolle aus
    // Aufgabe 24), bekommt auch keinen Knopf – statt eines leeren Dialogs.
    rolle: zielrolle(benutzer) !== null,
    sperre: true,
    // Ein gesperrter Benutzer hat kein Token, das man einlösen könnte.
    pruefen: entra && !benutzer.gesperrt,
    loeschen: true,
  };
}

/**
 * Die Rolle, die der Knopf setzt – aus der Rollentabelle. Root hat keine;
 * eine neue Rolle aus Aufgabe 24 bekommt ihren Wechsel dort (`wechsel`).
 */
export function zielrolle(benutzer: BenutzerSicht): Rolle | null {
  return rolle_eintrag(benutzer.rolle).wechsel ?? null;
}

/** Beschriftung des Rollenknopfs. */
export function rollenknopf(benutzer: BenutzerSicht): string {
  const ziel = zielrolle(benutzer);
  if (ziel === null) return "Rolle fest";
  return `Zum ${rollen_text(ziel)}`;
}

export function rollen_aendern(
  benutzer: BenutzerSicht,
  selbst: boolean,
): BenutzerAendern | null {
  const ziel = zielrolle(benutzer);
  if (ziel === null) return null;
  return { rolle: ziel, selbst_bestaetigt: selbst };
}

export function sperre_aendern(
  benutzer: BenutzerSicht,
  selbst: boolean,
): BenutzerAendern {
  return { gesperrt: !benutzer.gesperrt, selbst_bestaetigt: selbst };
}

/** Was eine Rückfrage erklärt – ein Satz je Handlung. */
export function rollen_frage(benutzer: BenutzerSicht, selbst: boolean): string {
  const name = benutzer_name(benutzer);
  const ziel = zielrolle(benutzer);
  if (ziel === null) return "";
  if (ziel === "admin") {
    return `${name} wird Admin. Admins dürfen Bibliothek, Sätze, Gruppen, Geräte und Benutzer verwalten.`;
  }
  if (ziel !== "benutzer") {
    return `${name} wird ${rollen_text(ziel)}.`;
  }
  if (selbst) {
    return `Du nimmst dir selbst die Adminrolle. Danach führt diese Sitzung zur Anmeldung zurück; ein anderer Admin muss dich wieder ernennen.`;
  }
  return `${name} wird wieder Mitarbeiter. Die Verwaltung ist damit sofort gesperrt; eigene Geräte und zugewiesene Tools bleiben.`;
}

export function sperre_frage(benutzer: BenutzerSicht, selbst: boolean): string {
  const name = benutzer_name(benutzer);
  if (benutzer.gesperrt) {
    return `${name} wird entsperrt. Die Anmeldung ist danach wieder möglich; die Geräte holen ihre Tools beim nächsten Abgleich.`;
  }
  if (selbst) {
    return `Du sperrst dich selbst. Alle deine Sitzungen enden sofort, auch diese; nur ein anderer Admin kann dich entsperren.`;
  }
  return `${name} wird gesperrt. Alle Sitzungen enden sofort, und die Geräte verlieren ihre Tools, bis wieder entsperrt wird.`;
}

export function pruef_frage(benutzer: BenutzerSicht): string {
  return `Die Entra-Gruppen von ${benutzer_name(
    benutzer,
  )} werden jetzt beim Server nachgeprüft. Das kann einige Sekunden dauern; danach gilt der neue Stand.`;
}

export function loesch_frage(benutzer: BenutzerSicht, selbst: boolean): string {
  const name = benutzer_name(benutzer);
  const konto = benutzer.email ? `${name} (${benutzer.email})` : name;
  if (selbst) {
    return `${konto} wird gelöscht – dein eigenes Konto. Sitzungen und Geräte werden entfernt; das lässt sich nicht rückgängig machen.`;
  }
  return `${konto} wird gelöscht. Sitzungen und Geräte dieses Benutzers werden entfernt; das lässt sich nicht rückgängig machen.`;
}

// ------------------------------------------------------------------- Anzeige

/** „1 Gerät“ / „3 Geräte“. */
export function geraete_text(anzahl: number): string {
  return `${anzahl} ${anzahl === 1 ? "Gerät" : "Geräte"}`;
}
