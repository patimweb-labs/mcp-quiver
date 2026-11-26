// Updates einordnen (Aufgabe 34): Versionen vergleichen, veraltete Geräte
// erkennen, den Hinweis der Übersicht bauen. Reine Funktionen, damit die
// Ansichten nur zeichnen und die Regeln geprüft sind.

import type { UpdateStand } from "../api/typen";

/** Eine Version als Zahlen plus Vorabkennung; `null`, wenn unlesbar. */
export function version_lesen(
  text: string,
): { teile: number[]; vorab: string | null } | null {
  const roh = text.trim().replace(/^v/, "");
  if (!roh || roh.length > 64) return null;
  const [zahlen, vorab] = roh.split("-", 2);
  const teile = zahlen.split(".").map((teil) => Number(teil));
  if (
    teile.length === 0 ||
    teile.length > 4 ||
    teile.some((teil) => !Number.isInteger(teil) || teil < 0)
  ) {
    return null;
  }
  if (vorab !== undefined && !/^[0-9A-Za-z.]+$/.test(vorab)) return null;
  return { teile, vorab: vorab ?? null };
}

/** Ist `neu` echt neuer als `alt`? Unlesbares ist nie neuer. */
export function version_neuer(neu: string, alt: string): boolean {
  const a = version_lesen(neu);
  const b = version_lesen(alt);
  if (!a || !b) return false;
  for (let i = 0; i < Math.max(a.teile.length, b.teile.length); i += 1) {
    const x = a.teile[i] ?? 0;
    const y = b.teile[i] ?? 0;
    if (x !== y) return x > y;
  }
  // Fertig ist größer als vorab.
  if (a.vorab === null && b.vorab !== null) return true;
  if (a.vorab !== null && b.vorab === null) return false;
  if (a.vorab !== null && b.vorab !== null) return a.vorab > b.vorab;
  return false;
}

/** Die neueste Version, die der Server kennt; leer heißt: noch keine. */
export function neueste_version(stand: UpdateStand | null): string {
  return stand?.neueste_version ?? "";
}

/** Ist der Client dieses Geräts älter als die neueste bekannte Version? */
export function veraltet(
  clientVersion: string | undefined,
  stand: UpdateStand | null,
): boolean {
  if (!clientVersion || !stand) return false;
  const neueste = neueste_version(stand);
  if (!neueste) return false;
  return version_neuer(neueste, clientVersion);
}

/** Der Satz über dem Hinweis in der Übersicht; leer heißt: kein Hinweis. */
export function stand_hinweis(stand: UpdateStand | null): string {
  if (!stand) return "";
  if (stand.fehler) return stand.fehler;
  if (!stand.abfrage_an) return "";
  if (!stand.neueste_version) {
    return "Die Veröffentlichung ist noch nicht abgefragt.";
  }
  if (stand.server_veraltet) {
    return `Der Server läuft mit ${stand.server_version}; veröffentlicht ist ${stand.neueste_version}. Der Server tauscht sich nicht selbst aus.`;
  }
  return "";
}

/** Ist die Abfrage der Veröffentlichungen eingeschaltet? */
export function abfrage_text(stand: UpdateStand | null): string {
  if (!stand) return "—";
  if (!stand.abfrage_an) return "Aus (QUIVER_UPDATES=aus)";
  return "An (höchstens alle 6 Stunden, ohne Kennung)";
}

/** Der Satz über die Geräte mit älterem Client; leer heißt: keiner. */
export function veraltete_text(stand: UpdateStand | null): string {
  const anzahl = stand?.veraltete_clients.length ?? 0;
  if (anzahl === 0) return "";
  if (anzahl === 1) return "Ein Gerät meldet einen älteren Client.";
  return `${anzahl} Geräte melden einen älteren Client.`;
}
