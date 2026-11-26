// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Die Anmeldung eines Eintrags (SPEC.md §7, Aufgabe 10).
//!
//! Ein Tool kann eine Anmeldung des Users brauchen (Beispiel: quiver-m365,
//! Microsoft im Browser). **Die Anmeldung löst der User im Fenster aus – nie
//! ein Agent.** Das Datenmodell steht in [`quiver_domain::Anmeldung`]: drei
//! Argumentlisten für **dasselbe** Programm, das den Eintrag startet, mit
//! **derselben** Umgebung und **denselben** `${feld}`-Ersetzungen wie die
//! Laufzeit.
//!
//! Hier liegt, was ohne Laufzeit prüfbar ist: die Befehlszeile und die
//! Auswertung des Status-Befehls. Der Zustand bleibt lokal und geht nie an
//! den Server (kein Vertragswechsel, AGENTS.md §2.1).

use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use quiver_domain::{Anmeldung, EintragId, Laufzeit, SollEintrag, Werte};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, BufReader};

use crate::hilfen::Abbruch;
use crate::protokoll::Protokoll;

/// Wie es um die Anmeldung eines Eintrags steht. Nur für das Fenster; der
/// Server bekommt sie nie zu sehen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum AnmeldeZustand {
    /// Noch nicht geprüft.
    Unbekannt,
    /// Der Status-Befehl läuft gerade. Das Fenster zeigt „Prüfe Anmeldung …“
    /// mit Drehsymbol und keinen Knopf: die Prüfung hat eine Frist und endet
    /// mit einem Satz (Aufgabe 35, §5).
    Pruefe,
    /// Angemeldet; `konto` ist der Anzeigename aus der ersten Zeile von stdout.
    Angemeldet {
        konto: Option<String>,
    },
    /// Angemeldet, aber die Anmeldung läuft bald ab: ein Werkzeug, das den
    /// Ablauf kennt, meldet ihn mit Exit 3 ([`zustand_aus`]).
    LaeuftAb {
        konto: Option<String>,
    },
    Abgemeldet,
    /// Die Anmeldung läuft – der Befehl öffnet den Browser und wartet.
    Laeuft,
    Fehler {
        text: String,
    },
}

/// Wie lange der Status-Befehl höchstens dauern darf. Zehn Sekunden: eine
/// Prüfung ist ein Blick in den Tresor, kein Browser-Ablauf. Antwortet das
/// Werkzeug nicht, sagt die Karte das und bietet „Erneut prüfen“ an, statt
/// dass ein Drehsymbol stehen bleibt (Aufgabe 35, §5).
pub const STATUS_FRIST: Duration = Duration::from_secs(10);
/// Die Anmeldung im Browser darf lange dauern (der User muss mitmachen).
pub const ANMELDEN_FRIST: Duration = Duration::from_secs(5 * 60);
/// Das Abmelden ist ein Aufräumschritt und braucht keine fünf Minuten.
pub const ABMELDEN_FRIST: Duration = Duration::from_secs(60);
/// So lange darf die Einrichtung im Browser höchstens dauern – danach wird
/// der Prozess beendet, damit kein hängender Kindprozess zurückbleibt.
pub const EINRICHT_FRIST: Duration = Duration::from_secs(15 * 60);
/// So viel stdout eines Anmelde- oder Einrichtungsbefehls wird gelesen. Was
/// darüber hinausgeht, wird verworfen – ein Befehl soll den Client nicht mit
/// einer riesigen Ausgabe füllen.
pub const AUSGABE_BYTES: u64 = 64 * 1024;
/// So groß ist der Puffer einer Ausgabezeile.
pub const AUSGABE_ZEILE_BYTES: usize = 8 * 1024;

/// Welcher der drei Befehle eines Eintrags gemeint ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anmeldebefehl {
    Anmelden,
    Status,
    Abmelden,
}

impl Anmeldebefehl {
    /// Die Argumentliste aus dem Datenmodell.
    pub fn liste(self, anmeldung: &Anmeldung) -> &[String] {
        match self {
            Anmeldebefehl::Anmelden => &anmeldung.anmelden,
            Anmeldebefehl::Status => &anmeldung.status,
            Anmeldebefehl::Abmelden => &anmeldung.abmelden,
        }
    }

    /// Wie lange dieser Befehl höchstens dauern darf.
    pub fn frist(self) -> Duration {
        match self {
            Anmeldebefehl::Anmelden => ANMELDEN_FRIST,
            Anmeldebefehl::Status => STATUS_FRIST,
            Anmeldebefehl::Abmelden => ABMELDEN_FRIST,
        }
    }

    /// Name für Meldungen und das Protokoll.
    pub fn name(self) -> &'static str {
        match self {
            Anmeldebefehl::Anmelden => "anmelden",
            Anmeldebefehl::Status => "status",
            Anmeldebefehl::Abmelden => "abmelden",
        }
    }
}

/// Eine fertige Befehlszeile: Programm, Argumente, Umgebung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Befehl {
    pub programm: String,
    pub argumente: Vec<String>,
    pub umgebung: Werte,
}

impl Befehl {
    /// Die Zeile zum Nachlesen im Protokoll (ohne Geheimwerte der Umgebung).
    pub fn zeile(&self) -> String {
        if self.argumente.is_empty() {
            self.programm.clone()
        } else {
            format!("{} {}", self.programm, self.argumente.join(" "))
        }
    }
}

/// Was im Protokoll statt eines Geheimwerts steht.
pub const VERBORGEN: &str = "<verborgen>";

/// Wörter, nach denen der Rest einer Zeile nie im Protokoll steht.
const GEHEIMWOERTER: [&str; 5] = ["token", "password", "passwort", "secret", "bearer"];

/// Eine Logzeile entschärfen: Einmal-Token in Einrichtungsadressen
/// (`/e/<token>/`) und alles hinter einem Geheimwort werden verborgen.
///
/// Reine Funktion, damit sie prüfbar ist: `quiver-<tool> einrichten` gibt die
/// Adresse seiner Einrichtungsseite samt Einmal-Token aus – die darf nicht im
/// Protokoll landen (Aufgabe 12, Teil B).
pub fn maskieren(zeile: &str) -> String {
    nach_geheimwort(&adressen_maskieren(zeile))
}

/// `/e/<hex>/` → `/e/<verborgen>/`.
fn adressen_maskieren(zeile: &str) -> String {
    let mut aus = String::with_capacity(zeile.len());
    let mut rest = zeile;
    while let Some(stelle) = rest.find("/e/") {
        let (davor, ab) = rest.split_at(stelle);
        aus.push_str(davor);
        aus.push_str("/e/");
        let nach = &ab[3..];
        let hex = nach.bytes().take_while(u8::is_ascii_hexdigit).count();
        let ende = &nach[hex..];
        if hex >= 4 && (ende.is_empty() || ende.starts_with('/')) {
            aus.push_str(VERBORGEN);
            rest = ende;
        } else {
            // Kein Token: `/e/` bleibt stehen, dahinter geht es normal weiter.
            rest = nach;
        }
    }
    aus.push_str(rest);
    aus
}

/// Alles hinter dem ersten Geheimwort (`token`, `password`, `passwort`,
/// `secret`, `Bearer`) verbinden – das Schlüsselwort selbst bleibt lesbar.
fn nach_geheimwort(zeile: &str) -> String {
    let klein = zeile.to_ascii_lowercase();
    let treffer = GEHEIMWOERTER
        .iter()
        .filter_map(|wort| klein.find(wort).map(|stelle| (stelle, *wort)))
        .min_by_key(|(stelle, _)| *stelle);
    let Some((stelle, wort)) = treffer else {
        return zeile.to_string();
    };
    let nach = &zeile[stelle + wort.len()..];
    let trenner = nach.len()
        - nach
            .trim_start_matches(|zeichen: char| {
                zeichen.is_whitespace() || zeichen == '=' || zeichen == ':' || zeichen == '"'
            })
            .len();
    let wert = nach[trenner..].trim();
    if wert.is_empty() {
        // Hinter dem Wort steht nichts, was man verbergen müsste.
        return zeile.to_string();
    }
    let mut aus = zeile[..stelle + wort.len() + trenner].to_string();
    aus.push_str(VERBORGEN);
    aus
}

/// Die Befehlszeile eines Anmelde-Befehls bauen.
///
/// Programm, Umgebung und Platzhalter kommen aus derselben Funktion wie die
/// Laufzeit selbst ([`quiver_domain::konfig_zusammensetzen`]); die Argumente
/// des Anmelde-Befehls werden genauso ersetzt. Nur `Stdio`-Einträge können
/// sich anmelden – bei einem entfernten MCP-Server gibt es kein Programm.
pub fn befehl_bauen(
    soll: &SollEintrag,
    art: Anmeldebefehl,
    lokal: &Werte,
    geheim: &Werte,
) -> anyhow::Result<Befehl> {
    let anmeldung =
        soll.eintrag.anmeldung.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Eintrag {} braucht keine Anmeldung", soll.eintrag.id)
        })?;
    let liste = art.liste(anmeldung);
    if liste.is_empty() {
        anyhow::bail!(
            "Für „{}“ fehlt der Befehl bei Eintrag {}",
            art.name(),
            soll.eintrag.id
        );
    }
    befehl_aus_liste(soll, liste, lokal, geheim)
}

/// Eine Befehlszeile aus einer Argumentliste bauen.
///
/// Programm, Umgebung und Platzhalter kommen aus derselben Funktion wie die
/// Laufzeit selbst ([`quiver_domain::konfig_zusammensetzen`]); die Argumente
/// der Liste werden genauso ersetzt. Gilt für die Anmeldung (Aufgabe 10) und
/// die Einrichtung (Aufgabe 11) eines Eintrags.
///
/// Nur `Stdio`-Einträge haben ein Programm – bei einem entfernten MCP-Server
/// gibt es nichts zu starten.
pub fn befehl_aus_liste(
    soll: &SollEintrag,
    liste: &[String],
    lokal: &Werte,
    geheim: &Werte,
) -> anyhow::Result<Befehl> {
    let laufzeit = quiver_domain::konfig_zusammensetzen(soll, lokal, geheim)
        .map_err(|fehlend| fehlende_angaben(&fehlend))?;
    let Laufzeit::Stdio {
        befehl, umgebung, ..
    } = laufzeit
    else {
        anyhow::bail!(
            "Einen Befehl gibt es nur für Stdio-Einträge, {} läuft entfernt",
            soll.eintrag.id
        );
    };

    // Dieselben Werte, dieselbe Ersetzung – nur für die Argumente dieses
    // Befehls: `konfig_zusammensetzen` füllt auch sie aus den Feldern des
    // Eintrags. Deshalb wird die Laufzeit des Eintrags für diese eine Frage
    // durch die Argumentliste ersetzt.
    let mut probe = soll.clone();
    probe.eintrag.laufzeit = Laufzeit::Stdio {
        befehl: String::new(),
        argumente: liste.to_vec(),
        umgebung: Werte::new(),
    };
    let Laufzeit::Stdio { argumente, .. } =
        quiver_domain::konfig_zusammensetzen(&probe, lokal, geheim)
            .map_err(|fehlend| fehlende_angaben(&fehlend))?
    else {
        unreachable!("die Probe ist ein Stdio-Eintrag");
    };

    Ok(Befehl {
        programm: befehl,
        argumente,
        umgebung,
    })
}

/// Was fehlt, als ein Satz – nie ein Geheimwert, immer nur der Feldname.
fn fehlende_angaben(fehlend: &[quiver_domain::FehlendesFeld]) -> anyhow::Error {
    let namen: Vec<&str> = fehlend.iter().map(|feld| feld.name.as_str()).collect();
    anyhow::anyhow!("Es fehlen Angaben: {}", namen.join(", "))
}

/// Was ein Befehl ausgegeben hat.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ausgang {
    /// Exit-Code; `None`, wenn der Prozess abgebrochen wurde.
    pub code: Option<i32>,
    /// stdout, unverändert – die erste Zeile ist der Anzeigename des Kontos.
    pub ausgabe: String,
}

/// Einen Anmelde-Befehl ausführen. stdout und stderr gehen ins Protokoll des
/// Clients (wie bei den MCP-Prozessen), nie in eine Meldung an den Server.
pub async fn laufen(
    id: &EintragId,
    befehl: &Befehl,
    frist: Duration,
    protokoll: &Arc<Protokoll>,
) -> anyhow::Result<Ausgang> {
    befehl_laufen(id, befehl, Some(frist), None, protokoll).await
}

/// Einen Befehl ausführen und auf sein Ende warten.
///
/// `frist: None` heißt: warten, bis der User fertig ist – so lief die
/// Einrichtung früher. Seit Paket 3 hat auch sie eine Frist
/// ([`EINRICHT_FRIST`]). Eine Frist für die Anmeldung gibt es, damit ein
/// hängender Status-Befehl den Kern nicht blockiert.
///
/// `abbruch` beendet den Kindprozess sofort – „Abbrechen“ im Fenster und das
/// Beenden des Clients räumen so jede laufende Einrichtung ab.
pub async fn befehl_laufen(
    id: &EintragId,
    befehl: &Befehl,
    frist: Option<Duration>,
    abbruch: Option<Abbruch>,
    protokoll: &Arc<Protokoll>,
) -> anyhow::Result<Ausgang> {
    tracing::info!(eintrag = %id, "Befehl: {}", maskieren(&befehl.zeile()));
    // Dieselbe Suche wie beim Start des Eintrags (Aufgabe 35).
    let fund = crate::tools::fuer_eintrag(id, &befehl.programm, protokoll)?;
    let mut kommando = tokio::process::Command::new(&fund.pfad);
    kommando
        .args(&befehl.argumente)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Kein Konsolenfenster (Windows): angemeldet wird im Browser, der
    // Status-Befehl ist ein Hintergrundvorgang (Aufgabe 35, §5).
    crate::hilfen::ohne_fenster(kommando.as_std_mut());
    // Dieselbe Whitelist wie bei den Werkzeugen (Paket 3).
    crate::laufzeit::umgebung_setzen(&mut kommando, &befehl.umgebung);
    let mut kind = kommando
        .spawn()
        // Nur der Programmname, nie der Pfad: die Meldung steht im Fenster.
        .with_context(|| format!("{} lässt sich nicht starten", fund.name()))?;

    let aus_aufgabe = kind
        .stdout
        .take()
        .map(|stdout| tokio::spawn(ausgabe_lesen(id.als_str().to_string(), stdout)));
    let err_aufgabe = kind
        .stderr
        .take()
        .map(|stderr| tokio::spawn(fehler_lesen(id.als_str().to_string(), stderr)));

    let gewartet = match (frist, &abbruch) {
        (None, None) => Some(kind.wait().await),
        (Some(frist), None) => tokio::time::timeout(frist, kind.wait()).await.ok(),
        (None, Some(abbruch)) => Some(tokio::select! {
            _ = abbruch.warten() => {
                let _ = kind.kill().await;
                anyhow::bail!("Der Befehl wurde abgebrochen")
            }
            status = kind.wait() => status,
        }),
        (Some(frist), Some(abbruch)) => Some(tokio::select! {
            _ = abbruch.warten() => {
                let _ = kind.kill().await;
                anyhow::bail!("Der Befehl wurde abgebrochen")
            }
            status = tokio::time::timeout(frist, kind.wait()) => match status {
                Ok(status) => status,
                Err(_) => {
                    let _ = kind.kill().await;
                    anyhow::bail!(
                        "{} antwortet nicht binnen {} s",
                        fund.name(),
                        frist.as_secs()
                    )
                }
            },
        }),
    };
    let Some(status) = gewartet else {
        let _ = kind.kill().await;
        if let Some(aufgabe) = &aus_aufgabe {
            aufgabe.abort();
        }
        if let Some(aufgabe) = &err_aufgabe {
            aufgabe.abort();
        }
        anyhow::bail!(
            "{} antwortet nicht binnen {} s",
            fund.name(),
            frist.map(|frist| frist.as_secs()).unwrap_or_default()
        );
    };
    let status = status.context("Der Befehl endet nicht ordentlich")?;
    let ausgabe = match aus_aufgabe {
        Some(aufgabe) => aufgabe.await.unwrap_or_default(),
        None => String::new(),
    };
    if let Some(aufgabe) = err_aufgabe {
        let _ = aufgabe.await;
    }
    Ok(Ausgang {
        code: status.code(),
        ausgabe,
    })
}

/// stdout einsammeln (höchstens [`AUSGABE_BYTES`]) und ins Protokoll schreiben.
async fn ausgabe_lesen(id: String, stdout: tokio::process::ChildStdout) -> String {
    let mut ausgabe = String::new();
    let mut begrenzt = stdout.take(AUSGABE_BYTES);
    if let Err(fehler) = begrenzt.read_to_string(&mut ausgabe).await {
        tracing::warn!(eintrag = %id, "Anmeldung: stdout nicht lesbar: {fehler}");
    }
    for zeile in ausgabe.lines() {
        tracing::info!(eintrag = %id, "{}", maskieren(&kuerzen(zeile)));
    }
    ausgabe
}

/// stderr Zeile für Zeile ins Protokoll schreiben – mit Puffer und gekürzt.
///
/// Gelesen wird mit [`crate::laufzeit::zeile_begrenzt`]: eine Zeile ohne Ende
/// füllt so nie den Speicher.
async fn fehler_lesen(id: String, stderr: tokio::process::ChildStderr) {
    let mut leser = BufReader::new(stderr);
    // Der Puffer einer Zeile ([`AUSGABE_ZEILE_BYTES`]) ist kleiner als der
    // allgemeine; deshalb wird hier nach dem Lesen noch einmal gekürzt.
    let mut zeile = Vec::new();
    loop {
        match crate::laufzeit::zeile_begrenzt(&mut leser, &mut zeile).await {
            Ok(Some(_)) => {}
            Ok(None) | Err(_) => return,
        }
        let text = String::from_utf8_lossy(&zeile[..zeile.len().min(AUSGABE_ZEILE_BYTES)]);
        tracing::info!(eintrag = %id, "{}", maskieren(&kuerzen(text.trim_end())));
    }
}

/// Eine Ausgabezeile auf das Protokollmaß kürzen.
fn kuerzen(zeile: &str) -> String {
    crate::protokoll::kuerzen(
        &crate::protokoll::anzeige_zeichen(zeile),
        crate::protokoll::ZEILE_ZEICHEN,
    )
}

/// Den Status-Befehl auswerten: Exit 0 = angemeldet, Exit 1 = abgemeldet,
/// Exit 3 = angemeldet, aber die Anmeldung läuft bald ab, alles andere ist ein
/// Fehler.
///
/// Exit 3 ist eine **Zusage an das Werkzeug**, nicht an den Server: wer den
/// Ablauf seines Tokens kennt, kann ihn melden, statt zu warten, bis der User
/// mitten in der Arbeit abgemeldet wird. Ein Werkzeug, das ihn nicht kennt,
/// meldet ihn nicht – dann bleibt es bei Exit 0.
pub fn zustand_aus(ausgang: &Ausgang) -> AnmeldeZustand {
    match ausgang.code {
        Some(0) => AnmeldeZustand::Angemeldet {
            konto: erste_zeile(&ausgang.ausgabe),
        },
        Some(1) => AnmeldeZustand::Abgemeldet,
        Some(3) => AnmeldeZustand::LaeuftAb {
            konto: erste_zeile(&ausgang.ausgabe),
        },
        Some(code) => AnmeldeZustand::Fehler {
            text: format!("Der Status endet mit Code {code}"),
        },
        None => AnmeldeZustand::Fehler {
            text: "Der Status wurde abgebrochen".to_string(),
        },
    }
}

/// Die erste Zeile von stdout; leer heißt „kein Konto bekannt".
fn erste_zeile(ausgabe: &str) -> Option<String> {
    let zeile = ausgabe.lines().next()?.trim();
    (!zeile.is_empty()).then(|| zeile.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ausgang(code: Option<i32>, ausgabe: &str) -> Ausgang {
        Ausgang {
            code,
            ausgabe: ausgabe.to_string(),
        }
    }

    #[test]
    fn status_null_ist_angemeldet_mit_konto() {
        assert_eq!(
            zustand_aus(&ausgang(Some(0), "test@beispiel.de\nzweite Zeile\n")),
            AnmeldeZustand::Angemeldet {
                konto: Some("test@beispiel.de".to_string())
            }
        );
        // Ohne Konto bleibt es bei „angemeldet".
        assert_eq!(
            zustand_aus(&ausgang(Some(0), "  \n")),
            AnmeldeZustand::Angemeldet { konto: None }
        );
        assert_eq!(
            zustand_aus(&ausgang(Some(0), "")),
            AnmeldeZustand::Angemeldet { konto: None }
        );
    }

    #[test]
    fn status_eins_ist_abgemeldet_alles_andere_fehler() {
        assert_eq!(
            zustand_aus(&ausgang(Some(1), "egal")),
            AnmeldeZustand::Abgemeldet,
            "Exit 1 heißt abgemeldet, nicht Fehler"
        );
        // Exit 3: angemeldet, läuft aber bald ab (Aufgabe 35, §5).
        assert_eq!(
            zustand_aus(&ausgang(Some(3), "test@beispiel.de\n")),
            AnmeldeZustand::LaeuftAb {
                konto: Some("test@beispiel.de".to_string())
            }
        );
        assert_eq!(
            zustand_aus(&ausgang(Some(7), "")),
            AnmeldeZustand::Fehler {
                text: "Der Status endet mit Code 7".to_string()
            }
        );
        assert!(matches!(
            zustand_aus(&ausgang(None, "")),
            AnmeldeZustand::Fehler { .. }
        ));
    }

    /// Die Prüfung der Anmeldung hat eine Frist von zehn Sekunden – kurz genug,
    /// dass niemand auf ein Drehsymbol wartet (Aufgabe 35, §5).
    #[test]
    fn die_pruefung_hat_zehn_sekunden() {
        assert_eq!(STATUS_FRIST, Duration::from_secs(10));
        assert_eq!(Anmeldebefehl::Status.frist(), STATUS_FRIST);
    }

    fn feld(name: &str, quelle: quiver_domain::Quelle) -> quiver_domain::Feld {
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

    fn eintrag(laufzeit: Laufzeit, felder: Vec<quiver_domain::Feld>) -> SollEintrag {
        SollEintrag {
            eintrag: quiver_domain::Eintrag {
                id: EintragId::neu("postfach"),
                version: "1.0.0".into(),
                titel: "Postfach".into(),
                beschreibung: String::new(),
                mehrfach: None,
                herkunft: quiver_domain::Herkunft::Eigen,
                laufzeit,
                felder,
                aktiv: true,
                anmeldung: Some(Anmeldung {
                    anmelden: vec!["anmelden".into(), "${KONTO}".into()],
                    status: vec!["status".into()],
                    abmelden: vec!["abmelden".into(), "${KONTO}".into()],
                }),
                einrichtung: None,
            },
            werte: Werte::from([("ABLAGE".to_string(), "/tmp/stand".to_string())]),
        }
    }

    #[test]
    fn befehl_hat_programm_umgebung_und_ersetzte_argumente() {
        let soll = eintrag(
            Laufzeit::Stdio {
                befehl: "${PFAD}".into(),
                argumente: vec!["mcp".into()],
                umgebung: Werte::from([(
                    "QUIVER_ATTRAPPE_DATEI".to_string(),
                    "${ABLAGE}".to_string(),
                )]),
            },
            vec![
                feld("ABLAGE", quiver_domain::Quelle::Server),
                feld("PFAD", quiver_domain::Quelle::Client),
            ],
        );
        let lokal = Werte::from([("PFAD".to_string(), "/opt/quiver/postfach".to_string())]);
        let geheim = Werte::new();

        let befehl = befehl_bauen(&soll, Anmeldebefehl::Anmelden, &lokal, &geheim).expect("Befehl");
        assert_eq!(befehl.programm, "/opt/quiver/postfach");
        assert_eq!(
            befehl.argumente,
            vec!["anmelden".to_string(), "${KONTO}".to_string()],
            "unbekannte Platzhalter bleiben stehen"
        );
        assert_eq!(
            befehl
                .umgebung
                .get("QUIVER_ATTRAPPE_DATEI")
                .map(String::as_str),
            Some("/tmp/stand"),
            "dieselbe Umgebung wie die Laufzeit, mit denselben Ersetzungen"
        );
        assert_eq!(befehl.zeile(), "/opt/quiver/postfach anmelden ${KONTO}");

        // Unbekannte Platzhalter bleiben stehen – wie in der Laufzeit.
        let soll = eintrag(
            Laufzeit::Stdio {
                befehl: "postfach".into(),
                argumente: Vec::new(),
                umgebung: Werte::new(),
            },
            vec![feld("ABLAGE", quiver_domain::Quelle::Server)],
        );
        let befehl = befehl_bauen(&soll, Anmeldebefehl::Status, &Werte::new(), &Werte::new())
            .expect("Befehl");
        assert_eq!(befehl.argumente, vec!["status".to_string()]);
    }

    #[test]
    fn befehl_ersetzt_platzhalter_aus_dem_modell() {
        let soll = eintrag(
            Laufzeit::Stdio {
                befehl: "postfach".into(),
                argumente: Vec::new(),
                umgebung: Werte::new(),
            },
            vec![
                feld("ABLAGE", quiver_domain::Quelle::Server),
                feld("KONTO", quiver_domain::Quelle::Server),
            ],
        );
        let soll = SollEintrag {
            werte: Werte::from([
                ("ABLAGE".to_string(), "/tmp/stand".to_string()),
                ("KONTO".to_string(), "test@beispiel.de".to_string()),
            ]),
            ..soll
        };
        let befehl = befehl_bauen(&soll, Anmeldebefehl::Anmelden, &Werte::new(), &Werte::new())
            .expect("Befehl");
        assert_eq!(
            befehl.argumente,
            vec!["anmelden".to_string(), "test@beispiel.de".to_string()]
        );
    }

    #[test]
    fn fehlende_angaben_und_entfernte_eintraege_sind_fehler() {
        let soll = eintrag(
            Laufzeit::Stdio {
                befehl: "postfach".into(),
                argumente: Vec::new(),
                umgebung: Werte::new(),
            },
            vec![feld("PFAD", quiver_domain::Quelle::Client)],
        );
        let fehler = befehl_bauen(&soll, Anmeldebefehl::Status, &Werte::new(), &Werte::new())
            .expect_err("PFAD fehlt");
        assert!(fehler.to_string().contains("PFAD"), "{fehler}");

        let entfernt = eintrag(
            Laufzeit::Remote {
                url: "https://example.org/mcp".into(),
                kopfzeilen: Werte::new(),
            },
            Vec::new(),
        );
        let fehler = befehl_bauen(
            &entfernt,
            Anmeldebefehl::Status,
            &Werte::new(),
            &Werte::new(),
        )
        .expect_err("Remote kennt keine Anmeldung");
        assert!(fehler.to_string().contains("Stdio"), "{fehler}");

        let ohne = SollEintrag {
            eintrag: quiver_domain::Eintrag {
                anmeldung: None,
                ..entfernt.eintrag
            },
            werte: Werte::new(),
        };
        assert!(befehl_bauen(&ohne, Anmeldebefehl::Status, &Werte::new(), &Werte::new()).is_err());
    }

    #[test]
    fn einrichtungstoken_steht_nie_im_protokoll() {
        let zeile =
            "Einrichten: http://127.0.0.1:4711/e/9f8e7d6c5b4a39281706f5e4d3c2b1a0/ im Browser";
        assert_eq!(
            maskieren(zeile),
            "Einrichten: http://127.0.0.1:4711/e/<verborgen>/ im Browser"
        );
        // Auch ohne Schrägstrich am Ende und bei kurzen Token.
        assert_eq!(
            maskieren("http://127.0.0.1:4711/e/abcd"),
            "http://127.0.0.1:4711/e/<verborgen>"
        );
        // Was kein Hex ist, bleibt stehen: das ist kein Einmal-Token.
        assert_eq!(
            maskieren("http://127.0.0.1:4711/e/hilfe"),
            "http://127.0.0.1:4711/e/hilfe"
        );
        assert_eq!(maskieren("/e/12/uebersicht"), "/e/12/uebersicht");
    }

    #[test]
    fn schluesselwoerter_verbergen_den_rest_der_zeile() {
        assert_eq!(
            maskieren("device token: 0123456789abcdef"),
            "device token: <verborgen>"
        );
        assert_eq!(maskieren("password=geheim"), "password=<verborgen>");
        assert_eq!(maskieren("Passwort geheim"), "Passwort <verborgen>");
        assert_eq!(maskieren("Bearer abcdef"), "Bearer <verborgen>");
        assert_eq!(maskieren("secret: so-geheim"), "secret: <verborgen>");
        // Ohne Wert dahinter gibt es nichts zu verbergen.
        assert_eq!(maskieren("token"), "token");
        assert_eq!(maskieren("password: "), "password: ");
        // Harmlose Zeilen bleiben Zeile.
        assert_eq!(maskieren("Einrichtung fertig"), "Einrichtung fertig");
    }

    #[test]
    fn maskieren_aendert_den_befehl_nicht() {
        // Die Maske entsteht erst fürs Protokoll, nie für den Aufruf.
        let befehl = Befehl {
            programm: "/opt/quiver/outline".into(),
            argumente: vec!["einrichten".into()],
            umgebung: Werte::new(),
        };
        assert_eq!(maskieren(&befehl.zeile()), "/opt/quiver/outline einrichten");
        assert_eq!(befehl.zeile(), "/opt/quiver/outline einrichten");
    }
}
