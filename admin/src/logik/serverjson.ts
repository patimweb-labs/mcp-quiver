// server.json einlesen und grob prüfen, bevor sie zum Server geht. Die
// eigentliche Prüfung (bekannter Pakettyp, Pflichtfelder) macht
// `quiver_bibliothek::importieren` auf der anderen Seite.

export type Leseergebnis =
  | { art: "ok"; wert: Record<string, unknown> }
  | { art: "fehler"; text: string };

export function server_json_lesen(text: string): Leseergebnis {
  const inhalt = text.trim();
  if (inhalt.length === 0) {
    return { art: "fehler", text: "Der Inhalt ist leer." };
  }
  let wert: unknown;
  try {
    wert = JSON.parse(inhalt);
  } catch (fehler) {
    const text = fehler instanceof Error ? fehler.message : String(fehler);
    return { art: "fehler", text: `Kein gültiges JSON: ${text}` };
  }
  if (wert === null || typeof wert !== "object" || Array.isArray(wert)) {
    return {
      art: "fehler",
      text: "server.json muss ein JSON-Objekt sein.",
    };
  }
  return { art: "ok", wert: wert as Record<string, unknown> };
}
