// Wer in welcher Gruppe ist – von Hand oder über Entra (Aufgabe 33).
//
// Alles hier ist reine Rechnung: die Herkunft, was entfernbar ist, wer noch
// fehlt und wann eine Warnung nötig ist (AGENTS.md §2.4). Die Ansichten zeigen
// nur, was hier herauskommt.
//
// Der Schlüssel ist immer die Kennung; Namen sind Anzeige. Zwei Entra-Gruppen
// mit gleichem Namen bleiben damit zwei Gruppen.

import type { BenutzerGruppe, BenutzerSicht, Gruppe, Gruppenmitglied } from "../api/typen";

/** Ein Lesetext für eine Herkunft. */
export interface Herkunft {
  text: string;
  /** Für den Hinweis daneben: was heißt das? */
  hinweis: string;
}

/**
 * Woher eine Zugehörigkeit kommt – in der Reihenfolge, in der sie angezeigt
 * wird. Beides zugleich ist möglich (Entra und von Hand).
 */
export function herkuenfte(eintrag: {
  aus_entra: boolean;
  von_hand: boolean;
  aus_geraet?: boolean;
}): Herkunft[] {
  const aus: Herkunft[] = [];
  if (eintrag.aus_entra) {
    aus.push({
      text: "aus Entra",
      hinweis: "Kommt über die Entra-Gruppe; ändern lässt sich das nur in Entra.",
    });
  }
  if (eintrag.von_hand) {
    aus.push({
      text: "von Hand",
      hinweis: "In der Verwaltung zugeordnet; hier wieder entfernbar.",
    });
  }
  if (eintrag.aus_geraet) {
    aus.push({
      text: "am Gerät",
      hinweis:
        "Am Gerät gesetzt (Altweg). Das steht in der Geräteliste und bleibt hier unberührt.",
    });
  }
  return aus;
}

/** Lässt sich die Zugehörigkeit hier entfernen? Nur die von Hand vergebene. */
export function entfernbar(eintrag: { von_hand: boolean }): boolean {
  return eintrag.von_hand;
}

/** Nimmt das Entfernen dem Benutzer die **letzte** wirksame Gruppe? */
export function letzte_gruppe(gruppen: BenutzerGruppe[], gruppe: string): boolean {
  const eintrag = gruppen.find((eintrag) => eintrag.gruppe === gruppe);
  if (!eintrag || !entfernbar(eintrag)) return false;
  // Kommt sie auch über Entra oder das Gerät, bleibt sie wirksam.
  if (eintrag.aus_entra || eintrag.aus_geraet) return false;
  return gruppen.length === 1;
}

/** Warntext für „du sperrst dich selbst aus“; leer, wenn keine Warnung nötig ist. */
export function aussperr_warnung(gruppen: BenutzerGruppe[], selbst: boolean): string {
  if (!selbst) return "";
  const einzig = gruppen[0];
  if (!einzig || !letzte_gruppe(gruppen, einzig.gruppe)) return "";
  return (
    `Das ist deine letzte Gruppe. Danach gilt „kein Zugriff“: deine Geräte ` +
    `bekommen keine Tools mehr, bis dir jemand wieder eine Gruppe gibt.`
  );
}

/**
 * Gruppen, die der Benutzer noch nicht hat – als Auswahl für „hinzufügen“.
 * Sortiert nach Titel, gesperrte Gruppen gibt es nicht.
 */
export function fehlende_gruppen(
  gruppen: Gruppe[],
  vorhandene: BenutzerGruppe[],
): Gruppe[] {
  const hat = new Set(vorhandene.map((eintrag) => eintrag.gruppe));
  return gruppen
    .filter((gruppe) => !hat.has(gruppe.id))
    .sort((a, b) => a.titel.localeCompare(b.titel, "de", { sensitivity: "base" }));
}

/** Anzeige eines Benutzers: Name, sonst E-Mail, sonst Objekt-Id. */
export function mitglied_name(mitglied: {
  oid: string;
  email: string | null;
  name: string | null;
}): string {
  return mitglied.name?.trim() || mitglied.email?.trim() || mitglied.oid;
}

/** Die kleine Zeile unter dem Namen: E-Mail, sonst nichts. */
export function mitglied_zeile(mitglied: {
  oid: string;
  email: string | null;
  name: string | null;
}): string {
  if (mitglied.name?.trim() && mitglied.email?.trim()) return mitglied.email.trim();
  return mitglied.oid;
}

/** Wie viele Mitglieder sind von Hand da? */
export function von_hand_anzahl(mitglieder: Gruppenmitglied[]): number {
  return mitglieder.filter((mitglied) => mitglied.von_hand).length;
}

/**
 * Benutzer, die der Gruppe noch fehlen – gesucht in Name, E-Mail und
 * Objekt-Id. Der Root bekommt keine Gruppen und fehlt deshalb immer.
 */
export function kandidaten(
  benutzer: BenutzerSicht[],
  mitglieder: Gruppenmitglied[],
  suche: string,
): BenutzerSicht[] {
  const drin = new Set(mitglieder.map((mitglied) => mitglied.oid));
  const gesucht = suche.trim().toLowerCase();
  return benutzer
    .filter((eintrag) => eintrag.rolle !== "root" && !drin.has(eintrag.oid))
    .filter((eintrag) => {
      if (gesucht.length === 0) return true;
      return [eintrag.name ?? "", eintrag.email ?? "", eintrag.oid]
        .join(" ")
        .toLowerCase()
        .includes(gesucht);
    })
    .sort((a, b) =>
      mitglied_name(a).localeCompare(mitglied_name(b), "de", { sensitivity: "base" }),
    );
}

/** „1 Mitglied“ / „3 Mitglieder“. */
export function mitglieder_text(anzahl: number): string {
  return `${anzahl} ${anzahl === 1 ? "Mitglied" : "Mitglieder"}`;
}

/** „1 Gruppe“ / „3 Gruppen“. */
export function gruppen_text(anzahl: number): string {
  return `${anzahl} ${anzahl === 1 ? "Gruppe" : "Gruppen"}`;
}

/** Ein Hinweis für Entra-Mitglieder: hier ist nichts zu entfernen. */
export function entra_hinweis(mitglied: Gruppenmitglied): string {
  if (!mitglied.aus_entra) return "";
  if (mitglied.von_hand) {
    return "Aus Entra; die Zugehörigkeit von Hand lässt sich zusätzlich entfernen.";
  }
  return "Aus Entra; entfernen lässt sich das nur in Entra.";
}

/** Ist der Benutzer gesperrt? Dann wirkt seine Zugehörigkeit nicht. */
export function gesperrt_hinweis(mitglied: Gruppenmitglied): string {
  return mitglied.gesperrt
    ? "Gesperrt: die Zugehörigkeit ist gespeichert, wirkt aber nicht."
    : "";
}
