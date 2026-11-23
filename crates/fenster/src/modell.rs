// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Modell des Fensters: der Stand des Kerns ([`quiver_client::Modell`])
//! plus das, was nur die Oberfläche betrifft — offene Seite, offener Dialog,
//! Eingaben, Theme und Sprache. Ein anderer Zustand ist nicht erlaubt
//! (AGENTS.md §2.1); die Ansichten lesen ihn und legen Aktionen ab.

use std::collections::BTreeSet;

use quiver_client::{EintragSicht, Modell};
use quiver_domain::EintragId;

use crate::app::Aktion;
use crate::bausteine::Icon;
use crate::logik::{self, AgentArt};
use crate::profil::Einstellungen;
use crate::texte::Sprache;
use crate::thema::Thema;

/// Welche Seite offen ist.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Seite {
    Verbindung,
    #[default]
    Tools,
    Agenten,
}

impl Seite {
    pub const ALLE: [Seite; 3] = [Seite::Verbindung, Seite::Tools, Seite::Agenten];

    /// Name für die Bildschirmfotos und die Einstellungsdatei.
    pub fn name(self) -> &'static str {
        match self {
            Seite::Verbindung => "verbindung",
            Seite::Tools => "tools",
            Seite::Agenten => "agenten",
        }
    }

    /// Schlüssel des Anzeigenamens in `texte.json`.
    pub fn text_schluessel(self) -> &'static str {
        match self {
            Seite::Verbindung => "nav.verbindung",
            Seite::Tools => "nav.tools",
            Seite::Agenten => "nav.agenten",
        }
    }

    /// Zeichen der Seite in der Navigation.
    pub fn icon(self) -> Icon {
        match self {
            Seite::Verbindung => Icon::Stecker,
            Seite::Tools => Icon::Paket,
            Seite::Agenten => Icon::Agent,
        }
    }
}

/// Ein Feld des Dialogs „Angaben": Beschriftung, Inhalt, und ob es ein
/// Geheimnis ist (dann wird es als Passwort eingegeben und nie angezeigt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feldeingabe {
    pub name: String,
    pub beschreibung: String,
    pub geheim: bool,
    pub pflicht: bool,
    /// Schon ein Geheimnis hinterlegt? Dann steht „••••" als Hinweis im Feld.
    pub gesetzt: bool,
    /// Was im Feld steht. Leer heißt bei Geheimnissen „unverändert".
    pub wert: String,
}

/// Der offene Dialog — höchstens einer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Dialog {
    #[default]
    Keiner,
    /// Angaben eines Eintrags (Client- und Geheim-Felder).
    Werte {
        eintrag: EintragId,
        titel: String,
        eingaben: Vec<Feldeingabe>,
    },
    /// Art wählen und koppeln; `name` gilt nur für „Andere".
    Koppeln { art: AgentArt, name: String },
    /// Der Agent ist gekoppelt: Endpunkt, Token und die Schnipsel.
    Token { name: String, token: String },
    /// Neues Token für einen Agenten (Rückfrage).
    TokenErneuern { id: String, name: String },
    /// Einem Agenten den Zugang nehmen.
    AgentEntfernen { id: String, name: String },
}

/// Was in den Eingabefeldern der Verbindungsseite steht.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Eingaben {
    pub server: String,
    pub code: String,
}

/// Rückmeldung des Kerns, die nicht im Modell steht (Fehler, frisches Token).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Meldung {
    /// Ein Agent wurde gekoppelt; das Token ist **einmal** sichtbar.
    Token { name: String, token: String },
    /// Etwas ist schiefgegangen — der Text steht über der Seite.
    Fehler(String),
}

/// Alles, was das Fenster zeigt und was es sich merkt.
#[derive(Debug, Clone)]
pub struct Sicht {
    /// Stand des Kerns — die einzige Wahrheit über Verbindung, Einträge, Agenten.
    pub modell: Modell,
    pub seite: Seite,
    pub dialog: Dialog,
    pub thema: Thema,
    pub sprache: Sprache,
    /// Aufgeklappte Werkzeuglisten (je Eintrag).
    pub offen: BTreeSet<EintragId>,
    pub eingaben: Eingaben,
    /// Was der Kern zuletzt gemeldet hat (Fehler), oben auf jeder Seite.
    pub hinweis: Option<String>,
    /// Startet quiver beim Anmelden? Der Wert kommt aus dem System
    /// ([`crate::autostart::aktiv`]) und wird dorthin zurückgeschrieben.
    pub autostart: bool,
}

impl Sicht {
    pub fn neu(modell: Modell, einstellungen: &Einstellungen) -> Self {
        Self {
            seite: logik::startseite(&modell),
            modell,
            dialog: Dialog::Keiner,
            thema: einstellungen.thema(),
            sprache: einstellungen.sprache(),
            offen: BTreeSet::new(),
            eingaben: Eingaben::default(),
            hinweis: None,
            autostart: false,
        }
    }

    /// Was in der Einstellungsdatei landet.
    pub fn einstellungen(&self) -> Einstellungen {
        Einstellungen::neu(self.thema, self.sprache)
    }

    /// Eine Meldung des Kerns verarbeiten — reine Funktion über dem Modell.
    pub fn melden(&mut self, meldung: Meldung) {
        match meldung {
            Meldung::Token { name, token } => self.dialog = Dialog::Token { name, token },
            Meldung::Fehler(text) => self.hinweis = Some(text),
        }
    }

    /// Ein neues Modell des Kerns übernehmen. Wechselt die Verbindung, folgt
    /// die Seite daraus (Startseite, siehe [`logik::seite_nach_wechsel`]).
    pub fn modell_setzen(&mut self, neu: Modell) {
        if self.modell == neu {
            return;
        }
        self.seite =
            logik::seite_nach_wechsel(&self.modell.verbindung, &neu.verbindung, self.seite);
        // Was der Server nicht mehr kennt, hat keinen Dialog mehr verdient.
        let verschwunden = match &self.dialog {
            Dialog::Werte { eintrag, .. } => !neu.eintraege.iter().any(|e| &e.id == eintrag),
            _ => false,
        };
        if verschwunden {
            self.dialog = Dialog::Keiner;
        }
        self.modell = neu;
    }

    /// Den Angaben-Dialog für diesen Eintrag öffnen.
    pub fn werte_dialog(&mut self, eintrag: &EintragSicht) {
        self.dialog = Dialog::Werte {
            eintrag: eintrag.id.clone(),
            titel: eintrag.titel.clone(),
            eingaben: logik::eingaben(eintrag),
        };
    }

    /// Werkzeugliste eines Eintrags ein- oder ausklappen.
    pub fn umschalten(&mut self, id: &EintragId) {
        if !self.offen.remove(id) {
            self.offen.insert(id.clone());
        }
    }

    /// Der bestätigte Dialog als Aktion; schließt den Dialog.
    pub fn dialog_bestaetigen(&mut self) -> Option<Aktion> {
        let dialog = std::mem::take(&mut self.dialog);
        match dialog {
            Dialog::Keiner => {}
            Dialog::Werte {
                eintrag, eingaben, ..
            } => {
                let (lokal, geheim) = logik::werte_splitten(&eingaben);
                return Some(Aktion::WerteSetzen {
                    eintrag,
                    lokal,
                    geheim,
                });
            }
            Dialog::Koppeln { art, name } => {
                let name = art.einzutragen(&name);
                if name.is_empty() {
                    return None;
                }
                return Some(Aktion::AgentKoppeln { name });
            }
            Dialog::Token { .. } => {}
            Dialog::TokenErneuern { id, .. } => return Some(Aktion::AgentTokenErneuern { id }),
            Dialog::AgentEntfernen { id, .. } => return Some(Aktion::AgentEntfernen { id }),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_client::Verbindung;

    fn leeres_modell() -> Modell {
        Modell {
            verbindung: Verbindung::NichtEingebunden,
            eintraege: Vec::new(),
            agenten: Vec::new(),
            endpunkt: "http://127.0.0.1:8741/mcp".into(),
        }
    }

    #[test]
    fn seiten_namen_und_schluessel_sind_eindeutig() {
        let mut namen: Vec<&str> = Seite::ALLE.iter().map(|s| s.name()).collect();
        namen.dedup();
        assert_eq!(namen.len(), Seite::ALLE.len());
        for seite in Seite::ALLE {
            let text = crate::texte::t(Sprache::De, seite.text_schluessel());
            assert_ne!(text, seite.text_schluessel(), "{} ohne Text", seite.name());
        }
    }

    #[test]
    fn unbekannter_dialog_bestaetigt_nichts() {
        let mut sicht = Sicht::neu(leeres_modell(), &Einstellungen::default());
        assert_eq!(sicht.dialog_bestaetigen(), None);
        sicht.dialog = Dialog::Koppeln {
            art: AgentArt::Andere(String::new()),
            name: String::new(),
        };
        assert_eq!(
            sicht.dialog_bestaetigen(),
            None,
            "leerer Name, kein Koppeln"
        );
        assert_eq!(sicht.dialog, Dialog::Keiner);
        // Eine feste Art trägt ihren Namen selbst.
        sicht.dialog = Dialog::Koppeln {
            art: AgentArt::Hermes,
            name: String::new(),
        };
        assert_eq!(
            sicht.dialog_bestaetigen(),
            Some(Aktion::AgentKoppeln {
                name: "hermes".into()
            })
        );
    }
}
