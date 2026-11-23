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

/// Ein Eintrag des Tray-Menüs. Beide Plattform-Teile (macOS/Windows und Linux)
/// zeichnen dieselbe Liste, deshalb steht sie hier als Daten und nicht dort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEintrag {
    /// Zustandszeile, nicht klickbar.
    Zustand(String),
    /// „Fenster öffnen".
    Zeigen(String),
    /// „Beim Anmelden starten" mit Häkchen.
    Autostart {
        text: String,
        an: bool,
    },
    Trenner,
    /// „Beenden".
    Beenden(String),
}

/// Die Zustandszeile des Trays: „Verbunden · 4 Tools · 1 Agent" bzw.
/// „Nicht verbunden". Gezählt wird, was gerade läuft und wer gerade da ist.
pub fn tray_zeile(modell: &Modell, sprache: Sprache) -> String {
    match &modell.verbindung {
        Verbindung::NichtEingebunden => t(sprache, "tray.nicht_verbunden"),
        Verbindung::Verbindet { .. } => t(sprache, "tray.verbindet"),
        Verbindung::Getrennt { .. } => t(sprache, "tray.getrennt"),
        Verbindung::Verbunden { .. } => {
            let tools: usize = modell
                .eintraege
                .iter()
                .map(|eintrag| eintrag.tools.len())
                .sum();
            let agenten = modell
                .agenten
                .iter()
                .filter(|agent| agent.verbunden)
                .count();
            let agenten = if agenten == 1 {
                t(sprache, "tray.agent_einer")
            } else {
                tf(sprache, "tray.agenten", &[&agenten])
            };
            format!(
                "{} · {} · {agenten}",
                t(sprache, "tray.verbunden"),
                tf(sprache, "tray.tools", &[&tools])
            )
        }
    }
}

/// Das Tray-Menü als Daten — reine Funktion über dem Modell. Die Reihenfolge
/// ist die des Menüs: Zustand, Fenster zeigen, Autostart, Trenner, Beenden.
pub fn tray_menu(modell: &Modell, autostart: bool, sprache: Sprache) -> Vec<TrayEintrag> {
    vec![
        TrayEintrag::Zustand(tray_zeile(modell, sprache)),
        TrayEintrag::Zeigen(t(sprache, "tray.fenster")),
        TrayEintrag::Autostart {
            text: t(sprache, "tray.autostart"),
            an: autostart,
        },
        TrayEintrag::Trenner,
        TrayEintrag::Beenden(t(sprache, "tray.beenden")),
    ]
}

/// Was auf der Kommandozeile steht.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Argumente {
    /// `--hintergrund`: nur Tray und Kern, das Fenster erst auf Wunsch.
    pub hintergrund: bool,
    /// `--headless`: nur der Kern, ohne Fenster und ohne Tray.
    pub kopflos: bool,
}

/// Die Argumente lesen. Unbekanntes wird nicht bemängelt — eine Anwendung mit
/// Tray darf ein zusätzliches `--flag` nicht am Start hindern.
pub fn argumente<S: AsRef<str>>(argumente: impl IntoIterator<Item = S>) -> Argumente {
    let mut werte = Argumente::default();
    for argument in argumente {
        match argument.as_ref() {
            "--hintergrund" => werte.hintergrund = true,
            "--headless" => werte.kopflos = true,
            _ => {}
        }
    }
    werte
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

/// Der Befehl für pi 1.5 (`pi mcp add --help`: `--url <url>`, `--header <KEY=VALUE>`).
pub fn schnipsel_pi(endpunkt: &str, token: &str) -> String {
    format!("pi mcp add quiver --url {endpunkt} --header \"Authorization=Bearer {token}\"")
}

/// Der Befehl für Claude Code (`claude mcp add --help`: `--transport http`,
/// `--header <header>`).
pub fn schnipsel_claude(endpunkt: &str, token: &str) -> String {
    format!(
        "claude mcp add --transport http quiver {endpunkt} --header \"Authorization: Bearer {token}\""
    )
}

/// Die Einstellung für OpenCode (`opencode.json`, Schlüssel `mcp`).
pub fn schnipsel_opencode(endpunkt: &str, token: &str) -> String {
    format!(
        "{{\"mcp\":{{\"quiver\":{{\"type\":\"remote\",\"url\":\"{endpunkt}\",\"headers\":{{\"Authorization\":\"Bearer {token}\"}}}}}}}}"
    )
}

/// Die Einstellung für GitHub Copilot in VS Code (`.vscode/mcp.json` bzw. die
/// Benutzer-`mcp.json`, Schlüssel `servers`).
pub fn schnipsel_copilot(endpunkt: &str, token: &str) -> String {
    format!(
        "{{\"servers\":{{\"quiver\":{{\"type\":\"http\",\"url\":\"{endpunkt}\",\"headers\":{{\"Authorization\":\"Bearer {token}\"}}}}}}}}"
    )
}

/// Der Eintrag für Hermes (`~/.hermes/config.yaml`, unter `mcp_servers:`).
pub fn schnipsel_hermes(endpunkt: &str, token: &str) -> String {
    format!(
        "  quiver:\n    url: {endpunkt}\n    headers:\n      Authorization: Bearer {token}\n    enabled: true"
    )
}

/// Dasselbe als JSON für andere MCP-Clients.
pub fn schnipsel_json(endpunkt: &str, token: &str) -> String {
    format!(
        "{{\"mcpServers\":{{\"quiver\":{{\"type\":\"http\",\"url\":\"{endpunkt}\",\"headers\":{{\"Authorization\":\"Bearer {token}\"}}}}}}}}"
    )
}

/// Ein Schnipsel zum Einrichten: wohin er gehört und was zu kopieren ist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schnipsel {
    /// Die Zeile, die sagt, wo man ihn einfügt.
    pub ort: String,
    /// Der Text, der kopiert wird.
    pub text: String,
}

/// Die Schnipsel für eine Agent-Art. Hermes bekommt zwei: den Eintrag und den
/// Prüfbefehl danach. „Andere" bekommt das allgemeine JSON.
pub fn schnipsel(sprache: Sprache, art: &AgentArt, endpunkt: &str, token: &str) -> Vec<Schnipsel> {
    let ort = |schluessel: &str| t(sprache, schluessel);
    match art {
        AgentArt::Pi => vec![Schnipsel {
            ort: ort("agenten.ort.shell"),
            text: schnipsel_pi(endpunkt, token),
        }],
        AgentArt::ClaudeCode => vec![Schnipsel {
            ort: ort("agenten.ort.shell"),
            text: schnipsel_claude(endpunkt, token),
        }],
        AgentArt::OpenCode => vec![Schnipsel {
            ort: ort("agenten.ort.opencode"),
            text: schnipsel_opencode(endpunkt, token),
        }],
        AgentArt::Copilot => vec![Schnipsel {
            ort: ort("agenten.ort.copilot"),
            text: schnipsel_copilot(endpunkt, token),
        }],
        AgentArt::Hermes => vec![
            Schnipsel {
                ort: ort("agenten.ort.hermes"),
                text: schnipsel_hermes(endpunkt, token),
            },
            Schnipsel {
                ort: ort("agenten.ort.hermes_test"),
                text: "hermes mcp test quiver".to_string(),
            },
        ],
        AgentArt::Andere(_) => vec![Schnipsel {
            ort: ort("agenten.ort.andere"),
            text: schnipsel_json(endpunkt, token),
        }],
    }
}

/// Die Art eines Agenten (Summentyp). Bekannte Arten haben einen festen Namen,
/// unter dem sie gekoppelt werden; „Andere" trägt seinen Namen selbst.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentArt {
    Pi,
    ClaudeCode,
    OpenCode,
    Copilot,
    Hermes,
    Andere(String),
}

impl AgentArt {
    /// Die wählbaren Arten in der Reihenfolge der Auswahl.
    pub fn alle() -> Vec<AgentArt> {
        vec![
            AgentArt::Pi,
            AgentArt::ClaudeCode,
            AgentArt::OpenCode,
            AgentArt::Copilot,
            AgentArt::Hermes,
            AgentArt::Andere(String::new()),
        ]
    }

    /// Der Name, unter dem der Agent gekoppelt wird.
    pub fn name(&self) -> String {
        match self {
            AgentArt::Pi => "pi".to_string(),
            AgentArt::ClaudeCode => "claude-code".to_string(),
            AgentArt::OpenCode => "opencode".to_string(),
            AgentArt::Copilot => "copilot".to_string(),
            AgentArt::Hermes => "hermes".to_string(),
            AgentArt::Andere(name) => name.trim().to_string(),
        }
    }

    /// Anzeigename in der Auswahl. Markennamen bleiben, wie sie heißen; nur
    /// „Andere" wird übersetzt.
    pub fn anzeige(&self, sprache: Sprache) -> String {
        match self {
            AgentArt::Pi => "pi".to_string(),
            AgentArt::ClaudeCode => "Claude Code".to_string(),
            AgentArt::OpenCode => "OpenCode".to_string(),
            AgentArt::Copilot => "GitHub Copilot".to_string(),
            AgentArt::Hermes => "Hermes".to_string(),
            AgentArt::Andere(_) => t(sprache, "agenten.art_andere"),
        }
    }

    /// Bekannte Art: der Name steht fest.
    pub fn ist_fest(&self) -> bool {
        !matches!(self, AgentArt::Andere(_))
    }

    /// Gleiche Art? „Andere" zählt als eine Art, egal welcher Name darin steht.
    pub fn gleich(&self, andere: &AgentArt) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(andere)
    }

    /// Aus einem gekoppelten Namen die Art erkennen (ohne Groß-/
    /// Kleinschreibung). Unbekanntes wird zu „Andere".
    pub fn aus_name(name: &str) -> AgentArt {
        match name.trim().to_lowercase().as_str() {
            "pi" => AgentArt::Pi,
            "claude-code" | "claude" => AgentArt::ClaudeCode,
            "opencode" => AgentArt::OpenCode,
            "copilot" | "github-copilot" => AgentArt::Copilot,
            "hermes" => AgentArt::Hermes,
            _ => AgentArt::Andere(name.trim().to_string()),
        }
    }

    /// Der einzutragende Name: fest bei bekannter Art, sonst die Eingabe.
    pub fn einzutragen(&self, eingabe: &str) -> String {
        match self {
            AgentArt::Andere(_) => eingabe.trim().to_string(),
            fest => fest.name(),
        }
    }
}

/// Eine Zeile der Art-Auswahl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtZeile {
    pub art: AgentArt,
    /// Schon gekoppelt: dann nicht mehr wählbar.
    pub gekoppelt: bool,
}

/// Die Art-Auswahl über den schon gekoppelten Namen: bekannte Arten, die es
/// bereits gibt, sind ausgegraut; „Andere" bleibt immer wählbar.
pub fn arten_auswahl(gekoppelte: &[String]) -> Vec<ArtZeile> {
    AgentArt::alle()
        .into_iter()
        .map(|art| {
            let gekoppelt = art.ist_fest() && name_vergeben(&art.name(), gekoppelte);
            ArtZeile { art, gekoppelt }
        })
        .collect()
}

/// Die erste noch freie Art — damit ein Dialog nicht mit einer ausgegrauten
/// Zeile beginnt. Ist alles Bekannte gekoppelt, ist es „Andere".
pub fn erste_freie(gekoppelte: &[String]) -> AgentArt {
    arten_auswahl(gekoppelte)
        .into_iter()
        .find(|zeile| !zeile.gekoppelt)
        .map(|zeile| zeile.art)
        .unwrap_or_else(|| AgentArt::Andere(String::new()))
}

/// Ist dieser Name schon vergeben? Ohne Groß-/Kleinschreibung und ohne
/// Leerzeichen am Rand – dieselbe Regel wie in [`quiver_client::Kern::agent_koppeln`].
pub fn name_vergeben(name: &str, gekoppelte: &[String]) -> bool {
    let gesucht = name.trim().to_lowercase();
    !gesucht.is_empty()
        && gekoppelte
            .iter()
            .any(|vorhanden| vorhanden.trim().to_lowercase() == gesucht)
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
    fn jeder_agent_nur_einmal() {
        let gekoppelt = vec!["pi".to_string(), "Claude Code".to_string()];
        assert!(name_vergeben("PI", &gekoppelt), "ohne Groß/Klein");
        assert!(name_vergeben("  claude code ", &gekoppelt), "am Rand");
        assert!(!name_vergeben("hermes", &gekoppelt));
        assert!(!name_vergeben("  ", &gekoppelt), "leer ist kein Name");
        assert!(!name_vergeben("pi", &[]));
    }

    #[test]
    fn arten_haben_feste_namen() {
        assert_eq!(AgentArt::Pi.name(), "pi");
        assert_eq!(AgentArt::ClaudeCode.name(), "claude-code");
        assert_eq!(AgentArt::OpenCode.name(), "opencode");
        assert_eq!(AgentArt::Copilot.name(), "copilot");
        assert_eq!(AgentArt::Hermes.name(), "hermes");
        let andere = AgentArt::Andere("  mein-agent ".into());
        assert_eq!(andere.name(), "mein-agent");
        assert!(!andere.ist_fest());
        assert!(AgentArt::Pi.ist_fest());

        // Hin und zurück: bekannte Namen werden erkannt, fremde bleiben.
        for art in [
            AgentArt::Pi,
            AgentArt::ClaudeCode,
            AgentArt::OpenCode,
            AgentArt::Copilot,
            AgentArt::Hermes,
        ] {
            assert!(AgentArt::aus_name(&art.name()).gleich(&art));
        }
        assert!(AgentArt::aus_name("PI").gleich(&AgentArt::Pi));
        assert!(AgentArt::aus_name("claude").gleich(&AgentArt::ClaudeCode));
        assert_eq!(
            AgentArt::aus_name(" mein-agent "),
            AgentArt::Andere("mein-agent".into())
        );
        // Andere bleibt Andere, egal welcher Name darin steht.
        assert!(AgentArt::Andere("x".into()).gleich(&AgentArt::Andere("y".into())));

        // Der einzutragende Name: fest bei bekannter Art, sonst die Eingabe.
        assert_eq!(AgentArt::Hermes.einzutragen("egal"), "hermes");
        assert_eq!(
            AgentArt::Andere(String::new()).einzutragen("  mein-agent "),
            "mein-agent"
        );
    }

    #[test]
    fn gekoppelte_arten_sind_ausgegraut() {
        let zeilen = arten_auswahl(&["pi".into(), "Hermes".into(), "x".into()]);
        assert_eq!(zeilen.len(), 6, "fünf bekannte Arten und Andere");
        let gekoppelt = |art: &AgentArt| {
            zeilen
                .iter()
                .find(|zeile| zeile.art.gleich(art))
                .map(|zeile| zeile.gekoppelt)
                .expect("Art steht in der Auswahl")
        };
        assert!(gekoppelt(&AgentArt::Pi));
        assert!(gekoppelt(&AgentArt::Hermes), "ohne Groß/Klein");
        assert!(!gekoppelt(&AgentArt::OpenCode));
        assert!(
            !gekoppelt(&AgentArt::Andere(String::new())),
            "Andere bleibt immer wählbar"
        );

        // Doppelte Namen aus alten Profilen bleiben erhalten und stören nicht:
        // „pi" ist trotzdem nur eine ausgegraute Zeile.
        let mit_doppelten = arten_auswahl(&["pi".into(), "PI".into()]);
        assert_eq!(mit_doppelten.len(), 6);
        assert_eq!(
            mit_doppelten.iter().filter(|zeile| zeile.gekoppelt).count(),
            1
        );

        // Der Dialog beginnt bei der ersten freien Art …
        assert!(erste_freie(&["pi".into()]).gleich(&AgentArt::ClaudeCode));
        assert!(erste_freie(&[]).gleich(&AgentArt::Pi));
        // … und bei „Andere", wenn alles Bekannte vergeben ist.
        let alle = ["pi", "claude-code", "opencode", "copilot", "hermes"].map(String::from);
        assert!(erste_freie(&alle).gleich(&AgentArt::Andere(String::new())));
    }

    #[test]
    fn schnipsel_je_art() {
        let e = "http://127.0.0.1:8741/mcp";
        let t = "abc123";
        let schnipsel = |art: AgentArt| {
            let liste = schnipsel(Sprache::De, &art, e, t);
            assert!(!liste.is_empty(), "{art:?} ohne Schnipsel");
            for s in &liste {
                assert!(!s.ort.is_empty(), "{art:?} ohne Ort");
                // Der Prüfbefehl für Hermes trägt weder Endpunkt noch Token.
                if s.text != "hermes mcp test quiver" {
                    assert!(s.text.contains("abc123"), "{art:?} ohne Token: {}", s.text);
                    assert!(s.text.contains(e), "{art:?} ohne Endpunkt: {}", s.text);
                }
            }
            liste
        };
        assert_eq!(schnipsel(AgentArt::Pi)[0].text, schnipsel_pi(e, t));
        assert_eq!(
            schnipsel(AgentArt::ClaudeCode)[0].text,
            schnipsel_claude(e, t)
        );
        assert_eq!(
            schnipsel(AgentArt::OpenCode)[0].text,
            schnipsel_opencode(e, t)
        );
        assert_eq!(
            schnipsel(AgentArt::Copilot)[0].text,
            schnipsel_copilot(e, t)
        );
        let hermes = schnipsel(AgentArt::Hermes);
        assert_eq!(hermes.len(), 2, "Eintrag und Prüfbefehl");
        assert_eq!(hermes[1].text, "hermes mcp test quiver");
        assert_eq!(
            schnipsel(AgentArt::Andere(String::new()))[0].text,
            schnipsel_json(e, t)
        );

        // Die JSON-Schnipsel sind gültiges JSON, nicht nur richtig aussehend.
        for (art, wert) in [
            (AgentArt::OpenCode, schnipsel_opencode(e, t)),
            (AgentArt::Copilot, schnipsel_copilot(e, t)),
            (AgentArt::Andere(String::new()), schnipsel_json(e, t)),
        ] {
            let json: serde_json::Value = serde_json::from_str(&wert)
                .unwrap_or_else(|fehler| panic!("{art:?} ist kein JSON: {fehler}\n{wert}"));
            assert_eq!(
                json.to_string().matches("Authorization").count(),
                1,
                "{art:?} trägt den Authorization-Kopf"
            );
            assert_eq!(
                json.to_string().matches("Bearer abc123").count(),
                1,
                "{art:?} trägt das Token"
            );
        }
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

    #[test]
    fn tray_zeile_zaehlt_was_laeuft() {
        let mut m = modell(verbunden());
        m.eintraege = vec![
            EintragSicht {
                tools: vec!["echo".into(), "ping".into()],
                ..eintrag()
            },
            EintragSicht {
                tools: vec!["lesen".into(), "schreiben".into()],
                ..eintrag()
            },
        ];
        assert_eq!(tray_zeile(&m, Sprache::De), "Verbunden · 4 Tools · 1 Agent");
        assert_eq!(tray_zeile(&m, Sprache::En), "Connected · 4 tools · 1 agent");
        // Zwei Agenten und einer davon weg: gezählt wird, wer da ist.
        m.agenten.push(AgentSicht {
            id: "a2".into(),
            name: "hermes".into(),
            verbunden: false,
        });
        assert_eq!(
            tray_zeile(&m, Sprache::De),
            "Verbunden · 4 Tools · 1 Agent",
            "getrennte Agenten zählen nicht"
        );
        m.agenten.push(AgentSicht {
            id: "a3".into(),
            name: "claude".into(),
            verbunden: true,
        });
        assert!(
            tray_zeile(&m, Sprache::De).ends_with("2 Agenten"),
            "Mehrzahl"
        );
    }

    #[test]
    fn tray_zeile_kennt_die_anderen_zustaende() {
        let ohne = modell(Verbindung::NichtEingebunden);
        assert_eq!(tray_zeile(&ohne, Sprache::De), "Nicht verbunden");
        let getrennt = modell(Verbindung::Getrennt {
            server: "http://x".into(),
            fehler: "weg".into(),
        });
        assert_eq!(
            tray_zeile(&getrennt, Sprache::De),
            "Getrennt · läuft weiter"
        );
        let verbindet = modell(Verbindung::Verbindet {
            server: "http://x".into(),
        });
        assert_eq!(tray_zeile(&verbindet, Sprache::De), "Verbinde …");
    }

    #[test]
    fn tray_menu_hat_die_vereinbarte_reihenfolge() {
        let menu = tray_menu(&modell(verbunden()), true, Sprache::De);
        assert_eq!(menu.len(), 5);
        assert!(
            matches!(&menu[0], TrayEintrag::Zustand(text) if text == "Verbunden · 0 Tools · 1 Agent")
        );
        assert!(matches!(&menu[1], TrayEintrag::Zeigen(text) if text == "Fenster öffnen"));
        assert!(matches!(
            &menu[2],
            TrayEintrag::Autostart { text, an: true } if text == "Beim Anmelden starten"
        ));
        assert_eq!(menu[3], TrayEintrag::Trenner);
        assert!(matches!(&menu[4], TrayEintrag::Beenden(text) if text == "Beenden"));
        // Englisch, Autostart aus.
        let menu = tray_menu(&modell(verbunden()), false, Sprache::En);
        assert!(
            matches!(&menu[2], TrayEintrag::Autostart { text, an: false } if text == "Start at login")
        );
        assert!(matches!(&menu[4], TrayEintrag::Beenden(text) if text == "Quit"));
    }

    #[test]
    fn argumente_lesen_hintergrund_und_kopflos() {
        assert_eq!(argumente(Vec::<&str>::new()), Argumente::default());
        let hintergrund = argumente(["--hintergrund"]);
        assert!(hintergrund.hintergrund);
        assert!(
            !hintergrund.kopflos,
            "ein Argument schaltet nicht das andere"
        );
        let kopflos = argumente(["--headless"]);
        assert!(kopflos.kopflos);
        assert!(!kopflos.hintergrund);
        // Reihenfolge und Unbekanntes sind egal.
        let beide = argumente(["--unbekannt", "--hintergrund", "--headless"]);
        assert!(beide.hintergrund && beide.kopflos);
    }
}
