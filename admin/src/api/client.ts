// Der eine Weg zum Server: `Authorization: Bearer <admin-token>`, JSON rein,
// JSON raus, Fehler im Vertrag (`{ fehler: "…" }`).
//
// Ein 401 ist kein gewöhnlicher Fehler: die Verwaltung wirft das Token weg und
// zeigt wieder die Anmeldung. Dafür meldet sich dieses Modul bei einem
// Beobachter an – die Ansichten müssen 401 nicht selbst behandeln.

import type {
  Code,
  CodeAnlegen,
  Eintrag,
  EintragId,
  GeraetAendern,
  GeraetStand,
  Gruppe,
  GruppeId,
  Satz,
  SatzId,
  Ueberblick,
} from "./typen";

export const API_BASIS = "/api/v1";

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

interface Anfrage {
  methode?: "GET" | "PUT" | "POST" | "DELETE";
  rumpf?: unknown;
}

async function anfrage<T>(
  token: string,
  pfad: string,
  optionen: Anfrage = {},
): Promise<T> {
  const kopfzeilen: Record<string, string> = {
    Authorization: `Bearer ${token}`,
    Accept: "application/json",
  };
  let rumpf: string | undefined;
  if (optionen.rumpf !== undefined) {
    kopfzeilen["Content-Type"] = "application/json";
    rumpf = JSON.stringify(optionen.rumpf);
  }

  let antwort: Response;
  try {
    antwort = await fetch(`${API_BASIS}${pfad}`, {
      method: optionen.methode ?? "GET",
      headers: kopfzeilen,
      body: rumpf,
    });
  } catch {
    throw new ApiFehler(0, "Server nicht erreichbar.");
  }

  if (antwort.status === 401) {
    beiNichtAngemeldet?.();
    throw new ApiFehler(401, "Anmeldung abgelaufen. Bitte erneut anmelden.");
  }

  if (!antwort.ok) {
    let text = `Der Server antwortet mit ${antwort.status}.`;
    try {
      const rumpfFehler = (await antwort.json()) as { fehler?: unknown };
      if (typeof rumpfFehler.fehler === "string" && rumpfFehler.fehler.length > 0) {
        text = rumpfFehler.fehler;
      }
    } catch {
      // Kein JSON im Fehlerfall: der Status reicht als Meldung.
    }
    throw new ApiFehler(antwort.status, text);
  }

  if (antwort.status === 204) return undefined as T;
  return (await antwort.json()) as T;
}

const pfad = (teil: string): string => encodeURIComponent(teil);

/** Ein Client je angemeldetem Token. */
export class Client {
  private readonly token: string;

  constructor(token: string) {
    this.token = token;
  }

  // ------------------------------------------------------------ Überblick

  ueberblick(): Promise<Ueberblick> {
    return anfrage<Ueberblick>(this.token, "/admin/ueberblick");
  }

  // ------------------------------------------------------------ Einträge

  eintraege(): Promise<Eintrag[]> {
    return anfrage<Eintrag[]>(this.token, "/admin/eintraege");
  }

  eintragSetzen(eintrag: Eintrag): Promise<Eintrag> {
    return anfrage<Eintrag>(
      this.token,
      `/admin/eintraege/${pfad(eintrag.id)}`,
      { methode: "PUT", rumpf: eintrag },
    );
  }

  eintragLoeschen(id: EintragId): Promise<void> {
    return anfrage<void>(this.token, `/admin/eintraege/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  /** server.json als geprüftes JSON-Objekt (der Server liest es aus). */
  eintragImportieren(serverJson: unknown): Promise<Eintrag> {
    return anfrage<Eintrag>(this.token, "/admin/eintraege/import", {
      methode: "POST",
      rumpf: serverJson,
    });
  }

  // --------------------------------------------------------------- Sätze

  saetze(): Promise<Satz[]> {
    return anfrage<Satz[]>(this.token, "/admin/saetze");
  }

  satzSetzen(satz: Satz): Promise<Satz> {
    return anfrage<Satz>(this.token, `/admin/saetze/${pfad(satz.id)}`, {
      methode: "PUT",
      rumpf: satz,
    });
  }

  satzLoeschen(id: SatzId): Promise<void> {
    return anfrage<void>(this.token, `/admin/saetze/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  // ------------------------------------------------------------- Gruppen

  gruppen(): Promise<Gruppe[]> {
    return anfrage<Gruppe[]>(this.token, "/admin/gruppen");
  }

  gruppeSetzen(gruppe: Gruppe): Promise<Gruppe> {
    return anfrage<Gruppe>(this.token, `/admin/gruppen/${pfad(gruppe.id)}`, {
      methode: "PUT",
      rumpf: gruppe,
    });
  }

  gruppeLoeschen(id: GruppeId): Promise<void> {
    return anfrage<void>(this.token, `/admin/gruppen/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  // -------------------------------------------------------------- Geräte

  geraete(): Promise<GeraetStand[]> {
    return anfrage<GeraetStand[]>(this.token, "/admin/geraete");
  }

  geraetSetzen(id: string, aenderung: GeraetAendern): Promise<GeraetStand> {
    return anfrage<GeraetStand>(this.token, `/admin/geraete/${pfad(id)}`, {
      methode: "PUT",
      rumpf: aenderung,
    });
  }

  geraetLoeschen(id: string): Promise<void> {
    return anfrage<void>(this.token, `/admin/geraete/${pfad(id)}`, {
      methode: "DELETE",
    });
  }

  /** Einmal-Code zum Einbinden eines Geräts. */
  codeAnlegen(daten: CodeAnlegen): Promise<Code> {
    return anfrage<Code>(this.token, "/admin/codes", {
      methode: "POST",
      rumpf: daten,
    });
  }
}
