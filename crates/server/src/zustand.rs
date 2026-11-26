// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was der Server im Speicher hält: Datenbank, Änderungs-Kanal, offene Ströme,
//! Konfiguration, OIDC-Client, Schlüssel, Drossel und Sperren.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use quiver_domain::GeraetId;
use rusqlite::Connection;
use time::OffsetDateTime;
use tokio::sync::{broadcast, Mutex as AsyncMutex, Semaphore};

use crate::benutzer::ARGON2_GLEICHZEITIG;
use crate::drossel::Drossel;
use crate::einrichtung::Einrichtung;
use crate::graph::Graph;
use crate::krypto::Schluessel;
use crate::oidc::Oidc;
use crate::Konfig;

/// So viele Ereignis-Ströme darf ein Gerät gleichzeitig offen halten.
///
/// Ein Client braucht einen; ein zweiter ist ein Neustart, der den alten noch
/// nicht geschlossen hat. Mehr ist ein Fehler und soll den Server nicht binden.
pub const STROEME_JE_GERAET: usize = 4;
/// So viele Ereignis-Ströme dürfen insgesamt offen sein.
pub const STROEME_GESAMT: usize = 200;

/// Der gemeinsame Zustand aller Aufrufe.
pub(crate) struct Zustand {
    /// Eine Verbindung, von einer blockierenden Aufgabe gehalten.
    pub(crate) db: Mutex<Connection>,
    /// „Etwas hat sich geändert" – jede Schreiboperation löst aus.
    pub(crate) geaendert: broadcast::Sender<()>,
    /// „Der Server fährt herunter" – offene Ereignis-Ströme enden sofort.
    ende: broadcast::Sender<()>,
    /// Wann die Prüfspur zuletzt aufgeräumt wurde (höchstens stündlich).
    pruefspur_zuletzt: Mutex<Option<OffsetDateTime>>,
    /// Offene Ereignis-Ströme je Gerät; solange einer offen ist, gilt es als verbunden.
    verbunden: Mutex<HashMap<GeraetId, usize>>,
    pub(crate) konfig: Arc<Konfig>,
    pub(crate) oidc: Option<Arc<Oidc>>,
    /// Nur gesetzt, wenn Gruppennamen geholt werden sollen (Aufgabe 30).
    pub(crate) graph: Option<Arc<Graph>>,
    /// Bis wann nach einem 403 nicht mehr gefragt wird.
    graph_pause: Mutex<Option<OffsetDateTime>>,
    /// Wann zuletzt ein Graph-Fehler im Log stand.
    graph_log: Mutex<Option<OffsetDateTime>>,
    pub(crate) schluessel: Schluessel,
    pub(crate) drossel: Drossel,
    /// Die Programme, die der Server an die Geräte verteilt (Aufgabe 37).
    pub(crate) programme: Arc<crate::programme::Ablage>,
    /// Argon2-Hash für unbekannte E-Mails, beim Start erzeugt (Aufgabe 26.4).
    pub(crate) dummy: String,
    /// Der Stand der Ersteinrichtung (Aufgabe 25).
    pub(crate) einrichtung: Einrichtung,
    /// Höchstens zwei Argon2-Prüfungen gleichzeitig.
    pub(crate) argon: Semaphore,
    /// Ein Einlösen je Benutzer; verhindert doppelte Refresh-Aufrufe.
    refresh: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    pub(crate) uhr: crate::config::Uhr,
    /// Die zuletzt gelesene Veröffentlichung (Aufgabe 34).
    pub(crate) updates: Mutex<crate::updates::Lage>,
    /// Nur ein Abruf der Veröffentlichung zur Zeit (Aufgabe 39): ein Paket,
    /// ein Weg nach draußen.
    pub(crate) update_wache: AsyncMutex<()>,
}

impl Zustand {
    pub(crate) fn neu(
        conn: Connection,
        konfig: Konfig,
        schluessel: Schluessel,
        oidc: Option<Arc<Oidc>>,
        graph: Option<Arc<Graph>>,
        dummy: String,
    ) -> Self {
        let (geaendert, _) = broadcast::channel(64);
        let (ende, _) = broadcast::channel(1);
        let uhr = konfig.uhr.clone();
        let einrichtung = Einrichtung::neu(&konfig.oeffentlich, konfig.einrichtung_mit_code());
        let programme = Arc::new(crate::programme::Ablage::neu(konfig.programme.clone()));
        Self {
            db: Mutex::new(conn),
            geaendert,
            ende,
            pruefspur_zuletzt: Mutex::new(None),
            verbunden: Mutex::new(HashMap::new()),
            konfig: Arc::new(konfig),
            oidc,
            graph,
            graph_pause: Mutex::new(None),
            graph_log: Mutex::new(None),
            schluessel,
            drossel: Drossel::neu(),
            programme,
            dummy,
            einrichtung,
            argon: Semaphore::new(ARGON2_GLEICHZEITIG),
            refresh: Mutex::new(HashMap::new()),
            uhr,
            updates: Mutex::new(crate::updates::Lage::neu()),
            update_wache: AsyncMutex::new(()),
        }
    }

    /// Alle Ströme über eine Änderung in Kenntnis setzen.
    pub(crate) fn melde_aenderung(&self) {
        let _ = self.geaendert.send(());
    }

    /// Alle offenen Ereignis-Ströme beenden (Herunterfahren).
    pub(crate) fn beenden(&self) {
        let _ = self.ende.send(());
    }

    /// Ein Abonnement auf das Ende des Servers.
    pub(crate) fn ende_abo(&self) -> broadcast::Receiver<()> {
        self.ende.subscribe()
    }

    /// Ist das Aufräumen der Prüfspur fällig? Höchstens einmal je Stunde.
    pub(crate) fn pruefspur_aufraeumen_faellig(&self, jetzt: OffsetDateTime) -> bool {
        let mut zuletzt = crate::sperre(&self.pruefspur_zuletzt);
        if zuletzt.is_some_and(|zeit| jetzt - zeit < time::Duration::hours(1)) {
            return false;
        }
        *zuletzt = Some(jetzt);
        true
    }

    pub(crate) fn strom_auf(&self, id: &GeraetId) -> bool {
        let mut karte = crate::sperre(&self.verbunden);
        if karte.values().sum::<usize>() >= STROEME_GESAMT {
            return false;
        }
        let anzahl = karte.entry(id.clone()).or_insert(0);
        if *anzahl >= STROEME_JE_GERAET {
            return false;
        }
        *anzahl += 1;
        true
    }

    pub(crate) fn strom_zu(&self, id: &GeraetId) {
        let mut karte = crate::sperre(&self.verbunden);
        if let Some(anzahl) = karte.get_mut(id) {
            *anzahl = anzahl.saturating_sub(1);
            if *anzahl == 0 {
                karte.remove(id);
            }
        }
    }

    pub(crate) fn verbundene(&self) -> Vec<GeraetId> {
        crate::sperre(&self.verbunden).keys().cloned().collect()
    }

    /// Die Sperre für das Nachprüfen eines Benutzers.
    pub(crate) fn refresh_sperre(&self, oid: &str) -> Arc<AsyncMutex<()>> {
        let mut karte = crate::sperre(&self.refresh);
        karte
            .entry(oid.to_string())
            .or_insert_with(|| Arc::new(AsyncMutex::new(())))
            .clone()
    }

    /// Nach einem 403 einen Tag nicht mehr fragen (Aufgabe 30).
    pub(crate) fn graph_pausiert(&self, jetzt: OffsetDateTime) -> bool {
        crate::sperre(&self.graph_pause).is_some_and(|bis| jetzt < bis)
    }

    pub(crate) fn graph_pausieren(&self, bis: OffsetDateTime) {
        *crate::sperre(&self.graph_pause) = Some(bis);
    }

    /// Darf jetzt ein Graph-Fehler ins Log? Höchstens einmal je Stunde.
    pub(crate) fn graph_log_faellig(&self, jetzt: OffsetDateTime) -> bool {
        let mut zuletzt = crate::sperre(&self.graph_log);
        if zuletzt
            .is_some_and(|zeit| jetzt - zeit < time::Duration::hours(crate::graph::LOG_STUNDEN))
        {
            return false;
        }
        *zuletzt = Some(jetzt);
        true
    }
}

/// Zählt einen offenen Strom und räumt beim Fallenlassen auf.
pub(crate) struct Stromwaechter {
    zustand: Arc<Zustand>,
    id: GeraetId,
}

impl Stromwaechter {
    /// `None`, wenn die Grenze erreicht ist (je Gerät oder insgesamt).
    pub(crate) fn neu(zustand: Arc<Zustand>, id: GeraetId) -> Option<Self> {
        if !zustand.strom_auf(&id) {
            return None;
        }
        Some(Self { zustand, id })
    }
}

impl Drop for Stromwaechter {
    fn drop(&mut self) {
        self.zustand.strom_zu(&self.id);
    }
}
