// Der Vertrag zwischen Server und Verwaltung, eins zu eins aus
// `crates/domain/src/lib.rs` und `crates/protocol/src/lib.rs`.
//
// Die Namen folgen serde: `kebab-case` für Aufzählungen, `art` als Tag der
// Laufzeit, `zustand` als Tag des Eintrag-Zustands. Ein Feld, das hier anders
// heißt, ist ein Fehler – nicht eine andere Schreibweise.

/** Kennung eines Eintrags, z. B. `microsoft-mail`. */
export type EintragId = string;
/** Kennung eines Satzes. */
export type SatzId = string;
/** Kennung einer Gruppe. */
export type GruppeId = string;
/** Kennung eines Geräts (vom Server vergeben). */
export type GeraetId = string;

/** Name eines Felds → Wert. */
export type Werte = Record<string, string>;

/** Woher ein Eintrag kommt (SPEC.md §4). */
export type Herkunft = "eigen" | "fremd";

/** Wie ein Eintrag läuft. Werte dürfen Platzhalter `${feld}` enthalten. */
export type Laufzeit =
  | {
      art: "stdio";
      befehl: string;
      argumente: string[];
      umgebung: Werte;
    }
  | {
      art: "remote";
      url: string;
      kopfzeilen: Werte;
    };

/** Wer ein Feld füllt (SPEC.md §6). */
export type Quelle = "server" | "client" | "geheim";

/**
 * Ein Hilfe-Link neben einem Feld (Aufgabe 13; in Rust `FeldLink`,
 * `crates/domain`). `url` darf Platzhalter `${feld}` anderer Felder enthalten;
 * der Client zeigt den Knopf erst, wenn alle verwendeten Felder einen Wert
 * haben.
 */
export interface FeldLink {
  /** Beschriftung des Knopfs, z. B. „Token in Outline erstellen“. */
  text: string;
  /** Adresse im Standardbrowser; Platzhalter `${feld}` sind erlaubt. */
  url: string;
}

/** Ein Konfigurationsfeld eines Eintrags. */
export interface Feld {
  name: string;
  beschreibung: string;
  quelle: Quelle;
  pflicht: boolean;
  standard: string | null;
  /** Hilfe-Link neben dem Feld; `null` heißt: keiner. */
  link: FeldLink | null;
}

/**
 * Wie der Client die Anmeldung eines `Stdio`-Tools steuert (SPEC.md §7).
 * Jeder Befehl ist eine Argumentliste für **dasselbe** Programm mit
 * **derselben** Umgebung wie die Laufzeit des Eintrags.
 */
export interface Anmeldung {
  /** Öffnet den Browser; Exit 0 = angemeldet. */
  anmelden: string[];
  /** Exit 0 = angemeldet, Exit 1 = nicht angemeldet, sonst Fehler. */
  status: string[];
  /** Exit 0 = abgemeldet. */
  abmelden: string[];
}

/**
 * Wie der Client die Einrichtung eines `Stdio`-Tools startet (Aufgabe 11).
 *
 * `einrichten` ist eine Argumentliste für **dasselbe** Programm mit
 * **derselben** Umgebung wie die Laufzeit des Eintrags. Der Befehl öffnet
 * selbst eine Seite im Browser und endet, wenn der User sie abschließt.
 */
export interface Einrichtung {
  /** Argumente für das Programm; Exit 0 = eingerichtet, Exit 1 = abgebrochen. */
  einrichten: string[];
}

/**
 * Mehrere gleichartige Zugänge, die der User im Client-Fenster anlegt
 * (Aufgabe 14; in Rust `Mehrfach`, `crates/domain`). Der Client speichert je
 * Zugang einen Namen und die Werte, Geheimes in der Tresor-Datei, und übergibt
 * beim Start alle Zugänge als JSON in `QUIVER_ZUGAENGE`.
 */
export interface Mehrfach {
  /** Überschrift der Liste, z. B. „Verbindungen“. */
  titel: string;
  /** Bezeichnung eines Elements, z. B. „Verbindung“. */
  einzahl: string;
  /** Die Felder eines Zugangs; `Quelle::Server` ist hier nicht erlaubt. */
  felder: Feld[];
}

/** Ein MCP-Tool in der Bibliothek. */
export interface Eintrag {
  id: EintragId;
  version: string;
  titel: string;
  beschreibung: string;
  herkunft: Herkunft;
  laufzeit: Laufzeit;
  felder: Feld[];
  /**
   * Abgeschaltet: bleibt in Bibliothek und Sätzen, wird aber auf keinem Gerät
   * ausgeführt. Fehlt das Feld (ältere Daten), gilt der Eintrag als aktiv.
   */
  aktiv: boolean;
  /**
   * Braucht das Tool eine Anmeldung des Users? Dann bietet das Client-Fenster
   * „Anmelden“ an; ein Agent löst sie nie aus. Fehlt das Feld, braucht der
   * Eintrag keine Anmeldung (Serde lässt `None` weg).
   */
  anmeldung?: Anmeldung | null;
  /**
   * Hat das Tool eine eigene Einrichtungsseite (Profile, Zugänge)? Dann bietet
   * das Client-Fenster „Einrichten“ an; Zugangsdaten gehen so nie durch einen
   * Agenten. Fehlt das Feld, gibt es keine Einrichtung (Serde lässt `None` weg).
   */
  einrichtung?: Einrichtung | null;
  /**
   * Führt der Eintrag eine Liste von Zugängen, die der User am Gerät pflegt?
   * Fehlt das Feld, gibt es keine Zugangsliste (Serde lässt `None` weg).
   */
  mehrfach?: Mehrfach | null;
}

/** Ein Eintrag in einem Satz, mit den Werten, die der Admin vorbelegt. */
export interface SatzEintrag {
  eintrag: EintragId;
  werte: Werte;
}

/** Eine benannte Sammlung von Einträgen. */
export interface Satz {
  id: SatzId;
  titel: string;
  prioritaet: number;
  eintraege: SatzEintrag[];
}

/** Eine Gruppe bekommt Sätze. */
export interface Gruppe {
  id: GruppeId;
  titel: string;
  saetze: SatzId[];
}

/** Ein eingebundenes Gerät. */
export interface Geraet {
  id: GeraetId;
  name: string;
  gruppen: GruppeId[];
  gesperrt: boolean;
}

/** Zustand eines Eintrags auf dem Gerät, wie der Client ihn meldet. */
export type EintragZustand =
  | { zustand: "laeuft"; tools: number }
  | { zustand: "wartet-auf-werte"; felder: string[] }
  | { zustand: "startet" }
  | { zustand: "fehler"; text: string };

/** Was der Client nach jedem Abgleich meldet. */
export interface Meldung {
  revision: string;
  client_version: string;
  /** Serde schreibt `Vec<(EintragId, EintragZustand)>` als Paare. */
  eintraege: [EintragId, EintragZustand][];
}

/** Ein Gerät, wie die Verwaltung es sieht. */
export interface GeraetStand {
  geraet: Geraet;
  soll_revision: string;
  meldung: Meldung | null;
  zuletzt_gesehen: string | null;
  verbunden: boolean;
}

/** Zähler für die Überblicksseite. */
export interface Ueberblick {
  eintraege: number;
  saetze: number;
  gruppen: number;
  geraete: number;
  geraete_verbunden: number;
  server_version: string;
}

/** Rumpf für `PUT /admin/geraete/{id}`. */
export interface GeraetAendern {
  name: string;
  gruppen: GruppeId[];
  gesperrt: boolean;
}

/** Rumpf für `POST /admin/codes`. */
export interface CodeAnlegen {
  gruppen: GruppeId[];
  minuten: number | null;
}

/** Antwort auf `POST /admin/codes`. */
export interface Code {
  code: string;
  /** RFC 3339. */
  gueltig_bis: string;
}

/** Fehlerantwort des Servers. */
export interface Fehler {
  fehler: string;
}

/** Die Seiten der Verwaltung. */
export type Seite =
  | "ueberblick"
  | "bibliothek"
  | "saetze"
  | "gruppen"
  | "geraete";

/** Die drei Themes, sonst nichts (AGENTS.md §2.7). */
export type Thema = "dark" | "light" | "mono";
