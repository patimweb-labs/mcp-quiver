// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Tools: eine Karte je Eintrag — Titel, Beschreibung, Zustand als Abzeichen,
//! Angaben bearbeiten, Werkzeugliste aufklappen. Leer heißt leer.

use eframe::egui::{self, RichText};

use crate::ansichten::{abstand, ergebnis, hinweis, leer, text};
use crate::app::Aktion;
use crate::bausteine::{self, Icon};
use crate::logik;
use crate::modell::{Dialog, Sicht};
use crate::texte::{t, tf};
use crate::thema::{self, Ton};
use quiver_client::EintragSicht;

pub fn zeigen(ui: &mut egui::Ui, sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    let mut exportieren = false;
    let mut importieren = false;
    bausteine::seite(ui, |ui| {
        hinweis(ui, sicht, aktionen);
        ergebnis(ui, sicht);
        kopfzeile(ui, sicht, &mut exportieren, &mut importieren, aktionen);
        fortschritt(ui, sicht);
        if sicht.modell.eintraege.is_empty() {
            leer(
                ui,
                sicht,
                &t(sicht.sprache, "tools.leer"),
                &t(sicht.sprache, "tools.leer_text"),
            );
            return;
        }
        // Filter „nur aktive“ (Aufgabe 19): abgeschaltete Karten verschwinden.
        // Die Zeile gibt es nur, wenn es etwas zu filtern gibt – sonst nähme sie
        // den Karten Platz weg (Aufgabe 18).
        if sicht
            .modell
            .eintraege
            .iter()
            .any(|eintrag| eintrag.am_geraet_aus)
        {
            bausteine::leiste(ui, |ui| {
                bausteine::haken(
                    ui,
                    &mut sicht.nur_aktive,
                    &t(sicht.sprache, "tools.nur_aktive"),
                );
            });
            ui.add_space(thema::SPACE_4);
        }
        let sichtbar = logik::sichtbare_eintraege(&sicht.modell.eintraege, sicht.nur_aktive);
        if sichtbar.is_empty() {
            leer(
                ui,
                sicht,
                &t(sicht.sprache, "tools.leer_aus"),
                &t(sicht.sprache, "tools.abgeschaltet_hinweis"),
            );
            return;
        }
        for eintrag in sichtbar {
            karte(ui, sicht, &eintrag, aktionen);
            abstand(ui);
        }
    });
    if exportieren {
        // Der Pfad kommt zuerst: erst dann fragt der Dialog nach dem Passwort.
        if let Some(pfad) = sicht.datei_waehlen(true) {
            sicht.dialog = Dialog::Export {
                pfad,
                geheimnisse: false,
                passwort: String::new(),
                wiederholung: String::new(),
            };
        }
    }
    if importieren {
        if let Some(pfad) = sicht.datei_waehlen(false) {
            // Der Kern liest und prüft; die Vorschau kommt als Meldung zurück
            // und öffnet den Dialog ([`crate::modell::Sicht::melden`]).
            aktionen.push(Aktion::EinstellungenPruefen { pfad });
        }
    }
}

/// Die Kopfzeile der Seite: Titel und Unterzeile links, „Exportieren“ und
/// „Importieren“ rechts (Aufgabe 15). Ist das Fenster zu schmal für beides
/// nebeneinander, stehen die Knöpfe darunter (Aufgabe 18): die Unterzeile
/// wird dann nicht abgeschnitten.
fn kopfzeile(
    ui: &mut egui::Ui,
    sicht: &mut Sicht,
    exportieren: &mut bool,
    importieren: &mut bool,
    aktionen: &mut Vec<Aktion>,
) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let export = t(sprache, "einstellungen.exportieren");
    let import = t(sprache, "einstellungen.importieren");
    let aktualisieren = t(sprache, "tools.aktualisieren");
    let knoepfe = bausteine::knopfbreite(ui, &export)
        + bausteine::knopfbreite(ui, &import)
        + bausteine::knopfbreite(ui, &aktualisieren);
    // So viel Platz braucht der Titel, damit er lesbar bleibt.
    let titel = thema::SPACE_8 * 5.0;
    let nebeneinander = ui.available_width() >= titel + knoepfe + thema::SPACE_3;
    if nebeneinander {
        ui.add_space(thema::SPACE_4);
        egui::Sides::new().shrink_left().truncate().show(
            ui,
            |ui| titelkopf(ui, &f, sprache),
            |ui| {
                // Die rechte Seite füllt egui von rechts: erst „Importieren“,
                // damit links „Tools aktualisieren“ steht.
                if bausteine::knopf(ui, &f, Icon::Hochladen, &import).clicked() {
                    *importieren = true;
                }
                if bausteine::knopf(ui, &f, Icon::Herunterladen, &export).clicked() {
                    *exportieren = true;
                }
                // Nachtrag zu Aufgabe 37: sofort beim Server nachfragen.
                // Gesperrt, solange etwas läuft – dann gibt es nichts zu stoßen.
                let laeuft = logik::fortschritt_dreht(&sicht.modell.fortschritt);
                ui.add_enabled_ui(!laeuft, |ui| {
                    if bausteine::knopf(ui, &f, Icon::Neuladen, &aktualisieren).clicked() {
                        aktualisieren_anfordern(sicht, aktionen);
                    }
                });
            },
        );
    } else {
        bausteine::seitenkopf(ui, &f, &t(sprache, "nav.tools"), &t(sprache, "tools.sub"));
        bausteine::leiste(ui, |ui| {
            if bausteine::knopf(ui, &f, Icon::Herunterladen, &export).clicked() {
                *exportieren = true;
            }
            if bausteine::knopf(ui, &f, Icon::Hochladen, &import).clicked() {
                *importieren = true;
            }
            let laeuft = logik::fortschritt_dreht(&sicht.modell.fortschritt);
            ui.add_enabled_ui(!laeuft, |ui| {
                if bausteine::knopf(ui, &f, Icon::Neuladen, &aktualisieren).clicked() {
                    aktualisieren_anfordern(sicht, aktionen);
                }
            });
        });
    }
    ui.add_space(thema::SPACE_5);
}

/// „Tools aktualisieren“: ohne Verbindung sagt das Fenster warum, statt eine
/// Anfrage zu schicken, die ins Leere ginge (Nachtrag zu Aufgabe 37).
fn aktualisieren_anfordern(sicht: &mut Sicht, aktionen: &mut Vec<Aktion>) {
    if matches!(
        sicht.modell.verbindung,
        quiver_client::Verbindung::Verbunden { .. }
    ) {
        aktionen.push(Aktion::ToolsAktualisieren);
    } else {
        sicht.hinweis = Some(t(sicht.sprache, "tools.ohne_verbindung"));
    }
}

/// Die Anzeige am Kopf: was gerade getan wird („Verbinde mit dem Server …“,
/// „Tools werden geladen (3 von 12)“, „Wird gestartet …“), am Ende kurz
/// „Alles aktuell“ (Nachtrag zu Aufgabe 37).
///
/// Sie steht nur, wenn es etwas zu tun gibt: unter 300 ms erscheint nichts,
/// „Alles aktuell“ verschwindet nach vier Sekunden. Solange ein Ring dreht,
/// zeichnet das Fenster weiter; sonst bleibt es ruhig (kein Dauerbild).
fn fortschritt(ui: &mut egui::Ui, sicht: &mut Sicht) {
    let (fortschritt, alter) = sicht.fortschritt_jetzt();
    let Some(text) = logik::fortschritt_text(sicht.sprache, &fortschritt, alter) else {
        return;
    };
    let f = *sicht.thema.farben();
    // „Weniger Bewegung“ im System: dann kein drehender Ring, nur der Text.
    let dreht = logik::fortschritt_dreht(&fortschritt) && ui.style().animation_time > 0.0;
    bausteine::kasten(ui, &f, |ui| {
        if dreht {
            bausteine::status_dreht(ui, &f, Ton::Leise, &text);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(100));
        } else {
            bausteine::status(ui, &f, Ton::Leise, &text);
            let rest = logik::KURZ.saturating_sub(alter);
            if !logik::fortschritt_dreht(&fortschritt) && !rest.is_zero() {
                ui.ctx().request_repaint_after(rest);
            }
        }
    });
    ui.add_space(thema::SPACE_4);
}

/// Titel und Unterzeile der Seite.
fn titelkopf(ui: &mut egui::Ui, f: &crate::thema::Farben, sprache: crate::texte::Sprache) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = thema::SPACE_1;
        ui.label(
            RichText::new(t(sprache, "nav.tools"))
                .font(egui::FontId::new(thema::FONT_SIZE_2XL, thema::fett())),
        );
        ui.label(text(f, t(sprache, "tools.sub"), Ton::Leise));
    });
}

fn karte(ui: &mut egui::Ui, sicht: &mut Sicht, eintrag: &EintragSicht, aktionen: &mut Vec<Aktion>) {
    let f = *sicht.thema.farben();
    let sprache = sicht.sprache;
    let ton = logik::zustand_ton(&eintrag.zustand);
    let stand = logik::laden_text(sprache, &eintrag.zustand, eintrag.ladeanteil);
    let fehler = logik::fehlertext(&eintrag.zustand).map(str::to_string);
    // Fehlt das Programm, sagt die Karte das einmal – und zwar an der Stelle
    // des Zustands. Alles Technische bleibt weg, und einen Anmelde-Knopf gibt
    // es nicht: es gäbe nichts zu starten (Aufgabe 35, §5).
    let nicht_installiert = eintrag.nicht_installiert.is_some();
    // Der Server hat das Programm für dieses System nicht (Aufgabe 37): kein
    // Fehler, eine Auskunft – ebenfalls ohne Anmelde-Knopf.
    let nicht_verfuegbar = nicht_installiert || eintrag.nicht_fuer_system;
    // Ein Knopf für die Angaben, nie zwei (Aufgabe 35, §5).
    let angaben_knopf = logik::kartenknopf(eintrag);
    let unter = (!eintrag.beschreibung.is_empty()).then(|| eintrag.beschreibung.clone());
    let mut angaben = false;
    let mut bearbeiten = false;
    let mut aufklappen = false;
    let mut anmelden = false;
    let mut pruefen = false;
    let mut abmelden = false;
    let mut einrichten = false;
    let mut abbrechen = false;
    let mut schalten: Option<bool> = None;
    let mut neu_versuchen = false;
    // Abgeschaltet: die Karte ist gedämpft, die Knöpfe sind gesperrt
    // (Aufgabe 19).
    let aus = eintrag.am_geraet_aus;
    let gesperrt = t(sprache, "tools.gesperrt_hinweis");
    // Zugänge (Aufgabe 14): aufklappen, anlegen, bearbeiten, löschen.
    let mut zugaenge_aufklappen = false;
    let mut zugaenge_oeffnen = false;
    let mut zugang_neu = false;
    let mut zugang_bearbeiten: Option<String> = None;
    let mut zugang_loeschen: Option<String> = None;
    let offen = sicht.offen.contains(&eintrag.id);
    let zugaenge_offen = sicht.zugaenge_offen.contains(&eintrag.id);

    bausteine::karte_mit(ui, &f, &eintrag.titel, unter.as_deref(), aus, |ui| {
        // `shrink_left`: die Knöpfe sitzen zuerst, ganz rechts, und der Text
        // links bekommt den Rest und wird gekürzt. Mit `shrink_right` wüchse
        // die Karte mit einem langen Zustandstext (etwa einer Liste fehlender
        // Felder) über den Rand, die Knöpfe liefen hinaus und der Text
        // darunter sähe aus, als läge er unter ihnen (Aufgabe 35).
        egui::Sides::new()
            .height(thema::SIZE_CONTROL_SM)
            .shrink_left()
            .truncate()
            .show(
                ui,
                |ui| {
                    if logik::karte_dreht(&eintrag.zustand) {
                        // Das Programm kommt gerade vom Server: Ring und Text,
                        // mit Anteil, wenn die Größe bekannt ist (Nachtrag zu
                        // Aufgabe 37).
                        if ui.style().animation_time > 0.0 {
                            bausteine::status_dreht(ui, &f, ton, &stand);
                            ui.ctx()
                                .request_repaint_after(std::time::Duration::from_millis(100));
                        } else {
                            bausteine::status(ui, &f, ton, &stand);
                        }
                    } else if nicht_installiert || eintrag.nicht_fuer_system {
                        let satz = if eintrag.nicht_fuer_system {
                            t(sprache, "tools.nicht_fuer_system")
                        } else {
                            t(sprache, "tools.nicht_installiert_text")
                        };
                        bausteine::status(ui, &f, Ton::Warnung, &satz).on_hover_text(satz);
                    } else {
                        bausteine::status(ui, &f, ton, &stand);
                    }
                },
                |ui| {
                    // Der Knopf für die Angaben steht auf jeder Karte an
                    // derselben Stelle: ganz rechts.
                    match angaben_knopf {
                        logik::Kartenknopf::Keiner => {}
                        logik::Kartenknopf::AngabenErgaenzen => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Schluessel,
                                    &t(sprache, "tools.ergaenzen"),
                                )
                            }) {
                                angaben = true;
                            }
                        }
                        logik::Kartenknopf::Bearbeiten => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::icon_knopf(
                                    ui,
                                    &f,
                                    Icon::Bearbeiten,
                                    &t(sprache, "tools.bearbeiten"),
                                    false,
                                )
                            }) {
                                bearbeiten = true;
                            }
                        }
                    }
                },
            );

        // Der Fehlertext bricht um, auch mitten im Wort: eine lange Adresse
        // oder ein Pfad darf die Karte nicht breiter machen als ihre
        // Geschwister und nicht unter die Knöpfe laufen (Aufgabe 35).
        //
        // Fehlt das Programm, steht der Satz schon in der Zustandszeile; der
        // technische Text des Startfehlers kommt nicht zusätzlich auf die
        // Karte (Aufgabe 35, §5).
        if let Some(fehler) = fehler.filter(|_| !nicht_verfuegbar) {
            ui.add_space(thema::SPACE_1);
            bausteine::absatz(ui, &f, fehler, Ton::Fehler);
            // „Erneut versuchen“ (Nachtrag zu Aufgabe 37): ein Programm, das
            // nicht geladen werden konnte, wird dabei erneut geholt; ein
            // abgeschalteter Eintrag bietet den Knopf nicht an.
            if !aus {
                ui.add_space(thema::SPACE_1);
                if bausteine::knopf(
                    ui,
                    &f,
                    Icon::Neuladen,
                    &t(sprache, "tools.erneut_versuchen"),
                )
                .clicked()
                {
                    neu_versuchen = true;
                }
            }
        }
        // Eine Kennung, die nicht als Präfix taugt: der Eintrag startet nicht,
        // und das Fenster sagt warum (Paket 3).
        if let Some(grund) = &eintrag.unzulaessig {
            ui.add_space(thema::SPACE_1);
            bausteine::absatz(
                ui,
                &f,
                tf(sprache, "tools.kennung_unbrauchbar", &[grund]),
                Ton::Fehler,
            );
        }
        // Die Anmeldung (Aufgabe 10): ein Tool kann eine Anmeldung des Users
        // brauchen. Sie wird hier im Fenster ausgelöst, nie von einem Agenten.
        // Fehlt das Programm, gibt es nichts anzumelden (Aufgabe 35, §5).
        if let Some(zustand) = eintrag.anmeldung.as_ref().filter(|_| !nicht_verfuegbar) {
            ui.add_space(thema::SPACE_2);
            let zeile = logik::anmeldung_zeile(sprache, zustand);
            egui::Sides::new()
                .height(thema::SIZE_CONTROL_SM)
                .shrink_left()
                .truncate()
                .show(
                    ui,
                    |ui| {
                        if logik::dreht_sich(zustand) {
                            // Eine Prüfung läuft: Drehsymbol, kein Knopf.
                            bausteine::status_dreht(ui, &f, zeile.ton, &zeile.text);
                        } else {
                            bausteine::status(ui, &f, zeile.ton, &zeile.text);
                        }
                    },
                    |ui| match zeile.knopf {
                        logik::AnmeldeKnopf::Keiner => {}
                        logik::AnmeldeKnopf::Anmelden => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::hauptknopf_mit(
                                    ui,
                                    &f,
                                    Icon::Schluessel,
                                    &t(sprache, "anmeldung.anmelden"),
                                )
                            }) {
                                anmelden = true;
                            }
                        }
                        logik::AnmeldeKnopf::Pruefen => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Neuladen,
                                    &t(sprache, "anmeldung.erneut"),
                                )
                            }) {
                                pruefen = true;
                            }
                        }
                        logik::AnmeldeKnopf::Abmelden => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Trennen,
                                    &t(sprache, "anmeldung.abmelden"),
                                )
                            }) {
                                abmelden = true;
                            }
                        }
                    },
                );
        }
        // Die Einrichtung (Aufgabe 11): Tools verwalten Zugänge in Profilen.
        // Eingerichtet wird auf der Seite des Tools selbst — „Einrichten“ ist
        // ein normaler Knopf, kein Hauptknopf. Ohne Programm gibt es auch
        // nichts einzurichten (Aufgabe 35, §5).
        if let Some(zustand) = eintrag.einrichtung.as_ref().filter(|_| !nicht_verfuegbar) {
            ui.add_space(thema::SPACE_2);
            let zeile = logik::einrichtung_zeile(sprache, zustand);
            egui::Sides::new()
                .height(thema::SIZE_CONTROL_SM)
                .shrink_left()
                .truncate()
                .show(
                    ui,
                    |ui| {
                        bausteine::status(ui, &f, zeile.ton, &zeile.text);
                    },
                    |ui| match zeile.knopf {
                        logik::EinrichtKnopf::Keiner => {}
                        logik::EinrichtKnopf::Einrichten => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Einstellungen,
                                    &t(sprache, "einrichtung.einrichten"),
                                )
                            }) {
                                einrichten = true;
                            }
                        }
                        logik::EinrichtKnopf::Erneut => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Neuladen,
                                    &t(sprache, "einrichtung.erneut"),
                                )
                            }) {
                                einrichten = true;
                            }
                        }
                        logik::EinrichtKnopf::Abbrechen => {
                            if nur_wenn_an(ui, aus, &gesperrt, |ui| {
                                bausteine::knopf(
                                    ui,
                                    &f,
                                    Icon::Schliessen,
                                    &t(sprache, "einrichtung.abbrechen"),
                                )
                            }) {
                                abbrechen = true;
                            }
                        }
                    },
                );
        }
        // Mehrere Zugänge (Aufgabe 14): Zeile „Verbindungen (2)“, Knopf
        // „Verbindung hinzufügen“ und die aufklappbare Liste. Der Eintrag
        // läuft auch ohne Zugang – das Tool meldet dann selbst „kein Zugang“.
        if let Some(zugaenge) = &eintrag.zugaenge {
            ui.add_space(thema::SPACE_2);
            let kopf = logik::zugang_beschriftung(sprache, zugaenge);
            ui.horizontal(|ui| {
                if bausteine::icon_text_knopf(ui, &f, Icon::Schluessel, &kopf, zugaenge_offen)
                    .clicked()
                {
                    zugaenge_aufklappen = true;
                }
                if bausteine::knopf(
                    ui,
                    &f,
                    Icon::Plus,
                    &logik::zugang_hinzufuegen(sprache, zugaenge),
                )
                .clicked()
                {
                    // „… hinzufügen“ führt zur Liste: sie klappt auf, und der
                    // Dialog für den neuen Zugang steht davor. So braucht es
                    // keinen zweiten Weg über den Stift (Aufgabe 35, §5).
                    zugaenge_oeffnen = true;
                    zugang_neu = true;
                }
            });
            if zugaenge_offen {
                ui.add_space(thema::SPACE_2);
                egui::Frame::new()
                    .fill(f.bg_raised)
                    .inner_margin(egui::Margin::symmetric(
                        thema::SPACE_3 as i8,
                        thema::SPACE_2 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        if zugaenge.liste.is_empty() {
                            ui.add(
                                egui::Label::new(
                                    text(&f, t(sprache, "zugaenge.leer"), Ton::Blass).small(),
                                )
                                .wrap(),
                            );
                        }
                        for zugang in &zugaenge.liste {
                            egui::Sides::new()
                                .height(thema::SIZE_CONTROL_SM)
                                .shrink_left()
                                .truncate()
                                .show(
                                    ui,
                                    |ui| {
                                        // Gekürzt, mit dem ganzen Text im Hinweis.
                                        ui.add(
                                            egui::Label::new(RichText::new(&zugang.name).strong())
                                                .truncate(),
                                        )
                                        .on_hover_text(&zugang.name);
                                        let werte =
                                            logik::zugang_werte_zeile(&zugaenge.felder, zugang);
                                        if !werte.is_empty() {
                                            ui.add(
                                                egui::Label::new(
                                                    text(&f, &werte, Ton::Leise).small(),
                                                )
                                                .truncate(),
                                            )
                                            .on_hover_text(&werte);
                                        }
                                    },
                                    |ui| {
                                        if bausteine::knopf(
                                            ui,
                                            &f,
                                            Icon::Bearbeiten,
                                            &t(sprache, "zugaenge.bearbeiten"),
                                        )
                                        .clicked()
                                        {
                                            zugang_bearbeiten = Some(zugang.name.clone());
                                        }
                                        if bausteine::knopf(
                                            ui,
                                            &f,
                                            Icon::Papierkorb,
                                            &t(sprache, "zugaenge.loeschen"),
                                        )
                                        .clicked()
                                        {
                                            zugang_loeschen = Some(zugang.name.clone());
                                        }
                                    },
                                );
                        }
                    });
            }
        }
        // Was fehlt, steht im Abzeichen der Karte („Braucht Angaben: PFAD“).
        // Eine zweite Zeile „Fehlt: …“ sagte dasselbe noch einmal
        // (Aufgabe 35, §5).

        if !eintrag.tools.is_empty() {
            ui.add_space(thema::SPACE_2);
            let anzahl = eintrag.tools.len();
            let beschriftung = if offen {
                t(sprache, "tools.verbergen")
            } else {
                tf(sprache, "tools.anzeigen", &[&anzahl])
            };
            if bausteine::icon_text_knopf(ui, &f, Icon::Satz, &beschriftung, offen).clicked() {
                aufklappen = true;
            }
            if offen {
                ui.add_space(thema::SPACE_2);
                egui::Frame::new()
                    .fill(f.bg_raised)
                    .inner_margin(egui::Margin::symmetric(
                        thema::SPACE_3 as i8,
                        thema::SPACE_2 as i8,
                    ))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        for werkzeug in &eintrag.tools {
                            ui.add(egui::Label::new(RichText::new(werkzeug).monospace()).wrap());
                        }
                    });
            }
        }

        // Der Schalter „Auf diesem Gerät aktiv“ steht rechts in der Karte
        // (Aufgabe 19). Standard an – auch für neu zugewiesene Einträge. Er
        // schließt die Karte ab, damit die Zeilen darüber an ihrem Platz
        // bleiben (Aufgabe 18: nichts verrutscht, wenn es enger wird).
        if aus {
            ui.add(
                egui::Label::new(
                    text(&f, t(sprache, "tools.abgeschaltet_hinweis"), Ton::Leise).small(),
                )
                .wrap(),
            );
        }
        ui.add_space(thema::SPACE_2);
        egui::Sides::new()
            .height(thema::SIZE_CONTROL_SM)
            .shrink_left()
            .truncate()
            .show(
                ui,
                |_ui| {},
                |ui| {
                    let mut an = !aus;
                    if bausteine::haken(ui, &mut an, &t(sprache, "tools.aktiv")).changed() {
                        schalten = Some(an);
                    }
                },
            );
    });

    if angaben || bearbeiten {
        sicht.werte_dialog(eintrag);
    }
    if aufklappen {
        sicht.umschalten(&eintrag.id);
    }
    if zugaenge_aufklappen {
        sicht.zugaenge_umschalten(&eintrag.id);
    }
    if zugaenge_oeffnen {
        sicht.zugaenge_oeffnen(&eintrag.id);
    }
    if zugang_neu {
        sicht.zugang_dialog(eintrag, None);
    }
    if let Some(name) = zugang_bearbeiten {
        sicht.zugang_dialog(eintrag, Some(&name));
    }
    if let Some(name) = zugang_loeschen {
        // Löschen nimmt die Geheimnisse mit – deshalb erst die Rückfrage.
        sicht.dialog = Dialog::ZugangEntfernen {
            eintrag: eintrag.id.clone(),
            name,
            einzahl: eintrag
                .zugaenge
                .as_ref()
                .map(|zugaenge| zugaenge.einzahl.clone())
                .unwrap_or_default(),
        };
    }
    if neu_versuchen {
        aktionen.push(Aktion::EintragNeuVersuchen {
            eintrag: eintrag.id.clone(),
        });
    }
    if anmelden {
        aktionen.push(Aktion::EintragAnmelden {
            eintrag: eintrag.id.clone(),
        });
    }
    if pruefen {
        aktionen.push(Aktion::AnmeldungPruefen {
            eintrag: eintrag.id.clone(),
        });
    }
    if einrichten {
        aktionen.push(Aktion::EintragEinrichten {
            eintrag: eintrag.id.clone(),
        });
    }
    if abbrechen {
        aktionen.push(Aktion::EinrichtungAbbrechen {
            eintrag: eintrag.id.clone(),
        });
    }
    if abmelden {
        // Abmelden beendet eine laufende Sitzung – deshalb erst die Rückfrage.
        sicht.dialog = Dialog::AnmeldungBeenden {
            eintrag: eintrag.id.clone(),
            titel: eintrag.titel.clone(),
        };
    }
    if let Some(an) = schalten {
        aktionen.push(Aktion::EintragSchalten {
            eintrag: eintrag.id.clone(),
            an,
        });
    }
}

/// Einen Knopf der Karte zeichnen. Ist der Eintrag auf diesem Gerät
/// abgeschaltet, bleibt der Knopf gesperrt und nennt am Zeiger den Grund
/// (Aufgabe 19). `true` heißt: geklickt.
fn nur_wenn_an(
    ui: &mut egui::Ui,
    aus: bool,
    hinweis: &str,
    knopf: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> bool {
    let antwort = ui.add_enabled_ui(!aus, knopf).inner;
    antwort.on_disabled_hover_text(hinweis).clicked()
}
