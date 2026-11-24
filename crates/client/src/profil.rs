// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was der Client in `<profil>/zustand.json` behält.
//!
//! Nur, was einen Neustart überleben muss: Server-Adresse, das eigene Gerät,
//! die lokalen Werte, die gekoppelten Agenten (mit Token-Hash) und der letzte
//! Soll-Zustand, damit ein Gerät offline weiterläuft.
//!
//! Geheimnisse stehen **nicht** hier, sondern im [`Tresor`](crate::Tresor).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use quiver_domain::{Geraet, Soll, Werte};
use serde::{Deserialize, Serialize};

/// Ein gekoppelter Agent. Vom Token steht hier nur der Hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub name: String,
    pub token_hash: String,
}

/// Der gespeicherte Stand.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Profil {
    /// Adresse des Servers, z. B. `http://127.0.0.1:8080`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    /// Das eigene Gerät (kommt aus der Anmeldung).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geraet: Option<Geraet>,
    /// Werte von Feldern mit `Quelle::Client`, je Eintrag.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub lokale_werte: BTreeMap<String, Werte>,
    /// Zugänge je Eintrag (`mehrfach`, Aufgabe 14): Eintrag → Name →
    /// nicht-geheime Werte. Die Geheimnisse liegen im
    /// [`Tresor`](crate::Tresor).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub zugaenge: BTreeMap<String, BTreeMap<String, Werte>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub agenten: Vec<Agent>,
    /// Letzter Soll-Zustand – läuft offline weiter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soll: Option<Soll>,
    /// Dürfen Agenten Inhalte im Protokoll sehen (Aufgabe 17)? Standard: aus.
    #[serde(default, skip_serializing_if = "is_false")]
    pub protokoll_fuer_agenten: bool,
}

/// Nur schreiben, wenn der Schalter an ist.
fn is_false(wert: &bool) -> bool {
    !*wert
}

impl Profil {
    /// Datei lesen; fehlt sie, ist der Stand leer.
    pub fn laden(ordner: &Path) -> anyhow::Result<Self> {
        let pfad = ordner.join("zustand.json");
        match std::fs::read_to_string(&pfad) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(profil) => Ok(profil),
                Err(fehler) => {
                    // Nicht scheitern: ein kaputter Stand darf den Client nicht
                    // am Starten hindern. Wir fangen mit einem leeren Stand an.
                    tracing::warn!("{pfad:?} ist unlesbar, fange leer an: {fehler}");
                    Ok(Self::default())
                }
            },
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(fehler) => Err(fehler.into()),
        }
    }

    /// Datei schreiben (erst nebenhin, dann umbenennen).
    pub fn speichern(&self, ordner: &Path) -> anyhow::Result<()> {
        std::fs::create_dir_all(ordner)?;
        let ziel = ordner.join("zustand.json");
        let zwischen = zwischenpfad(&ziel);
        let text = serde_json::to_vec_pretty(self)?;
        std::fs::write(&zwischen, text)?;
        std::fs::rename(&zwischen, &ziel)?;
        Ok(())
    }
}

fn zwischenpfad(ziel: &Path) -> PathBuf {
    let mut name = ziel.file_name().unwrap_or_default().to_os_string();
    name.push(".neu");
    ziel.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner() -> PathBuf {
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let pfad = std::env::temp_dir().join(format!(
            "quiver-client-profil-{}-{stempel}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    #[test]
    fn rundlauf() {
        let ordner = ordner();
        let mut profil = Profil::laden(&ordner).expect("leer lesbar");
        assert_eq!(profil.server_url, None);
        profil.server_url = Some("http://127.0.0.1:8080".into());
        profil.lokale_werte.insert(
            "echo".into(),
            Werte::from([("PFAD".to_string(), "/tmp".to_string())]),
        );
        profil.zugaenge.insert(
            "ssh".into(),
            BTreeMap::from([(
                "web-01".to_string(),
                Werte::from([("host".to_string(), "web-01.example".to_string())]),
            )]),
        );
        profil.agenten.push(Agent {
            id: "a1".into(),
            name: "pi".into(),
            token_hash: "00".into(),
        });
        profil.protokoll_fuer_agenten = true;
        profil.speichern(&ordner).expect("schreibbar");

        let zurueck = Profil::laden(&ordner).expect("lesbar");
        assert_eq!(zurueck.server_url, profil.server_url);
        assert_eq!(zurueck.lokale_werte, profil.lokale_werte);
        assert_eq!(zurueck.zugaenge, profil.zugaenge);
        assert_eq!(zurueck.agenten, profil.agenten);
        assert!(zurueck.protokoll_fuer_agenten, "der Schalter bleibt");
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn kaputte_datei_stoppt_den_start_nicht() {
        let ordner = ordner();
        std::fs::write(ordner.join("zustand.json"), "kein json").expect("schreibbar");
        let profil = Profil::laden(&ordner).expect("wird nicht zum Fehler");
        assert_eq!(profil.server_url, None);
        std::fs::remove_dir_all(&ordner).ok();
    }
}
