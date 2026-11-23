// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Anwendung: `render(&Sicht) → Vec<Aktion>` (AGENTS.md §2.1). Die
//! Ansichten lesen die Sicht und legen Aktionen ab; ausgeführt werden sie vom
//! Kern — im Betrieb der echte ([`crate::kern::EchterKern`]), im Test eine
//! Attrappe.

use std::path::PathBuf;

use eframe::egui;
use quiver_client::Modell;
use quiver_domain::{EintragId, Werte};

use crate::ansichten;
use crate::modell::{Meldung, Seite, Sicht};
use crate::profil::Einstellungen;
use crate::thema::{self, Thema};

/// Was das Fenster vom Kern verlangt. Nur so kann die Oberfläche ohne Server
/// und ohne Laufzeit geprüft werden.
pub trait KernZugang {
    /// Eine Aktion ausführen. Sie darf nicht blockieren — das Fenster zeichnet
    /// in dieser Zeit weiter.
    fn ausfuehren(&self, aktion: Aktion);
    /// Der aktuelle Stand.
    fn modell(&self) -> Modell;
    /// Was der Kern dem Fenster sonst noch sagen will (frisches Token,
    /// Fehler). Standard: nichts.
    fn meldungen(&self) -> Vec<Meldung> {
        Vec::new()
    }
    /// Alles anhalten (Fenster zu, Strg+C). Standard: nichts zu tun.
    fn beenden(&self) {}
}

/// Eine Anweisung des Fensters an den Kern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Aktion {
    /// Gerät mit Server-Adresse und Einmal-Code einbinden.
    Einbinden { server: String, code: String },
    /// Gerät abmelden (Token weg, nichts läuft mehr).
    Abmelden,
    /// Werte eines Eintrags setzen; leere Geheimnisse bleiben unverändert.
    WerteSetzen {
        eintrag: EintragId,
        lokal: Werte,
        geheim: Werte,
    },
    /// Einen Agenten koppeln; das Token kommt als [`Meldung::Token`] zurück.
    AgentKoppeln { name: String },
    /// Einem Agenten den Zugang nehmen.
    AgentEntfernen { id: String },
}

/// Ein Bild: Kopf, Navigation, Seite, Dialog. Gibt die Aktionen zurück, die
/// aus den Eingaben dieses Bildes folgen.
pub fn render(ui: &mut egui::Ui, sicht: &mut Sicht) -> Vec<Aktion> {
    let mut aktionen = Vec::new();
    ansichten::kopf::zeigen(ui, sicht);
    ansichten::seitenleiste::zeigen(ui, sicht);
    egui::CentralPanel::default()
        // Ohne Rand: den setzt die Seite innen (`bausteine::seite`), damit der
        // Scrollbalken am Fensterrand sitzt.
        .frame(egui::Frame::new().fill(sicht.thema.farben().bg_canvas))
        .show(ui, |ui| match sicht.seite {
            Seite::Verbindung => ansichten::verbindung::zeigen(ui, sicht, &mut aktionen),
            Seite::Tools => ansichten::tools::zeigen(ui, sicht, &mut aktionen),
            Seite::Agenten => ansichten::agenten::zeigen(ui, sicht, &mut aktionen),
        });
    ansichten::dialog::zeigen(ui.ctx(), sicht, &mut aktionen);
    aktionen
}

/// Das Fenster: Kern, Sicht und der Ort, an dem Theme und Sprache liegen.
pub struct Fenster<K: KernZugang> {
    kern: K,
    pub sicht: Sicht,
    /// Ordner für `oberflaeche.json`; `None` schreibt nichts (Tests).
    profil: Option<PathBuf>,
    /// Zuletzt angewandtes Theme (egui nur setzen, wenn es sich ändert).
    angewandt: Option<Thema>,
    /// Zuletzt gemerkte Einstellungen (nur dann schreiben).
    gemerkt: Option<Einstellungen>,
}

impl<K: KernZugang> Fenster<K> {
    pub fn neu(kern: K, sicht: Sicht, profil: Option<PathBuf>) -> Self {
        Self {
            kern,
            sicht,
            profil,
            angewandt: None,
            gemerkt: None,
        }
    }

    /// Ein Bild: Modell holen, Meldungen einsammeln, zeichnen, Aktionen abgeben.
    pub fn bild(&mut self, ui: &mut egui::Ui) {
        self.sicht.modell_setzen(self.kern.modell());
        for meldung in self.kern.meldungen() {
            self.sicht.melden(meldung);
        }
        // Theme und Sprache gehören ins Profil, nicht nur in den Speicher.
        let jetzt = self.sicht.einstellungen();
        if self.gemerkt.as_ref() != Some(&jetzt) {
            if self.angewandt != Some(jetzt.thema()) {
                thema::anwenden(ui.ctx(), jetzt.thema());
                self.angewandt = Some(jetzt.thema());
            }
            if let Some(ordner) = &self.profil {
                if let Err(fehler) = jetzt.speichern(ordner) {
                    tracing::warn!("Oberflächen-Einstellungen nicht speicherbar: {fehler}");
                }
            }
            self.gemerkt = Some(jetzt);
        }

        for aktion in render(ui, &mut self.sicht) {
            // Ein neuer Versuch räumt den alten Fehler weg: er ist nicht mehr
            // der Stand der Dinge.
            self.sicht.hinweis = None;
            self.kern.ausfuehren(aktion);
        }
    }
}

impl<K: KernZugang> eframe::App for Fenster<K> {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.bild(ui);
    }

    /// Fenster zu heißt: Kern sauber anhalten.
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.kern.beenden();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aktionen_sind_vergleichbar() {
        // Der Test vergleicht Aktionen mit dem, was die Attrappe bekommen hat.
        let a = Aktion::AgentKoppeln { name: "pi".into() };
        assert_eq!(a.clone(), a);
        assert_ne!(a, Aktion::Abmelden);
    }
}
