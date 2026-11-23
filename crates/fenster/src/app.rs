// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Anwendung: `render(&Sicht) → Vec<Aktion>` (AGENTS.md §2.1). Die
//! Ansichten lesen die Sicht und legen Aktionen ab; ausgeführt werden sie vom
//! Kern — im Betrieb der echte ([`crate::kern::EchterKern`]), im Test eine
//! Attrappe.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use crate::ansichten;
use crate::modell::{Meldung, Seite, Sicht};
use crate::profil::Einstellungen;
use crate::thema::{self, Thema};
use crate::tray::{self, Befehl, Tray};
use eframe::egui;
use quiver_client::Modell;
use quiver_domain::{EintragId, Werte};

/// Wie der Autostart geschaltet wird — im Betrieb [`crate::autostart::setzen`],
/// im Test nichts (`None`), damit kein Test im System herumschreibt.
pub type AutostartSchalter = Box<dyn Fn(bool) -> anyhow::Result<()> + Send + Sync>;

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
    /// Ein neues Token für einen gekoppelten Agenten; es kommt wie beim
    /// Koppeln als [`Meldung::Token`] zurück.
    AgentTokenErneuern { id: String },
    /// Einem Agenten den Zugang nehmen.
    AgentEntfernen { id: String },
    /// Die Anmeldung eines Eintrags im Browser starten (nie ein Agent).
    EintragAnmelden { eintrag: EintragId },
    /// Die Anmeldung eines Eintrags beenden (nach der Rückfrage im Dialog).
    EintragAbmelden { eintrag: EintragId },
    /// Die Einrichtung eines Eintrags im Browser starten (Aufgabe 11): das
    /// Tool öffnet seine Seite selbst, ein Agent löst sie nie aus.
    EintragEinrichten { eintrag: EintragId },
    /// Einen Hilfe-Link neben einem Feld im Standardbrowser öffnen
    /// (Aufgabe 13). Sonst passiert nichts: keine Anmeldung, kein Lauscher.
    LinkOeffnen(String),
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
    /// Autostart schalten; `None` schreibt nichts (Tests).
    autostart: Option<AutostartSchalter>,
    /// Zuletzt angewandter Autostart (nur bei Änderung schreiben).
    autostart_angewandt: bool,
    /// Das Tray-Symbol, falls die Plattform eines hat.
    tray: Option<Tray>,
    /// Befehle von Tray und zweiter Instanz.
    befehle: Option<Receiver<Befehl>>,
    /// Beenden ist angefordert: dann wird das Fenster nicht mehr festgehalten.
    beenden: bool,
}

impl<K: KernZugang> Fenster<K> {
    pub fn neu(kern: K, sicht: Sicht, profil: Option<PathBuf>) -> Self {
        Self {
            autostart_angewandt: sicht.autostart,
            kern,
            sicht,
            profil,
            angewandt: None,
            gemerkt: None,
            autostart: None,
            tray: None,
            befehle: None,
            beenden: false,
        }
    }

    /// Wie der Autostart geschaltet wird (im Betrieb die Systemdatei).
    pub fn mit_autostart(mut self, schalter: AutostartSchalter) -> Self {
        self.autostart = Some(schalter);
        self
    }

    /// Tray-Symbol und die Befehle, die es (und eine zweite Instanz) schickt.
    pub fn mit_tray(mut self, tray: Option<Tray>, befehle: Receiver<Befehl>) -> Self {
        self.tray = tray;
        self.befehle = Some(befehle);
        self
    }

    /// Das Tray-Menü folgt aus dem Modell (reine Funktion in [`crate::logik`]).
    fn tray_pflegen(&self) {
        let Some(tray) = &self.tray else {
            return;
        };
        tray.setzen(crate::logik::tray_menu(
            &self.sicht.modell,
            self.sicht.autostart,
            self.sicht.sprache,
        ));
        tray.auffrischen();
    }

    /// Befehle ausführen, die nicht aus dem Fenster kommen: Tray und zweite
    /// Instanz. Sie müssen auch dann ankommen, wenn nichts zu sehen ist.
    fn befehle_ausfuehren(&mut self, ctx: &egui::Context) {
        let Some(empfaenger) = &self.befehle else {
            return;
        };
        let befehle: Vec<Befehl> = empfaenger.try_iter().collect();
        for befehl in befehle {
            match befehl {
                Befehl::Zeigen => tray::zeigen(ctx),
                Befehl::AutostartUmschalten => {
                    self.sicht.autostart = !self.sicht.autostart;
                }
                Befehl::Beenden => {
                    self.kern.beenden();
                    self.beenden = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// Den Autostart im System nachziehen. Der Haken zeigt, was dort steht:
    /// scheitert das Schreiben, springt er zurück.
    fn autostart_pflegen(&mut self) {
        if self.sicht.autostart == self.autostart_angewandt {
            return;
        }
        let Some(schalter) = &self.autostart else {
            self.autostart_angewandt = self.sicht.autostart;
            return;
        };
        match schalter(self.sicht.autostart) {
            Ok(()) => self.autostart_angewandt = self.sicht.autostart,
            Err(fehler) => {
                self.sicht.autostart = self.autostart_angewandt;
                self.sicht.hinweis = Some(format!("Autostart nicht änderbar: {fehler:#}"));
            }
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
            // Einen Hilfe-Link öffnet das Fenster selbst: der Standardbrowser
            // hängt am Fenster (egui-winit führt `OpenUrl` aus), nicht am Kern.
            // Damit braucht es keine weitere Abhängigkeit.
            if let Aktion::LinkOeffnen(url) = &aktion {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                continue;
            }
            self.kern.ausfuehren(aktion);
        }
        self.autostart_pflegen();
    }
}

impl<K: KernZugang> eframe::App for Fenster<K> {
    /// Läuft auch, wenn das Fenster weggelegt ist (Tray, zweite Instanz).
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.befehle_ausfuehren(ctx);
        self.tray_pflegen();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.befehle_ausfuehren(&ctx);
        schliessen_abfangen(&ctx, self.beenden);
        self.bild(ui);
        self.tray_pflegen();
    }

    /// Letzter Halt: Kern sauber anhalten (Fenster zu, Beenden aus dem Tray).
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.kern.beenden();
    }
}

/// Fenster schließen heißt nicht Programm beenden: der Kern läuft im Tray
/// weiter (Aufgabe 08). Das Schließen wird abgefangen und das Fenster
/// weggelegt. Nur wenn das Beenden schon angefordert ist (Tray-Menü), bleibt
/// es beim Schließen — sonst käme das Programm nie zum Ende.
///
/// Gibt zurück, ob abgefangen wurde.
pub fn schliessen_abfangen(ctx: &egui::Context, beenden: bool) -> bool {
    if beenden || !ctx.input(|i| i.viewport().close_requested()) {
        return false;
    }
    ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
    tray::weglegen(ctx);
    true
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
        assert_ne!(
            Aktion::EintragAnmelden {
                eintrag: EintragId::neu("postfach")
            },
            Aktion::EintragAbmelden {
                eintrag: EintragId::neu("postfach")
            },
            "anmelden und abmelden sind zwei Aktionen"
        );
        assert_ne!(
            Aktion::EintragAnmelden {
                eintrag: EintragId::neu("postfach")
            },
            Aktion::EintragEinrichten {
                eintrag: EintragId::neu("postfach")
            },
            "anmelden und einrichten sind zwei Aktionen"
        );
        assert_ne!(
            Aktion::LinkOeffnen("https://wiki.example".into()),
            Aktion::LinkOeffnen("https://wiki.example/zwei".into()),
            "die Adresse gehört zur Aktion"
        );
    }

    /// Ein Bild mit „das Fenster will schließen" — wie es der Fensterrahmen
    /// meldet. Geprüft wird, welche Befehle daraus folgen.
    fn schliessen(beenden: bool) -> Vec<egui::ViewportCommand> {
        let ctx = egui::Context::default();
        let mut eingabe = egui::RawInput::default();
        eingabe
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .expect("Wurzel-Fenster")
            .events
            .push(egui::ViewportEvent::Close);
        let mut ausgabe = ctx.run_ui(eingabe, |ui| {
            schliessen_abfangen(ui.ctx(), beenden);
        });
        let befehle = ausgabe.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .clone();
        // Ohne Zeichner nimmt niemand die Texturen ab.
        ausgabe.textures_delta.clear();
        befehle
    }

    #[test]
    fn fenster_schliessen_legt_weg_statt_zu_beenden() {
        let befehle = schliessen(false);
        assert!(
            befehle.contains(&egui::ViewportCommand::CancelClose),
            "das Schließen wird abgefangen: {befehle:?}"
        );
        let weglegen = if tray::MINIMIEREN {
            egui::ViewportCommand::Minimized(true)
        } else {
            egui::ViewportCommand::Visible(false)
        };
        assert!(
            befehle.contains(&weglegen),
            "Fenster weggelegt: {befehle:?}"
        );
    }

    #[test]
    fn beenden_schliesst_wirklich() {
        let befehle = schliessen(true);
        assert!(
            !befehle.contains(&egui::ViewportCommand::CancelClose),
            "beim Beenden darf nichts abgefangen werden: {befehle:?}"
        );
    }
}
