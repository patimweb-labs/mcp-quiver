// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der eine offene Dialog. Was er zeigt, folgt aus [`Dialog`] im Modell;
//! welchen Text er trägt, aus den Texten.

use eframe::egui::{self, RichText};
use quiver_domain::Werte;

use crate::ansichten::{agenten, fett, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik::{self, AgentArt};
use crate::modell::{Dialog, Sicht};
use crate::texte::{t, tf};
use crate::thema::{self, Ton};

/// Breite der Dialoge: breit genug für Adresse und Schnipsel, schmal genug
/// für ein kleines Fenster.
const BREITE: f32 = thema::SPACE_8 * 9.0;

/// Höchste Höhe der Feldliste – wie `.felder` in der Verwaltung
/// (`calc(var(--wp-space-8) * 9)`). Mehr Felder scrollen.
const FELDER_HOEHE: f32 = thema::SPACE_8 * 9.0;

/// Platz für Titel, Untertitel, Knöpfe und Rahmen, damit der Dialog auch in
/// einem kleinen Fenster ganz sichtbar bleibt.
const RAHMEN_HOEHE: f32 = thema::SPACE_8 * 5.0;

pub fn zeigen(ctx: &egui::Context, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    if sicht.dialog == Dialog::Keiner {
        // Der nächste Dialog bekommt den Fokus wieder ins erste Feld.
        ctx.data_mut(|d| d.remove::<bool>(egui::Id::unique("dialog-fokus")));
        return;
    }
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let endpunkt = sicht.modell.endpunkt.clone();
    // Schon gekoppelte Namen: sie steuern, welche Arten ausgegraut sind.
    let gekoppelte: Vec<String> = sicht
        .modell
        .agenten
        .iter()
        .map(|a| a.name.clone())
        .collect();
    let breite = BREITE.min(ctx.content_rect().width() - thema::SPACE_8);
    let felder_hoehe = FELDER_HOEHE
        .min(ctx.content_rect().height() - RAHMEN_HOEHE)
        .max(thema::SPACE_8 * 2.0);
    let mut bestaetigen = false;
    let mut abbrechen = false;
    // Werte, die der Dialog nicht zeigt (Standardwerte der Felder). Ein
    // Hilfe-Link darf auch sie als Platzhalter nutzen (Aufgabe 13).
    let basis = match &sicht.dialog {
        Dialog::Werte { eintrag, .. } => sicht
            .modell
            .eintraege
            .iter()
            .find(|e| &e.id == eintrag)
            .map(logik::link_basis)
            .unwrap_or_default(),
        // Die Felder eines Zugangs haben ihre eigenen Standardwerte.
        Dialog::Zugang { eintrag, .. } => sicht
            .modell
            .eintraege
            .iter()
            .find(|e| &e.id == eintrag)
            .and_then(|e| e.zugaenge.as_ref())
            .map(|zugaenge| logik::link_basis_aus_feldern(&zugaenge.felder))
            .unwrap_or_default(),
        _ => Werte::new(),
    };
    // Schon vergebene Zugangsnamen: sie entscheiden, ob ein neuer Name geht.
    let vorhandene_zugaenge: Vec<String> = match &sicht.dialog {
        Dialog::Zugang { eintrag, .. } => sicht
            .modell
            .eintraege
            .iter()
            .find(|e| &e.id == eintrag)
            .and_then(|e| e.zugaenge.as_ref())
            .map(|zugaenge| {
                zugaenge
                    .liste
                    .iter()
                    .map(|zugang| zugang.name.clone())
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    // Innenabstand wie `WpDialog` in der Verwaltung (`--wp-space-5`); der
    // Standardrahmen von egui nimmt nur den Menüabstand (4 px).
    let rahmen = egui::Frame::popup(&ctx.global_style()).inner_margin(thema::SPACE_5 as i8);
    egui::Modal::new(egui::Id::unique("dialog"))
        .frame(rahmen)
        .show(ctx, |ui| {
            ui.set_width(breite);
            match &mut sicht.dialog {
                Dialog::Keiner => {}
                Dialog::Werte {
                    titel, eingaben, ..
                } => {
                    ui.label(RichText::new(tf(sprache, "tools.werte_titel", &[titel])).heading());
                    ui.add(
                        egui::Label::new(text(&f, t(sprache, "tools.werte_sub"), Ton::Leise))
                            .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    egui::ScrollArea::vertical()
                        .id_salt("dialog-werte")
                        .max_height(felder_hoehe)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for i in 0..eingaben.len() {
                                let name = eingaben[i].name.clone();
                                let geheim = eingaben[i].geheim;
                                let gesetzt = eingaben[i].gesetzt;
                                let link = eingaben[i].link.clone();
                                let hinweis = (!eingaben[i].beschreibung.is_empty()).then(|| {
                                    if eingaben[i].pflicht {
                                        format!(
                                            "{} · {}",
                                            eingaben[i].beschreibung,
                                            t(sprache, "tools.pflicht")
                                        )
                                    } else {
                                        eingaben[i].beschreibung.clone()
                                    }
                                });
                                bausteine::feld(ui, &f, &name, hinweis.as_deref(), |ui| {
                                    let mut feld = bausteine::eingabefeld(&mut eingaben[i].wert)
                                        .desired_width(bausteine::feldbreite(ui));
                                    if geheim {
                                        feld = feld.password(true);
                                        if gesetzt {
                                            // Gesetzt heißt: das Feld bleibt leer, der
                                            // Hinweis sagt, dass schon etwas da ist.
                                            feld = feld.hint_text("••••");
                                        }
                                    }
                                    ui.add(feld)
                                });
                                // Hilfe-Link neben dem Feld: nur aktiv, wenn die
                                // **aktuellen** Eingaben eine Adresse ergeben; sonst
                                // gesperrt mit dem Hinweis, was noch fehlt.
                                if let Some(link) = link {
                                    let adresse = logik::link_adresse(&link, eingaben, &basis);
                                    let fehlende = logik::link_fehlende(&link, eingaben, &basis);
                                    ui.add_space(thema::SPACE_1);
                                    let geklickt = ui
                                        .add_enabled_ui(adresse.is_some(), |ui| {
                                            let knopf =
                                                bausteine::knopf(ui, &f, Icon::Extern, &link.text);
                                            if fehlende.is_empty() {
                                                knopf
                                            } else {
                                                knopf.on_disabled_hover_text(logik::link_hinweis(
                                                    &fehlende, sprache,
                                                ))
                                            }
                                        })
                                        .inner
                                        .clicked();
                                    if let (true, Some(url)) = (geklickt, adresse) {
                                        aktionen.push(Aktion::LinkOeffnen(url));
                                    }
                                }
                            }
                        });
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        if bausteine::hauptknopf_mit(
                            ui,
                            &f,
                            Icon::Haken,
                            &t(sprache, "tools.speichern"),
                        )
                        .clicked()
                        {
                            bestaetigen = true;
                        }
                        if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                            .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
                Dialog::Koppeln { art, name } => {
                    ui.label(RichText::new(t(sprache, "agenten.koppeln")).heading());
                    ui.add(
                        egui::Label::new(text(&f, t(sprache, "agenten.koppeln_text"), Ton::Leise))
                            .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    ui.label(text(&f, t(sprache, "agenten.art_titel"), Ton::Leise).small());
                    // Die Art als Liste: schon gekoppelte Arten sind ausgegraut.
                    for zeile in logik::arten_auswahl(&gekoppelte) {
                        let hinweis = zeile
                            .gekoppelt
                            .then(|| t(sprache, "agenten.schon_gekoppelt"));
                        let gewaehlt = art.gleich(&zeile.art);
                        if bausteine::auswahlzeile(
                            ui,
                            &f,
                            &zeile.art.anzeige(sprache),
                            hinweis.as_deref(),
                            gewaehlt,
                            !zeile.gekoppelt,
                        )
                        .clicked()
                        {
                            *art = zeile.art;
                        }
                    }
                    // „Andere" braucht einen Namen; bekannte Arten tragen ihn selbst.
                    let mut eingabe_fertig = false;
                    if !art.ist_fest() {
                        ui.add_space(thema::SPACE_2);
                        let feld =
                            bausteine::feld(ui, &f, &t(sprache, "agenten.name"), None, |ui| {
                                ui.add(
                                    bausteine::eingabefeld(name)
                                        .hint_text("mein-agent")
                                        .desired_width(f32::INFINITY),
                                )
                            });
                        feld.request_focus_once(ui);
                        eingabe_fertig =
                            feld.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    }
                    // Doppelte Namen werden schon vor dem Absenden angezeigt.
                    let einzutragen = art.einzutragen(name);
                    let doppelt = logik::name_vergeben(&einzutragen, &gekoppelte);
                    if doppelt {
                        ui.add_space(thema::SPACE_1);
                        ui.add(
                            egui::Label::new(text(
                                &f,
                                tf(sprache, "agenten.name_doppelt", &[&einzutragen]),
                                Ton::Fehler,
                            ))
                            .wrap(),
                        );
                    }
                    // Enter im Namensfeld bestätigt – aber nur, wenn der Name frei ist.
                    if eingabe_fertig && !einzutragen.is_empty() && !doppelt {
                        bestaetigen = true;
                    }
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        let leer = einzutragen.is_empty();
                        let geklickt = ui
                            .add_enabled_ui(!leer && !doppelt, |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Plus,
                                    &t(sprache, "agenten.koppeln_jetzt"),
                                )
                            })
                            .inner
                            .clicked();
                        if geklickt {
                            bestaetigen = true;
                        }
                        if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                            .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
                Dialog::Token { name, token } => {
                    ui.label(RichText::new(tf(sprache, "agenten.token_titel", &[name])).heading());
                    ui.add(
                        egui::Label::new(text(&f, t(sprache, "agenten.hinweis"), Ton::Warnung))
                            .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    ui.label(text(&f, t(sprache, "agenten.endpunkt"), Ton::Leise).small());
                    bausteine::kopierbar(ui, &f, &endpunkt, &t(sprache, "allg.kopieren"));
                    ui.add_space(thema::SPACE_2);
                    ui.label(text(&f, t(sprache, "agenten.token"), Ton::Leise).small());
                    bausteine::kopierbar(ui, &f, token, &t(sprache, "allg.kopieren"));
                    ui.add_space(thema::SPACE_3);
                    // Der Schnipsel folgt der Art des Agenten.
                    let art = AgentArt::aus_name(name);
                    for schnipsel in agenten::schnipsel(sprache, &art, &endpunkt, token) {
                        ui.label(fett(&schnipsel.ort));
                        bausteine::kopierbar(ui, &f, &schnipsel.text, &t(sprache, "allg.kopieren"));
                        ui.add_space(thema::SPACE_2);
                    }
                    ui.add_space(thema::SPACE_1);
                    if bausteine::hauptknopf(ui, &f, &t(sprache, "allg.schliessen")).clicked() {
                        bestaetigen = true;
                    }
                }
                Dialog::TokenErneuern { name, .. } => {
                    ui.label(RichText::new(t(sprache, "agenten.token_erneuern_titel")).heading());
                    ui.add(
                        egui::Label::new(text(
                            &f,
                            tf(sprache, "agenten.token_erneuern_text", &[name]),
                            Ton::Leise,
                        ))
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        if bausteine::knopf(
                            ui,
                            &f,
                            Icon::Neuladen,
                            &t(sprache, "agenten.token_erneuern"),
                        )
                        .clicked()
                        {
                            bestaetigen = true;
                        }
                        if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                            .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
                Dialog::AgentEntfernen { name, .. } => {
                    ui.label(RichText::new(t(sprache, "agenten.entfernen_titel")).heading());
                    ui.add(
                        egui::Label::new(text(
                            &f,
                            tf(sprache, "agenten.entfernen_text", &[name]),
                            Ton::Leise,
                        ))
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        if bausteine::knopf(
                            ui,
                            &f,
                            Icon::Papierkorb,
                            &t(sprache, "agenten.entfernen"),
                        )
                        .clicked()
                        {
                            bestaetigen = true;
                        }
                        if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                            .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
                // Ein Zugang: Name (beim Bearbeiten fest), die Felder, Geheimes
                // als Passwortfeld mit „gesetzt“ und einem eigenen „Löschen“
                // je gesetztem Geheimnis (Aufgabe 14).
                Dialog::Zugang {
                    eintrag,
                    einzahl,
                    name,
                    neu,
                    eingaben,
                    ..
                } => {
                    let frei = *neu;
                    let zugangsname = name.trim().to_string();
                    ui.label(RichText::new(logik::zugang_titel(sprache, einzahl, frei)).heading());
                    ui.add(
                        egui::Label::new(text(&f, t(sprache, "zugaenge.sub"), Ton::Leise)).wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    let name_fehler = if frei {
                        logik::zugang_name_fehler(name, &vorhandene_zugaenge)
                    } else {
                        None
                    };
                    let name_hinweis = name_fehler
                        .map(|schluessel| t(sprache, schluessel))
                        .or_else(|| Some(t(sprache, "zugaenge.name_hinweis")));
                    bausteine::feld(
                        ui,
                        &f,
                        &t(sprache, "zugaenge.name"),
                        name_hinweis.as_deref(),
                        |ui| {
                            let feld = bausteine::eingabefeld(name)
                                .desired_width(bausteine::feldbreite(ui));
                            // Beim Bearbeiten bleibt der Name, wie er ist.
                            ui.add(if frei { feld } else { feld.interactive(false) })
                        },
                    );
                    ui.add_space(thema::SPACE_2);
                    egui::ScrollArea::vertical()
                        .id_salt("dialog-zugang")
                        .max_height(felder_hoehe)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for i in 0..eingaben.len() {
                                let feldname = eingaben[i].name.clone();
                                let geheim = eingaben[i].geheim;
                                let gesetzt = eingaben[i].gesetzt;
                                let link = eingaben[i].link.clone();
                                let mut teile = Vec::new();
                                if !eingaben[i].beschreibung.is_empty() {
                                    teile.push(eingaben[i].beschreibung.clone());
                                }
                                if eingaben[i].pflicht {
                                    teile.push(t(sprache, "tools.pflicht"));
                                }
                                if geheim && gesetzt {
                                    teile.push(t(sprache, "zugaenge.geheim_gesetzt"));
                                }
                                let hinweis = (!teile.is_empty()).then(|| teile.join(" · "));
                                bausteine::feld(ui, &f, &feldname, hinweis.as_deref(), |ui| {
                                    let mut feld = bausteine::eingabefeld(&mut eingaben[i].wert)
                                        .desired_width(bausteine::feldbreite(ui));
                                    if geheim {
                                        feld = feld.password(true);
                                        if gesetzt {
                                            // Gesetzt heißt: leer lassen behält es.
                                            feld = feld.hint_text("••••");
                                        }
                                    }
                                    ui.add(feld)
                                });
                                // Ein gesetztes Geheimnis lässt sich einzeln entfernen –
                                // etwa das Anmelde-Passwort, nachdem der Schlüssel steht.
                                // Den Knopf gibt es nur, wo ein Geheimnis gesetzt ist –
                                // die Kurzschluss-Auswertung zeichnet ihn auch nur dann.
                                if geheim
                                    && gesetzt
                                    && bausteine::knopf(
                                        ui,
                                        &f,
                                        Icon::Papierkorb,
                                        &t(sprache, "zugaenge.geheim_loeschen"),
                                    )
                                    .clicked()
                                {
                                    eingaben[i].gesetzt = false;
                                    aktionen.push(Aktion::ZugangGeheimLoeschen {
                                        eintrag: eintrag.clone(),
                                        name: zugangsname.clone(),
                                        feld: feldname.clone(),
                                    });
                                }
                                // Hilfe-Link neben dem Feld wie im Angaben-Dialog.
                                if let Some(link) = link {
                                    let adresse = logik::link_adresse(&link, eingaben, &basis);
                                    let fehlende = logik::link_fehlende(&link, eingaben, &basis);
                                    ui.add_space(thema::SPACE_1);
                                    let geklickt = ui
                                        .add_enabled_ui(adresse.is_some(), |ui| {
                                            let knopf =
                                                bausteine::knopf(ui, &f, Icon::Extern, &link.text);
                                            if fehlende.is_empty() {
                                                knopf
                                            } else {
                                                knopf.on_disabled_hover_text(logik::link_hinweis(
                                                    &fehlende, sprache,
                                                ))
                                            }
                                        })
                                        .inner
                                        .clicked();
                                    if let (true, Some(url)) = (geklickt, adresse) {
                                        aktionen.push(Aktion::LinkOeffnen(url));
                                    }
                                }
                            }
                        });
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        let geklickt = ui
                            .add_enabled_ui(name_fehler.is_none(), |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Haken,
                                    &t(sprache, "tools.speichern"),
                                )
                            })
                            .inner
                            .clicked();
                        if geklickt {
                            bestaetigen = true;
                        }
                        if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                            .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
                Dialog::ZugangEntfernen { name, einzahl, .. } => {
                    ui.label(
                        RichText::new(tf(sprache, "zugaenge.entfernen_titel", &[einzahl]))
                            .heading(),
                    );
                    ui.add(
                        egui::Label::new(text(
                            &f,
                            tf(sprache, "zugaenge.entfernen_text", &[name]),
                            Ton::Leise,
                        ))
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        if bausteine::knopf(
                            ui,
                            &f,
                            Icon::Papierkorb,
                            &t(sprache, "zugaenge.loeschen"),
                        )
                        .clicked()
                        {
                            bestaetigen = true;
                        }
                        if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                            .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
                Dialog::AnmeldungBeenden { titel, .. } => {
                    ui.label(RichText::new(t(sprache, "anmeldung.beenden_titel")).heading());
                    ui.add(
                        egui::Label::new(text(
                            &f,
                            tf(sprache, "anmeldung.beenden_text", &[titel]),
                            Ton::Leise,
                        ))
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        if bausteine::knopf(
                            ui,
                            &f,
                            Icon::Trennen,
                            &t(sprache, "anmeldung.abmelden"),
                        )
                        .clicked()
                        {
                            bestaetigen = true;
                        }
                        if bausteine::knopf(ui, &f, Icon::Schliessen, &t(sprache, "allg.abbrechen"))
                            .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
            }
            // Escape schließt jeden Dialog.
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                abbrechen = true;
            }
        });

    if bestaetigen {
        aktionen.extend(sicht.dialog_bestaetigen());
    } else if abbrechen {
        sicht.dialog = Dialog::Keiner;
    }
}

/// Fokus beim ersten Bild des Dialogs ins erste Feld.
trait FokusEinmal {
    fn request_focus_once(&self, ui: &egui::Ui);
}

impl FokusEinmal for egui::Response {
    fn request_focus_once(&self, ui: &egui::Ui) {
        let id = egui::Id::unique("dialog-fokus");
        let schon = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
        if !schon {
            self.request_focus();
            ui.ctx().data_mut(|d| d.insert_temp(id, true));
        }
    }
}
