// Der eine Weg zum Server: JSON rein, JSON raus, Fehler im Vertrag
// (`{ fehler: "…" }`).
//
// Seit ADR 0002 gibt es keinen Admin-Token mehr. Die Anmeldung liegt in einem
// HttpOnly-Cookie (`quiver_sitzung`), das der Browser selbst mitschickt –
// deshalb `credentials: "same-origin"` und nirgends ein `Authorization`-Kopf.
// Nichts wird im Browser gespeichert (kein `localStorage`, kein
// `sessionStorage`). Ändernde Anfragen tragen zusätzlich `X-Quiver: 1`; der
// Server verlangt das zusammen mit einem passenden `Origin` (CSRF, Aufgabe 20).
//
// Ein 401 ist kein gewöhnlicher Fehler: die Verwaltung wirft die Sitzung weg
// und zeigt wieder die Anmeldung. Dafür meldet sich dieses Modul bei einem
// Beobachter an – die Ansichten müssen 401 nicht selbst behandeln.

import type {
  Anmeldearten,
  AuthStand,
  Code,
  CodeAnlegen,
  Eintrag,
  EintragId,
  Einrichten,
  GeraetAendern,
  GeraetStand,
  Gruppe,
  GruppeId,
  Ich,
  Satz,
  SatzId,
  Ueberblick,
} from "./typen";

export const API_BASIS = "/api/v1";
/** Der Browser-Weg der Anmeldung; liegt auf derselben Wurzel wie die Seite. */
export const AUTH_BASIS = "/auth";

export class ApiFehler extends Error {
  readonly status: number;

  constructor(status: number, text: string) {
    super(text);
    this.name = "ApiFehler";
    this.status = status;
  }
}

/** Text für die Ansicht: die Meldung des Servers, sonst etwas Lesbares. */
export function fehlertext(fehler: unknown): string {
  if (fehler instanceof ApiFehler) return fehler.message;
  if (fehler instanceof Error) return fehler.message;
  return String(fehler);
}

let beiNichtAngemeldet: (() => void) | null = null;

/** Wird beim 401 gerufen (einmal je Antwort). */
export function aufNichtAngemeldet(beobachter: (() => void) | null): void {
  beiNichtAngemeldet = beobachter;
}

type Methode = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

/** Ändernde Methoden brauchen `Origin` und `X-Quiver: 1` (Rust: `ist_aendernd`). */
export function ist_aendernd(methode: Methode): boolean {
  return methode !== "GET";
}

/**
 * Die Köpfe einer Anfrage. Kein `Authorization`, kein Token: die Sitzung ist
 * ein Cookie. `X-Quiver: 1` steht nur an ändernden Anfragen, weil GET ihn nicht
 * braucht – und ein Kopf, den niemand prüft, ist eine Lüge.
 */
export function kopfzeilen(
  methode: Methode,
  mitRumpf: boolean,
): Record<string, string> {
  const aus: Record<string, string> = { Accept: "application/json" };
  if (mitRumpf) aus["Content-Type"] = "application/json";
  if (ist_aendernd(methode)) aus["X-Quiver"] = "1";
  return aus;
}

interface Anfrage {
  methode?: Methode;
  rumpf?: unknown;
  /**
   * `true` für Wege, bei denen ein 401 ein gewöhnlicher Fehler ist (Anmelden,
   * Einrichten, Abmelden): dort gibt es keine Sitzung, die ablaufen könnte.
   */
  ohneRueckfall?: boolean;
}

/** Die Meldung des Servers aus einer Fehlerantwort, sonst `null`. */
async function server_text(antwort: Response): Promise<string | null> {
  try {
    const rumpf = (await antwort.json()) as { fehler?: unknown };
    if (typeof rumpf.fehler === "string" && rumpf.fehler.length > 0) {
      return rumpf.fehler;
    }
  } catch {
    // Kein JSON im Fehlerfall: der Status reicht als Meldung.
  }
  return null;
}

async function anfrage<T>(pfad: string, optionen: Anfrage = {}): Promise<T> {
  const methode = optionen.methode ?? "GET";
  const rumpf =
    optionen.rumpf === undefined ? undefined : JSON.stringify(optionen.rumpf);

  let antwort: Response;
  try {
    antwort = await fetch(pfad, {
      method: methode,
      headers: kopfzeilen(methode, rumpf !== undefined),
      body: rumpf,
      // Nur die eigene Herkunft bekommt das Sitzungscookie; kein CORS.
      credentials: "same-origin",
      // Ein Zwischenspeicher dürfte keine Antwort mit Sitzung zeigen.
      cache: "no-store",
    });
  } catch {
    throw new ApiFehler(0, "Server nicht erreichbar.");
  }

  if (antwort.status === 401) {
    const text = await server_text(antwort);
    if (!optionen.ohneRueckfall) beiNichtAngemeldet?.();
    throw new ApiFehler(
      401,
      text ?? "Die Anmeldung ist abgelaufen. Bitte erneut anmelden.",
    );
  }

  if (!antwort.ok) {
    const text =
      (await server_text(antwort)) ??
      `Der Server antwortet mit ${antwort.status}.`;
    throw new ApiFehler(antwort.status, text);
  }

  if (antwort.status === 204) return undefined as T;
  return (await antwort.json()) as T;
}

const pfad = (teil: string): string => encodeURIComponent(teil);

// --------------------------------------------------------------- Anmeldung

/**
 * `GET /auth/stand` (Aufgabe 25, offen). Antwort:
 * `{ eingerichtet, einrichtung: "offen"|"geschlossen"|null, code_noetig, entra }`.
 *
 * Ein älterer Server kennt den Weg nicht und antwortet 404: dann gibt es
 * `null`, und die Verwaltung bleibt bei der Anmeldung.
 */
export async function stand(): Promise<AuthStand | null> {
  try {
    return await anfrage<AuthStand>(`${AUTH_BASIS}/stand`, {
      ohneRueckfall: true,
    });
  } catch (fehler) {
    if (fehler instanceof ApiFehler && fehler.status === 404) return null;
    throw fehler;
  }
}

/** `GET /api/v1/anmeldearten` → `{ entra, root }` (Aufgabe 20). */
export function anmeldearten(): Promise<Anmeldearten> {
  return anfrage<Anmeldearten>(`${API_BASIS}/anmeldearten`, {
    ohneRueckfall: true,
  });
}

/** `POST /auth/einrichten {email, passwort, code?}` → `Ich` und Cookie. */
export function einrichten(daten: Einrichten): Promise<Ich> {
  return anfrage<Ich>(`${AUTH_BASIS}/einrichten`, {
    methode: "POST",
    rumpf: daten,
    ohneRueckfall: true,
  });
}

/** `POST /api/v1/auth/root {email, passwort}` → `Ich` und Cookie. */
export function anmelden(email: string, passwort: string): Promise<Ich> {
  return anfrage<Ich>(`${API_BASIS}/auth/root`, {
    methode: "POST",
    rumpf: { email, passwort },
    ohneRueckfall: true,
  });
}

/** `GET /api/v1/auth/ich` → wer ist angemeldet? `null` heißt: niemand. */
export async function ich(): Promise<Ich | null> {
  try {
    return await anfrage<Ich>(`${API_BASIS}/auth/ich`, { ohneRueckfall: true });
  } catch (fehler) {
    if (fehler instanceof ApiFehler && fehler.status === 401) return null;
    throw fehler;
  }
}

/** `POST /api/v1/auth/abmelden` → 204; die Sitzung ist auch am Server weg. */
export async function abmelden(): Promise<void> {
  try {
    await anfrage<void>(`${API_BASIS}/auth/abmelden`, {
      methode: "POST",
      ohneRueckfall: true,
    });
  } catch (fehler) {
    // Ohne Sitzung gibt es nichts zu löschen; alles andere ist ein Fehler.
    if (fehler instanceof ApiFehler && fehler.status === 401) return;
    throw fehler;
  }
}

/**
 * Die Adresse für „Mit Microsoft anmelden“. Der Browser folgt ihr selbst
 * (Weiterleitung zu Entra und zurück), deshalb ein Link und kein `fetch`.
 */
export const MICROSOFT_ANMELDUNG = `${AUTH_BASIS}/admin/start`;

// ------------------------------------------------------------- Verwaltung

/** Der Client der Verwaltung; die Sitzung steckt im Cookie. */
export class Client {
  // ------------------------------------------------------------ Überblick

  ueberblick(): Promise<Ueberblick> {
    return anfrage<Ueberblick>(`${API_BASIS}/admin/ueberblick`);
  }

  // ------------------------------------------------------------ Einträge

  eintraege(): Promise<Eintrag[]> {
    return anfrage<Eintrag[]>(`${API_BASIS}/admin/eintraege`);
  }

  eintragSetzen(eintrag: Eintrag): Promise<Eintrag> {
    return anfrage<Eintrag>(`${API_BASIS}/admin/eintraege/${pfad(eintrag.id)}`, {
      methode: "PUT",
      rumpf: eintrag,
    });
  }

  eintragLoeschen(id: EintragId): Promise<void> {
    return anfrage<void>(`${API_BASIS}/admin/eintraege/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  /** server.json als geprüftes JSON-Objekt (der Server liest es aus). */
  eintragImportieren(serverJson: unknown): Promise<Eintrag> {
    return anfrage<Eintrag>(`${API_BASIS}/admin/eintraege/import`, {
      methode: "POST",
      rumpf: serverJson,
    });
  }

  // --------------------------------------------------------------- Sätze

  saetze(): Promise<Satz[]> {
    return anfrage<Satz[]>(`${API_BASIS}/admin/saetze`);
  }

  satzSetzen(satz: Satz): Promise<Satz> {
    return anfrage<Satz>(`${API_BASIS}/admin/saetze/${pfad(satz.id)}`, {
      methode: "PUT",
      rumpf: satz,
    });
  }

  satzLoeschen(id: SatzId): Promise<void> {
    return anfrage<void>(`${API_BASIS}/admin/saetze/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  // ------------------------------------------------------------- Gruppen

  gruppen(): Promise<Gruppe[]> {
    return anfrage<Gruppe[]>(`${API_BASIS}/admin/gruppen`);
  }

  gruppeSetzen(gruppe: Gruppe): Promise<Gruppe> {
    return anfrage<Gruppe>(`${API_BASIS}/admin/gruppen/${pfad(gruppe.id)}`, {
      methode: "PUT",
      rumpf: gruppe,
    });
  }

  gruppeLoeschen(id: GruppeId): Promise<void> {
    return anfrage<void>(`${API_BASIS}/admin/gruppen/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  // -------------------------------------------------------------- Geräte

  geraete(): Promise<GeraetStand[]> {
    return anfrage<GeraetStand[]>(`${API_BASIS}/admin/geraete`);
  }

  geraetSetzen(id: string, aenderung: GeraetAendern): Promise<GeraetStand> {
    return anfrage<GeraetStand>(`${API_BASIS}/admin/geraete/${pfad(id)}`, {
      methode: "PUT",
      rumpf: aenderung,
    });
  }

  geraetLoeschen(id: string): Promise<void> {
    return anfrage<void>(`${API_BASIS}/admin/geraete/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  /** Einmal-Code zum Einbinden eines Geräts. */
  codeAnlegen(daten: CodeAnlegen): Promise<Code> {
    return anfrage<Code>(`${API_BASIS}/admin/codes`, {
      methode: "POST",
      rumpf: daten,
    });
  }
}
