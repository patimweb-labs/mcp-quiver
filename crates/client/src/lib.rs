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
pub mod einstellungen;
pub(crate) mod endpunkt;
mod hilfen;
mod laufzeit;
mod netz;
mod profil;
pub mod protokoll;
mod server_anmeldung;
mod sse;
mod tresor;

use std::collections::{BTreeMap, BTreeSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use quiver_domain::{
    EintragId, FehlendesFeld, Feld, Geraet, Mehrfach, Quelle, Soll, SollEintrag, Werte, Zugang,
    MEHRFACH_UMGEBUNG,
};
use quiver_protocol::{AnmeldeGrund, Anmeldearten, EintragZustand, PRAEFIX};
use rmcp::model::Tool;
use rmcp::{Peer, RoleClient, RoleServer};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Notify};

pub use anmeldung::AnmeldeZustand;
pub use einrichtung::EinrichtZustand;
pub use einstellungen::{Vorschau, VorschauArt, VorschauEintrag, Zusammenfassung};
use hilfen::Abbruch;
pub use protokoll::{Aufruf, LogFilter, Zeile};
pub use server_anmeldung::{Browser, SystemBrowser};
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
    /// Wer die Anmeldeseite des Servers öffnet (Aufgabe 22). Standard: das
    /// System; der Ende-zu-Ende-Test setzt eine Attrappe ein.
    pub browser: Arc<dyn Browser>,
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
            browser: Arc::new(SystemBrowser),
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
    /// Anmeldung läuft (Server fragen, Code tauschen).
    Verbindet { server: String },
    /// Der Browser ist offen; der Lauscher wartet auf genau eine Antwort.
    /// `adresse` ist die Anmeldeseite des Servers (für „Browser erneut öffnen“).
    WartetAufBrowser { server: String, adresse: String },
    /// Verbunden; `geraet` ist der Anzeigename des eigenen Geräts.
    Verbunden { server: String, geraet: String },
    /// Verbindung weg – was nach der Anmeldung lief, läuft weiter.
    Getrennt { server: String, fehler: String },
    /// Der Server verlangt eine neue Anmeldung (`neu-anmelden` oder ein
    /// unbekannter Grund): alle Tools sind aus, **Geräte-Token und alle
    /// Angaben bleiben**, der Knopf „Erneut anmelden“ führt zurück.
    NeuAnmelden { server: String, text: String },
    /// Die Anmeldung ist gescheitert (gesperrt, kein Zugriff, abgebrochen,
    /// Zeit abgelaufen, Server nicht erreichbar). Kein Geräte-Token, die
    /// Server-Adresse bleibt vorbelegt.
    AnmeldeFehler { server: String, text: String },
}

impl Verbindung {
    /// Die Server-Adresse, falls es eine gibt (auch nach dem Abmelden).
    pub fn server(&self) -> Option<&str> {
        match self {
            Verbindung::NichtEingebunden => None,
            Verbindung::Verbindet { server }
            | Verbindung::WartetAufBrowser { server, .. }
            | Verbindung::Verbunden { server, .. }
            | Verbindung::Getrennt { server, .. }
            | Verbindung::NeuAnmelden { server, .. }
            | Verbindung::AnmeldeFehler { server, .. } => Some(server),
        }
    }
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
    /// Hat der User diesen Eintrag **auf diesem Gerät** abgeschaltet
    /// (Aufgabe 19)? Dann steht `zustand` auf
    /// [`EintragZustand::AmGeraetAus`] und nichts startet ihn.
    #[serde(default)]
    pub am_geraet_aus: bool,
    /// Warum dieser Eintrag nicht startet, obwohl er zugewiesen ist
    /// (Paket 3): seine Kennung taugt nicht als Präfix der Werkzeugnamen.
    /// Dann steht hier der Grund im Klartext – das Fenster zeigt ihn.
    #[serde(default)]
    pub unzulaessig: Option<String>,
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

/// Welche Wege der Server für die Anmeldung dieses Geräts anbietet.
///
/// Kommt aus `GET /api/v1/anmeldearten` und gilt nur für die Adresse, für die
/// gefragt wurde. Ändert der User die Adresse, ist der Eintrag nicht mehr
/// gültig — die Ansicht zeigt dann wieder „Weiter“.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anmeldewege {
    pub server: String,
    /// Entra: Knopf „Mit Microsoft anmelden“.
    pub entra: bool,
    /// Einmal-Code: ohne Entra der Weg dieses Servers.
    pub code: bool,
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
    /// Warum das Gerät abgelehnt wurde (Aufgabe 20/22): `gesperrt`,
    /// `kein-zugriff` oder `neu-anmelden`. `None` heißt: kein Grund bekannt.
    #[serde(default)]
    pub anmelde_grund: Option<AnmeldeGrund>,
    /// Anzeigename des angemeldeten Benutzers („Angemeldet als …“). Kein
    /// Geheimnis; kommt aus der Anmeldeantwort.
    #[serde(default)]
    pub benutzer: Option<String>,
    /// Die zuletzt benutzte Server-Adresse — auch nach dem Abmelden, damit
    /// sie nicht neu getippt werden muss.
    #[serde(default)]
    pub server_url: Option<String>,
    /// Welche Anmeldewege der Server für `server_url` genannt hat.
    #[serde(default)]
    pub anmeldewege: Option<Anmeldewege>,
    /// Dürfen Agenten Inhalte des Protokolls sehen (Aufgabe 17)? Standard:
    /// aus – dann gibt es nur Metadaten.
    #[serde(default)]
    pub protokoll_fuer_agenten: bool,
    /// Warum der Tresor nicht lesbar ist (Paket 3). Solange hier etwas steht,
    /// wird nichts geschrieben und **kein** neuer Schlüssel erzeugt; das
    /// Fenster bietet „Neu beginnen“ an.
    #[serde(default)]
    pub tresor_fehler: Option<String>,
    /// Der gespeicherte Stand war nicht lesbar und liegt als
    /// `.kaputt-<zeit>` daneben (Paket 3). Der Client läuft weiter.
    #[serde(default)]
    pub profil_hinweis: Option<String>,
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
    /// Wann diese Sitzung zuletzt benutzt wurde (Paket 3: sie läuft nach
    /// 10 Minuten ohne Nutzung ab).
    zuletzt: std::time::Instant,
}

/// So viele Sitzungen hält der Endpunkt höchstens.
pub const SITZUNGEN_MAX: usize = 64;
/// So lange bleibt eine Sitzung ohne Nutzung bestehen.
pub const SITZUNG_FRIST: Duration = Duration::from_secs(10 * 60);

/// Welche Sitzung muss für eine neue weichen?
///
/// Zuerst eine **abgelaufene** (länger als [`SITZUNG_FRIST`] ohne Nutzung);
/// gibt es keine, dann – wenn [`SITZUNGEN_MAX`] erreicht ist – die älteste.
/// `None` heißt: es ist Platz.
pub(crate) fn sitzung_weicht(
    zuletzt: &BTreeMap<u64, std::time::Instant>,
    jetzt: std::time::Instant,
) -> Option<u64> {
    let abgelaufen = zuletzt
        .iter()
        .find(|(_, zeit)| jetzt.duration_since(**zeit) >= SITZUNG_FRIST)
        .map(|(schluessel, _)| *schluessel);
    if abgelaufen.is_some() {
        return abgelaufen;
    }
    if zuletzt.len() >= SITZUNGEN_MAX {
        return zuletzt
            .iter()
            .min_by_key(|(_, zeit)| **zeit)
            .map(|(schluessel, _)| *schluessel);
    }
    None
}

/// Zählt die gleichzeitigen Werkzeugaufrufe – insgesamt und je Eintrag.
///
/// Ohne Grenze könnte ein Agent mit vielen parallelen Aufrufen jeden Eintrag
/// und den ganzen Client ausbremsen; die Grenze antwortet stattdessen sofort
/// mit „ausgelastet“.
#[derive(Debug, Default)]
pub(crate) struct Auslastung {
    gesamt: usize,
    je_eintrag: BTreeMap<EintragId, usize>,
}

impl Auslastung {
    /// So viele Aufrufe laufen höchstens gleichzeitig.
    pub(crate) const GESAMT_MAX: usize = 32;
    /// So viele Aufrufe je Eintrag.
    pub(crate) const EINTRAG_MAX: usize = 8;

    /// Einen Aufruf belegen. `false` heißt: ausgelastet.
    pub(crate) fn belegen(&mut self, id: &EintragId) -> bool {
        if self.gesamt >= Self::GESAMT_MAX {
            return false;
        }
        let offen = self.je_eintrag.entry(id.clone()).or_insert(0);
        if *offen >= Self::EINTRAG_MAX {
            return false;
        }
        *offen += 1;
        self.gesamt += 1;
        true
    }

    pub(crate) fn freigeben(&mut self, id: &EintragId) {
        if let Some(offen) = self.je_eintrag.get_mut(id) {
            *offen = offen.saturating_sub(1);
            if *offen == 0 {
                self.je_eintrag.remove(id);
            }
        }
        self.gesamt = self.gesamt.saturating_sub(1);
    }
}

/// Alles, was der Kern im Speicher hält.
struct Stand {
    /// Siehe [`Modell::abgelehnt`].
    abgelehnt: bool,
    /// Siehe [`Modell::anmelde_grund`].
    anmelde_grund: Option<AnmeldeGrund>,
    server_url: Option<String>,
    geraet: Option<Geraet>,
    token: Option<String>,
    /// Siehe [`Modell::benutzer`].
    benutzer: Option<String>,
    /// Siehe [`Modell::anmeldewege`].
    anmeldewege: Option<Anmeldewege>,
    /// Bricht eine laufende Browser-Anmeldung ab (Aufgabe 22).
    anmeldung_abbruch: Option<Abbruch>,
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
    /// Abbruchsignal je laufender Einrichtung (Paket 3: „Abbrechen“ im
    /// Fenster und das Beenden des Clients räumen sie ab).
    einrichtung_abbruch: BTreeMap<EintragId, Abbruch>,
    /// Frist je Werkzeugaufruf in Sekunden, je Eintrag (Paket 3).
    aufruf_fristen: BTreeMap<EintragId, u64>,
    /// Warum der Tresor nicht lesbar ist (Paket 3); `None` heißt: in Ordnung.
    tresor_fehler: Option<String>,
    /// Hinweis zum gespeicherten Stand (Paket 3).
    profil_hinweis: Option<String>,
    /// Einträge, die der User auf diesem Gerät abgeschaltet hat (Aufgabe 19).
    am_geraet_aus: BTreeSet<EintragId>,
    /// Hat der Server dieses Gerät in **diesem** Lauf angenommen (erster
    /// erfolgreicher Abgleich)? Erst dann startet ein Wiedereinschalten etwas
    /// (Aufgabe 19, SPEC §0).
    angenommen: bool,
    /// Dürfen Agenten Inhalte des Protokolls sehen (Aufgabe 17)?
    protokoll_fuer_agenten: bool,
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
                    am_geraet_aus: &self.am_geraet_aus,
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
            anmelde_grund: self.anmelde_grund,
            benutzer: self.benutzer.clone(),
            server_url: self.server_url.clone(),
            anmeldewege: self.anmeldewege.clone(),
            protokoll_fuer_agenten: self.protokoll_fuer_agenten,
            tresor_fehler: self.tresor_fehler.clone(),
            profil_hinweis: self.profil_hinweis.clone(),
        }
    }

    fn profil(&self) -> profil::Profil {
        profil::Profil {
            server_url: self.server_url.clone(),
            geraet: self.geraet.clone(),
            benutzer: self.benutzer.clone(),
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
            protokoll_fuer_agenten: self.protokoll_fuer_agenten,
            am_geraet_aus: self
                .am_geraet_aus
                .iter()
                .map(|id| id.als_str().to_string())
                .collect(),
            aufruf_fristen: self
                .aufruf_fristen
                .iter()
                .map(|(id, sekunden)| (id.als_str().to_string(), *sekunden))
                .collect(),
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
    /// Auf diesem Gerät abgeschaltete Einträge (Aufgabe 19).
    am_geraet_aus: &'a BTreeSet<EintragId>,
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
        am_geraet_aus,
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
            // Abgeschaltet ist ein eigener Zustand: kein Fehler, sondern die
            // Wahl des Users (Aufgabe 19). Seine Werkzeuge gibt es nicht.
            let aus = am_geraet_aus.contains(id);
            // Eine Kennung, die nicht als Präfix taugt, startet nichts – und
            // das Fenster sagt, warum (Paket 3).
            let unzulaessig = quiver_domain::kennung_fehler(id.als_str()).map(str::to_string);
            EintragSicht {
                id: id.clone(),
                titel: eintrag.eintrag.titel.clone(),
                beschreibung: eintrag.eintrag.beschreibung.clone(),
                zustand: if aus {
                    EintragZustand::AmGeraetAus
                } else {
                    zustand
                        .map(|l| l.stand.zustand.clone())
                        .unwrap_or(EintragZustand::Startet)
                },
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
                tools: if aus {
                    Vec::new()
                } else {
                    zustand
                        .map(|l| {
                            l.stand
                                .tools
                                .iter()
                                .map(|tool| tool.name.to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                },
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
                am_geraet_aus: aus,
                unzulaessig,
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

/// Was in eine Sicherung geht (Aufgabe 15): die offenen Werte und Zugänge je
/// Eintrag und – nur mit Passwort – die Geheimnisse aus dem Tresor.
/// Geräte-Token, Agent-Tokens und die Anmeldungen der Tools bleiben draußen.
fn export_daten(
    stand: &Stand,
    mit_geheimnissen: bool,
) -> (
    BTreeMap<String, einstellungen::EintragTeil>,
    Option<einstellungen::Geheimnisse>,
) {
    let mut ids: BTreeSet<String> = stand
        .lokale_werte
        .keys()
        .map(|id| id.als_str().to_string())
        .collect();
    ids.extend(stand.zugaenge.keys().map(|id| id.als_str().to_string()));
    // Die Wahl „auf diesem Gerät abgeschaltet“ gehört in die Sicherung,
    // auch wenn der Eintrag sonst nichts Offenes hat (Aufgabe 19).
    ids.extend(
        stand
            .am_geraet_aus
            .iter()
            .map(|id| id.als_str().to_string()),
    );
    if mit_geheimnissen {
        // Geheimnisse gehören zu einem Eintrag – auch wenn er sonst nichts
        // Offenes hat, soll er in der Datei stehen.
        for (id, werte) in &stand.geheim {
            if !werte.is_empty() {
                ids.insert(id.als_str().to_string());
            }
        }
        for (id, liste) in &stand.zugang_geheim {
            if liste.values().any(|werte| !werte.is_empty()) {
                ids.insert(id.als_str().to_string());
            }
        }
    }
    let mut eintraege: BTreeMap<String, einstellungen::EintragTeil> = BTreeMap::new();
    for id in &ids {
        let eintrag_id = EintragId::neu(id.clone());
        let mut teil = einstellungen::EintragTeil::default();
        if let Some(werte) = stand.lokale_werte.get(&eintrag_id) {
            teil.werte = werte
                .iter()
                .filter(|(_, wert)| !wert.trim().is_empty())
                .map(|(name, wert)| (name.clone(), wert.clone()))
                .collect();
        }
        if let Some(liste) = stand.zugaenge.get(&eintrag_id) {
            teil.zugaenge = liste.clone();
        }
        teil.am_geraet_aus = stand.am_geraet_aus.contains(&eintrag_id);
        eintraege.insert(id.clone(), teil);
    }
    if !mit_geheimnissen {
        return (eintraege, None);
    }
    let mut geheimnisse = einstellungen::Geheimnisse::default();
    for id in &ids {
        let eintrag_id = EintragId::neu(id.clone());
        if let Some(werte) = stand.geheim.get(&eintrag_id) {
            let werte: Werte = werte
                .iter()
                .filter(|(_, wert)| !wert.trim().is_empty())
                .map(|(name, wert)| (name.clone(), wert.clone()))
                .collect();
            if !werte.is_empty() {
                geheimnisse.eintraege.insert(id.clone(), werte);
            }
        }
        if let Some(liste) = stand.zugang_geheim.get(&eintrag_id) {
            let liste: BTreeMap<String, Werte> = liste
                .iter()
                .map(|(name, werte)| {
                    let werte: Werte = werte
                        .iter()
                        .filter(|(_, wert)| !wert.trim().is_empty())
                        .map(|(feld, wert)| (feld.clone(), wert.clone()))
                        .collect();
                    (name.clone(), werte)
                })
                .filter(|(_, werte)| !werte.is_empty())
                .collect();
            if !liste.is_empty() {
                geheimnisse.zugaenge.insert(id.clone(), liste);
            }
        }
    }
    (eintraege, Some(geheimnisse))
}

/// Was beim Lesen der Geheimnisse je Eintrag gebraucht wird: der Eintrag, seine
/// Geheimfelder und je Zugang dessen Geheimfelder.
type Geheimplan = (EintragId, Vec<String>, Vec<(String, Vec<String>)>);

/// Innerei des Kerns; geteilt zwischen Fassade, Abgleich und Endpunkt.
pub(crate) struct Innen {
    pub(crate) konfig: KernKonfig,
    /// Die tatsächliche Adresse des Endpunkts (nach dem Binden).
    pub(crate) adresse: SocketAddr,
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
    /// Nur **ein** Schreibvorgang auf `zustand.json` zur Zeit. Zwei Aufgaben
    /// (Abgleich, Schalten, Meldung) schreiben sonst gleichzeitig dieselbe
    /// Zwischendatei, und einer von beiden fehlt sie beim Umbenennen
    /// (Aufgabe 19: gleichzeitiges Schalten und Abgleichen).
    pub(crate) speicher_wache: Mutex<()>,
    /// Hält gleichzeitige Schaltungen auseinander (Lesen, Schreiben, Wirken).
    pub(crate) schalt_wache: tokio::sync::Mutex<()>,
    /// Aufrufe und Prozesszeilen (Aufgabe 17).
    pub(crate) protokoll: Arc<protokoll::Protokoll>,
    /// Die Sperre des Profils (Paket 3): solange sie lebt, arbeitet dieser
    /// Kern auf dem Profil, und ein zweiter startet nicht. Sie wird nie
    /// gelesen – ihr Dasein ist die Wirkung.
    pub(crate) _profil_sperre: Mutex<Option<std::fs::File>>,
    /// Wie viele Werkzeugaufrufe gerade laufen (Paket 3).
    pub(crate) auslastung: Mutex<Auslastung>,
    /// Wann allen Agenten zuletzt `list_changed` geschickt wurde (Paket 3:
    /// höchstens einmal je Sekunde).
    pub(crate) liste_zuletzt: Mutex<Option<std::time::Instant>>,
    /// Ist eine weitere Meldung schon vorgemerkt?
    pub(crate) liste_geplant: AtomicBool,
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
        if let Err(fehler) = self.speichern_ergebnis() {
            tracing::warn!("Zustand lässt sich nicht speichern: {fehler}");
        }
    }

    /// Wie [`Innen::speichern`], aber mit Ergebnis. Das Schalten eines
    /// Eintrags am Gerät braucht das: scheitert das Speichern, wird **nichts**
    /// geschaltet (Aufgabe 19).
    pub(crate) fn speichern_ergebnis(&self) -> anyhow::Result<()> {
        let _wache = self
            .speicher_wache
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        let profil = self.stand().profil();
        profil.speichern(&self.konfig.profil)?;
        // Die Maske des Protokolls folgt dem Tresor (Aufgabe 17).
        self.protokoll.geheimnisse_setzen(self.geheimnisse());
        Ok(())
    }

    /// Den Stand mit einer **anderen** Menge abgeschalteter Einträge
    /// schreiben, ohne ihn zu übernehmen. So prüft das Schalten erst die
    /// Platte und ändert danach den Speicher (Aufgabe 19).
    pub(crate) fn speichern_mit_aus(&self, aus: &BTreeSet<EintragId>) -> anyhow::Result<()> {
        let _wache = self
            .speicher_wache
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        let mut profil = self.stand().profil();
        profil.am_geraet_aus = aus.iter().map(|id| id.als_str().to_string()).collect();
        profil.speichern(&self.konfig.profil)
    }

    /// Alle Geheimwerte, die gerade im Speicher stehen – die Maske des
    /// Protokolls ersetzt sie überall (Aufgabe 17).
    fn geheimnisse(&self) -> Vec<String> {
        let stand = self.stand();
        let mut werte: Vec<String> = stand.token.iter().cloned().collect();
        for liste in stand.geheim.values() {
            werte.extend(liste.values().cloned());
        }
        for je_eintrag in stand.zugang_geheim.values() {
            for liste in je_eintrag.values() {
                werte.extend(liste.values().cloned());
            }
        }
        werte
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
            // Die Adresse bleibt **immer** stehen (Aufgabe 22): Abmelden
            // verwirft Token und Gerät, nicht die Adresse.
            stand.geraet = None;
            stand.token = None;
            stand.benutzer = None;
            stand.soll = Soll::neu(Vec::new());
            stand.anmeldung.clear();
            stand.einrichtung.clear();
            stand.einrichtung_laufend.clear();
            // Ohne Gerät nimmt der Server nichts mehr an – ein abgeschalteter
            // Eintrag bleibt aber abgeschaltet (Aufgabe 19).
            stand.angenommen = false;
            stand.verbindung = Verbindung::NichtEingebunden;
        }
        self.speichern();
        self.modell_schicken();
        self.liste_geaendert().await;
        geloescht
    }

    /// Der Server verlangt eine neue Anmeldung: alle Tools aus, aber
    /// **Geräte-Token, Soll und alle lokalen Angaben bleiben** (Aufgabe 22).
    /// Erst ein neuer Anmeldevorgang ersetzt das Token.
    pub(crate) async fn neu_anmelden_setzen(self: &Arc<Self>, server: &str) {
        let ids: Vec<EintragId> = self.stand().laeufer.keys().cloned().collect();
        for id in ids {
            stoppe_in(self, &id);
        }
        {
            let mut stand = self.stand();
            stand.angenommen = false;
            stand.abgelehnt = false;
            stand.anmelde_grund = Some(AnmeldeGrund::NeuAnmelden);
            stand.verbindung = Verbindung::NeuAnmelden {
                server: server.to_string(),
                text: "Die Anmeldung gilt nicht mehr. Geräte-Token und Angaben bleiben."
                    .to_string(),
            };
        }
        self.speichern();
        self.modell_schicken();
        self.liste_geaendert().await;
    }

    /// Eine Ablehnung des Geräts einordnen (Aufgabe 22):
    /// - `gesperrt`/`kein-zugriff`: alle Tools aus, Token weg, klare Meldung.
    /// - `neu-anmelden` **und jeder unbekannte Grund**: alle Tools aus, Token
    ///   und Angaben bleiben; der User meldet sich erneut an.
    /// - 401 ohne Grund (Token ungültig/entfernt): wie bisher neu einbinden.
    pub(crate) async fn ablehnung_behandeln(
        self: &Arc<Self>,
        server: &str,
        fehler: &anyhow::Error,
    ) {
        // 401 heißt: das Token gilt nicht mehr (Gerät entfernt). Dann wie
        // bisher neu einbinden — auch ohne `grund`.
        if abgleich::status_von(fehler) == Some(reqwest::StatusCode::UNAUTHORIZED) {
            if let Err(fehler) = self.ausbinden(true).await {
                tracing::warn!("Geräte-Token ließ sich nicht löschen: {fehler:#}");
            }
            return;
        }
        match abgleich::grund_von(fehler) {
            Some(grund @ (AnmeldeGrund::Gesperrt | AnmeldeGrund::KeinZugriff)) => {
                if let Err(fehler) = self.ausbinden(true).await {
                    tracing::warn!("Geräte-Token ließ sich nicht löschen: {fehler:#}");
                }
                {
                    let mut stand = self.stand();
                    stand.anmelde_grund = Some(grund);
                }
                self.speichern();
                self.modell_schicken();
            }
            _ => self.neu_anmelden_setzen(server).await,
        }
    }

    pub(crate) fn verbunden_setzen(&self, server: &str) {
        let geraet = self
            .stand()
            .geraet
            .as_ref()
            .map(|geraet| geraet.name.clone())
            .unwrap_or_default();
        // Ab hier hat der Server dieses Gerät in diesem Lauf angenommen –
        // erst jetzt darf ein Wiedereinschalten etwas starten (Aufgabe 19).
        self.stand().angenommen = true;
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
    ///
    /// Der Tresor wird **einmal** gelesen (eine Sperre, eine Entschlüsselung),
    /// und zwar in `spawn_blocking`: ein Dateizugriff gehört nicht in den
    /// async-Aufgabenbereich, in dem die Oberfläche weiterläuft (Paket 3).
    pub(crate) async fn geheim_laden(self: &Arc<Self>, soll: &Soll) {
        let gespeicherte_zugaenge = self.stand().zugaenge.clone();
        // Erst sammeln, was gebraucht wird – in derselben Reihenfolge, in der
        // es nachher zugeordnet wird.
        let mut schluessel: Vec<String> = Vec::new();
        // Je Eintrag: seine Geheimfelder und je Zugang die Geheimfelder.
        let mut plan: Vec<Geheimplan> = Vec::new();
        for eintrag in &soll.eintraege {
            let geheim_felder: Vec<String> = eintrag
                .eintrag
                .felder
                .iter()
                .filter(|feld| feld.quelle == Quelle::Geheim)
                .map(|feld| feld.name.clone())
                .collect();
            for feld in &geheim_felder {
                schluessel.push(geheim_schluessel(&eintrag.eintrag.id, feld));
            }
            let mut zugangs_felder: Vec<String> = Vec::new();
            if let Some(mehrfach) = &eintrag.eintrag.mehrfach {
                zugangs_felder = mehrfach
                    .felder
                    .iter()
                    .filter(|feld| feld.quelle == Quelle::Geheim)
                    .map(|feld| feld.name.clone())
                    .collect();
            }
            let mut zugangsplan = Vec::new();
            for name in gespeicherte_zugaenge
                .get(&eintrag.eintrag.id)
                .into_iter()
                .flat_map(|liste| liste.keys())
            {
                for feld in &zugangs_felder {
                    schluessel.push(zugang_geheim_schluessel(&eintrag.eintrag.id, name, feld));
                }
                zugangsplan.push((name.clone(), zugangs_felder.clone()));
            }
            plan.push((eintrag.eintrag.id.clone(), geheim_felder, zugangsplan));
        }

        let tresor = self.konfig.tresor.clone();
        let gelesen = tokio::task::spawn_blocking(move || tresor.werte_laden(&schluessel))
            .await
            .unwrap_or_else(|fehler| Err(anyhow::anyhow!("Tresor: {fehler}")));
        let werte = match gelesen {
            Ok(werte) => {
                self.tresor_fehler_setzen(None);
                werte
            }
            Err(fehler) => {
                tracing::warn!("Tresor nicht lesbar: {fehler:#}");
                self.tresor_fehler_setzen(Some(format!("{fehler:#}")));
                return;
            }
        };

        let mut alle = BTreeMap::new();
        let mut zugangs_geheim: BTreeMap<EintragId, BTreeMap<String, Werte>> = BTreeMap::new();
        for (id, geheim_felder, zugangsplan) in plan {
            let mut eigene = Werte::new();
            for feld in &geheim_felder {
                let schluessel = geheim_schluessel(&id, feld);
                if let Some(wert) = werte.get(&schluessel) {
                    eigene.insert(feld.clone(), wert.clone());
                }
            }
            let mut je_eintrag = BTreeMap::new();
            for (name, felder) in zugangsplan {
                let mut eigene_felder = Werte::new();
                for feld in &felder {
                    let schluessel = zugang_geheim_schluessel(&id, &name, feld);
                    if let Some(wert) = werte.get(&schluessel) {
                        eigene_felder.insert(feld.clone(), wert.clone());
                    }
                }
                je_eintrag.insert(name, eigene_felder);
            }
            zugangs_geheim.insert(id.clone(), je_eintrag);
            alle.insert(id, eigene);
        }
        let mut stand = self.stand();
        stand.geheim = alle;
        stand.zugang_geheim = zugangs_geheim;
    }

    /// Den Zustand des Tresors im Modell führen (Paket 3): das Fenster zeigt
    /// „Tresor nicht lesbar“, und ein Schreibversuch unterbleibt.
    pub(crate) fn tresor_fehler_setzen(&self, text: Option<String>) {
        let geaendert = {
            let mut stand = self.stand();
            if stand.tresor_fehler == text {
                false
            } else {
                stand.tresor_fehler = text;
                true
            }
        };
        if geaendert {
            self.modell_schicken();
        }
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
                if !quiver_domain::kennung_gueltig(id.als_str()) {
                    continue;
                }
                let mut kopie = tool.clone();
                kopie.name = quiver_domain::tool_name(id, &tool.name).into();
                kopie.input_schema = schema_mit_typ(&tool.input_schema);
                alle.push(kopie);
            }
        }
        alle.sort_by(|a, b| a.name.cmp(&b.name));
        alle
    }

    /// Die Zuordnung Werkzeugname → (Eintrag, Werkzeug) für den Endpunkt.
    /// Eine Tabelle, keine Zerlegung (Paket 3): so führt ein `__` in einem
    /// Werkzeugnamen nie zum falschen Eintrag, und doppelte Namen werden
    /// einmal vergeben.
    pub(crate) fn werkzeugtabelle(&self) -> BTreeMap<String, (EintragId, String)> {
        let stand = self.stand();
        let eintraege: Vec<(EintragId, Vec<String>)> = stand
            .laeufer
            .iter()
            .map(|(id, laeufer)| {
                (
                    id.clone(),
                    laeufer
                        .stand
                        .tools
                        .iter()
                        .map(|tool| tool.name.to_string())
                        .collect(),
                )
            })
            .collect();
        drop(stand);
        laufzeit::werkzeugtabelle(&eintraege)
    }

    /// Der Agent (Kennung, Name) zu einem Token – Vergleich über die Hashes in
    /// konstanter Zeit.
    pub(crate) fn agent_fuer_token(&self, token: &str) -> Option<(String, String)> {
        let stand = self.stand();
        let id = agent_fuer(&stand.agenten, token)?;
        stand
            .agenten
            .iter()
            .find(|agent| agent.id == id)
            .map(|agent| (agent.id.clone(), agent.name.clone()))
    }

    /// Die Frist für einen Werkzeugaufruf dieses Eintrags (Paket 3): die
    /// eingestellte, sonst [`laufzeit::AUFRUF_FRIST`] – nie mehr als
    /// [`laufzeit::AUFRUF_FRIST_MAX`].
    pub(crate) fn aufruf_frist(&self, id: &EintragId) -> Duration {
        let sekunden = self.stand().aufruf_fristen.get(id).copied();
        match sekunden {
            Some(sekunden) => Duration::from_secs(sekunden.clamp(1, 900)),
            None => laufzeit::AUFRUF_FRIST,
        }
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
    ///
    /// Höchstens **einmal je Sekunde** (Paket 3): ein Eintrag, der beim
    /// Starten und Neustarten mehrmals die Liste ändert, soll die Agenten
    /// nicht mit `list_changed` überschütten. Eine Meldung ist dann
    /// vorgemerkt und kommt am Ende der Sekunde nach.
    pub(crate) async fn liste_geaendert(self: &Arc<Self>) {
        const ABSTAND: Duration = Duration::from_secs(1);
        let warten = {
            let mut zuletzt = self
                .liste_zuletzt
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            let jetzt = std::time::Instant::now();
            match *zuletzt {
                Some(letzte) if jetzt.duration_since(letzte) < ABSTAND => {
                    Some(ABSTAND - jetzt.duration_since(letzte))
                }
                _ => {
                    *zuletzt = Some(jetzt);
                    None
                }
            }
        };
        if let Some(rest) = warten {
            // Eine Meldung ist schon unterwegs: sie deckt diese Änderung mit ab.
            if self
                .liste_geplant
                .swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                return;
            }
            let innen = self.clone();
            tokio::spawn(async move {
                tokio::time::sleep(rest).await;
                innen
                    .liste_geplant
                    .store(false, std::sync::atomic::Ordering::SeqCst);
                innen.liste_senden().await;
            });
            return;
        }
        self.liste_senden().await;
    }

    /// Die Meldung selbst: allen offenen Sitzungen `list_changed` schicken.
    async fn liste_senden(&self) {
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
                        let id = &eintrag.eintrag.id;
                        // Die Wahl „am Gerät abgeschaltet“ geht als eigener
                        // Zustand an den Server (Aufgabe 19) – sie ist kein
                        // Fehler und zählt dort nicht als Problem.
                        let zustand = if stand.am_geraet_aus.contains(id) {
                            EintragZustand::AmGeraetAus
                        } else {
                            stand
                                .laeufer
                                .get(id)
                                .map(|l| l.stand.zustand.clone())
                                .unwrap_or(EintragZustand::Startet)
                        };
                        (id.clone(), zustand)
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
        let ids: Vec<EintragId> = {
            let stand = innen.stand();
            stand
                .soll
                .eintraege
                .iter()
                // Ein abgeschalteter Eintrag wird nicht geprüft: kein Befehl
                // darf ihn anfassen (Aufgabe 19).
                .filter(|eintrag| {
                    eintrag.eintrag.anmeldung.is_some()
                        && !stand.am_geraet_aus.contains(&eintrag.eintrag.id)
                })
                .map(|eintrag| eintrag.eintrag.id.clone())
                .collect()
        };
        for id in ids {
            if innen.ende.ist_abgebrochen() {
                return;
            }
            status_pruefen(&innen, &id).await;
        }
    }
}

/// Einmal täglich alte Protokolldateien löschen und die Größengrenze
/// durchsetzen (Aufgabe 17).
async fn protokoll_aufraeumen(innen: Arc<Innen>) {
    let mut tag = protokoll::tag_heute();
    loop {
        tokio::select! {
            _ = innen.ende.warten() => return,
            _ = tokio::time::sleep(Duration::from_secs(60 * 60)) => {}
        }
        let heute = protokoll::tag_heute();
        if heute != tag {
            tag = heute;
            innen.protokoll.aufraeumen();
        }
    }
}

/// Alte Agenten-Sitzungen abräumen (Paket 3): eine Sitzung, die 10 Minuten
/// nicht benutzt wurde, gilt als weg. Das Fenster zeigt „verbunden“ daraus.
async fn sitzungen_lauf(innen: Arc<Innen>) {
    loop {
        tokio::select! {
            _ = innen.ende.warten() => return,
            _ = tokio::time::sleep(Duration::from_secs(60)) => {}
        }
        let jetzt = std::time::Instant::now();
        let entfernt = {
            let mut sitzungen = innen
                .sitzungen
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            let vorher = sitzungen.len();
            sitzungen.retain(|_, sitzung| jetzt.duration_since(sitzung.zuletzt) < SITZUNG_FRIST);
            vorher != sitzungen.len()
        };
        if entfernt {
            innen.modell_schicken();
        }
    }
}

/// Die Sperre des Profils (`<profil>/kern.sperre`). Sie lebt so lange wie der
/// Kern; ein zweiter Kern auf demselben Ordner bekommt sie nicht und startet
/// nicht. Der Port taugt dafür nicht: zwei Profile dürfen nebeneinander laufen.
fn profil_sperren(profil: &Path) -> anyhow::Result<std::fs::File> {
    std::fs::create_dir_all(profil)?;
    let datei = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(profil.join("kern.sperre"))?;
    match datei.try_lock() {
        Ok(()) => Ok(datei),
        Err(_) => anyhow::bail!(
            "Das Profil {} wird schon benutzt – bitte nur ein quiver je Profil starten",
            profil.display()
        ),
    }
}

/// Die **eine** Stelle, die entscheidet, ob ein Eintrag laufen darf: der User
/// hat ihn auf diesem Gerät nicht abgeschaltet (Aufgabe 19) **und** seine
/// Kennung taugt als Präfix der Werkzeugnamen (Paket 3). Alle Start-Wege
/// gehen durch [`starte_in`].
fn darf_laufen(stand: &Stand, id: &EintragId) -> bool {
    !stand.am_geraet_aus.contains(id) && quiver_domain::kennung_gueltig(id.als_str())
}

/// Der Eintrag startet: Zustand merken und die Aufsicht starten.
///
/// Je Eintrag gibt es genau **einen** Läufer und damit genau eine wirksame
/// Start-Aufgabe: jeder Start bekommt eine neue Generation, der alte Läufer
/// wird verworfen und seine Aufgabe über ihr Abbruchsignal beendet. Die alte
/// Aufgabe wird nicht abgebrochen, sondern lässt ihren Aufbau los – sonst
/// bliebe ein Kindprozess zurück, dessen Dienst in einer eigenen Aufgabe lebt.
///
/// Ein abgeschalteter Eintrag startet hier **nie** – egal, wer ihn starten will
/// (Abgleich, Wiedereinschalten, Import, Werte, Anmeldung, Einrichtung).
async fn starte_in(innen: &Arc<Innen>, eintrag: &SollEintrag) {
    let id = eintrag.eintrag.id.clone();
    if !darf_laufen(&innen.stand(), &id) {
        tracing::info!(
            eintrag = %id,
            "wird nicht gestartet (abgeschaltet oder Kennung unbrauchbar)"
        );
        return;
    }
    let generation = innen
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    let abbruch = Abbruch::neu();
    let vorheriger = {
        let mut stand = innen.stand();
        // Noch einmal unter derselben Sperre wie das Eintragen: dazwischen kann
        // der User abgeschaltet haben.
        if !darf_laufen(&stand, &id) {
            return;
        }
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
    // Wie oft eine Meldung des Tools zu groß war (Paket 3).
    let mut zu_gross_anzahl: u64 = 0;
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
                laufzeit::verbinden(id.als_str(), &laufzeit, &innen.protokoll),
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

        // Jetzt läuft es. Ende entweder durch uns (Abbruch), von selbst -
        // oder weil eine Meldung des Tools die Größengrenze überschritt
        // (Paket 3): dann ist die Leitung nicht mehr in einem bekannten
        // Zustand, und der Eintrag wird neu gestartet.
        let wache = verbindung.groessen_wache();
        let mut zu_gross = false;
        let selbst_zu = tokio::select! {
            _ = abbruch.warten() => false,
            _ = innen.ende.warten() => false,
            _ = wache.warten() => {
                zu_gross = true;
                true
            }
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
        if zu_gross {
            zu_gross_anzahl += 1;
            let text = format!(
                "Eine Meldung des Tools war größer als {} Byte – der Eintrag wird neu gestartet ({zu_gross_anzahl}. Mal)",
                laufzeit::NACHRICHT_BYTES
            );
            tracing::warn!(eintrag = %id, "{text}");
            innen.protokoll.zeile_schreiben(&id, &text);
            innen.modell_schicken();
            innen.liste_geaendert().await;
            // Eine Pause: ein Tool, das immer wieder zu große Meldungen
            // schickt, soll nicht in einer Schleife Prozesse starten.
            if !warten_auf_neustart(&abbruch, &mut backoff).await {
                return;
            }
            continue;
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
    innen.geheim_laden(&soll).await;
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

/// Eine Server-Adresse prüfen und normalisieren: `https` überall, `http`
/// **nur** auf Loopback (ADR 0002). Kein Benutzer, keine Abfrage, kein Pfad.
pub fn server_adresse(wert: &str) -> anyhow::Result<String> {
    let wert = wert.trim().trim_end_matches('/');
    if wert.is_empty() {
        anyhow::bail!("Server-Adresse fehlt");
    }
    let adresse = reqwest::Url::parse(wert)
        .map_err(|fehler| anyhow::anyhow!("„{wert}“ ist keine gültige Adresse: {fehler}"))?;
    match adresse.scheme() {
        "https" => {}
        "http" => {
            let host = adresse.host_str().unwrap_or_default();
            let host = host
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
                .unwrap_or(host);
            if !matches!(host, "127.0.0.1" | "::1" | "localhost") {
                anyhow::bail!(
                    "Unverschlüsseltes http ist nur auf diesem Rechner erlaubt (127.0.0.1, ::1, localhost)"
                );
            }
        }
        anderes => anyhow::bail!("Die Adresse muss mit https beginnen (nicht {anderes})"),
    }
    if !adresse.username().is_empty() || adresse.password().is_some() {
        anyhow::bail!("Die Adresse darf keinen Benutzer und kein Passwort enthalten");
    }
    if adresse.query().is_some() || adresse.fragment().is_some() {
        anyhow::bail!("Die Adresse darf keine Abfrage und kein Fragment enthalten");
    }
    if !matches!(adresse.path(), "" | "/") {
        anyhow::bail!("Die Adresse darf keinen Pfad enthalten");
    }
    Ok(adresse.as_str().trim_end_matches('/').to_string())
}

/// Ein verständlicher Satz zu einem maschinenlesbaren Grund; ohne Grund steht
/// der technische Text da.
fn grund_text(grund: Option<AnmeldeGrund>, technisch: &str) -> String {
    match grund {
        Some(AnmeldeGrund::Gesperrt) => {
            "Dieses Konto oder Gerät ist gesperrt. Bitte an die Verwaltung wenden."
                .to_string()
        }
        Some(AnmeldeGrund::KeinZugriff) => {
            "Diesem Konto ist keine Gruppe zugeordnet – es gibt noch keine Tools. Bitte an die Verwaltung wenden."
                .to_string()
        }
        Some(AnmeldeGrund::NeuAnmelden) => {
            "Die Anmeldung gilt nicht mehr. Bitte erneut anmelden.".to_string()
        }
        None => technisch.to_string(),
    }
}

/// Die Fassade für das Fenster. Billig zu klonen (ein Zeiger).
#[derive(Clone)]
pub struct Kern {
    innen: Arc<Innen>,
}

impl Kern {
    /// Hintergrundaufgaben starten, gespeicherten Stand laden, Endpunkt binden.
    ///
    /// Ein kaputter gespeicherter Stand oder ein unlesbarer Tresor verhindert
    /// den Start **nicht** (Paket 3): der Client läuft weiter und sagt es im
    /// Fenster. Ein zweiter Kern auf demselben Profil startet nicht.
    pub async fn starten(konfig: KernKonfig) -> anyhow::Result<Kern> {
        if !konfig.mcp_adresse.ip().is_loopback() {
            anyhow::bail!(
                "Der MCP-Endpunkt darf nur lokal lauschen, nicht auf {}",
                konfig.mcp_adresse
            );
        }
        // Eine Sperre je Profil: zwei Kerne auf demselben Ordner würden sich
        // gegenseitig die Dateien überschreiben.
        let profil_sperre = profil_sperren(&konfig.profil)?;
        let (mut gespeichert, mut profil_hinweis) =
            profil::Profil::laden_mit_hinweis(&konfig.profil);
        // Auch die gespeicherte Adresse wird geprüft (Paket 3): was aus einer
        // alten oder fremden Datei kommt, ist keine vertrauenswürdige Adresse.
        if let Some(url) = gespeichert.server_url.clone() {
            match server_adresse(&url) {
                Ok(sauber) => gespeichert.server_url = Some(sauber),
                Err(fehler) => {
                    tracing::warn!("Server-Adresse im Profil taugt nicht: {fehler:#}");
                    gespeichert.server_url = None;
                    gespeichert.geraet = None;
                    profil_hinweis = Some(format!(
                        "Die gespeicherte Server-Adresse taugt nicht: {fehler:#}"
                    ));
                }
            }
        }
        // Ein kaputter Tresor ist ein Zustand, kein Startfehler: das Fenster
        // zeigt „Tresor nicht lesbar“ und bietet „Neu beginnen“ an.
        let (token, tresor_fehler) = match konfig.tresor.lesen("geraet-token") {
            Ok(token) => (token, None),
            Err(fehler) => (None, Some(format!("{fehler:#}"))),
        };

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
            anmelde_grund: None,
            server_url: gespeichert.server_url.clone(),
            geraet,
            token,
            benutzer: gespeichert.benutzer.clone(),
            anmeldewege: None,
            anmeldung_abbruch: None,
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
            einrichtung_abbruch: BTreeMap::new(),
            aufruf_fristen: gespeichert
                .aufruf_fristen
                .iter()
                .map(|(id, sekunden)| (EintragId::neu(id.clone()), *sekunden))
                .collect(),
            tresor_fehler,
            profil_hinweis,
            am_geraet_aus: gespeichert
                .am_geraet_aus
                .iter()
                .map(|id| EintragId::neu(id.clone()))
                .collect(),
            // Der gespeicherte Stand läuft nicht an; er ist nicht angenommen.
            angenommen: false,
            protokoll_fuer_agenten: gespeichert.protokoll_fuer_agenten,
            verbindung,
            endpunkt: endpunkt.clone(),
        };
        let (sender, _) = watch::channel(stand.modell(&BTreeSet::new()));
        let innen = Arc::new(Innen {
            konfig: konfig.clone(),
            adresse,
            http: netz::client(),
            stand: Mutex::new(stand),
            modell: sender,
            sitzungen: Mutex::new(BTreeMap::new()),
            zaehler: AtomicU64::new(0),
            ende: Abbruch::neu(),
            erneut: Notify::new(),
            aufgaben: Mutex::new(Vec::new()),
            speicher_wache: Mutex::new(()),
            schalt_wache: tokio::sync::Mutex::new(()),
            generation: AtomicU64::new(0),
            protokoll: Arc::new(protokoll::Protokoll::neu(&konfig.profil)),
            _profil_sperre: Mutex::new(Some(profil_sperre)),
            auslastung: Mutex::new(Auslastung::default()),
            liste_zuletzt: Mutex::new(None),
            liste_geplant: AtomicBool::new(false),
        });

        // Geheimnisse wieder aufnehmen. Der gespeicherte Soll-Zustand läuft
        // **nicht** an: kein Tool startet, bevor der Server das Gerät in diesem
        // Lauf angenommen hat (erster erfolgreicher Abgleich).
        let soll = innen.stand().soll.clone();
        innen.geheim_laden(&soll).await;
        innen.protokoll.geheimnisse_setzen(innen.geheimnisse());
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
        let aufraeumen = tokio::spawn(protokoll_aufraeumen(innen.clone()));
        let sitzungen = tokio::spawn(sitzungen_lauf(innen.clone()));
        {
            let mut aufgaben = innen
                .aufgaben
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            aufgaben.push(server);
            aufgaben.push(abgleich);
            aufgaben.push(status);
            aufgaben.push(aufraeumen);
            aufgaben.push(sitzungen);
        }

        tracing::info!("MCP-Endpunkt für Agenten: {endpunkt}");
        Ok(Kern { innen })
    }

    /// Das aktuelle Modell; ändert sich bei jedem Ereignis.
    pub fn modell(&self) -> watch::Receiver<Modell> {
        self.innen.modell.subscribe()
    }

    /// Die Aufrufe im Protokoll, die zum Filter passen – die neuesten zuerst
    /// (Aufgabe 17). Das Modell bekommt bewusst keine Liste.
    pub fn protokoll(&self, filter: &protokoll::Filter) -> Vec<protokoll::Aufruf> {
        self.innen.protokoll.abfragen(filter)
    }

    /// Die Prozesszeilen eines Eintrags, die zum Filter passen.
    pub fn prozess_log(
        &self,
        eintrag: &EintragId,
        filter: &protokoll::LogFilter,
    ) -> Vec<protokoll::Zeile> {
        self.innen.protokoll.zeilen(eintrag, filter)
    }

    /// Der Zähler des Protokolls: steigt bei jedem neuen Eintrag. Das Fenster
    /// lädt dann neu und zeichnet.
    pub fn protokoll_zaehler(&self) -> watch::Receiver<u64> {
        self.innen.protokoll.zaehler()
    }

    /// Der aktuelle Stand des Zählers.
    pub fn protokoll_stand(&self) -> u64 {
        self.innen.protokoll.stand()
    }

    /// Speicher und Dateien des Protokolls leeren.
    pub fn protokoll_leeren(&self) -> anyhow::Result<()> {
        self.innen.protokoll.leeren()
    }

    /// Dürfen Agenten Inhalte im Protokoll sehen (Aufgabe 17)?
    pub fn protokoll_fuer_agenten(&self) -> bool {
        self.innen.stand().protokoll_fuer_agenten
    }

    /// Den Schalter setzen; er landet in `zustand.json`.
    pub async fn protokoll_fuer_agenten_setzen(&self, an: bool) {
        {
            let mut stand = self.innen.stand();
            if stand.protokoll_fuer_agenten == an {
                return;
            }
            stand.protokoll_fuer_agenten = an;
        }
        self.innen.speichern();
        self.innen.modell_schicken();
    }

    /// Fragen, welche Anmeldewege dieser Server anbietet (Aufgabe 22).
    ///
    /// Das ist der Schritt hinter „Weiter“: die Adresse wird geprüft (https,
    /// `http` nur Loopback), gespeichert und die Antwort in `anmeldewege`
    /// abgelegt. Erst danach zeigt die Seite **entweder** „Mit Microsoft
    /// anmelden“ **oder** das Code-Feld, nie beides.
    pub async fn anmeldewege(&self, server: &str) -> anyhow::Result<()> {
        let server = server_adresse(server)?;
        let antwort = self
            .innen
            .http
            .get(format!("{server}{PRAEFIX}/anmeldearten"))
            .send()
            .await
            .map_err(|fehler| anyhow::anyhow!("Server nicht erreichbar: {fehler}"))?;
        let antwort = abgleich::pruefen(antwort).await?;
        let arten: Anmeldearten = antwort
            .json()
            .await
            .map_err(|fehler| anyhow::anyhow!("Antwort der Anmeldung: {fehler}"))?;
        {
            let mut stand = self.innen.stand();
            stand.server_url = Some(server.clone());
            stand.abgelehnt = false;
            stand.anmelde_grund = None;
            // Ohne Entra gibt es die Einmal-Codes; mit Entra ausdrücklich nicht.
            stand.anmeldewege = Some(Anmeldewege {
                server: server.clone(),
                entra: arten.entra,
                code: !arten.entra,
            });
            if !matches!(stand.verbindung, Verbindung::Verbunden { .. }) {
                stand.verbindung = Verbindung::NichtEingebunden;
            }
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        Ok(())
    }

    /// Mit Entra über den Server anmelden (Aufgabe 22). Der Client spricht
    /// **nie** mit Entra: er öffnet die Seite des Servers, wartet auf genau
    /// eine Antwort auf `127.0.0.1` und tauscht den Code mit seinem
    /// PKCE-Verifier. Ein altes Token wird mitgeschickt, damit das Gerät
    /// dasselbe bleibt.
    pub async fn anmelden_server(&self, server: &str, name: &str) -> anyhow::Result<()> {
        let server = server_adresse(server)?;
        if !self
            .innen
            .stand()
            .anmeldewege
            .as_ref()
            .is_some_and(|wege| wege.server == server && wege.entra)
        {
            anyhow::bail!("Dieser Server meldet keinen Entra-Weg; bitte „Weiter“ erneut wählen");
        }
        let altes_token = self.innen.stand().token.clone();
        // Eine schon laufende Anmeldung (etwa ein alter Lauscher) abbrechen.
        if let Some(alt) = self.innen.stand().anmeldung_abbruch.take() {
            alt.abbrechen();
        }
        let abbruch = Abbruch::neu();
        self.innen.stand().anmeldung_abbruch = Some(abbruch.clone());
        self.innen.verbindung_setzen(Verbindung::Verbindet {
            server: server.clone(),
        });

        let innen = self.innen.clone();
        let server_fuer_adresse = server.clone();
        let ergebnis = server_anmeldung::anmelden(
            &self.innen.http,
            &*self.innen.konfig.browser,
            &server,
            name,
            altes_token.as_deref(),
            &abbruch,
            move |adresse| {
                innen.verbindung_setzen(Verbindung::WartetAufBrowser {
                    server: server_fuer_adresse.clone(),
                    adresse,
                });
            },
        )
        .await;
        self.innen.stand().anmeldung_abbruch = None;

        match ergebnis {
            Ok(antwort) => self.anmelden_uebernehmen(&server, antwort),
            Err(fehler) => {
                self.anmeldefehler_setzen(&server, &fehler).await;
                Err(fehler)
            }
        }
    }

    /// Eine laufende Browser-Anmeldung abbrechen (Knopf „Abbrechen“).
    pub fn anmeldung_abbrechen(&self) {
        if let Some(abbruch) = self.innen.stand().anmeldung_abbruch.take() {
            abbruch.abbrechen();
        }
    }

    /// Ein gescheitertes Anmelden in den Zustand schreiben: verständlicher
    /// Text, richtiger Umgang mit Token und Angaben.
    async fn anmeldefehler_setzen(self: &Kern, server: &str, fehler: &anyhow::Error) {
        let text = grund_text(abgleich::grund_von(fehler), &format!("{fehler:#}"));
        match abgleich::grund_von(fehler) {
            Some(AnmeldeGrund::Gesperrt | AnmeldeGrund::KeinZugriff) => {
                if let Err(fehler) = self.innen.ausbinden(true).await {
                    tracing::warn!("Geräte-Token ließ sich nicht löschen: {fehler:#}");
                }
                {
                    let mut stand = self.innen.stand();
                    stand.anmelde_grund = abgleich::grund_von(fehler);
                    stand.verbindung = Verbindung::AnmeldeFehler {
                        server: server.to_string(),
                        text: text.clone(),
                    };
                }
            }
            Some(AnmeldeGrund::NeuAnmelden) | None if self.innen.stand().token.is_some() => {
                self.innen.neu_anmelden_setzen(server).await;
                let mut stand = self.innen.stand();
                if let Verbindung::NeuAnmelden { text: alt, .. } = &mut stand.verbindung {
                    *alt = text.clone();
                }
            }
            _ => {
                let mut stand = self.innen.stand();
                stand.abgelehnt = false;
                stand.anmelde_grund = abgleich::grund_von(fehler);
                stand.verbindung = Verbindung::AnmeldeFehler {
                    server: server.to_string(),
                    text,
                };
            }
        }
        self.innen.speichern();
        self.innen.modell_schicken();
    }

    /// Das Ergebnis einer Anmeldung übernehmen: Token in den Tresor, Gerät
    /// und Benutzer ins Modell, den Abgleich wecken.
    ///
    /// Die Adresse wird **noch einmal** geprüft (Paket 3): sie ist die letzte
    /// Wache vor dem Schreiben des Geräte-Tokens – keine Anmeldung kann ein
    /// Token an eine Adresse binden, die nicht https (oder Loopback) ist.
    fn anmelden_uebernehmen(
        &self,
        server: &str,
        antwort: quiver_protocol::AnmeldenAntwort,
    ) -> anyhow::Result<()> {
        let server = server_adresse(server)?;
        self.innen
            .konfig
            .tresor
            .schreiben("geraet-token", &antwort.token)?;
        {
            let mut stand = self.innen.stand();
            stand.abgelehnt = false;
            stand.anmelde_grund = None;
            stand.server_url = Some(server.clone());
            stand.geraet = Some(antwort.geraet);
            stand.token = Some(antwort.token);
            stand.benutzer = antwort.benutzer;
            stand.verbindung = Verbindung::Verbindet {
                server: server.clone(),
            };
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        self.innen.erneut.notify_one();
        Ok(())
    }

    /// Gerät beim Server einbinden (Einmal-Code aus der Admin-Oberfläche).
    ///
    /// Die Adresse wird wie überall geprüft: `https` überall, `http` nur auf
    /// Loopback (Paket 3 – der Weg über den Einmal-Code war die letzte Lücke).
    pub async fn einbinden(&self, server: &str, code: &str, name: &str) -> anyhow::Result<()> {
        let server = server_adresse(server)?;
        let antwort = abgleich::anmelden(&self.innen.http, &server, code, name).await?;
        self.anmelden_uebernehmen(&server, antwort)
    }

    /// Gerät abmelden: das Gerät wird **auch am Server** gelöscht, dann sind
    /// Token und Gerät weg; die Server-Adresse und alle Angaben bleiben.
    /// Ist der Server nicht erreichbar, sagt der Client das — lokal ist das
    /// Gerät trotzdem abgemeldet.
    pub async fn abmelden(&self) -> anyhow::Result<()> {
        tracing::info!("Abmelden im Fenster");
        let (server, token) = self.innen.zugang();
        let mut server_fehler = None;
        if let (Some(server), Some(token)) = (server, token) {
            if let Err(fehler) = abgleich::geraet_abmelden(&self.innen.http, &server, &token).await
            {
                tracing::warn!("Gerät am Server nicht gelöscht: {fehler:#}");
                server_fehler = Some(fehler);
            }
        }
        self.innen.ausbinden(false).await?;
        {
            let mut stand = self.innen.stand();
            stand.anmelde_grund = None;
            stand.anmeldewege = None;
        }
        self.innen.speichern();
        self.innen.modell_schicken();
        match server_fehler {
            Some(fehler) => anyhow::bail!(
                "Lokal abgemeldet, aber das Gerät ließ sich am Server nicht löschen: {fehler:#}"
            ),
            None => Ok(()),
        }
    }

    /// Die Anmeldung eines Eintrags im Fenster auslösen (Aufgabe 10).
    ///
    /// Der Befehl öffnet selbst den Browser und wartet auf den User. Danach
    /// wird der Status geprüft und der Eintrag **neu gestartet**, damit der
    /// laufende MCP-Prozess das neue Token sieht.
    pub async fn anmelden(&self, eintrag: &EintragId) -> anyhow::Result<()> {
        if self.aus_am_geraet(eintrag) {
            return Err(self.abgeschaltet(eintrag));
        }
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
        if self.aus_am_geraet(eintrag) {
            return Err(self.abgeschaltet(eintrag));
        }
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
        if self.aus_am_geraet(eintrag) {
            return Err(self.abgeschaltet(eintrag));
        }
        let soll = self.innen.soll_eintrag(eintrag).ok_or_else(|| {
            anyhow::anyhow!("Eintrag {eintrag} ist diesem Gerät nicht zugewiesen")
        })?;
        if !self.innen.einrichtung_beginnen(eintrag) {
            return Ok(());
        }
        // Das Abbruchsignal gehört zum Eintrag: „Abbrechen“ im Fenster und
        // [`Kern::beenden`] beenden den Kindprozess (Paket 3).
        let abbruch = Abbruch::neu();
        self.innen
            .stand()
            .einrichtung_abbruch
            .insert(eintrag.clone(), abbruch.clone());
        let ergebnis = self
            .einrichtung_ausfuehren(eintrag, &soll, abbruch.clone())
            .await;
        self.innen.einrichtung_freigeben(eintrag);
        self.innen.stand().einrichtung_abbruch.remove(eintrag);
        // Ein Abbruch ist kein Fehler: der Zustand ist wieder „Bereit“.
        if abbruch.ist_abgebrochen() {
            self.innen
                .einrichtung_setzen(eintrag, EinrichtZustand::Bereit);
            return Ok(());
        }
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
        abbruch: Abbruch,
    ) -> anyhow::Result<anmeldung::Ausgang> {
        let (lokal, geheim) = self.innen.werte(eintrag);
        let befehl = einrichtung::befehl_bauen(soll, &lokal, &geheim)?;
        einrichtung::laufen(eintrag, &befehl, Some(abbruch)).await
    }

    /// Eine laufende Einrichtung abbrechen (Knopf „Abbrechen“). Der Prozess
    /// endet sofort, der Zustand geht zurück auf „Bereit“.
    pub fn einrichtung_abbrechen(&self, eintrag: &EintragId) {
        if let Some(abbruch) = self.innen.stand().einrichtung_abbruch.get(eintrag).cloned() {
            abbruch.abbrechen();
        }
    }

    /// Einen kaputten Tresor beiseitelegen und mit einem leeren beginnen
    /// (Paket 3, nach der Rückfrage im Fenster). Danach ist der Tresor wieder
    /// schreibbar; die alten Dateien liegen als `.kaputt-<zeit>` daneben.
    pub fn tresor_neu_beginnen(&self) -> anyhow::Result<()> {
        self.innen.konfig.tresor.neu_beginnen()?;
        self.innen.tresor_fehler_setzen(None);
        self.innen
            .protokoll
            .geheimnisse_setzen(self.innen.geheimnisse());
        Ok(())
    }

    /// Wie lange ein Werkzeugaufruf dieses Eintrags höchstens dauern darf
    /// (Paket 3). Erlaubt sind 1–900 Sekunden; ohne Angabe gelten 120 s.
    pub fn aufruf_frist_setzen(&self, eintrag: &EintragId, sekunden: u64) -> anyhow::Result<()> {
        if !(1..=900).contains(&sekunden) {
            anyhow::bail!("Die Frist muss zwischen 1 und 900 Sekunden liegen");
        }
        self.innen
            .stand()
            .aufruf_fristen
            .insert(eintrag.clone(), sekunden);
        self.innen.speichern();
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

    /// Ein Eintrag auf **diesem** Gerät abschalten oder wieder einschalten
    /// (Aufgabe 19).
    ///
    /// **Aus:** Der laufende Prozess wird beendet, seine Werkzeuge verschwinden
    /// sofort aus `tools/list`, alle Agenten bekommen `list_changed`, und der
    /// Zustand `abgeschaltet` geht an den Server. **An:** startet sofort mit den
    /// aktuellen Werten – aber nur, wenn der Server das Gerät in diesem Lauf
    /// angenommen hat.
    ///
    /// Gespeichert wird **vor** dem Schalten: scheitert das Schreiben, ist
    /// nichts geschaltet und der Aufruf endet mit einem Fehler.
    pub async fn eintrag_schalten(&self, eintrag: &EintragId, an: bool) -> anyhow::Result<()> {
        // Immer nur eine Schaltung: sonst überschreibt eine die andere.
        let _schaltung = self.innen.schalt_wache.lock().await;
        let neu = {
            let stand = self.innen.stand();
            if !stand
                .soll
                .eintraege
                .iter()
                .any(|e| &e.eintrag.id == eintrag)
            {
                anyhow::bail!("Eintrag {eintrag} ist diesem Gerät nicht zugewiesen");
            }
            let mut neu = stand.am_geraet_aus.clone();
            let geaendert = if an {
                neu.remove(eintrag)
            } else {
                neu.insert(eintrag.clone())
            };
            if !geaendert {
                return Ok(());
            }
            neu
        };
        // Erst die Platte, dann der Speicher: ein Fehler lässt alles, wie es
        // war.
        self.innen.speichern_mit_aus(&neu)?;
        {
            let mut stand = self.innen.stand();
            stand.am_geraet_aus = neu;
        }
        if an {
            // Wie nach einer Zuweisung – aber nur, wenn der Server das Gerät in
            // diesem Lauf angenommen hat. Sonst wartet der nächste Abgleich.
            let startbar = {
                let stand = self.innen.stand();
                stand.angenommen && !stand.abgelehnt
            };
            if startbar {
                if let Some(soll) = self.innen.soll_eintrag(eintrag) {
                    starte_in(&self.innen, &soll).await;
                }
            }
        } else {
            stoppe_in(&self.innen, eintrag);
        }
        self.innen.modell_schicken();
        self.innen.liste_geaendert().await;
        self.innen.melde().await;
        Ok(())
    }

    /// Ist der Eintrag auf diesem Gerät abgeschaltet (Aufgabe 19)?
    fn aus_am_geraet(&self, eintrag: &EintragId) -> bool {
        self.innen.stand().am_geraet_aus.contains(eintrag)
    }

    /// Der Eintrag ist abgeschaltet – Meldung für Befehle, die ihn anfassen
    /// würden.
    fn abgeschaltet(&self, eintrag: &EintragId) -> anyhow::Error {
        let titel = self
            .innen
            .soll_eintrag(eintrag)
            .map(|soll| soll.eintrag.titel)
            .unwrap_or_else(|| eintrag.als_str().to_string());
        anyhow::anyhow!(
            "{titel} ist auf diesem Gerät abgeschaltet – im quiver-Fenster unter Tools einschalten."
        )
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

    /// Die Angaben dieses Geräts sichern (Aufgabe 15).
    ///
    /// Ohne Passwort gehen nur die offenen Werte und Zugänge in die Datei
    /// (`"geheim": null`); mit Passwort kommen die Geheimnisse aus dem Tresor
    /// verschlüsselt dazu. Geräte-Token, Agent-Tokens und Anmeldungen der Tools
    /// sind nie dabei.
    pub fn einstellungen_exportieren(
        &self,
        pfad: &Path,
        passwort: Option<&str>,
    ) -> anyhow::Result<Zusammenfassung> {
        let passwort = passwort.map(str::trim).filter(|wert| !wert.is_empty());
        let (eintraege, geheimnisse) = {
            let stand = self.innen.stand();
            export_daten(&stand, passwort.is_some())
        };
        einstellungen::schreiben(pfad, eintraege, geheimnisse.as_ref(), passwort)
    }

    /// Was ein Import täte – ohne etwas zu ändern (Vorschau im Fenster).
    pub fn einstellungen_pruefen(
        &self,
        pfad: &Path,
        passwort: Option<&str>,
    ) -> anyhow::Result<Vorschau> {
        let gelesen = einstellungen::lesen(pfad, passwort)?;
        let stand = self.innen.stand();
        Ok(einstellungen::vorschau(
            &gelesen,
            &stand.soll,
            &stand.lokale_werte,
            &stand.zugaenge,
            &stand.am_geraet_aus,
        ))
    }

    /// Eine Sicherung übernehmen (Aufgabe 15).
    ///
    /// Gemischt wird, nicht ersetzt: Werte aus der Datei überschreiben
    /// gleichnamige, was nur am Gerät steht, bleibt; Zugänge werden über den
    /// Namen zusammengeführt. Einträge, die es hier nicht gibt (nicht im Soll),
    /// werden übersprungen. Ein falsches Passwort ist ein Fehler, bevor
    /// irgendetwas geschrieben wird. Danach starten die betroffenen Einträge
    /// neu, wie bei [`Kern::werte_setzen`].
    pub async fn einstellungen_importieren(
        &self,
        pfad: &Path,
        passwort: Option<&str>,
    ) -> anyhow::Result<Zusammenfassung> {
        let passwort = passwort.map(str::trim).filter(|wert| !wert.is_empty());
        let gelesen = einstellungen::lesen(pfad, passwort)?;
        if gelesen.hat_geheimnisse && gelesen.geheimnisse.is_none() {
            anyhow::bail!("Die Sicherung enthält Geheimnisse – dafür wird das Passwort gebraucht");
        }
        // Der Import kann abschalten, also nie neben einer Schaltung.
        let _schaltung = self.innen.schalt_wache.lock().await;
        // Was beim Lesen schon übergangen wurde (unbrauchbare Kennungen,
        // Mengen-Grenzen), steht in der Zusammenfassung (Paket 3).
        let mut zusammenfassung = Zusammenfassung {
            uebersprungen: gelesen.uebersprungen.clone(),
            ..Zusammenfassung::default()
        };
        let mut betroffen: Vec<EintragId> = Vec::new();
        let mut neue_werte: Vec<(EintragId, Werte)> = Vec::new();
        let mut neue_zugaenge: Vec<(EintragId, BTreeMap<String, Werte>)> = Vec::new();
        let mut neue_aus: Vec<EintragId> = Vec::new();
        let mut schreibungen: Vec<(String, String)> = Vec::new();
        let bekannt: BTreeMap<String, EintragId> = {
            let stand = self.innen.stand();
            stand
                .soll
                .eintraege
                .iter()
                .map(|eintrag| {
                    (
                        eintrag.eintrag.id.als_str().to_string(),
                        eintrag.eintrag.id.clone(),
                    )
                })
                .collect()
        };
        {
            let stand = self.innen.stand();
            for (id, teil) in &gelesen.eintraege {
                let Some(eintrag) = bekannt.get(id) else {
                    zusammenfassung.uebersprungen.push(id.clone());
                    continue;
                };
                let vorher = stand.lokale_werte.get(eintrag).cloned().unwrap_or_default();
                zusammenfassung.werte += teil
                    .werte
                    .iter()
                    .filter(|(name, wert)| {
                        !wert.trim().is_empty() && vorher.get(*name) != Some(*wert)
                    })
                    .count();
                zusammenfassung.zugaenge += teil.zugaenge.len();
                zusammenfassung.eintraege += 1;
                // Aus bleibt aus: die Wahl der Datei schaltet nur aus, nie ein.
                if einstellungen::aus_mischen(
                    stand.am_geraet_aus.contains(eintrag),
                    teil.am_geraet_aus,
                ) {
                    neue_aus.push(eintrag.clone());
                }
                neue_werte.push((
                    eintrag.clone(),
                    einstellungen::werte_mischen(&vorher, &teil.werte),
                ));
                let leer = BTreeMap::new();
                let vorher_zugaenge = stand.zugaenge.get(eintrag).unwrap_or(&leer);
                neue_zugaenge.push((
                    eintrag.clone(),
                    einstellungen::zugaenge_mischen(vorher_zugaenge, &teil.zugaenge),
                ));
                betroffen.push(eintrag.clone());
            }
        }
        // Geheimnisse gehören nur zu Einträgen, die es hier gibt.
        if let Some(geheimnisse) = &gelesen.geheimnisse {
            for (id, werte) in &geheimnisse.eintraege {
                let Some(eintrag) = bekannt.get(id) else {
                    continue;
                };
                for (feld, wert) in werte {
                    if !wert.trim().is_empty() {
                        schreibungen.push((geheim_schluessel(eintrag, feld), wert.clone()));
                    }
                }
            }
            for (id, liste) in &geheimnisse.zugaenge {
                let Some(eintrag) = bekannt.get(id) else {
                    continue;
                };
                for (name, werte) in liste {
                    for (feld, wert) in werte {
                        if !wert.trim().is_empty() {
                            schreibungen.push((
                                zugang_geheim_schluessel(eintrag, name, feld),
                                wert.clone(),
                            ));
                        }
                    }
                }
            }
        }
        zusammenfassung.geheimnisse = schreibungen.len();
        // Erst schreiben, wenn alles geprüft ist: ein Fehler ändert nichts.
        for (schluessel, wert) in &schreibungen {
            self.innen.konfig.tresor.schreiben(schluessel, wert)?;
        }
        {
            let mut stand = self.innen.stand();
            for (eintrag, werte) in neue_werte {
                if werte.is_empty() {
                    stand.lokale_werte.remove(&eintrag);
                } else {
                    stand.lokale_werte.insert(eintrag, werte);
                }
            }
            for (eintrag, liste) in neue_zugaenge {
                if liste.is_empty() {
                    stand.zugaenge.remove(&eintrag);
                } else {
                    stand.zugaenge.insert(eintrag, liste);
                }
            }
            for eintrag in &neue_aus {
                stand.am_geraet_aus.insert(eintrag.clone());
            }
            if let Some(geheimnisse) = &gelesen.geheimnisse {
                for (id, werte) in &geheimnisse.eintraege {
                    let Some(eintrag) = bekannt.get(id) else {
                        continue;
                    };
                    let gespeichert = stand.geheim.entry(eintrag.clone()).or_default();
                    for (feld, wert) in werte {
                        if !wert.trim().is_empty() {
                            gespeichert.insert(feld.clone(), wert.clone());
                        }
                    }
                }
                for (id, liste) in &geheimnisse.zugaenge {
                    let Some(eintrag) = bekannt.get(id) else {
                        continue;
                    };
                    for (name, werte) in liste {
                        let gespeichert = stand
                            .zugang_geheim
                            .entry(eintrag.clone())
                            .or_default()
                            .entry(name.clone())
                            .or_default();
                        for (feld, wert) in werte {
                            if !wert.trim().is_empty() {
                                gespeichert.insert(feld.clone(), wert.clone());
                            }
                        }
                    }
                }
            }
        }
        self.innen.speichern();
        for eintrag in &betroffen {
            self.innen.neu_starten(eintrag).await;
        }
        Ok(zusammenfassung)
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
        // Die Profil-Sperre fällt mit dem Beenden: ein neuer Kern auf
        // demselben Profil (ein Neustart) darf starten, ein **gleichzeitig**
        // laufender nicht (Paket 3).
        self.innen
            ._profil_sperre
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .take();
        // Laufende Einrichtungen brechen ab, damit kein Kindprozess stehen
        // bleibt (Paket 3).
        let abbrueche: Vec<Abbruch> = self
            .innen
            .stand()
            .einrichtung_abbruch
            .values()
            .cloned()
            .collect();
        for abbruch in abbrueche {
            abbruch.abbrechen();
        }
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
