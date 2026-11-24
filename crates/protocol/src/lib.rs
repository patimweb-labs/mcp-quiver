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
//! | GET | `/geraet/soll` | Gerät | → [`quiver_domain::Soll`] |
//! | GET | `/geraet/ereignisse` | Gerät | SSE, Ereignis `soll`, Daten = Revision |
//! | POST | `/geraet/meldung` | Gerät | [`Meldung`] → 204 |
//! | GET/PUT/DELETE | `/admin/eintraege[/{id}]` | Admin | [`quiver_domain::Eintrag`] |
//! | POST | `/admin/eintraege/import` | Admin | server.json → [`quiver_domain::Eintrag`] |
//! | GET/PUT/DELETE | `/admin/saetze[/{id}]` | Admin | [`quiver_domain::Satz`] |
//! | GET/PUT/DELETE | `/admin/gruppen[/{id}]` | Admin | [`quiver_domain::Gruppe`] |
//! | GET | `/admin/geraete` | Admin | → `Vec<`[`GeraetStand`]`>` |
//! | PUT/DELETE | `/admin/geraete/{id}` | Admin | [`GeraetAendern`] → [`GeraetStand`] |
//! | POST | `/admin/codes` | Admin | [`CodeAnlegen`] → [`Code`] |
//! | GET | `/admin/ueberblick` | Admin | → [`Ueberblick`] |
//!
//! PUT legt an oder ersetzt (die Kennung steht im Pfad und im Rumpf, beide
//! müssen gleich sein). Fehler kommen als [`Fehler`] mit passendem Status.

use quiver_domain::{EintragId, Geraet, GruppeId};
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

/// Ein Eintrag der Prüfspur (ohne Geheimnisse).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruefspurEintrag {
    pub zeit: String,
    pub art: String,
    pub akteur: String,
    pub text: String,
}

/// Eine in Anmeldungen gesehene Entra-Gruppe; der Anzeigename kommt vom Admin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntraGruppe {
    pub id: String,
    pub anzeigename: Option<String>,
    pub zuletzt: String,
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
