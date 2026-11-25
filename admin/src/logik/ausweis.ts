// Zwei Anzeigen aus Aufgabe 23, als reine Funktionen: die Warnung zum
// Entra-Zertifikat und der Hinweis an einer Gruppe aus Entra ID. Die Ansicht
// rechnet nichts selbst (AGENTS.md §2.4).

import type { EntraAusweis, EntraGruppe, Gruppe } from "../api/typen";

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
  // Die Herkunft steht als Merkmal neben dem Titel, hier nur, was zu tun ist.
  return ohne_satz(gruppe) ? "Noch kein Satz – gibt noch keine Tools" : null;
}

/**
 * Der zuletzt aus Entra geholte Anzeigename zu einer Objekt-Id, oder `null`.
 * Nur Anzeige (Aufgabe 30); der Schlüssel bleibt die Id.
 */
export function entra_name(id: string, gesehen: EntraGruppe[]): string | null {
  const treffer = gesehen.find(
    (eintrag) => eintrag.id.toLowerCase() === id.toLowerCase(),
  );
  const name = treffer?.name_entra?.trim();
  return name ? name : null;
}

/**
 * Die kleine Zeile unter dem Titel einer Gruppe: „Entra: <Name>“, wenn der
 * Name aus Entra vom Titel abweicht. Sonst `null` – dann gibt es nichts zu
 * sagen.
 */
export function entra_name_hinweis(
  gruppe: Gruppe,
  gesehen: EntraGruppe[],
): string | null {
  const id = (gruppe.entra ?? [])[0];
  if (!id) return null;
  const name = entra_name(id, gesehen);
  if (!name || name === gruppe.titel.trim()) return null;
  return `Entra: ${name}`;
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
