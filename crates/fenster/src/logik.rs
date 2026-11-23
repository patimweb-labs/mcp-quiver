// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Reine Funktionen der Oberfläche (AGENTS.md §2.4): welche Seite gilt,
//! welche Farbe ein Zustand hat, was in einem Feld steht, welche Schnipsel
//! ein Agent bekommt. Ohne egui, mit Tests — die Ansichten rufen nur hier auf.

use quiver_client::{EintragSicht, Modell, Verbindung};
use quiver_domain::{FehlendesFeld, Quelle, Werte};
use quiver_protocol::EintragZustand;

use crate::modell::{Feldeingabe, Seite};
use crate::texte::{t, tf, Sprache};
use crate::thema::Ton;

/// Die Startseite: solange kein Server eingebunden ist, führt der Weg über
/// die Verbindung; danach steht die Arbeit (Tools) vorn.
pub fn startseite(modell: &Modell) -> Seite {
    match modell.verbindung {
        Verbindung::NichtEingebunden => Seite::Verbindung,
        _ => Seite::Tools,
    }
}

/// Welche Seite nach einem Wechsel der Verbindung gilt. Fällt die Verbindung
/// auf „nicht eingebunden" zurück, ist die Verbindungsseite die einzige, die
/// noch etwas anbietet; gelingt das Einbinden, geht es zu den Tools.
pub fn seite_nach_wechsel(vorher: &Verbindung, nachher: &Verbindung, aktuell: Seite) -> Seite {
    match nachher {
        Verbindung::NichtEingebunden => Seite::Verbindung,
        _ if matches!(vorher, Verbindung::NichtEingebunden) => Seite::Tools,
        _ => aktuell,
    }
}

/// Bedeutung des Zustands eines Eintrags (ok, warn, error, leise).
pub fn zustand_ton(zustand: &EintragZustand) -> Ton {
    match zustand {
        EintragZustand::Laeuft { .. } => Ton::Gut,
        EintragZustand::WartetAufWerte { .. } => Ton::Warnung,
        EintragZustand::Startet => Ton::Leise,
        EintragZustand::Fehler { .. } => Ton::Fehler,
    }
}

/// Das Abzeichen eines Eintrags. Beim Fehler steht hier nur „Fehler"; den
/// Text zeigt die Karte darunter (er kann lang sein).
pub fn zustand_text(sprache: Sprache, zustand: &EintragZustand) -> String {
    match zustand {
        EintragZustand::Laeuft { tools } => tf(sprache, "tools.laeuft", &[tools]),
        EintragZustand::WartetAufWerte { felder } if !felder.is_empty() => {
            tf(sprache, "tools.wartet_mit", &[&felder.join(", ")])
        }
        EintragZustand::WartetAufWerte { .. } => t(sprache, "tools.wartet"),
        EintragZustand::Startet => t(sprache, "tools.startet"),
        EintragZustand::Fehler { .. } => t(sprache, "tools.fehler"),
    }
}

/// Der Fehlertext eines Eintrags, falls es einen gibt.
pub fn fehlertext(zustand: &EintragZustand) -> Option<&str> {
    match zustand {
        EintragZustand::Fehler { text } => Some(text),
        _ => None,
    }
}

/// Bedeutung und Text der Verbindung für Kopf und Statuskarte.
pub fn verbindung_status(sprache: Sprache, verbindung: &Verbindung) -> (Ton, String) {
    match verbindung {
        Verbindung::NichtEingebunden => (Ton::Blass, t(sprache, "verbindung.kein_server")),
        Verbindung::Verbindet { server } => {
            (Ton::Warnung, tf(sprache, "verbindung.verbindet", &[server]))
        }
        Verbindung::Verbunden { server, .. } => {
            (Ton::Gut, tf(sprache, "verbindung.verbunden", &[server]))
        }
        Verbindung::Getrennt { server, .. } => (
            Ton::Fehler,
            tf(sprache, "verbindung.getrennt_von", &[server]),
        ),
    }
}

/// Was einem Eintrag fehlt, als ein Satz: die Erklärung des Felds, sonst sein
/// Name. (Der Name steht schon im Abzeichen.)
pub fn fehlende_texte(fehlende: &[FehlendesFeld]) -> String {
    fehlende
        .iter()
        .map(|feld| {
            if feld.beschreibung.trim().is_empty() {
                feld.name.clone()
            } else {
                feld.beschreibung.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Die Felder eines Eintrags als Eingaben: lokale Werte vorbelegt, Geheimnisse
/// leer (gesetzte zeigen „••••" nur als Hinweis).
pub fn eingaben(eintrag: &EintragSicht) -> Vec<Feldeingabe> {
    eintrag
        .felder
        .iter()
        .map(|feld| {
            let geheim = feld.quelle == Quelle::Geheim;
            Feldeingabe {
                name: feld.name.clone(),
                beschreibung: feld.beschreibung.clone(),
                geheim,
                pflicht: feld.pflicht,
                gesetzt: geheim && eintrag.geheim_gesetzt.contains(&feld.name),
                wert: if geheim {
                    String::new()
                } else {
                    eintrag
                        .lokale_werte
                        .get(&feld.name)
                        .cloned()
                        .unwrap_or_default()
                },
            }
        })
        .collect()
}

/// Die Eingaben aufteilen in lokale und geheime Werte. Ein leeres Geheimnis
/// heißt „unverändert" und wird nicht geschickt; ein leerer lokaler Wert
/// löscht das Feld am Gerät.
pub fn werte_splitten(eingaben: &[Feldeingabe]) -> (Werte, Werte) {
    let mut lokal = Werte::new();
    let mut geheim = Werte::new();
    for eingabe in eingaben {
        let wert = eingabe.wert.trim().to_string();
        if eingabe.geheim {
            if !wert.is_empty() {
                geheim.insert(eingabe.name.clone(), wert);
            }
        } else {
            lokal.insert(eingabe.name.clone(), wert);
        }
    }
    (lokal, geheim)
}

/// Der Befehl für pi 1.5 (`pi mcp add --help`: `--header <KEY=VALUE>`).
pub fn schnipsel_pi(endpunkt: &str, token: &str) -> String {
    format!("pi mcp add quiver --url {endpunkt} --header \"Authorization=Bearer {token}\"")
}

/// Dasselbe als JSON für andere MCP-Clients.
pub fn schnipsel_json(endpunkt: &str, token: &str) -> String {
    format!(
        "{{\"mcpServers\":{{\"quiver\":{{\"type\":\"http\",\"url\":\"{endpunkt}\",\"headers\":{{\"Authorization\":\"Bearer {token}\"}}}}}}}}"
    )
}

/// Name, unter dem sich das Gerät beim Server vorstellt. Ohne
/// Plattformabfrage (AGENTS.md §2.10): erst eine eigene Umgebungsvariable,
/// dann, was die Umgebung ohnehin mitbringt.
pub fn geraetename() -> String {
    geraetename_aus(|name| std::env::var(name).ok())
}

/// Wie [`geraetename`], aber mit einer nachschlagbaren Umgebung (Test).
pub fn geraetename_aus(lesen: impl Fn(&str) -> Option<String>) -> String {
    for name in ["MCP_QUIVER_GERAET", "HOSTNAME", "COMPUTERNAME"] {
        if let Some(wert) = lesen(name) {
            let wert = wert.trim();
            if !wert.is_empty() {
                return wert.to_string();
            }
        }
    }
    "quiver".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_client::AgentSicht;
    use quiver_domain::{EintragId, Feld};

    fn modell(verbindung: Verbindung) -> Modell {
        Modell {
            verbindung,
            eintraege: Vec::new(),
            agenten: vec![AgentSicht {
                id: "a1".into(),
                name: "pi".into(),
                verbunden: true,
            }],
            endpunkt: "http://127.0.0.1:8741/mcp".into(),
        }
    }

    fn verbunden() -> Verbindung {
        Verbindung::Verbunden {
            server: "http://quiver.example".into(),
            geraet: "rechner".into(),
        }
    }

    fn eintrag() -> EintragSicht {
        EintragSicht {
            id: EintragId::neu("echo"),
            titel: "Echo".into(),
            beschreibung: "Werkzeuge zum Ausprobieren.".into(),
            zustand: EintragZustand::WartetAufWerte {
                felder: vec!["PFAD".into()],
            },
            fehlende: Vec::new(),
            felder: vec![
                Feld {
                    name: "PFAD".into(),
                    beschreibung: "Ordner".into(),
                    quelle: Quelle::Client,
                    pflicht: true,
                    standard: None,
                },
                Feld {
                    name: "SCHLUESSEL".into(),
                    beschreibung: "Geheimnis".into(),
                    quelle: Quelle::Geheim,
                    pflicht: false,
                    standard: None,
                },
            ],
            lokale_werte: Werte::from([("PFAD".to_string(), "/tmp".to_string())]),
            geheim_gesetzt: vec!["SCHLUESSEL".into()],
            tools: vec!["echo".into()],
        }
    }

    #[test]
    fn startseite_ohne_server_ist_die_verbindung() {
        assert_eq!(
            startseite(&modell(Verbindung::NichtEingebunden)),
            Seite::Verbindung
        );
        assert_eq!(startseite(&modell(verbunden())), Seite::Tools);
        assert_eq!(
            startseite(&modell(Verbindung::Getrennt {
                server: "http://x".into(),
                fehler: "weg".into(),
            })),
            Seite::Tools,
            "getrennt heißt nicht ungebunden"
        );
    }

    #[test]
    fn seite_folgt_der_verbindung() {
        let zu = verbunden();
        // Gerade eingebunden: ab zu den Tools.
        assert_eq!(
            seite_nach_wechsel(&Verbindung::NichtEingebunden, &zu, Seite::Verbindung),
            Seite::Tools
        );
        // Abgemeldet: zurück zur Verbindung, egal wo man war.
        assert_eq!(
            seite_nach_wechsel(&zu, &Verbindung::NichtEingebunden, Seite::Agenten),
            Seite::Verbindung
        );
        // Sonst bleibt die gewählte Seite.
        assert_eq!(seite_nach_wechsel(&zu, &zu, Seite::Agenten), Seite::Agenten);
    }

    #[test]
    fn drei_zustaende_drei_bedeutungen() {
        let sprache = Sprache::De;
        let laeuft = EintragZustand::Laeuft { tools: 3 };
        let wartet = EintragZustand::WartetAufWerte {
            felder: vec!["PFAD".into(), "KEY".into()],
        };
        let fehler = EintragZustand::Fehler {
            text: "kaputt".into(),
        };
        assert_eq!(zustand_ton(&laeuft), Ton::Gut);
        assert_eq!(zustand_ton(&wartet), Ton::Warnung);
        assert_eq!(zustand_ton(&fehler), Ton::Fehler);
        assert_eq!(zustand_ton(&EintragZustand::Startet), Ton::Leise);
        assert_eq!(zustand_text(sprache, &laeuft), "Läuft · 3 Tools");
        assert_eq!(zustand_text(sprache, &wartet), "Braucht Angaben: PFAD, KEY");
        assert_eq!(zustand_text(sprache, &fehler), "Fehler");
        assert_eq!(fehlertext(&fehler), Some("kaputt"));
        assert_eq!(fehlertext(&laeuft), None);
        // Eine leere Feldliste bleibt beim kurzen Text.
        assert_eq!(
            zustand_text(
                sprache,
                &EintragZustand::WartetAufWerte { felder: Vec::new() }
            ),
            "Braucht Angaben"
        );
    }

    #[test]
    fn verbindung_in_worten() {
        let (ton, text) = verbindung_status(Sprache::De, &verbunden());
        assert_eq!(ton, Ton::Gut);
        assert_eq!(text, "Verbunden mit http://quiver.example");
        let (ton, _) = verbindung_status(
            Sprache::De,
            &Verbindung::Getrennt {
                server: "http://x".into(),
                fehler: "weg".into(),
            },
        );
        assert_eq!(ton, Ton::Fehler);
    }

    #[test]
    fn fehlendes_wird_erklaert() {
        let feld = |name: &str, beschreibung: &str| FehlendesFeld {
            name: name.into(),
            beschreibung: beschreibung.into(),
            quelle: Quelle::Client,
        };
        assert_eq!(
            fehlende_texte(&[feld("PFAD", "Ordner, den das Tool lesen darf")]),
            "Ordner, den das Tool lesen darf"
        );
        assert_eq!(fehlende_texte(&[feld("PFAD", "  ")]), "PFAD", "ohne Text");
        assert_eq!(
            fehlende_texte(&[feld("A", "eins"), feld("B", "zwei")]),
            "eins · zwei"
        );
        assert_eq!(fehlende_texte(&[]), "");
    }

    #[test]
    fn eingaben_vorbelegt_und_geheim_leer() {
        let eingaben = eingaben(&eintrag());
        assert_eq!(eingaben.len(), 2);
        assert_eq!(eingaben[0].name, "PFAD");
        assert_eq!(eingaben[0].wert, "/tmp");
        assert!(!eingaben[0].geheim);
        assert_eq!(eingaben[1].name, "SCHLUESSEL");
        assert!(eingaben[1].geheim);
        assert_eq!(eingaben[1].wert, "", "Geheimes wird nie angezeigt");
        assert!(eingaben[1].gesetzt);
    }

    #[test]
    fn leeres_geheimnis_bleibt_unveraendert() {
        let mut eingaben = eingaben(&eintrag());
        eingaben[0].wert = " /var/lib ".into();
        eingaben[1].wert = String::new();
        let (lokal, geheim) = werte_splitten(&eingaben);
        assert_eq!(lokal.get("PFAD").map(String::as_str), Some("/var/lib"));
        assert!(geheim.is_empty(), "nichts überschreiben");
        // Mit Wert wird es geschickt — und ein leerer lokaler Wert löscht.
        eingaben[1].wert = "s3cret".into();
        eingaben[0].wert = String::new();
        let (lokal, geheim) = werte_splitten(&eingaben);
        assert_eq!(lokal.get("PFAD").map(String::as_str), Some(""));
        assert_eq!(geheim.get("SCHLUESSEL").map(String::as_str), Some("s3cret"));
    }

    #[test]
    fn schnipsel_tragen_adresse_und_token() {
        let pi = schnipsel_pi("http://127.0.0.1:8741/mcp", "abc123");
        assert_eq!(
            pi,
            "pi mcp add quiver --url http://127.0.0.1:8741/mcp \
             --header \"Authorization=Bearer abc123\""
        );
        let json = schnipsel_json("http://127.0.0.1:8741/mcp", "abc123");
        assert_eq!(
            json,
            "{\"mcpServers\":{\"quiver\":{\"type\":\"http\",\"url\":\
             \"http://127.0.0.1:8741/mcp\",\"headers\":\
             {\"Authorization\":\"Bearer abc123\"}}}}"
        );
        // Gültiges JSON, nicht nur richtig aussehend.
        let wert: serde_json::Value = serde_json::from_str(&json).expect("JSON");
        assert_eq!(
            wert["mcpServers"]["quiver"]["url"],
            "http://127.0.0.1:8741/mcp"
        );
    }

    #[test]
    fn geraetename_kommt_aus_der_umgebung() {
        let leer = |_: &str| None;
        assert_eq!(geraetename_aus(leer), "quiver");
        let mit = |name: &str| (name == "HOSTNAME").then(|| "  rechner ".to_string());
        assert_eq!(geraetename_aus(mit), "rechner");
        let eigen = |name: &str| match name {
            "MCP_QUIVER_GERAET" => Some("labor".to_string()),
            "HOSTNAME" => Some("rechner".to_string()),
            _ => None,
        };
        assert_eq!(geraetename_aus(eigen), "labor", "die eigene gilt zuerst");
    }
}
