// Die Lage vor der Anmeldung als Summentyp (AGENTS.md §2.2): laden, Einrichten,
// geschlossen, Anmelden, angemeldet, keine Rolle, Fehler schließen einander
// aus. Kein Fächer aus `laedt`/`eingerichtet`/`gesperrt`.
//
// Alles, was hier steht, ist reine Rechnung: der Stand des Servers wird zu
// einer Lage, Formulare werden geprüft, der Rücksprung nach der Anmeldung wird
// auf eigene Pfade begrenzt. Die Ansichten entscheiden nichts (AGENTS.md §2.4).

import type {
  Anmeldearten,
  AuthStand,
  BenutzerSicht,
  Ich,
  Rolle,
  Seite,
} from "../api/typen";
import { STANDARD_SEITE, ist_seite, seite_aus_hash } from "./navigation";

/** Kürzeste Länge des Passworts; dasselbe Maß wie `PASSWORT_MIN` im Server. */
export const PASSWORT_MIN = 12;
/** Längste Länge des Passworts; dasselbe Maß wie `PASSWORT_MAX` im Server. */
export const PASSWORT_MAX = 256;

/** Die Lage der Verwaltung. Die Ansicht zeigt genau einen dieser Fälle. */
export type Lage =
  /** Der Stand des Servers wird geholt. */
  | { art: "laedt" }
  /** Es gibt keinen Root: Ersteinrichtung (Aufgabe 25). */
  | { art: "einrichtung"; code_noetig: boolean }
  /** Das Zeitfenster der Einrichtung ist zu (15 min nach dem Start). */
  | { art: "geschlossen" }
  /** Anmelden, mit Root und – falls eingerichtet – Microsoft. */
  | { art: "anmeldung"; entra: boolean; hinweis: string }
  /** Angemeldet mit einer Rolle, die verwalten darf. */
  | { art: "bereit"; ich: Ich }
  /** Angemeldet, aber die Rolle darf die Verwaltung nicht benutzen. */
  | { art: "kein_zugang"; ich: Ich }
  /** Der Stand ließ sich nicht holen. */
  | { art: "fehler"; text: string };

/** Der Satz, den ein 401 zeigt. */
export const HINWEIS_ABGELAUFEN =
  "Die Anmeldung ist abgelaufen. Bitte erneut anmelden.";

export function anmeldung_lage(entra: boolean, hinweis = ""): Lage {
  return { art: "anmeldung", entra, hinweis };
}

/**
 * `GET /auth/stand` → Lage.
 *
 * Ist ein Root gesetzt, wird angemeldet. Ist keiner gesetzt und das Fenster
 * offen, kommt die Ersteinrichtung. Sonst – auch bei `einrichtung: null`, denn
 * dann ist die Frage nicht anwendbar – zeigt die Verwaltung die geschlossene
 * Seite: ein Weg zur Einrichtung entsteht nur, wenn der Server ihn ausdrücklich
 * anbietet.
 */
export function lage_aus_stand(stand: AuthStand): Lage {
  if (stand.eingerichtet) return anmeldung_lage(stand.entra);
  if (stand.einrichtung === "offen") {
    return { art: "einrichtung", code_noetig: stand.code_noetig };
  }
  return { art: "geschlossen" };
}

/**
 * Ältere Server (Aufgabe 20 ohne 25) antworten auf `/auth/stand` mit 404.
 * Dann bleibt nur die Anmeldung; mehr weiß die Verwaltung nicht.
 */
export function lage_ohne_stand(arten: Anmeldearten): Lage {
  return anmeldung_lage(arten.entra);
}

/** Die Sitzung gilt nicht mehr: zurück zur Anmeldung, mit Begründung. */
export function lage_nach_401(entra: boolean): Lage {
  return anmeldung_lage(entra, HINWEIS_ABGELAUFEN);
}

/** Darf diese Rolle die Verwaltung benutzen? (Rust: `Rolle::darf_verwalten`.) */
export function darf_verwalten(rolle: Rolle): boolean {
  return rolle === "root" || rolle === "admin";
}

/** Nach der Anmeldung: darf es weitergehen, oder fehlt die Rolle? */
export function lage_aus_ich(ich: Ich): Lage {
  return darf_verwalten(ich.benutzer.rolle)
    ? { art: "bereit", ich }
    : { art: "kein_zugang", ich };
}

/**
 * Wohin nach der Anmeldung? Nur eigene Seiten: `#/geraete` führt zu den
 * Geräten, `https://fremd.example/…` oder `#/quatsch` führen zum Überblick.
 * Die Verwaltung baut das Ziel selbst aus `Seite`, nie aus einer fremden
 * Adresse (kein offener Rücksprung).
 */
export function weiter_nach_anmeldung(ziel: string | null | undefined): Seite {
  const roh = ziel ?? "";
  if (!ist_eigener_pfad(roh)) return STANDARD_SEITE;
  return seite_aus_hash(roh);
}

/** Ist das Ziel eine Seite dieser Verwaltung (und keine fremde Adresse)? */
export function ist_eigener_pfad(ziel: string): boolean {
  return ist_seite(ziel.replace(/^#\/?/, "").trim());
}

// ------------------------------------------------------------------ Prüfung

/** Ein Feldfehler: welches Feld, und was daran nicht stimmt. */
export interface Feldfehler {
  feld: "email" | "passwort" | "wiederholung" | "code";
  text: string;
}

/**
 * Die E-Mail, so wie der Server sie normalisiert (klein, ohne Rand). Der
 * Server prüft nur Länge und `@`; mehr zu verlangen wäre eine Lüge.
 */
export function email_normalisieren(email: string): string {
  return email.trim().toLowerCase();
}

export function email_fehler(email: string): string | null {
  const wert = email_normalisieren(email);
  if (wert.length === 0) return "Bitte die E-Mail eintragen.";
  if (wert.length > 254) return "Die E-Mail ist länger als 254 Zeichen.";
  if (!wert.includes("@")) return "Die E-Mail braucht ein @.";
  return null;
}

/** Zeichen zählen wie `chars().count()` im Server, nicht Bytes. */
export function zeichen(wert: string): number {
  return [...wert].length;
}

export function passwort_fehler(passwort: string): string | null {
  const laenge = zeichen(passwort);
  if (laenge === 0) return "Bitte ein Passwort eintragen.";
  if (laenge < PASSWORT_MIN) return `Mindestens ${PASSWORT_MIN} Zeichen.`;
  if (laenge > PASSWORT_MAX) return `Höchstens ${PASSWORT_MAX} Zeichen.`;
  return null;
}

export function wiederholung_fehler(
  passwort: string,
  wiederholung: string,
): string | null {
  if (zeichen(wiederholung) === 0) return "Bitte das Passwort wiederholen.";
  if (passwort !== wiederholung) return "Die Passwörter sind nicht gleich.";
  return null;
}

export function code_fehler(code: string, noetig: boolean): string | null {
  if (!noetig) return null;
  if (code.trim().length === 0) return "Bitte den Einmal-Code eintragen.";
  return null;
}

/** Die Felder der Ersteinrichtung, so wie sie im Formular stehen. */
export interface EinrichtungFelder {
  email: string;
  passwort: string;
  wiederholung: string;
  code: string;
}

/** Der erste Fehler im Formular der Ersteinrichtung – oder keiner. */
export function einrichtung_fehler(
  felder: EinrichtungFelder,
  code_noetig: boolean,
): Feldfehler | null {
  const email = email_fehler(felder.email);
  if (email) return { feld: "email", text: email };
  const passwort = passwort_fehler(felder.passwort);
  if (passwort) return { feld: "passwort", text: passwort };
  const wiederholung = wiederholung_fehler(felder.passwort, felder.wiederholung);
  if (wiederholung) return { feld: "wiederholung", text: wiederholung };
  const code = code_fehler(felder.code, code_noetig);
  if (code) return { feld: "code", text: code };
  return null;
}

/**
 * Der erste Fehler beim Anmelden. Hier wird die Mindestlänge nicht geprüft:
 * Ein zu kurzes Passwort kann nicht stimmen, und der Server sagt es genauer.
 */
export function anmeldung_fehler(
  email: string,
  passwort: string,
): Feldfehler | null {
  const emailWert = email_fehler(email);
  if (emailWert) return { feld: "email", text: emailWert };
  if (passwort.length === 0) {
    return { feld: "passwort", text: "Bitte das Passwort eintragen." };
  }
  return null;
}

// ------------------------------------------------------------------- Anzeige

export const ROLLE_TEXT: Record<Rolle, string> = {
  root: "Root",
  admin: "Admin",
  benutzer: "Mitarbeiter",
};

export function rollen_text(rolle: Rolle): string {
  return ROLLE_TEXT[rolle] ?? "Mitarbeiter";
}

/**
 * Der Name im Kopf: Name, sonst E-Mail, sonst die Objekt-Id. Nie leer – ein
 * leerer Platz wäre eine Behauptung, die nicht stimmt.
 */
export function anzeige_name(benutzer: BenutzerSicht): string {
  const name = benutzer.name?.trim();
  if (name) return name;
  const email = benutzer.email?.trim();
  if (email) return email;
  return benutzer.oid;
}

/**
 * Der Titel der Lage: die Überschrift der Seite (für Screenreader und den
 * Fenstertitel). Jede Lage hat genau einen, damit keine Seite namenlos bleibt.
 */
export function lagetitel(lage: Lage): string {
  switch (lage.art) {
    case "einrichtung":
      return "Ersteinrichtung";
    case "geschlossen":
      return "Ersteinrichtung geschlossen";
    case "anmeldung":
      return "Anmelden";
    case "bereit":
      return "Verwaltung";
    case "kein_zugang":
      return "Keine Rolle für die Verwaltung";
    case "fehler":
      return "Der Server antwortet nicht";
    case "laedt":
      return "Verwaltung";
  }
}
