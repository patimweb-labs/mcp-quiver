// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Grenzen für das, was die Verwaltung und die Geräte annehmen.
//!
//! Jede Grenze steht hier einmal und wird vor dem Speichern geprüft; eine
//! Überschreitung ist [`axum::http::StatusCode::UNPROCESSABLE_ENTITY`] (422),
//! nicht 400: der Aufruf ist wohlgeformt, nur zu groß.
//!
//! Die Zahlen sind großzügig gegenüber echten Einträgen (`server.json` mit
//! vielen Feldern) und klein genug, dass ein einzelner Aufruf die Datenbank
//! nicht flutet.

use quiver_domain::{Eintrag, Gruppe, Laufzeit, Satz, SatzEintrag, Werte};
use quiver_protocol::Meldung;

/// Längster Name (Kennung von Eintrag, Satz, Gruppe; Name eines Geräts).
pub const NAME_MAX: usize = 80;
/// Längster Titel.
pub const TITEL_MAX: usize = 120;
/// Längste Beschreibung.
pub const BESCHREIBUNG_MAX: usize = 1000;
/// So viele Felder darf ein Eintrag haben.
pub const FELDER_MAX: usize = 50;
/// Länge einer Feldbeschreibung.
pub const FELDBESCHREIBUNG_MAX: usize = 500;
/// So viele Optionen darf eine Auswahl haben.
pub const OPTIONEN_MAX: usize = 50;
/// So viele Argumente darf ein Programm bekommen.
pub const ARGUMENTE_MAX: usize = 50;
/// Länge eines Arguments.
pub const ARGUMENT_MAX: usize = 4000;
/// So viele Umgebungs- bzw. Kopfzeilen-Einträge.
pub const UMGEBUNG_MAX: usize = 50;
/// Länge eines Umgebungs- oder Kopfzeilenwerts.
pub const UMGEBUNGSWERT_MAX: usize = 4000;
/// So viele Einträge einer Meldung werden gespeichert.
pub const MELDUNG_EINTRAEGE_MAX: usize = 200;
/// Länge eines Fehlertexts in einer Meldung.
pub const MELDUNG_TEXT_MAX: usize = 500;
/// Länge der Client-Fassung in einer Meldung.
pub const MELDUNG_VERSION_MAX: usize = 64;
/// So viele Programme darf ein Gerät in einer Meldung nennen (Aufgabe 37).
pub const PROGRAMME_MAX: usize = 200;
/// So viele Sätze darf eine Gruppe nennen.
pub const SAETZE_MAX: usize = 200;
/// So viele Einträge darf ein Satz nennen.
pub const SATZEINTRAEGE_MAX: usize = 500;

/// Ein Text mit höchstens `max` Zeichen (nicht Bytes: Umlaute zählen als eins).
fn laenge(wert: &str) -> usize {
    wert.chars().count()
}

fn pruefe_text(feld: &str, wert: &str, max: usize) -> Result<(), String> {
    if laenge(wert) > max {
        return Err(format!(
            "{feld} ist zu lang ({} Zeichen, höchstens {max})",
            laenge(wert)
        ));
    }
    Ok(())
}

fn pruefe_karte(wo: &str, karte: &Werte) -> Result<(), String> {
    if karte.len() > UMGEBUNG_MAX {
        return Err(format!(
            "{wo} hat {} Einträge (höchstens {UMGEBUNG_MAX})",
            karte.len()
        ));
    }
    for (name, wert) in karte {
        pruefe_text(&format!("{wo}: Name „{name}\""), name, NAME_MAX)?;
        pruefe_text(&format!("{wo}: Wert zu „{name}\""), wert, UMGEBUNGSWERT_MAX)?;
    }
    Ok(())
}

/// Paßt der Eintrag in die Grenzen? `Err` trägt den Text für die Antwort.
pub fn eintrag_pruefen(eintrag: &Eintrag) -> Result<(), String> {
    pruefe_text("Kennung", eintrag.id.als_str(), NAME_MAX)?;
    if eintrag.id.als_str().trim().is_empty() {
        return Err(String::from("Kennung fehlt"));
    }
    pruefe_text("Titel", &eintrag.titel, TITEL_MAX)?;
    if eintrag.titel.trim().is_empty() {
        return Err(String::from("Titel fehlt"));
    }
    pruefe_text("Beschreibung", &eintrag.beschreibung, BESCHREIBUNG_MAX)?;
    pruefe_text("Fassung", &eintrag.version, NAME_MAX)?;

    match &eintrag.laufzeit {
        Laufzeit::Stdio {
            befehl,
            argumente,
            umgebung,
        } => {
            if befehl.trim().is_empty() {
                return Err(String::from("Befehl fehlt"));
            }
            pruefe_text("Befehl", befehl, UMGEBUNGSWERT_MAX)?;
            if argumente.len() > ARGUMENTE_MAX {
                return Err(format!(
                    "zu viele Argumente ({} , höchstens {ARGUMENTE_MAX})",
                    argumente.len()
                ));
            }
            for (nummer, argument) in argumente.iter().enumerate() {
                pruefe_text(&format!("Argument {}", nummer + 1), argument, ARGUMENT_MAX)?;
            }
            pruefe_karte("Umgebung", umgebung)?;
        }
        Laufzeit::Remote { url, kopfzeilen } => {
            if url.trim().is_empty() {
                return Err(String::from("Adresse fehlt"));
            }
            pruefe_text("Adresse", url, UMGEBUNGSWERT_MAX)?;
            pruefe_karte("Kopfzeilen", kopfzeilen)?;
        }
    }

    if eintrag.felder.len() > FELDER_MAX {
        return Err(format!(
            "zu viele Felder ({} , höchstens {FELDER_MAX})",
            eintrag.felder.len()
        ));
    }
    for feld in &eintrag.felder {
        pruefe_text("Feldname", &feld.name, NAME_MAX)?;
        if feld.name.trim().is_empty() {
            return Err(String::from("ein Feld hat keinen Namen"));
        }
        pruefe_text(
            &format!("Beschreibung von Feld „{}“", feld.name),
            &feld.beschreibung,
            FELDBESCHREIBUNG_MAX,
        )?;
        if let Some(titel) = feld.titel.as_deref() {
            pruefe_text(
                &format!("Anzeige von Feld „{}“", feld.name),
                titel,
                TITEL_MAX,
            )?;
        }
        if let Some(standard) = feld.standard.as_deref() {
            pruefe_text(
                &format!("Standard von Feld „{}“", feld.name),
                standard,
                UMGEBUNGSWERT_MAX,
            )?;
        }
        if let quiver_domain::FeldArt::Auswahl { optionen } = &feld.art {
            if optionen.len() > OPTIONEN_MAX {
                return Err(format!(
                    "Feld „{}“ hat zu viele Optionen (höchstens {OPTIONEN_MAX})",
                    feld.name
                ));
            }
            for option in optionen {
                pruefe_text("Optionswert", &option.wert, UMGEBUNGSWERT_MAX)?;
                pruefe_text("Optionsanzeige", &option.anzeige, TITEL_MAX)?;
            }
        }
    }

    if let Some(mehrfach) = &eintrag.mehrfach {
        pruefe_text("Überschrift der Zugangsliste", &mehrfach.titel, TITEL_MAX)?;
        pruefe_text("Bezeichnung eines Zugangs", &mehrfach.einzahl, TITEL_MAX)?;
        if mehrfach.felder.len() > FELDER_MAX {
            return Err(format!(
                "zu viele Felder in der Zugangsliste (höchstens {FELDER_MAX})"
            ));
        }
    }
    for (name, liste) in [
        ("Anmelden", eintrag.anmeldung.as_ref().map(|a| &a.anmelden)),
        ("Status", eintrag.anmeldung.as_ref().map(|a| &a.status)),
        ("Abmelden", eintrag.anmeldung.as_ref().map(|a| &a.abmelden)),
        (
            "Einrichten",
            eintrag.einrichtung.as_ref().map(|e| &e.einrichten),
        ),
    ] {
        let Some(liste) = liste else { continue };
        if liste.len() > ARGUMENTE_MAX {
            return Err(format!("{name}: zu viele Argumente"));
        }
        for argument in liste {
            pruefe_text(&format!("{name}: Argument"), argument, ARGUMENT_MAX)?;
        }
    }
    Ok(())
}

/// Grenzen eines Satzes.
pub fn satz_pruefen(satz: &Satz) -> Result<(), String> {
    pruefe_text("Kennung", satz.id.als_str(), NAME_MAX)?;
    if satz.id.als_str().trim().is_empty() {
        return Err(String::from("Kennung fehlt"));
    }
    pruefe_text("Titel", &satz.titel, TITEL_MAX)?;
    if satz.eintraege.len() > SATZEINTRAEGE_MAX {
        return Err(format!(
            "zu viele Einträge im Satz ({} , höchstens {SATZEINTRAEGE_MAX})",
            satz.eintraege.len()
        ));
    }
    for SatzEintrag { eintrag, werte } in &satz.eintraege {
        pruefe_text("Eintrag im Satz", eintrag.als_str(), NAME_MAX)?;
        pruefe_karte(&format!("Werte für „{eintrag}“"), werte)?;
    }
    Ok(())
}

/// Grenzen einer Gruppe.
pub fn gruppe_pruefen(gruppe: &Gruppe) -> Result<(), String> {
    pruefe_text("Kennung", gruppe.id.als_str(), NAME_MAX)?;
    if gruppe.id.als_str().trim().is_empty() {
        return Err(String::from("Kennung fehlt"));
    }
    pruefe_text("Titel", &gruppe.titel, TITEL_MAX)?;
    if gruppe.saetze.len() > SAETZE_MAX {
        return Err(format!(
            "zu viele Sätze ({} , höchstens {SAETZE_MAX})",
            gruppe.saetze.len()
        ));
    }
    Ok(())
}

/// Eine Meldung auf die gespeicherte Form kürzen.
///
/// Statt die Meldung abzulehnen (der Client hätte dann keinen Weg, seinen
/// Zustand loszuwerden), wird gekürzt: höchstens
/// [`MELDUNG_EINTRAEGE_MAX`] Einträge, Texte auf [`MELDUNG_TEXT_MAX`].
pub fn meldung_kuerzen(meldung: &mut Meldung) {
    let mut version: String = meldung
        .client_version
        .chars()
        .take(MELDUNG_VERSION_MAX)
        .collect();
    version.retain(|zeichen| !zeichen.is_control());
    meldung.client_version = version;
    meldung.eintraege.truncate(MELDUNG_EINTRAEGE_MAX);
    // Die Programmliste kommt vom Gerät: auch sie hat eine Obergrenze.
    meldung.programme.truncate(PROGRAMME_MAX);
    for (_, zustand) in &mut meldung.eintraege {
        if let quiver_protocol::EintragZustand::Fehler { text } = zustand {
            let mut gekuerzt: String = text.chars().take(MELDUNG_TEXT_MAX).collect();
            if text.chars().count() > MELDUNG_TEXT_MAX {
                gekuerzt.push('…');
            }
            *text = gekuerzt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_domain::{EintragId, Feld, GruppeId, Herkunft, Quelle, SatzId};

    fn eintrag() -> Eintrag {
        Eintrag {
            id: EintragId::neu("echo"),
            version: String::from("1.0.0"),
            titel: String::from("Echo"),
            beschreibung: String::new(),
            herkunft: Herkunft::Eigen,
            laufzeit: Laufzeit::Stdio {
                befehl: String::from("echo"),
                argumente: vec![String::from("hallo")],
                umgebung: Werte::new(),
            },
            felder: vec![Feld {
                name: String::from("pfad"),
                quelle: Quelle::Client,
                ..Feld::default()
            }],
            aktiv: true,
            anmeldung: None,
            einrichtung: None,
            mehrfach: None,
        }
    }

    #[test]
    fn ein_normaler_eintrag_passt() {
        assert_eq!(eintrag_pruefen(&eintrag()), Ok(()));
    }

    #[test]
    fn zu_lange_texte_werden_abgelehnt() {
        let mut lang = eintrag();
        lang.titel = "T".repeat(TITEL_MAX + 1);
        assert!(eintrag_pruefen(&lang).unwrap_err().contains("Titel"));

        let mut lang = eintrag();
        lang.beschreibung = "B".repeat(BESCHREIBUNG_MAX + 1);
        assert!(eintrag_pruefen(&lang).unwrap_err().contains("Beschreibung"));

        let mut lang = eintrag();
        lang.id = EintragId::neu("i".repeat(NAME_MAX + 1));
        assert!(eintrag_pruefen(&lang).unwrap_err().contains("Kennung"));

        let mut lang = eintrag();
        lang.felder[0].name = "f".repeat(NAME_MAX + 1);
        assert!(eintrag_pruefen(&lang).unwrap_err().contains("Feldname"));
    }

    #[test]
    fn zu_viele_argumente_und_umgebung() {
        let mut viel = eintrag();
        if let Laufzeit::Stdio { argumente, .. } = &mut viel.laufzeit {
            *argumente = (0..=ARGUMENTE_MAX).map(|i| i.to_string()).collect();
        }
        assert!(eintrag_pruefen(&viel).unwrap_err().contains("Argumente"));

        let mut viel = eintrag();
        if let Laufzeit::Stdio { umgebung, .. } = &mut viel.laufzeit {
            for i in 0..=UMGEBUNG_MAX {
                umgebung.insert(format!("Q{i}"), String::from("x"));
            }
        }
        assert!(eintrag_pruefen(&viel).unwrap_err().contains("Umgebung"));
    }

    #[test]
    fn satz_und_gruppe_haben_grenzen() {
        let satz = Satz {
            id: SatzId::neu("s"),
            titel: String::from("Satz"),
            prioritaet: 0,
            eintraege: Vec::new(),
        };
        assert_eq!(satz_pruefen(&satz), Ok(()));
        let mut zu_lang = satz.clone();
        zu_lang.titel = "T".repeat(TITEL_MAX + 1);
        assert!(satz_pruefen(&zu_lang).is_err());

        let gruppe = Gruppe {
            id: GruppeId::neu("g"),
            titel: String::from("Gruppe"),
            saetze: Vec::new(),
            entra: Vec::new(),
        };
        assert_eq!(gruppe_pruefen(&gruppe), Ok(()));
        let mut zu_viel = gruppe.clone();
        zu_viel.saetze = (0..=SAETZE_MAX)
            .map(|i| SatzId::neu(format!("s{i}")))
            .collect();
        assert!(gruppe_pruefen(&zu_viel).is_err());
    }

    #[test]
    fn meldung_wird_gekuerzt_statt_abgelehnt() {
        let mut meldung = Meldung {
            revision: String::from("r1"),
            client_version: "v".repeat(MELDUNG_VERSION_MAX + 10),
            eintraege: (0..MELDUNG_EINTRAEGE_MAX + 10)
                .map(|i| {
                    (
                        EintragId::neu(format!("e{i}")),
                        quiver_protocol::EintragZustand::Fehler {
                            text: "x".repeat(MELDUNG_TEXT_MAX + 10),
                        },
                    )
                })
                .collect(),
            programme: Vec::new(),
        };
        meldung_kuerzen(&mut meldung);
        assert_eq!(meldung.client_version.chars().count(), MELDUNG_VERSION_MAX);
        assert_eq!(meldung.eintraege.len(), MELDUNG_EINTRAEGE_MAX);
        match &meldung.eintraege[0].1 {
            quiver_protocol::EintragZustand::Fehler { text } => {
                assert_eq!(text.chars().count(), MELDUNG_TEXT_MAX + 1, "gekürzt mit …")
            }
            anderes => panic!("unerwartet: {anderes:?}"),
        }
    }
}
