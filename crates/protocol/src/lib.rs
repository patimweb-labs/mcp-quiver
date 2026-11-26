// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Vertrag zwischen Server, Client und Admin-Oberfläche (HTTP + JSON).
//!
//! Alle Pfade beginnen mit [`PRAEFIX`]. Authentifizierung über
//! `Authorization: Bearer <token>`:
//! - Gerät: das Token aus [`AnmeldenAntwort`].
//! - Admin: das Admin-Token aus der Server-Konfiguration (MVP, später Entra).
//!
//! | Methode | Pfad | Wer | Rumpf → Antwort |
//! |---|---|---|---|
//! | POST | `/anmelden` | – | [`Anmelden`] → [`AnmeldenAntwort`] |
//! | GET | `/geraet/soll` | Gerät | → [`quiver_domain::Soll`], `?system=…&arch=…` |
//! | GET | `/geraet/tools/{name}` | Gerät | Programm, `?system=…&arch=…` (Aufgabe 37) |
//! | GET | `/geraet/ereignisse` | Gerät | SSE, Ereignis `soll`, Daten = Revision |
//! | POST | `/geraet/meldung` | Gerät | [`Meldung`] → 204 |
//! | POST | `/geraet/abmelden` | Gerät | → 204, das Gerät ist am Server gelöscht |
//! | GET/PUT/DELETE | `/admin/eintraege[/{id}]` | Admin | [`quiver_domain::Eintrag`] |
//! | POST | `/admin/eintraege/import` | Admin | server.json → [`quiver_domain::Eintrag`] |
//! | GET/PUT/DELETE | `/admin/saetze[/{id}]` | Admin | [`quiver_domain::Satz`] |
//! | GET/PUT/DELETE | `/admin/gruppen[/{id}]` | Admin | [`quiver_domain::Gruppe`] |
//! | GET | `/admin/geraete` | Admin | → `Vec<`[`GeraetStand`]`>` |
//! | PUT/DELETE | `/admin/geraete/{id}` | Admin | [`GeraetAendern`] → [`GeraetStand`] |
//! | POST | `/admin/codes` | Admin | [`CodeAnlegen`] → [`Code`] |
//! | GET | `/admin/ueberblick` | Admin | → [`Ueberblick`] |
//! | GET | `/admin/programme` | Admin | → `Vec<`[`quiver_domain::Programm`]`>` |
//! | GET | `/geraet/updates` | Gerät | → [`UpdateAngebot`] (Aufgabe 34) |
//! | GET | `/admin/updates` | Admin | → [`UpdateStand`] |
//!
//! Ohne `/api/v1` liegen die offenen Wege der Anmeldung: `/auth/stand` →
//! [`AuthStand`] und `/auth/einrichten` → [`Ich`] (Aufgabe 25).
//!
//! PUT legt an oder ersetzt (die Kennung steht im Pfad und im Rumpf, beide
//! müssen gleich sein). Fehler kommen als [`Fehler`] mit passendem Status.

use quiver_domain::{EintragId, Geraet, GeraetId, GruppeId};
use serde::{Deserialize, Serialize};

pub const PRAEFIX: &str = "/api/v1";
/// Name des SSE-Ereignisses, wenn sich der Soll-Zustand eines Geräts ändert.
pub const EREIGNIS_SOLL: &str = "soll";

/// Gerät einbinden mit einem Einmal-Code aus der Admin-Oberfläche.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anmelden {
    pub code: String,
    /// Anzeigename, z. B. der Rechnername.
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnmeldenAntwort {
    pub geraet: Geraet,
    /// Geheimes Geräte-Token. Der Server speichert nur seinen Hash.
    pub token: String,
    /// Anzeigename des angemeldeten Benutzers (E-Mail oder Name), nur für
    /// „Angemeldet als …“. Kein Schlüssel, kein Geheimnis; fehlt bei
    /// Altgeräten ohne Benutzer und bei älteren Servern.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benutzer: Option<String>,
}

/// Einmal-Code anlegen; das Gerät landet beim Einbinden in diesen Gruppen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeAnlegen {
    #[serde(default)]
    pub gruppen: Vec<GruppeId>,
    /// Gültigkeit in Minuten (Standard 60).
    #[serde(default)]
    pub minuten: Option<u32>,
    /// Ohne Ablauf: der Code gilt, bis ein Gerät ihn einlöst (einmalig
    /// bleibt er trotzdem). `minuten` wird dann nicht beachtet.
    #[serde(default)]
    pub unbegrenzt: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Code {
    /// Kurz und abtippbar, z. B. `K7QM-3XPA`.
    pub code: String,
    /// RFC 3339; `None` heißt: ohne Ablauf.
    pub gueltig_bis: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeraetAendern {
    pub name: String,
    pub gruppen: Vec<GruppeId>,
    pub gesperrt: bool,
}

/// Zustand eines Eintrags auf dem Gerät, wie der Client ihn meldet.
///
/// **Verträglichkeit (Aufgabe 19):** Beide Seiten lesen einen Zustand, den sie
/// nicht kennen, als [`EintragZustand::Unbekannt`] – einen neutralen Wert. Ein
/// älterer Server lehnt die Meldung eines neueren Clients deshalb nicht ab, und
/// ein älterer Client versteht die Meldung eines neueren Servers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "zustand", rename_all = "kebab-case")]
pub enum EintragZustand {
    Laeuft {
        tools: u32,
    },
    /// Pflichtfelder fehlen am Gerät.
    WartetAufWerte {
        felder: Vec<String>,
    },
    Startet,
    /// Das Programm wird gerade vom Server geladen (Aufgabe 37).
    WirdGeladen,
    /// Das Programm wird ersetzt, weil seine Prüfsumme nicht mehr stimmt
    /// (Aufgabe 37).
    Aktualisiert,
    /// Der User hat den Eintrag auf **diesem** Gerät abgeschaltet
    /// (Aufgabe 19). Kein Fehler: es ist seine Wahl.
    AmGeraetAus,
    Fehler {
        text: String,
    },
    /// Ein Zustand, den diese Fassung nicht kennt. Neutral: kein Fehler, kein
    /// Problem, nur „unbekannt“.
    #[serde(other)]
    Unbekannt,
}

/// Was der Client nach jedem Abgleich meldet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meldung {
    pub revision: String,
    pub client_version: String,
    pub eintraege: Vec<(EintragId, EintragZustand)>,
    /// Die Programme, die dieses Gerät vom Server geladen hat und in welcher
    /// Fassung sie laufen (Aufgabe 37). Nur zur Anzeige in der Verwaltung;
    /// fehlt bei älteren Clients.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub programme: Vec<quiver_domain::Programm>,
}

/// Ein Gerät, wie die Admin-Oberfläche es sieht.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeraetStand {
    pub geraet: Geraet,
    /// Revision, die das Gerät haben sollte.
    pub soll_revision: String,
    /// Letzte Meldung des Geräts, falls es schon eine gab.
    pub meldung: Option<Meldung>,
    /// RFC 3339.
    pub zuletzt_gesehen: Option<String>,
    pub verbunden: bool,
    /// Wem das Gerät gehört (E-Mail, sonst Name, sonst Objekt-Id). `None` bei
    /// Geräten aus der Zeit der Einmal-Codes.
    #[serde(default)]
    pub besitzer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ueberblick {
    pub eintraege: u32,
    pub saetze: u32,
    pub gruppen: u32,
    pub geraete: u32,
    pub geraete_verbunden: u32,
    pub server_version: String,
}

/// Warum ein Gerät nicht mehr darf (Aufgabe 20).
///
/// Steht maschinenlesbar in [`Fehler::grund`], damit der Client den richtigen
/// Satz zeigen kann, statt den Text zu raten.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AnmeldeGrund {
    /// Der Benutzer oder das Gerät ist gesperrt.
    Gesperrt,
    /// Das Refresh-Token gilt nicht mehr; neu anmelden.
    NeuAnmelden,
    /// Dem Benutzer ist keine Gruppe zugeordnet.
    KeinZugriff,
}

/// Rolle eines Benutzers. `Root` ist der Notzugang ohne Entra, genau einer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Rolle {
    Root,
    Admin,
    Benutzer,
}

impl Rolle {
    pub fn als_str(self) -> &'static str {
        match self {
            Rolle::Root => "root",
            Rolle::Admin => "admin",
            Rolle::Benutzer => "benutzer",
        }
    }

    pub fn aus_str(wert: &str) -> Self {
        match wert {
            "root" => Rolle::Root,
            "admin" => Rolle::Admin,
            _ => Rolle::Benutzer,
        }
    }

    /// Darf diese Rolle die Verwaltung benutzen?
    pub fn darf_verwalten(self) -> bool {
        matches!(self, Rolle::Root | Rolle::Admin)
    }
}

/// Woher die Sitzung kommt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anmeldeart {
    Root,
    Entra,
}

/// `GET /auth/ich`: wer ist angemeldet?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ich {
    pub benutzer: BenutzerSicht,
    pub art: Anmeldeart,
}

/// Ein Benutzer, wie ihn die Verwaltung sieht.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenutzerSicht {
    pub oid: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub rolle: Rolle,
    pub gesperrt: bool,
    /// Entra-Objekt-Ids der Gruppen aus dem letzten ID-Token.
    #[serde(default)]
    pub entra: Vec<String>,
    /// RFC 3339.
    pub zuletzt: Option<String>,
    /// `gueltig` oder `neu_anmelden`.
    pub token_stand: String,
    pub geraete: u32,
    /// Alle wirksamen Gruppen mit Herkunft (Aufgabe 33).
    #[serde(default)]
    pub gruppen: Vec<BenutzerGruppe>,
}

/// Eine wirksame Gruppenzugehörigkeit eines Benutzers (Aufgabe 33).
///
/// Der Schlüssel ist `gruppe` (die Kennung); `titel` und `entra_ids` sind nur
/// Anzeige. Entra-Gruppen mit gleichem Namen bleiben so unterscheidbar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenutzerGruppe {
    pub gruppe: String,
    /// Titel der Gruppe, sonst die Kennung.
    pub titel: String,
    /// Kommt über eine Entra-Gruppe; nicht hier entfernbar (in Entra ändern).
    pub aus_entra: bool,
    /// Von Hand vergeben; entfernbar.
    pub von_hand: bool,
    /// Am Gerät gesetzt (Altweg, Aufgabe 20); hier nicht entfernbar.
    #[serde(default)]
    pub aus_geraet: bool,
    /// Wer sie von Hand vergeben hat.
    #[serde(default)]
    pub von: Option<String>,
    /// Wann sie von Hand vergeben wurde (RFC 3339).
    #[serde(default)]
    pub seit: Option<String>,
    /// Die Entra-Objekt-Ids, über die die Gruppe kommt (Anzeige).
    #[serde(default)]
    pub entra_ids: Vec<String>,
}

/// Ein Mitglied einer Gruppe (Aufgabe 33).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gruppenmitglied {
    pub oid: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub gesperrt: bool,
    /// Über Entra zugeordnet; nicht hier entfernbar.
    pub aus_entra: bool,
    /// Von Hand hinzugefügt; entfernbar.
    pub von_hand: bool,
    /// Wer hinzugefügt hat.
    #[serde(default)]
    pub von: Option<String>,
    /// Wann hinzugefügt wurde (RFC 3339).
    #[serde(default)]
    pub seit: Option<String>,
}

/// `PUT /admin/benutzer/{oid}/gruppen/{gruppe}` und
/// `PUT /admin/gruppen/{id}/mitglieder/{oid}` (Aufgabe 33): dieselbe
/// Zugehörigkeit, von beiden Seiten bedient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ZuordnungAendern {
    /// `true` = hinzufügen, `false` = entfernen.
    pub vorhanden: bool,
    /// Nötig, wenn sich ein Admin selbst die letzte Gruppe nimmt.
    #[serde(default)]
    pub selbst_bestaetigt: bool,
}

/// `PUT /admin/benutzer/{oid}`; fehlende Felder bleiben, wie sie sind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BenutzerAendern {
    #[serde(default)]
    pub rolle: Option<Rolle>,
    #[serde(default)]
    pub gesperrt: Option<bool>,
    /// Nötig, wenn ein Admin sich selbst die Rolle entzieht.
    #[serde(default)]
    pub selbst_bestaetigt: bool,
}

/// Root-Anmeldung mit E-Mail und Passwort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootAnmelden {
    pub email: String,
    pub passwort: String,
}

/// Ist das Zeitfenster der Ersteinrichtung offen oder zu? (Aufgabe 25)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Einrichtungslage {
    /// Offen: es gibt noch keinen Root, und die 15 Minuten laufen.
    Offen,
    /// Zu: die 15 Minuten nach dem Start sind vorbei (oder zehn Codes falsch).
    Geschlossen,
}

/// `GET /auth/stand` (offen, Aufgabe 25): mehr sagt der Server vor der
/// Anmeldung nicht. `einrichtung` ist `None`, wenn die Frage nicht anwendbar
/// ist – es gibt bereits einen Root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthStand {
    /// Gibt es einen Root-Benutzer?
    pub eingerichtet: bool,
    /// Nur ohne Root gesetzt.
    pub einrichtung: Option<Einrichtungslage>,
    /// Braucht die Einrichtung den Einmal-Code aus dem Server-Log?
    pub code_noetig: bool,
    /// Ist Entra eingerichtet („Mit Microsoft anmelden“)?
    pub entra: bool,
}

/// `POST /auth/einrichten` (Aufgabe 25): den einen Root anlegen. Der Server
/// meldet ihn danach gleich an.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Einrichten {
    pub email: String,
    pub passwort: String,
    /// Nur nötig, wenn [`AuthStand::code_noetig`] gilt.
    #[serde(default)]
    pub code: Option<String>,
}

/// Ein Eintrag der Prüfspur (ohne Geheimnisse).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruefspurEintrag {
    pub zeit: String,
    pub art: String,
    pub akteur: String,
    pub text: String,
    /// Wie oft dasselbe geschah (Fehlversuche einer Adresse in einer Stunde
    /// stehen als **ein** Eintrag mit Zähler). Fehlt der Wert, ist es 1.
    #[serde(default = "eins")]
    pub anzahl: u32,
}

fn eins() -> u32 {
    1
}

/// Eine in Anmeldungen gesehene Entra-Gruppe (Aufgabe 23).
///
/// Der Server legt für jede Id **einmal** eine quiver-Gruppe an; Titel und
/// Sätze gehören danach dem Admin. `gruppe` fehlt, wenn der Admin sie gelöscht
/// hat; `aufnehmen` legt sie wieder an.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntraGruppe {
    pub id: String,
    /// Titel der quiver-Gruppe, die diese Id trägt (sonst `null`).
    pub anzeigename: Option<String>,
    pub zuletzt: String,
    /// Kennung der quiver-Gruppe, falls es sie gibt.
    #[serde(default)]
    pub gruppe: Option<String>,
    /// Vom Admin gelöscht: bleibt gelöscht, bis er sie wieder aufnimmt.
    #[serde(default)]
    pub geloescht: bool,
    /// Anzeigename aus Entra (Aufgabe 30); nur Anzeige, nie der Schlüssel.
    #[serde(default)]
    pub name_entra: Option<String>,
    /// Wann der Name aus Entra geholt wurde (RFC 3339).
    #[serde(default)]
    pub name_entra_am: Option<String>,
}

/// Rumpf für `PUT /admin/entra-gruppen/{id}` (Aufgabe 23).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct EntraGruppeAendern {
    /// Neuer Titel der quiver-Gruppe (nur wenn es sie gibt).
    #[serde(default)]
    pub anzeigename: Option<String>,
    /// Eine gelöschte Gruppe wieder aufnehmen (neu anlegen, ohne Sätze).
    #[serde(default)]
    pub aufnehmen: bool,
}

/// Stand des Entra-Ausweises (Aufgabe 23): Geheimnis, Zertifikat oder keins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntraAusweis {
    /// `keine`, `geheimnis` oder `schluessel`.
    pub art: String,
    /// RFC 3339; nur bei einem Zertifikat.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ablauf: Option<String>,
    /// Negative Zahl, wenn das Zertifikat schon abgelaufen ist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tage_uebrig: Option<i64>,
    /// Ab 30 Tagen vor dem Ablauf `true`.
    pub warnung: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerabdruck: Option<String>,
    /// Fertiger Warntext für die Verwaltung.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hinweis: Option<String>,
}

/// `POST /auth/geraet/tausch`: Code plus PKCE-Verifier gegen ein Geräte-Token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeraeteTausch {
    pub code: String,
    pub verifier: String,
    /// Dasselbe Gerät weiterführen, wenn es keinem oder demselben Benutzer gehört.
    #[serde(default)]
    pub altes_token: Option<String>,
}

/// `GET /anmeldearten`: welche Wege bietet dieser Server?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anmeldearten {
    /// Entra ist eingerichtet (Browser-Anmeldung).
    pub entra: bool,
    /// Ein Root-Benutzer ist gesetzt (E-Mail und Passwort).
    pub root: bool,
}

// ---------------------------------------------------------------- Updates

/// `GET /api/v1/geraet/updates`: was der Server über die Veröffentlichung
/// weiß. **Kein Paket, keine Adresse zum Laden** – der Client lädt nichts; er
/// zeigt nur an (Aufgabe 34).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAngebot {
    /// Die Version, die das Gerät gemeldet hat (oder aus der Abfrage).
    pub aktuelle_version: String,
    /// Die neueste veröffentlichte Version; `None`, solange der Server nichts
    /// erfahren hat.
    #[serde(default)]
    pub neueste_version: Option<String>,
    /// Die Seite mit den Änderungen, falls die Veröffentlichung eine nennt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aenderungen: Option<String>,
    /// Die Pakete, die es für das gemeldete System gibt (Aufgabe 39). Leer
    /// heißt: nichts zum Tauschen (älterer Server, keine Prüfsumme, anderes
    /// System) – dann bleibt es beim Hinweis.
    #[serde(default)]
    pub pakete: Vec<UpdatePaket>,
}

/// Ein Paket dieser Veröffentlichung für das System des Geräts (Aufgabe 39).
///
/// Der Client wählt die Art, die zu seiner Installation passt, lädt sie über
/// [`crate::PRAEFIX`]`/geraet/updates/paket/{name}` und prüft Größe und
/// SHA-256 **vor** jedem Schritt. Ohne Prüfsumme gibt es kein Paket.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePaket {
    /// Der Dateiname im Release (`quiver-0.5.0-linux-x86_64.tar.gz`).
    pub name: String,
    /// Größe in Bytes.
    pub groesse: u64,
    /// SHA-256 über die Datei, klein und hexadezimal.
    pub sha256: String,
    /// `zip`, `msi`, `dmg`, `pkg`, `appimage`, `tar.gz`, `deb`, `rpm`.
    pub art: String,
}

/// Ein Gerät, dessen Client älter ist als die neueste Veröffentlichung.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VeralteterClient {
    pub geraet: GeraetId,
    /// Der Anzeigename des Geräts (aus seiner Meldung).
    pub name: String,
    /// Die Version, die es zuletzt gemeldet hat.
    pub version: String,
}

/// `GET /api/v1/admin/updates`: Stand der Abfrage und was daraus folgt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateStand {
    /// Die laufende Server-Version.
    pub server_version: String,
    /// Ist die Abfrage der Veröffentlichungen eingeschaltet (`QUIVER_UPDATES`)?
    pub abfrage_an: bool,
    /// RFC 3339; `None`, wenn noch nie gefragt wurde.
    #[serde(default)]
    pub zuletzt_geprueft: Option<String>,
    /// Die neueste veröffentlichte Version; `None`, solange nichts vorliegt.
    #[serde(default)]
    pub neueste_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aenderungen: Option<String>,
    /// Ist der **Server** älter als die neueste Veröffentlichung? Er tauscht
    /// sich nicht selbst aus; der Hinweis sagt, dass er neu aufgesetzt wird.
    pub server_veraltet: bool,
    /// Geräte, deren Client älter ist (höchstens [`VERALTETE_MAX`] Stück).
    #[serde(default)]
    pub veraltete_clients: Vec<VeralteterClient>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fehler: Option<String>,
}

/// So viele veraltete Geräte nennt die Verwaltung höchstens.
pub const VERALTETE_MAX: usize = 50;

/// Fehlerantwort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fehler {
    pub fehler: String,
    /// Nur bei Geräte-Antworten gesetzt; ältere Clients ignorieren das Feld.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grund: Option<AnmeldeGrund>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zustand_json_form() {
        let z = EintragZustand::Laeuft { tools: 3 };
        assert_eq!(
            serde_json::to_string(&z).unwrap(),
            r#"{"zustand":"laeuft","tools":3}"#
        );
        assert_eq!(
            serde_json::to_string(&EintragZustand::AmGeraetAus).unwrap(),
            r#"{"zustand":"am-geraet-aus"}"#
        );
        let z: EintragZustand =
            serde_json::from_str(r#"{"zustand":"wartet-auf-werte","felder":["pfad"]}"#).unwrap();
        assert_eq!(
            z,
            EintragZustand::WartetAufWerte {
                felder: vec!["pfad".into()]
            }
        );
    }

    /// Aufgabe 23: Ein alter `EntraGruppe`-Eintrag ohne die neuen Felder wird
    /// weiter gelesen (beide Seiten sind verträglich).
    #[test]
    fn entra_gruppe_ohne_neue_felder_wird_gelesen() {
        let alt: EntraGruppe = serde_json::from_str(
            r#"{"id":"id-1","anzeigename":null,"zuletzt":"2026-11-24T00:00:00Z"}"#,
        )
        .expect("alter Eintrag");
        assert_eq!(alt.id, "id-1");
        assert!(alt.gruppe.is_none());
        assert!(!alt.geloescht);
        assert!(alt.name_entra.is_none());
        assert!(alt.name_entra_am.is_none());

        let stand = EntraAusweis {
            art: String::from("schluessel"),
            ablauf: Some(String::from("2026-12-24T00:00:00Z")),
            tage_uebrig: Some(30),
            warnung: true,
            fingerabdruck: Some(String::from("AA:BB")),
            hinweis: Some(String::from("Bitte erneuern")),
        };
        let json = serde_json::to_string(&stand).expect("JSON");
        assert!(json.contains(r#""art":"schluessel""#));
        assert!(json.contains(r#""tage_uebrig":30"#));
    }

    /// Ein Wert, den diese Fassung nicht kennt, wird **nicht** abgelehnt: die
    /// andere Seite ist neuer und darf melden, was sie will (Aufgabe 19).
    #[test]
    fn ein_fremder_zustand_ist_neutral() {
        for fremd in [
            r#"{"zustand":"am-geraet-aus"}"#,
            r#"{"zustand":"erfunden","was":1}"#,
            r#"{"zustand":"ausser-betrieb"}"#,
        ] {
            let z: EintragZustand = serde_json::from_str(fremd).expect("wird gelesen");
            assert!(
                matches!(z, EintragZustand::AmGeraetAus | EintragZustand::Unbekannt),
                "{fremd} → {z:?}"
            );
        }
        // Eine Meldung mit fremdem Zustand lässt sich lesen (der Vertrag).
        let meldung: Meldung = serde_json::from_str(
            r#"{"revision":"r1","client_version":"0.1.0","eintraege":[["echo",{"zustand":"von-morgen"}]]}"#,
        )
        .expect("Meldung wird gelesen");
        assert_eq!(meldung.eintraege[0].1, EintragZustand::Unbekannt);
    }
}
