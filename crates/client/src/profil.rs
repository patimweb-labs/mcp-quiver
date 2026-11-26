// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was der Client in `<profil>/zustand.json` behält.
//!
//! Nur, was einen Neustart überleben muss: Server-Adresse, das eigene Gerät,
//! die lokalen Werte, die gekoppelten Agenten (mit Token-Hash) und der letzte
//! Soll-Zustand, damit ein Gerät offline weiterläuft.
//!
//! Geheimnisse stehen **nicht** hier, sondern im [`Tresor`](crate::Tresor).

use std::collections::{BTreeMap, BTreeSet};
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
    /// Anzeigename des angemeldeten Benutzers („Angemeldet als …“). Kein
    /// Geheimnis; dient nur der Anzeige und überlebt einen Neustart.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub benutzer: Option<String>,
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
    /// Einträge, die der User **auf diesem Gerät** abgeschaltet hat
    /// (Aufgabe 19). Die Wahl überlebt Neustart, Abgleich und Import; nichts
    /// startet einen Eintrag, der hier steht. `serde(default)` hält alte
    /// Dateien lesbar.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub am_geraet_aus: BTreeSet<String>,
    /// Frist je Werkzeugaufruf in Sekunden, je Eintrag (Paket 3). Fehlt ein
    /// Eintrag, gelten 120 s; erlaubt sind 1–900 s.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub aufruf_fristen: BTreeMap<String, u64>,
    /// Der frühere Merker für „später“ und einen gescheiterten Tausch
    /// (Aufgabe 34). Der Selbst-Tausch ist entfallen; das Feld wird nur noch
    /// gelesen, damit eine vorhandene `zustand.json` weiter lesbar bleibt, und
    /// beim nächsten Schreiben weggelassen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update: Option<serde_json::Value>,
}

/// Nur schreiben, wenn der Schalter an ist.
fn is_false(wert: &bool) -> bool {
    !*wert
}

impl Profil {
    /// Wie [`Profil::laden`], aber mit dem Grund, warum ein vorhandener Stand
    /// nicht gelesen werden konnte. Der Client startet trotzdem – und sagt es
    /// im Fenster. Die kaputte Datei bleibt unter `.kaputt-<zeit>` erhalten
    /// und wird **nicht** stillschweigend durch einen leeren Stand ersetzt.
    pub fn laden_mit_hinweis(ordner: &Path) -> (Self, Option<String>) {
        let pfad = ordner.join("zustand.json");
        let text = match std::fs::read_to_string(&pfad) {
            Ok(text) => text,
            Err(fehler) if fehler.kind() == std::io::ErrorKind::NotFound => {
                return (Self::default(), None)
            }
            Err(fehler) => {
                return (
                    Self::default(),
                    Some(format!("Der gespeicherte Stand ist nicht lesbar: {fehler}")),
                )
            }
        };
        match serde_json::from_str(&text) {
            Ok(profil) => (profil, None),
            Err(fehler) => {
                // Beiseitelegen statt überschreiben: die alte Datei bleibt
                // lesbar, und der nächste Speichervorgang beginnt auf einem
                // leeren Blatt.
                let ziel = kaputt_pfad(&pfad);
                let hinweis = match std::fs::rename(&pfad, &ziel) {
                    Ok(()) => format!(
                        "Der gespeicherte Stand war nicht lesbar und liegt als {} daneben",
                        ziel.file_name()
                            .map(|name| name.to_string_lossy().to_string())
                            .unwrap_or_default()
                    ),
                    Err(rename) => format!(
                        "Der gespeicherte Stand ist nicht lesbar ({fehler}); er ließ sich nicht beiseitelegen: {rename}"
                    ),
                };
                tracing::warn!("{hinweis}");
                (Self::default(), Some(hinweis))
            }
        }
    }

    /// Datei schreiben: atomar, mit 0600 im Ordner 0700.
    ///
    /// Der Zwischenweg über [`crate::rechte::schreiben_atomar`] sorgt dafür,
    /// dass die Datei nie halb und nie für andere lesbar auf der Platte liegt.
    pub fn speichern(&self, ordner: &Path) -> anyhow::Result<()> {
        let ziel = ordner.join("zustand.json");
        let text = serde_json::to_vec_pretty(self)?;
        crate::rechte::schreiben_atomar(&ziel, &text)
    }
}

/// `zustand.json.kaputt-20260102T030405Z`.
fn kaputt_pfad(pfad: &Path) -> PathBuf {
    let mut name = pfad.file_name().unwrap_or_default().to_os_string();
    name.push(format!(".kaputt-{}", stempel()));
    pfad.with_file_name(name)
}

/// Ein Zeitstempel für Dateinamen.
fn stempel() -> String {
    let sekunden = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|dauer| dauer.as_secs())
        .unwrap_or_default();
    let tage = i64::try_from(sekunden / 86_400).unwrap_or_default();
    let rest = sekunden % 86_400;
    let (jahr, monat, tag) = zivil(tage);
    format!(
        "{jahr:04}{monat:02}{tag:02}T{:02}{:02}{:02}Z",
        rest / 3600,
        (rest % 3600) / 60,
        rest % 60
    )
}

/// Tage seit 1970 → Jahr, Monat, Tag (Howard Hinnant).
fn zivil(tage: i64) -> (i64, u32, u32) {
    let z = tage + 719_468;
    let aera = z.div_euclid(146_097);
    let jahreszeit = z.rem_euclid(146_097);
    let jahr_des_zeitalters =
        (jahreszeit - jahreszeit / 1_460 + jahreszeit / 36_524 - jahreszeit / 146_096) / 365;
    let jahr = jahr_des_zeitalters + aera * 400;
    let tag_des_jahres = jahreszeit
        - (365 * jahr_des_zeitalters + jahr_des_zeitalters / 4 - jahr_des_zeitalters / 100);
    let monatsteil = (5 * tag_des_jahres + 2) / 153;
    let tag = u32::try_from(tag_des_jahres - (153 * monatsteil + 2) / 5 + 1).unwrap_or_default();
    let monat = u32::try_from(if monatsteil < 10 {
        monatsteil + 3
    } else {
        monatsteil - 9
    })
    .unwrap_or_default();
    (jahr + i64::from(monat <= 2), monat, tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ordner() -> PathBuf {
        // Zähler **und** Zeitstempel: zwei Tests im selben Prozess dürfen sich
        // niemals denselben Ordner teilen – sonst löscht einer dem anderen die
        // Dateien unter den Händen weg.
        use std::sync::atomic::{AtomicUsize, Ordering};
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let stempel = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let pfad = std::env::temp_dir().join(format!(
            "quiver-client-profil-{}-{nummer}-{stempel}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    #[test]
    fn rundlauf() {
        let ordner = ordner();
        let (mut profil, hinweis) = Profil::laden_mit_hinweis(&ordner);
        assert_eq!(hinweis, None, "ein leerer Ordner ist kein Fehler");
        assert_eq!(profil.server_url, None);
        profil.server_url = Some("http://127.0.0.1:8080".into());
        profil.benutzer = Some("test@example.test".into());
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
        profil.am_geraet_aus.insert("graph".into());
        profil.speichern(&ordner).expect("schreibbar");

        let (zurueck, hinweis) = Profil::laden_mit_hinweis(&ordner);
        assert_eq!(hinweis, None, "ein lesbarer Stand meldet nichts");
        assert_eq!(zurueck.server_url, profil.server_url);
        assert_eq!(zurueck.benutzer, profil.benutzer);
        assert_eq!(zurueck.lokale_werte, profil.lokale_werte);
        assert_eq!(zurueck.zugaenge, profil.zugaenge);
        assert_eq!(zurueck.agenten, profil.agenten);
        assert!(zurueck.protokoll_fuer_agenten, "der Schalter bleibt");
        assert_eq!(
            zurueck.am_geraet_aus, profil.am_geraet_aus,
            "die Wahl am Gerät bleibt (Aufgabe 19)"
        );
        std::fs::remove_dir_all(&ordner).ok();
    }

    /// Eine Datei von vor Aufgabe 19 kennt `am_geraet_aus` nicht – sie muss
    /// weiter lesbar sein.
    #[test]
    fn eine_alte_datei_ohne_abschaltung_ist_lesbar() {
        let ordner = ordner();
        std::fs::write(
            ordner.join("zustand.json"),
            r#"{"server_url":"http://127.0.0.1:8080","protokoll_fuer_agenten":true}"#,
        )
        .expect("schreibbar");
        let (profil, hinweis) = Profil::laden_mit_hinweis(&ordner);
        assert_eq!(hinweis, None);
        assert_eq!(profil.server_url.as_deref(), Some("http://127.0.0.1:8080"));
        assert!(profil.am_geraet_aus.is_empty(), "nichts ist abgeschaltet");
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn kaputte_datei_stoppt_den_start_nicht() {
        let ordner = ordner();
        std::fs::write(ordner.join("zustand.json"), "kein json").expect("schreibbar");
        let (profil, hinweis) = Profil::laden_mit_hinweis(&ordner);
        let hinweis = hinweis.expect("der Grund steht im Hinweis");
        assert!(hinweis.contains("nicht lesbar"), "{hinweis}");
        assert_eq!(profil.server_url, None);
        std::fs::remove_dir_all(&ordner).ok();
    }

    // ---------------------------------------------------------- Paket 3

    /// Der Zwischenname trägt PID **und** Zufall: zwei Aufgaben im selben
    /// Prozess dürfen nie dieselbe Zwischendatei nutzen.
    #[test]
    fn der_zwischenname_hat_pid_und_zufall() {
        let ziel = PathBuf::from("/tmp/zustand.json");
        let a = crate::rechte::zwischenpfad(&ziel);
        let b = crate::rechte::zwischenpfad(&ziel);
        assert_ne!(a, b, "zwei Namen sind verschieden");
        let name = a.file_name().unwrap().to_string_lossy().to_string();
        assert!(
            name.starts_with(&format!("zustand.json.{}.", std::process::id())),
            "{name}"
        );
        assert!(name.ends_with(".neu"), "{name}");
    }

    /// Eine kaputte Datei wird beiseitegelegt (`.kaputt-<zeit>`) und **nicht**
    /// stillschweigend durch einen leeren Stand ersetzt.
    #[test]
    fn eine_kaputte_datei_bleibt_als_kaputt_liegen() {
        let ordner = ordner();
        std::fs::write(ordner.join("zustand.json"), "kein json").expect("schreibbar");
        let (profil, hinweis) = Profil::laden_mit_hinweis(&ordner);
        let hinweis = hinweis.expect("Hinweis");
        assert!(hinweis.contains("nicht lesbar"), "{hinweis}");
        assert_eq!(profil.server_url, None);
        assert!(
            !ordner.join("zustand.json").exists(),
            "die kaputte Datei steht nicht mehr im Weg"
        );
        let kaputt: Vec<_> = std::fs::read_dir(&ordner)
            .unwrap()
            .flatten()
            .map(|eintrag| eintrag.file_name().to_string_lossy().to_string())
            .filter(|name| name.starts_with("zustand.json.kaputt-"))
            .collect();
        assert_eq!(kaputt.len(), 1, "{kaputt:?}");
        assert_eq!(
            std::fs::read_to_string(ordner.join(&kaputt[0])).unwrap(),
            "kein json",
            "der Inhalt bleibt erhalten"
        );
        std::fs::remove_dir_all(&ordner).ok();
    }

    /// Die Fristen je Aufruf stehen im Profil (Paket 3) und überleben einen
    /// Neustart.
    #[test]
    fn aufruf_fristen_stehen_im_profil() {
        let ordner = ordner();
        let mut profil = Profil::default();
        profil.aufruf_fristen.insert("echo".into(), 300);
        profil.speichern(&ordner).expect("schreibbar");
        let (zurueck, hinweis) = Profil::laden_mit_hinweis(&ordner);
        assert_eq!(hinweis, None);
        assert_eq!(zurueck.aufruf_fristen.get("echo"), Some(&300));
        std::fs::remove_dir_all(&ordner).ok();
    }
}
