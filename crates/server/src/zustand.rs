// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Was der Server im Speicher hält: Datenbank, Änderungs-Kanal, offene Ströme.

use std::collections::HashMap;
use std::sync::Mutex;

use quiver_domain::GeraetId;
use rusqlite::Connection;
use tokio::sync::broadcast;

/// Der gemeinsame Zustand aller Aufrufe.
pub(crate) struct Zustand {
    /// Eine Verbindung, von einer blockierenden Aufgabe gehalten.
    pub(crate) db: Mutex<Connection>,
    /// „Etwas hat sich geändert" – jede Admin-Schreiboperation löst aus.
    pub(crate) geaendert: broadcast::Sender<()>,
    /// Offene Ereignis-Ströme je Gerät; solange einer offen ist, gilt es als verbunden.
    verbunden: Mutex<HashMap<GeraetId, usize>>,
    pub(crate) admin_token: String,
}

impl Zustand {
    pub(crate) fn neu(conn: Connection, admin_token: String) -> Self {
        let (geaendert, _) = broadcast::channel(64);
        Self {
            db: Mutex::new(conn),
            geaendert,
            verbunden: Mutex::new(HashMap::new()),
            admin_token,
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
}

/// Zählt einen offenen Strom und räumt beim Fallenlassen auf.
pub(crate) struct Stromwaechter {
    zustand: std::sync::Arc<Zustand>,
    id: GeraetId,
}

impl Stromwaechter {
    pub(crate) fn neu(zustand: std::sync::Arc<Zustand>, id: GeraetId) -> Self {
        zustand.strom_auf(&id);
        Self { zustand, id }
    }
}

impl Drop for Stromwaechter {
    fn drop(&mut self) {
        self.zustand.strom_zu(&self.id);
    }
}
