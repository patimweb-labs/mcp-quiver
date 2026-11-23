// Zeit und Text für die Ansichten. Reine Funktionen mit `jetzt` als
// Parameter, damit sie ohne Uhr testbar sind.

function als_datum(iso: string): Date | null {
  const datum = new Date(iso);
  return Number.isNaN(datum.getTime()) ? null : datum;
}

function zwei(n: number): string {
  return String(n).padStart(2, "0");
}

/** `05.02.2026, 14:07` in Ortszeit. */
export function datum(iso: string): string {
  const d = als_datum(iso);
  if (!d) return "—";
  return `${zwei(d.getDate())}.${zwei(d.getMonth() + 1)}.${d.getFullYear()}, ${zwei(
    d.getHours(),
  )}:${zwei(d.getMinutes())}`;
}

/** „gerade eben", „vor 4 Minuten", „in 2 Stunden", sonst das Datum. */
export function zeitpunkt(iso: string | null, jetzt: Date = new Date()): string {
  if (!iso) return "noch nie";
  const d = als_datum(iso);
  if (!d) return "—";

  const sekunden = Math.round((jetzt.getTime() - d.getTime()) / 1000);
  const vergangen = sekunden >= 0;
  const betrag = Math.abs(sekunden);

  const sage = (n: number, einzahl: string, mehrzahl: string): string => {
    const teil = `${n} ${n === 1 ? einzahl : mehrzahl}`;
    return vergangen ? `vor ${teil}` : `in ${teil}`;
  };

  if (betrag < 45) return "gerade eben";
  if (betrag < 3600) return sage(Math.round(betrag / 60), "Minute", "Minuten");
  if (betrag < 86400) return sage(Math.round(betrag / 3600), "Stunde", "Stunden");
  if (betrag < 60 * 86400) return sage(Math.round(betrag / 86400), "Tag", "Tagen");
  return datum(iso);
}

/** Ist der Zeitpunkt vorbei? Unbekanntes gilt als abgelaufen. */
export function abgelaufen(iso: string, jetzt: Date = new Date()): boolean {
  const d = als_datum(iso);
  if (!d) return true;
  return d.getTime() <= jetzt.getTime();
}

/** „noch 42 Minuten gültig" / „abgelaufen". */
export function restzeit(iso: string, jetzt: Date = new Date()): string {
  if (abgelaufen(iso, jetzt)) return "abgelaufen";
  const minuten = Math.max(
    1,
    Math.round((new Date(iso).getTime() - jetzt.getTime()) / 60000),
  );
  if (minuten < 60) return `noch ${minuten} ${minuten === 1 ? "Minute" : "Minuten"} gültig`;
  const stunden = Math.round(minuten / 60);
  return `noch ${stunden} ${stunden === 1 ? "Stunde" : "Stunden"} gültig`;
}

/** Kurzform für enge Spalten; lange Texte bekommen ein „…". */
export function kuerzen(text: string, laenge = 80): string {
  if (text.length <= laenge) return text;
  return `${text.slice(0, Math.max(0, laenge - 1)).trimEnd()}…`;
}

/** Ein Name für ein Gerät in einer Meldung, notfalls die Kennung. */
export function name_oder_kennung(name: string, kennung: string): string {
  return name.trim().length > 0 ? name : kennung;
}
