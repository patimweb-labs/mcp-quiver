// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der eine offene Dialog. Was er zeigt, folgt aus [`Dialog`] im Modell;
//! welchen Text er trägt, aus den Texten.

use eframe::egui::{self, RichText};
use quiver_domain::{FeldArt, Werte};

use crate::ansichten::{agenten, fett, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik::{self, AgentArt};
use crate::modell::{Dialog, Sicht};
use crate::texte::{t, tf};
use crate::thema::{self, Ton};

/// Breite der Dialoge: breit genug für Adresse und Schnipsel, schmal genug
/// für ein kleines Fenster. Derselbe Wert wie in der Verwaltung
/// (`size.dialog`).
const BREITE: f32 = thema::SIZE_DIALOG;

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
    // Für welches Feld der Ordner-Dialog aufgeht – erst nach dem Zeichnen.
    let mut ordner_fuer: Option<usize> = None;
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
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .max_height(felder_hoehe)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for i in 0..eingaben.len() {
                                let anzeige = eingaben[i].anzeige.clone();
                                let gesetzt = eingaben[i].gesetzt;
                                let link = eingaben[i].link.clone();
                                let art = eingaben[i].art.clone();
                                let schalter = art == FeldArt::Schalter;
                                let hinweis = (!eingaben[i].beschreibung.is_empty()).then(|| {
                                    if eingaben[i].pflicht && !schalter {
                                        format!(
                                            "{} · {}",
                                            eingaben[i].beschreibung,
                                            t(sprache, "tools.pflicht")
                                        )
                                    } else {
                                        eingaben[i].beschreibung.clone()
                                    }
                                });
                                // Ein Schalter ist eine Checkbox mit dem Titel
                                // daneben, keine Formularzeile (Aufgabe 16).
                                if schalter {
                                    let mut an = logik::schalter_an(&eingaben[i].wert);
                                    ui.add_space(thema::SPACE_1);
                                    if bausteine::haken(ui, &mut an, &anzeige).changed() {
                                        eingaben[i].wert =
                                            if an { "an".into() } else { "aus".into() };
                                    }
                                    if let Some(hinweis) = &hinweis {
                                        ui.add(
                                            egui::Label::new(text(&f, hinweis, Ton::Blass).small())
                                                .wrap(),
                                        );
                                    }
                                    ui.add_space(thema::SPACE_1);
                                } else {
                                    bausteine::feld(ui, &f, &anzeige, hinweis.as_deref(), |ui| {
                                        match &art {
                                            FeldArt::Auswahl { optionen } => {
                                                let liste: Vec<(String, String)> = optionen
                                                    .iter()
                                                    .map(|option| {
                                                        (
                                                            option.wert.clone(),
                                                            option.anzeige.clone(),
                                                        )
                                                    })
                                                    .collect();
                                                bausteine::auswahl(
                                                    ui,
                                                    &f,
                                                    &mut eingaben[i].wert,
                                                    &liste,
                                                    &t(sprache, "tools.auswaehlen"),
                                                )
                                            }
                                            FeldArt::Ordner => ordnerfeld(
                                                ui,
                                                &f,
                                                &mut eingaben[i].wert,
                                                &t(sprache, "tools.ordner_waehlen"),
                                                || ordner_fuer = Some(i),
                                            ),
                                            _ => {
                                                let mut feld =
                                                    bausteine::eingabefeld(&mut eingaben[i].wert)
                                                        .desired_width(bausteine::feldbreite(ui));
                                                if art == FeldArt::Passwort {
                                                    feld = feld.password(true);
                                                    if gesetzt {
                                                        // Gesetzt heißt: das Feld bleibt leer,
                                                        // der Hinweis sagt, dass schon etwas da ist.
                                                        feld = feld.hint_text("••••");
                                                    }
                                                }
                                                let antwort = ui.add(feld);
                                                // Eine Zahl nimmt nur Ziffern an; die
                                                // Prüfung bleibt beim Speichern.
                                                if art == FeldArt::Zahl && antwort.changed() {
                                                    eingaben[i]
                                                        .wert
                                                        .retain(|zeichen| zeichen.is_ascii_digit());
                                                }
                                                antwort
                                            }
                                        }
                                    });
                                }
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
                    let grund = logik::sperrgrund(sprache, eingaben);
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        let klick = ui
                            .add_enabled_ui(grund.is_none(), |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Haken,
                                    &t(sprache, "tools.speichern"),
                                )
                            })
                            .inner
                            .clicked();
                        if klick {
                            bestaetigen = true;
                        }
                        if let Some(grund) = &grund {
                            ui.add(egui::Label::new(text(&f, grund, Ton::Fehler)).wrap());
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
                    // Unter dem Feld steht immer die Regel; was gerade nicht
                    // passt, steht als Fehler neben „Speichern“.
                    let name_hinweis = Some(t(sprache, "zugaenge.name_hinweis"));
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
                        .scroll_bar_visibility(
                            egui::scroll_area::ScrollBarVisibility::AlwaysVisible,
                        )
                        .max_height(felder_hoehe)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            for i in 0..eingaben.len() {
                                let feldname = eingaben[i].name.clone();
                                let anzeige = eingaben[i].anzeige.clone();
                                let geheim = eingaben[i].geheim;
                                let gesetzt = eingaben[i].gesetzt;
                                let link = eingaben[i].link.clone();
                                let art = eingaben[i].art.clone();
                                let schalter = art == FeldArt::Schalter;
                                let mut teile = Vec::new();
                                if !eingaben[i].beschreibung.is_empty() {
                                    teile.push(eingaben[i].beschreibung.clone());
                                }
                                if eingaben[i].pflicht && !schalter {
                                    teile.push(t(sprache, "tools.pflicht"));
                                }
                                if geheim && gesetzt {
                                    teile.push(t(sprache, "zugaenge.geheim_gesetzt"));
                                }
                                let hinweis = (!teile.is_empty()).then(|| teile.join(" · "));
                                if schalter {
                                    let mut an = logik::schalter_an(&eingaben[i].wert);
                                    ui.add_space(thema::SPACE_1);
                                    if bausteine::haken(ui, &mut an, &anzeige).changed() {
                                        eingaben[i].wert =
                                            if an { "an".into() } else { "aus".into() };
                                    }
                                    if let Some(hinweis) = &hinweis {
                                        ui.add(
                                            egui::Label::new(text(&f, hinweis, Ton::Blass).small())
                                                .wrap(),
                                        );
                                    }
                                    ui.add_space(thema::SPACE_1);
                                } else {
                                    bausteine::feld(ui, &f, &anzeige, hinweis.as_deref(), |ui| {
                                        match &art {
                                            FeldArt::Auswahl { optionen } => {
                                                let liste: Vec<(String, String)> = optionen
                                                    .iter()
                                                    .map(|option| {
                                                        (
                                                            option.wert.clone(),
                                                            option.anzeige.clone(),
                                                        )
                                                    })
                                                    .collect();
                                                bausteine::auswahl(
                                                    ui,
                                                    &f,
                                                    &mut eingaben[i].wert,
                                                    &liste,
                                                    &t(sprache, "tools.auswaehlen"),
                                                )
                                            }
                                            FeldArt::Ordner => ordnerfeld(
                                                ui,
                                                &f,
                                                &mut eingaben[i].wert,
                                                &t(sprache, "tools.ordner_waehlen"),
                                                || ordner_fuer = Some(i),
                                            ),
                                            _ => {
                                                let mut feld =
                                                    bausteine::eingabefeld(&mut eingaben[i].wert)
                                                        .desired_width(bausteine::feldbreite(ui));
                                                if art == FeldArt::Passwort {
                                                    feld = feld.password(true);
                                                    if gesetzt {
                                                        // Gesetzt heißt: leer lassen behält es.
                                                        feld = feld.hint_text("••••");
                                                    }
                                                }
                                                let antwort = ui.add(feld);
                                                // Eine Zahl nimmt nur Ziffern an; die
                                                // Prüfung bleibt beim Speichern.
                                                if art == FeldArt::Zahl && antwort.changed() {
                                                    eingaben[i]
                                                        .wert
                                                        .retain(|zeichen| zeichen.is_ascii_digit());
                                                }
                                                antwort
                                            }
                                        }
                                    });
                                }
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
                    // Auch ein ungültiger Name ist ein Grund – er steht neben
                    // „Speichern“, nicht nur grau unter dem Namensfeld.
                    let grund = name_fehler
                        .map(|schluessel| t(sprache, schluessel))
                        .or_else(|| logik::sperrgrund(sprache, eingaben));
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        let bereit = grund.is_none();
                        let geklickt = ui
                            .add_enabled_ui(bereit, |ui| {
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
                        if let Some(grund) = &grund {
                            ui.add(egui::Label::new(text(&f, grund, Ton::Fehler)).wrap());
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
                // Die Einstellungen sichern (Aufgabe 15): erst der Schalter
                // „Geheimnisse mitnehmen“, dann – nur dann – zwei
                // Passwortfelder und die Prüfung auf Gleichheit und Länge.
                Dialog::Export {
                    pfad,
                    geheimnisse,
                    passwort,
                    wiederholung,
                } => {
                    ui.label(RichText::new(t(sprache, "einstellungen.export_titel")).heading());
                    ui.add(
                        egui::Label::new(text(
                            &f,
                            t(sprache, "einstellungen.export_text"),
                            Ton::Leise,
                        ))
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_2);
                    ui.add(
                        egui::Label::new(
                            text(
                                &f,
                                tf(sprache, "einstellungen.datei", &[&pfad.display()]),
                                Ton::Blass,
                            )
                            .small(),
                        )
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    bausteine::haken(
                        ui,
                        geheimnisse,
                        &t(sprache, "einstellungen.mit_geheimnissen_schalter"),
                    );
                    let mut fehler = None;
                    if *geheimnisse {
                        ui.add_space(thema::SPACE_2);
                        let hinweis = t(sprache, "einstellungen.passwort_hinweis");
                        bausteine::feld(
                            ui,
                            &f,
                            &t(sprache, "einstellungen.passwort"),
                            Some(&hinweis),
                            |ui| {
                                ui.add(
                                    bausteine::eingabefeld(passwort)
                                        .password(true)
                                        .desired_width(bausteine::feldbreite(ui)),
                                )
                            },
                        );
                        bausteine::feld(
                            ui,
                            &f,
                            &t(sprache, "einstellungen.passwort_wiederholen"),
                            None,
                            |ui| {
                                ui.add(
                                    bausteine::eingabefeld(wiederholung)
                                        .password(true)
                                        .desired_width(bausteine::feldbreite(ui)),
                                )
                            },
                        );
                        fehler = logik::export_passwort_fehler(passwort, wiederholung)
                            .map(|schluessel| t(sprache, schluessel));
                    }
                    if let Some(fehler) = &fehler {
                        ui.add_space(thema::SPACE_1);
                        ui.add(egui::Label::new(text(&f, fehler, Ton::Fehler)).wrap());
                    }
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        let geklickt = ui
                            .add_enabled_ui(fehler.is_none(), |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Herunterladen,
                                    &t(sprache, "einstellungen.sichern"),
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
                // Eine Sicherung einspielen (Aufgabe 15): die Vorschau sagt je
                // Eintrag, was neu ist, was sich ändert und was übersprungen
                // wird; ein Passwort gibt es nur, wenn die Datei Geheimnisse
                // führt.
                Dialog::Import {
                    pfad,
                    vorschau,
                    passwort,
                } => {
                    ui.label(RichText::new(t(sprache, "einstellungen.import_titel")).heading());
                    ui.add(
                        egui::Label::new(text(
                            &f,
                            t(sprache, "einstellungen.import_text"),
                            Ton::Leise,
                        ))
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_2);
                    ui.add(
                        egui::Label::new(
                            text(
                                &f,
                                tf(sprache, "einstellungen.datei", &[&pfad.display()]),
                                Ton::Blass,
                            )
                            .small(),
                        )
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_2);
                    ui.add(
                        egui::Label::new(
                            text(
                                &f,
                                tf(
                                    sprache,
                                    "einstellungen.vorschau_summe",
                                    &[&vorschau.uebernommen(), &vorschau.uebersprungen()],
                                ),
                                Ton::Leise,
                            )
                            .small(),
                        )
                        .wrap(),
                    );
                    ui.add_space(thema::SPACE_2);
                    // Die Liste scrollt, wenn die Datei viele Einträge hat.
                    egui::ScrollArea::vertical()
                        .id_salt("dialog-import")
                        .max_height(felder_hoehe)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            if vorschau.eintraege.is_empty() {
                                ui.add(
                                    egui::Label::new(text(
                                        &f,
                                        t(sprache, "einstellungen.vorschau_leer"),
                                        Ton::Blass,
                                    ))
                                    .wrap(),
                                );
                            }
                            for eintrag in &vorschau.eintraege {
                                let ton = match eintrag.art {
                                    quiver_client::VorschauArt::Uebersprungen => Ton::Blass,
                                    quiver_client::VorschauArt::Neu => Ton::Gut,
                                    _ => Ton::Warnung,
                                };
                                let detail = logik::vorschau_detail(sprache, eintrag);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(&eintrag.titel).strong());
                                    ui.add(
                                        egui::Label::new(text(&f, detail, ton).small())
                                            .selectable(false),
                                    );
                                });
                            }
                        });
                    let mut fehler = None;
                    if vorschau.hat_geheimnisse {
                        ui.add_space(thema::SPACE_2);
                        let hinweis = t(sprache, "einstellungen.passwort_datei");
                        bausteine::feld(
                            ui,
                            &f,
                            &t(sprache, "einstellungen.passwort"),
                            Some(&hinweis),
                            |ui| {
                                ui.add(
                                    bausteine::eingabefeld(passwort)
                                        .password(true)
                                        .desired_width(bausteine::feldbreite(ui)),
                                )
                            },
                        );
                        fehler = logik::import_passwort_fehler(passwort)
                            .map(|schluessel| t(sprache, schluessel));
                    }
                    if let Some(fehler) = &fehler {
                        ui.add_space(thema::SPACE_1);
                        ui.add(egui::Label::new(text(&f, fehler, Ton::Fehler)).wrap());
                    }
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        let bereit = fehler.is_none() && !vorschau.eintraege.is_empty();
                        let geklickt = ui
                            .add_enabled_ui(bereit, |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Haken,
                                    &t(sprache, "einstellungen.uebernehmen"),
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
                Dialog::Aufruf { aufruf } => {
                    aufruf_details(ui, &f, sprache, aufruf, felder_hoehe);
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        if bausteine::knopf(ui, &f, Icon::Kopieren, &t(sprache, "allg.kopieren"))
                            .clicked()
                        {
                            ui.ctx().copy_text(logik::aufruf_text(sprache, aufruf));
                        }
                        if bausteine::knopf(
                            ui,
                            &f,
                            Icon::Schliessen,
                            &t(sprache, "allg.schliessen"),
                        )
                        .clicked()
                        {
                            abbrechen = true;
                        }
                    });
                }
                Dialog::ProtokollLeeren => {
                    ui.label(RichText::new(t(sprache, "protokoll.leeren_titel")).heading());
                    ui.add(
                        egui::Label::new(text(&f, t(sprache, "protokoll.leeren_text"), Ton::Leise))
                            .wrap(),
                    );
                    ui.add_space(thema::SPACE_3);
                    ui.horizontal(|ui| {
                        if bausteine::knopf(
                            ui,
                            &f,
                            Icon::Papierkorb,
                            &t(sprache, "protokoll.leeren"),
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

    if let Some(index) = ordner_fuer {
        sicht.ordner_waehlen(index);
    }
    if bestaetigen {
        aktionen.extend(sicht.dialog_bestaetigen());
    } else if abbrechen {
        sicht.dialog = Dialog::Keiner;
    }
}

/// Die Einzelheiten eines Aufrufs (Aufgabe 17): Zeit, Agent, Eintrag,
/// Werkzeug, Dauer und Ergebnis; darunter Argumente, Fehlertext und die
/// Vorschau der Antwort. Alles ist schon maskiert und gekürzt.
fn aufruf_details(
    ui: &mut egui::Ui,
    f: &crate::thema::Farben,
    sprache: crate::texte::Sprache,
    aufruf: &quiver_client::Aufruf,
    hoehe: f32,
) {
    ui.label(RichText::new(t(sprache, "protokoll.detail_titel")).heading());
    ui.add(
        egui::Label::new(text(
            f,
            format!(
                "{} · {} · {}",
                aufruf.eintrag,
                aufruf.tool,
                logik::status_text(sprache, aufruf.status)
            ),
            Ton::Leise,
        ))
        .wrap(),
    );
    ui.add_space(thema::SPACE_3);
    let zeilen = vec![
        (
            t(sprache, "protokoll.spalte.zeit"),
            aufruf.zeit.clone(),
            true,
        ),
        (
            t(sprache, "protokoll.spalte.status"),
            logik::status_text(sprache, aufruf.status),
            false,
        ),
        (
            t(sprache, "protokoll.spalte.agent"),
            aufruf.agent.clone(),
            false,
        ),
        (
            t(sprache, "protokoll.spalte.dauer"),
            logik::dauer_text(sprache, aufruf.dauer_ms),
            false,
        ),
        (
            t(sprache, "protokoll.antwort_groesse"),
            tf(sprache, "protokoll.zeichen", &[&aufruf.antwort_groesse]),
            false,
        ),
    ];
    bausteine::fakten(ui, f, &zeilen);
    egui::ScrollArea::vertical()
        .id_salt("dialog-aufruf")
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .max_height(hoehe)
        .auto_shrink([false, true])
        .show(ui, |ui| {
            // Drei Blöcke, jeder in Monoschrift, mit Rahmen und eigenem
            // Knopf „Kopieren“ (Aufgabe 18).
            if !aufruf.argumente.is_empty() {
                abschnitt(
                    ui,
                    f,
                    sprache,
                    "protokoll.argumente",
                    &aufruf.argumente,
                    false,
                );
            }
            if let Some(fehler) = &aufruf.fehler {
                abschnitt(ui, f, sprache, "protokoll.fehlertext", fehler, true);
            }
            if !aufruf.vorschau.is_empty() {
                abschnitt(
                    ui,
                    f,
                    sprache,
                    "protokoll.vorschau",
                    &aufruf.vorschau,
                    false,
                );
            }
        });
}

/// Ein Abschnitt im Detail: Überschrift und Kopier-Knopf, darunter der Inhalt
/// als Block in Monoschrift mit Rahmen. `fehler` färbt ihn als Fehlertext.
fn abschnitt(
    ui: &mut egui::Ui,
    f: &crate::thema::Farben,
    sprache: crate::texte::Sprache,
    schluessel: &str,
    inhalt: &str,
    fehler: bool,
) {
    ui.add_space(thema::SPACE_3);
    egui::Sides::new().shrink_left().truncate().show(
        ui,
        |ui| {
            ui.label(text(f, t(sprache, schluessel), Ton::Leise).small());
        },
        |ui| {
            if bausteine::icon_knopf(ui, f, Icon::Kopieren, &t(sprache, "allg.kopieren"), false)
                .clicked()
            {
                ui.ctx().copy_text(inhalt.to_string());
            }
        },
    );
    ui.add_space(thema::SPACE_1);
    egui::Frame::new()
        .fill(f.bg_input)
        .stroke(egui::Stroke::new(thema::SIZE_BORDER, f.border_subtle))
        .inner_margin(egui::Margin::same(thema::SPACE_2 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut wert = RichText::new(inhalt).font(egui::FontId::monospace(thema::FONT_SIZE_SM));
            if fehler {
                wert = wert.color(f.status_error);
            }
            ui.add(egui::Label::new(wert).wrap());
        });
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

/// Ein Ordner: der Pfad und daneben der Knopf, der den Ordner-Dialog öffnet.
/// Der Pfad bleibt bearbeitbar, etwa für `~` oder zum Einfügen.
fn ordnerfeld(
    ui: &mut egui::Ui,
    f: &thema::Farben,
    wert: &mut String,
    knopf: &str,
    waehlen: impl FnOnce(),
) -> egui::Response {
    // Rechts bleibt Platz für den Scrollbalken des Dialogs.
    let breite = bausteine::feldbreite(ui).min(ui.available_width() - thema::SPACE_3);
    let hoehe = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(
        egui::vec2(breite, hoehe),
        egui::Layout::right_to_left(egui::Align::Center),
        |ui| {
            if bausteine::knopf(ui, f, Icon::Ordner, knopf).clicked() {
                waehlen();
            }
            ui.add(bausteine::eingabefeld(wert).desired_width(f32::INFINITY))
        },
    )
    .inner
}
