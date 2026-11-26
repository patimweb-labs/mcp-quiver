// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Modell des Fensters: der Stand des Kerns ([`quiver_client::Modell`])
//! plus das, was nur die Oberfläche betrifft — offene Seite, offener Dialog,
//! Eingaben, Theme und Sprache. Ein anderer Zustand ist nicht erlaubt
//! (AGENTS.md §2.1); die Ansichten lesen ihn und legen Aktionen ab.

use std::collections::BTreeSet;
use std::path::PathBuf;

use quiver_client::protokoll::Zeitraum;
use quiver_client::{Aufruf, EintragSicht, Modell, Vorschau, Zeile, Zusammenfassung};
use quiver_domain::{EintragId, FeldArt, FeldLink};

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
    Protokoll,
}

impl Seite {
    pub const ALLE: [Seite; 4] = [
        Seite::Verbindung,
        Seite::Tools,
        Seite::Agenten,
        Seite::Protokoll,
    ];

    /// Name für die Bildschirmfotos und die Einstellungsdatei.
    pub fn name(self) -> &'static str {
        match self {
            Seite::Verbindung => "verbindung",
            Seite::Tools => "tools",
            Seite::Agenten => "agenten",
            Seite::Protokoll => "protokoll",
        }
    }

    /// Schlüssel des Anzeigenamens in `texte.json`.
    pub fn text_schluessel(self) -> &'static str {
        match self {
            Seite::Verbindung => "nav.verbindung",
            Seite::Tools => "nav.tools",
            Seite::Agenten => "nav.agenten",
            Seite::Protokoll => "nav.protokoll",
        }
    }

    /// Zeichen der Seite in der Navigation.
    pub fn icon(self) -> Icon {
        match self {
            Seite::Verbindung => Icon::Stecker,
            Seite::Tools => Icon::Paket,
            Seite::Agenten => Icon::Agent,
            Seite::Protokoll => Icon::Protokoll,
        }
    }
}

/// Welcher Reiter auf der Protokollseite offen ist (Aufgabe 17).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProtokollReiter {
    #[default]
    Aufrufe,
    ToolLogs,
}

/// Was die Filterleiste der Protokollseite einstellt. Leere Namen heißen
/// „alle“.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtokollEingaben {
    pub eintrag: String,
    pub tool: String,
    pub agent: String,
    pub nur_fehler: bool,
    pub zeitraum: Zeitraum,
    /// Freitext im Reiter „Tool-Logs“.
    pub suche: String,
}

impl Default for ProtokollEingaben {
    fn default() -> Self {
        Self {
            eintrag: String::new(),
            tool: String::new(),
            agent: String::new(),
            nur_fehler: false,
            zeitraum: Zeitraum::Stunde,
            suche: String::new(),
        }
    }
}

/// Ein Feld des Dialogs „Angaben": Beschriftung, Inhalt, und ob es ein
/// Geheimnis ist (dann wird es als Passwort eingegeben und nie angezeigt).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feldeingabe {
    pub name: String,
    /// Beschriftung im Dialog: `titel`, sonst der Name.
    pub anzeige: String,
    pub beschreibung: String,
    pub geheim: bool,
    pub pflicht: bool,
    /// Schon ein Geheimnis hinterlegt? Dann steht „••••" als Hinweis im Feld.
    pub gesetzt: bool,
    /// Was im Feld steht. Leer heißt bei Geheimnissen „unverändert".
    pub wert: String,
    /// Hilfe-Link neben dem Feld (Aufgabe 13), z. B. „Token in Outline
    /// erstellen". Er darf die Eingaben des Dialogs als Platzhalter nutzen.
    pub link: Option<FeldLink>,
    /// Wie das Feld dargestellt wird (Aufgabe 16). Bei Geheimnissen steht
    /// hier immer [`FeldArt::Passwort`].
    pub art: FeldArt,
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
    /// Die Anmeldung eines Eintrags beenden (Rückfrage).
    AnmeldungBeenden { eintrag: EintragId, titel: String },
    /// Das Gerät vom Server abmelden (Rückfrage, Aufgabe 22): Agenten
    /// verlieren alle Tools, Angaben und Adresse bleiben.
    Abmelden {
        server: String,
        benutzer: Option<String>,
    },
    /// Einen Zugang anlegen (`neu`) oder ändern (Aufgabe 14). Beim Ändern ist
    /// der Name fest; die Felder stehen als Eingaben im Dialog.
    Zugang {
        eintrag: EintragId,
        titel: String,
        einzahl: String,
        name: String,
        neu: bool,
        eingaben: Vec<Feldeingabe>,
    },
    /// Einen Zugang samt seiner Geheimnisse entfernen (Rückfrage).
    ZugangEntfernen {
        eintrag: EintragId,
        name: String,
        einzahl: String,
    },
    /// Die Einstellungen sichern (Aufgabe 15). `geheimnisse` ist der Schalter
    /// „Geheimnisse mitnehmen“; nur dann gibt es ein Passwort.
    Export {
        pfad: PathBuf,
        geheimnisse: bool,
        passwort: String,
        wiederholung: String,
    },
    /// Eine Sicherung einspielen (Aufgabe 15): Vorschau, Passwort (nur wenn die
    /// Datei Geheimnisse führt) und „Übernehmen“.
    Import {
        pfad: PathBuf,
        vorschau: Vorschau,
        passwort: String,
    },
    /// Ein Aufruf im Detail (Aufgabe 17): Argumente, Fehlertext, Vorschau.
    Aufruf { aufruf: Aufruf },
    /// Das Protokoll leeren — mit Rückfrage (Aufgabe 17).
    ProtokollLeeren,
    /// Der Tresor ist nicht lesbar: mit einem leeren beginnen (Paket 3).
    /// Die alten Dateien bleiben als `.kaputt-<zeit>` liegen.
    TresorNeuBeginnen,
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
    /// Eine Sicherung ist geprüft: der Import-Dialog zeigt die Vorschau.
    EinstellungenVorschau { pfad: PathBuf, vorschau: Vorschau },
    /// Export oder Import sind durch: der Hinweis oben sagt, was geschah.
    EinstellungenFertig {
        importiert: bool,
        zusammenfassung: Zusammenfassung,
    },
    /// Das Protokoll ist geleert (Aufgabe 17): Speicher und Dateien.
    ProtokollGeleert,
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
    /// Aufgeklappte Zugangslisten (je Eintrag, Aufgabe 14).
    pub zugaenge_offen: BTreeSet<EintragId>,
    /// Zeigt die Seite Tools nur die aktiven Einträge (Aufgabe 19)?
    pub nur_aktive: bool,
    pub eingaben: Eingaben,
    /// Was der Kern zuletzt gemeldet hat (Fehler), oben auf jeder Seite.
    pub hinweis: Option<String>,
    /// Was die letzte Aktion ergeben hat („6 Einträge exportiert“), oben auf
    /// jeder Seite (Aufgabe 15).
    pub ergebnis: Option<String>,
    /// Ein Pfad für den nächsten Datei-Dialog. Im Betrieb leer; im Test steht
    /// hier die Datei, damit kein nativer Dialog aufgeht (Aufgabe 15).
    pub datei: Option<PathBuf>,
    /// Startet quiver beim Anmelden? Der Wert kommt aus dem System
    /// ([`crate::autostart::aktiv`]) und wird dorthin zurückgeschrieben.
    pub autostart: bool,
    /// Die Aufrufe, die der Filter gerade zulässt (Aufgabe 17) — gefüllt aus
    /// [`crate::app::KernZugang::protokoll`], nie aus einer eigenen Quelle.
    pub aufrufe: Vec<Aufruf>,
    /// Die Prozesszeilen des gewählten Eintrags.
    pub log_zeilen: Vec<Zeile>,
    pub protokoll_eingaben: ProtokollEingaben,
    pub protokoll_reiter: ProtokollReiter,
    /// Welcher Eintrag im Reiter „Tool-Logs“ gewählt ist.
    pub log_eintrag: Option<EintragId>,
    /// Stand des Protokollzählers, zu dem `aufrufe` gehört.
    pub protokoll_stand: u64,
    /// Filter, zu dem `aufrufe` gehört.
    pub protokoll_gefiltert: Option<quiver_client::protokoll::Filter>,
    /// Eintrag und Filter, zu denen `log_zeilen` gehören.
    pub log_gefiltert: Option<(Option<EintragId>, quiver_client::protokoll::LogFilter)>,
    /// Stand des Protokollzählers, zu dem `log_zeilen` gehören.
    pub log_stand: u64,
    /// Hat der User in der Liste gescrollt? Dann bleiben die Zeilen stehen,
    /// statt unter ihm wegzurutschen.
    pub protokoll_gescrollt: bool,
}

impl Sicht {
    pub fn neu(modell: Modell, einstellungen: &Einstellungen) -> Self {
        // Eine vorgegebene Server-Adresse (Aufgabe 34) gilt immer.
        let server = modell
            .vorgaben
            .server_url
            .clone()
            .or_else(|| modell.server_url.clone())
            .or_else(|| modell.verbindung.server().map(str::to_string))
            .unwrap_or_default();
        // Was schon beim Start schiefsteht (Tresor, gespeicherter Stand),
        // steht als Hinweis oben (Paket 3) – noch bevor sich das Modell ändert.
        let hinweis = modell
            .tresor_fehler
            .as_ref()
            .map(|fehler| {
                format!(
                    "{} {}",
                    crate::texte::t(einstellungen.sprache(), "tresor.nicht_lesbar"),
                    fehler
                )
            })
            .or_else(|| modell.profil_hinweis.clone());
        Self {
            seite: logik::startseite(&modell),
            modell,
            dialog: Dialog::Keiner,
            thema: einstellungen.thema(),
            sprache: einstellungen.sprache(),
            offen: BTreeSet::new(),
            zugaenge_offen: BTreeSet::new(),
            nur_aktive: false,
            eingaben: Eingaben {
                server,
                code: String::new(),
            },
            hinweis,
            ergebnis: None,
            datei: None,
            autostart: false,
            aufrufe: Vec::new(),
            log_zeilen: Vec::new(),
            protokoll_eingaben: ProtokollEingaben::default(),
            protokoll_reiter: ProtokollReiter::Aufrufe,
            log_eintrag: None,
            protokoll_stand: u64::MAX,
            protokoll_gefiltert: None,
            log_gefiltert: None,
            log_stand: u64::MAX,
            protokoll_gescrollt: false,
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
            Meldung::EinstellungenVorschau { pfad, vorschau } => {
                self.dialog = Dialog::Import {
                    pfad,
                    vorschau,
                    passwort: String::new(),
                };
            }
            Meldung::EinstellungenFertig {
                importiert,
                zusammenfassung,
            } => {
                self.ergebnis = Some(logik::ergebnis_text(
                    self.sprache,
                    importiert,
                    &zusammenfassung,
                ));
            }
            Meldung::ProtokollGeleert => {
                self.ergebnis = Some(crate::texte::t(self.sprache, "protokoll.geleert"));
                // Ein leerer Speicher heißt: nichts mehr zu zeigen.
                self.aufrufe.clear();
                self.log_zeilen.clear();
                self.protokoll_gefiltert = None;
                self.log_gefiltert = None;
                self.log_stand = u64::MAX;
                self.protokoll_gescrollt = false;
            }
            Meldung::Fehler(text) => self.hinweis = Some(text),
        }
    }

    /// Den Pfad für eine Sicherung wählen: im Test steht er in der Sicht
    /// ([`Sicht::datei`]), im Betrieb kommt er aus dem Datei-Dialog.
    /// `speichern` öffnet den Speichern- statt des Öffnen-Dialogs.
    pub fn datei_waehlen(&mut self, speichern: bool) -> Option<PathBuf> {
        if let Some(pfad) = self.datei.take() {
            return Some(pfad);
        }
        let titel = crate::texte::t(
            self.sprache,
            if speichern {
                "einstellungen.export_titel"
            } else {
                "einstellungen.import_titel"
            },
        );
        crate::datei::waehlen(
            speichern,
            &titel,
            &quiver_client::einstellungen::dateiname(),
        )
    }

    /// Den Ordner für das Feld `index` des offenen Dialogs wählen. Abbrechen
    /// lässt den bisherigen Wert stehen. Im Test steht der Pfad in der Sicht.
    pub fn ordner_waehlen(&mut self, index: usize) {
        let vorgabe = self.datei.take();
        let titel = crate::texte::t(self.sprache, "tools.ordner_titel");
        let (Dialog::Werte { eingaben, .. } | Dialog::Zugang { eingaben, .. }) = &mut self.dialog
        else {
            return;
        };
        let Some(eingabe) = eingaben.get_mut(index) else {
            return;
        };
        let pfad = vorgabe.or_else(|| crate::datei::ordner_waehlen(&titel, &eingabe.wert));
        if let Some(pfad) = pfad {
            eingabe.wert = pfad.display().to_string();
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
        // Die zuletzt benutzte Adresse vorbelegen — auch nach dem Abmelden.
        if self.eingaben.server.trim().is_empty() {
            if let Some(server) = &neu.server_url {
                self.eingaben.server = server.clone();
            }
        }
        // Der Server hat das Gerät abgelehnt: das sagt der Hinweis oben.
        if neu.abgelehnt && !self.modell.abgelehnt {
            self.hinweis = Some(crate::texte::t(self.sprache, "verbindung.abgelehnt"));
        }
        // Der Tresor ist nicht lesbar (Paket 3): das Fenster sagt es, und der
        // Knopf „Neu beginnen“ steht daneben. Nichts wird überschrieben.
        if neu.tresor_fehler.is_some() && self.modell.tresor_fehler != neu.tresor_fehler {
            self.hinweis = Some(format!(
                "{} {}",
                crate::texte::t(self.sprache, "tresor.nicht_lesbar"),
                neu.tresor_fehler.clone().unwrap_or_default()
            ));
        }
        // Der gespeicherte Stand war nicht lesbar (Paket 3) – einmal sagen.
        if let Some(text) = &neu.profil_hinweis {
            if self.modell.profil_hinweis.as_ref() != Some(text) {
                self.hinweis = Some(text.clone());
            }
        }
        // Was der Server nicht mehr kennt, hat keinen Dialog mehr verdient.
        let verschwunden = match &self.dialog {
            Dialog::Werte { eintrag, .. } => !neu.eintraege.iter().any(|e| &e.id == eintrag),
            Dialog::AnmeldungBeenden { eintrag, .. } => {
                !neu.eintraege.iter().any(|e| &e.id == eintrag)
            }
            Dialog::Zugang { eintrag, .. } | Dialog::ZugangEntfernen { eintrag, .. } => {
                !neu.eintraege.iter().any(|e| &e.id == eintrag)
            }
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

    /// Zugangsliste eines Eintrags ein- oder ausklappen (Aufgabe 14).
    pub fn zugaenge_umschalten(&mut self, id: &EintragId) {
        if !self.zugaenge_offen.remove(id) {
            self.zugaenge_offen.insert(id.clone());
        }
    }

    /// Den Zugangs-Dialog öffnen: `name: None` legt einen neuen an, `Some`
    /// bearbeitet einen vorhandenen (der Name bleibt dann fest).
    pub fn zugang_dialog(&mut self, eintrag: &EintragSicht, name: Option<&str>) {
        let Some(zugaenge) = &eintrag.zugaenge else {
            return;
        };
        self.dialog = Dialog::Zugang {
            eintrag: eintrag.id.clone(),
            titel: eintrag.titel.clone(),
            einzahl: zugaenge.einzahl.clone(),
            name: name.unwrap_or_default().to_string(),
            neu: name.is_none(),
            eingaben: logik::zugang_eingaben(zugaenge, name),
        };
    }

    /// Der bestätigte Dialog als Aktion; schließt den Dialog.
    pub fn dialog_bestaetigen(&mut self) -> Option<Aktion> {
        let dialog = std::mem::take(&mut self.dialog);
        match dialog {
            Dialog::Keiner => {}
            Dialog::Werte {
                eintrag, eingaben, ..
            } => {
                // Der Knopf ist schon gesperrt; hier steht die Prüfung noch
                // einmal, damit nichts Halbes hinausgeht (Aufgabe 16).
                if logik::sperrgrund(self.sprache, &eingaben).is_some() {
                    return None;
                }
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
            Dialog::AnmeldungBeenden { eintrag, .. } => {
                return Some(Aktion::EintragAbmelden { eintrag });
            }
            Dialog::Abmelden { .. } => return Some(Aktion::Abmelden),
            Dialog::Zugang {
                eintrag,
                name,
                eingaben,
                ..
            } => {
                let name = name.trim().to_string();
                if name.is_empty() || logik::sperrgrund(self.sprache, &eingaben).is_some() {
                    return None;
                }
                let (lokal, geheim) = logik::werte_splitten(&eingaben);
                return Some(Aktion::ZugangSpeichern {
                    eintrag,
                    name,
                    lokal,
                    geheim,
                });
            }
            Dialog::ZugangEntfernen { eintrag, name, .. } => {
                return Some(Aktion::ZugangLoeschen { eintrag, name })
            }
            Dialog::Export {
                pfad,
                geheimnisse,
                passwort,
                wiederholung,
            } => {
                // Der Knopf ist gesperrt, solange etwas fehlt; hier steht die
                // Prüfung noch einmal, damit nichts Halbes hinausgeht.
                if geheimnisse && logik::export_passwort_fehler(&passwort, &wiederholung).is_some()
                {
                    return None;
                }
                return Some(Aktion::EinstellungenExportieren {
                    pfad,
                    passwort: geheimnisse.then(|| passwort.trim().to_string()),
                });
            }
            Dialog::Import {
                pfad,
                vorschau,
                passwort,
            } => {
                if vorschau.hat_geheimnisse && logik::import_passwort_fehler(&passwort).is_some() {
                    return None;
                }
                return Some(Aktion::EinstellungenImportieren {
                    pfad,
                    passwort: vorschau
                        .hat_geheimnisse
                        .then(|| passwort.trim().to_string()),
                });
            }
            Dialog::Aufruf { .. } => {}
            Dialog::ProtokollLeeren => return Some(Aktion::ProtokollLeeren),
            Dialog::TresorNeuBeginnen => return Some(Aktion::TresorNeuBeginnen),
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_client::{Verbindung, Vorschau};

    fn leeres_modell() -> Modell {
        Modell {
            verbindung: Verbindung::NichtEingebunden,
            eintraege: Vec::new(),
            agenten: Vec::new(),
            endpunkt: "http://127.0.0.1:8741/mcp".into(),
            abgelehnt: false,
            anmelde_grund: None,
            benutzer: None,
            server_url: None,
            anmeldewege: None,
            protokoll_fuer_agenten: false,
            tresor_fehler: None,
            profil_hinweis: None,
            update: quiver_client::UpdateLage::Unbekannt,
            vorgaben: quiver_client::Vorgaben::default(),
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
    fn ein_abgelehntes_geraet_bekommt_einen_hinweis() {
        let mut sicht = Sicht::neu(leeres_modell(), &Einstellungen::default());
        let mut neu = leeres_modell();
        neu.abgelehnt = true;
        sicht.modell_setzen(neu.clone());
        let hinweis = sicht.hinweis.clone().expect("Hinweis");
        assert!(hinweis.contains("anmelden"), "{hinweis}");
        // Bleibt es abgelehnt, kommt der Hinweis nicht ständig neu.
        sicht.hinweis = None;
        neu.endpunkt = "http://127.0.0.1:9/mcp".into();
        sicht.modell_setzen(neu);
        assert_eq!(sicht.hinweis, None);
    }

    #[test]
    fn export_und_import_bestaetigen_nur_vollstaendig() {
        let pfad = PathBuf::from("/tmp/quiver-einstellungen.json");
        let mut sicht = Sicht::neu(leeres_modell(), &Einstellungen::default());

        // Ohne Geheimnisse braucht der Export kein Passwort.
        sicht.dialog = Dialog::Export {
            pfad: pfad.clone(),
            geheimnisse: false,
            passwort: String::new(),
            wiederholung: String::new(),
        };
        assert_eq!(
            sicht.dialog_bestaetigen(),
            Some(Aktion::EinstellungenExportieren {
                pfad: pfad.clone(),
                passwort: None,
            })
        );
        assert_eq!(sicht.dialog, Dialog::Keiner);

        // Mit Geheimnissen, aber ohne Passwort: nichts geht hinaus.
        sicht.dialog = Dialog::Export {
            pfad: pfad.clone(),
            geheimnisse: true,
            passwort: String::new(),
            wiederholung: String::new(),
        };
        assert_eq!(sicht.dialog_bestaetigen(), None);

        // Eine offene Sicherung wird ohne Passwort übernommen.
        sicht.dialog = Dialog::Import {
            pfad: pfad.clone(),
            vorschau: Vorschau::default(),
            passwort: "egal".into(),
        };
        assert_eq!(
            sicht.dialog_bestaetigen(),
            Some(Aktion::EinstellungenImportieren {
                pfad: pfad.clone(),
                passwort: None,
            })
        );

        // Führt die Datei Geheimnisse, muss das Passwort lang genug sein.
        sicht.dialog = Dialog::Import {
            pfad: pfad.clone(),
            vorschau: Vorschau {
                eintraege: Vec::new(),
                hat_geheimnisse: true,
            },
            passwort: "kurz".into(),
        };
        assert_eq!(sicht.dialog_bestaetigen(), None);
        sicht.dialog = Dialog::Import {
            pfad,
            vorschau: Vorschau {
                eintraege: Vec::new(),
                hat_geheimnisse: true,
            },
            passwort: "ein-langes-passwort".into(),
        };
        assert_eq!(
            sicht.dialog_bestaetigen(),
            Some(Aktion::EinstellungenImportieren {
                pfad: PathBuf::from("/tmp/quiver-einstellungen.json"),
                passwort: Some("ein-langes-passwort".into()),
            })
        );
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

    /// Ein kaputter gespeicherter Stand und ein unlesbarer Tresor stehen als
    /// Hinweis oben – einmal, nicht bei jedem Bild neu (Paket 3).
    #[test]
    fn tresor_und_profilfehler_stehen_als_hinweis() {
        let mut neu = leeres_modell();
        neu.tresor_fehler = Some("tresor.json ist beschädigt".into());
        let mut sicht = Sicht::neu(neu.clone(), &Einstellungen::default());
        assert_eq!(
            sicht.hinweis.as_deref(),
            Some("Tresor nicht lesbar: tresor.json ist beschädigt"),
            "der Hinweis steht schon beim Start"
        );

        // Ein neuer Stand mit demselben Fehler erzeugt keinen neuen Hinweis.
        sicht.hinweis = Some("etwas anderes".into());
        sicht.modell_setzen(neu);
        assert_eq!(sicht.hinweis.as_deref(), Some("etwas anderes"));

        // Ein kaputter gespeicherter Stand wird gemeldet.
        let mut neu = leeres_modell();
        neu.profil_hinweis = Some(
            "Der gespeicherte Stand war nicht lesbar und liegt als zustand.json.kaputt-… daneben"
                .into(),
        );
        sicht.modell_setzen(neu);
        assert!(sicht
            .hinweis
            .as_deref()
            .unwrap_or_default()
            .contains("nicht lesbar"));
    }
}
