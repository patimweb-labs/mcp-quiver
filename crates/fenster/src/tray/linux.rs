// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Linux: StatusNotifierItem über `ksni` (reines Rust, D-Bus, kein GTK).
//!
//! Das Menü entsteht bei jeder Auffrischung neu aus [`TrayZustand::menu`] —
//! `ksni` fragt es bei `update` erneut ab. Damit steht im Menü genau das, was
//! das Modell sagt.

use ksni::blocking::{Handle, TrayMethods};
use ksni::menu::{CheckmarkItem, StandardItem};
use ksni::{Icon, MenuItem, ToolTip};

use super::symbol::{self, KANTEN};
use super::{Befehl, TrayZustand};
use crate::logik::TrayEintrag;

struct Symbol {
    z: TrayZustand,
    icon: Vec<Icon>,
}

impl ksni::Tray for Symbol {
    fn id(&self) -> String {
        "mcp-quiver".into()
    }

    fn title(&self) -> String {
        "mcp-quiver".into()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        self.icon.clone()
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "mcp-quiver".into(),
            // Die Zustandszeile steht auch im Tooltip — das ist die erste
            // Zeile des Menüs, dieselbe Quelle.
            description: self
                .z
                .menu()
                .iter()
                .find_map(|eintrag| match eintrag {
                    TrayEintrag::Zustand(text) => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_default(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        self.z.senden(Befehl::Zeigen);
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        self.z
            .menu()
            .iter()
            .map(|eintrag| match eintrag.clone() {
                TrayEintrag::Zustand(text) => StandardItem {
                    label: text,
                    // Eine Zeile zum Lesen, nicht zum Anklicken.
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                TrayEintrag::Zeigen(text) => StandardItem {
                    label: text,
                    activate: Box::new(|s: &mut Symbol| s.z.senden(Befehl::Zeigen)),
                    ..Default::default()
                }
                .into(),
                TrayEintrag::Autostart { text, an } => CheckmarkItem {
                    label: text,
                    checked: an,
                    activate: Box::new(|s: &mut Symbol| s.z.senden(Befehl::AutostartUmschalten)),
                    ..Default::default()
                }
                .into(),
                TrayEintrag::Trenner => MenuItem::Separator,
                TrayEintrag::Beenden(text) => StandardItem {
                    label: text,
                    activate: Box::new(|s: &mut Symbol| s.z.senden(Befehl::Beenden)),
                    ..Default::default()
                }
                .into(),
            })
            .collect()
    }
}

pub struct Tray {
    handle: Handle<Symbol>,
}

impl Tray {
    /// Symbol anmelden. `None`, wenn kein StatusNotifierWatcher antwortet.
    pub fn starten(z: TrayZustand) -> Option<Self> {
        let icon = KANTEN
            .iter()
            .filter_map(|&kante| {
                let rgba = symbol::raster(kante)
                    .map_err(|fehler| tracing::warn!("Tray-Symbol: {fehler}"))
                    .ok()?;
                Some(Icon {
                    width: kante as i32,
                    height: kante as i32,
                    data: argb(&rgba),
                })
            })
            .collect();
        match (Symbol { z, icon }).spawn() {
            Ok(handle) => Some(Tray { handle }),
            Err(fehler) => {
                tracing::warn!("kein Tray: {fehler}");
                None
            }
        }
    }

    /// Die Einträge setzen, die das Menü zeigt (abgeleitet aus dem Modell).
    pub fn setzen(&self, eintraege: Vec<TrayEintrag>) {
        self.z.setzen(eintraege);
    }

    /// Menü und Tooltip neu lesen (die Anwendung ruft es jedes Bild).
    pub fn auffrischen(&self) {
        self.handle.update(|_| {});
    }
}

/// RGBA → ARGB32 in Netzwerk-Bytefolge, wie StatusNotifierItem es will.
fn argb(rgba: &[u8]) -> Vec<u8> {
    rgba.as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[3], pixel[0], pixel[1], pixel[2]])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_in_netzwerkfolge() {
        assert_eq!(
            argb(&[1, 2, 3, 4, 5, 6, 7, 8]),
            vec![4, 1, 2, 3, 8, 5, 6, 7]
        );
    }
}
