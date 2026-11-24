// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Bausteine des Client-Fensters: Karte, Seitenkopf, Hauptknopf, leere
//! Fläche, Kürzel, Formularzeilen. Sie bekommen Farben aus dem Theme, nie
//! Werte.

use crate::thema::{self, fett, Farben, Thema, Ton};
use egui::{Color32, RichText, Sense, Stroke};

/// Text in einer Bedeutungsfarbe.
pub fn text(f: &Farben, inhalt: impl Into<String>, art: Ton) -> RichText {
    RichText::new(inhalt).color(thema::ton(f, art))
}

/// Überschrift einer Seite mit Unterzeile.
pub fn seitenkopf(ui: &mut egui::Ui, f: &Farben, titel: &str, unter: &str) {
    // Titel und Unterzeile stehen 4 px auseinander, darunter folgt der Inhalt
    // mit 32 px — dieselben Maße wie `Seitenkopf.vue` und `.seite`. In einem
    // flachen Fenster (`breakpoint.flach`) ist die Überschrift eine Stufe
    // kleiner und der Abstand darunter enger: sonst bliebe für den Inhalt
    // nichts übrig (Aufgabe 18).
    let flach = ist_flach(ui);
    ui.add_space(if flach {
        thema::SPACE_2
    } else {
        thema::SPACE_4
    });
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = thema::SPACE_1;
        let groesse = if flach {
            thema::FONT_SIZE_XL
        } else {
            thema::FONT_SIZE_2XL
        };
        ui.label(RichText::new(titel).font(egui::FontId::new(groesse, fett())));
        ui.label(text(f, unter, Ton::Leise));
    });
    ui.add_space(if flach {
        thema::SPACE_4
    } else {
        thema::SPACE_5
    });
}

/// Ist das Fenster flach (`breakpoint.flach`)? Dann sind Überschriften und
/// Abstände eine Stufe kleiner.
pub fn ist_flach(ui: &egui::Ui) -> bool {
    ui.ctx().content_rect().height() < thema::BREAKPOINT_FLACH
}

/// Eine Karte: Rahmen, Titel, Inhalt.
pub fn karte<R>(
    ui: &mut egui::Ui,
    f: &Farben,
    titel: &str,
    unter: Option<&str>,
    inhalt: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    karte_mit(ui, f, titel, unter, false, inhalt)
}

/// Wie [`karte`], aber `gedaempft`: Titel und Unterzeile stehen leiser. So
/// zeigt die Seite Tools einen am Gerät abgeschalteten Eintrag (Aufgabe 19),
/// ohne eine zweite Fläche zu erfinden (dieselben Token, dieselben Maße).
pub fn karte_mit<R>(
    ui: &mut egui::Ui,
    f: &Farben,
    titel: &str,
    unter: Option<&str>,
    gedaempft: bool,
    inhalt: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let titel_ton = if gedaempft { Ton::Leise } else { Ton::Normal };
    egui::Frame::new()
        .fill(f.bg_panel)
        .stroke(Stroke::new(thema::SIZE_BORDER, f.border_subtle))
        .inner_margin(egui::Margin::same(thema::SPACE_4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            // Titel und Unterzeile 4 px auseinander, der Inhalt 8 px darunter
            // (`Karte.vue`).
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = thema::SPACE_1;
                ui.label(
                    RichText::new(titel)
                        .font(egui::FontId::new(thema::FONT_SIZE_LG, fett()))
                        .color(thema::ton(f, titel_ton)),
                );
                if let Some(u) = unter {
                    ui.label(RichText::new(u).color(f.text_muted));
                }
            });
            inhalt(ui)
        })
        .inner
}

/// Hauptschaltfläche (Akzent).
pub fn hauptknopf(ui: &mut egui::Ui, f: &Farben, beschriftung: &str) -> egui::Response {
    ui.add(
        egui::Button::new(RichText::new(beschriftung).color(f.text_on_accent))
            .fill(f.accent_default)
            .stroke(Stroke::new(thema::SIZE_BORDER, f.accent_default)),
    )
}

/// Eine leere Fläche mit Titel und Erklärung.
pub fn leer(ui: &mut egui::Ui, f: &Farben, titel: &str, erklaerung: &str) {
    // Linksbündig wie die Karten der Seite — nicht mittig in einer Fläche,
    // in der alles andere links steht.
    kasten(ui, f, |ui| {
        ui.label(RichText::new(titel).font(egui::FontId::new(thema::FONT_SIZE_MD, fett())));
        if !erklaerung.is_empty() {
            ui.add(egui::Label::new(text(f, erklaerung, Ton::Leise)).wrap());
        }
    });
}

/// Eine leere Fläche mit **mittig** stehender Zeile — dort, wo eine Liste
/// leer ist und die Zeile für sich steht (Protokoll, Aufgabe 18).
pub fn leer_mitte(ui: &mut egui::Ui, f: &Farben, titel: &str, erklaerung: &str) {
    ui.add_space(thema::SPACE_6);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(titel).font(egui::FontId::new(thema::FONT_SIZE_MD, fett())));
        if !erklaerung.is_empty() {
            ui.add_space(thema::SPACE_1);
            ui.add(egui::Label::new(text(f, erklaerung, Ton::Leise).small()).wrap());
        }
    });
}

/// Ein Kasten wie eine Karte, ohne Titel (Listen, leere Ansichten).
pub fn kasten<R>(ui: &mut egui::Ui, f: &Farben, inhalt: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .fill(f.bg_panel)
        .stroke(Stroke::new(thema::SIZE_BORDER, f.border_subtle))
        .inner_margin(egui::Margin::same(thema::SPACE_4 as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            inhalt(ui)
        })
        .inner
}

/// Kürzel-Quadrat (Avatar): Akzent für mich, sonst neutral; ein Rand im
/// Signal, wenn hervorgehoben.
pub fn kuerzel(ui: &mut egui::Ui, f: &Farben, name: &str, ich: bool, hervorgehoben: bool) {
    let groesse = thema::SIZE_CONTROL_SM;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(groesse, groesse), Sense::hover());
    let p = ui.painter();
    let (flaeche, schrift): (Color32, Color32) = if ich {
        (f.accent_default, f.text_on_accent)
    } else {
        (f.bg_selected, f.text_default)
    };
    p.rect_filled(rect, 0.0, flaeche);
    if hervorgehoben {
        p.rect_stroke(
            rect.expand(thema::SIZE_FOCUS),
            0.0,
            Stroke::new(thema::SIZE_FOCUS, f.signal_default),
            egui::StrokeKind::Outside,
        );
    }
    let buchstabe: String = name
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default();
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        buchstabe,
        egui::FontId::new(thema::FONT_SIZE_MD, fett()),
        schrift,
    );
}

/// Das Zeichen zum Farbschema (`assets/marke`, freigestellt).
fn logo(thema: Thema) -> egui::ImageSource<'static> {
    match thema {
        Thema::Dunkel => egui::include_image!("../../../assets/marke/logo-dunkel.png"),
        Thema::Hell => egui::include_image!("../../../assets/marke/logo-hell.png"),
        Thema::Mono => egui::include_image!("../../../assets/marke/logo-mono.png"),
    }
}

/// Die Marke im Kopf: Zeichen und Wortmarke, wie `Marke.vue` in der Verwaltung.
/// Das Zeichen ist Schmuck, den Namen trägt die Wortmarke.
pub fn marke(ui: &mut egui::Ui, f: &Farben, thema: Thema) {
    ui.add(
        egui::Image::new(logo(thema))
            .fit_to_exact_size(egui::Vec2::splat(thema::SIZE_LOGO))
            .alt_text(""),
    );
    // Genau `SPACE_2` wie in `Marke.vue`, unabhängig vom Abstand der Zeile.
    ui.add_space(thema::SPACE_2 - ui.spacing().item_spacing.x);
    ui.label(
        RichText::new("mcp-quiver")
            .font(egui::FontId::new(thema::FONT_SIZE_LG, fett()))
            .color(f.text_default),
    );
}

/// Die Icons der Fenster — dieselbe kuratierte Lucide-Auswahl wie
/// `@weppelmann/icons`. Ein Summentyp: eine Ansicht kann kein Icon verlangen,
/// das es nicht gibt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Plus,
    Mehr,
    Schloss,
    PanelRechts,
    PanelZu,
    Schliessen,
    Stecker,
    Trennen,
    Einstellungen,
    Neuladen,
    Chat,
    Paket,
    Satz,
    Gruppe,
    Geraet,
    Schluessel,
    Agent,
    Papierkorb,
    Kopieren,
    Server,
    Protokoll,
    Sperre,
    Start,
    Stopp,
    Haken,
    Hinweis,
    Bearbeiten,
    Hochladen,
    Suchen,
    Warnung,
    Datei,
    Herunterladen,
    Extern,
}

impl Icon {
    pub fn quelle(self) -> egui::ImageSource<'static> {
        match self {
            Icon::Plus => egui::include_image!("../icons/plus.svg"),
            Icon::Mehr => egui::include_image!("../icons/more.svg"),
            Icon::Schloss => egui::include_image!("../icons/lock.svg"),
            Icon::PanelRechts => egui::include_image!("../icons/panel_right.svg"),
            Icon::PanelZu => egui::include_image!("../icons/panel_close.svg"),
            Icon::Schliessen => egui::include_image!("../icons/close.svg"),
            Icon::Stecker => egui::include_image!("../icons/plug.svg"),
            Icon::Trennen => egui::include_image!("../icons/unplug.svg"),
            Icon::Einstellungen => egui::include_image!("../icons/settings.svg"),
            Icon::Neuladen => egui::include_image!("../icons/refresh.svg"),
            Icon::Chat => egui::include_image!("../icons/chat.svg"),
            Icon::Paket => egui::include_image!("../icons/package.svg"),
            Icon::Satz => egui::include_image!("../icons/layers.svg"),
            Icon::Gruppe => egui::include_image!("../icons/users.svg"),
            Icon::Geraet => egui::include_image!("../icons/monitor.svg"),
            Icon::Schluessel => egui::include_image!("../icons/key_round.svg"),
            Icon::Agent => egui::include_image!("../icons/bot.svg"),
            Icon::Papierkorb => egui::include_image!("../icons/trash.svg"),
            Icon::Kopieren => egui::include_image!("../icons/copy.svg"),
            Icon::Server => egui::include_image!("../icons/server.svg"),
            Icon::Protokoll => egui::include_image!("../icons/log.svg"),
            Icon::Sperre => egui::include_image!("../icons/ban.svg"),
            Icon::Start => egui::include_image!("../icons/play.svg"),
            Icon::Stopp => egui::include_image!("../icons/stop.svg"),
            Icon::Haken => egui::include_image!("../icons/check.svg"),
            Icon::Hinweis => egui::include_image!("../icons/circle_alert.svg"),
            Icon::Bearbeiten => egui::include_image!("../icons/pencil.svg"),
            Icon::Hochladen => egui::include_image!("../icons/upload.svg"),
            Icon::Suchen => egui::include_image!("../icons/search.svg"),
            Icon::Warnung => egui::include_image!("../icons/warning.svg"),
            Icon::Datei => egui::include_image!("../icons/datei.svg"),
            Icon::Herunterladen => egui::include_image!("../icons/herunterladen.svg"),
            Icon::Extern => egui::include_image!("../icons/external.svg"),
        }
    }

    /// Das Icon als Bild in Größe und Farbe.
    pub fn bild(self, groesse: f32, farbe: Color32) -> egui::Image<'static> {
        egui::Image::new(self.quelle())
            .tint(farbe)
            .fit_to_exact_size(egui::vec2(groesse, groesse))
    }
}

/// Ein Icon ohne Knopf (Schloss neben dem Raumnamen u. ä.).
pub fn icon(ui: &mut egui::Ui, f: &Farben, icon: Icon, ton: Ton) -> egui::Response {
    ui.add(icon.bild(thema::FONT_SIZE_MD, thema::ton(f, ton)))
}

/// Knopf nur mit Icon. `name` ist Hinweis **und** zugänglicher Name — ein
/// Screenreader liest ihn vor, die Maus zeigt ihn beim Darüberfahren.
/// `an` hebt den Knopf als eingeschaltet hervor (Mikrofon stumm …).
pub fn icon_knopf(
    ui: &mut egui::Ui,
    f: &Farben,
    icon: Icon,
    name: &str,
    an: bool,
) -> egui::Response {
    let farbe = if an { f.accent_default } else { f.text_muted };
    let antwort = ui
        .add(
            egui::Button::image(icon.bild(thema::FONT_SIZE_LG, farbe))
                .selected(an)
                .frame_when_inactive(an)
                .min_size(egui::vec2(thema::SIZE_CONTROL_SM, thema::SIZE_CONTROL_SM)),
        )
        .on_hover_text(name);
    let name = name.to_string();
    antwort.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
    antwort
}

/// Knopf mit Icon und Text (Reiter, Hauptaktionen).
pub fn icon_text_knopf(
    ui: &mut egui::Ui,
    f: &Farben,
    icon: Icon,
    text: &str,
    gewaehlt: bool,
) -> egui::Response {
    let (farbe, schrift) = if gewaehlt {
        (f.accent_default, f.text_default)
    } else {
        (f.text_muted, f.text_muted)
    };
    ui.add(
        egui::Button::image_and_text(
            icon.bild(thema::FONT_SIZE_MD, farbe),
            RichText::new(text).color(schrift),
        )
        .selected(gewaehlt)
        .frame_when_inactive(gewaehlt),
    )
}

/// Gewöhnlicher Knopf mit Icon vor dem Text (Neu laden, Sprechtest …).
pub fn knopf(ui: &mut egui::Ui, f: &Farben, icon: Icon, text: &str) -> egui::Response {
    ui.add(egui::Button::image_and_text(
        icon.bild(thema::FONT_SIZE_MD, f.text_muted),
        text,
    ))
}

/// Hauptschaltfläche (Akzent) mit Icon vor dem Text.
pub fn hauptknopf_mit(ui: &mut egui::Ui, f: &Farben, icon: Icon, text: &str) -> egui::Response {
    ui.add(
        egui::Button::image_and_text(
            icon.bild(thema::FONT_SIZE_MD, f.text_on_accent),
            RichText::new(text).color(f.text_on_accent),
        )
        .fill(f.accent_default)
        .stroke(Stroke::new(thema::SIZE_BORDER, f.accent_default)),
    )
}

// ---- Seiten und Formulare ------------------------------------------------

/// Größte Breite eines Eingabefelds — lange Felder lesen sich schlecht,
/// und alle Felder einer Seite sind gleich breit.
pub const FELDBREITE_MAX: f32 = thema::SPACE_8 * 10.0;
/// Ab dieser Breite steht die Beschriftung links neben dem Feld, darunter
/// darüber (`breakpoint.formular`, Aufgabe 18).
const FORMULAR_NEBENEINANDER: f32 = thema::BREAKPOINT_FORMULAR;
/// Breite der Beschriftungsspalte.
const BESCHRIFTUNG: f32 = thema::SPACE_8 * 3.0;

/// Abstand des Inhalts zum Fensterrand.
pub const SEITENRAND: f32 = thema::SPACE_5;

/// Ist das Fenster schmal (`breakpoint.schmal`)? Dann trägt die
/// Seitenleiste nur Zeichen mit Hinweis (Aufgabe 18).
pub fn schmal(ui: &egui::Ui) -> bool {
    ui.ctx().content_rect().width() < thema::BREAKPOINT_SCHMAL
}

/// Ist für die Kopfzeile zu wenig Platz (`breakpoint.formular`)? Dann stehen
/// Farbschema und Sprache in einem Menü statt als Segmentreihen.
pub fn eng(ui: &egui::Ui) -> bool {
    ui.ctx().content_rect().width() < thema::BREAKPOINT_FORMULAR
}

/// Eine Seite: senkrecht scrollbar, so breit wie das Fenster — die Karten
/// füllen die Fläche, statt mit leerem Rand daneben zu hängen. Der Rand liegt
/// **innen**: der Scrollbalken sitzt am Fensterrand, nur der Inhalt hält
/// Abstand.
pub fn seite<R>(ui: &mut egui::Ui, inhalt: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            rand(ui, |ui| {
                // Nie breiter als das Fenster — sonst wüchse der Inhalt mit
                // dem längsten Element über den Rand.
                ui.set_max_width(ui.available_width());
                let r = inhalt(ui);
                ui.add_space(thema::SPACE_6);
                r
            })
        })
        .inner
}

/// Der Seitenrand für Seiten mit eigenem Scrollbereich (Protokoll). Der
/// Inhalt steht höchstens `size.inhalt-max` breit und mittig (Aufgabe 18).
pub fn rand<R>(ui: &mut egui::Ui, inhalt: impl FnOnce(&mut egui::Ui) -> R) -> R {
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(SEITENRAND as i8, 0))
        .show(ui, |ui| spalte(ui, inhalt))
        .inner
}

/// Eine Spalte mit höchstens `size.inhalt-max` Breite, mittig im Platz, der
/// übrig ist. Bei schmalem Fenster nimmt sie den ganzen Platz.
fn spalte<R>(ui: &mut egui::Ui, inhalt: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let verfuegbar = ui.available_width();
    let breite = verfuegbar.min(thema::SIZE_INHALT_MAX);
    let links = ((verfuegbar - breite) / 2.0).floor();
    if links < 1.0 {
        return inhalt(ui);
    }
    let rect = egui::Rect::from_min_size(
        ui.cursor().min + egui::vec2(links, 0.0),
        egui::vec2(breite, ui.available_height()),
    );
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), inhalt)
        .inner
}

/// Eine Leiste aus Steuerelementen (Filter, Knöpfe): bricht um, statt seitlich
/// hinauszuragen, und hält einen einheitlichen Abstand (Aufgabe 18).
pub fn leiste<R>(ui: &mut egui::Ui, inhalt: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = thema::SPACE_2;
        inhalt(ui)
    })
    .inner
}

/// Ein Feld in einer Leiste: Beschriftung klein darüber, Steuerelement darunter.
/// `None` lässt den Platz der Beschriftung frei, damit ein Häkchen auf einer
/// Höhe mit den Auswahlen steht (Aufgabe 18). `breite` ist die Breite des
/// Blocks — eine umbrechende Leiste entscheidet daran, ob das Feld noch in
/// die Zeile passt (ein verschachteltes `ui.vertical` nähme sonst den ganzen
/// Rest und bräche nie um).
pub fn leistenfeld<R>(
    ui: &mut egui::Ui,
    f: &Farben,
    beschriftung: Option<&str>,
    breite: f32,
    inhalt: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let hoehe =
        thema::FONT_SIZE_SM * thema::FONT_LINE_NORMAL + thema::SPACE_1 + thema::SIZE_CONTROL_MD;
    ui.allocate_ui_with_layout(
        egui::vec2(breite, hoehe),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.set_width(breite);
            // Beschriftung und Feld stehen dicht beieinander — die Leiste
            // soll so flach wie möglich bleiben.
            ui.spacing_mut().item_spacing.y = thema::SPACE_1;
            match beschriftung {
                Some(titel) => {
                    ui.label(text(f, titel, Ton::Leise).small());
                }
                None => {
                    ui.add_space(thema::FONT_SIZE_SM * thema::FONT_LINE_NORMAL);
                }
            }
            inhalt(ui)
        },
    )
    .inner
}

/// Drückt das nächste Element an den rechten Rand der laufenden Zeile — in
/// einer umbrechenden Leiste die letzte Aktion einer Zeile. Passt sie nicht
/// mehr, rückt sie in die nächste Zeile.
pub fn nach_rechts(ui: &mut egui::Ui, breite: f32) {
    // In einer umbrechenden Leiste ist `available_width` die ganze Zeile,
    // nicht der Rest — der Rest rechnet sich aus Cursor und Rand.
    if ui.max_rect().right() - ui.cursor().min.x < breite {
        // Passt nicht mehr: die Aktion beginnt eine neue Zeile und steht
        // dort rechts.
        ui.end_row();
    }
    let rest = ui.max_rect().right() - ui.cursor().min.x - breite;
    if rest > 0.0 {
        ui.add_space(rest);
    }
}

/// Die Breite eines Knopfes mit Zeichen und Text — damit sich die letzte
/// Aktion einer Leiste an den rechten Rand schieben lässt (`nach_rechts`).
pub fn knopfbreite(ui: &egui::Ui, beschriftung: &str) -> f32 {
    textbreite(ui, beschriftung, thema::FONT_SIZE_MD)
        + thema::FONT_SIZE_MD
        + thema::SPACE_2
        + 2.0 * (thema::SIZE_CONTROL_PADDING + thema::SIZE_BORDER)
}

/// Die Breite einer Segmentreihe aus ihren Beschriftungen — dieselbe Rechnung
/// wie in [`segmentauswahl`].
pub fn segmentbreite(ui: &egui::Ui, namen: &[String]) -> f32 {
    let mut breite = 0.0;
    for name in namen {
        breite += textbreite(ui, name, thema::FONT_SIZE_SM)
            + 2.0 * thema::SPACE_3
            + 2.0 * thema::SIZE_BORDER
            - thema::SIZE_BORDER;
    }
    breite
}

/// Die Breite eines Häkchens mit Beschriftung.
pub fn hakenbreite(ui: &egui::Ui, beschriftung: &str) -> f32 {
    ui.spacing().icon_width
        + ui.spacing().icon_spacing
        + textbreite(ui, beschriftung, thema::FONT_SIZE_MD)
}

/// Wie breit ein Text in dieser Schrift ist.
fn textbreite(ui: &egui::Ui, inhalt: &str, groesse: f32) -> f32 {
    ui.painter()
        .layout_no_wrap(
            inhalt.to_string(),
            egui::FontId::proportional(groesse),
            egui::Color32::WHITE,
        )
        .rect
        .width()
}

/// Eine Formularzeile: Beschriftung, Bedienelement, Hinweis darunter. Breit
/// nebeneinander, schmal untereinander. Die Beschriftung wird zum
/// zugänglichen Namen des Elements.
pub fn feld(
    ui: &mut egui::Ui,
    f: &Farben,
    beschriftung: &str,
    hinweis: Option<&str>,
    inhalt: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    formularzeile(ui, f, beschriftung, hinweis, true, inhalt)
}

/// Wie [`feld`], aber die Beschriftung wird **nicht** zum Namen des
/// Elements — für Knöpfe und Regler, die einen eigenen Namen tragen
/// („Sprechtest starten" statt „Sprechtest").
pub fn zeile(
    ui: &mut egui::Ui,
    f: &Farben,
    beschriftung: &str,
    hinweis: Option<&str>,
    inhalt: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    formularzeile(ui, f, beschriftung, hinweis, false, inhalt)
}

fn formularzeile(
    ui: &mut egui::Ui,
    f: &Farben,
    beschriftung: &str,
    hinweis: Option<&str>,
    benennt: bool,
    inhalt: impl FnOnce(&mut egui::Ui) -> egui::Response,
) -> egui::Response {
    // `Feld.vue` hat 4 px Polster oben und unten; hier steht dasselbe.
    ui.add_space(thema::SPACE_1);
    let nebeneinander = ui.available_width() >= FORMULAR_NEBENEINANDER;
    let verknuepfen = |a: egui::Response, l: &egui::Response| {
        if benennt {
            a.labelled_by(l.id)
        } else {
            a
        }
    };
    let antwort = if nebeneinander {
        ui.horizontal(|ui| {
            // Beschriftung und Feld stehen 12 px auseinander — wie in der
            // Verwaltung (`Feld.vue`).
            ui.spacing_mut().item_spacing.x = thema::SPACE_3;
            let l = beschriftungsspalte(ui, f, beschriftung, thema::SIZE_CONTROL_MD);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = thema::SPACE_1;
                let a = verknuepfen(inhalt(ui), &l);
                if let Some(h) = hinweis {
                    ui.add(egui::Label::new(text(f, h, Ton::Blass).small()).wrap());
                }
                a
            })
            .inner
        })
        .inner
    } else {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = thema::SPACE_1;
            let l = ui.label(text(f, beschriftung, Ton::Leise).small());
            let a = verknuepfen(inhalt(ui), &l);
            if let Some(h) = hinweis {
                ui.add(egui::Label::new(text(f, h, Ton::Blass).small()).wrap());
            }
            a
        })
        .inner
    };
    ui.add_space(thema::SPACE_1);
    antwort
}

/// Die linke Spalte eines Formulars: feste Breite, Text linksbündig, gekürzt.
fn beschriftungsspalte(
    ui: &mut egui::Ui,
    f: &Farben,
    beschriftung: &str,
    hoehe: f32,
) -> egui::Response {
    ui.allocate_ui_with_layout(
        egui::vec2(BESCHRIFTUNG, hoehe),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_size(egui::vec2(BESCHRIFTUNG, hoehe));
            ui.add(egui::Label::new(text(f, beschriftung, Ton::Leise).small()).truncate())
        },
    )
    .inner
}

/// Breite für Eingabefelder: so breit wie Platz ist, höchstens eine Spalte.
pub fn feldbreite(ui: &egui::Ui) -> f32 {
    ui.available_width().min(FELDBREITE_MAX)
}

/// Fakten (Beschriftung → Wert), Werte brechen um statt aus dem Rand zu
/// laufen. `mono` für Kennungen und Adressen.
pub fn fakten(ui: &mut egui::Ui, f: &Farben, zeilen: &[(String, String, bool)]) {
    let nebeneinander = ui.available_width() >= FORMULAR_NEBENEINANDER;
    for (k, w, mono) in zeilen {
        let wert = if *mono {
            RichText::new(w).monospace()
        } else {
            RichText::new(w)
        };
        if nebeneinander {
            ui.horizontal_top(|ui| {
                beschriftungsspalte(ui, f, k, thema::FONT_SIZE_LG);
                ui.add(egui::Label::new(wert).wrap());
            });
        } else {
            ui.label(text(f, k, Ton::Leise).small());
            ui.add(egui::Label::new(wert).wrap());
        }
        ui.add_space(thema::SPACE_1);
    }
}

// ---- Kopf, Seitenleiste, Segmentauswahl ---------------------------------

/// Der Kopf des Fensters: links Wortmarke und Verbindungsstatus, rechts die
/// Auswahl. Die Verwaltung baut ihn genauso (`Kopf.vue`): feste Höhe aus
/// `size.kopf`, Inhalt mittig, 16 px Rand links und rechts (AGENTS.md §2.11).
/// Der Rahmen unten ist die Trennlinie des Panels.
pub fn kopf(
    ui: &mut egui::Ui,
    f: &Farben,
    links: impl FnOnce(&mut egui::Ui),
    rechts: impl FnOnce(&mut egui::Ui),
) {
    egui::Panel::top("kopf")
        .exact_size(thema::SIZE_KOPF)
        .frame(
            egui::Frame::new()
                .fill(f.bg_panel)
                .inner_margin(egui::Margin::symmetric(
                    thema::SPACE_4 as i8,
                    thema::SPACE_2 as i8,
                )),
        )
        .show(ui, |ui| {
            egui::Sides::new()
                .height(ui.available_height())
                .spacing(thema::SPACE_3)
                .shrink_left()
                .truncate()
                .show(ui, links, rechts);
        });
}

/// Eine Auswahl aus wenigen Möglichkeiten als zusammenhängende Segmentreihe
/// (Farbschema, Sprache). Gewählt ist eine Akzentfläche mit Schrift in
/// `text-on-accent`; die Knöpfe teilen sich die Linien zwischen sich, wie
/// `margin-left: -1px` im Browser. Höhe: `size.control-sm`, Schrift: `sm`.
pub fn segmentauswahl<T: PartialEq + Copy>(
    ui: &mut egui::Ui,
    f: &Farben,
    wert: &mut T,
    moeglichkeiten: &[(T, String)],
) -> bool {
    let schrift = egui::FontId::proportional(thema::FONT_SIZE_SM);
    // Im Kopf füllt egui die rechte Seite von rechts nach links; dort stünde
    // die erste Möglichkeit sonst ganz rechts. Die Liste gilt immer von links
    // nach rechts gelesen — in beiden Welten.
    let reihenfolge: Vec<&(T, String)> = if ui.layout().main_dir() == egui::Direction::RightToLeft {
        moeglichkeiten.iter().rev().collect()
    } else {
        moeglichkeiten.iter().collect()
    };
    let mut geaendert = false;
    ui.horizontal(|ui| {
        // Die Knöpfe überlappen sich um den Rahmen: eine Linie für zwei.
        ui.spacing_mut().item_spacing.x = -thema::SIZE_BORDER;
        // 12 px Polster wie im Segment der Verwaltung (nicht die 15 px eines
        // gewöhnlichen Knopfes mit 14 px Polster).
        ui.spacing_mut().button_padding.x = thema::SPACE_3 + thema::SIZE_BORDER;
        for (m, name) in reihenfolge {
            let gewaehlt = *wert == *m;
            let (flaeche, linie, schriftfarbe) = if gewaehlt {
                (f.accent_default, f.accent_default, f.text_on_accent)
            } else {
                (f.bg_raised, f.border_strong, f.text_muted)
            };
            // So breit wie der Text plus zweimal das Polster und den Rahmen —
            // der Browser rechnet dasselbe.
            let breite = ui
                .painter()
                .layout_no_wrap(name.clone(), schrift.clone(), schriftfarbe)
                .rect
                .width()
                + 2.0 * thema::SPACE_3
                + 2.0 * thema::SIZE_BORDER;
            let antwort = ui.add(
                egui::Button::new(
                    RichText::new(name)
                        .font(schrift.clone())
                        .color(schriftfarbe),
                )
                .small()
                .min_size(egui::vec2(breite, thema::SIZE_CONTROL_SM))
                .fill(flaeche)
                .stroke(Stroke::new(thema::SIZE_BORDER, linie)),
            );
            antwort.widget_info(|| {
                egui::WidgetInfo::selected(egui::Role::RadioButton, true, gewaehlt, name)
            });
            if antwort.clicked() && !gewaehlt {
                *wert = *m;
                geaendert = true;
            }
        }
    });
    geaendert
}

/// Ein Eintrag der Seitenleiste.
pub struct NavEintrag<'a> {
    pub icon: Icon,
    pub name: &'a str,
    pub gewaehlt: bool,
}

/// Die Seitenleiste: Abschnittsname klein in Großbuchstaben, darunter eine
/// Zeile je Ziel (Zeichen und Name). Gewählt ist eine ruhige Fläche mit einem
/// Akzentbalken links. Unter `breakpoint.schmal` stehen nur die Zeichen, der
/// Name erscheint als Hinweis (Aufgabe 18). Gibt den Index des angeklickten
/// Eintrags zurück.
pub fn seitenleiste(
    ui: &mut egui::Ui,
    f: &Farben,
    titel: &str,
    eintraege: &[NavEintrag<'_>],
) -> Option<usize> {
    let mut wahl = None;
    let nur_zeichen = schmal(ui);
    let breite = if nur_zeichen {
        thema::SIZE_SEITENLEISTE_SCHMAL
    } else {
        thema::SIZE_SEITENLEISTE
    };
    egui::Panel::left("seitenleiste")
        .exact_size(breite)
        .resizable(false)
        .frame(
            egui::Frame::new()
                .fill(f.bg_panel)
                .inner_margin(egui::Margin::same(thema::SPACE_2 as i8)),
        )
        .show(ui, |ui| {
            // Die Zeilen stehen dicht an dicht wie in der Verwaltung.
            ui.spacing_mut().item_spacing.y = 0.0;
            if !nur_zeichen {
                abschnitt(ui, f, titel);
            }
            for (index, eintrag) in eintraege.iter().enumerate() {
                if navzeile(ui, f, eintrag, nur_zeichen).clicked() {
                    wahl = Some(index);
                }
            }
        });
    wahl
}

/// Der Abschnittsname über der Liste: eine Zeile hoch wie ein Eintrag.
fn abschnitt(ui: &mut egui::Ui, f: &Farben, titel: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), thema::SIZE_NAVZEILE),
        Sense::hover(),
    );
    ui.painter().text(
        egui::pos2(rect.left() + thema::SPACE_2, rect.center().y),
        egui::Align2::LEFT_CENTER,
        titel.to_uppercase(),
        egui::FontId::proportional(thema::FONT_SIZE_XS),
        thema::ton(f, Ton::Blass),
    );
}

/// Eine Zeile der Seitenleiste. Hand-Cursor und Name für den Screenreader
/// kommen von hier (AGENTS.md §2.12).
fn navzeile(
    ui: &mut egui::Ui,
    f: &Farben,
    eintrag: &NavEintrag<'_>,
    nur_zeichen: bool,
) -> egui::Response {
    let breite = ui.available_width();
    let (rect, antwort) =
        ui.allocate_exact_size(egui::vec2(breite, thema::SIZE_NAVZEILE), Sense::click());
    if ui.is_rect_visible(rect) {
        let flaeche = if eintrag.gewaehlt {
            f.bg_raised
        } else if antwort.hovered() {
            f.bg_hover
        } else {
            egui::Color32::TRANSPARENT
        };
        let (zeichenfarbe, schrift) = if eintrag.gewaehlt {
            (f.accent_default, f.text_default)
        } else {
            (f.text_muted, f.text_muted)
        };
        let maler = ui.painter();
        maler.rect_filled(rect, 0.0, flaeche);
        if eintrag.gewaehlt {
            maler.rect_filled(
                egui::Rect::from_min_size(
                    rect.min,
                    egui::vec2(thema::SIZE_AKZENTBALKEN, rect.height()),
                ),
                0.0,
                f.accent_default,
            );
        }
        // Schmal: nur das Zeichen, mittig. Sonst Zeichen und Name.
        let links = if nur_zeichen {
            rect.center().x - thema::FONT_SIZE_MD / 2.0
        } else {
            rect.left() + thema::SPACE_2
        };
        let zeichen_rect = egui::Rect::from_min_size(
            egui::pos2(links, rect.center().y - thema::FONT_SIZE_MD / 2.0),
            egui::vec2(thema::FONT_SIZE_MD, thema::FONT_SIZE_MD),
        );
        eintrag
            .icon
            .bild(thema::FONT_SIZE_MD, zeichenfarbe)
            .paint_at(ui, zeichen_rect);
        if !nur_zeichen {
            ui.painter().text(
                egui::pos2(zeichen_rect.right() + thema::SPACE_2, rect.center().y),
                egui::Align2::LEFT_CENTER,
                eintrag.name,
                egui::FontId::proportional(thema::FONT_SIZE_MD),
                schrift,
            );
        }
    }
    antwort.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, eintrag.name));
    let antwort = antwort.on_hover_cursor(egui::CursorIcon::PointingHand);
    if nur_zeichen {
        antwort.on_hover_text(eintrag.name)
    } else {
        antwort
    }
}

/// Ein Eingabefeld im Maß der Verwaltung: so hoch wie ein Bedienelement, 8 px
/// Polster und der Text mittig. egui setzt sonst das Polster der Knöpfe, dann
/// stünde der Text am oberen Rand.
pub fn eingabefeld(wert: &mut String) -> egui::TextEdit<'_> {
    egui::TextEdit::singleline(wert)
        .align(egui::Align2::LEFT_CENTER)
        .margin(egui::Margin::symmetric(
            (thema::SPACE_2 + thema::SIZE_BORDER) as i8,
            0,
        ))
}

/// Häkchen mit Beschriftung. egui setzt auf Häkchen keinen Hand-Cursor — hier
/// steht er (AGENTS.md §2.12).
pub fn haken(ui: &mut egui::Ui, an: &mut bool, beschriftung: &str) -> egui::Response {
    ui.add(egui::Checkbox::new(an, beschriftung))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// Eine Auswahlliste im Maß eines Eingabefelds (Aufgabe 16). Der Auslöser
/// trägt den Hand-Cursor, die Einträge ebenso.
pub fn auswahl(
    ui: &mut egui::Ui,
    f: &Farben,
    wert: &mut String,
    optionen: &[(String, String)],
    platzhalter: &str,
) -> egui::Response {
    let breite = feldbreite(ui);
    auswahl_mit(ui, f, wert, optionen, platzhalter, breite)
}

/// Wie [`auswahl`], aber mit fester Breite — für Filterleisten, in denen
/// mehrere Auswahlen nebeneinander stehen (Protokoll, Aufgabe 17).
pub fn auswahl_mit(
    ui: &mut egui::Ui,
    f: &Farben,
    wert: &mut String,
    optionen: &[(String, String)],
    platzhalter: &str,
    breite: f32,
) -> egui::Response {
    let anzeige = optionen
        .iter()
        .find(|(option, _)| option == wert)
        .map(|(_, anzeige)| anzeige.clone())
        .unwrap_or_else(|| platzhalter.to_string());
    let mut antwort = ui
        .add(
            egui::Button::new(RichText::new(anzeige.clone()).color(f.text_default))
                .fill(f.bg_input)
                .stroke(Stroke::new(thema::SIZE_BORDER, f.border_subtle))
                .min_size(egui::vec2(breite, thema::SIZE_CONTROL_MD)),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    // Das Zeichen fürs Aufklappen malen statt schreiben: die Schrift im
    // Fenster führt kein „▾“.
    if ui.is_rect_visible(antwort.rect) {
        let mitte = egui::pos2(
            antwort.rect.right() - thema::SPACE_2,
            antwort.rect.center().y,
        );
        let strich = Stroke::new(thema::SIZE_BORDER, f.text_faint);
        ui.painter().line_segment(
            [
                mitte + egui::vec2(-thema::SPACE_1, -thema::SPACE_1 / 2.0),
                mitte + egui::vec2(0.0, thema::SPACE_1 / 2.0),
            ],
            strich,
        );
        ui.painter().line_segment(
            [
                mitte + egui::vec2(0.0, thema::SPACE_1 / 2.0),
                mitte + egui::vec2(thema::SPACE_1, -thema::SPACE_1 / 2.0),
            ],
            strich,
        );
    }
    let mut gewaehlt = None;
    egui::Popup::menu(&antwort).show(|ui| {
        ui.set_min_width(breite);
        for (option, anzeige) in optionen {
            let klick = ui
                .selectable_label(option == wert, anzeige)
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
            if klick {
                gewaehlt = Some(option.clone());
            }
        }
    });
    if let Some(option) = gewaehlt {
        *wert = option;
        antwort.mark_changed();
    }
    let name = anzeige;
    antwort.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, &name));
    antwort
}

/// Eine Zeile einer Auswahl (Radio): Punkt, Name und optional ein Hinweis
/// rechts. `waehlbar == false` graut die Zeile aus — sie ist dann nicht zu
/// klicken und trägt den Hinweis als Text. Hand-Cursor und Name für den
/// Screenreader kommen von hier (AGENTS.md §2.12).
pub fn auswahlzeile(
    ui: &mut egui::Ui,
    f: &Farben,
    name: &str,
    hinweis: Option<&str>,
    gewaehlt: bool,
    waehlbar: bool,
) -> egui::Response {
    let hoehe = thema::SIZE_CONTROL_MD;
    let breite = ui.available_width();
    let (rect, antwort) = ui.allocate_exact_size(
        egui::vec2(breite, hoehe),
        if waehlbar {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let (schriftfarbe, punktfarbe) = if !waehlbar {
        (f.text_muted, f.border_strong)
    } else if gewaehlt {
        (f.text_default, f.accent_default)
    } else {
        (f.text_default, f.border_strong)
    };
    if ui.is_rect_visible(rect) {
        if waehlbar && antwort.hovered() {
            ui.painter().rect_filled(rect, 0.0, f.bg_hover);
        }
        let punkt = thema::FONT_SIZE_MD;
        let punkt_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + thema::SPACE_2 + punkt / 2.0, rect.center().y),
            egui::vec2(punkt, punkt),
        );
        ui.painter().circle_stroke(
            punkt_rect.center(),
            punkt / 2.0 - thema::SIZE_BORDER,
            egui::Stroke::new(thema::SIZE_BORDER, punktfarbe),
        );
        if gewaehlt {
            ui.painter().circle_filled(
                punkt_rect.center(),
                punkt / 2.0 - thema::SPACE_1,
                punktfarbe,
            );
        }
        // Name und Hinweis sind echte Beschriftungen — der Screenreader liest
        // sie, auch wenn die Zeile ausgegraut ist.
        let text_rect = egui::Rect::from_min_max(
            egui::pos2(punkt_rect.right() + thema::SPACE_2, rect.top()),
            egui::pos2(rect.right() - thema::SPACE_2, rect.bottom()),
        );
        let mut inhalt = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        inhalt.add(
            egui::Label::new(
                RichText::new(name)
                    .font(egui::FontId::proportional(thema::FONT_SIZE_MD))
                    .color(schriftfarbe),
            )
            .selectable(false),
        );
        if let Some(hinweis) = hinweis {
            inhalt.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(
                    egui::Label::new(
                        RichText::new(hinweis)
                            .font(egui::FontId::proportional(thema::FONT_SIZE_SM))
                            .color(f.text_muted),
                    )
                    .selectable(false),
                );
            });
        }
    }
    antwort.widget_info(|| {
        egui::WidgetInfo::selected(egui::Role::RadioButton, waehlbar, gewaehlt, name)
    });
    if waehlbar {
        antwort.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        antwort
    }
}

/// Zustand als kleines Schild: Quadrat in der Statusfarbe, Text daneben.
pub fn status(ui: &mut egui::Ui, f: &Farben, ton: Ton, beschriftung: &str) -> egui::Response {
    // Eine Zeile ist so hoch wie eine Textzeile — nicht wie ein Bedienelement
    // (`ui.horizontal` nähme sonst die Höhe eines Knopfes an).
    let hoehe = thema::FONT_SIZE_MD * thema::FONT_LINE_NORMAL;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), hoehe),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = thema::SPACE_1;
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(thema::SPACE_2, thema::SPACE_2), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, thema::ton(f, ton));
            ui.label(text(f, beschriftung, ton));
        },
    )
    .response
}

/// Ein Schild, dessen Text gekürzt wird, statt umzubrechen — für die
/// Kopfzeile, in der nur eine Zeile Platz hat (Aufgabe 18).
pub fn status_gekuerzt(
    ui: &mut egui::Ui,
    f: &Farben,
    ton: Ton,
    beschriftung: &str,
) -> egui::Response {
    let hoehe = thema::FONT_SIZE_MD * thema::FONT_LINE_NORMAL;
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), hoehe),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = thema::SPACE_1;
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(thema::SPACE_2, thema::SPACE_2), Sense::hover());
            ui.painter().rect_filled(rect, 0.0, thema::ton(f, ton));
            ui.add(egui::Label::new(text(f, beschriftung, ton)).truncate())
        },
    )
    .inner
    .on_hover_text(beschriftung)
}

/// Dasselbe Schild ohne Text (wenn der Platz fehlt) — der Text steht im
/// Hinweis und ist der zugängliche Name.
pub fn status_klein(ui: &mut egui::Ui, f: &Farben, ton: Ton, beschriftung: &str) -> egui::Response {
    let (rect, antwort) =
        ui.allocate_exact_size(egui::vec2(thema::SPACE_2, thema::SPACE_2), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, thema::ton(f, ton));
    let name = beschriftung.to_string();
    antwort.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &name));
    antwort.on_hover_text(beschriftung)
}

/// Ein Wert zum Weitergeben (Fingerabdruck, Kennung): umbrechend, daneben
/// ein Knopf zum Kopieren.
pub fn kopierbar(ui: &mut egui::Ui, f: &Farben, wert: &str, kopieren: &str) {
    ui.horizontal(|ui| {
        if icon_knopf(ui, f, Icon::Kopieren, kopieren, false).clicked() {
            ui.ctx().copy_text(wert.to_string());
        }
        ui.add(egui::Label::new(RichText::new(wert).monospace()).wrap());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Jedes neue Icon hat eine Datei: der Summentyp verhindert, dass eine
    /// Ansicht ein Icon verlangt, das es nicht gibt.
    #[test]
    fn jedes_icon_hat_eine_quelle() {
        for icon in [
            Icon::Paket,
            Icon::Satz,
            Icon::Gruppe,
            Icon::Geraet,
            Icon::Schluessel,
            Icon::Agent,
            Icon::Haken,
            Icon::Hinweis,
            Icon::Bearbeiten,
            Icon::Hochladen,
            Icon::Suchen,
        ] {
            let quelle: egui::ImageSource<'static> = icon.quelle();
            let _ = quelle;
        }
    }
}
