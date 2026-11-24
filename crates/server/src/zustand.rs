// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was der Server im Speicher hält: Datenbank, Änderungs-Kanal, offene Ströme,
//! Konfiguration, OIDC-Client, Schlüssel, Drossel und Sperren.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use quiver_domain::GeraetId;
use rusqlite::Connection;
use tokio::sync::{broadcast, Mutex as AsyncMutex, Semaphore};

use crate::benutzer::ARGON2_GLEICHZEITIG;
use crate::drossel::Drossel;
use crate::krypto::Schluessel;
use crate::oidc::Oidc;
use crate::Konfig;

/// Der gemeinsame Zustand aller Aufrufe.
pub(crate) struct Zustand {
    /// Eine Verbindung, von einer blockierenden Aufgabe gehalten.
    pub(crate) db: Mutex<Connection>,
    /// „Etwas hat sich geändert" – jede Schreiboperation löst aus.
    pub(crate) geaendert: broadcast::Sender<()>,
    /// Offene Ereignis-Ströme je Gerät; solange einer offen ist, gilt es als verbunden.
    verbunden: Mutex<HashMap<GeraetId, usize>>,
    pub(crate) konfig: Arc<Konfig>,
    pub(crate) oidc: Option<Arc<Oidc>>,
    pub(crate) schluessel: Schluessel,
    pub(crate) drossel: Drossel,
    /// Höchstens zwei Argon2-Prüfungen gleichzeitig.
    pub(crate) argon: Semaphore,
    /// Ein Einlösen je Benutzer; verhindert doppelte Refresh-Aufrufe.
    refresh: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
    pub(crate) uhr: crate::config::Uhr,
}

impl Zustand {
    pub(crate) fn neu(
        conn: Connection,
        konfig: Konfig,
        schluessel: Schluessel,
        oidc: Option<Arc<Oidc>>,
    ) -> Self {
        let (geaendert, _) = broadcast::channel(64);
        let uhr = konfig.uhr.clone();
        Self {
            db: Mutex::new(conn),
            geaendert,
            verbunden: Mutex::new(HashMap::new()),
            konfig: Arc::new(konfig),
            oidc,
            schluessel,
            drossel: Drossel::neu(),
            argon: Semaphore::new(ARGON2_GLEICHZEITIG),
            refresh: Mutex::new(HashMap::new()),
            uhr,
        }
    }

    /// Alle Ströme über eine Änderung in Kenntnis setzen.
    pub(crate) fn melde_aenderung(&self) {
        let _ = self.geaendert.send(());
    }

    pub(crate) fn strom_auf(&self, id: &GeraetId) {
        let mut karte = crate::sperre(&self.verbunden);
        *karte.entry(id.clone()).or_insert(0) += 1;
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
}

/// Zählt einen offenen Strom und räumt beim Fallenlassen auf.
pub(crate) struct Stromwaechter {
    zustand: Arc<Zustand>,
    id: GeraetId,
}

impl Stromwaechter {
    pub(crate) fn neu(zustand: Arc<Zustand>, id: GeraetId) -> Self {
        zustand.strom_auf(&id);
        Self { zustand, id }
    }
}

impl Drop for Stromwaechter {
    fn drop(&mut self) {
        self.zustand.strom_zu(&self.id);
    }
}
