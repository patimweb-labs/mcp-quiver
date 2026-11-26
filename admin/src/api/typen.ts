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

/** Eine Option einer Auswahl (Aufgabe 16; in Rust `FeldOption`). */
export interface FeldOption {
  /** Der gespeicherte Wert. */
  wert: string;
  /** Was der User in der Liste sieht. */
  anzeige: string;
}

/**
 * Wie ein Feld dargestellt wird (Aufgabe 16; in Rust `FeldArt`).
 *
 * Die Form folgt serde: die einfachen Arten sind Zeichenketten, die Auswahl
 * ist extern getaggt (`{ auswahl: { optionen: [...] } }`). Daten ohne `art`
 * gelten als `"text"`.
 */
export type FeldArt =
  | "text"
  | "passwort"
  | "zahl"
  | "schalter"
  | "url"
  | "ordner"
  | { auswahl: { optionen: FeldOption[] } };

/** Ein Konfigurationsfeld eines Eintrags. */
export interface Feld {
  name: string;
  beschreibung: string;
  quelle: Quelle;
  pflicht: boolean;
  standard: string | null;
  /** Hilfe-Link neben dem Feld; `null` heißt: keiner. */
  link: FeldLink | null;
  /** Wie das Feld dargestellt wird; fehlt der Wert, gilt `"text"`. */
  art?: FeldArt;
  /** Anzeige statt des Feldnamens, z. B. „SAP-Benutzer“. */
  titel?: string | null;
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

/**
 * Eine Gruppe bekommt Sätze. Trägt sie `entra`-Ids, stammt sie aus Entra ID
 * (Aufgabe 23): Titel und Sätze gehören dem Admin, die Mitglieder kommen aus
 * dem ID-Token. Die Kennung ist dann die Objekt-Id der Entra-Gruppe.
 */
export interface Gruppe {
  id: GruppeId;
  titel: string;
  saetze: SatzId[];
  /** Entra-Objekt-Ids, die auf diese Gruppe abgebildet werden. */
  entra: string[];
}

/**
 * `GET /api/v1/admin/entra-schluessel`: womit weist sich der Server bei Entra
 * aus, und läuft das Zertifikat bald ab? (Aufgabe 23)
 */
export interface EntraAusweis {
  /** `keine`, `geheimnis` oder `schluessel`. */
  art: "keine" | "geheimnis" | "schluessel";
  /** RFC 3339; nur bei einem Zertifikat. */
  ablauf?: string | null;
  /** Negative Zahl, wenn das Zertifikat schon abgelaufen ist. */
  tage_uebrig?: number | null;
  /** Ab 30 Tagen vor dem Ablauf `true`. */
  warnung: boolean;
  fingerabdruck?: string | null;
  /** Fertiger Warntext des Servers. */
  hinweis?: string | null;
}

/**
 * Eine in Anmeldungen gesehene Entra-Gruppe (Aufgabe 23). Der Server legt
 * für jede Id einmal eine quiver-Gruppe an; gelöschte bleiben gemerkt und
 * lassen sich wieder aufnehmen.
 *
 * `name_entra` ist der Anzeigename aus Entra (Aufgabe 30) – nur Anzeige, nie
 * der Schlüssel; Rechte laufen weiter über `id`.
 */
export interface EntraGruppe {
  id: string;
  anzeigename: string | null;
  zuletzt: string;
  /** Kennung der quiver-Gruppe, falls es sie gibt. */
  gruppe: GruppeId | null;
  geloescht: boolean;
  /** Anzeigename aus Entra, sofern geholt (Aufgabe 30). */
  name_entra: string | null;
  /** Wann er geholt wurde (RFC 3339). */
  name_entra_am: string | null;
}

/** Der Besitzer eines Geräts, wie ihn der Server nennt (Aufgabe 21). */
export interface GeraetBesitzer {
  oid: string;
  email?: string | null;
  name?: string | null;
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
  | { zustand: "am-geraet-aus" }
  | { zustand: "fehler"; text: string }
  | { zustand: "unbekannt" };

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
  /**
   * Wem das Gerät gehört, sofern der Server es nennt: als Objekt oder als
   * Anzeigename. Ältere Server lassen das Feld weg (Altgeräte mit Einmal-Code
   * haben keinen Besitzer).
   */
  besitzer?: GeraetBesitzer | string | null;
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
  /** Ohne Ablauf: gilt, bis ein Gerät ihn einlöst (einmalig bleibt er). */
  unbegrenzt?: boolean;
}

/** Antwort auf `POST /admin/codes`. */
export interface Code {
  code: string;
  /** RFC 3339; `null` heißt: ohne Ablauf. */
  gueltig_bis: string | null;
}

/** Fehlerantwort des Servers. */
export interface Fehler {
  fehler: string;
}

// ------------------------------------------------------------ Anmeldung
//
// Ohne Admin-Token (ADR 0002): Die Sitzung liegt in einem HttpOnly-Cookie,
// Namen und Rolle kommen aus `/api/v1/auth/ich`. Die Formen stehen in
// `crates/protocol/src/lib.rs` (`Rolle`, `Anmeldeart`, `Ich`, `BenutzerSicht`,
// `Anmeldearten`, `RootAnmelden`) und in Aufgabe 25 (`AuthStand`, `Einrichten`).

/** Rolle eines Benutzers; `root` ist der Notzugang, genau einer. */
export type Rolle = "root" | "admin" | "benutzer";

/** Woher die Sitzung kommt. */
export type Anmeldeart = "root" | "entra";

/** Ein Benutzer, wie ihn die Verwaltung sieht (`BenutzerSicht`). */
export interface BenutzerSicht {
  oid: string;
  email: string | null;
  name: string | null;
  rolle: Rolle;
  gesperrt: boolean;
  /** Entra-Objekt-Ids der Gruppen aus dem letzten ID-Token. */
  entra: string[];
  /** RFC 3339, zuletzt gesehen. */
  zuletzt: string | null;
  /** `gueltig` oder `neu_anmelden`. */
  token_stand: string;
  geraete: number;
  /** Alle wirksamen Gruppen mit Herkunft (Aufgabe 33). */
  gruppen: BenutzerGruppe[];
}

/**
 * Eine wirksame Gruppenzugehörigkeit eines Benutzers (Aufgabe 33). Der
 * Schlüssel ist `gruppe`; `titel` und `entra_ids` sind nur Anzeige.
 */
export interface BenutzerGruppe {
  gruppe: GruppeId;
  titel: string;
  /** Über eine Entra-Gruppe; nicht in der Verwaltung entfernbar. */
  aus_entra: boolean;
  /** Von Hand vergeben; entfernbar. */
  von_hand: boolean;
  /** Am Gerät gesetzt (Altweg); hier nicht entfernbar. */
  aus_geraet: boolean;
  /** Wer sie von Hand vergeben hat. */
  von: string | null;
  /** Wann (RFC 3339). */
  seit: string | null;
  /** Die Entra-Objekt-Ids, über die die Gruppe kommt. */
  entra_ids: string[];
}

/** Ein Mitglied einer Gruppe (Aufgabe 33). */
export interface Gruppenmitglied {
  oid: string;
  email: string | null;
  name: string | null;
  gesperrt: boolean;
  /** Über Entra zugeordnet; nicht in der Verwaltung entfernbar. */
  aus_entra: boolean;
  /** Von Hand hinzugefügt; entfernbar. */
  von_hand: boolean;
  von: string | null;
  seit: string | null;
}

/**
 * `PUT /api/v1/admin/benutzer/{oid}/gruppen/{gruppe}` und
 * `PUT /api/v1/admin/gruppen/{id}/mitglieder/{oid}` (Aufgabe 33).
 */
export interface ZuordnungAendern {
  /** `true` = hinzufügen, `false` = entfernen. */
  vorhanden: boolean;
  /** Nötig, wenn sich ein Admin selbst die letzte Gruppe nimmt. */
  selbst_bestaetigt?: boolean;
}

/** `GET /api/v1/auth/ich`: wer ist angemeldet? */
export interface Ich {
  benutzer: BenutzerSicht;
  art: Anmeldeart;
}

/** `GET /api/v1/anmeldearten`: welche Wege bietet dieser Server? */
export interface Anmeldearten {
  entra: boolean;
  root: boolean;
}

/** `PUT /api/v1/admin/benutzer/{oid}`; fehlende Felder bleiben, wie sie sind. */
export interface BenutzerAendern {
  rolle?: Rolle;
  gesperrt?: boolean;
  /** Nötig, wenn sich ein Admin selbst die Rolle entzieht oder sich sperrt. */
  selbst_bestaetigt?: boolean;
}

/** Ein Eintrag der Prüfspur (`PruefspurEintrag`); ohne Geheimnisse. */
export interface PruefspurEintrag {
  /** RFC 3339. */
  zeit: string;
  /** `anmeldung`, `fehlschlag`, `rolle`, `aenderung`, `geraet`, … */
  art: string;
  akteur: string;
  text: string;
  /** Wie oft dasselbe geschah (Fehlversuche einer Adresse in einer Stunde). */
  anzahl: number;
}

/** `POST /api/v1/auth/root`: Root-Anmeldung mit E-Mail und Passwort. */
export interface RootAnmelden {
  email: string;
  passwort: string;
}

/**
 * `GET /auth/stand` (Aufgabe 25, offen): Gibt es einen Root, und ist die
 * Ersteinrichtung offen? `einrichtung` ist `null`, wenn die Frage nicht
 * anwendbar ist; die Verwaltung zeigt dann dieselbe Seite wie `geschlossen`.
 */
export type Einrichtungslage = "offen" | "geschlossen" | null;

export interface AuthStand {
  eingerichtet: boolean;
  einrichtung: Einrichtungslage;
  code_noetig: boolean;
  entra: boolean;
}

/** `POST /auth/einrichten` (Aufgabe 25): den einen Root anlegen. */
export interface Einrichten {
  email: string;
  passwort: string;
  /** Nur nötig, wenn `AuthStand.code_noetig` gilt. */
  code?: string;
}

// ---------------------------------------------------------------- Updates
// (Aufgabe 34)

/** Wie eine Gruppe mit Updates umgeht. */
/** Ein Gerät, dessen Client älter ist als die neueste Veröffentlichung. */
export interface VeralteterClient {
  geraet: string;
  /** Der Anzeigename des Geräts (aus seiner Meldung). */
  name: string;
  /** Die Version, die es zuletzt gemeldet hat. */
  version: string;
}

/** `GET /api/v1/admin/updates`: Stand der Abfrage und was daraus folgt. */
export interface UpdateStand {
  /** Die laufende Server-Version. */
  server_version: string;
  /** Ist die Abfrage der Veröffentlichungen an (`QUIVER_UPDATES`)? */
  abfrage_an: boolean;
  /** RFC 3339; `null`, wenn noch nie gefragt wurde. */
  zuletzt_geprueft?: string | null;
  /** Die neueste veröffentlichte Version; `null`, solange nichts vorliegt. */
  neueste_version?: string | null;
  /** Die Seite mit den Änderungen, falls die Veröffentlichung eine nennt. */
  aenderungen?: string | null;
  /** Ist der Server selbst älter als die neueste Veröffentlichung? */
  server_veraltet: boolean;
  /** Geräte, deren Client älter ist (der Server nennt höchstens 50). */
  veraltete_clients: VeralteterClient[];
  fehler?: string | null;
}

/** Die Seiten der Verwaltung. */
export type Seite =
  | "ueberblick"
  | "bibliothek"
  | "saetze"
  | "gruppen"
  | "benutzer"
  | "geraete"
  | "pruefspur";

/** Die drei Themes, sonst nichts (AGENTS.md §2.7). */
export type Thema = "dark" | "light" | "mono";
