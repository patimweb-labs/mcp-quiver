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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Code {
    /// Kurz und abtippbar, z. B. `K7QM-3XPA`.
    pub code: String,
    /// RFC 3339.
    pub gueltig_bis: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeraetAendern {
    pub name: String,
    pub gruppen: Vec<GruppeId>,
    pub gesperrt: bool,
}

/// Zustand eines Eintrags auf dem Gerät, wie der Client ihn meldet.
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
    Fehler {
        text: String,
    },
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

/// Fehlerantwort.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fehler {
    pub fehler: String,
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
        let z: EintragZustand =
            serde_json::from_str(r#"{"zustand":"wartet-auf-werte","felder":["pfad"]}"#).unwrap();
        assert_eq!(
            z,
            EintragZustand::WartetAufWerte {
                felder: vec!["pfad".into()]
            }
        );
    }
}
