// Geräte lesen: Was läuft, was fehlt, was hinkt hinterher. Reine Funktionen,
// damit die Hinweise testbar sind und die Ansicht nur zeichnet.

import type {
  EintragId,
  EintragZustand,
  GeraetStand,
  Gruppe,
  GruppeId,
} from "../api/typen";
import type { Ton } from "./ton";

/** Farbe des Zustands: grün läuft, orange braucht Angaben, rot Fehler.
 *
 * „Am Gerät abgeschaltet“ ist die Wahl des Users und **kein** Problem: neutral
 * und leise. Einen Zustand, den diese Fassung nicht kennt (neuere Gegenseite),
 * liest die Verwaltung genauso neutral (Aufgabe 19).
 */
export function zustand_ton(zustand: EintragZustand): Ton {
  switch (zustand.zustand) {
    case "laeuft":
      return "gut";
    case "wartet-auf-werte":
      return "warnung";
    case "fehler":
      return "fehler";
    case "startet":
      return "leise";
    case "am-geraet-aus":
    case "unbekannt":
      return "leise";
    default:
      return "leise";
  }
}

/** Der Zustand als Satz, wie ihn der Client meldet. */
export function zustand_text(zustand: EintragZustand): string {
  switch (zustand.zustand) {
    case "laeuft":
      return `Läuft · ${zustand.tools} ${zustand.tools === 1 ? "Tool" : "Tools"}`;
    case "wartet-auf-werte":
      return zustand.felder.length === 0
        ? "Braucht Angaben"
        : `Braucht Angaben: ${zustand.felder.join(", ")}`;
    case "startet":
      return "Startet …";
    case "am-geraet-aus":
      return "am Gerät abgeschaltet";
    case "fehler":
      return `Fehler: ${zustand.text}`;
    case "unbekannt":
      return "Unbekannt";
    default:
      return "Unbekannt";
  }
}

/** Braucht das Gerät Aufmerksamkeit? (Fehler oder fehlende Angaben) */
export function hat_problem(stand: GeraetStand): boolean {
  return zustand_eintraege(stand).some(
    ([, zustand]) =>
      zustand.zustand === "fehler" || zustand.zustand === "wartet-auf-werte",
  );
}

/** Die gemeldeten Zustände, alphabetisch nach Eintragskennung. */
export function zustand_eintraege(
  stand: GeraetStand,
): [EintragId, EintragZustand][] {
  return [...(stand.meldung?.eintraege ?? [])].sort((a, b) =>
    a[0].localeCompare(b[0]),
  );
}

/**
 * „wird aktualisiert": Der Server möchte eine andere Revision, als das Gerät
 * zuletzt gemeldet hat. Ohne Meldung gibt es nichts zu vergleichen.
 */
export function wird_aktualisiert(stand: GeraetStand): boolean {
  return stand.meldung !== null && stand.meldung.revision !== stand.soll_revision;
}

/** Kurzer Stand in Worten für die Übersicht. */
export function stand_text(stand: GeraetStand): string {
  if (stand.geraet.gesperrt) return "gesperrt";
  if (!stand.meldung) return "noch keine Meldung";
  if (wird_aktualisiert(stand)) return "wird aktualisiert";
  return "aktuell";
}

/** Farbe des Gerätestands: gesperrt rot, hinkt nach orange, sonst leise. */
export function stand_ton(stand: GeraetStand): Ton {
  if (stand.geraet.gesperrt) return "fehler";
  if (wird_aktualisiert(stand)) return "warnung";
  if (!stand.meldung) return "leise";
  return "gut";
}

/** Ein Problem als Satz, mit Titel statt Kennung. */
export function problem_texte(
  stand: GeraetStand,
  titel: (id: EintragId) => string,
): string[] {
  return zustand_eintraege(stand)
    .filter(
      ([, zustand]) =>
        zustand.zustand === "fehler" || zustand.zustand === "wartet-auf-werte",
    )
    .map(([id, zustand]) => `${titel(id)}: ${zustand_text(zustand)}`);
}

/** Geräte, die Aufmerksamkeit brauchen – für den Überblick. */
export function geraete_mit_problemen(geraete: GeraetStand[]): GeraetStand[] {
  return geraete.filter(hat_problem);
}

/** Gruppen einer Kennung als lesbare Namen. */
export function gruppen_namen(ids: GruppeId[], gruppen: Gruppe[]): string[] {
  const nach_id = new Map(gruppen.map((gruppe) => [gruppe.id, gruppe.titel]));
  return ids.map((id) => nach_id.get(id) ?? id);
}

/** Ein Gerät für die Anzeige: Name, sonst Kennung. */
export function geraet_name(stand: GeraetStand): string {
  return stand.geraet.name.trim().length > 0
    ? stand.geraet.name
    : stand.geraet.id;
}

/** Kurze Revision für die Anzeige (die volle steht im Hinweis). */
export function revision_kurz(revision: string): string {
  return revision.length <= 12 ? revision : `${revision.slice(0, 12)}…`;
}

/** Anzahl der Geräte mit Problemen. */
export function anzahl_mit_problemen(geraete: GeraetStand[]): number {
  return geraete_mit_problemen(geraete).length;
}
