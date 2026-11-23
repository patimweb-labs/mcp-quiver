// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Reine Funktionen der Oberfläche (AGENTS.md §2.4): welche Seite gilt,
//! welche Farbe ein Zustand hat, was in einem Feld steht, welche Schnipsel
//! ein Agent bekommt. Ohne egui, mit Tests — die Ansichten rufen nur hier auf.

use quiver_client::{
    AnmeldeZustand, EinrichtZustand, EintragSicht, Modell, Verbindung, ZugaengeSicht, ZugangSicht,
};
use quiver_domain::{link_aufloesen, zugangsname_gueltig, FehlendesFeld, FeldLink, Quelle, Werte};
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

/// Welcher Knopf zur Anmeldung gehört.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnmeldeKnopf {
    /// Nichts zu tun – die Anmeldung läuft gerade.
    Keiner,
    /// „Anmelden“: der User löst sie im Fenster aus, nie ein Agent.
    Anmelden,
    /// „Erneut versuchen“ nach einem Fehler.
    Erneut,
    /// „Abmelden“ – mit Rückfrage.
    Abmelden,
}

/// Was die Karte zur Anmeldung eines Eintrags zeigt: Bedeutung, Text, Knopf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnmeldeZeile {
    pub ton: Ton,
    pub text: String,
    pub knopf: AnmeldeKnopf,
}

/// Die Anmeldezeile eines Eintrags (Aufgabe 10): abgemeldet und unbekannt
/// warnen, „läuft“ ist leise, angemeldet ist gut, ein Fehler zeigt seinen Text.
pub fn anmeldung_zeile(sprache: Sprache, zustand: &AnmeldeZustand) -> AnmeldeZeile {
    let warnung = AnmeldeZeile {
        ton: Ton::Warnung,
        text: t(sprache, "anmeldung.noetig"),
        knopf: AnmeldeKnopf::Anmelden,
    };
    match zustand {
        AnmeldeZustand::Unbekannt | AnmeldeZustand::Abgemeldet => warnung,
        AnmeldeZustand::Laeuft => AnmeldeZeile {
            ton: Ton::Leise,
            text: t(sprache, "anmeldung.laeuft"),
            knopf: AnmeldeKnopf::Keiner,
        },
        AnmeldeZustand::Angemeldet { konto } => AnmeldeZeile {
            ton: Ton::Gut,
            text: match konto {
                Some(konto) => tf(sprache, "anmeldung.angemeldet", &[konto]),
                None => t(sprache, "anmeldung.angemeldet_ohne_konto"),
            },
            knopf: AnmeldeKnopf::Abmelden,
        },
        AnmeldeZustand::Fehler { text } => AnmeldeZeile {
            ton: Ton::Fehler,
            text: text.clone(),
            knopf: AnmeldeKnopf::Erneut,
        },
    }
}

/// Welcher Knopf zur Einrichtung gehört (Aufgabe 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EinrichtKnopf {
    /// Nichts zu tun – die Einrichtung ist im Browser geöffnet.
    Keiner,
    /// „Einrichten“: ein normaler Knopf, kein Hauptknopf.
    Einrichten,
    /// „Erneut versuchen“ nach einem Fehler.
    Erneut,
}

/// Was die Karte zur Einrichtung eines Eintrags zeigt: Bedeutung, Text, Knopf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EinrichtZeile {
    pub ton: Ton,
    pub text: String,
    pub knopf: EinrichtKnopf,
}

/// Die Einrichtungszeile eines Eintrags (Aufgabe 11): der Knopf „Einrichten“
/// öffnet die Seite im Browser. Während sie offen ist, steht dort nur der
/// leise Hinweis; nach einem Fehler sein Text und „Erneut versuchen“.
pub fn einrichtung_zeile(sprache: Sprache, zustand: &EinrichtZustand) -> EinrichtZeile {
    match zustand {
        EinrichtZustand::Bereit => EinrichtZeile {
            ton: Ton::Leise,
            text: t(sprache, "einrichtung.bereit"),
            knopf: EinrichtKnopf::Einrichten,
        },
        EinrichtZustand::Laeuft => EinrichtZeile {
            ton: Ton::Leise,
            text: t(sprache, "einrichtung.laeuft"),
            knopf: EinrichtKnopf::Keiner,
        },
        EinrichtZustand::Fehler { text } => EinrichtZeile {
            ton: Ton::Fehler,
            text: text.clone(),
            knopf: EinrichtKnopf::Erneut,
        },
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
                link: feld.link.clone(),
            }
        })
        .collect()
}

/// Die Werte, die der Dialog nicht hergibt: die Standardwerte der Felder
/// (Aufgabe 13). Was der Server selbst einsetzt, kennt das Fenster nicht —
/// `EintragSicht` führt nur Client- und Geheimfelder.
pub fn link_basis(eintrag: &EintragSicht) -> Werte {
    link_basis_aus_feldern(&eintrag.felder)
}

/// Wie [`link_basis`], aber für eine beliebige Feldliste — etwa die Felder
/// einer Zugangsliste (Aufgabe 14).
pub fn link_basis_aus_feldern(felder: &[quiver_domain::Feld]) -> Werte {
    felder
        .iter()
        .filter_map(|feld| {
            let wert = feld.standard.as_deref()?.trim();
            (!wert.is_empty()).then(|| (feld.name.clone(), wert.to_string()))
        })
        .collect()
}

/// Die Werte, mit denen ein Hilfe-Link aufgelöst wird: erst die Basis, dann
/// die **aktuellen** Eingaben des Dialogs — sie gewinnen (Aufgabe 13: „nicht
/// nur die gespeicherten Werte"). Leere Eingaben zählen nicht.
pub fn link_werte(eingaben: &[Feldeingabe], basis: &Werte) -> Werte {
    let mut werte = basis.clone();
    for eingabe in eingaben {
        let wert = eingabe.wert.trim();
        if !wert.is_empty() {
            werte.insert(eingabe.name.clone(), wert.to_string());
        }
    }
    werte
}

/// Die Adresse des Hilfe-Links oder `None`, solange ein verwendeter
/// Platzhalter ohne Wert ist (dann bleibt der Knopf gesperrt).
pub fn link_adresse(link: &FeldLink, eingaben: &[Feldeingabe], basis: &Werte) -> Option<String> {
    link_aufloesen(link, &link_werte(eingaben, basis))
}

/// Welche Felder noch fehlen, damit der Link aufgeht: die Platzhalter in
/// `link.url`, für die kein Wert da ist. Leer heißt: der Link ist fertig.
pub fn link_fehlende(link: &FeldLink, eingaben: &[Feldeingabe], basis: &Werte) -> Vec<String> {
    let werte = link_werte(eingaben, basis);
    platzhalter(&link.url)
        .into_iter()
        .filter(|name| !werte.contains_key(name))
        .collect()
}

/// Der Tooltip des gesperrten Knopfs: „Erst <Feld> eintragen".
pub fn link_hinweis(fehlende: &[String], sprache: Sprache) -> String {
    tf(sprache, "tools.link_erst", &[&fehlende.join(", ")])
}

/// Die Namen in `${...}` einer Vorlage, in der Reihenfolge des Auftretens und
/// ohne Doppelte.
fn platzhalter(text: &str) -> Vec<String> {
    let mut namen: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let nach = &rest[start + 2..];
        let Some(ende) = nach.find('}') else {
            break;
        };
        let name = &nach[..ende];
        if !namen.iter().any(|vorhanden| vorhanden == name) {
            namen.push(name.to_string());
        }
        rest = &nach[ende + 1..];
    }
    namen
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

/// Die Eingaben eines Zugangs (Aufgabe 14): die Felder der Zugangsliste,
/// vorbelegt mit den nicht-geheimen Werten. Geheimes wird nie angezeigt,
/// nur „gesetzt“.
pub fn zugang_eingaben(zugaenge: &ZugaengeSicht, name: Option<&str>) -> Vec<Feldeingabe> {
    let zugang = name.and_then(|name| zugaenge.liste.iter().find(|zugang| zugang.name == name));
    zugaenge
        .felder
        .iter()
        .map(|feld| {
            let geheim = feld.quelle == Quelle::Geheim;
            Feldeingabe {
                name: feld.name.clone(),
                beschreibung: feld.beschreibung.clone(),
                geheim,
                pflicht: feld.pflicht,
                gesetzt: geheim
                    && zugang.is_some_and(|zugang| zugang.geheim_gesetzt.contains(&feld.name)),
                wert: if geheim {
                    String::new()
                } else {
                    zugang
                        .and_then(|zugang| zugang.werte.get(&feld.name))
                        .cloned()
                        .unwrap_or_default()
                },
                link: feld.link.clone(),
            }
        })
        .collect()
}

/// Die Überschrift einer Zugangsliste: „Verbindungen (2)“.
pub fn zugang_beschriftung(sprache: Sprache, zugaenge: &ZugaengeSicht) -> String {
    tf(
        sprache,
        "zugaenge.kopf",
        &[&zugaenge.titel, &zugaenge.liste.len()],
    )
}

/// Der Knopf, der einen Zugang anlegt: „Verbindung hinzufügen“.
pub fn zugang_hinzufuegen(sprache: Sprache, zugaenge: &ZugaengeSicht) -> String {
    tf(sprache, "zugaenge.hinzufuegen", &[&zugaenge.einzahl])
}

/// Die Überschrift des Zugangs-Dialogs: anlegen oder bearbeiten.
pub fn zugang_titel(sprache: Sprache, einzahl: &str, neu: bool) -> String {
    let schluessel = if neu {
        "zugaenge.anlegen_titel"
    } else {
        "zugaenge.bearbeiten_titel"
    };
    tf(sprache, schluessel, &[&einzahl])
}

/// Eine Zeile unter dem Namen eines Zugangs: ein bis zwei nicht-geheime
/// Werte in der Reihenfolge der Felder (z. B. `benutzer@host`).
pub fn zugang_werte_zeile(felder: &[quiver_domain::Feld], zugang: &ZugangSicht) -> String {
    felder
        .iter()
        .filter(|feld| feld.quelle != Quelle::Geheim)
        .filter_map(|feld| zugang.werte.get(&feld.name))
        .map(|wert| wert.trim())
        .filter(|wert| !wert.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join("@")
}

/// Warum dieser Name für einen Zugang nicht geht – `None` heißt: in Ordnung.
/// Dieselbe Regel wie [`quiver_domain::zugangsname_gueltig`], zusätzlich
/// eindeutig: `vorhandene` sind die Namen der schon gespeicherten Zugänge.
pub fn zugang_name_fehler(name: &str, vorhandene: &[String]) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Some("zugaenge.name_fehlt");
    }
    if !zugangsname_gueltig(name) {
        return Some("zugaenge.name_ungueltig");
    }
    if vorhandene.iter().any(|vorhanden| vorhanden == name) {
        return Some("zugaenge.name_doppelt");
    }
    None
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

/// Der Befehl für Hermes. `--auth header` fragt das Token verdeckt ab und legt
/// es in `~/.hermes/.env` (`MCP_QUIVER_API_KEY`) ab; in `config.yaml` steht nur
/// der Verweis darauf (geprüft im Quelltext von Hermes 0.21, `mcp_config.py`).
pub fn schnipsel_hermes(endpunkt: &str) -> String {
    format!("hermes mcp add quiver --url {endpunkt} --auth header")
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

/// Die Schnipsel für eine Agent-Art. Hermes bekommt drei: den Befehl, das
/// Token für seine Rückfrage und den Prüfbefehl danach. „Andere" bekommt das allgemeine JSON.
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
                text: schnipsel_hermes(endpunkt),
            },
            Schnipsel {
                ort: ort("agenten.ort.hermes_token"),
                text: token.to_string(),
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
                    link: None,
                },
                Feld {
                    name: "SCHLUESSEL".into(),
                    beschreibung: "Geheimnis".into(),
                    quelle: Quelle::Geheim,
                    pflicht: false,
                    standard: None,
                    link: None,
                },
            ],
            lokale_werte: Werte::from([("PFAD".to_string(), "/tmp".to_string())]),
            geheim_gesetzt: vec!["SCHLUESSEL".into()],
            tools: vec!["echo".into()],
            anmeldung: None,
            einrichtung: None,
            zugaenge: None,
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
    fn anmeldung_hat_vier_zustaende_und_je_einen_knopf() {
        let sprache = Sprache::De;
        // Abgemeldet und unbekannt: derselbe Hinweis, „Anmelden“.
        for zustand in [AnmeldeZustand::Abgemeldet, AnmeldeZustand::Unbekannt] {
            let zeile = anmeldung_zeile(sprache, &zustand);
            assert_eq!(zeile.ton, Ton::Warnung);
            assert_eq!(zeile.text, "Anmeldung nötig");
            assert_eq!(zeile.knopf, AnmeldeKnopf::Anmelden);
        }
        // Läuft: leise, kein Knopf.
        let laeuft = anmeldung_zeile(sprache, &AnmeldeZustand::Laeuft);
        assert_eq!(laeuft.ton, Ton::Leise);
        assert_eq!(laeuft.text, "Anmeldung im Browser läuft …");
        assert_eq!(laeuft.knopf, AnmeldeKnopf::Keiner);
        // Angemeldet: Konto und „Abmelden“; ohne Konto bleibt der Name weg.
        let angemeldet = anmeldung_zeile(
            sprache,
            &AnmeldeZustand::Angemeldet {
                konto: Some("test@beispiel.de".into()),
            },
        );
        assert_eq!(angemeldet.ton, Ton::Gut);
        assert_eq!(angemeldet.text, "Angemeldet als test@beispiel.de");
        assert_eq!(angemeldet.knopf, AnmeldeKnopf::Abmelden);
        let ohne = anmeldung_zeile(sprache, &AnmeldeZustand::Angemeldet { konto: None });
        assert_eq!(ohne.text, "Angemeldet");
        // Fehler: der Text des Fehlers, „Erneut versuchen“.
        let fehler = anmeldung_zeile(
            sprache,
            &AnmeldeZustand::Fehler {
                text: "Der Status endet mit Code 3".into(),
            },
        );
        assert_eq!(fehler.ton, Ton::Fehler);
        assert_eq!(fehler.text, "Der Status endet mit Code 3");
        assert_eq!(fehler.knopf, AnmeldeKnopf::Erneut);

        // Englisch ist übersetzt, nicht kopiert.
        let en = anmeldung_zeile(Sprache::En, &AnmeldeZustand::Abgemeldet);
        assert_eq!(en.text, "Sign-in required");
        let en = anmeldung_zeile(
            Sprache::En,
            &AnmeldeZustand::Angemeldet {
                konto: Some("test@beispiel.de".into()),
            },
        );
        assert_eq!(en.text, "Signed in as test@beispiel.de");
    }

    #[test]
    fn einrichtung_hat_drei_zustaende_und_je_einen_knopf() {
        let sprache = Sprache::De;
        // Bereit: der Knopf „Einrichten“, leise erklärt.
        let bereit = einrichtung_zeile(sprache, &EinrichtZustand::Bereit);
        assert_eq!(bereit.ton, Ton::Leise);
        assert_eq!(bereit.text, "Einrichtung im Browser");
        assert_eq!(bereit.knopf, EinrichtKnopf::Einrichten);
        // Läuft: derselbe leise Ton, aber kein Knopf – ein zweiter Klick
        // würde nichts tun.
        let laeuft = einrichtung_zeile(sprache, &EinrichtZustand::Laeuft);
        assert_eq!(laeuft.ton, Ton::Leise);
        assert_eq!(laeuft.text, "Einrichtung im Browser geöffnet …");
        assert_eq!(laeuft.knopf, EinrichtKnopf::Keiner);
        // Fehler: sein Text und „Erneut versuchen“.
        let fehler = einrichtung_zeile(
            sprache,
            &EinrichtZustand::Fehler {
                text: "Die Einrichtung endet mit Code 3".into(),
            },
        );
        assert_eq!(fehler.ton, Ton::Fehler);
        assert_eq!(fehler.text, "Die Einrichtung endet mit Code 3");
        assert_eq!(fehler.knopf, EinrichtKnopf::Erneut);
        // Englisch ist übersetzt, nicht kopiert.
        let en = einrichtung_zeile(Sprache::En, &EinrichtZustand::Bereit);
        assert_eq!(en.text, "Setup in the browser");
        let en = einrichtung_zeile(Sprache::En, &EinrichtZustand::Laeuft);
        assert_eq!(en.text, "Setup opened in the browser …");
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

    /// Outline aus der Aufgabe: `url` trägt die Adresse, `api_key` den Link.
    fn eintrag_mit_link() -> EintragSicht {
        let mut eintrag = eintrag();
        eintrag.felder = vec![
            Feld {
                name: "url".into(),
                beschreibung: "Adresse der Wiki".into(),
                quelle: Quelle::Client,
                pflicht: true,
                standard: None,
                link: None,
            },
            Feld {
                name: "api_key".into(),
                beschreibung: "Token".into(),
                quelle: Quelle::Geheim,
                pflicht: true,
                standard: None,
                link: Some(FeldLink {
                    text: "Token in Outline erstellen".into(),
                    url: "${url}/settings/api-and-access".into(),
                }),
            },
        ];
        eintrag.lokale_werte = Werte::new();
        eintrag.geheim_gesetzt = Vec::new();
        eintrag
    }

    #[test]
    fn link_knopf_folgt_den_aktuellen_eingaben() {
        let eintrag = eintrag_mit_link();
        let mut eingaben = eingaben(&eintrag);
        let link = eingaben[1].link.clone().expect("Link am zweiten Feld");
        assert!(eingaben[0].link.is_none(), "url ohne Link");
        let basis = link_basis(&eintrag);

        // Ohne Adresse bleibt der Knopf gesperrt — und sagt, was fehlt.
        assert_eq!(link_adresse(&link, &eingaben, &basis), None);
        assert_eq!(link_fehlende(&link, &eingaben, &basis), vec!["url"]);
        assert_eq!(
            link_hinweis(&link_fehlende(&link, &eingaben, &basis), Sprache::De),
            "Erst url eintragen"
        );

        // Die Eingabe im Dialog zählt, nicht nur der gespeicherte Wert (der
        // hier leer ist). Ein Schrägstrich am Ende stört nicht.
        eingaben[0].wert = " https://wiki.example.de/ ".into();
        assert_eq!(
            link_adresse(&link, &eingaben, &basis).as_deref(),
            Some("https://wiki.example.de/settings/api-and-access")
        );
        assert!(link_fehlende(&link, &eingaben, &basis).is_empty());

        // Nur http(s) kommt durch — auch wenn alles ausgefüllt ist.
        eingaben[0].wert = "javascript:alert(1)".into();
        assert_eq!(link_adresse(&link, &eingaben, &basis), None);
    }

    #[test]
    fn link_basis_sind_die_standardwerte() {
        let mut eintrag = eintrag_mit_link();
        eintrag.felder[0].standard = Some(" https://wiki.example.de ".into());
        let basis = link_basis(&eintrag);
        assert_eq!(
            basis.get("url").map(String::as_str),
            Some("https://wiki.example.de")
        );
        assert!(
            !basis.contains_key("api_key"),
            "leere Standards zählen nicht"
        );

        // Standard füllt die Lücke, die Eingabe gewinnt.
        let mut eingaben = eingaben(&eintrag);
        let link = eingaben[1].link.clone().expect("Link am zweiten Feld");
        assert_eq!(
            link_adresse(&link, &eingaben, &basis).as_deref(),
            Some("https://wiki.example.de/settings/api-and-access")
        );
        eingaben[0].wert = "https://anderes.example".into();
        assert_eq!(
            link_adresse(&link, &eingaben, &basis).as_deref(),
            Some("https://anderes.example/settings/api-and-access")
        );
    }

    #[test]
    fn platzhalter_kommen_in_reihenfolge_und_einmal() {
        assert_eq!(
            platzhalter("${b}/x/${a}/${b}"),
            vec!["b".to_string(), "a".to_string()]
        );
        assert!(platzhalter("ohne").is_empty(), "nichts zu ersetzen");
        assert!(platzhalter("${offen").is_empty(), "unvollständig");
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

    /// Ein Eintrag mit Zugangsliste wie bei quiver-ssh (Aufgabe 14).
    fn zugang_eintrag() -> EintragSicht {
        let mut port = Feld {
            name: "port".into(),
            beschreibung: "Port".into(),
            quelle: Quelle::Client,
            pflicht: false,
            standard: None,
            link: None,
        };
        port.standard = Some("22".into());
        let mut eintrag = eintrag();
        eintrag.zugaenge = Some(ZugaengeSicht {
            titel: "Verbindungen".into(),
            einzahl: "Verbindung".into(),
            felder: vec![
                Feld {
                    name: "benutzer".into(),
                    beschreibung: "Benutzer".into(),
                    quelle: Quelle::Client,
                    pflicht: true,
                    standard: None,
                    link: None,
                },
                Feld {
                    name: "host".into(),
                    beschreibung: "Host".into(),
                    quelle: Quelle::Client,
                    pflicht: true,
                    standard: None,
                    link: None,
                },
                port,
                Feld {
                    name: "passwort".into(),
                    beschreibung: "Passwort".into(),
                    quelle: Quelle::Geheim,
                    pflicht: false,
                    standard: None,
                    link: None,
                },
            ],
            liste: vec![
                ZugangSicht {
                    name: "db-01".into(),
                    werte: Werte::from([
                        ("benutzer".to_string(), "admin".to_string()),
                        ("host".to_string(), "db-01.example".to_string()),
                    ]),
                    geheim_gesetzt: vec!["passwort".into()],
                },
                ZugangSicht {
                    name: "web-01".into(),
                    werte: Werte::from([
                        ("benutzer".to_string(), "root".to_string()),
                        ("host".to_string(), "web-01.example".to_string()),
                        ("port".to_string(), "2222".to_string()),
                    ]),
                    geheim_gesetzt: Vec::new(),
                },
            ],
        });
        eintrag
    }

    #[test]
    fn zugangsliste_hat_ueberschrift_anzahl_und_knopf() {
        let eintrag = zugang_eintrag();
        let zugaenge = eintrag.zugaenge.clone().expect("Liste");
        assert_eq!(
            zugang_beschriftung(Sprache::De, &zugaenge),
            "Verbindungen (2)"
        );
        assert_eq!(
            zugang_hinzufuegen(Sprache::De, &zugaenge),
            "Verbindung hinzufügen"
        );
        assert_eq!(
            zugang_hinzufuegen(Sprache::En, &zugaenge),
            "Add Verbindung",
            "der Name des Elements kommt aus den Daten"
        );
        assert_eq!(
            zugang_titel(Sprache::De, &zugaenge.einzahl, true),
            "Verbindung anlegen"
        );
        assert_eq!(
            zugang_titel(Sprache::De, &zugaenge.einzahl, false),
            "Verbindung bearbeiten"
        );
    }

    #[test]
    fn zugangszeile_zeigt_zwei_werte_ohne_geheimes() {
        let eintrag = zugang_eintrag();
        let zugaenge = eintrag.zugaenge.clone().expect("Liste");
        let db = &zugaenge.liste[0];
        assert_eq!(
            zugang_werte_zeile(&zugaenge.felder, db),
            "admin@db-01.example",
            "Name und die ersten zwei Werte, Geheimes bleibt weg"
        );
        // Ein Zugang ohne Werte ergibt eine leere Zeile, keine „@“-Kette.
        let leer = ZugangSicht {
            name: "neu".into(),
            werte: Werte::new(),
            geheim_gesetzt: Vec::new(),
        };
        assert_eq!(zugang_werte_zeile(&zugaenge.felder, &leer), "");
    }

    #[test]
    fn zugang_eingaben_vorbelegt_und_geheim_leer() {
        let eintrag = zugang_eintrag();
        let zugaenge = eintrag.zugaenge.clone().expect("Liste");
        // Bearbeiten: die Werte stehen da, das Geheimnis nur als „gesetzt“.
        let eingaben = zugang_eingaben(&zugaenge, Some("db-01"));
        assert_eq!(eingaben.len(), 4);
        assert_eq!(eingaben[0].name, "benutzer");
        assert_eq!(eingaben[0].wert, "admin");
        assert_eq!(eingaben[1].wert, "db-01.example");
        assert_eq!(eingaben[2].wert, "", "port ist bei db-01 nicht gesetzt");
        assert!(eingaben[3].geheim);
        assert!(eingaben[3].gesetzt);
        assert_eq!(eingaben[3].wert, "", "Geheimes wird nie angezeigt");
        // Der Standard des Felds füllt den Port bei einem leeren Zugang.
        let neu = zugang_eingaben(&zugaenge, None);
        assert_eq!(neu[2].wert, "", "Standard steht nicht im Feld");
        assert!(!neu[3].gesetzt, "ein neuer Zugang hat keine Geheimnisse");
        assert!(!neu[0].gesetzt);
        // Der Hilfe-Link eines Zugangsfelds nutzt dessen Standardwerte.
        let basis = link_basis_aus_feldern(&zugaenge.felder);
        assert_eq!(basis.get("port").map(String::as_str), Some("22"));
    }

    #[test]
    fn zugangsname_folgt_der_regel_und_ist_eindeutig() {
        let vorhanden = vec!["web-01".to_string()];
        assert_eq!(
            zugang_name_fehler("", &vorhanden),
            Some("zugaenge.name_fehlt")
        );
        assert_eq!(
            zugang_name_fehler("  ", &vorhanden),
            Some("zugaenge.name_fehlt"),
            "Leerzeichen sind kein Name"
        );
        for name in ["web 01", "Web-01", "web/01", "ä"] {
            assert_eq!(
                zugang_name_fehler(name, &vorhanden),
                Some("zugaenge.name_ungueltig"),
                "{name}"
            );
        }
        assert_eq!(
            zugang_name_fehler(&"a".repeat(41), &vorhanden),
            Some("zugaenge.name_ungueltig"),
            "41 Zeichen sind zu lang"
        );
        assert_eq!(
            zugang_name_fehler(&"a".repeat(40), &vorhanden),
            None,
            "40 Zeichen gehen"
        );
        assert_eq!(
            zugang_name_fehler("web-01", &vorhanden),
            Some("zugaenge.name_doppelt")
        );
        assert_eq!(zugang_name_fehler(" web-02 ", &vorhanden), None);
        // Die Texte gibt es in beiden Sprachen.
        for schluessel in [
            "zugaenge.name_fehlt",
            "zugaenge.name_ungueltig",
            "zugaenge.name_doppelt",
        ] {
            for sprache in Sprache::ALLE {
                assert_ne!(t(sprache, schluessel), schluessel, "{schluessel}");
            }
        }
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
                // Bei Hermes verteilen sich Endpunkt und Token auf drei Schnipsel.
                if !matches!(art, AgentArt::Hermes) {
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
        assert_eq!(hermes.len(), 3, "Befehl, Token und Prüfbefehl");
        assert_eq!(
            hermes[0].text,
            "hermes mcp add quiver --url http://127.0.0.1:8741/mcp --auth header"
        );
        assert_eq!(hermes[1].text, "abc123");
        assert_eq!(hermes[2].text, "hermes mcp test quiver");
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
