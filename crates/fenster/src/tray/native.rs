// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Windows-Infobereich und macOS-Menüleiste über `tray-icon`.
//!
//! Das Symbol muss im Hauptthread entstehen und dort bleiben; eframe baut die
//! Anwendung dort, deshalb ruft [`crate::app`] [`Tray::starten`] beim Anlegen
//! des Fensters. Menüklicks kommen in der Ereignisschleife an und setzen nur
//! einen [`Befehl`]; ausgeführt wird er von der Anwendung.

use std::cell::RefCell;

use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

use super::symbol::{self, KANTEN};
use super::{Befehl, TrayZustand};
use crate::logik::TrayEintrag;

const ZUSTAND: &str = "zustand";
const AKTUALISIEREN: &str = "aktualisieren";
const ZEIGEN: &str = "zeigen";
const AUTOSTART: &str = "autostart";
const BEENDEN: &str = "beenden";

pub struct Tray {
    _icon: TrayIcon,
    zustand: MenuItem,
    zeigen: MenuItem,
    aktualisieren: MenuItem,
    autostart: CheckMenuItem,
    beenden: MenuItem,
    z: TrayZustand,
    /// Zuletzt ins Menü geschriebene Einträge (nur bei Änderung anfassen).
    angewandt: RefCell<Vec<TrayEintrag>>,
}

impl Tray {
    pub fn starten(z: TrayZustand) -> Option<Self> {
        let eintraege = z.menu();
        let text = |art: fn(&TrayEintrag) -> Option<&String>| {
            eintraege.iter().find_map(art).cloned().unwrap_or_default()
        };
        let zustand = MenuItem::with_id(
            ZUSTAND,
            text(|e| match e {
                TrayEintrag::Zustand(text) => Some(text),
                _ => None,
            }),
            // Eine Zeile zum Lesen, nicht zum Anklicken.
            false,
            None,
        );
        let zeigen = MenuItem::with_id(
            ZEIGEN,
            text(|e| match e {
                TrayEintrag::Zeigen(text) => Some(text),
                _ => None,
            }),
            true,
            None,
        );
        let aktualisieren = MenuItem::with_id(
            AKTUALISIEREN,
            text(|e| match e {
                TrayEintrag::Aktualisieren(text) => Some(text),
                _ => None,
            }),
            true,
            None,
        );
        let an = eintraege
            .iter()
            .any(|e| matches!(e, TrayEintrag::Autostart { an: true, .. }));
        let autostart = CheckMenuItem::with_id(
            AUTOSTART,
            text(|e| match e {
                TrayEintrag::Autostart { text, .. } => Some(text),
                _ => None,
            }),
            true,
            an,
            None,
        );
        let beenden = MenuItem::with_id(
            BEENDEN,
            text(|e| match e {
                TrayEintrag::Beenden(text) => Some(text),
                _ => None,
            }),
            true,
            None,
        );
        let trenner = PredefinedMenuItem::separator();
        let menu = Menu::new();
        for eintrag in [
            &zustand as &dyn tray_icon::menu::IsMenuItem,
            &zeigen,
            &aktualisieren,
            &autostart,
            &trenner,
            &beenden,
        ] {
            if let Err(fehler) = menu.append(eintrag) {
                tracing::warn!("Tray-Menü: {fehler}");
            }
        }
        // Die macOS-Menüleiste will ein kleines Bild und passt es an hell/dunkel
        // an (Template); Windows verkleinert selbst.
        let kante = if cfg!(target_os = "macos") {
            KANTEN[0]
        } else {
            KANTEN[1]
        };
        let rgba = match symbol::raster(kante) {
            Ok(rgba) => rgba,
            Err(fehler) => {
                tracing::warn!("Tray-Symbol: {fehler}");
                return None;
            }
        };
        let bild = tray_icon::Icon::from_rgba(rgba, kante, kante).ok()?;
        let bauer = TrayIconBuilder::new()
            .with_id("mcp-quiver")
            .with_tooltip("mcp-quiver")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(cfg!(target_os = "macos"));
        // macOS: Template-Bild, das sich hell und dunkel anpasst (die Methode
        // gibt es nur dort). Windows nimmt das farbige Bild unverändert.
        #[cfg(target_os = "macos")]
        let bauer = bauer.with_icon_templated(bild);
        #[cfg(not(target_os = "macos"))]
        let bauer = bauer.with_icon(bild);
        let icon = match bauer.build() {
            Ok(icon) => icon,
            Err(fehler) => {
                tracing::warn!("kein Tray: {fehler}");
                return None;
            }
        };
        let s = z.clone();
        MenuEvent::set_event_handler(Some(move |ereignis: MenuEvent| {
            if ereignis.id == ZEIGEN {
                s.senden(Befehl::Zeigen);
            } else if ereignis.id == AKTUALISIEREN {
                s.senden(Befehl::ToolsAktualisieren);
            } else if ereignis.id == AUTOSTART {
                s.senden(Befehl::AutostartUmschalten);
            } else if ereignis.id == BEENDEN {
                s.senden(Befehl::Beenden);
                // Ein weggelegtes Fenster zeichnet kein Bild und sähe den Befehl
                // nie: dann endet das Programm hier.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_secs(4));
                    std::process::exit(0);
                });
            }
        }));
        let s = z.clone();
        TrayIconEvent::set_event_handler(Some(move |ereignis: TrayIconEvent| {
            // Windows: Linksklick auf das Symbol holt das Fenster zurück.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = ereignis
            {
                s.senden(Befehl::Zeigen);
            }
        }));
        Some(Tray {
            _icon: icon,
            zustand,
            zeigen,
            aktualisieren,
            autostart,
            beenden,
            z,
            angewandt: RefCell::new(eintraege),
        })
    }

    /// Die Einträge setzen, die das Menü zeigt (abgeleitet aus dem Modell).
    pub fn setzen(&self, eintraege: Vec<TrayEintrag>) {
        self.z.setzen(eintraege);
    }

    /// Texte und Haken nachziehen. Billig; die Anwendung ruft es jedes Bild.
    pub fn auffrischen(&self) {
        let eintraege = self.z.menu();
        {
            let mut angewandt = self.angewandt.borrow_mut();
            if *angewandt == eintraege {
                return;
            }
            *angewandt = eintraege.clone();
        }
        for eintrag in &eintraege {
            match eintrag {
                TrayEintrag::Zustand(text) => self.zustand.set_text(text),
                TrayEintrag::Zeigen(text) => self.zeigen.set_text(text),
                TrayEintrag::Aktualisieren(text) => self.aktualisieren.set_text(text),
                TrayEintrag::Autostart { text, an } => {
                    self.autostart.set_text(text);
                    if self.autostart.is_checked() != *an {
                        self.autostart.set_checked(*an);
                    }
                }
                TrayEintrag::Beenden(text) => self.beenden.set_text(text),
                TrayEintrag::Trenner => {}
            }
        }
    }
}
