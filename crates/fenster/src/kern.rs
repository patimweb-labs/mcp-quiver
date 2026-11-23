// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der echte Zugang zum Kern: [`quiver_client::Kern`] läuft in einer eigenen
//! Tokio-Laufzeit, das Fenster fragt nur den zuletzt bekannten Stand ab und
//! gibt Aktionen weiter. So bleibt das Zeichnen frei von Warten
//! (AGENTS.md §2.4) und der Test kann den Kern durch eine Attrappe ersetzen.

use std::sync::{Arc, Mutex};

use eframe::egui;
use quiver_client::{Kern, KernKonfig, Modell};

use crate::app::{Aktion, KernZugang};
use crate::logik;
use crate::modell::Meldung;

/// Kern und Fenster verbunden.
pub struct EchterKern {
    laufzeit: tokio::runtime::Runtime,
    kern: Kern,
    /// Zuletzt gemeldeter Stand, von einer Hintergrundaufgabe fortgeschrieben.
    stand: Arc<Mutex<Modell>>,
    /// Meldungen, die das Fenster noch abholen muss (Token, Fehler).
    meldungen: Arc<Mutex<Vec<Meldung>>>,
    /// Womit sich das Fenster für „neuer Stand" wecken lässt.
    wecker: Arc<Mutex<Option<egui::Context>>>,
}

impl EchterKern {
    /// Kern starten. Der Aufruf wartet, bis der Kern läuft (Endpunkt gebunden,
    /// gespeicherter Stand geladen) — danach zeichnet das Fenster.
    pub fn starten(konfig: KernKonfig) -> anyhow::Result<Self> {
        let laufzeit = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let kern = laufzeit.block_on(Kern::starten(konfig))?;
        let stand = Arc::new(Mutex::new(kern.modell().borrow().clone()));
        let meldungen: Arc<Mutex<Vec<Meldung>>> = Arc::new(Mutex::new(Vec::new()));
        let wecker: Arc<Mutex<Option<egui::Context>>> = Arc::new(Mutex::new(None));

        // Ein Zuschauer: jede Änderung des Kerns landet im Zwischenspeicher und
        // weckt das Fenster. Ohne Fenster (vor `wecken`) wird nur gespeichert.
        let mut empfänger = kern.modell();
        let ziel = stand.clone();
        let wecker_lauf = wecker.clone();
        laufzeit.spawn(async move {
            loop {
                let neu = empfänger.borrow_and_update().clone();
                *ziel.lock().unwrap_or_else(|e| e.into_inner()) = neu;
                if let Some(ctx) = wecker_lauf
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .as_ref()
                {
                    ctx.request_repaint();
                }
                if empfänger.changed().await.is_err() {
                    break;
                }
            }
        });

        Ok(Self {
            laufzeit,
            kern,
            stand,
            meldungen,
            wecker,
        })
    }

    /// Das Fenster meldet sich an: ab jetzt wird es bei jeder Änderung geweckt.
    pub fn wecken(&self, ctx: egui::Context) {
        *self.wecker.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx);
    }
}

impl KernZugang for EchterKern {
    fn ausfuehren(&self, aktion: Aktion) {
        let kern = self.kern.clone();
        let meldungen = self.meldungen.clone();
        let wecker = self.wecker.clone();
        self.laufzeit.spawn(async move {
            let ergebnis = match aktion {
                Aktion::Einbinden { server, code } => {
                    kern.einbinden(&server, &code, &logik::geraetename()).await
                }
                Aktion::Abmelden => kern.abmelden().await,
                Aktion::WerteSetzen {
                    eintrag,
                    lokal,
                    geheim,
                } => kern.werte_setzen(&eintrag, lokal, geheim).await,
                Aktion::AgentKoppeln { name } => kern.agent_koppeln(&name).await.map(|token| {
                    meldungen
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(Meldung::Token { name, token });
                }),
                Aktion::AgentTokenErneuern { id } => {
                    // Der Name gehört zur Meldung; er steht schon im Modell.
                    let name = kern
                        .modell()
                        .borrow()
                        .agenten
                        .iter()
                        .find(|agent| agent.id == id)
                        .map(|agent| agent.name.clone())
                        .unwrap_or_else(|| id.clone());
                    kern.agent_token_erneuern(&id).await.map(|token| {
                        meldungen
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push(Meldung::Token { name, token });
                    })
                }
                Aktion::AgentEntfernen { id } => kern.agent_entfernen(&id).await,
                Aktion::EintragAnmelden { eintrag } => kern.anmelden(&eintrag).await,
                Aktion::EintragAbmelden { eintrag } => kern.abmelden_eintrag(&eintrag).await,
                Aktion::EintragEinrichten { eintrag } => kern.einrichten(&eintrag).await,
                Aktion::ZugangSpeichern {
                    eintrag,
                    name,
                    lokal,
                    geheim,
                } => kern.zugang_speichern(&eintrag, &name, lokal, geheim).await,
                Aktion::ZugangLoeschen { eintrag, name } => {
                    kern.zugang_loeschen(&eintrag, &name).await
                }
                Aktion::ZugangGeheimLoeschen {
                    eintrag,
                    name,
                    feld,
                } => kern.zugang_geheim_loeschen(&eintrag, &name, &feld).await,
                // Den Browser öffnet das Fenster selbst (siehe `Fenster::bild`):
                // hier gibt es nichts zu tun.
                Aktion::LinkOeffnen(_) => Ok(()),
            };
            if let Err(fehler) = ergebnis {
                meldungen
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(Meldung::Fehler(format!("{fehler:#}")));
            }
            if let Some(ctx) = wecker.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
                ctx.request_repaint();
            }
        });
    }

    fn modell(&self) -> Modell {
        self.stand.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn meldungen(&self) -> Vec<Meldung> {
        std::mem::take(&mut *self.meldungen.lock().unwrap_or_else(|e| e.into_inner()))
    }

    fn beenden(&self) {
        let kern = self.kern.clone();
        // Hier läuft die Oberfläche, nicht die Laufzeit: blockieren ist erlaubt
        // und sorgt dafür, dass wirklich alles angehalten ist, bevor das
        // Programm endet.
        self.laufzeit.block_on(async move { kern.beenden().await });
    }
}

impl std::fmt::Debug for EchterKern {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EchterKern").finish_non_exhaustive()
    }
}
