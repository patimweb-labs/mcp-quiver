// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Einrichtung eines Eintrags (Aufgabe 11).
//!
//! Tools wie quiver-outline, quiver-m365 oder quiver-abap verwalten ihre
//! Zugänge in **Profilen** (mehrere Wikis, Postfächer, Hosts, SAP-Systeme).
//! Eingerichtet wird auf einer Seite, die **das Tool selbst** im Browser
//! öffnet – nie über einen Agenten. Das Datenmodell steht in
//! [`quiver_domain::Einrichtung`]: **eine** Argumentliste für **dasselbe**
//! Programm mit **derselben** Umgebung und **denselben** `${feld}`-Ersetzungen
//! wie die Laufzeit (siehe [`crate::anmeldung::befehl_aus_liste`]).
//!
//! Anders als die Anmeldung hat die Einrichtung **keine Frist**: der User
//! arbeitet auf der Seite, der Befehl endet von selbst. Der Zustand bleibt
//! lokal und geht nie an den Server (kein Vertragswechsel, AGENTS.md §2.1).

use quiver_domain::{EintragId, SollEintrag, Werte};
use serde::{Deserialize, Serialize};

use crate::anmeldung::{befehl_aus_liste, befehl_laufen, Ausgang, Befehl};

/// Wie es um die Einrichtung eines Eintrags steht. Nur für das Fenster; der
/// Server bekommt sie nie zu sehen. `None` im [`crate::EintragSicht`] heißt:
/// der Eintrag bietet gar keine Einrichtung an.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum EinrichtZustand {
    /// Der Eintrag lässt sich einrichten (die Seite war vielleicht schon offen).
    Bereit,
    /// Der Befehl hat die Seite im Browser geöffnet und wartet auf den User.
    Laeuft,
    Fehler {
        text: String,
    },
}

impl EinrichtZustand {
    /// Läuft gerade eine Einrichtung? Dann tut ein zweiter Klick nichts.
    pub fn ist_laeuft(&self) -> bool {
        matches!(self, EinrichtZustand::Laeuft)
    }
}

/// Die Befehlszeile der Einrichtung bauen.
///
/// Programm, Umgebung und Platzhalter sind dieselben wie in der Laufzeit des
/// Eintrags; die Argumente aus `einrichtung.einrichten` werden genauso
/// ersetzt. Nur `Stdio`-Einträge haben ein Programm.
pub fn befehl_bauen(soll: &SollEintrag, lokal: &Werte, geheim: &Werte) -> anyhow::Result<Befehl> {
    let einrichtung = soll
        .eintrag
        .einrichtung
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Eintrag {} hat keine Einrichtung", soll.eintrag.id))?;
    if einrichtung.einrichten.is_empty() {
        anyhow::bail!(
            "Für die Einrichtung von {} fehlt der Befehl",
            soll.eintrag.id
        );
    }
    befehl_aus_liste(soll, &einrichtung.einrichten, lokal, geheim)
}

/// Die Einrichtung eines Eintrags starten und auf ihr Ende warten.
///
/// **Ohne Frist:** der Befehl öffnet die Seite im Browser und endet, wenn der
/// User sie abschließt (Exit 0) oder abbricht (Exit 1).
pub async fn laufen(id: &EintragId, befehl: &Befehl) -> anyhow::Result<Ausgang> {
    befehl_laufen(id, befehl, None).await
}

/// Was aus dem Ausgang eines Einrichtungs-Befehls folgt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Ergebnis {
    /// Fertig. `neustart` nur, wenn die Einrichtung neue Profile angelegt hat
    /// (Exit 0) – dann muss der Eintrag sie sofort sehen. Exit 1 heißt: der
    /// User hat abgebrochen, es gibt nichts Neues.
    Fertig {
        neustart: bool,
    },
    Fehler {
        text: String,
    },
}

/// Exit 0 = eingerichtet, Exit 1 = abgebrochen, alles andere ein Fehler.
pub fn ergebnis_aus(ausgang: &Ausgang) -> Ergebnis {
    match ausgang.code {
        Some(0) => Ergebnis::Fertig { neustart: true },
        Some(1) => Ergebnis::Fertig { neustart: false },
        Some(code) => Ergebnis::Fehler {
            text: format!("Die Einrichtung endet mit Code {code}"),
        },
        None => Ergebnis::Fehler {
            text: "Die Einrichtung wurde beendet".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quiver_domain::{Einrichtung, Eintrag, EintragId, Herkunft, Laufzeit, Quelle, SollEintrag};

    fn feld(name: &str, quelle: Quelle) -> quiver_domain::Feld {
        quiver_domain::Feld {
            name: name.into(),
            beschreibung: String::new(),
            quelle,
            pflicht: true,
            standard: None,
            link: None,
            ..Default::default()
        }
    }

    fn eintrag(laufzeit: Laufzeit, einrichtung: Option<Einrichtung>) -> SollEintrag {
        SollEintrag {
            eintrag: Eintrag {
                id: EintragId::neu("outline"),
                version: "1.0.0".into(),
                titel: "Outline".into(),
                beschreibung: String::new(),
                mehrfach: None,
                herkunft: Herkunft::Eigen,
                laufzeit,
                felder: vec![feld("ABLAGE", Quelle::Server), feld("PFAD", Quelle::Client)],
                aktiv: true,
                anmeldung: None,
                einrichtung,
            },
            werte: Werte::from([("ABLAGE".to_string(), "/tmp/profil".to_string())]),
        }
    }

    fn stdio() -> Laufzeit {
        Laufzeit::Stdio {
            befehl: "${PFAD}".into(),
            argumente: vec!["mcp".into()],
            umgebung: Werte::from([("QUIVER_ABLAGE".to_string(), "${ABLAGE}".to_string())]),
        }
    }

    fn einrichtung() -> Option<Einrichtung> {
        Some(Einrichtung {
            einrichten: vec!["einrichten".into(), "${ABLAGE}".into()],
        })
    }

    #[test]
    fn befehl_hat_programm_umgebung_und_ersetzte_argumente() {
        let soll = eintrag(stdio(), einrichtung());
        let lokal = Werte::from([("PFAD".to_string(), "/opt/quiver/outline".to_string())]);
        let befehl = befehl_bauen(&soll, &lokal, &Werte::new()).expect("Befehl");
        assert_eq!(befehl.programm, "/opt/quiver/outline");
        assert_eq!(
            befehl.argumente,
            vec!["einrichten".to_string(), "/tmp/profil".to_string()],
            "dieselben Platzhalter wie in der Laufzeit"
        );
        assert_eq!(
            befehl.umgebung.get("QUIVER_ABLAGE").map(String::as_str),
            Some("/tmp/profil"),
            "dieselbe Umgebung wie die Laufzeit"
        );
        assert_eq!(befehl.zeile(), "/opt/quiver/outline einrichten /tmp/profil");
    }

    #[test]
    fn ohne_einrichtung_oder_ohne_befehl_gibt_es_keine_zeile() {
        let ohne = eintrag(stdio(), None);
        let fehler =
            befehl_bauen(&ohne, &Werte::new(), &Werte::new()).expect_err("keine Einrichtung");
        assert!(fehler.to_string().contains("Einrichtung"), "{fehler}");

        let leer = eintrag(
            stdio(),
            Some(Einrichtung {
                einrichten: Vec::new(),
            }),
        );
        assert!(befehl_bauen(&leer, &Werte::new(), &Werte::new()).is_err());
    }

    #[test]
    fn entfernte_eintraege_haben_kein_programm() {
        let entfernt = eintrag(
            Laufzeit::Remote {
                url: "https://example.org/mcp".into(),
                kopfzeilen: Werte::new(),
            },
            einrichtung(),
        );
        let fehler = befehl_bauen(
            &entfernt,
            &Werte::from([("PFAD".to_string(), "/opt/quiver/outline".to_string())]),
            &Werte::new(),
        )
        .expect_err("Remote");
        assert!(fehler.to_string().contains("Stdio"), "{fehler}");
    }

    fn ausgang(code: Option<i32>) -> Ausgang {
        Ausgang {
            code,
            ausgabe: String::new(),
        }
    }

    #[test]
    fn exit_null_startet_neu_exit_eins_nicht_alles_andere_ist_fehler() {
        assert_eq!(
            ergebnis_aus(&ausgang(Some(0))),
            Ergebnis::Fertig { neustart: true },
            "Exit 0 heißt: neue Profile wirken sofort"
        );
        assert_eq!(
            ergebnis_aus(&ausgang(Some(1))),
            Ergebnis::Fertig { neustart: false },
            "Exit 1 heißt abgebrochen, nicht Fehler"
        );
        assert_eq!(
            ergebnis_aus(&ausgang(Some(3))),
            Ergebnis::Fehler {
                text: "Die Einrichtung endet mit Code 3".to_string()
            }
        );
        assert!(matches!(
            ergebnis_aus(&ausgang(None)),
            Ergebnis::Fehler { .. }
        ));
    }

    #[test]
    fn nur_laeuft_sperrt_den_zweiten_klick() {
        assert!(EinrichtZustand::Laeuft.ist_laeuft());
        assert!(!EinrichtZustand::Bereit.ist_laeuft());
        assert!(!EinrichtZustand::Fehler {
            text: "kaputt".into()
        }
        .ist_laeuft());
    }
}
