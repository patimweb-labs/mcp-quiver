// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tray-Symbol: Fenster öffnen, Autostart schalten, beenden — auf allen drei
//! Plattformen dieselbe Liste.
//!
//! - Linux: StatusNotifierItem über `ksni` (KDE, GNOME mit AppIndicator, Xfce …)
//! - Windows / macOS: Infobereich bzw. Menüleiste über `tray-icon`
//!
//! Das Menü ist eine Ableitung aus dem Modell ([`crate::logik::tray_menu`]):
//! [`TrayZustand`] hält die Einträge, die Plattform-Teile zeichnen sie nur.
//! Was das Fenster tun soll (zeigen, beenden), geht als [`Befehl`] an die
//! Anwendung in [`crate::app`]; damit funktioniert es auch, wenn das Fenster
//! gerade weggelegt ist.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::logik::TrayEintrag;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::Tray;

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod native;
#[cfg(any(target_os = "windows", target_os = "macos"))]
pub use native::Tray;

/// Andere Plattformen (BSD …): kein Tray; das Fenster bleibt der einzige Weg.
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub struct Tray;

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
impl Tray {
    pub fn starten(_: TrayZustand) -> Option<Self> {
        None
    }

    pub fn setzen(&self, _: Vec<TrayEintrag>) {}

    pub fn auffrischen(&self) {}
}

pub mod symbol;

/// Wie ein Fenster „weggelegt" wird. Unter Wayland lässt sich ein Fenster
/// nicht verstecken (und nicht wieder hervorholen), dort wird es minimiert;
/// Windows und macOS legen es beiseite — und brauchen für ihr Tray ohnehin
/// eine laufende Ereignisschleife, die es ohne Fenster nicht gäbe.
pub const MINIMIEREN: bool = cfg!(target_os = "linux");

/// Was das Fenster tun soll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Befehl {
    /// Fenster zeigen und in den Vordergrund holen.
    Zeigen,
    /// Kern sauber anhalten und das Programm beenden.
    Beenden,
    /// Autostart ein- oder ausschalten.
    AutostartUmschalten,
}

/// Was das Tray-Menü zeigt und wohin seine Klicks gehen. Geteilt zwischen
/// Tray, Anwendung und der Anforderung einer zweiten Instanz.
#[derive(Clone)]
pub struct TrayZustand {
    /// Die Einträge, abgeleitet aus dem Modell (siehe `logik::tray_menu`).
    menu: Arc<Mutex<Vec<TrayEintrag>>>,
    /// Wohin Klicks gehen.
    befehle: Sender<Befehl>,
    /// Womit sich das Fenster wecken lässt (auch wenn es versteckt ist).
    wecker: Arc<Mutex<Option<egui::Context>>>,
}

impl TrayZustand {
    pub fn neu(befehle: Sender<Befehl>) -> Self {
        Self {
            menu: Arc::default(),
            befehle,
            wecker: Arc::default(),
        }
    }

    /// Das Fenster wecken: nur dann läuft `App::logic`, wenn nichts zu sehen ist.
    pub fn wecken_mit(&self, ctx: egui::Context) {
        *self.wecker.lock().unwrap_or_else(|e| e.into_inner()) = Some(ctx);
    }

    /// Die Einträge setzen — nur, wenn sie sich geändert haben.
    pub fn setzen(&self, eintraege: Vec<TrayEintrag>) {
        let mut menu = self.menu.lock().unwrap_or_else(|e| e.into_inner());
        if *menu != eintraege {
            *menu = eintraege;
        }
    }

    pub fn menu(&self) -> Vec<TrayEintrag> {
        self.menu.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Einen Befehl abgeben und das Fenster wecken, damit es ihn ausführt.
    pub fn senden(&self, befehl: Befehl) {
        let _ = self.befehle.send(befehl);
        self.wecken();
    }

    fn wecken(&self) {
        if let Some(ctx) = self
            .wecker
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            ctx.request_repaint();
        }
    }
}

/// Das Fenster zeigen und nach vorn holen.
pub fn zeigen(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

/// Das Fenster weglegen — der Kern läuft weiter (AGENTS.md: Hintergrundbetrieb).
pub fn weglegen(ctx: &egui::Context) {
    if MINIMIEREN {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    } else {
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }
}
