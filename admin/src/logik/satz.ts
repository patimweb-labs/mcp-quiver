// Sätze und Gruppen: Vorlagen anlegen, Einträge hinzufügen, Werte nur für
// Server-Felder setzen. Reine Funktionen mit neuen Objekten (kein Zustand
// wird an Ort und Stelle verändert).

import type { Gruppe, Satz, SatzId, SatzEintrag, Werte } from "../api/typen";
import { gueltige_kennung } from "./eintrag";
import { tief_kopie } from "./kopie";

export function neuer_satz(id = ""): Satz {
  return { id, titel: "", prioritaet: 0, eintraege: [] };
}

export function neue_gruppe(id = ""): Gruppe {
  return { id, titel: "", saetze: [], entra: [] };
}

/** Prüfung vor dem Senden; Feld → Meldung, leer heißt in Ordnung. */
export function satz_pruefen(satz: Satz): Record<string, string> {
  const fehler: Record<string, string> = {};
  if (satz.id.trim().length === 0) {
    fehler.id = "Kennung fehlt.";
  } else if (!gueltige_kennung(satz.id)) {
    fehler.id = "Nur a–z, 0–9 und Bindestriche, nicht mit einem Bindestrich beginnen.";
  }
  if (satz.titel.trim().length === 0) fehler.titel = "Titel fehlt.";
  if (!Number.isInteger(satz.prioritaet)) {
    fehler.prioritaet = "Priorität muss eine ganze Zahl sein.";
  }
  return fehler;
}

export function gruppe_pruefen(gruppe: Gruppe): Record<string, string> {
  const fehler: Record<string, string> = {};
  if (gruppe.id.trim().length === 0) {
    fehler.id = "Kennung fehlt.";
  } else if (!gueltige_kennung(gruppe.id)) {
    fehler.id = "Nur a–z, 0–9 und Bindestriche, nicht mit einem Bindestrich beginnen.";
  }
  if (gruppe.titel.trim().length === 0) fehler.titel = "Titel fehlt.";
  return fehler;
}

/** Vor dem Senden: Ränder weg. */
export function satz_saeubern(satz: Satz): Satz {
  const sauber = tief_kopie(satz);
  sauber.id = sauber.id.trim();
  sauber.titel = sauber.titel.trim();
  return sauber;
}

/** Vor dem Senden: Ränder weg. */
export function gruppe_saeubern(gruppe: Gruppe): Gruppe {
  const sauber = tief_kopie(gruppe);
  sauber.id = sauber.id.trim();
  sauber.titel = sauber.titel.trim();
  return sauber;
}

/** Einen Eintrag in den Satz aufnehmen; doppelt ergibt keine zweite Zeile. */
export function satz_eintrag_hinzufuegen(
  satz: Satz,
  eintrag: string,
): Satz {
  if (satz.eintraege.some((zeile) => zeile.eintrag === eintrag)) return satz;
  return {
    ...satz,
    eintraege: [...satz.eintraege, { eintrag, werte: {} }],
  };
}

export function satz_eintrag_entfernen(satz: Satz, eintrag: string): Satz {
  return {
    ...satz,
    eintraege: satz.eintraege.filter((zeile) => zeile.eintrag !== eintrag),
  };
}

/** Einen Wert in einem Satz-Eintrag setzen (leer löscht den Wert). */
export function satz_wert_setzen(
  satz: Satz,
  eintrag: string,
  name: string,
  wert: string,
): Satz {
  return {
    ...satz,
    eintraege: satz.eintraege.map((zeile) => {
      if (zeile.eintrag !== eintrag) return zeile;
      const werte: Werte = { ...zeile.werte };
      if (wert.length === 0) {
        delete werte[name];
      } else {
        werte[name] = wert;
      }
      return { ...zeile, werte };
    }),
  };
}

/** Sätze in der Reihenfolge, in der sie gelten: höhere Priorität zuerst. */
export function saetze_sortiert(saetze: Satz[]): Satz[] {
  return [...saetze].sort((a, b) => {
    if (b.prioritaet !== a.prioritaet) return b.prioritaet - a.prioritaet;
    return a.titel.localeCompare(b.titel, "de", { sensitivity: "base" });
  });
}

/** Die Sätze einer Gruppe, in Prioritätsreihenfolge. */
export function saetze_der_gruppe(gruppe: Gruppe, saetze: Satz[]): Satz[] {
  const gewaehlt = new Set(gruppe.saetze);
  return saetze_sortiert(saetze.filter((satz) => gewaehlt.has(satz.id)));
}

/** Satz einem Gerät zuordnen bzw. entfernen (Mehrfachauswahl). */
export function gruppe_satz_setzen(
  gruppe: Gruppe,
  satz: SatzId,
  an: boolean,
): Gruppe {
  const vorhanden = gruppe.saetze.includes(satz);
  if (an === vorhanden) return gruppe;
  return {
    ...gruppe,
    saetze: an
      ? [...gruppe.saetze, satz]
      : gruppe.saetze.filter((id) => id !== satz),
  };
}

/** Ein Satz-Eintrag, falls vorhanden. */
export function satz_eintrag(
  satz: Satz,
  eintrag: string,
): SatzEintrag | undefined {
  return satz.eintraege.find((zeile) => zeile.eintrag === eintrag);
}

/** Wie viele Gruppen benutzen diesen Satz? (Löschen wäre ein Konflikt.) */
export function satz_wird_benutzt(satz: SatzId, gruppen: Gruppe[]): number {
  return gruppen.filter((gruppe) => gruppe.saetze.includes(satz)).length;
}
