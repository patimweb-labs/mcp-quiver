// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Warum der Client endet — und warum nicht.
//!
//! Jedes Ende bekommt **eine Ursache** ins Protokoll: Tray, Fenster
//! (weggelegt, kein Ende), Cmd+Q (auch kein Ende, es legt nur weg), Strg+C,
//! SIGTERM, eine zweite Instanz oder das Abmelden des Systems. Nur das Tray
//! beendet wirklich; SIGTERM wird wie Strg+C geordnet behandelt.

use std::sync::{Arc, Mutex};

/// Warum der Client endet (oder warum er weiterläuft).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndeGrund {
    /// „Beenden“ im Tray-Menü — der einzige Weg, quiver wirklich zu beenden.
    Tray,
    /// Das Fenster wurde geschlossen und in den Tray gelegt (kein Ende).
    FensterWeggelegt,
    /// Cmd+Q: legt das Fenster in den Tray, beendet nicht.
    CmdQ,
    /// Das Fenster wurde auf andere Weise geschlossen (System, Rahmen); der
    /// Kern hält geordnet an.
    FensterBeendet,
    /// Strg+C im Terminal.
    StrgC,
    /// SIGTERM (Unix), etwa beim Abmelden des Systems.
    Sigterm,
    /// Eine zweite Instanz hat das Fenster der laufenden geöffnet; dieser
    /// Start endet.
    ZweiteInstanz,
}

impl EndeGrund {
    /// Der Satz, der im Protokoll steht.
    pub fn zeile(self) -> &'static str {
        match self {
            EndeGrund::Tray => "Beenden über das Tray-Menü",
            EndeGrund::FensterWeggelegt => {
                "Fenster geschlossen — in den Tray gelegt, quiver läuft weiter"
            }
            EndeGrund::CmdQ => "Cmd+Q — Fenster in den Tray gelegt, quiver läuft weiter",
            EndeGrund::FensterBeendet => "Fenster geschlossen — quiver wird beendet",
            EndeGrund::StrgC => "Strg+C — quiver wird beendet",
            EndeGrund::Sigterm => "SIGTERM — quiver wird beendet",
            EndeGrund::ZweiteInstanz => {
                "Zweite Instanz — Fenster der laufenden geöffnet, dieser Start endet"
            }
        }
    }

    /// Ins Protokoll schreiben.
    pub fn protokollieren(self) {
        tracing::info!(ende = ?self, "{}", self.zeile());
    }
}

/// Eine geteilte Fahne: Der Signal-Faden setzt die Ursache, das Fenster liest
/// sie und schließt geordnet.
#[derive(Debug, Clone, Default)]
pub struct EndeFahne {
    innen: Arc<Mutex<Option<EndeGrund>>>,
}

impl EndeFahne {
    pub fn neu() -> Self {
        Self::default()
    }

    /// Die Ursache setzen (nur die erste zählt).
    pub fn setzen(&self, grund: EndeGrund) {
        let mut innen = self.innen.lock().unwrap_or_else(|e| e.into_inner());
        if innen.is_none() {
            *innen = Some(grund);
            grund.protokollieren();
        }
    }

    pub fn grund(&self) -> Option<EndeGrund> {
        *self.innen.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn ist_gesetzt(&self) -> bool {
        self.grund().is_some()
    }
}

/// Auf Strg+C und (Unix) SIGTERM warten.
pub async fn warte_auf_signal() -> EndeGrund {
    warter().await
}

#[cfg(unix)]
async fn warter() -> EndeGrund {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(term) => term,
        Err(fehler) => {
            tracing::warn!("SIGTERM nicht abonnierbar: {fehler}");
            let _ = tokio::signal::ctrl_c().await;
            return EndeGrund::StrgC;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => EndeGrund::StrgC,
        _ = term.recv() => EndeGrund::Sigterm,
    }
}

#[cfg(not(unix))]
async fn warter() -> EndeGrund {
    let _ = tokio::signal::ctrl_c().await;
    EndeGrund::StrgC
}

/// So lange darf das Fenster brauchen, um nach einem Signal selbst zu enden.
pub const FENSTER_FRIST: std::time::Duration = std::time::Duration::from_secs(3);

/// Im Fensterbetrieb: Signale in einem eigenen Faden abwarten und die
/// Oberfläche wecken. `ctx` kommt von eframe; ohne ihn bliebe das Fenster
/// stehen, bis der User es anfasst.
///
/// Ein weggelegtes Fenster zeichnet je nach System gar nicht mehr und sähe die
/// Fahne nie. Nach [`FENSTER_FRIST`] hält deshalb `anhalten` den Kern selbst an
/// und der Prozess endet.
pub fn signal_faden(
    ctx: eframe::egui::Context,
    fahne: EndeFahne,
    anhalten: impl FnOnce() + Send + 'static,
) {
    std::thread::spawn(move || {
        let laufzeit = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(laufzeit) => laufzeit,
            Err(fehler) => {
                tracing::warn!("Signal-Faden nicht startbar: {fehler}");
                return;
            }
        };
        let grund = laufzeit.block_on(warte_auf_signal());
        fahne.setzen(grund);
        let beginn = std::time::Instant::now();
        while beginn.elapsed() < FENSTER_FRIST {
            ctx.request_repaint();
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        tracing::info!("Das Fenster ist weggelegt — der Kern wird direkt angehalten");
        anhalten();
        std::process::exit(0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jede_ursache_hat_einen_eigenen_satz() {
        let alle = [
            EndeGrund::Tray,
            EndeGrund::FensterWeggelegt,
            EndeGrund::CmdQ,
            EndeGrund::FensterBeendet,
            EndeGrund::StrgC,
            EndeGrund::Sigterm,
            EndeGrund::ZweiteInstanz,
        ];
        let mut saetze: Vec<&str> = alle.iter().map(|grund| grund.zeile()).collect();
        saetze.sort_unstable();
        saetze.dedup();
        assert_eq!(saetze.len(), alle.len(), "jede Ursache ist unterscheidbar");
        for satz in saetze {
            assert!(!satz.trim().is_empty());
        }
        // Cmd+Q und das Schließen sind ausdrücklich **kein** Ende.
        assert!(EndeGrund::CmdQ.zeile().contains("läuft weiter"));
        assert!(EndeGrund::FensterWeggelegt.zeile().contains("läuft weiter"));
    }

    #[test]
    fn die_fahne_nimmt_nur_die_erste_ursache() {
        let fahne = EndeFahne::neu();
        assert!(!fahne.ist_gesetzt());
        fahne.setzen(EndeGrund::Tray);
        fahne.setzen(EndeGrund::Sigterm);
        assert_eq!(fahne.grund(), Some(EndeGrund::Tray));
    }
}
