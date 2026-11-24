// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Client-Kern von mcp-quiver: Abgleich mit dem Server, Laufzeiten, MCP-Endpunkt.
//!
//! Das Fenster (Aufgabe 05) kennt nur diese Fassade:
//!
//! ```no_run
//! # async fn beispiel() -> anyhow::Result<()> {
//! use quiver_client::{Kern, KernKonfig};
//!
//! let kern = Kern::starten(KernKonfig::standard()).await?;
//! let modell = kern.modell();
//! let _ = modell.borrow().endpunkt.clone();
//! kern.beenden().await;
//! # Ok(())
//! # }
//! ```
//!
//! Aufgaben:
//! 1. **Abgleich** ([`abgleich`]): Soll vom Server holen, per SSE auf Änderungen
//!    warten, `quiver_domain::abgleichen` ausführen.
//! 2. **Laufzeiten** ([`laufzeit`]): je Eintrag ein Kindprozess oder eine
//!    entfernte MCP-Verbindung, Neustart mit Backoff.
//! 3. **MCP-Endpunkt** ([`endpunkt`]): alle laufenden Werkzeuge unter einem
//!    lokalen HTTP-Endpunkt für die Agenten, mit eigenem Token je Agent.
//!
//! Geheimnisse liegen ausschließlich hier (im [`Tresor`]) und werden beim Start
//! eines Eintrags in dessen Umgebung eingesetzt. Der Agent sieht sie nie.

mod abgleich;
mod anmeldung;
mod dateitresor;
mod einrichtung;
mod endpunkt;
mod hilfen;
mod laufzeit;
mod profil;
mod sse;
mod tresor;

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use quiver_domain::{
    EintragId, FehlendesFeld, Feld, Geraet, Mehrfach, Quelle, Soll, SollEintrag, Werte, Zugang,
    MEHRFACH_UMGEBUNG,
};
use quiver_protocol::EintragZustand;
use rmcp::model::Tool;
use rmcp::{Peer, RoleClient, RoleServer};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Notify};

pub use anmeldung::AnmeldeZustand;
pub use einrichtung::EinrichtZustand;
use hilfen::Abbruch;
pub use tresor::{DateiTresor, SpeicherTresor, Tresor};

/// Wie oft der Anmeldezustand der Einträge nachgesehen wird.
const STATUS_ABSTAND: Duration = Duration::from_secs(10 * 60);

/// Die Version dieses Clients, wie sie in der Meldung an den Server steht.
pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Was der Kern zum Starten braucht.
#[derive(Clone)]
pub struct KernKonfig {
    /// Ordner für `zustand.json` (Standard: `dirs::config_dir()/mcp-quiver`).
    pub profil: PathBuf,
    /// Adresse des MCP-Endpunkts für die Agenten (Standard: `127.0.0.1:8741`).
    /// Port `0` lässt sich vom Betriebssystem vergeben; die tatsächliche
    /// Adresse steht dann in [`Modell::endpunkt`].
    pub mcp_adresse: SocketAddr,
    /// Wo Geheimnisse liegen (Standard: Tresor-Datei `<profil>/tresor`, in Tests: im Speicher).
    pub tresor: Arc<dyn Tresor>,
}

impl KernKonfig {
    /// Die üblichen Werte.
    pub fn standard() -> Self {
        let profil = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mcp-quiver");
        Self {
            tresor: Arc::new(DateiTresor::fuer_profil(&profil)),
            profil,
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 8741)),
        }
    }
}

impl Default for KernKonfig {
    fn default() -> Self {
        Self::standard()
    }
}

impl std::fmt::Debug for KernKonfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KernKonfig")
            .field("profil", &self.profil)
            .field("mcp_adresse", &self.mcp_adresse)
            .finish_non_exhaustive()
    }
}

/// Zustand der Verbindung zum Server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum Verbindung {
    /// Noch kein Server eingebunden.
    NichtEingebunden,
    /// Anmeldung läuft.
    Verbindet { server: String },
    /// Verbunden; `geraet` ist der Anzeigename des eigenen Geräts.
    Verbunden { server: String, geraet: String },
    /// Verbindung weg – was nach der Anmeldung lief, läuft weiter.
    Getrennt { server: String, fehler: String },
}

/// Ein Eintrag, wie ihn das Fenster zeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EintragSicht {
    pub id: EintragId,
    pub titel: String,
    pub beschreibung: String,
    pub zustand: EintragZustand,
    /// Pflichtfelder, die am Gerät noch fehlen.
    pub fehlende: Vec<FehlendesFeld>,
    /// Nur Client- und Geheim-Felder, zum Bearbeiten.
    pub felder: Vec<Feld>,
    /// Werte, die am Gerät hinterlegt sind – ohne Geheimes.
    pub lokale_werte: Werte,
    /// Welche Geheim-Felder gesetzt sind (nie der Wert selbst).
    pub geheim_gesetzt: Vec<String>,
    /// Werkzeugnamen ohne Präfix (`echo`, nicht `echo__echo`).
    pub tools: Vec<String>,
    /// Wie es um die Anmeldung steht — `None`, wenn der Eintrag keine braucht.
    /// Der Zustand bleibt lokal und geht nie an den Server.
    pub anmeldung: Option<AnmeldeZustand>,
    /// Wie es um die Einrichtung steht — `None`, wenn der Eintrag keine
    /// anbietet. Auch dieser Zustand bleibt lokal.
    pub einrichtung: Option<EinrichtZustand>,
    /// Die Zugänge des Eintrags (Aufgabe 14) — `None`, wenn er keine
    /// Zugangsliste führt. Geheimes steht auch hier nie im Modell.
    pub zugaenge: Option<ZugaengeSicht>,
}

/// Ein Zugang eines Eintrags mit `mehrfach`, wie ihn das Fenster zeigt:
/// Name und nicht-geheime Werte; von Geheimnissen nur, **welche** gesetzt sind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZugangSicht {
    pub name: String,
    /// Die nicht-geheimen Werte des Zugangs.
    pub werte: Werte,
    /// Welche Geheimfelder gesetzt sind (nie der Wert selbst).
    pub geheim_gesetzt: Vec<String>,
}

/// Die Zugangsliste eines Eintrags ([`quiver_domain::Mehrfach`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZugaengeSicht {
    /// Überschrift der Liste, z. B. „Verbindungen“.
    pub titel: String,
    /// Bezeichnung eines Elements, z. B. „Verbindung“.
    pub einzahl: String,
    /// Die Felder eines Zugangs (Client- und Geheimfelder).
    pub felder: Vec<Feld>,
    pub liste: Vec<ZugangSicht>,
}

/// Ein gekoppelter Agent, wie ihn das Fenster zeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSicht {
    pub id: String,
    pub name: String,
    /// Der Agent hat gerade eine offene Sitzung am Endpunkt.
    pub verbunden: bool,
}

/// Alles, was das Fenster zeigt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Modell {
    pub verbindung: Verbindung,
    pub eintraege: Vec<EintragSicht>,
    pub agenten: Vec<AgentSicht>,
    /// `http://127.0.0.1:8741/mcp`
    pub endpunkt: String,
    /// Der Server hat dieses Gerät abgelehnt (entfernt oder Token ungültig):
    /// alle Tools sind gestoppt, das Gerät muss neu eingebunden werden.
    #[serde(default)]
    pub abgelehnt: bool,
}

/// Der Stand eines Eintrags, ohne Verbindung (der Teil, der ins Modell geht).
#[derive(Debug, Clone)]
struct LaeuferStand {
    zustand: EintragZustand,
    fehlende: Vec<FehlendesFeld>,
    tools: Vec<Tool>,
}

impl LaeuferStand {
    fn neu(zustand: EintragZustand) -> Self {
        Self {
            zustand,
            fehlende: Vec::new(),
            tools: Vec::new(),
        }
    }
}

/// Ein Eintrag, der gerade läuft (oder es versucht).
struct Laeufer {
    /// Zählt jeden Start hoch. Eine Start-Aufgabe darf nur in den Läufer
    /// schreiben, der zu ihrer Generation gehört (Aufgabe 12).
    generation: u64,
    fingerabdruck: String,
    stand: LaeuferStand,
    peer: Option<Peer<RoleClient>>,
    abbruch: Abbruch,
}

/// Eine offene Sitzung eines Agenten am MCP-Endpunkt.
struct Sitzung {
    agent: Option<String>,
    peer: Peer<RoleServer>,
}

/// Alles, was der Kern im Speicher hält.
struct Stand {
    /// Siehe [`Modell::abgelehnt`].
    abgelehnt: bool,
    server_url: Option<String>,
    geraet: Option<Geraet>,
    token: Option<String>,
    lokale_werte: BTreeMap<EintragId, Werte>,
    /// Geheimwerte im Speicher (Quelle bleibt der Tresor).
    geheim: BTreeMap<EintragId, Werte>,
    /// Zugänge je Eintrag: Name → nicht-geheime Werte (Aufgabe 14).
    zugaenge: BTreeMap<EintragId, BTreeMap<String, Werte>>,
    /// Geheimwerte der Zugänge im Speicher (Quelle bleibt der Tresor).
    zugang_geheim: BTreeMap<EintragId, BTreeMap<String, Werte>>,
    agenten: Vec<profil::Agent>,
    soll: Soll,
    laeufer: BTreeMap<EintragId, Laeufer>,
    /// Anmeldezustand je Eintrag; nur lokal, gehört nicht zur Meldung.
    anmeldung: BTreeMap<EintragId, AnmeldeZustand>,
    /// Einrichtungszustand je Eintrag; ebenfalls nur lokal.
    einrichtung: BTreeMap<EintragId, EinrichtZustand>,
    /// Einträge, deren Einrichtung gerade läuft (höchstens einer je Eintrag).
    einrichtung_laufend: BTreeSet<EintragId>,
    verbindung: Verbindung,
    endpunkt: String,
}

impl Stand {
    fn modell(&self, verbunden: &BTreeSet<String>) -> Modell {
        let geheim_gesetzt: BTreeMap<EintragId, BTreeSet<String>> = self
            .geheim
            .iter()
            .map(|(id, werte)| (id.clone(), werte.keys().cloned().collect()))
            .collect();
        Modell {
            verbindung: self.verbindung.clone(),
            eintraege: eintraege_sicht(
                &self.soll,
                &Lokales {
                    werte: &self.lokale_werte,
                    geheim_gesetzt: &geheim_gesetzt,
                    zugaenge: &self.zugaenge,
                    zugang_geheim: &self.zugang_geheim,
                },
                &self.laeufer,
                &self.anmeldung,
                &self.einrichtung,
            ),
            agenten: self
                .agenten
                .iter()
                .map(|agent| AgentSicht {
                    id: agent.id.clone(),
                    name: agent.name.clone(),
                    verbunden: verbunden.contains(&agent.id),
                })
                .collect(),
            endpunkt: self.endpunkt.clone(),
            abgelehnt: self.abgelehnt,
        }
    }

    fn profil(&self) -> profil::Profil {
        profil::Profil {
            server_url: self.server_url.clone(),
            geraet: self.geraet.clone(),
            lokale_werte: self
                .lokale_werte
                .iter()
                .map(|(id, werte)| (id.als_str().to_string(), werte.clone()))
                .collect(),
            zugaenge: self
                .zugaenge
                .iter()
                .map(|(id, liste)| (id.als_str().to_string(), liste.clone()))
                .collect(),
            agenten: self.agenten.clone(),
            soll: Some(self.soll.clone()),
        }
    }
}

/// Was der Client je Eintrag lokal speichert: Werte, Geheimnisse und die
/// Zugangslisten (Aufgabe 14). Gebündelt, damit die Sicht nicht aus acht
/// Feldern zusammengesucht werden muss.
struct Lokales<'a> {
    werte: &'a BTreeMap<EintragId, Werte>,
    geheim_gesetzt: &'a BTreeMap<EintragId, BTreeSet<String>>,
    zugaenge: &'a BTreeMap<EintragId, BTreeMap<String, Werte>>,
    zugang_geheim: &'a BTreeMap<EintragId, BTreeMap<String, Werte>>,
}

/// Die Sicht auf die Einträge: Soll + lokale Werte → was das Fenster zeigt.
fn eintraege_sicht(
    soll: &Soll,
    lokal: &Lokales,
    laeufer: &BTreeMap<EintragId, Laeufer>,
    anmeldung: &BTreeMap<EintragId, AnmeldeZustand>,
    einrichtung: &BTreeMap<EintragId, EinrichtZustand>,
) -> Vec<EintragSicht> {
    let Lokales {
        werte: lokale_werte,
        geheim_gesetzt,
        zugaenge,
        zugang_geheim,
    } = lokal;
    soll.eintraege
        .iter()
        .map(|eintrag| {
            let id = &eintrag.eintrag.id;
            let geheim_namen: BTreeSet<&str> = eintrag
                .eintrag
                .felder
                .iter()
                .filter(|feld| feld.quelle == Quelle::Geheim)
                .map(|feld| feld.name.as_str())
                .collect();
            let werte = lokale_werte
                .get(id)
                .map(|werte| {
                    werte
                        .iter()
                        .filter(|(name, _)| !geheim_namen.contains(name.as_str()))
                        .map(|(name, wert)| (name.clone(), wert.clone()))
                        .collect()
                })
                .unwrap_or_default();
            let zustand = laeufer.get(id);
            EintragSicht {
                id: id.clone(),
                titel: eintrag.eintrag.titel.clone(),
                beschreibung: eintrag.eintrag.beschreibung.clone(),
                zustand: zustand
                    .map(|l| l.stand.zustand.clone())
                    .unwrap_or(EintragZustand::Startet),
                fehlende: zustand
                    .map(|l| l.stand.fehlende.clone())
                    .unwrap_or_default(),
                felder: eintrag
                    .eintrag
                    .felder
                    .iter()
                    .filter(|feld| feld.quelle != Quelle::Server)
                    .cloned()
                    .collect(),
                lokale_werte: werte,
                geheim_gesetzt: geheim_gesetzt
                    .get(id)
                    .map(|namen| namen.iter().cloned().collect())
                    .unwrap_or_default(),
                tools: zustand
                    .map(|l| {
                        l.stand
                            .tools
                            .iter()
                            .map(|tool| tool.name.to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                anmeldung: eintrag.eintrag.anmeldung.as_ref().map(|_| {
                    anmeldung
                        .get(id)
                        .cloned()
                        .unwrap_or(AnmeldeZustand::Unbekannt)
                }),
                einrichtung: eintrag.eintrag.einrichtung.as_ref().map(|_| {
                    einrichtung
                        .get(id)
                        .cloned()
                        .unwrap_or(EinrichtZustand::Bereit)
                }),
                zugaenge: eintrag.eintrag.mehrfach.as_ref().map(|mehrfach| {
                    zugaenge_sicht(mehrfach, zugaenge.get(id), zugang_geheim.get(id))
                }),
            }
        })
        .collect()
}

/// Die Zugangsliste eines Eintrags: Titel, Bezeichnung, Felder und die
/// gespeicherten Zugänge – ohne Geheimes, nur mit „gesetzt“.
fn zugaenge_sicht(
    mehrfach: &Mehrfach,
    zugaenge: Option<&BTreeMap<String, Werte>>,
    geheim: Option<&BTreeMap<String, Werte>>,
) -> ZugaengeSicht {
    let geheim_namen: BTreeSet<&str> = mehrfach
        .felder
        .iter()
        .filter(|feld| feld.quelle == Quelle::Geheim)
        .map(|feld| feld.name.as_str())
        .collect();
    ZugaengeSicht {
        titel: mehrfach.titel.clone(),
        einzahl: mehrfach.einzahl.clone(),
        felder: mehrfach.felder.clone(),
        liste: zugaenge
            .map(|liste| {
                liste
                    .iter()
                    .map(|(name, werte)| ZugangSicht {
                        name: name.clone(),
                        werte: werte
                            .iter()
                            .filter(|(feld, _)| !geheim_namen.contains(feld.as_str()))
                            .map(|(feld, wert)| (feld.clone(), wert.clone()))
                            .collect(),
                        geheim_gesetzt: geheim
                            .and_then(|je_eintrag| je_eintrag.get(name))
                            .map(|werte| werte.keys().cloned().collect())
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Die Zugänge eines Eintrags als JSON für [`MEHRFACH_UMGEBUNG`]: je Zugang
/// der Name und alle Werte, Geheimes eingesetzt. Ein Eintrag mit `mehrfach`,
/// aber ohne Zugang, bekommt eine leere Liste – er läuft trotzdem.
fn zugaenge_json(
    mehrfach: &Mehrfach,
    zugaenge: &BTreeMap<String, Werte>,
    geheim: &BTreeMap<String, Werte>,
) -> String {
    let liste: Vec<Zugang> = zugaenge
        .iter()
        .map(|(name, werte)| {
            let mut alle = Werte::new();
            for feld in &mehrfach.felder {
                let wert = match feld.quelle {
                    Quelle::Geheim => geheim.get(name).and_then(|werte| werte.get(&feld.name)),
                    _ => werte.get(&feld.name),
                };
                if let Some(wert) = wert.filter(|wert| !wert.trim().is_empty()) {
                    alle.insert(feld.name.clone(), wert.clone());
                }
            }
            Zugang {
                name: name.clone(),
                werte: alle,
            }
        })
        .collect();
    serde_json::to_string(&liste).expect("Zugänge sind serialisierbar")
}

/// Innerei des Kerns; geteilt zwischen Fassade, Abgleich und Endpunkt.
pub(crate) struct Innen {
    pub(crate) konfig: KernKonfig,
    pub(crate) http: reqwest::Client,
    pub(crate) stand: Mutex<Stand>,
    pub(crate) modell: watch::Sender<Modell>,
    pub(crate) sitzungen: Mutex<BTreeMap<u64, Sitzung>>,
    pub(crate) zaehler: AtomicU64,
    /// Zählt die Starts je Eintrag hoch (Generation eines Läufers).
    pub(crate) generation: AtomicU64,
    pub(crate) ende: Abbruch,
    pub(crate) erneut: Notify,
    pub(crate) aufgaben: Mutex<Vec<tokio::task::JoinHandle<()>>>,
}

impl Innen {
    pub(crate) fn stand(&self) -> MutexGuard<'_, Stand> {
        self.stand
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    /// Das aktuelle Modell an alle Zuschauer schicken.
    pub(crate) fn modell_schicken(&self) {
        let verbunden: BTreeSet<String> = self
            .sitzungen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .values()
            .filter_map(|sitzung| sitzung.agent.clone())
            .collect();
        let modell = self.stand().modell(&verbunden);
        // `send_replace` legt den Wert auch dann ab, wenn gerade niemand zusieht.
        self.modell.send_replace(modell);
    }

    pub(crate) fn speichern(&self) {
        let profil = self.stand().profil();
        if let Err(fehler) = profil.speichern(&self.konfig.profil) {
            tracing::warn!("Zustand lässt sich nicht speichern: {fehler}");
        }
    }

    /// Die Verbindung setzen, aber nur melden, wenn sie sich wirklich ändert.
    pub(crate) fn verbindung_setzen(&self, verbindung: Verbindung) {
        let geaendert = {
            let mut stand = self.stand();
            if stand.verbindung == verbindung {
                false
            } else {
                stand.verbindung = verbindung;
                true
            }
        };
        if geaendert {
            self.modell_schicken();
        }
    }

    /// Das Gerät ausbinden: alle Tools stoppen, Token löschen, Soll leeren.
    ///
    /// `abgelehnt` heißt: der Server hat das Gerät nicht mehr angenommen
    /// (entfernt oder Token ungültig) – das Fenster sagt dann „neu einbinden“.
    /// Die Server-Adresse bleibt in dem Fall stehen, damit sie nicht neu
    /// getippt werden muss.
    pub(crate) async fn ausbinden(self: &Arc<Self>, abgelehnt: bool) -> anyhow::Result<()> {
        let ids: Vec<EintragId> = self.stand().laeufer.keys().cloned().collect();
        for id in ids {
            stoppe_in(self, &id);
        }
        let geloescht = self.konfig.tresor.loeschen("geraet-token");
        {
            let mut stand = self.stand();
            stand.abgelehnt = abgelehnt;
            if !abgelehnt {
                stand.server_url = None;
            }
            stand.geraet = None;
            stand.token = None;
            stand.soll = Soll::neu(Vec::new());
            stand.anmeldung.clear();
            stand.einrichtung.clear();
            stand.einrichtung_laufend.clear();
            stand.verbindung = Verbindung::NichtEingebunden;
        }
        self.speichern();
        self.modell_schicken();
        self.liste_geaendert().await;
        geloescht
    }

    pub(crate) fn verbunden_setzen(&self, server: &str) {
        let geraet = self
            .stand()
            .geraet
            .as_ref()
            .map(|geraet| geraet.name.clone())
            .unwrap_or_default();
        self.verbindung_setzen(Verbindung::Verbunden {
            server: server.to_string(),
            geraet,
        });
    }

    /// Server-Adresse und Geräte-Token, falls eingebunden.
    pub(crate) fn zugang(&self) -> (Option<String>, Option<String>) {
        let stand = self.stand();
        (stand.server_url.clone(), stand.token.clone())
    }

    /// Die Geheimwerte der Soll-Einträge aus dem Tresor holen – die des
    /// Eintrags und die seiner Zugänge (Aufgabe 14).
    pub(crate) fn geheim_laden(&self, soll: &Soll) {
        let gespeicherte_zugaenge = self.stand().zugaenge.clone();
        let mut alle = BTreeMap::new();
        let mut zugangs_geheim: BTreeMap<EintragId, BTreeMap<String, Werte>> = BTreeMap::new();
        for eintrag in &soll.eintraege {
            let mut werte = Werte::new();
            for feld in eintrag
                .eintrag
                .felder
                .iter()
                .filter(|feld| feld.quelle == Quelle::Geheim)
            {
                let schluessel = geheim_schluessel(&eintrag.eintrag.id, &feld.name);
                match self.konfig.tresor.lesen(&schluessel) {
                    Ok(Some(wert)) => {
                        werte.insert(feld.name.clone(), wert);
                    }
                    Ok(None) => {}
                    Err(fehler) => tracing::warn!("{schluessel} nicht lesbar: {fehler}"),
                }
            }
            alle.insert(eintrag.eintrag.id.clone(), werte);

            let Some(mehrfach) = &eintrag.eintrag.mehrfach else {
                continue;
            };
            let geheim_felder: Vec<&str> = mehrfach
                .felder
                .iter()
                .filter(|feld| feld.quelle == Quelle::Geheim)
                .map(|feld| feld.name.as_str())
                .collect();
            let mut je_eintrag = BTreeMap::new();
            for name in gespeicherte_zugaenge
                .get(&eintrag.eintrag.id)
                .into_iter()
                .flat_map(|liste| liste.keys())
            {
                let mut werte = Werte::new();
                for feld in &geheim_felder {
                    let schluessel = zugang_geheim_schluessel(&eintrag.eintrag.id, name, feld);
                    match self.konfig.tresor.lesen(&schluessel) {
                        Ok(Some(wert)) => {
                            werte.insert((*feld).to_string(), wert);
                        }
                        Ok(None) => {}
                        Err(fehler) => tracing::warn!("{schluessel} nicht lesbar: {fehler}"),
                    }
                }
                je_eintrag.insert(name.clone(), werte);
            }
            zugangs_geheim.insert(eintrag.eintrag.id.clone(), je_eintrag);
        }
        let mut stand = self.stand();
        stand.geheim = alle;
        stand.zugang_geheim = zugangs_geheim;
    }

    /// Die gesetzten Geheimwerte eines Zugangs (leer, wenn keine).
    pub(crate) fn zugang_geheim(&self, id: &EintragId, name: &str) -> Werte {
        self.stand()
            .zugang_geheim
            .get(id)
            .and_then(|liste| liste.get(name))
            .cloned()
            .unwrap_or_default()
    }

    /// Die Zugänge eines Eintrags als JSON für [`MEHRFACH_UMGEBUNG`]. `None`,
    /// wenn der Eintrag keine Zugangsliste führt.
    pub(crate) fn zugaenge_umgebung(&self, id: &EintragId) -> Option<String> {
        let stand = self.stand();
        let eintrag = stand.soll.eintraege.iter().find(|e| &e.eintrag.id == id)?;
        let mehrfach = eintrag.eintrag.mehrfach.as_ref()?;
        let leer = BTreeMap::new();
        let zugaenge = stand.zugaenge.get(id).unwrap_or(&leer);
        let geheim = stand.zugang_geheim.get(id).unwrap_or(&leer);
        Some(zugaenge_json(mehrfach, zugaenge, geheim))
    }

    pub(crate) fn soll_eintrag(&self, id: &EintragId) -> Option<SollEintrag> {
        self.stand()
            .soll
            .eintraege
            .iter()
            .find(|eintrag| &eintrag.eintrag.id == id)
            .cloned()
    }

    /// Den Anmeldezustand eines Eintrags setzen – nur lokal, nie an den Server.
    pub(crate) fn anmeldung_setzen(&self, id: &EintragId, zustand: AnmeldeZustand) {
        {
            let mut stand = self.stand();
            if !stand.soll.eintraege.iter().any(|e| &e.eintrag.id == id) {
                return;
            }
            if stand.anmeldung.get(id) == Some(&zustand) {
                return;
            }
            stand.anmeldung.insert(id.clone(), zustand);
        }
        self.modell_schicken();
    }

    /// Den Einrichtungszustand eines Eintrags setzen – nur lokal, nie an den Server.
    pub(crate) fn einrichtung_setzen(&self, id: &EintragId, zustand: EinrichtZustand) {
        {
            let mut stand = self.stand();
            if !stand.soll.eintraege.iter().any(|e| &e.eintrag.id == id) {
                return;
            }
            if stand.einrichtung.get(id) == Some(&zustand) {
                return;
            }
            stand.einrichtung.insert(id.clone(), zustand);
        }
        self.modell_schicken();
    }

    /// Eine Einrichtung beginnen. `false`, wenn für diesen Eintrag schon eine
    /// läuft – dann tut ein zweiter Klick nichts (Aufgabe 11).
    pub(crate) fn einrichtung_beginnen(&self, id: &EintragId) -> bool {
        {
            let mut stand = self.stand();
            if !stand.einrichtung_laufend.insert(id.clone()) {
                return false;
            }
            stand
                .einrichtung
                .insert(id.clone(), EinrichtZustand::Laeuft);
        }
        self.modell_schicken();
        true
    }

    /// Die Marke „läuft“ wieder freigeben; den Zustand setzt der Aufrufer
    /// (`Bereit` oder `Fehler`), weil nur er den Ausgang kennt.
    pub(crate) fn einrichtung_freigeben(&self, id: &EintragId) {
        self.stand().einrichtung_laufend.remove(id);
    }

    /// Lokale und geheime Werte eines Eintrags.
    pub(crate) fn werte(&self, id: &EintragId) -> (Werte, Werte) {
        let stand = self.stand();
        (
            stand.lokale_werte.get(id).cloned().unwrap_or_default(),
            stand.geheim.get(id).cloned().unwrap_or_default(),
        )
    }

    /// In den Läufer eines Eintrags schreiben – aber nur, wenn er noch zu
    /// dieser Generation gehört. Sonst ist der Start überholt und schreibt
    /// nichts (Aufgabe 12). `false` heißt: nicht geschrieben.
    pub(crate) fn aendern(
        &self,
        id: &EintragId,
        generation: u64,
        aenderung: impl FnOnce(&mut Laeufer),
    ) -> bool {
        let mut stand = self.stand();
        match stand.laeufer.get_mut(id) {
            Some(laeufer) if laeufer.generation == generation => {
                aenderung(laeufer);
                true
            }
            _ => false,
        }
    }

    /// Fingerabdruck je Eintrag, der gerade läuft (oder es versucht).
    fn fingerabdruecke(&self) -> BTreeMap<EintragId, String> {
        self.stand()
            .laeufer
            .iter()
            .map(|(id, laeufer)| (id.clone(), laeufer.fingerabdruck.clone()))
            .collect()
    }

    /// Alle Werkzeuge aller laufenden Einträge, mit Präfix.
    pub(crate) fn werkzeuge(&self) -> Vec<Tool> {
        let stand = self.stand();
        let mut alle = Vec::new();
        for (id, laeufer) in &stand.laeufer {
            for tool in &laeufer.stand.tools {
                let mut kopie = tool.clone();
                kopie.name = quiver_domain::tool_name(id, &tool.name).into();
                kopie.input_schema = schema_mit_typ(&tool.input_schema);
                alle.push(kopie);
            }
        }
        alle.sort_by(|a, b| a.name.cmp(&b.name));
        alle
    }

    /// Der Agent zu einem Token – Vergleich über die Hashes in konstanter Zeit.
    pub(crate) fn agent_fuer_token(&self, token: &str) -> Option<String> {
        let stand = self.stand();
        agent_fuer(&stand.agenten, token)
    }

    /// Einen Eintrag anhalten und mit den aktuellen Werten neu starten.
    pub(crate) async fn neu_starten(self: &Arc<Self>, id: &EintragId) {
        stoppe_in(self, id);
        if let Some(eintrag) = self.soll_eintrag(id) {
            starte_in(self, &eintrag).await;
        }
        self.modell_schicken();
        self.liste_geaendert().await;
        self.melde().await;
    }

    /// Nach jeder Änderung der Werkzeugliste: allen Agenten Bescheid sagen.
    pub(crate) async fn liste_geaendert(&self) {
        let sitzungen: Vec<(u64, Peer<RoleServer>)> = self
            .sitzungen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .iter()
            .map(|(schluessel, sitzung)| (*schluessel, sitzung.peer.clone()))
            .collect();
        let mut tot = Vec::new();
        for (schluessel, peer) in sitzungen {
            if peer.notify_tool_list_changed().await.is_err() {
                tot.push(schluessel);
            }
        }
        if !tot.is_empty() {
            let mut sitzungen = self
                .sitzungen
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            let vorher = sitzungen.len();
            for schluessel in tot {
                sitzungen.remove(&schluessel);
            }
            let geschrumpft = sitzungen.len() != vorher;
            drop(sitzungen);
            if geschrumpft {
                self.modell_schicken();
            }
        }
    }

    /// Den aktuellen Stand an den Server melden.
    pub(crate) async fn melde(&self) {
        let (server, token) = self.zugang();
        let (Some(server), Some(token)) = (server, token) else {
            return;
        };
        let meldung = {
            let stand = self.stand();
            quiver_protocol::Meldung {
                revision: stand.soll.revision.clone(),
                client_version: CLIENT_VERSION.to_string(),
                eintraege: stand
                    .soll
                    .eintraege
                    .iter()
                    .map(|eintrag| {
                        let zustand = stand
                            .laeufer
                            .get(&eintrag.eintrag.id)
                            .map(|l| l.stand.zustand.clone())
                            .unwrap_or(EintragZustand::Startet);
                        (eintrag.eintrag.id.clone(), zustand)
                    })
                    .collect(),
            }
        };
        if let Err(fehler) = abgleich::meldung_senden(&self.http, &server, &token, &meldung).await {
            tracing::warn!("Meldung an den Server: {fehler}");
        }
    }
}

/// Ist dieser Name schon vergeben? Vergleich ohne Groß-/Kleinschreibung und
/// ohne Leerzeichen am Rand – „PI“ und „ pi “ sind derselbe Agent.
fn name_vergeben(agenten: &[profil::Agent], name: &str) -> bool {
    let gesucht = name.trim().to_lowercase();
    !gesucht.is_empty()
        && agenten
            .iter()
            .any(|agent| agent.name.trim().to_lowercase() == gesucht)
}

/// Der Agent zu einem Token, gesucht über den Hash (konstanter Vergleich).
fn agent_fuer(agenten: &[profil::Agent], token: &str) -> Option<String> {
    let hash = hilfen::hash(token);
    agenten
        .iter()
        .find(|agent| hilfen::gleich_konstant(&agent.token_hash, &hash))
        .map(|agent| agent.id.clone())
}

/// Schlüssel eines Geheimwerts im Tresor.
pub(crate) fn geheim_schluessel(id: &EintragId, feld: &str) -> String {
    format!("wert/{id}/{feld}")
}

/// Schlüssel eines Geheimwerts eines Zugangs im Tresor (Aufgabe 14).
/// Das Eingabeschema eines Tools, wie MCP es verlangt: ein JSON-Schema mit
/// `"type": "object"`. Manche Server schicken `{}` – pi nimmt das hin, Hermes
/// (pydantic) lehnt dann die **ganze** Tool-Liste ab. Fehlt der Typ, ergänzt
/// der Client `"type": "object"` (und leere `properties`).
pub(crate) fn schema_mit_typ(
    schema: &std::sync::Arc<serde_json::Map<String, serde_json::Value>>,
) -> std::sync::Arc<serde_json::Map<String, serde_json::Value>> {
    if schema.get("type").is_some() {
        return schema.clone();
    }
    let mut neu = (**schema).clone();
    neu.insert("type".to_string(), serde_json::Value::from("object"));
    neu.entry("properties".to_string())
        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    std::sync::Arc::new(neu)
}

pub(crate) fn zugang_geheim_schluessel(id: &EintragId, name: &str, feld: &str) -> String {
    format!("zugang/{id}/{name}/{feld}")
}

/// Den Anmeldezustand eines Eintrags prüfen und ablegen.
///
/// `None`, wenn der Eintrag keine Anmeldung braucht (dann passiert nichts).
/// Sonst das Ergebnis des `status`-Befehls: Exit 0 = angemeldet (erste Zeile
/// von stdout ist das Konto), Exit 1 = abgemeldet, alles andere ein Fehler.
async fn status_pruefen(innen: &Arc<Innen>, id: &EintragId) -> Option<AnmeldeZustand> {
    let soll = innen.soll_eintrag(id)?;
    soll.eintrag.anmeldung.as_ref()?;
    let (lokal, geheim) = innen.werte(id);
    let zustand =
        match anmeldung::befehl_bauen(&soll, anmeldung::Anmeldebefehl::Status, &lokal, &geheim) {
            Ok(befehl) => {
                let frist = anmeldung::Anmeldebefehl::Status.frist();
                match anmeldung::laufen(id, &befehl, frist).await {
                    Ok(ausgang) => anmeldung::zustand_aus(&ausgang),
                    Err(fehler) => AnmeldeZustand::Fehler {
                        text: format!("{fehler:#}"),
                    },
                }
            }
            Err(fehler) => AnmeldeZustand::Fehler {
                text: format!("{fehler:#}"),
            },
        };
    innen.anmeldung_setzen(id, zustand.clone());
    Some(zustand)
}

/// Den Status im Hintergrund prüfen – ein startender Eintrag wartet nicht
/// darauf.
fn status_im_hintergrund(innen: &Arc<Innen>, id: &EintragId) {
    let aufsicht = innen.clone();
    let id = id.clone();
    let aufgabe = tokio::spawn(async move {
        let _ = status_pruefen(&aufsicht, &id).await;
    });
    innen
        .aufgaben
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
        .push(aufgabe);
}

/// Alle 10 Minuten nachsehen, ob die Anmeldungen noch stehen (das Token kann
/// im Browser ablaufen, ohne dass der Eintrag etwas davon merkt).
async fn anmeldung_lauf(innen: Arc<Innen>) {
    loop {
        tokio::select! {
            _ = innen.ende.warten() => return,
            _ = tokio::time::sleep(STATUS_ABSTAND) => {}
        }
        let ids: Vec<EintragId> = innen
            .stand()
            .soll
            .eintraege
            .iter()
            .filter(|eintrag| eintrag.eintrag.anmeldung.is_some())
            .map(|eintrag| eintrag.eintrag.id.clone())
            .collect();
        for id in ids {
            if innen.ende.ist_abgebrochen() {
                return;
            }
            status_pruefen(&innen, &id).await;
        }
    }
}

/// Der Eintrag startet: Zustand merken und die Aufsicht starten.
///
/// Je Eintrag gibt es genau **einen** Läufer und damit genau eine wirksame
/// Start-Aufgabe: jeder Start bekommt eine neue Generation, der alte Läufer
/// wird verworfen und seine Aufgabe über ihr Abbruchsignal beendet. Die alte
/// Aufgabe wird nicht abgebrochen, sondern lässt ihren Aufbau los – sonst
/// bliebe ein Kindprozess zurück, dessen Dienst in einer eigenen Aufgabe lebt.
async fn starte_in(innen: &Arc<Innen>, eintrag: &SollEintrag) {
    let id = eintrag.eintrag.id.clone();
    let generation = innen
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    let abbruch = Abbruch::neu();
    let vorheriger = {
        let mut stand = innen.stand();
        stand.laeufer.insert(
            id.clone(),
            Laeufer {
                generation,
                fingerabdruck: eintrag.fingerabdruck(),
                stand: LaeuferStand::neu(EintragZustand::Startet),
                peer: None,
                abbruch: abbruch.clone(),
            },
        )
    };
    if let Some(vorheriger) = vorheriger {
        vorheriger.abbruch.abbrechen();
    }
    innen.modell_schicken();
    let aufsicht = innen.clone();
    let aufgabe =
        tokio::spawn(async move { eintrag_lauf(aufsicht, id, generation, abbruch).await });
    innen
        .aufgaben
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
        .push(aufgabe);
}

/// Der Eintrag soll aufhören. Die Aufgabe merkt es an ihrem Abbruchsignal.
fn stoppe_in(innen: &Arc<Innen>, id: &EintragId) {
    let laeufer = innen.stand().laeufer.remove(id);
    if let Some(laeufer) = laeufer {
        laeufer.abbruch.abbrechen();
    }
}

/// Die Aufsicht eines Eintrags: starten, Werkzeuge holen, warten, neu starten.
///
/// `generation` bindet die Aufgabe an ihren Läufer: schreibt ein neuer Start
/// denselben Eintrag, ist diese Aufgabe überholt und schreibt nichts mehr.
async fn eintrag_lauf(innen: Arc<Innen>, id: EintragId, generation: u64, abbruch: Abbruch) {
    let mut backoff = 1u64;
    // Einmal beim Start: steht die Anmeldung noch?
    status_im_hintergrund(&innen, &id);
    loop {
        if abbruch.ist_abgebrochen() || innen.ende.ist_abgebrochen() {
            return;
        }
        let Some(eintrag) = innen.soll_eintrag(&id) else {
            return;
        };
        let (lokal, geheim) = innen.werte(&id);
        let laufzeit = match quiver_domain::konfig_zusammensetzen(&eintrag, &lokal, &geheim) {
            Ok(laufzeit) => laufzeit,
            Err(fehlend) => {
                let felder = fehlend.iter().map(|feld| feld.name.clone()).collect();
                let geschrieben = innen.aendern(&id, generation, |laeufer| {
                    laeufer.stand = LaeuferStand {
                        zustand: EintragZustand::WartetAufWerte { felder },
                        fehlende: fehlend,
                        tools: Vec::new(),
                    };
                    laeufer.peer = None;
                });
                if geschrieben {
                    innen.modell_schicken();
                    innen.melde().await;
                }
                return;
            }
        };
        // Die Zugänge gehören in die Umgebung, nicht in die Felder: das Tool
        // liest sie beim Start (Aufgabe 14).
        let laufzeit = mit_zugaengen(&innen, &id, laufzeit);

        innen.aendern(&id, generation, |laeufer| {
            laeufer.stand.zustand = EintragZustand::Startet;
            laeufer.stand.fehlende.clear();
        });
        innen.modell_schicken();

        // Der Aufbau lauscht auf den Abbruch: wird der Eintrag während des
        // Startens ersetzt oder angehalten, gibt diese Aufgabe sofort auf,
        // statt nach dem Aufbau in den neuen Läufer zu schreiben (Aufgabe 12).
        let verbunden = tokio::select! {
            _ = abbruch.warten() => return,
            ergebnis = tokio::time::timeout(
                laufzeit::AUFBAU_TIMEOUT,
                laufzeit::verbinden(id.als_str(), &laufzeit),
            ) => ergebnis,
        };
        let mut verbindung = match verbunden {
            Ok(Ok(verbindung)) => verbindung,
            Ok(Err(fehler)) => {
                fehler_setzen(&innen, &id, generation, format!("{fehler:#}")).await;
                if !warten_auf_neustart(&abbruch, &mut backoff).await {
                    return;
                }
                continue;
            }
            Err(_) => {
                fehler_setzen(
                    &innen,
                    &id,
                    generation,
                    "Zeitüberschreitung beim Start".to_string(),
                )
                .await;
                if !warten_auf_neustart(&abbruch, &mut backoff).await {
                    return;
                }
                continue;
            }
        };

        if abbruch.ist_abgebrochen() {
            verbindung.beenden();
            return;
        }
        let werkzeuge = match verbindung.werkzeuge().await {
            Ok(werkzeuge) => werkzeuge,
            Err(fehler) => {
                verbindung.beenden();
                fehler_setzen(&innen, &id, generation, format!("{fehler:#}")).await;
                if !warten_auf_neustart(&abbruch, &mut backoff).await {
                    return;
                }
                continue;
            }
        };

        let anzahl = u32::try_from(werkzeuge.len()).unwrap_or(u32::MAX);
        let peer = verbindung.peer().clone();
        let geschrieben = innen.aendern(&id, generation, |laeufer| {
            laeufer.stand.tools = werkzeuge;
            laeufer.stand.zustand = EintragZustand::Laeuft { tools: anzahl };
            laeufer.peer = Some(peer);
        });
        if !geschrieben {
            // Ein neuerer Start hat den Eintrag übernommen: dieser hier ist
            // überholt und gibt seine Verbindung wieder her.
            tracing::info!(eintrag = %id, "Start überholt, wird verworfen");
            verbindung.beenden();
            return;
        }
        innen.modell_schicken();
        innen.liste_geaendert().await;
        innen.melde().await;
        backoff = 1;

        // Jetzt läuft es. Ende entweder durch uns (Abbruch) oder von selbst.
        let selbst_zu = tokio::select! {
            _ = abbruch.warten() => false,
            _ = innen.ende.warten() => false,
            _ = verbindung.warten() => true,
        };
        verbindung.beenden();
        if !innen.aendern(&id, generation, |laeufer| {
            laeufer.peer = None;
            laeufer.stand.tools.clear();
        }) {
            return;
        }
        if !selbst_zu {
            innen.modell_schicken();
            innen.liste_geaendert().await;
            return;
        }
        fehler_setzen(
            &innen,
            &id,
            generation,
            "Der Eintrag hat die Verbindung beendet".to_string(),
        )
        .await;
        if !warten_auf_neustart(&abbruch, &mut backoff).await {
            return;
        }
    }
}

/// Die Zugänge eines Eintrags in die Umgebung der Laufzeit setzen
/// (Aufgabe 14, [`MEHRFACH_UMGEBUNG`]). Einträge ohne `mehrfach` und entfernte
/// MCP-Server bleiben, wie sie sind.
fn mit_zugaengen(
    innen: &Arc<Innen>,
    id: &EintragId,
    laufzeit: quiver_domain::Laufzeit,
) -> quiver_domain::Laufzeit {
    let Some(zugaenge) = innen.zugaenge_umgebung(id) else {
        return laufzeit;
    };
    match laufzeit {
        quiver_domain::Laufzeit::Stdio {
            befehl,
            argumente,
            mut umgebung,
        } => {
            umgebung.insert(MEHRFACH_UMGEBUNG.to_string(), zugaenge);
            quiver_domain::Laufzeit::Stdio {
                befehl,
                argumente,
                umgebung,
            }
        }
        remote => remote,
    }
}

/// Zustand `Fehler` setzen, Modell und Agenten informieren, melden.
/// Ein überholter Start (andere Generation) schreibt nichts.
async fn fehler_setzen(innen: &Arc<Innen>, id: &EintragId, generation: u64, text: String) {
    tracing::warn!(eintrag = %id, "{text}");
    let geschrieben = innen.aendern(id, generation, |laeufer| {
        laeufer.stand = LaeuferStand::neu(EintragZustand::Fehler { text });
        laeufer.peer = None;
    });
    if !geschrieben {
        return;
    }
    innen.modell_schicken();
    innen.liste_geaendert().await;
    innen.melde().await;
}

/// Backoff 1 s … 30 s. `false`, wenn abgebrochen wurde.
async fn warten_auf_neustart(abbruch: &Abbruch, backoff: &mut u64) -> bool {
    tokio::select! {
        _ = abbruch.warten() => return false,
        _ = tokio::time::sleep(Duration::from_secs(*backoff)) => {}
    }
    *backoff = (*backoff * 2).min(30);
    true
}

/// Ist die Aktion inzwischen überholt? Zwischen dem Vergleich und dem
/// Ausführen kann ein zweiter Abgleich denselben Eintrag schon gestartet
/// haben – dann gibt es nichts zu tun (Aufgabe 12).
pub(crate) fn ueberholt(
    aktion: &quiver_domain::Aktion,
    ist: &BTreeMap<EintragId, String>,
    soll: &Soll,
) -> bool {
    match aktion {
        quiver_domain::Aktion::Stoppen(id) => !ist.contains_key(id),
        quiver_domain::Aktion::Starten(id) => ist.contains_key(id),
        quiver_domain::Aktion::Neustarten(id) => soll
            .eintraege
            .iter()
            .find(|eintrag| &eintrag.eintrag.id == id)
            .is_some_and(|eintrag| ist.get(id) == Some(&eintrag.fingerabdruck())),
    }
}

/// Den Soll-Zustand anwenden: vergleichen und die passenden Aktionen ausführen.
pub(crate) async fn anwenden(innen: &Arc<Innen>, soll: Soll) {
    innen.geheim_laden(&soll);
    let ist = innen.fingerabdruecke();
    let aktionen = quiver_domain::abgleichen(&soll, &ist);
    {
        let mut stand = innen.stand();
        stand.anmeldung.retain(|id, _| {
            soll.eintraege
                .iter()
                .any(|eintrag| &eintrag.eintrag.id == id)
        });
        stand.einrichtung.retain(|id, _| {
            soll.eintraege
                .iter()
                .any(|eintrag| &eintrag.eintrag.id == id)
        });
        stand.einrichtung_laufend.retain(|id| {
            soll.eintraege
                .iter()
                .any(|eintrag| &eintrag.eintrag.id == id)
        });
        stand.soll = soll.clone();
    }
    for aktion in aktionen {
        if ueberholt(&aktion, &innen.fingerabdruecke(), &soll) {
            continue;
        }
        match aktion {
            quiver_domain::Aktion::Stoppen(id) => stoppe_in(innen, &id),
            quiver_domain::Aktion::Neustarten(id) => {
                stoppe_in(innen, &id);
                starte_aus_soll(innen, &soll, &id).await;
            }
            quiver_domain::Aktion::Starten(id) => starte_aus_soll(innen, &soll, &id).await,
        }
    }
    innen.modell_schicken();
    innen.liste_geaendert().await;
    innen.speichern();
    innen.melde().await;
}

/// Einen Eintrag des Solls starten, wenn es ihn dort gibt.
async fn starte_aus_soll(innen: &Arc<Innen>, soll: &Soll, id: &EintragId) {
    if let Some(eintrag) = soll
        .eintraege
        .iter()
        .find(|eintrag| &eintrag.eintrag.id == id)
    {
        starte_in(innen, eintrag).await;
    }
}

/// Die Fassade für das Fenster. Billig zu klonen (ein Zeiger).
#[derive(Clone)]
pub struct Kern {
    innen: Arc<Innen>,
}

impl Kern {
    /// Hintergrundaufgaben starten, gespeicherten Stand laden, Endpunkt binden.
    pub async fn starten(konfig: KernKonfig) -> anyhow::Result<Kern> {
        if !konfig.mcp_adresse.ip().is_loopback() {
            anyhow::bail!(
                "Der MCP-Endpunkt darf nur lokal lauschen, nicht auf {}",
                konfig.mcp_adresse
            );
        }
        let gespeichert = profil::Profil::laden(&konfig.profil)?;
        let token = konfig.tresor.lesen("geraet-token")?;

        // Erst binden: dann steht die tatsächliche Adresse im Modell.
        let listener = tokio::net::TcpListener::bind(konfig.mcp_adresse).await?;
        let adresse = listener.local_addr()?;
        let endpunkt = format!("http://{adresse}/mcp");

        let geraet = gespeichert.geraet.clone();
        let verbindung = match (&gespeichert.server_url, &token) {
            (Some(server), Some(_)) => Verbindung::Verbindet {
                server: server.clone(),
            },
            _ => Verbindung::NichtEingebunden,
        };
        let stand = Stand {
            abgelehnt: false,
            server_url: gespeichert.server_url.clone(),
            geraet,
            token,
            lokale_werte: gespeichert
                .lokale_werte
                .iter()
                .map(|(id, werte)| (EintragId::neu(id.clone()), werte.clone()))
                .collect(),
            geheim: BTreeMap::new(),
            zugaenge: gespeichert
                .zugaenge
                .iter()
                .map(|(id, liste)| (EintragId::neu(id.clone()), liste.clone()))
                .collect(),
            zugang_geheim: BTreeMap::new(),
            agenten: gespeichert.agenten.clone(),
            soll: gespeichert
                .soll
                .clone()
                .unwrap_or_else(|| Soll::neu(Vec::new())),
            laeufer: BTreeMap::new(),
            anmeldung: BTreeMap::new(),
            einrichtung: BTreeMap::new(),
            einrichtung_laufend: BTreeSet::new(),
            verbindung,
            endpunkt: endpunkt.clone(),
        };
        let (sender, _) = watch::channel(stand.modell(&BTreeSet::new()));
        let innen = Arc::new(Innen {
            konfig: konfig.clone(),
            http: reqwest::Client::new(),
            stand: Mutex::new(stand),
            modell: sender,
            sitzungen: Mutex::new(BTreeMap::new()),
            zaehler: AtomicU64::new(0),
            ende: Abbruch::neu(),
            erneut: Notify::new(),
            aufgaben: Mutex::new(Vec::new()),
            generation: AtomicU64::new(0),
        });

        // Geheimnisse wieder aufnehmen. Der gespeicherte Soll-Zustand läuft
        // **nicht** an: kein Tool startet, bevor der Server das Gerät in diesem
        // Lauf angenommen hat (erster erfolgreicher Abgleich).
        let soll = innen.stand().soll.clone();
        innen.geheim_laden(&soll);
        innen.modell_schicken();

        let router = endpunkt::router(innen.clone());
        let ende = innen.ende.clone();
        let server = tokio::spawn(async move {
            let dienen = axum::serve(listener, router)
                .with_graceful_shutdown(async move { ende.warten().await });
            if let Err(fehler) = dienen.await {
                tracing::warn!("MCP-Endpunkt beendet: {fehler}");
            }
        });
        let abgleich = tokio::spawn(abgleich::lauf(innen.clone()));
        let status = tokio::spawn(anmeldung_lauf(innen.clone()));
        {
            let mut aufgaben = innen
                .aufgaben
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            aufgaben.push(server);
            aufgaben.push(abgleich);
            aufgaben.push(status);
        }

        tracing::info!("MCP-Endpunkt für Agenten: {endpunkt}");
        Ok(Kern { innen })
    }

    /// Das aktuelle Modell; ändert sich bei jedem Ereignis.
    pub fn modell(&self) -> watch::Receiver<Modell> {
        self.innen.modell.subscribe()
    }

    /// Gerät beim Server einbinden (Einmal-Code aus der Admin-Oberfläche).
    pub async fn einbinden(&self, server: &str, code: &str, name: &str) -> anyhow::Result<()> {
        let server = server.trim().trim_end_matches('/').to_string();
        if server.is_empty() {
            anyhow::bail!("Server-Adresse fehlt");
        }
        let antwort = abgleich::anmelden(&self.innen.http, &server, code, name).await?;
        self.innen
            .konfig
            .tresor
            .schreiben("geraet-token", &antwort.token)?;
        {
            let mut stand = self.innen.stand();
            stand.abgelehnt = false;
            stand.server_url = Some(server.clone());
            stand.geraet = Some(antwort.geraet);
            stand.token = Some(antwort.token);
            stand.verbindung = Verbindung::Verbindet {
                server: server.clone(),
            };
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        self.innen.erneut.notify_one();
        Ok(())
    }

    /// Gerät abmelden: Token weg, nichts läuft mehr.
    pub async fn abmelden(&self) -> anyhow::Result<()> {
        self.innen.ausbinden(false).await
    }

    /// Die Anmeldung eines Eintrags im Fenster auslösen (Aufgabe 10).
    ///
    /// Der Befehl öffnet selbst den Browser und wartet auf den User. Danach
    /// wird der Status geprüft und der Eintrag **neu gestartet**, damit der
    /// laufende MCP-Prozess das neue Token sieht.
    pub async fn anmelden(&self, eintrag: &EintragId) -> anyhow::Result<()> {
        self.innen.anmeldung_setzen(eintrag, AnmeldeZustand::Laeuft);
        self.anmelde_befehl(eintrag, anmeldung::Anmeldebefehl::Anmelden)
            .await?;
        status_pruefen(&self.innen, eintrag).await;
        self.innen.neu_starten(eintrag).await;
        Ok(())
    }

    /// Die Anmeldung eines Eintrags beenden: `abmelden`-Befehl, Status
    /// prüfen, Eintrag neu starten.
    pub async fn abmelden_eintrag(&self, eintrag: &EintragId) -> anyhow::Result<()> {
        self.anmelde_befehl(eintrag, anmeldung::Anmeldebefehl::Abmelden)
            .await?;
        status_pruefen(&self.innen, eintrag).await;
        self.innen.neu_starten(eintrag).await;
        Ok(())
    }

    /// Die Einrichtung eines Eintrags im Fenster auslösen (Aufgabe 11).
    ///
    /// Der Befehl öffnet selbst die Seite im Browser und endet, wenn der User
    /// fertig ist – **ohne Frist**. Exit 0 heißt „eingerichtet“; dann wird der
    /// Eintrag **neu gestartet**, damit neue Profile sofort wirken. Exit 1
    /// heißt „abgebrochen“: `Bereit`, ohne Neustart. Alles andere ist ein
    /// Fehler und steht im Einrichtungszustand.
    ///
    /// Höchstens eine Einrichtung je Eintrag: ein zweiter Klick tut nichts.
    pub async fn einrichten(&self, eintrag: &EintragId) -> anyhow::Result<()> {
        let soll = self.innen.soll_eintrag(eintrag).ok_or_else(|| {
            anyhow::anyhow!("Eintrag {eintrag} ist diesem Gerät nicht zugewiesen")
        })?;
        if !self.innen.einrichtung_beginnen(eintrag) {
            return Ok(());
        }
        let ergebnis = self.einrichtung_ausfuehren(eintrag, &soll).await;
        self.innen.einrichtung_freigeben(eintrag);
        match ergebnis {
            Ok(ausgang) => match einrichtung::ergebnis_aus(&ausgang) {
                einrichtung::Ergebnis::Fertig { neustart } => {
                    self.innen
                        .einrichtung_setzen(eintrag, EinrichtZustand::Bereit);
                    if neustart {
                        self.innen.neu_starten(eintrag).await;
                    }
                    Ok(())
                }
                einrichtung::Ergebnis::Fehler { text } => {
                    tracing::warn!(eintrag = %eintrag, "{text}");
                    self.innen.einrichtung_setzen(
                        eintrag,
                        EinrichtZustand::Fehler { text: text.clone() },
                    );
                    anyhow::bail!(text)
                }
            },
            Err(fehler) => {
                let text = format!("{fehler:#}");
                self.innen
                    .einrichtung_setzen(eintrag, EinrichtZustand::Fehler { text: text.clone() });
                Err(fehler)
            }
        }
    }

    /// Die Befehlszeile bauen und ausführen.
    async fn einrichtung_ausfuehren(
        &self,
        eintrag: &EintragId,
        soll: &SollEintrag,
    ) -> anyhow::Result<anmeldung::Ausgang> {
        let (lokal, geheim) = self.innen.werte(eintrag);
        let befehl = einrichtung::befehl_bauen(soll, &lokal, &geheim)?;
        einrichtung::laufen(eintrag, &befehl).await
    }

    /// Einen der drei Anmelde-Befehle ausführen. Scheitert er, steht der
    /// Grund im Anmeldezustand — das Fenster zeigt ihn dann selbst.
    async fn anmelde_befehl(
        &self,
        eintrag: &EintragId,
        art: anmeldung::Anmeldebefehl,
    ) -> anyhow::Result<()> {
        let soll = self.innen.soll_eintrag(eintrag).ok_or_else(|| {
            anyhow::anyhow!("Eintrag {eintrag} ist diesem Gerät nicht zugewiesen")
        })?;
        let (lokal, geheim) = self.innen.werte(eintrag);
        let befehl = anmeldung::befehl_bauen(&soll, art, &lokal, &geheim)?;
        let ausgang = match anmeldung::laufen(eintrag, &befehl, art.frist()).await {
            Ok(ausgang) => ausgang,
            Err(fehler) => {
                self.innen.anmeldung_setzen(
                    eintrag,
                    AnmeldeZustand::Fehler {
                        text: format!("{fehler:#}"),
                    },
                );
                return Err(fehler);
            }
        };
        let text = match ausgang.code {
            Some(0) => return Ok(()),
            Some(code) => format!("Der Befehl „{}“ endet mit Code {code}", art.name()),
            None => format!("Der Befehl „{}“ wurde abgebrochen", art.name()),
        };
        tracing::warn!(eintrag = %eintrag, "{text}");
        self.innen
            .anmeldung_setzen(eintrag, AnmeldeZustand::Fehler { text: text.clone() });
        anyhow::bail!(text)
    }

    /// Werte eines Eintrags setzen. Geheimes landet im Tresor, nie im Profil.
    pub async fn werte_setzen(
        &self,
        eintrag: &EintragId,
        lokal: Werte,
        geheim: Werte,
    ) -> anyhow::Result<()> {
        for (name, wert) in &geheim {
            let schluessel = geheim_schluessel(eintrag, name);
            if wert.is_empty() {
                self.innen.konfig.tresor.loeschen(&schluessel)?;
            } else {
                self.innen.konfig.tresor.schreiben(&schluessel, wert)?;
            }
        }
        {
            let mut stand = self.innen.stand();
            // Gesetzt wird je Feld; ein leerer Wert löscht das Feld.
            let gespeichert = stand.lokale_werte.entry(eintrag.clone()).or_default();
            for (name, wert) in lokal {
                if wert.is_empty() {
                    gespeichert.remove(&name);
                } else {
                    gespeichert.insert(name, wert);
                }
            }
            let gespeichert = stand.geheim.entry(eintrag.clone()).or_default();
            for (name, wert) in geheim {
                if wert.is_empty() {
                    gespeichert.remove(&name);
                } else {
                    gespeichert.insert(name, wert);
                }
            }
        }
        self.innen.speichern();
        self.innen.neu_starten(eintrag).await;
        Ok(())
    }

    /// Einen Zugang anlegen oder ändern (Aufgabe 14).
    ///
    /// Die Zugangsliste gehört dem User: der Name muss
    /// [`quiver_domain::zugangsname_gueltig`] sein, die Pflichtfelder aus
    /// `mehrfach.felder` müssen gefüllt sein. Nicht-geheime Werte landen in der
    /// Client-Konfiguration, Geheimes im Tresor unter
    /// `zugang/<eintrag>/<name>/<feld>`. Ein **leeres Geheimfeld bleibt
    /// unverändert**; ein leerer lokaler Wert löscht das Feld.
    ///
    /// Danach startet der Eintrag neu, damit das Tool die neue Liste sieht.
    pub async fn zugang_speichern(
        &self,
        eintrag: &EintragId,
        name: &str,
        lokal: Werte,
        geheim: Werte,
    ) -> anyhow::Result<()> {
        let mehrfach = self.mehrfach(eintrag)?;
        let name = name.trim();
        if !quiver_domain::zugangsname_gueltig(name) {
            anyhow::bail!("„{name}“ ist kein gültiger Name: a–z, 0–9, - und _, 1–40 Zeichen");
        }
        // Ein leeres Geheimfeld heißt „unverändert“ – schon Gesetztes zählt mit.
        let vorhanden = self.innen.zugang_geheim(eintrag, name);
        let mut fehlend = Vec::new();
        for feld in mehrfach.felder.iter().filter(|feld| feld.pflicht) {
            let wert = match feld.quelle {
                Quelle::Geheim => geheim
                    .get(&feld.name)
                    .filter(|wert| !wert.trim().is_empty())
                    .or_else(|| vorhanden.get(&feld.name)),
                _ => lokal.get(&feld.name),
            };
            if wert.is_none_or(|wert| wert.trim().is_empty()) {
                fehlend.push(feld.name.clone());
            }
        }
        if !fehlend.is_empty() {
            anyhow::bail!("Es fehlen Angaben: {}", fehlend.join(", "));
        }
        for (feld, wert) in &geheim {
            if wert.trim().is_empty() {
                continue;
            }
            self.innen
                .konfig
                .tresor
                .schreiben(&zugang_geheim_schluessel(eintrag, name, feld), wert)?;
        }
        // Nur die Felder, die die Zugangsliste kennt.
        let mut gespeichert = Werte::new();
        for feld in &mehrfach.felder {
            if feld.quelle == Quelle::Geheim {
                continue;
            }
            let wert = lokal.get(&feld.name).map(|wert| wert.trim()).unwrap_or("");
            if !wert.is_empty() {
                gespeichert.insert(feld.name.clone(), wert.to_string());
            }
        }
        {
            let mut stand = self.innen.stand();
            stand
                .zugaenge
                .entry(eintrag.clone())
                .or_default()
                .insert(name.to_string(), gespeichert);
            let gesetzte_geheim = stand
                .zugang_geheim
                .entry(eintrag.clone())
                .or_default()
                .entry(name.to_string())
                .or_default();
            for (feld, wert) in geheim {
                if wert.trim().is_empty() {
                    continue;
                }
                gesetzte_geheim.insert(feld, wert);
            }
        }
        self.innen.speichern();
        self.innen.neu_starten(eintrag).await;
        Ok(())
    }

    /// Einen Zugang entfernen – samt seiner Geheimnisse im Tresor (Aufgabe 14).
    pub async fn zugang_loeschen(&self, eintrag: &EintragId, name: &str) -> anyhow::Result<()> {
        let mehrfach = self.mehrfach(eintrag)?;
        let name = name.trim();
        let vorhanden = self.innen.zugang_geheim(eintrag, name);
        // Auch Geheimnisse zu Feldern löschen, die es nicht mehr gibt.
        let mut felder: BTreeSet<String> = mehrfach
            .felder
            .iter()
            .filter(|feld| feld.quelle == Quelle::Geheim)
            .map(|feld| feld.name.clone())
            .collect();
        felder.extend(vorhanden.keys().cloned());
        for feld in &felder {
            self.innen
                .konfig
                .tresor
                .loeschen(&zugang_geheim_schluessel(eintrag, name, feld))?;
        }
        {
            let mut stand = self.innen.stand();
            if let Some(liste) = stand.zugaenge.get_mut(eintrag) {
                liste.remove(name);
            }
            if let Some(liste) = stand.zugang_geheim.get_mut(eintrag) {
                liste.remove(name);
            }
        }
        self.innen.speichern();
        self.innen.neu_starten(eintrag).await;
        Ok(())
    }

    /// Ein einzelnes Geheimnis eines Zugangs entfernen – etwa das
    /// Anmelde-Passwort, nachdem der Schlüssel eingerichtet ist (Aufgabe 14).
    pub async fn zugang_geheim_loeschen(
        &self,
        eintrag: &EintragId,
        name: &str,
        feld: &str,
    ) -> anyhow::Result<()> {
        self.mehrfach(eintrag)?;
        let name = name.trim();
        self.innen
            .konfig
            .tresor
            .loeschen(&zugang_geheim_schluessel(eintrag, name, feld))?;
        {
            let mut stand = self.innen.stand();
            if let Some(liste) = stand.zugang_geheim.get_mut(eintrag) {
                if let Some(werte) = liste.get_mut(name) {
                    werte.remove(feld);
                }
            }
        }
        self.innen.speichern();
        self.innen.neu_starten(eintrag).await;
        Ok(())
    }

    /// Die Zugangsliste eines Eintrags – ein Fehler, wenn er keine führt.
    fn mehrfach(&self, eintrag: &EintragId) -> anyhow::Result<Mehrfach> {
        let soll = self.innen.soll_eintrag(eintrag).ok_or_else(|| {
            anyhow::anyhow!("Eintrag {eintrag} ist diesem Gerät nicht zugewiesen")
        })?;
        soll.eintrag
            .mehrfach
            .ok_or_else(|| anyhow::anyhow!("Eintrag {eintrag} führt keine Zugangsliste"))
    }

    /// Einen Agenten koppeln. Das Token kommt **einmal** zurück; gespeichert
    /// wird nur sein Hash.
    ///
    /// Jeder Agent nur einmal: ein Name, den es schon gibt (ohne Groß-/
    /// Kleinschreibung und ohne Leerzeichen am Rand), wird abgelehnt. Mehrere
    /// *Sitzungen* desselben Agenten sind davon nicht betroffen – sie teilen
    /// sich das eine Token.
    pub async fn agent_koppeln(&self, name: &str) -> anyhow::Result<String> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("Name fehlt");
        }
        let vergeben = name_vergeben(&self.innen.stand().agenten, name);
        if vergeben {
            anyhow::bail!("Agent {name} ist schon gekoppelt");
        }
        let token = hilfen::token_erzeugen();
        let id = hilfen::kennung_erzeugen();
        {
            let mut stand = self.innen.stand();
            stand.agenten.push(profil::Agent {
                id,
                name: name.to_string(),
                token_hash: hilfen::hash(&token),
            });
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        Ok(token)
    }

    /// Neues Token für einen gekoppelten Agenten. Das alte gilt sofort nicht
    /// mehr: offene Sitzungen werden beim nächsten Aufruf mit 401 abgewiesen.
    pub async fn agent_token_erneuern(&self, id: &str) -> anyhow::Result<String> {
        let token = hilfen::token_erzeugen();
        let hash = hilfen::hash(&token);
        {
            let mut stand = self.innen.stand();
            let agent = stand
                .agenten
                .iter_mut()
                .find(|agent| agent.id == id)
                .ok_or_else(|| anyhow::anyhow!("Agent {id} ist nicht gekoppelt"))?;
            agent.token_hash = hash;
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        Ok(token)
    }

    /// Einem Agenten den Zugang nehmen.
    pub async fn agent_entfernen(&self, id: &str) -> anyhow::Result<()> {
        {
            let mut stand = self.innen.stand();
            stand.agenten.retain(|agent| agent.id != id);
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        Ok(())
    }

    /// Alles anhalten (Fenster zu, Strg+C).
    pub async fn beenden(&self) {
        self.innen.ende.abbrechen();
        let ids: Vec<EintragId> = self.innen.stand().laeufer.keys().cloned().collect();
        for id in ids {
            stoppe_in(&self.innen, &id);
        }
        let aufgaben: Vec<tokio::task::JoinHandle<()>> = {
            let mut aufgaben = self
                .innen
                .aufgaben
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            std::mem::take(&mut *aufgaben)
        };
        for aufgabe in aufgaben {
            let _ = tokio::time::timeout(Duration::from_secs(3), aufgabe).await;
        }
    }
}

impl std::fmt::Debug for Kern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Kern").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
