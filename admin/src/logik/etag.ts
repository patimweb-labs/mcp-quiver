// Ein Vergleichswert für „hat sich das geändert?" – dasselbe Verfahren wie im
// Server (`crates/server/src/hilfen.rs`, `etag`): kanonisches JSON (Schlüssel
// sortiert, kein Leerraum) und FNV-1a über die UTF-8-Bytes, 64 Bit als Hex in
// Anführungszeichen.
//
// Kein Geheimnis, keine Signatur: nur ein Wert, den die Verwaltung beim Laden
// behält und beim Speichern als `If-Match` zurückschickt. Weicht er ab, hat
// jemand anders inzwischen gespeichert (Server: 409).

const MASKE = 0xffffffffffffffffn;
const PRIMZAHL = 0x100000001b3n;
const START = 0xcbf29ce484222325n;

/** JSON mit sortierten Schlüsseln und ohne Leerraum. */
export function kanonisch(wert: unknown): string {
  if (Array.isArray(wert)) {
    return `[${wert.map((teil) => kanonisch(teil)).join(",")}]`;
  }
  if (wert !== null && typeof wert === "object") {
    const karte = wert as Record<string, unknown>;
    const namen = Object.keys(karte).sort();
    const teile = namen.map(
      (name) => `${JSON.stringify(name)}:${kanonisch(karte[name])}`,
    );
    return `{${teile.join(",")}}`;
  }
  return JSON.stringify(wert) ?? "null";
}

/** FNV-1a über die Bytes eines Textes. */
export function fnv1a(text: string): bigint {
  let hash = START;
  for (const byte of new TextEncoder().encode(text)) {
    hash ^= BigInt(byte);
    hash = (hash * PRIMZAHL) & MASKE;
  }
  return hash;
}

/** Das ETag zu einem Wert (in Anführungszeichen, wie ein Entity-Tag). */
export function etag(wert: unknown): string {
  return `"${fnv1a(kanonisch(wert)).toString(16).padStart(16, "0")}"`;
}
