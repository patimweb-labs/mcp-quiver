// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Reine Funktionen der Oberfläche (AGENTS.md §2.4): welche Seite gilt,
//! welche Farbe ein Zustand hat, was in einem Feld steht, welche Schnipsel
//! ein Agent bekommt. Ohne egui, mit Tests — die Ansichten rufen nur hier auf.

use quiver_client::protokoll::{Status, Zeitraum};
use quiver_client::{
    protokoll, AnmeldeZustand, Aufruf, EinrichtZustand, EintragSicht, Modell, Verbindung,
    VorschauArt, VorschauEintrag, ZugaengeSicht, ZugangSicht, Zusammenfassung,
};
use quiver_domain::{
    link_aufloesen, zugangsname_gueltig, EintragId, FehlendesFeld, Feld, FeldArt, FeldLink, Quelle,
    Werte,
};
use quiver_protocol::EintragZustand;

use crate::modell::{Feldeingabe, ProtokollEingaben, Seite};
use crate::texte::{t, tf, Sprache};
use crate::thema::Ton;
use quiver_client::protokoll::Filter as ProtokollFilter;

/// Die Startseite: solange kein Server eingebunden ist, führt der Weg über
/// die Verbindung; danach steht die Arbeit (Tools) vorn.
pub fn startseite(modell: &Modell) -> Seite {
    match modell.verbindung {
        Verbindung::NichtEingebunden
        | Verbindung::WartetAufBrowser { .. }
        | Verbindung::NeuAnmelden { .. }
        | Verbindung::AnmeldeFehler { .. } => Seite::Verbindung,
        _ => Seite::Tools,
    }
}

/// Welche Seite nach einem Wechsel der Verbindung gilt. Fällt die Verbindung
/// auf „nicht eingebunden" zurück, ist die Verbindungsseite die einzige, die
/// noch etwas anbietet; gelingt das Einbinden, geht es zu den Tools.
pub fn seite_nach_wechsel(vorher: &Verbindung, nachher: &Verbindung, aktuell: Seite) -> Seite {
    match nachher {
        Verbindung::NichtEingebunden
        | Verbindung::WartetAufBrowser { .. }
        | Verbindung::NeuAnmelden { .. }
        | Verbindung::AnmeldeFehler { .. } => Seite::Verbindung,
        Verbindung::Verbindet { .. } | Verbindung::Verbunden { .. }
            if matches!(
                vorher,
                Verbindung::NichtEingebunden
                    | Verbindung::WartetAufBrowser { .. }
                    | Verbindung::NeuAnmelden { .. }
                    | Verbindung::AnmeldeFehler { .. }
            ) =>
        {
            if matches!(nachher, Verbindung::Verbunden { .. }) {
                Seite::Tools
            } else {
                Seite::Verbindung
            }
        }
        _ => aktuell,
    }
}

/// Bedeutung des Zustands eines Eintrags (ok, warn, error, leise).
///
/// „Am Gerät abgeschaltet“ ist **kein** Problem: es ist die Wahl des Users.
/// Ein unbekannter Zustand (neuere Gegenseite) bleibt ebenfalls neutral.
pub fn zustand_ton(zustand: &EintragZustand) -> Ton {
    match zustand {
        EintragZustand::Laeuft { .. } => Ton::Gut,
        EintragZustand::WartetAufWerte { .. } => Ton::Warnung,
        EintragZustand::Startet => Ton::Leise,
        EintragZustand::AmGeraetAus | EintragZustand::Unbekannt => Ton::Blass,
        EintragZustand::Fehler { .. } => Ton::Fehler,
    }
}

/// Das Abzeichen eines Eintrags. Beim Fehler steht hier nur „Fehler"; den
/// Text zeigt die Karte darunter (er kann lang sein).
pub fn zustand_text(sprache: Sprache, zustand: &EintragZustand) -> String {
    match zustand {
        EintragZustand::Laeuft { tools } => tf(sprache, "tools.laeuft", &[tools]),
        EintragZustand::WartetAufWerte { felder } if !felder.is_empty() => {
            tf(sprache, "tools.wartet_mit", &[&felder.join(", ")])
        }
        EintragZustand::WartetAufWerte { .. } => t(sprache, "tools.wartet"),
        EintragZustand::Startet => t(sprache, "tools.startet"),
        EintragZustand::AmGeraetAus => t(sprache, "tools.abgeschaltet"),
        EintragZustand::Unbekannt => t(sprache, "tools.unbekannt"),
        EintragZustand::Fehler { .. } => t(sprache, "tools.fehler"),
    }
}

/// Die Einträge, die der Filter „nur aktive“ durchlässt (Aufgabe 19).
pub fn sichtbare_eintraege(eintraege: &[EintragSicht], nur_aktive: bool) -> Vec<EintragSicht> {
    eintraege
        .iter()
        .filter(|eintrag| !nur_aktive || !eintrag.am_geraet_aus)
        .cloned()
        .collect()
}

/// Der Fehlertext eines Eintrags, falls es einen gibt.
pub fn fehlertext(zustand: &EintragZustand) -> Option<&str> {
    match zustand {
        EintragZustand::Fehler { text } => Some(text),
        _ => None,
    }
}

/// Welcher Knopf zur Anmeldung gehört.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnmeldeKnopf {
    /// Nichts zu tun – die Anmeldung läuft gerade.
    Keiner,
    /// „Anmelden“: der User löst sie im Fenster aus, nie ein Agent.
    Anmelden,
    /// „Erneut versuchen“ nach einem Fehler.
    Erneut,
    /// „Abmelden“ – mit Rückfrage.
    Abmelden,
}

/// Was die Karte zur Anmeldung eines Eintrags zeigt: Bedeutung, Text, Knopf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnmeldeZeile {
    pub ton: Ton,
    pub text: String,
    pub knopf: AnmeldeKnopf,
}

/// Die Anmeldezeile eines Eintrags (Aufgabe 10): abgemeldet und unbekannt
/// warnen, „läuft“ ist leise, angemeldet ist gut, ein Fehler zeigt seinen Text.
pub fn anmeldung_zeile(sprache: Sprache, zustand: &AnmeldeZustand) -> AnmeldeZeile {
    let warnung = AnmeldeZeile {
        ton: Ton::Warnung,
        text: t(sprache, "anmeldung.noetig"),
        knopf: AnmeldeKnopf::Anmelden,
    };
    match zustand {
        AnmeldeZustand::Unbekannt | AnmeldeZustand::Abgemeldet => warnung,
        AnmeldeZustand::Laeuft => AnmeldeZeile {
            ton: Ton::Leise,
            text: t(sprache, "anmeldung.laeuft"),
            knopf: AnmeldeKnopf::Keiner,
        },
        AnmeldeZustand::Angemeldet { konto } => AnmeldeZeile {
            ton: Ton::Gut,
            text: match konto {
                Some(konto) => tf(sprache, "anmeldung.angemeldet", &[konto]),
                None => t(sprache, "anmeldung.angemeldet_ohne_konto"),
            },
            knopf: AnmeldeKnopf::Abmelden,
        },
        AnmeldeZustand::Fehler { text } => AnmeldeZeile {
            ton: Ton::Fehler,
            text: text.clone(),
            knopf: AnmeldeKnopf::Erneut,
        },
    }
}

/// Welcher Knopf zur Einrichtung gehört (Aufgabe 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EinrichtKnopf {
    /// Nichts zu tun – die Einrichtung ist im Browser geöffnet.
    Keiner,
    /// „Einrichten“: ein normaler Knopf, kein Hauptknopf.
    Einrichten,
    /// „Erneut versuchen“ nach einem Fehler.
    Erneut,
    /// „Abbrechen“: die Einrichtung läuft und lässt sich beenden (Paket 3).
    Abbrechen,
}

/// Was die Karte zur Einrichtung eines Eintrags zeigt: Bedeutung, Text, Knopf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EinrichtZeile {
    pub ton: Ton,
    pub text: String,
    pub knopf: EinrichtKnopf,
}

/// Die Einrichtungszeile eines Eintrags (Aufgabe 11): der Knopf „Einrichten“
/// öffnet die Seite im Browser. Während sie offen ist, steht dort nur der
/// leise Hinweis; nach einem Fehler sein Text und „Erneut versuchen“.
pub fn einrichtung_zeile(sprache: Sprache, zustand: &EinrichtZustand) -> EinrichtZeile {
    match zustand {
        EinrichtZustand::Bereit => EinrichtZeile {
            ton: Ton::Leise,
            text: t(sprache, "einrichtung.bereit"),
            knopf: EinrichtKnopf::Einrichten,
        },
        // Läuft: der Knopf „Abbrechen“ beendet den Prozess im Browser
        // (Paket 3) – sonst bliebe eine hängende Einrichtung stehen.
        EinrichtZustand::Laeuft => EinrichtZeile {
            ton: Ton::Leise,
            text: t(sprache, "einrichtung.laeuft"),
            knopf: EinrichtKnopf::Abbrechen,
        },
        EinrichtZustand::Fehler { text } => EinrichtZeile {
            ton: Ton::Fehler,
            text: text.clone(),
            knopf: EinrichtKnopf::Erneut,
        },
    }
}

/// Ein Eintrag des Tray-Menüs. Beide Plattform-Teile (macOS/Windows und Linux)
/// zeichnen dieselbe Liste, deshalb steht sie hier als Daten und nicht dort.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEintrag {
    /// Zustandszeile, nicht klickbar.
    Zustand(String),
    /// „Fenster öffnen".
    Zeigen(String),
    /// „Beim Anmelden starten" mit Häkchen.
    Autostart {
        text: String,
        an: bool,
    },
    Trenner,
    /// „Beenden".
    Beenden(String),
}

/// Die Zustandszeile des Trays: „Verbunden · 4 Tools · 1 Agent" bzw.
/// „Nicht verbunden". Gezählt wird, was gerade läuft und wer gerade da ist.
pub fn tray_zeile(modell: &Modell, sprache: Sprache) -> String {
    // Ein anstehendes Update steht **vor** dem Zustand (Aufgabe 34). Der Satz
    // kommt aus der Vorgabe der Firma (oder aus dem Standardtext) und nennt
    // schon die Version; hier wird nichts nachgebaut.
    let update = modell
        .update
        .anstehend()
        .map(|(_, hinweis)| format!("{hinweis} · "))
        .unwrap_or_default();
    let zustand = match &modell.verbindung {
        Verbindung::NichtEingebunden => t(sprache, "tray.nicht_verbunden"),
        Verbindung::Verbindet { .. } => t(sprache, "tray.verbindet"),
        Verbindung::WartetAufBrowser { .. } => t(sprache, "tray.im_browser"),
        Verbindung::NeuAnmelden { .. } => t(sprache, "tray.neu_anmelden"),
        Verbindung::AnmeldeFehler { .. } => t(sprache, "tray.anmeldefehler"),
        Verbindung::Getrennt { .. } => t(sprache, "tray.getrennt"),
        Verbindung::Verbunden { .. } => {
            let tools: usize = modell
                .eintraege
                .iter()
                .map(|eintrag| eintrag.tools.len())
                .sum();
            let agenten = modell
                .agenten
                .iter()
                .filter(|agent| agent.verbunden)
                .count();
            let agenten = if agenten == 1 {
                t(sprache, "tray.agent_einer")
            } else {
                tf(sprache, "tray.agenten", &[&agenten])
            };
            format!(
                "{} · {} · {agenten}",
                t(sprache, "tray.verbunden"),
                tf(sprache, "tray.tools", &[&tools])
            )
        }
    };
    format!("{update}{zustand}")
}

/// Das Tray-Menü als Daten — reine Funktion über dem Modell. Die Reihenfolge
/// ist die des Menüs: Zustand, Fenster zeigen, Autostart, Trenner, Beenden.
///
/// Ist der Autostart von der Firma vorgegeben (Aufgabe 34), steht das Häkchen
/// danach und der Eintrag lässt sich nicht umschalten (siehe `app.rs`).
pub fn tray_menu(modell: &Modell, autostart: bool, sprache: Sprache) -> Vec<TrayEintrag> {
    let autostart_an = modell.vorgaben.autostart.unwrap_or(autostart);
    vec![
        TrayEintrag::Zustand(tray_zeile(modell, sprache)),
        TrayEintrag::Zeigen(t(sprache, "tray.fenster")),
        TrayEintrag::Autostart {
            text: t(sprache, "tray.autostart"),
            an: autostart_an,
        },
        TrayEintrag::Trenner,
        TrayEintrag::Beenden(t(sprache, "tray.beenden")),
    ]
}

/// Was auf der Kommandozeile steht.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Argumente {
    /// `--hintergrund`: nur Tray und Kern, das Fenster erst auf Wunsch.
    pub hintergrund: bool,
    /// `--headless`: nur der Kern, ohne Fenster und ohne Tray.
    pub kopflos: bool,
}

/// Die Argumente lesen. Unbekanntes wird nicht bemängelt — eine Anwendung mit
/// Tray darf ein zusätzliches `--flag` nicht am Start hindern.
pub fn argumente<S: AsRef<str>>(argumente: impl IntoIterator<Item = S>) -> Argumente {
    let mut werte = Argumente::default();
    for argument in argumente {
        match argument.as_ref() {
            "--hintergrund" => werte.hintergrund = true,
            "--headless" => werte.kopflos = true,
            _ => {}
        }
    }
    werte
}

/// Bedeutung und Text der Verbindung für Kopf und Statuskarte.
pub fn verbindung_status(sprache: Sprache, verbindung: &Verbindung) -> (Ton, String) {
    match verbindung {
        Verbindung::NichtEingebunden => (Ton::Blass, t(sprache, "verbindung.kein_server")),
        Verbindung::Verbindet { server } => {
            (Ton::Warnung, tf(sprache, "verbindung.verbindet", &[server]))
        }
        Verbindung::WartetAufBrowser { server, .. } => (
            Ton::Warnung,
            tf(sprache, "verbindung.im_browser", &[server]),
        ),
        Verbindung::Verbunden { server, .. } => {
            (Ton::Gut, tf(sprache, "verbindung.verbunden", &[server]))
        }
        Verbindung::Getrennt { server, .. } => (
            Ton::Fehler,
            tf(sprache, "verbindung.getrennt_von", &[server]),
        ),
        // Kurz halten: die Kopfzeile ist schmal und würde sonst abschneiden.
        Verbindung::NeuAnmelden { .. } => {
            (Ton::Warnung, t(sprache, "verbindung.neu_anmelden_kurz"))
        }
        Verbindung::AnmeldeFehler { .. } => {
            (Ton::Fehler, t(sprache, "verbindung.anmeldefehler_titel"))
        }
    }
}

/// Der Satz zu einem maschinenlesbaren Ablehnungsgrund (Aufgabe 20/22).
/// Unbekannt heißt: wie `neu-anmelden` — lieber zu vorsichtig als zu spät.
pub fn grund_text(sprache: Sprache, grund: quiver_protocol::AnmeldeGrund) -> String {
    match grund {
        quiver_protocol::AnmeldeGrund::Gesperrt => t(sprache, "verbindung.grund.gesperrt"),
        quiver_protocol::AnmeldeGrund::KeinZugriff => t(sprache, "verbindung.grund.kein_zugriff"),
        quiver_protocol::AnmeldeGrund::NeuAnmelden => t(sprache, "verbindung.grund.neu_anmelden"),
    }
}

/// Was einem Eintrag fehlt, als ein Satz: die Erklärung des Felds, sonst sein
/// Name. (Der Name steht schon im Abzeichen.)
pub fn fehlende_texte(fehlende: &[FehlendesFeld]) -> String {
    fehlende
        .iter()
        .map(|feld| {
            if feld.beschreibung.trim().is_empty() {
                feld.name.clone()
            } else {
                feld.beschreibung.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Die Beschriftung eines Felds: `titel`, sonst der Name (Aufgabe 16).
pub fn feld_anzeige(feld: &Feld) -> String {
    feld.titel
        .as_deref()
        .map(str::trim)
        .filter(|titel| !titel.is_empty())
        .unwrap_or(&feld.name)
        .to_string()
}

/// Die dargestellte Art eines Felds. Ein Geheimnis bleibt ein Geheimnis,
/// egal welche Art gespeichert ist (Aufgabe 16).
pub fn feld_art(feld: &Feld) -> FeldArt {
    if feld.quelle == Quelle::Geheim {
        FeldArt::Passwort
    } else {
        feld.art.clone()
    }
}

/// Ist ein Schalterwert „an“? Die gespeicherten Werte sind `an` und `aus`;
/// ältere oder handgeschriebene Daten dürfen auch `true`, `1` oder `ja` sein.
pub fn schalter_an(wert: &str) -> bool {
    matches!(
        wert.trim().to_ascii_lowercase().as_str(),
        "an" | "true" | "1" | "ja" | "yes"
    )
}

/// Der Wert eines Schalters: gespeichert, sonst der Standard, sonst `aus`.
pub fn schalter_wert(gespeichert: &str, standard: Option<&str>) -> String {
    if !gespeichert.trim().is_empty() {
        if schalter_an(gespeichert) {
            "an".to_string()
        } else {
            "aus".to_string()
        }
    } else if standard.is_some_and(schalter_an) {
        "an".to_string()
    } else {
        "aus".to_string()
    }
}

/// Was an einem eingegebenen Wert nicht stimmt (Aufgabe 16).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WertFehler {
    Zahl,
    Adresse,
}

/// Prüft einen nicht-leeren Wert gegen seine Art. Leere Werte sind Sache der
/// Pflichtprüfung in [`sperrgrund`].
pub fn wert_fehler(art: &FeldArt, wert: &str) -> Option<WertFehler> {
    match art {
        FeldArt::Zahl if !wert.chars().all(|zeichen| zeichen.is_ascii_digit()) => {
            Some(WertFehler::Zahl)
        }
        FeldArt::Url if !(wert.starts_with("http://") || wert.starts_with("https://")) => {
            Some(WertFehler::Adresse)
        }
        _ => None,
    }
}

/// Warum „Speichern“ gesperrt ist: fehlende Pflichtfelder und ungültige Werte.
/// `None` heißt: alles in Ordnung.
pub fn sperrgrund(sprache: Sprache, eingaben: &[Feldeingabe]) -> Option<String> {
    let mut fehlend = Vec::new();
    let mut zahlen = Vec::new();
    let mut adressen = Vec::new();
    for eingabe in eingaben {
        let wert = eingabe.wert.trim();
        let leer = wert.is_empty() && !(eingabe.geheim && eingabe.gesetzt);
        if eingabe.pflicht && leer {
            fehlend.push(eingabe.anzeige.clone());
            continue;
        }
        if wert.is_empty() {
            continue;
        }
        match wert_fehler(&eingabe.art, wert) {
            Some(WertFehler::Zahl) => zahlen.push(eingabe.anzeige.clone()),
            Some(WertFehler::Adresse) => adressen.push(eingabe.anzeige.clone()),
            None => {}
        }
    }
    let mut teile = Vec::new();
    if !fehlend.is_empty() {
        teile.push(tf(sprache, "tools.es_fehlt", &[&fehlend.join(", ")]));
    }
    if !zahlen.is_empty() {
        teile.push(tf(sprache, "tools.ungueltige_zahl", &[&zahlen.join(", ")]));
    }
    if !adressen.is_empty() {
        teile.push(tf(
            sprache,
            "tools.ungueltige_adresse",
            &[&adressen.join(", ")],
        ));
    }
    (!teile.is_empty()).then(|| teile.join(" · "))
}

/// Die Felder eines Eintrags als Eingaben: lokale Werte vorbelegt, Geheimnisse
/// leer (gesetzte zeigen „••••" nur als Hinweis).
pub fn eingaben(eintrag: &EintragSicht) -> Vec<Feldeingabe> {
    eintrag
        .felder
        .iter()
        .map(|feld| {
            let geheim = feld.quelle == Quelle::Geheim;
            let art = feld_art(feld);
            let gespeichert = eintrag
                .lokale_werte
                .get(&feld.name)
                .map(String::as_str)
                .unwrap_or_default();
            let wert = if geheim {
                String::new()
            } else if art == FeldArt::Schalter {
                schalter_wert(gespeichert, feld.standard.as_deref())
            } else {
                gespeichert.to_string()
            };
            Feldeingabe {
                name: feld.name.clone(),
                anzeige: feld_anzeige(feld),
                beschreibung: feld.beschreibung.clone(),
                geheim,
                pflicht: feld.pflicht,
                gesetzt: geheim && eintrag.geheim_gesetzt.contains(&feld.name),
                wert,
                link: feld.link.clone(),
                art,
            }
        })
        .collect()
}

/// Die Werte, die der Dialog nicht hergibt: die Standardwerte der Felder
/// (Aufgabe 13). Was der Server selbst einsetzt, kennt das Fenster nicht —
/// `EintragSicht` führt nur Client- und Geheimfelder.
pub fn link_basis(eintrag: &EintragSicht) -> Werte {
    link_basis_aus_feldern(&eintrag.felder)
}

/// Wie [`link_basis`], aber für eine beliebige Feldliste — etwa die Felder
/// einer Zugangsliste (Aufgabe 14).
pub fn link_basis_aus_feldern(felder: &[quiver_domain::Feld]) -> Werte {
    felder
        .iter()
        .filter_map(|feld| {
            let wert = feld.standard.as_deref()?.trim();
            (!wert.is_empty()).then(|| (feld.name.clone(), wert.to_string()))
        })
        .collect()
}

/// Die Werte, mit denen ein Hilfe-Link aufgelöst wird: erst die Basis, dann
/// die **aktuellen** Eingaben des Dialogs — sie gewinnen (Aufgabe 13: „nicht
/// nur die gespeicherten Werte"). Leere Eingaben zählen nicht.
pub fn link_werte(eingaben: &[Feldeingabe], basis: &Werte) -> Werte {
    let mut werte = basis.clone();
    for eingabe in eingaben {
        let wert = eingabe.wert.trim();
        if !wert.is_empty() {
            werte.insert(eingabe.name.clone(), wert.to_string());
        }
    }
    werte
}

/// Die Adresse des Hilfe-Links oder `None`, solange ein verwendeter
/// Platzhalter ohne Wert ist (dann bleibt der Knopf gesperrt).
pub fn link_adresse(link: &FeldLink, eingaben: &[Feldeingabe], basis: &Werte) -> Option<String> {
    link_aufloesen(link, &link_werte(eingaben, basis))
}

/// Welche Felder noch fehlen, damit der Link aufgeht: die Platzhalter in
/// `link.url`, für die kein Wert da ist. Leer heißt: der Link ist fertig.
pub fn link_fehlende(link: &FeldLink, eingaben: &[Feldeingabe], basis: &Werte) -> Vec<String> {
    let werte = link_werte(eingaben, basis);
    platzhalter(&link.url)
        .into_iter()
        .filter(|name| !werte.contains_key(name))
        .collect()
}

/// Der Tooltip des gesperrten Knopfs: „Erst <Feld> eintragen".
pub fn link_hinweis(fehlende: &[String], sprache: Sprache) -> String {
    tf(sprache, "tools.link_erst", &[&fehlende.join(", ")])
}

/// Die Namen in `${...}` einer Vorlage, in der Reihenfolge des Auftretens und
/// ohne Doppelte.
fn platzhalter(text: &str) -> Vec<String> {
    let mut namen: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        let nach = &rest[start + 2..];
        let Some(ende) = nach.find('}') else {
            break;
        };
        let name = &nach[..ende];
        if !namen.iter().any(|vorhanden| vorhanden == name) {
            namen.push(name.to_string());
        }
        rest = &nach[ende + 1..];
    }
    namen
}

/// Die Eingaben aufteilen in lokale und geheime Werte. Ein leeres Geheimnis
/// heißt „unverändert" und wird nicht geschickt; ein leerer lokaler Wert
/// löscht das Feld am Gerät.
pub fn werte_splitten(eingaben: &[Feldeingabe]) -> (Werte, Werte) {
    let mut lokal = Werte::new();
    let mut geheim = Werte::new();
    for eingabe in eingaben {
        let wert = eingabe.wert.trim().to_string();
        if eingabe.geheim {
            if !wert.is_empty() {
                geheim.insert(eingabe.name.clone(), wert);
            }
        } else {
            lokal.insert(eingabe.name.clone(), wert);
        }
    }
    (lokal, geheim)
}

/// Die Eingaben eines Zugangs (Aufgabe 14): die Felder der Zugangsliste,
/// vorbelegt mit den nicht-geheimen Werten. Geheimes wird nie angezeigt,
/// nur „gesetzt“.
pub fn zugang_eingaben(zugaenge: &ZugaengeSicht, name: Option<&str>) -> Vec<Feldeingabe> {
    let zugang = name.and_then(|name| zugaenge.liste.iter().find(|zugang| zugang.name == name));
    zugaenge
        .felder
        .iter()
        .map(|feld| {
            let geheim = feld.quelle == Quelle::Geheim;
            let art = feld_art(feld);
            let gespeichert = zugang
                .and_then(|zugang| zugang.werte.get(&feld.name))
                .map(String::as_str)
                .unwrap_or_default();
            let wert = if geheim {
                String::new()
            } else if art == FeldArt::Schalter {
                schalter_wert(gespeichert, feld.standard.as_deref())
            } else {
                gespeichert.to_string()
            };
            Feldeingabe {
                name: feld.name.clone(),
                anzeige: feld_anzeige(feld),
                beschreibung: feld.beschreibung.clone(),
                geheim,
                pflicht: feld.pflicht,
                gesetzt: geheim
                    && zugang.is_some_and(|zugang| zugang.geheim_gesetzt.contains(&feld.name)),
                wert,
                link: feld.link.clone(),
                art,
            }
        })
        .collect()
}

/// Die Überschrift einer Zugangsliste: „Verbindungen (2)“.
pub fn zugang_beschriftung(sprache: Sprache, zugaenge: &ZugaengeSicht) -> String {
    tf(
        sprache,
        "zugaenge.kopf",
        &[&zugaenge.titel, &zugaenge.liste.len()],
    )
}

/// Der Knopf, der einen Zugang anlegt: „Verbindung hinzufügen“.
pub fn zugang_hinzufuegen(sprache: Sprache, zugaenge: &ZugaengeSicht) -> String {
    tf(sprache, "zugaenge.hinzufuegen", &[&zugaenge.einzahl])
}

/// Die Überschrift des Zugangs-Dialogs: anlegen oder bearbeiten.
pub fn zugang_titel(sprache: Sprache, einzahl: &str, neu: bool) -> String {
    let schluessel = if neu {
        "zugaenge.anlegen_titel"
    } else {
        "zugaenge.bearbeiten_titel"
    };
    tf(sprache, schluessel, &[&einzahl])
}

/// Eine Zeile unter dem Namen eines Zugangs: ein bis zwei nicht-geheime
/// Werte in der Reihenfolge der Felder (z. B. `benutzer@host`).
pub fn zugang_werte_zeile(felder: &[quiver_domain::Feld], zugang: &ZugangSicht) -> String {
    felder
        .iter()
        .filter(|feld| feld.quelle != Quelle::Geheim)
        .filter_map(|feld| zugang.werte.get(&feld.name))
        .map(|wert| wert.trim())
        .filter(|wert| !wert.is_empty())
        .take(2)
        .collect::<Vec<_>>()
        .join("@")
}

/// Warum dieser Name für einen Zugang nicht geht – `None` heißt: in Ordnung.
/// Dieselbe Regel wie [`quiver_domain::zugangsname_gueltig`], zusätzlich
/// eindeutig: `vorhandene` sind die Namen der schon gespeicherten Zugänge.
pub fn zugang_name_fehler(name: &str, vorhandene: &[String]) -> Option<&'static str> {
    let name = name.trim();
    if name.is_empty() {
        return Some("zugaenge.name_fehlt");
    }
    if !zugangsname_gueltig(name) {
        return Some("zugaenge.name_ungueltig");
    }
    if vorhandene
        .iter()
        .any(|vorhanden| quiver_domain::zugangsname_gleich(vorhanden, name))
    {
        return Some("zugaenge.name_doppelt");
    }
    None
}

/// Warum das Passwort nicht reicht – `None` heißt: es geht (Aufgabe 15).
///
/// Die Regel steht an einer Stelle: mindestens `PASSWORT_MIN` Zeichen, sonst
/// lässt sich die Sicherung später nicht mehr öffnen.
pub fn export_passwort_fehler(passwort: &str, wiederholung: &str) -> Option<&'static str> {
    let passwort = passwort.trim();
    if passwort.is_empty() {
        return Some("einstellungen.passwort_fehlt");
    }
    if passwort.chars().count() < quiver_client::einstellungen::PASSWORT_MIN {
        return Some("einstellungen.passwort_kurz");
    }
    if passwort != wiederholung.trim() {
        return Some("einstellungen.passwort_ungleich");
    }
    None
}

/// Wie [`export_passwort_fehler`], aber ohne Wiederholung: beim Import gibt es
/// nur ein Feld.
pub fn import_passwort_fehler(passwort: &str) -> Option<&'static str> {
    let passwort = passwort.trim();
    if passwort.is_empty() {
        return Some("einstellungen.passwort_fehlt");
    }
    if passwort.chars().count() < quiver_client::einstellungen::PASSWORT_MIN {
        return Some("einstellungen.passwort_kurz");
    }
    None
}

/// Eine Zeile der Import-Vorschau: was neu ist, was sich ändert, was
/// übersprungen wird (Aufgabe 15).
pub fn vorschau_detail(sprache: Sprache, eintrag: &VorschauEintrag) -> String {
    if eintrag.art == VorschauArt::Uebersprungen {
        return t(sprache, "einstellungen.vorschau_uebersprungen");
    }
    let mut teile = vec![t(
        sprache,
        match eintrag.art {
            VorschauArt::Neu => "einstellungen.vorschau_neu",
            _ => "einstellungen.vorschau_aendert",
        },
    )];
    if eintrag.neue > 0 {
        teile.push(tf(sprache, "einstellungen.neue_werte", &[&eintrag.neue]));
    }
    if eintrag.geaenderte > 0 {
        teile.push(tf(
            sprache,
            "einstellungen.geaenderte_werte",
            &[&eintrag.geaenderte],
        ));
    }
    if eintrag.zugaenge > 0 {
        teile.push(tf(
            sprache,
            "einstellungen.zugaenge_anzahl",
            &[&eintrag.zugaenge],
        ));
    }
    // Die Wahl „abgeschaltet“ geht mit (Aufgabe 19) – aus bleibt aus.
    if eintrag.am_geraet_aus {
        teile.push(t(sprache, "einstellungen.vorschau_aus"));
    }
    teile.join(" · ")
}

/// Der Hinweis nach Export oder Import, z. B.
/// „6 Einträge exportiert (mit Geheimnissen)“.
pub fn ergebnis_text(
    sprache: Sprache,
    importiert: bool,
    zusammenfassung: &Zusammenfassung,
) -> String {
    let mut text = match (importiert, zusammenfassung.eintraege) {
        (false, 1) => t(sprache, "einstellungen.exportiert_einer"),
        (false, anzahl) => tf(sprache, "einstellungen.exportiert", &[&anzahl]),
        (true, 1) => t(sprache, "einstellungen.importiert_einer"),
        (true, anzahl) => tf(sprache, "einstellungen.importiert", &[&anzahl]),
    };
    if !importiert && zusammenfassung.mit_geheimnissen() {
        text.push(' ');
        text.push_str(&t(sprache, "einstellungen.mit_geheimnissen"));
    }
    if !zusammenfassung.uebersprungen.is_empty() {
        text.push_str(" · ");
        text.push_str(&tf(
            sprache,
            "einstellungen.uebersprungen",
            &[&zusammenfassung.uebersprungen.len()],
        ));
    }
    text
}

/// Die Shell-Variable, in der das Token für **einen** Befehl steht. Es wird
/// nie als Prozessargument übergeben (Paket 3): jedes Argument steht in der
/// Prozessliste und in der Shell-Historie.
pub const TOKEN_VARIABLE: &str = "QUIVER_TOKEN";

/// Vorspann jedes Schnipsels: das Token verdeckt einlesen (`read -s`).
fn token_einlesen() -> String {
    format!("read -rs {TOKEN_VARIABLE} && ")
}

/// Nachsatz: die Variable wieder vergessen.
fn token_verwerfen() -> String {
    format!("; unset {TOKEN_VARIABLE}")
}

/// Der Befehl für pi 1.5 (`pi mcp add --help`: `--url <url>`, `--header <KEY=VALUE>`).
///
/// Das Token kommt verdeckt aus `read -s` und steht nur als `$QUIVER_TOKEN`
/// im Argument – nie der Wert selbst.
pub fn schnipsel_pi(endpunkt: &str) -> String {
    format!(
        "{}{}{}",
        token_einlesen(),
        format_args!(
            "pi mcp add quiver --url {endpunkt} --header \"Authorization=Bearer ${TOKEN_VARIABLE}\""
        ),
        token_verwerfen()
    )
}

/// Der Befehl für Claude Code (`claude mcp add --help`: `--transport http`,
/// `--header <header>`).
pub fn schnipsel_claude(endpunkt: &str) -> String {
    format!(
        "{}{}{}",
        token_einlesen(),
        format_args!(
            "claude mcp add --transport http quiver {endpunkt} --header \"Authorization: Bearer ${TOKEN_VARIABLE}\""
        ),
        token_verwerfen()
    )
}

/// Die Einstellung für OpenCode (`opencode.json`, Schlüssel `mcp`).
///
/// Die Datei entsteht mit `printf`: das Token wird eingesetzt, ohne dass es in
/// einem Argument steht.
pub fn schnipsel_opencode(endpunkt: &str) -> String {
    format!(
        "{}{}{}",
        token_einlesen(),
        format_args!(
            "printf '{{\"mcp\":{{\"quiver\":{{\"type\":\"remote\",\"url\":\"{endpunkt}\",\"headers\":{{\"Authorization\":\"Bearer %s\"}}}}}}}}' \"${TOKEN_VARIABLE}\" > opencode.json"
        ),
        token_verwerfen()
    )
}

/// Die Einstellung für GitHub Copilot in VS Code (`.vscode/mcp.json` bzw. die
/// Benutzer-`mcp.json`, Schlüssel `servers`).
pub fn schnipsel_copilot(endpunkt: &str) -> String {
    format!(
        "{}{}{}",
        token_einlesen(),
        format_args!(
            "printf '{{\"servers\":{{\"quiver\":{{\"type\":\"http\",\"url\":\"{endpunkt}\",\"headers\":{{\"Authorization\":\"Bearer %s\"}}}}}}}}' \"${TOKEN_VARIABLE}\" > .vscode/mcp.json"
        ),
        token_verwerfen()
    )
}

/// Der Befehl für Hermes. `--auth header` fragt das Token verdeckt ab und legt
/// es in `~/.hermes/.env` (`MCP_QUIVER_API_KEY`) ab; in `config.yaml` steht nur
/// der Verweis darauf (geprüft im Quelltext von Hermes 0.21, `mcp_config.py`).
/// Auch hier steht kein Token in einem Argument.
pub fn schnipsel_hermes(endpunkt: &str) -> String {
    format!("hermes mcp add quiver --url {endpunkt} --auth header")
}

/// Dasselbe als JSON für andere MCP-Clients – zum Einfügen in die eigene
/// Konfiguration. Das Token wird verdeckt eingelesen und eingesetzt.
pub fn schnipsel_json(endpunkt: &str) -> String {
    format!(
        "{}{}{}",
        token_einlesen(),
        format_args!(
            "printf '{{\"mcpServers\":{{\"quiver\":{{\"type\":\"http\",\"url\":\"{endpunkt}\",\"headers\":{{\"Authorization\":\"Bearer %s\"}}}}}}}}' \"${TOKEN_VARIABLE}\""
        ),
        token_verwerfen()
    )
}

/// Ein Schnipsel zum Einrichten: wohin er gehört und was zu kopieren ist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schnipsel {
    /// Die Zeile, die sagt, wo man ihn einfügt.
    pub ort: String,
    /// Der Text, der kopiert wird.
    pub text: String,
}

/// Die Schnipsel für eine Agent-Art. Hermes bekommt drei: den Befehl, das
/// Token für seine Rückfrage und den Prüfbefehl danach. „Andere" bekommt das
/// allgemeine JSON.
///
/// Kein Schnipsel enthält das Token als **Wert** in einem Befehl (Paket 3):
/// die Shell liest es verdeckt ein (`read -s`). Nur der Hermes-Schnipsel für
/// die Rückfrage zeigt es – dort wird es an der Eingabeaufforderung eingefügt,
/// nicht als Argument.
pub fn schnipsel(sprache: Sprache, art: &AgentArt, endpunkt: &str, token: &str) -> Vec<Schnipsel> {
    let ort = |schluessel: &str| t(sprache, schluessel);
    match art {
        AgentArt::Pi => vec![Schnipsel {
            ort: ort("agenten.ort.shell"),
            text: schnipsel_pi(endpunkt),
        }],
        AgentArt::ClaudeCode => vec![Schnipsel {
            ort: ort("agenten.ort.shell"),
            text: schnipsel_claude(endpunkt),
        }],
        AgentArt::OpenCode => vec![Schnipsel {
            ort: ort("agenten.ort.opencode"),
            text: schnipsel_opencode(endpunkt),
        }],
        AgentArt::Copilot => vec![Schnipsel {
            ort: ort("agenten.ort.copilot"),
            text: schnipsel_copilot(endpunkt),
        }],
        AgentArt::Hermes => vec![
            Schnipsel {
                ort: ort("agenten.ort.hermes"),
                text: schnipsel_hermes(endpunkt),
            },
            Schnipsel {
                ort: ort("agenten.ort.hermes_token"),
                text: token.to_string(),
            },
            Schnipsel {
                ort: ort("agenten.ort.hermes_test"),
                text: "hermes mcp test quiver".to_string(),
            },
        ],
        AgentArt::Andere(_) => vec![Schnipsel {
            ort: ort("agenten.ort.andere"),
            text: schnipsel_json(endpunkt),
        }],
    }
}

/// Die Art eines Agenten (Summentyp). Bekannte Arten haben einen festen Namen,
/// unter dem sie gekoppelt werden; „Andere" trägt seinen Namen selbst.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentArt {
    Pi,
    ClaudeCode,
    OpenCode,
    Copilot,
    Hermes,
    Andere(String),
}

impl AgentArt {
    /// Die wählbaren Arten in der Reihenfolge der Auswahl.
    pub fn alle() -> Vec<AgentArt> {
        vec![
            AgentArt::Pi,
            AgentArt::ClaudeCode,
            AgentArt::OpenCode,
            AgentArt::Copilot,
            AgentArt::Hermes,
            AgentArt::Andere(String::new()),
        ]
    }

    /// Der Name, unter dem der Agent gekoppelt wird.
    pub fn name(&self) -> String {
        match self {
            AgentArt::Pi => "pi".to_string(),
            AgentArt::ClaudeCode => "claude-code".to_string(),
            AgentArt::OpenCode => "opencode".to_string(),
            AgentArt::Copilot => "copilot".to_string(),
            AgentArt::Hermes => "hermes".to_string(),
            AgentArt::Andere(name) => name.trim().to_string(),
        }
    }

    /// Anzeigename in der Auswahl. Markennamen bleiben, wie sie heißen; nur
    /// „Andere" wird übersetzt.
    pub fn anzeige(&self, sprache: Sprache) -> String {
        match self {
            AgentArt::Pi => "pi".to_string(),
            AgentArt::ClaudeCode => "Claude Code".to_string(),
            AgentArt::OpenCode => "OpenCode".to_string(),
            AgentArt::Copilot => "GitHub Copilot".to_string(),
            AgentArt::Hermes => "Hermes".to_string(),
            AgentArt::Andere(_) => t(sprache, "agenten.art_andere"),
        }
    }

    /// Bekannte Art: der Name steht fest.
    pub fn ist_fest(&self) -> bool {
        !matches!(self, AgentArt::Andere(_))
    }

    /// Gleiche Art? „Andere" zählt als eine Art, egal welcher Name darin steht.
    pub fn gleich(&self, andere: &AgentArt) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(andere)
    }

    /// Aus einem gekoppelten Namen die Art erkennen (ohne Groß-/
    /// Kleinschreibung). Unbekanntes wird zu „Andere".
    pub fn aus_name(name: &str) -> AgentArt {
        match name.trim().to_lowercase().as_str() {
            "pi" => AgentArt::Pi,
            "claude-code" | "claude" => AgentArt::ClaudeCode,
            "opencode" => AgentArt::OpenCode,
            "copilot" | "github-copilot" => AgentArt::Copilot,
            "hermes" => AgentArt::Hermes,
            _ => AgentArt::Andere(name.trim().to_string()),
        }
    }

    /// Der einzutragende Name: fest bei bekannter Art, sonst die Eingabe.
    pub fn einzutragen(&self, eingabe: &str) -> String {
        match self {
            AgentArt::Andere(_) => eingabe.trim().to_string(),
            fest => fest.name(),
        }
    }
}

/// Eine Zeile der Art-Auswahl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtZeile {
    pub art: AgentArt,
    /// Schon gekoppelt: dann nicht mehr wählbar.
    pub gekoppelt: bool,
}

/// Die Art-Auswahl über den schon gekoppelten Namen: bekannte Arten, die es
/// bereits gibt, sind ausgegraut; „Andere" bleibt immer wählbar.
pub fn arten_auswahl(gekoppelte: &[String]) -> Vec<ArtZeile> {
    AgentArt::alle()
        .into_iter()
        .map(|art| {
            let gekoppelt = art.ist_fest() && name_vergeben(&art.name(), gekoppelte);
            ArtZeile { art, gekoppelt }
        })
        .collect()
}

/// Die erste noch freie Art — damit ein Dialog nicht mit einer ausgegrauten
/// Zeile beginnt. Ist alles Bekannte gekoppelt, ist es „Andere".
pub fn erste_freie(gekoppelte: &[String]) -> AgentArt {
    arten_auswahl(gekoppelte)
        .into_iter()
        .find(|zeile| !zeile.gekoppelt)
        .map(|zeile| zeile.art)
        .unwrap_or_else(|| AgentArt::Andere(String::new()))
}

/// Ist dieser Name schon vergeben? Ohne Groß-/Kleinschreibung und ohne
/// Leerzeichen am Rand – dieselbe Regel wie in [`quiver_client::Kern::agent_koppeln`].
pub fn name_vergeben(name: &str, gekoppelte: &[String]) -> bool {
    let gesucht = name.trim().to_lowercase();
    !gesucht.is_empty()
        && gekoppelte
            .iter()
            .any(|vorhanden| vorhanden.trim().to_lowercase() == gesucht)
}

/// Name, unter dem sich das Gerät beim Server vorstellt. Ohne
/// Plattformabfrage (AGENTS.md §2.10): erst eine eigene Umgebungsvariable,
/// dann, was die Umgebung ohnehin mitbringt.
pub fn geraetename() -> String {
    geraetename_aus(|name| std::env::var(name).ok())
}

/// Wie [`geraetename`], aber mit einer nachschlagbaren Umgebung (Test).
pub fn geraetename_aus(lesen: impl Fn(&str) -> Option<String>) -> String {
    for name in ["MCP_QUIVER_GERAET", "HOSTNAME", "COMPUTERNAME"] {
        if let Some(wert) = lesen(name) {
            let wert = wert.trim();
            if !wert.is_empty() {
                return wert.to_string();
            }
        }
    }
    "quiver".to_string()
}

// ------------------------------------------------- Protokoll (Aufgabe 17)

/// Was die Filterleiste einstellt, als Abfrage für den Kern. Leere Namen
/// heißen „alle“.
pub fn protokoll_filter(eingaben: &ProtokollEingaben) -> ProtokollFilter {
    ProtokollFilter {
        eintrag: (!eingaben.eintrag.trim().is_empty())
            .then(|| EintragId::neu(eingaben.eintrag.trim())),
        tool: nicht_leer(&eingaben.tool),
        agent: nicht_leer(&eingaben.agent),
        nur_fehler: eingaben.nur_fehler,
        seit: Some(protokoll::seit(eingaben.zeitraum)),
        anzahl: None,
    }
}

/// Der Freitext des Reiters „Tool-Logs“ als Abfrage.
pub fn log_filter(suche: &str) -> quiver_client::protokoll::LogFilter {
    quiver_client::protokoll::LogFilter {
        enthaelt: nicht_leer(suche),
        seit: None,
        anzahl: None,
    }
}

fn nicht_leer(wert: &str) -> Option<String> {
    let wert = wert.trim();
    (!wert.is_empty()).then(|| wert.to_string())
}

/// Die Bedeutung eines Ergebnisses: gut, Fehler, Warnung.
pub fn status_ton(status: Status) -> Ton {
    match status {
        Status::Ok => Ton::Gut,
        Status::Fehler => Ton::Fehler,
        Status::Abgelehnt => Ton::Warnung,
    }
}

/// Das Abzeichen eines Aufrufs.
pub fn status_text(sprache: Sprache, status: Status) -> String {
    match status {
        Status::Ok => t(sprache, "protokoll.status.ok"),
        Status::Fehler => t(sprache, "protokoll.status.fehler"),
        Status::Abgelehnt => t(sprache, "protokoll.status.abgelehnt"),
    }
}

/// Die Uhrzeit eines Aufrufs (`2026-01-02T03:04:05Z` → `03:04:05`).
pub fn zeit_text(zeit: &str) -> String {
    let rest = zeit.split_once('T').map(|(_, rest)| rest).unwrap_or(zeit);
    rest.chars().take(8).collect()
}

/// Das Datum eines Aufrufs (`2026-01-02T03:04:05Z` → `2026-01-02`) — steht
/// in der Protokollzeile im Hinweis, nicht in der Spalte (Aufgabe 18).
pub fn datum_text(zeit: &str) -> String {
    zeit.split_once('T')
        .map_or(zeit, |(datum, _)| datum)
        .to_string()
}

/// Das Werkzeug einer Zeile: `eintrag › tool` — der Eintrag steht einmal,
/// nicht doppelt im Werkzeugnamen (Aufgabe 18).
pub fn tool_text(aufruf: &Aufruf) -> String {
    let eintrag = aufruf.eintrag.als_str();
    // Nur das eigene Präfix abtrennen – keine Zerlegung am `__` (Paket 3).
    let praefix = format!("{eintrag}__");
    let kurz = aufruf
        .tool
        .strip_prefix(&praefix)
        .filter(|kurz| !kurz.is_empty())
        .unwrap_or(&aufruf.tool);
    format!("{eintrag} › {kurz}")
}

/// Der Name einer Protokollzeile — der zugängliche Name des Knopfs, der das
/// Detail öffnet. Eine Wahrheit für Ansicht und Test.
pub fn aufruf_zeile(sprache: Sprache, aufruf: &Aufruf) -> String {
    format!(
        "{} {} {} {}",
        zeit_text(&aufruf.zeit),
        status_text(sprache, aufruf.status),
        tool_text(aufruf),
        aufruf.agent
    )
}

/// Welche Spalten die Protokolltabelle zeigt. Unter `breakpoint.formular`
/// fallen Agent und Dauer weg; sie stehen dann im Detail (Aufgabe 18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtokollSpalten {
    pub agent: bool,
    pub dauer: bool,
}

/// Die Spalten der Protokolltabelle bei dieser Fensterbreite.
pub fn protokoll_spalten(fenster_breite: f32) -> ProtokollSpalten {
    let breit = fenster_breite >= crate::thema::BREAKPOINT_FORMULAR;
    ProtokollSpalten {
        agent: breit,
        dauer: breit,
    }
}

/// Die Stufe, die eine Prozesszeile selbst nennt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStufe {
    Fehler,
    Warnung,
    Info,
}

/// Die Stufe aus dem Text einer Prozesszeile — viele Tools schreiben sie ins
/// stderr, ein eigenes Feld gibt es nicht (Aufgabe 18).
pub fn log_stufe_art(text: &str) -> Option<LogStufe> {
    let oben = text.to_uppercase();
    if ["ERROR", "FATAL", "PANIC", "FEHLER"]
        .iter()
        .any(|wort| oben.contains(wort))
    {
        Some(LogStufe::Fehler)
    } else if oben.contains("WARN") {
        Some(LogStufe::Warnung)
    } else if oben.contains("INFO") {
        Some(LogStufe::Info)
    } else {
        None
    }
}

/// Die Stufe als kurzes Wort für die Spalte.
pub fn log_stufe(sprache: Sprache, text: &str) -> Option<String> {
    log_stufe_art(text).map(|stufe| {
        t(
            sprache,
            match stufe {
                LogStufe::Fehler => "protokoll.stufe.fehler",
                LogStufe::Warnung => "protokoll.stufe.warnung",
                LogStufe::Info => "protokoll.stufe.info",
            },
        )
    })
}

/// Die Bedeutung einer Prozesszeile: Fehlerzeilen stehen in der Fehlerfarbe,
/// Warnungen als Warnung, alles andere normal (Aufgabe 18).
pub fn log_ton(text: &str) -> Ton {
    match log_stufe_art(text) {
        Some(LogStufe::Fehler) => Ton::Fehler,
        Some(LogStufe::Warnung) => Ton::Warnung,
        Some(LogStufe::Info) | None => Ton::Normal,
    }
}

/// Wie lange ein Aufruf gedauert hat.
pub fn dauer_text(sprache: Sprache, millisekunden: u64) -> String {
    tf(sprache, "protokoll.dauer", &[&millisekunden])
}

/// Der Name eines Zeitraums in der Filterleiste.
pub fn zeitraum_text(sprache: Sprache, zeitraum: Zeitraum) -> String {
    match zeitraum {
        Zeitraum::Stunde => t(sprache, "protokoll.zeitraum.stunde"),
        Zeitraum::Heute => t(sprache, "protokoll.zeitraum.heute"),
        Zeitraum::Tage => t(sprache, "protokoll.zeitraum.tage"),
    }
}

/// Was „Kopieren“ im Detail-Dialog in die Zwischenablage legt: eine Zeile
/// je Angabe, ohne Geheimes (die Argumente sind schon maskiert).
pub fn aufruf_text(sprache: Sprache, aufruf: &Aufruf) -> String {
    let mut zeilen = vec![
        format!("{}: {}", t(sprache, "protokoll.spalte.zeit"), aufruf.zeit),
        format!("{}: {}", t(sprache, "protokoll.spalte.agent"), aufruf.agent),
        format!(
            "{}: {}",
            t(sprache, "protokoll.spalte.eintrag"),
            aufruf.eintrag
        ),
        format!("{}: {}", t(sprache, "protokoll.spalte.tool"), aufruf.tool),
        format!(
            "{}: {}",
            t(sprache, "protokoll.spalte.dauer"),
            dauer_text(sprache, aufruf.dauer_ms)
        ),
        format!(
            "{}: {}",
            t(sprache, "protokoll.spalte.status"),
            status_text(sprache, aufruf.status)
        ),
    ];
    if !aufruf.argumente.is_empty() {
        zeilen.push(format!(
            "{}: {}",
            t(sprache, "protokoll.argumente"),
            aufruf.argumente
        ));
    }
    if let Some(fehler) = &aufruf.fehler {
        zeilen.push(format!("{}: {fehler}", t(sprache, "protokoll.fehlertext")));
    }
    if !aufruf.vorschau.is_empty() {
        zeilen.push(format!(
            "{}: {}",
            t(sprache, "protokoll.vorschau"),
            aufruf.vorschau
        ));
    }
    zeilen.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_client::AgentSicht;
    use quiver_domain::{EintragId, Feld};

    fn modell(verbindung: Verbindung) -> Modell {
        Modell {
            verbindung,
            eintraege: Vec::new(),
            agenten: vec![AgentSicht {
                id: "a1".into(),
                name: "pi".into(),
                verbunden: true,
            }],
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

    fn verbunden() -> Verbindung {
        Verbindung::Verbunden {
            server: "http://quiver.example".into(),
            geraet: "rechner".into(),
        }
    }

    fn eintrag() -> EintragSicht {
        EintragSicht {
            id: EintragId::neu("echo"),
            titel: "Echo".into(),
            beschreibung: "Werkzeuge zum Ausprobieren.".into(),
            zustand: EintragZustand::WartetAufWerte {
                felder: vec!["PFAD".into()],
            },
            fehlende: Vec::new(),
            felder: vec![
                Feld {
                    name: "PFAD".into(),
                    beschreibung: "Ordner".into(),
                    quelle: Quelle::Client,
                    pflicht: true,
                    standard: None,
                    link: None,
                    ..Default::default()
                },
                Feld {
                    name: "SCHLUESSEL".into(),
                    beschreibung: "Geheimnis".into(),
                    quelle: Quelle::Geheim,
                    pflicht: false,
                    standard: None,
                    link: None,
                    ..Default::default()
                },
            ],
            lokale_werte: Werte::from([("PFAD".to_string(), "/tmp".to_string())]),
            geheim_gesetzt: vec!["SCHLUESSEL".into()],
            tools: vec!["echo".into()],
            anmeldung: None,
            einrichtung: None,
            zugaenge: None,
            am_geraet_aus: false,
            unzulaessig: None,
        }
    }

    #[test]
    fn startseite_ohne_server_ist_die_verbindung() {
        assert_eq!(
            startseite(&modell(Verbindung::NichtEingebunden)),
            Seite::Verbindung
        );
        assert_eq!(startseite(&modell(verbunden())), Seite::Tools);
        assert_eq!(
            startseite(&modell(Verbindung::Getrennt {
                server: "http://x".into(),
                fehler: "weg".into(),
            })),
            Seite::Tools,
            "getrennt heißt nicht ungebunden"
        );
    }

    #[test]
    fn seite_folgt_der_verbindung() {
        let zu = verbunden();
        // Gerade eingebunden: ab zu den Tools.
        assert_eq!(
            seite_nach_wechsel(&Verbindung::NichtEingebunden, &zu, Seite::Verbindung),
            Seite::Tools
        );
        // Abgemeldet: zurück zur Verbindung, egal wo man war.
        assert_eq!(
            seite_nach_wechsel(&zu, &Verbindung::NichtEingebunden, Seite::Agenten),
            Seite::Verbindung
        );
        // Sonst bleibt die gewählte Seite.
        assert_eq!(seite_nach_wechsel(&zu, &zu, Seite::Agenten), Seite::Agenten);
        // Während der Browser-Anmeldung und bei „neu anmelden“ bleibt die
        // Verbindungsseite vorn, auch wenn man woanders war (Aufgabe 22).
        let browser = Verbindung::WartetAufBrowser {
            server: "https://quiver.example".into(),
            adresse: "https://quiver.example/auth/geraet/start".into(),
        };
        assert_eq!(
            seite_nach_wechsel(&zu, &browser, Seite::Tools),
            Seite::Verbindung
        );
        let neu = Verbindung::NeuAnmelden {
            server: "https://quiver.example".into(),
            text: "neu".into(),
        };
        assert_eq!(
            seite_nach_wechsel(&browser, &neu, Seite::Verbindung),
            Seite::Verbindung
        );
        // Danach wieder zu den Tools.
        assert_eq!(
            seite_nach_wechsel(&neu, &zu, Seite::Verbindung),
            Seite::Tools
        );
    }

    #[test]
    fn drei_zustaende_drei_bedeutungen() {
        let sprache = Sprache::De;
        let laeuft = EintragZustand::Laeuft { tools: 3 };
        let wartet = EintragZustand::WartetAufWerte {
            felder: vec!["PFAD".into(), "KEY".into()],
        };
        let fehler = EintragZustand::Fehler {
            text: "kaputt".into(),
        };
        assert_eq!(zustand_ton(&laeuft), Ton::Gut);
        assert_eq!(zustand_ton(&wartet), Ton::Warnung);
        assert_eq!(zustand_ton(&fehler), Ton::Fehler);
        assert_eq!(zustand_ton(&EintragZustand::Startet), Ton::Leise);
        assert_eq!(zustand_text(sprache, &laeuft), "Läuft · 3 Tools");
        assert_eq!(zustand_text(sprache, &wartet), "Braucht Angaben: PFAD, KEY");
        assert_eq!(zustand_text(sprache, &fehler), "Fehler");
        assert_eq!(fehlertext(&fehler), Some("kaputt"));
        assert_eq!(fehlertext(&laeuft), None);
        // Eine leere Feldliste bleibt beim kurzen Text.
        assert_eq!(
            zustand_text(
                sprache,
                &EintragZustand::WartetAufWerte { felder: Vec::new() }
            ),
            "Braucht Angaben"
        );
    }

    /// Am Gerät abgeschaltet und unbekannt sind neutral – kein Fehler
    /// (Aufgabe 19).
    #[test]
    fn abgeschaltet_und_unbekannt_sind_neutral() {
        for zustand in [EintragZustand::AmGeraetAus, EintragZustand::Unbekannt] {
            assert_eq!(zustand_ton(&zustand), Ton::Blass);
            assert_eq!(fehlertext(&zustand), None);
        }
        assert_eq!(
            zustand_text(Sprache::De, &EintragZustand::AmGeraetAus),
            "Abgeschaltet"
        );
        assert_eq!(
            zustand_text(Sprache::En, &EintragZustand::AmGeraetAus),
            "Switched off"
        );
        assert_eq!(
            zustand_text(Sprache::De, &EintragZustand::Unbekannt),
            "Unbekannt"
        );
    }

    /// Der Filter „nur aktive“ lässt abgeschaltete Karten weg (Aufgabe 19).
    #[test]
    fn der_filter_nur_aktive_laesst_abgeschaltete_weg() {
        let mut aus = eintrag();
        aus.id = EintragId::neu("aus");
        aus.am_geraet_aus = true;
        aus.zustand = EintragZustand::AmGeraetAus;
        aus.tools.clear();
        let liste = vec![eintrag(), aus];

        let alle = sichtbare_eintraege(&liste, false);
        assert_eq!(alle.len(), 2, "ohne Filter steht alles da");
        let aktive = sichtbare_eintraege(&liste, true);
        assert_eq!(aktive.len(), 1, "mit Filter nur das Aktive");
        assert_eq!(aktive[0].id.als_str(), "echo");
    }

    #[test]
    fn anmeldung_hat_vier_zustaende_und_je_einen_knopf() {
        let sprache = Sprache::De;
        // Abgemeldet und unbekannt: derselbe Hinweis, „Anmelden“.
        for zustand in [AnmeldeZustand::Abgemeldet, AnmeldeZustand::Unbekannt] {
            let zeile = anmeldung_zeile(sprache, &zustand);
            assert_eq!(zeile.ton, Ton::Warnung);
            assert_eq!(zeile.text, "Anmeldung nötig");
            assert_eq!(zeile.knopf, AnmeldeKnopf::Anmelden);
        }
        // Läuft: leise, kein Knopf.
        let laeuft = anmeldung_zeile(sprache, &AnmeldeZustand::Laeuft);
        assert_eq!(laeuft.ton, Ton::Leise);
        assert_eq!(laeuft.text, "Anmeldung im Browser läuft …");
        assert_eq!(laeuft.knopf, AnmeldeKnopf::Keiner);
        // Angemeldet: Konto und „Abmelden“; ohne Konto bleibt der Name weg.
        let angemeldet = anmeldung_zeile(
            sprache,
            &AnmeldeZustand::Angemeldet {
                konto: Some("test@beispiel.de".into()),
            },
        );
        assert_eq!(angemeldet.ton, Ton::Gut);
        assert_eq!(angemeldet.text, "Angemeldet als test@beispiel.de");
        assert_eq!(angemeldet.knopf, AnmeldeKnopf::Abmelden);
        let ohne = anmeldung_zeile(sprache, &AnmeldeZustand::Angemeldet { konto: None });
        assert_eq!(ohne.text, "Angemeldet");
        // Fehler: der Text des Fehlers, „Erneut versuchen“.
        let fehler = anmeldung_zeile(
            sprache,
            &AnmeldeZustand::Fehler {
                text: "Der Status endet mit Code 3".into(),
            },
        );
        assert_eq!(fehler.ton, Ton::Fehler);
        assert_eq!(fehler.text, "Der Status endet mit Code 3");
        assert_eq!(fehler.knopf, AnmeldeKnopf::Erneut);

        // Englisch ist übersetzt, nicht kopiert.
        let en = anmeldung_zeile(Sprache::En, &AnmeldeZustand::Abgemeldet);
        assert_eq!(en.text, "Sign-in required");
        let en = anmeldung_zeile(
            Sprache::En,
            &AnmeldeZustand::Angemeldet {
                konto: Some("test@beispiel.de".into()),
            },
        );
        assert_eq!(en.text, "Signed in as test@beispiel.de");
    }

    #[test]
    fn einrichtung_hat_drei_zustaende_und_je_einen_knopf() {
        let sprache = Sprache::De;
        // Bereit: der Knopf „Einrichten“, leise erklärt.
        let bereit = einrichtung_zeile(sprache, &EinrichtZustand::Bereit);
        assert_eq!(bereit.ton, Ton::Leise);
        assert_eq!(bereit.text, "Einrichtung im Browser");
        assert_eq!(bereit.knopf, EinrichtKnopf::Einrichten);
        // Läuft: derselbe leise Ton und „Abbrechen“ – eine hängende
        // Einrichtung lässt sich beenden (Paket 3).
        let laeuft = einrichtung_zeile(sprache, &EinrichtZustand::Laeuft);
        assert_eq!(laeuft.ton, Ton::Leise);
        assert_eq!(laeuft.text, "Einrichtung im Browser geöffnet …");
        assert_eq!(laeuft.knopf, EinrichtKnopf::Abbrechen);
        // Fehler: sein Text und „Erneut versuchen“.
        let fehler = einrichtung_zeile(
            sprache,
            &EinrichtZustand::Fehler {
                text: "Die Einrichtung endet mit Code 3".into(),
            },
        );
        assert_eq!(fehler.ton, Ton::Fehler);
        assert_eq!(fehler.text, "Die Einrichtung endet mit Code 3");
        assert_eq!(fehler.knopf, EinrichtKnopf::Erneut);
        // Englisch ist übersetzt, nicht kopiert.
        let en = einrichtung_zeile(Sprache::En, &EinrichtZustand::Bereit);
        assert_eq!(en.text, "Setup in the browser");
        let en = einrichtung_zeile(Sprache::En, &EinrichtZustand::Laeuft);
        assert_eq!(en.text, "Setup opened in the browser …");
    }

    #[test]
    fn verbindung_in_worten() {
        let (ton, text) = verbindung_status(Sprache::De, &verbunden());
        assert_eq!(ton, Ton::Gut);
        assert_eq!(text, "Verbunden mit http://quiver.example");
        let (ton, _) = verbindung_status(
            Sprache::De,
            &Verbindung::Getrennt {
                server: "http://x".into(),
                fehler: "weg".into(),
            },
        );
        assert_eq!(ton, Ton::Fehler);
        // Neue Zustände der Server-Anmeldung (Aufgabe 22) sind kurz genug
        // für die Kopfzeile.
        let neu = Verbindung::NeuAnmelden {
            server: "https://quiver.example".into(),
            text: "neu".into(),
        };
        assert_eq!(
            verbindung_status(Sprache::De, &neu).1,
            "Neu anmelden",
            "kurz, sonst schneidet die Kopfzeile ab"
        );
        assert_eq!(tray_zeile(&modell(neu), Sprache::De), "Neu anmelden nötig");
        assert_eq!(
            tray_zeile(
                &modell(Verbindung::WartetAufBrowser {
                    server: "https://quiver.example".into(),
                    adresse: "https://quiver.example/auth/geraet/start".into(),
                }),
                Sprache::En
            ),
            "Signing in via browser …"
        );
    }

    #[test]
    fn fehlendes_wird_erklaert() {
        let feld = |name: &str, beschreibung: &str| FehlendesFeld {
            name: name.into(),
            beschreibung: beschreibung.into(),
            quelle: Quelle::Client,
        };
        assert_eq!(
            fehlende_texte(&[feld("PFAD", "Ordner, den das Tool lesen darf")]),
            "Ordner, den das Tool lesen darf"
        );
        assert_eq!(fehlende_texte(&[feld("PFAD", "  ")]), "PFAD", "ohne Text");
        assert_eq!(
            fehlende_texte(&[feld("A", "eins"), feld("B", "zwei")]),
            "eins · zwei"
        );
        assert_eq!(fehlende_texte(&[]), "");
    }

    #[test]
    fn eingaben_vorbelegt_und_geheim_leer() {
        let eingaben = eingaben(&eintrag());
        assert_eq!(eingaben.len(), 2);
        assert_eq!(eingaben[0].name, "PFAD");
        assert_eq!(eingaben[0].wert, "/tmp");
        assert!(!eingaben[0].geheim);
        assert_eq!(eingaben[1].name, "SCHLUESSEL");
        assert!(eingaben[1].geheim);
        assert_eq!(eingaben[1].wert, "", "Geheimes wird nie angezeigt");
        assert!(eingaben[1].gesetzt);
    }

    #[test]
    fn leeres_geheimnis_bleibt_unveraendert() {
        let mut eingaben = eingaben(&eintrag());
        eingaben[0].wert = " /var/lib ".into();
        eingaben[1].wert = String::new();
        let (lokal, geheim) = werte_splitten(&eingaben);
        assert_eq!(lokal.get("PFAD").map(String::as_str), Some("/var/lib"));
        assert!(geheim.is_empty(), "nichts überschreiben");
        // Mit Wert wird es geschickt — und ein leerer lokaler Wert löscht.
        eingaben[1].wert = "s3cret".into();
        eingaben[0].wert = String::new();
        let (lokal, geheim) = werte_splitten(&eingaben);
        assert_eq!(lokal.get("PFAD").map(String::as_str), Some(""));
        assert_eq!(geheim.get("SCHLUESSEL").map(String::as_str), Some("s3cret"));
    }

    /// Outline aus der Aufgabe: `url` trägt die Adresse, `api_key` den Link.
    fn eintrag_mit_link() -> EintragSicht {
        let mut eintrag = eintrag();
        eintrag.felder = vec![
            Feld {
                name: "url".into(),
                beschreibung: "Adresse der Wiki".into(),
                quelle: Quelle::Client,
                pflicht: true,
                standard: None,
                link: None,
                ..Default::default()
            },
            Feld {
                name: "api_key".into(),
                beschreibung: "Token".into(),
                quelle: Quelle::Geheim,
                pflicht: true,
                standard: None,
                link: Some(FeldLink {
                    text: "Token in Outline erstellen".into(),
                    url: "${url}/settings/api-and-access".into(),
                }),
                ..Default::default()
            },
        ];
        eintrag.lokale_werte = Werte::new();
        eintrag.geheim_gesetzt = Vec::new();
        eintrag
    }

    #[test]
    fn link_knopf_folgt_den_aktuellen_eingaben() {
        let eintrag = eintrag_mit_link();
        let mut eingaben = eingaben(&eintrag);
        let link = eingaben[1].link.clone().expect("Link am zweiten Feld");
        assert!(eingaben[0].link.is_none(), "url ohne Link");
        let basis = link_basis(&eintrag);

        // Ohne Adresse bleibt der Knopf gesperrt — und sagt, was fehlt.
        assert_eq!(link_adresse(&link, &eingaben, &basis), None);
        assert_eq!(link_fehlende(&link, &eingaben, &basis), vec!["url"]);
        assert_eq!(
            link_hinweis(&link_fehlende(&link, &eingaben, &basis), Sprache::De),
            "Erst url eintragen"
        );

        // Die Eingabe im Dialog zählt, nicht nur der gespeicherte Wert (der
        // hier leer ist). Ein Schrägstrich am Ende stört nicht.
        eingaben[0].wert = " https://wiki.example.de/ ".into();
        assert_eq!(
            link_adresse(&link, &eingaben, &basis).as_deref(),
            Some("https://wiki.example.de/settings/api-and-access")
        );
        assert!(link_fehlende(&link, &eingaben, &basis).is_empty());

        // Nur http(s) kommt durch — auch wenn alles ausgefüllt ist.
        eingaben[0].wert = "javascript:alert(1)".into();
        assert_eq!(link_adresse(&link, &eingaben, &basis), None);
    }

    #[test]
    fn link_basis_sind_die_standardwerte() {
        let mut eintrag = eintrag_mit_link();
        eintrag.felder[0].standard = Some(" https://wiki.example.de ".into());
        let basis = link_basis(&eintrag);
        assert_eq!(
            basis.get("url").map(String::as_str),
            Some("https://wiki.example.de")
        );
        assert!(
            !basis.contains_key("api_key"),
            "leere Standards zählen nicht"
        );

        // Standard füllt die Lücke, die Eingabe gewinnt.
        let mut eingaben = eingaben(&eintrag);
        let link = eingaben[1].link.clone().expect("Link am zweiten Feld");
        assert_eq!(
            link_adresse(&link, &eingaben, &basis).as_deref(),
            Some("https://wiki.example.de/settings/api-and-access")
        );
        eingaben[0].wert = "https://anderes.example".into();
        assert_eq!(
            link_adresse(&link, &eingaben, &basis).as_deref(),
            Some("https://anderes.example/settings/api-and-access")
        );
    }

    #[test]
    fn platzhalter_kommen_in_reihenfolge_und_einmal() {
        assert_eq!(
            platzhalter("${b}/x/${a}/${b}"),
            vec!["b".to_string(), "a".to_string()]
        );
        assert!(platzhalter("ohne").is_empty(), "nichts zu ersetzen");
        assert!(platzhalter("${offen").is_empty(), "unvollständig");
    }

    #[test]
    fn schnipsel_tragen_adresse_und_kein_token_als_argument() {
        let e = "http://127.0.0.1:8741/mcp";
        let pi = schnipsel_pi(e);
        assert_eq!(
            pi,
            "read -rs QUIVER_TOKEN && pi mcp add quiver --url http://127.0.0.1:8741/mcp \
             --header \"Authorization=Bearer $QUIVER_TOKEN\"; unset QUIVER_TOKEN"
        );
        // Kein Wert, nur die Variable – und die wird verdeckt eingelesen.
        for text in [
            schnipsel_pi(e),
            schnipsel_claude(e),
            schnipsel_opencode(e),
            schnipsel_copilot(e),
            schnipsel_json(e),
        ] {
            assert!(text.contains("read -rs QUIVER_TOKEN"), "{text}");
            assert!(text.contains("$QUIVER_TOKEN"), "{text}");
            assert!(text.ends_with("unset QUIVER_TOKEN"), "{text}");
            assert!(text.contains(e), "{text}");
        }
        // Das JSON im printf ist gültig, wenn man das Token einsetzt.
        let schnipsel_text = schnipsel_json(e);
        let json = schnipsel_text
            .split_once("printf '")
            .and_then(|(_, rest)| rest.split_once('\''))
            .map(|(json, _)| json)
            .expect("printf mit JSON");
        let wert: serde_json::Value =
            serde_json::from_str(&json.replace("%s", "abc123")).expect("gültiges JSON");
        assert_eq!(wert["mcpServers"]["quiver"]["url"], e);
        assert_eq!(
            wert["mcpServers"]["quiver"]["headers"]["Authorization"],
            "Bearer abc123"
        );
    }

    #[test]
    fn jeder_agent_nur_einmal() {
        let gekoppelt = vec!["pi".to_string(), "Claude Code".to_string()];
        assert!(name_vergeben("PI", &gekoppelt), "ohne Groß/Klein");
        assert!(name_vergeben("  claude code ", &gekoppelt), "am Rand");
        assert!(!name_vergeben("hermes", &gekoppelt));
        assert!(!name_vergeben("  ", &gekoppelt), "leer ist kein Name");
        assert!(!name_vergeben("pi", &[]));
    }

    #[test]
    fn arten_haben_feste_namen() {
        assert_eq!(AgentArt::Pi.name(), "pi");
        assert_eq!(AgentArt::ClaudeCode.name(), "claude-code");
        assert_eq!(AgentArt::OpenCode.name(), "opencode");
        assert_eq!(AgentArt::Copilot.name(), "copilot");
        assert_eq!(AgentArt::Hermes.name(), "hermes");
        let andere = AgentArt::Andere("  mein-agent ".into());
        assert_eq!(andere.name(), "mein-agent");
        assert!(!andere.ist_fest());
        assert!(AgentArt::Pi.ist_fest());

        // Hin und zurück: bekannte Namen werden erkannt, fremde bleiben.
        for art in [
            AgentArt::Pi,
            AgentArt::ClaudeCode,
            AgentArt::OpenCode,
            AgentArt::Copilot,
            AgentArt::Hermes,
        ] {
            assert!(AgentArt::aus_name(&art.name()).gleich(&art));
        }
        assert!(AgentArt::aus_name("PI").gleich(&AgentArt::Pi));
        assert!(AgentArt::aus_name("claude").gleich(&AgentArt::ClaudeCode));
        assert_eq!(
            AgentArt::aus_name(" mein-agent "),
            AgentArt::Andere("mein-agent".into())
        );
        // Andere bleibt Andere, egal welcher Name darin steht.
        assert!(AgentArt::Andere("x".into()).gleich(&AgentArt::Andere("y".into())));

        // Der einzutragende Name: fest bei bekannter Art, sonst die Eingabe.
        assert_eq!(AgentArt::Hermes.einzutragen("egal"), "hermes");
        assert_eq!(
            AgentArt::Andere(String::new()).einzutragen("  mein-agent "),
            "mein-agent"
        );
    }

    #[test]
    fn gekoppelte_arten_sind_ausgegraut() {
        let zeilen = arten_auswahl(&["pi".into(), "Hermes".into(), "x".into()]);
        assert_eq!(zeilen.len(), 6, "fünf bekannte Arten und Andere");
        let gekoppelt = |art: &AgentArt| {
            zeilen
                .iter()
                .find(|zeile| zeile.art.gleich(art))
                .map(|zeile| zeile.gekoppelt)
                .expect("Art steht in der Auswahl")
        };
        assert!(gekoppelt(&AgentArt::Pi));
        assert!(gekoppelt(&AgentArt::Hermes), "ohne Groß/Klein");
        assert!(!gekoppelt(&AgentArt::OpenCode));
        assert!(
            !gekoppelt(&AgentArt::Andere(String::new())),
            "Andere bleibt immer wählbar"
        );

        // Doppelte Namen aus alten Profilen bleiben erhalten und stören nicht:
        // „pi" ist trotzdem nur eine ausgegraute Zeile.
        let mit_doppelten = arten_auswahl(&["pi".into(), "PI".into()]);
        assert_eq!(mit_doppelten.len(), 6);
        assert_eq!(
            mit_doppelten.iter().filter(|zeile| zeile.gekoppelt).count(),
            1
        );

        // Der Dialog beginnt bei der ersten freien Art …
        assert!(erste_freie(&["pi".into()]).gleich(&AgentArt::ClaudeCode));
        assert!(erste_freie(&[]).gleich(&AgentArt::Pi));
        // … und bei „Andere", wenn alles Bekannte vergeben ist.
        let alle = ["pi", "claude-code", "opencode", "copilot", "hermes"].map(String::from);
        assert!(erste_freie(&alle).gleich(&AgentArt::Andere(String::new())));
    }

    /// Ein Eintrag mit Zugangsliste wie bei quiver-ssh (Aufgabe 14).
    fn zugang_eintrag() -> EintragSicht {
        let mut port = Feld {
            name: "port".into(),
            beschreibung: "Port".into(),
            quelle: Quelle::Client,
            pflicht: false,
            standard: None,
            link: None,
            ..Default::default()
        };
        port.standard = Some("22".into());
        let mut eintrag = eintrag();
        eintrag.zugaenge = Some(ZugaengeSicht {
            titel: "Verbindungen".into(),
            einzahl: "Verbindung".into(),
            felder: vec![
                Feld {
                    name: "benutzer".into(),
                    beschreibung: "Benutzer".into(),
                    quelle: Quelle::Client,
                    pflicht: true,
                    standard: None,
                    link: None,
                    ..Default::default()
                },
                Feld {
                    name: "host".into(),
                    beschreibung: "Host".into(),
                    quelle: Quelle::Client,
                    pflicht: true,
                    standard: None,
                    link: None,
                    ..Default::default()
                },
                port,
                Feld {
                    name: "passwort".into(),
                    beschreibung: "Passwort".into(),
                    quelle: Quelle::Geheim,
                    pflicht: false,
                    standard: None,
                    link: None,
                    ..Default::default()
                },
            ],
            liste: vec![
                ZugangSicht {
                    name: "db-01".into(),
                    werte: Werte::from([
                        ("benutzer".to_string(), "admin".to_string()),
                        ("host".to_string(), "db-01.example".to_string()),
                    ]),
                    geheim_gesetzt: vec!["passwort".into()],
                },
                ZugangSicht {
                    name: "web-01".into(),
                    werte: Werte::from([
                        ("benutzer".to_string(), "root".to_string()),
                        ("host".to_string(), "web-01.example".to_string()),
                        ("port".to_string(), "2222".to_string()),
                    ]),
                    geheim_gesetzt: Vec::new(),
                },
            ],
        });
        eintrag
    }

    #[test]
    fn zugangsliste_hat_ueberschrift_anzahl_und_knopf() {
        let eintrag = zugang_eintrag();
        let zugaenge = eintrag.zugaenge.clone().expect("Liste");
        assert_eq!(
            zugang_beschriftung(Sprache::De, &zugaenge),
            "Verbindungen (2)"
        );
        assert_eq!(
            zugang_hinzufuegen(Sprache::De, &zugaenge),
            "Verbindung hinzufügen"
        );
        assert_eq!(
            zugang_hinzufuegen(Sprache::En, &zugaenge),
            "Add Verbindung",
            "der Name des Elements kommt aus den Daten"
        );
        assert_eq!(
            zugang_titel(Sprache::De, &zugaenge.einzahl, true),
            "Verbindung anlegen"
        );
        assert_eq!(
            zugang_titel(Sprache::De, &zugaenge.einzahl, false),
            "Verbindung bearbeiten"
        );
    }

    #[test]
    fn zugangszeile_zeigt_zwei_werte_ohne_geheimes() {
        let eintrag = zugang_eintrag();
        let zugaenge = eintrag.zugaenge.clone().expect("Liste");
        let db = &zugaenge.liste[0];
        assert_eq!(
            zugang_werte_zeile(&zugaenge.felder, db),
            "admin@db-01.example",
            "Name und die ersten zwei Werte, Geheimes bleibt weg"
        );
        // Ein Zugang ohne Werte ergibt eine leere Zeile, keine „@“-Kette.
        let leer = ZugangSicht {
            name: "neu".into(),
            werte: Werte::new(),
            geheim_gesetzt: Vec::new(),
        };
        assert_eq!(zugang_werte_zeile(&zugaenge.felder, &leer), "");
    }

    #[test]
    fn zugang_eingaben_vorbelegt_und_geheim_leer() {
        let eintrag = zugang_eintrag();
        let zugaenge = eintrag.zugaenge.clone().expect("Liste");
        // Bearbeiten: die Werte stehen da, das Geheimnis nur als „gesetzt“.
        let eingaben = zugang_eingaben(&zugaenge, Some("db-01"));
        assert_eq!(eingaben.len(), 4);
        assert_eq!(eingaben[0].name, "benutzer");
        assert_eq!(eingaben[0].wert, "admin");
        assert_eq!(eingaben[1].wert, "db-01.example");
        assert_eq!(eingaben[2].wert, "", "port ist bei db-01 nicht gesetzt");
        assert!(eingaben[3].geheim);
        assert!(eingaben[3].gesetzt);
        assert_eq!(eingaben[3].wert, "", "Geheimes wird nie angezeigt");
        // Der Standard des Felds füllt den Port bei einem leeren Zugang.
        let neu = zugang_eingaben(&zugaenge, None);
        assert_eq!(neu[2].wert, "", "Standard steht nicht im Feld");
        assert!(!neu[3].gesetzt, "ein neuer Zugang hat keine Geheimnisse");
        assert!(!neu[0].gesetzt);
        // Der Hilfe-Link eines Zugangsfelds nutzt dessen Standardwerte.
        let basis = link_basis_aus_feldern(&zugaenge.felder);
        assert_eq!(basis.get("port").map(String::as_str), Some("22"));
    }

    #[test]
    fn zugangsname_folgt_der_regel_und_ist_eindeutig() {
        let vorhanden = vec!["web-01".to_string()];
        assert_eq!(
            zugang_name_fehler("", &vorhanden),
            Some("zugaenge.name_fehlt")
        );
        assert_eq!(
            zugang_name_fehler("  ", &vorhanden),
            Some("zugaenge.name_fehlt"),
            "Leerzeichen sind kein Name"
        );
        // Großschreibung zählt nicht: „Web-01“ ist derselbe Zugang wie „web-01“.
        assert_eq!(
            zugang_name_fehler("Web-01", &vorhanden),
            Some("zugaenge.name_doppelt")
        );
        assert_eq!(zugang_name_fehler("BL4_900", &vorhanden), None);
        for name in ["web 01", "web/01", "ä"] {
            assert_eq!(
                zugang_name_fehler(name, &vorhanden),
                Some("zugaenge.name_ungueltig"),
                "{name}"
            );
        }
        assert_eq!(
            zugang_name_fehler(&"a".repeat(41), &vorhanden),
            Some("zugaenge.name_ungueltig"),
            "41 Zeichen sind zu lang"
        );
        assert_eq!(
            zugang_name_fehler(&"a".repeat(40), &vorhanden),
            None,
            "40 Zeichen gehen"
        );
        assert_eq!(
            zugang_name_fehler("web-01", &vorhanden),
            Some("zugaenge.name_doppelt")
        );
        assert_eq!(zugang_name_fehler(" web-02 ", &vorhanden), None);
        // Die Texte gibt es in beiden Sprachen.
        for schluessel in [
            "zugaenge.name_fehlt",
            "zugaenge.name_ungueltig",
            "zugaenge.name_doppelt",
        ] {
            for sprache in Sprache::ALLE {
                assert_ne!(t(sprache, schluessel), schluessel, "{schluessel}");
            }
        }
    }

    #[test]
    fn passwortregeln_fuer_die_sicherung() {
        // Leer, zu kurz und nicht gleich sind eigene Meldungen – Export und
        // Import teilen sich die Mindestlänge (Aufgabe 15).
        assert_eq!(
            export_passwort_fehler("", ""),
            Some("einstellungen.passwort_fehlt")
        );
        assert_eq!(
            export_passwort_fehler("   ", "   "),
            Some("einstellungen.passwort_fehlt")
        );
        assert_eq!(
            export_passwort_fehler("kurz", "kurz"),
            Some("einstellungen.passwort_kurz")
        );
        assert_eq!(
            export_passwort_fehler("ein-langes-passwort", "etwas-anderes"),
            Some("einstellungen.passwort_ungleich")
        );
        // Leerzeichen am Rand zählen nicht; acht Zeichen genügen.
        assert_eq!(
            export_passwort_fehler(" ein-langes-passwort ", "ein-langes-passwort"),
            None
        );
        assert_eq!(export_passwort_fehler("12345678", "12345678"), None);

        assert_eq!(
            import_passwort_fehler(""),
            Some("einstellungen.passwort_fehlt")
        );
        assert_eq!(
            import_passwort_fehler("1234567"),
            Some("einstellungen.passwort_kurz")
        );
        assert_eq!(import_passwort_fehler("12345678"), None);

        // Die Meldungen gibt es in beiden Sprachen.
        for schluessel in [
            "einstellungen.passwort_fehlt",
            "einstellungen.passwort_kurz",
            "einstellungen.passwort_ungleich",
        ] {
            for sprache in Sprache::ALLE {
                assert_ne!(t(sprache, schluessel), schluessel, "{schluessel}");
            }
        }
    }

    #[test]
    fn das_ergebnis_zaehlt_und_uebersetzt() {
        let alle = Zusammenfassung {
            eintraege: 6,
            werte: 9,
            zugaenge: 2,
            geheimnisse: 3,
            uebersprungen: vec!["weg".to_string()],
        };
        assert_eq!(
            ergebnis_text(Sprache::De, false, &alle),
            "6 Einträge exportiert (mit Geheimnissen) · 1 übersprungen"
        );
        // Ohne Geheimnisse steht der Zusatz nicht da.
        let offen = Zusammenfassung {
            geheimnisse: 0,
            uebersprungen: Vec::new(),
            ..alle
        };
        assert_eq!(
            ergebnis_text(Sprache::De, false, &offen),
            "6 Einträge exportiert"
        );
        // Einzahl und Import.
        let eins = Zusammenfassung {
            eintraege: 1,
            ..Zusammenfassung::default()
        };
        assert_eq!(
            ergebnis_text(Sprache::De, false, &eins),
            "1 Eintrag exportiert"
        );
        assert_eq!(
            ergebnis_text(Sprache::De, true, &eins),
            "1 Eintrag importiert"
        );
        assert_eq!(ergebnis_text(Sprache::En, true, &eins), "1 entry imported");
        assert_eq!(
            ergebnis_text(Sprache::En, false, &offen),
            "6 entries exported"
        );
    }

    #[test]
    fn die_vorschau_zeile_nennt_was_sich_aendert() {
        let zeile =
            |art: VorschauArt, neue: usize, geaenderte: usize, zugaenge: usize| VorschauEintrag {
                id: "echo".into(),
                titel: "Echo".into(),
                art,
                neue,
                geaenderte,
                zugaenge,
                am_geraet_aus: false,
            };
        assert_eq!(
            vorschau_detail(Sprache::De, &zeile(VorschauArt::Neu, 2, 0, 0)),
            "neu · 2 neu"
        );
        assert_eq!(
            vorschau_detail(Sprache::De, &zeile(VorschauArt::Aendert, 1, 2, 1)),
            "ändert · 1 neu · 2 geändert · 1 Zugänge"
        );
        assert_eq!(
            vorschau_detail(Sprache::De, &zeile(VorschauArt::Uebersprungen, 0, 0, 0)),
            "übersprungen – auf diesem Gerät nicht vorhanden"
        );
        assert_eq!(
            vorschau_detail(Sprache::En, &zeile(VorschauArt::Neu, 2, 0, 0)),
            "new · 2 new"
        );
    }

    #[test]
    fn schnipsel_je_art() {
        let e = "http://127.0.0.1:8741/mcp";
        let t = "abc123";
        let schnipsel = |art: AgentArt| {
            let liste = schnipsel(Sprache::De, &art, e, t);
            assert!(!liste.is_empty(), "{art:?} ohne Schnipsel");
            for s in &liste {
                assert!(!s.ort.is_empty(), "{art:?} ohne Ort");
                // Bei Hermes verteilen sich Endpunkt und Token auf drei Schnipsel.
                if !matches!(art, AgentArt::Hermes) {
                    assert!(s.text.contains(e), "{art:?} ohne Endpunkt: {}", s.text);
                    assert!(
                        !s.text.contains(t),
                        "{art:?} trägt das Token als Wert: {}",
                        s.text
                    );
                    assert!(
                        s.text.contains("read -rs"),
                        "{art:?} liest nicht verdeckt ein"
                    );
                }
            }
            liste
        };
        assert_eq!(schnipsel(AgentArt::Pi)[0].text, schnipsel_pi(e));
        assert_eq!(schnipsel(AgentArt::ClaudeCode)[0].text, schnipsel_claude(e));
        assert_eq!(schnipsel(AgentArt::OpenCode)[0].text, schnipsel_opencode(e));
        assert_eq!(schnipsel(AgentArt::Copilot)[0].text, schnipsel_copilot(e));
        let hermes = schnipsel(AgentArt::Hermes);
        assert_eq!(hermes.len(), 3, "Befehl, Token und Prüfbefehl");
        assert_eq!(
            hermes[0].text,
            "hermes mcp add quiver --url http://127.0.0.1:8741/mcp --auth header"
        );
        assert_eq!(hermes[1].text, "abc123");
        assert_eq!(hermes[2].text, "hermes mcp test quiver");
        assert_eq!(
            schnipsel(AgentArt::Andere(String::new()))[0].text,
            schnipsel_json(e)
        );

        // Die JSON-Schnipsel sind gültiges JSON (mit eingesetztem Token), nicht
        // nur richtig aussehend.
        for (art, wert) in [
            (AgentArt::OpenCode, schnipsel_opencode(e)),
            (AgentArt::Copilot, schnipsel_copilot(e)),
            (AgentArt::Andere(String::new()), schnipsel_json(e)),
        ] {
            let json_teil = wert
                .split_once("printf '")
                .and_then(|(_, rest)| rest.split_once('\''))
                .map(|(json, _)| json.to_string())
                .unwrap_or_else(|| panic!("{art:?} ohne printf: {wert}"));
            let json: serde_json::Value = serde_json::from_str(&json_teil.replace("%s", "abc123"))
                .unwrap_or_else(|fehler| panic!("{art:?} ist kein JSON: {fehler}\n{wert}"));
            assert_eq!(
                json.to_string().matches("Authorization").count(),
                1,
                "{art:?} trägt den Authorization-Kopf"
            );
            assert_eq!(
                json.to_string().matches("Bearer abc123").count(),
                1,
                "{art:?} trägt das Token"
            );
        }
    }

    #[test]
    fn geraetename_kommt_aus_der_umgebung() {
        let leer = |_: &str| None;
        assert_eq!(geraetename_aus(leer), "quiver");
        let mit = |name: &str| (name == "HOSTNAME").then(|| "  rechner ".to_string());
        assert_eq!(geraetename_aus(mit), "rechner");
        let eigen = |name: &str| match name {
            "MCP_QUIVER_GERAET" => Some("labor".to_string()),
            "HOSTNAME" => Some("rechner".to_string()),
            _ => None,
        };
        assert_eq!(geraetename_aus(eigen), "labor", "die eigene gilt zuerst");
    }

    #[test]
    fn tray_zeile_zaehlt_was_laeuft() {
        let mut m = modell(verbunden());
        m.eintraege = vec![
            EintragSicht {
                tools: vec!["echo".into(), "ping".into()],
                ..eintrag()
            },
            EintragSicht {
                tools: vec!["lesen".into(), "schreiben".into()],
                ..eintrag()
            },
        ];
        assert_eq!(tray_zeile(&m, Sprache::De), "Verbunden · 4 Tools · 1 Agent");
        assert_eq!(tray_zeile(&m, Sprache::En), "Connected · 4 tools · 1 agent");
        // Zwei Agenten und einer davon weg: gezählt wird, wer da ist.
        m.agenten.push(AgentSicht {
            id: "a2".into(),
            name: "hermes".into(),
            verbunden: false,
        });
        assert_eq!(
            tray_zeile(&m, Sprache::De),
            "Verbunden · 4 Tools · 1 Agent",
            "getrennte Agenten zählen nicht"
        );
        m.agenten.push(AgentSicht {
            id: "a3".into(),
            name: "claude".into(),
            verbunden: true,
        });
        assert!(
            tray_zeile(&m, Sprache::De).ends_with("2 Agenten"),
            "Mehrzahl"
        );
        // Am Gerät abgeschaltete Einträge zählen nicht: sie bieten keine
        // Werkzeuge an (Aufgabe 19).
        m.eintraege.push(EintragSicht {
            am_geraet_aus: true,
            zustand: EintragZustand::AmGeraetAus,
            tools: Vec::new(),
            ..eintrag()
        });
        assert_eq!(
            tray_zeile(&m, Sprache::De),
            "Verbunden · 4 Tools · 2 Agenten",
            "abgeschaltete Einträge zählen nicht"
        );
    }

    #[test]
    fn tray_zeile_kennt_die_anderen_zustaende() {
        let ohne = modell(Verbindung::NichtEingebunden);
        assert_eq!(tray_zeile(&ohne, Sprache::De), "Nicht verbunden");
        let getrennt = modell(Verbindung::Getrennt {
            server: "http://x".into(),
            fehler: "weg".into(),
        });
        assert_eq!(
            tray_zeile(&getrennt, Sprache::De),
            "Getrennt · läuft weiter"
        );
        let verbindet = modell(Verbindung::Verbindet {
            server: "http://x".into(),
        });
        assert_eq!(tray_zeile(&verbindet, Sprache::De), "Verbinde …");
    }

    fn aufruf() -> Aufruf {
        Aufruf {
            zeit: "2026-01-02T03:04:05Z".into(),
            agent: "pi".into(),
            eintrag: EintragId::neu("echo"),
            tool: "echo__echo".into(),
            dauer_ms: 12,
            status: Status::Ok,
            fehler: None,
            argumente: r#"{"text":"hallo"}"#.into(),
            antwort_groesse: 5,
            vorschau: "hallo".into(),
        }
    }

    #[test]
    fn protokoll_filter_deckt_die_leiste_ab() {
        let eingaben = ProtokollEingaben::default();
        let filter = protokoll_filter(&eingaben);
        assert_eq!(filter.eintrag, None, "leer heißt alle");
        assert_eq!(filter.tool, None);
        assert_eq!(filter.agent, None);
        assert!(!filter.nur_fehler);
        let seit = filter.seit.clone().expect("Zeitraum");
        assert!(seit.ends_with('Z'), "RFC 3339: {seit}");

        let eingaben = ProtokollEingaben {
            eintrag: " echo ".into(),
            tool: "echo__echo".into(),
            agent: "pi".into(),
            nur_fehler: true,
            zeitraum: Zeitraum::Tage,
            suche: String::new(),
        };
        let filter = protokoll_filter(&eingaben);
        assert_eq!(filter.eintrag, Some(EintragId::neu("echo")), "getrimmt");
        assert_eq!(filter.tool.as_deref(), Some("echo__echo"));
        assert_eq!(filter.agent.as_deref(), Some("pi"));
        assert!(filter.nur_fehler);
        assert_eq!(filter.anzahl, None, "das Fenster will alles im Zeitraum");
        assert!(
            filter.seit < protokoll_filter(&ProtokollEingaben::default()).seit,
            "7 Tage reichen weiter zurück als eine Stunde"
        );

        // Der Freitext wird zum Enthält-Filter; leer heißt kein Filter.
        assert_eq!(log_filter("  ").enthaelt, None);
        assert_eq!(log_filter(" Fehler ").enthaelt.as_deref(), Some("Fehler"));
    }

    /// Die Zeile der Protokolltabelle nennt Eintrag und Werkzeug einmal, die
    /// Spalten folgen der Fensterbreite (Aufgabe 18).
    #[test]
    fn protokollzeile_und_spalten() {
        use crate::thema::BREAKPOINT_FORMULAR;

        let aufruf = aufruf();
        assert_eq!(tool_text(&aufruf), "echo › echo");
        assert_eq!(
            aufruf_zeile(Sprache::De, &aufruf),
            "03:04:05 OK echo › echo pi"
        );

        let schmal = protokoll_spalten(BREAKPOINT_FORMULAR - 1.0);
        assert!(!schmal.agent && !schmal.dauer, "unter 700 px ohne Agent");
        let breit = protokoll_spalten(BREAKPOINT_FORMULAR);
        assert!(breit.agent && breit.dauer);

        // Die Stufe kommt aus dem Text; Fehler färben die Zeile.
        assert_eq!(log_stufe_art("ERROR: weg"), Some(LogStufe::Fehler));
        assert_eq!(log_stufe_art("warning: spät"), Some(LogStufe::Warnung));
        assert_eq!(log_stufe_art("info: da"), Some(LogStufe::Info));
        assert_eq!(log_stufe_art("nur so"), None);
        assert_eq!(log_ton("ERROR: weg"), Ton::Fehler);
        assert_eq!(log_ton("warnung"), Ton::Warnung);
        assert_eq!(log_ton("nur so"), Ton::Normal);
        assert_eq!(
            log_stufe(Sprache::En, "panic: kaputt"),
            Some("ERROR".to_string())
        );
    }

    #[test]
    fn status_zeit_dauer_und_kopiertext() {
        assert_eq!(status_ton(Status::Ok), Ton::Gut);
        assert_eq!(status_ton(Status::Fehler), Ton::Fehler);
        assert_eq!(status_ton(Status::Abgelehnt), Ton::Warnung);
        for (status, de, en) in [
            (Status::Ok, "OK", "OK"),
            (Status::Fehler, "Fehler", "Error"),
            (Status::Abgelehnt, "Abgelehnt", "Rejected"),
        ] {
            assert_eq!(status_text(Sprache::De, status), de);
            assert_eq!(status_text(Sprache::En, status), en);
        }
        assert_eq!(zeit_text("2026-01-02T03:04:05Z"), "03:04:05");
        assert_eq!(zeit_text("ohne-t"), "ohne-t");
        assert_eq!(datum_text("2026-01-02T03:04:05Z"), "2026-01-02");
        assert_eq!(dauer_text(Sprache::De, 12), "12 ms");
        assert_eq!(zeitraum_text(Sprache::De, Zeitraum::Heute), "Heute");
        assert_eq!(zeitraum_text(Sprache::En, Zeitraum::Tage), "7 days");

        // Der Kopiertext nennt die Angaben und lässt Leeres weg.
        let text = aufruf_text(Sprache::De, &aufruf());
        for erwartet in [
            "Zeit: 2026-01-02T03:04:05Z",
            "Agent: pi",
            "Tool: echo__echo",
            "Dauer: 12 ms",
            "Status: OK",
            r#"Argumente: {"text":"hallo"}"#,
            "Vorschau der Antwort: hallo",
        ] {
            assert!(text.contains(erwartet), "{erwartet} fehlt in {text}");
        }
        assert!(!text.contains("Fehlertext"), "kein Fehler, kein Text");
        let mut mit_fehler = aufruf();
        mit_fehler.fehler = Some("kaputt".into());
        mit_fehler.vorschau.clear();
        let text = aufruf_text(Sprache::En, &mit_fehler);
        assert!(text.contains("Error text: kaputt"), "{text}");
        assert!(!text.contains("Response preview"), "leer heißt weg");
    }

    #[test]
    fn tray_menu_hat_die_vereinbarte_reihenfolge() {
        let menu = tray_menu(&modell(verbunden()), true, Sprache::De);
        assert_eq!(menu.len(), 5);
        assert!(
            matches!(&menu[0], TrayEintrag::Zustand(text) if text == "Verbunden · 0 Tools · 1 Agent")
        );
        assert!(matches!(&menu[1], TrayEintrag::Zeigen(text) if text == "Fenster öffnen"));
        assert!(matches!(
            &menu[2],
            TrayEintrag::Autostart { text, an: true } if text == "Beim Anmelden starten"
        ));
        assert_eq!(menu[3], TrayEintrag::Trenner);
        assert!(matches!(&menu[4], TrayEintrag::Beenden(text) if text == "Beenden"));
        // Englisch, Autostart aus.
        let menu = tray_menu(&modell(verbunden()), false, Sprache::En);
        assert!(
            matches!(&menu[2], TrayEintrag::Autostart { text, an: false } if text == "Start at login")
        );
        assert!(matches!(&menu[4], TrayEintrag::Beenden(text) if text == "Quit"));
    }

    /// Ein anstehendes Update steht vorn in der Tray-Zeile (Aufgabe 34) – der
    /// Satz kommt aus der Vorgabe der Firma und nennt die Version schon.
    #[test]
    fn ein_update_steht_vorn_in_der_tray_zeile() {
        let mut m = modell(verbunden());
        assert_eq!(tray_zeile(&m, Sprache::De), "Verbunden · 0 Tools · 1 Agent");
        m.update = quiver_client::UpdateLage::Verfuegbar {
            version: String::from("0.3.0"),
            hinweis: String::from("Version 0.3.0 ist verfügbar."),
            aenderungen: None,
        };
        assert_eq!(
            tray_zeile(&m, Sprache::De),
            "Version 0.3.0 ist verfügbar. · Verbunden · 0 Tools · 1 Agent"
        );
        // Ohne Angebot bleibt die Zeile, wie sie war.
        m.update = quiver_client::UpdateLage::Keins;
        assert_eq!(tray_zeile(&m, Sprache::De), "Verbunden · 0 Tools · 1 Agent");
    }

    /// Ist der Autostart vorgegeben, zeigt das Tray die Vorgabe (Aufgabe 34).
    #[test]
    fn eine_autostart_vorgabe_steht_im_tray() {
        let mut m = modell(verbunden());
        let menu = tray_menu(&m, false, Sprache::De);
        assert!(matches!(&menu[2], TrayEintrag::Autostart { an: false, .. }));
        m.vorgaben.autostart = Some(true);
        let menu = tray_menu(&m, false, Sprache::De);
        assert!(
            matches!(&menu[2], TrayEintrag::Autostart { an: true, .. }),
            "die Vorgabe gewinnt"
        );
    }

    #[test]
    fn argumente_lesen_hintergrund_und_kopflos() {
        assert_eq!(argumente(Vec::<&str>::new()), Argumente::default());
        let hintergrund = argumente(["--hintergrund"]);
        assert!(hintergrund.hintergrund);
        assert!(
            !hintergrund.kopflos,
            "ein Argument schaltet nicht das andere"
        );
        let kopflos = argumente(["--headless"]);
        assert!(kopflos.kopflos);
        assert!(!kopflos.hintergrund);
        // Reihenfolge und Unbekanntes sind egal.
        let beide = argumente(["--unbekannt", "--hintergrund", "--headless"]);
        assert!(beide.hintergrund && beide.kopflos);
    }
}
