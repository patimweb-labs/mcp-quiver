// Sätze und Gruppen: Vorlagen anlegen, Einträge hinzufügen, Werte vorbelegen.
// Reine Funktionen mit neuen Objekten (kein Zustand wird an Ort und Stelle
// verändert). Seit Aufgabe 36 gilt je Wert ein Vorgabe-Modus: Vorschlag
// (änderbar) oder fest („von deiner Firma vorgegeben“).

import type { Feld, Gruppe, Satz, SatzEintrag, SatzId, Vorgabe, Werte } from "../api/typen";
import { gueltige_kennung } from "./eintrag";
import { ist_schalter, schalter_wert } from "./feld";
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

/** Vor dem Senden: Ränder weg, und nur Vorgaben mit Wert behalten. */
export function satz_saeubern(satz: Satz): Satz {
  const sauber = tief_kopie(satz);
  sauber.id = sauber.id.trim();
  sauber.titel = sauber.titel.trim();
  sauber.eintraege = sauber.eintraege.map((zeile) => ({
    ...zeile,
    fest: feste_felder(zeile).filter(
      (name) => (zeile.werte[name] ?? "").trim().length > 0,
    ),
  }));
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
    eintraege: [...satz.eintraege, { eintrag, werte: {}, fest: [] }],
  };
}

export function satz_eintrag_entfernen(satz: Satz, eintrag: string): Satz {
  return {
    ...satz,
    eintraege: satz.eintraege.filter((zeile) => zeile.eintrag !== eintrag),
  };
}

/** Einen Wert in einem Satz-Eintrag setzen (leer löscht Wert und Festlegung). */
export function satz_wert_setzen(
  satz: Satz,
  eintrag: string,
  name: string,
  wert: string,
  feld?: Feld,
): Satz {
  // Ein Schalter lässt sich nur auf seinen Standard (die sichere Stellung)
  // vorbelegen: eine andere Stellung ist keine Vorgabe (Aufgabe 36).
  if (feld && ist_schalter(feld) && wert.length > 0 && schalter_wert(wert, null) !== sichere_stellung(feld)) {
    wert = "";
  }
  return {
    ...satz,
    eintraege: satz.eintraege.map((zeile) => {
      if (zeile.eintrag !== eintrag) return zeile;
      const werte: Werte = { ...zeile.werte };
      if (wert.length === 0) {
        delete werte[name];
        return { ...zeile, werte, fest: ohne(zeile, name) };
      }
      werte[name] = wert;
      return { ...zeile, werte, fest: feste_felder(zeile) };
    }),
  };
}

/** Die festen Felder einer Zeile; ältere Daten kennen die Liste nicht. */
export function feste_felder(zeile: SatzEintrag): string[] {
  return zeile.fest ?? [];
}

/**
 * Wie ein Feld im Satz vorbelegt ist: gar nicht, als Vorschlag oder fest.
 * Ein leeres Feld ist keine Vorgabe.
 */
export function vorgabe_von(
  zeile: SatzEintrag,
  name: string,
): Vorgabe | null {
  const wert = (zeile.werte[name] ?? "").trim();
  if (wert.length === 0) return null;
  return feste_felder(zeile).includes(name) ? "fest" : "vorschlag";
}

/**
 * Die Vorgabe eines Felds setzen (Aufgabe 36).
 *
 * - `vorschlag` nimmt nur die Festlegung zurück, der Wert bleibt.
 * - `fest` braucht einen Wert. Bei einem Schalter ist das „aus“: der Admin
 *   kann etwas verbieten, nicht heimlich freigeben. Ein leerer Text bleibt
 *   ohne Vorgabe (der Server lehnt „fest“ ohne Wert ab).
 */
export function vorgabe_setzen(
  satz: Satz,
  eintrag: string,
  feld: Feld,
  vorgabe: Vorgabe,
): Satz {
  return {
    ...satz,
    eintraege: satz.eintraege.map((zeile) => {
      if (zeile.eintrag !== eintrag) return zeile;
      const werte: Werte = { ...zeile.werte };
      const wert = (werte[feld.name] ?? "").trim();
      if (vorgabe === "vorschlag") {
        return { ...zeile, fest: ohne(zeile, feld.name) };
      }
      // „Fest“ braucht einen Wert. Ein Schalter lässt sich nur auf „aus“
      // festlegen – der Admin kann verbieten, nicht heimlich freigeben.
      if (ist_schalter(feld)) {
        werte[feld.name] = sichere_stellung(feld);
      } else if (wert.length === 0) {
        return zeile;
      }
      return {
        ...zeile,
        werte,
        fest: [...ohne(zeile, feld.name), feld.name],
      };
    }),
  };
}

/** Die sichere Stellung eines Schalters: sein Standard, sonst „aus“. */
export function sichere_stellung(feld: Feld): string {
  return schalter_wert("", feld.standard ?? null);
}

/** Die festen Felder einer Zeile ohne `name`. */
function ohne(zeile: SatzEintrag, name: string): string[] {
  return feste_felder(zeile).filter((feld) => feld !== name);
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
