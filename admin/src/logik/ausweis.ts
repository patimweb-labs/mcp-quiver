// Zwei Anzeigen aus Aufgabe 23, als reine Funktionen: die Warnung zum
// Entra-Zertifikat und der Hinweis an einer Gruppe aus Entra ID. Die Ansicht
// rechnet nichts selbst (AGENTS.md §2.4).

import type { EntraAusweis, Gruppe } from "../api/typen";

/** Trägt die Gruppe eine Entra-Objekt-Id? Dann stammt sie aus Entra ID. */
export function ist_entra_gruppe(gruppe: Gruppe): boolean {
  return (gruppe.entra ?? []).length > 0;
}

/** Gibt diese Gruppe noch keine Tools, weil kein Satz zugeordnet ist? */
export function ohne_satz(gruppe: Gruppe): boolean {
  return gruppe.saetze.length === 0;
}

/**
 * Hinweis unter dem Titel einer Gruppe, oder `null`, wenn es nichts zu sagen
 * gibt. Eine Gruppe aus Entra ohne Satz ist der häufigste Fall: sie ist da,
 * gibt aber noch nichts.
 */
export function gruppe_hinweis(gruppe: Gruppe): string | null {
  const teile: string[] = [];
  if (ist_entra_gruppe(gruppe)) teile.push("Aus Entra ID");
  if (ohne_satz(gruppe)) teile.push("noch kein Satz – gibt noch keine Tools");
  return teile.length > 0 ? teile.join(" · ") : null;
}

/**
 * Warntext zum Entra-Zertifikat, oder `null`, wenn alles in Ordnung ist.
 * Den Text liefert der Server; die Funktion fällt nur ein, wenn er fehlt.
 */
export function zertifikat_warnung(stand: EntraAusweis | null): string | null {
  if (!stand || stand.art !== "schluessel" || !stand.warnung) return null;
  if (stand.hinweis) return stand.hinweis;
  const tage = stand.tage_uebrig ?? 0;
  if (tage < 0) {
    return `Das Entra-Zertifikat ist seit ${-tage} Tagen abgelaufen.`;
  }
  const bis = stand.ablauf ? ` (bis ${stand.ablauf})` : "";
  return `Das Entra-Zertifikat gilt noch ${tage} Tage${bis}.`;
}
