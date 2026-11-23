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

use quiver_domain::{EintragId, FehlendesFeld, Feld, Geraet, Quelle, Soll, SollEintrag, Werte};
use quiver_protocol::EintragZustand;
use rmcp::model::Tool;
use rmcp::{Peer, RoleClient, RoleServer};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Notify};

pub use anmeldung::AnmeldeZustand;
use hilfen::Abbruch;
pub use tresor::{Schluesselbund, SpeicherTresor, Tresor};

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
    /// Wo Geheimnisse liegen (Standard: Schlüsselbund, in Tests: im Speicher).
    pub tresor: Arc<dyn Tresor>,
}

impl KernKonfig {
    /// Die üblichen Werte.
    pub fn standard() -> Self {
        let profil = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("mcp-quiver");
        Self {
            profil,
            mcp_adresse: SocketAddr::from(([127, 0, 0, 1], 8741)),
            tresor: Arc::new(Schluesselbund),
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
    /// Verbindung weg – der letzte Soll-Zustand läuft weiter.
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
    server_url: Option<String>,
    geraet: Option<Geraet>,
    token: Option<String>,
    lokale_werte: BTreeMap<EintragId, Werte>,
    /// Geheimwerte im Speicher (Quelle bleibt der Tresor).
    geheim: BTreeMap<EintragId, Werte>,
    agenten: Vec<profil::Agent>,
    soll: Soll,
    laeufer: BTreeMap<EintragId, Laeufer>,
    /// Anmeldezustand je Eintrag; nur lokal, gehört nicht zur Meldung.
    anmeldung: BTreeMap<EintragId, AnmeldeZustand>,
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
                &self.lokale_werte,
                &geheim_gesetzt,
                &self.laeufer,
                &self.anmeldung,
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
            agenten: self.agenten.clone(),
            soll: Some(self.soll.clone()),
        }
    }
}

/// Die Sicht auf die Einträge: Soll + lokale Werte → was das Fenster zeigt.
fn eintraege_sicht(
    soll: &Soll,
    lokale_werte: &BTreeMap<EintragId, Werte>,
    geheim_gesetzt: &BTreeMap<EintragId, BTreeSet<String>>,
    laeufer: &BTreeMap<EintragId, Laeufer>,
    anmeldung: &BTreeMap<EintragId, AnmeldeZustand>,
) -> Vec<EintragSicht> {
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
            }
        })
        .collect()
}

/// Innerei des Kerns; geteilt zwischen Fassade, Abgleich und Endpunkt.
pub(crate) struct Innen {
    pub(crate) konfig: KernKonfig,
    pub(crate) http: reqwest::Client,
    pub(crate) stand: Mutex<Stand>,
    pub(crate) modell: watch::Sender<Modell>,
    pub(crate) sitzungen: Mutex<BTreeMap<u64, Sitzung>>,
    pub(crate) zaehler: AtomicU64,
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

    /// Die Geheimwerte der Soll-Einträge aus dem Tresor holen.
    pub(crate) fn geheim_laden(&self, soll: &Soll) {
        let mut alle = BTreeMap::new();
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
        }
        self.stand().geheim = alle;
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

    /// Lokale und geheime Werte eines Eintrags.
    pub(crate) fn werte(&self, id: &EintragId) -> (Werte, Werte) {
        let stand = self.stand();
        (
            stand.lokale_werte.get(id).cloned().unwrap_or_default(),
            stand.geheim.get(id).cloned().unwrap_or_default(),
        )
    }

    pub(crate) fn aendern(&self, id: &EintragId, aenderung: impl FnOnce(&mut Laeufer)) {
        let mut stand = self.stand();
        if let Some(laeufer) = stand.laeufer.get_mut(id) {
            aenderung(laeufer);
        }
    }

    /// Alle Werkzeuge aller laufenden Einträge, mit Präfix.
    pub(crate) fn werkzeuge(&self) -> Vec<Tool> {
        let stand = self.stand();
        let mut alle = Vec::new();
        for (id, laeufer) in &stand.laeufer {
            for tool in &laeufer.stand.tools {
                let mut kopie = tool.clone();
                kopie.name = quiver_domain::tool_name(id, &tool.name).into();
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
async fn starte_in(innen: &Arc<Innen>, eintrag: &SollEintrag) {
    let id = eintrag.eintrag.id.clone();
    let abbruch = Abbruch::neu();
    {
        let mut stand = innen.stand();
        stand.laeufer.insert(
            id.clone(),
            Laeufer {
                fingerabdruck: eintrag.fingerabdruck(),
                stand: LaeuferStand::neu(EintragZustand::Startet),
                peer: None,
                abbruch: abbruch.clone(),
            },
        );
    }
    innen.modell_schicken();
    let aufsicht = innen.clone();
    let aufgabe = tokio::spawn(async move { eintrag_lauf(aufsicht, id, abbruch).await });
    innen
        .aufgaben
        .lock()
        .unwrap_or_else(|vergiftet| vergiftet.into_inner())
        .push(aufgabe);
}

/// Der Eintrag soll aufhören.
fn stoppe_in(innen: &Arc<Innen>, id: &EintragId) {
    let laeufer = innen.stand().laeufer.remove(id);
    if let Some(laeufer) = laeufer {
        laeufer.abbruch.abbrechen();
    }
}

/// Die Aufsicht eines Eintrags: starten, Werkzeuge holen, warten, neu starten.
async fn eintrag_lauf(innen: Arc<Innen>, id: EintragId, abbruch: Abbruch) {
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
                innen.aendern(&id, |laeufer| {
                    laeufer.stand = LaeuferStand {
                        zustand: EintragZustand::WartetAufWerte { felder },
                        fehlende: fehlend,
                        tools: Vec::new(),
                    };
                    laeufer.peer = None;
                });
                innen.modell_schicken();
                innen.melde().await;
                return;
            }
        };

        innen.aendern(&id, |laeufer| {
            laeufer.stand.zustand = EintragZustand::Startet;
            laeufer.stand.fehlende.clear();
        });
        innen.modell_schicken();

        let verbunden = tokio::time::timeout(
            laufzeit::AUFBAU_TIMEOUT,
            laufzeit::verbinden(id.als_str(), &laufzeit),
        )
        .await;
        let mut verbindung = match verbunden {
            Ok(Ok(verbindung)) => verbindung,
            Ok(Err(fehler)) => {
                fehler_setzen(&innen, &id, format!("{fehler:#}")).await;
                if !warten_auf_neustart(&abbruch, &mut backoff).await {
                    return;
                }
                continue;
            }
            Err(_) => {
                fehler_setzen(&innen, &id, "Zeitüberschreitung beim Start".to_string()).await;
                if !warten_auf_neustart(&abbruch, &mut backoff).await {
                    return;
                }
                continue;
            }
        };

        let werkzeuge = match verbindung.werkzeuge().await {
            Ok(werkzeuge) => werkzeuge,
            Err(fehler) => {
                verbindung.beenden();
                fehler_setzen(&innen, &id, format!("{fehler:#}")).await;
                if !warten_auf_neustart(&abbruch, &mut backoff).await {
                    return;
                }
                continue;
            }
        };

        let anzahl = u32::try_from(werkzeuge.len()).unwrap_or(u32::MAX);
        {
            let peer = verbindung.peer().clone();
            innen.aendern(&id, |laeufer| {
                laeufer.stand.tools = werkzeuge;
                laeufer.stand.zustand = EintragZustand::Laeuft { tools: anzahl };
                laeufer.peer = Some(peer);
            });
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
        innen.aendern(&id, |laeufer| {
            laeufer.peer = None;
            laeufer.stand.tools.clear();
        });
        if !selbst_zu {
            innen.modell_schicken();
            innen.liste_geaendert().await;
            return;
        }
        fehler_setzen(
            &innen,
            &id,
            "Der Eintrag hat die Verbindung beendet".to_string(),
        )
        .await;
        if !warten_auf_neustart(&abbruch, &mut backoff).await {
            return;
        }
    }
}

/// Zustand `Fehler` setzen, Modell und Agenten informieren, melden.
async fn fehler_setzen(innen: &Arc<Innen>, id: &EintragId, text: String) {
    tracing::warn!(eintrag = %id, "{text}");
    innen.aendern(id, |laeufer| {
        laeufer.stand = LaeuferStand::neu(EintragZustand::Fehler { text });
        laeufer.peer = None;
    });
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

/// Den Soll-Zustand anwenden: vergleichen und die passenden Aktionen ausführen.
pub(crate) async fn anwenden(innen: &Arc<Innen>, soll: Soll) {
    innen.geheim_laden(&soll);
    let ist: BTreeMap<EintragId, String> = {
        let stand = innen.stand();
        stand
            .laeufer
            .iter()
            .map(|(id, laeufer)| (id.clone(), laeufer.fingerabdruck.clone()))
            .collect()
    };
    let aktionen = quiver_domain::abgleichen(&soll, &ist);
    {
        let mut stand = innen.stand();
        stand.anmeldung.retain(|id, _| {
            soll.eintraege
                .iter()
                .any(|eintrag| &eintrag.eintrag.id == id)
        });
        stand.soll = soll.clone();
    }
    for aktion in aktionen {
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
            server_url: gespeichert.server_url.clone(),
            geraet,
            token,
            lokale_werte: gespeichert
                .lokale_werte
                .iter()
                .map(|(id, werte)| (EintragId::neu(id.clone()), werte.clone()))
                .collect(),
            geheim: BTreeMap::new(),
            agenten: gespeichert.agenten.clone(),
            soll: gespeichert
                .soll
                .clone()
                .unwrap_or_else(|| Soll::neu(Vec::new())),
            laeufer: BTreeMap::new(),
            anmeldung: BTreeMap::new(),
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
        });

        // Geheimnisse und den letzten Soll-Zustand wieder aufnehmen. Ist ein
        // Server eingebunden, läuft der gespeicherte Stand sofort an – auch
        // wenn der Server gerade nicht erreichbar ist (offline).
        let soll = innen.stand().soll.clone();
        innen.geheim_laden(&soll);
        innen.modell_schicken();
        let (server, token) = innen.zugang();
        if server.is_some() && token.is_some() && !soll.eintraege.is_empty() {
            anwenden(&innen, soll).await;
        }

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
        let ids: Vec<EintragId> = self.innen.stand().laeufer.keys().cloned().collect();
        for id in ids {
            stoppe_in(&self.innen, &id);
        }
        self.innen.konfig.tresor.loeschen("geraet-token")?;
        {
            let mut stand = self.innen.stand();
            stand.server_url = None;
            stand.geraet = None;
            stand.token = None;
            stand.soll = Soll::neu(Vec::new());
            stand.anmeldung.clear();
            stand.verbindung = Verbindung::NichtEingebunden;
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        self.innen.liste_geaendert().await;
        Ok(())
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
