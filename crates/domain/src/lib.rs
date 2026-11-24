// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Datenmodell und Regeln von mcp-quiver (SPEC.md §4–§6). Keine I/O.
//!
//! - [`Eintrag`]: ein MCP-Tool in der Bibliothek (wie es gestartet wird, welche
//!   Felder es braucht und wer sie füllt).
//! - [`Satz`]: eine benannte Sammlung von Einträgen mit vorbelegten Werten.
//! - [`Gruppe`]: bekommt Sätze; ein [`Geraet`] gehört zu Gruppen.
//! - [`soll_berechnen`]: was auf einem Gerät laufen soll.
//! - [`konfig_zusammensetzen`]: Server-Werte + Gerätewerte → startbare [`Laufzeit`].
//! - [`abgleichen`]: Soll gegen Ist → [`Aktion`]en für den Client.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Name eines Felds → Wert.
pub type Werte = BTreeMap<String, String>;

macro_rules! kennung {
    ($name:ident, $doku:literal) => {
        #[doc = $doku]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn neu(s: impl Into<String>) -> Self {
                Self(s.into())
            }
            pub fn als_str(&self) -> &str {
                &self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

kennung!(
    EintragId,
    "Kennung eines Eintrags, ein Kürzel wie `microsoft-mail`. Wird Präfix der Tool-Namen."
);
kennung!(SatzId, "Kennung eines Satzes.");
kennung!(GruppeId, "Kennung einer Gruppe.");
kennung!(GeraetId, "Kennung eines Geräts (vom Server vergeben).");

/// Woher ein Eintrag kommt (SPEC.md §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Herkunft {
    /// Von uns gebaut.
    Eigen,
    /// Übernommen (Registry, npm, PyPI, Remote). Muss freigegeben sein.
    Fremd,
}

/// Wie ein Eintrag läuft. Werte dürfen Platzhalter `${feld}` enthalten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", rename_all = "kebab-case")]
pub enum Laufzeit {
    /// Ein Prozess, MCP über stdin/stdout (eigene Binaries, `npx`, `uvx`, …).
    Stdio {
        befehl: String,
        #[serde(default)]
        argumente: Vec<String>,
        #[serde(default)]
        umgebung: Werte,
    },
    /// Ein entfernter MCP-Server über Streamable HTTP.
    Remote {
        url: String,
        #[serde(default)]
        kopfzeilen: Werte,
    },
}

/// Wer ein Feld füllt (SPEC.md §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Quelle {
    /// Admin, wird mitgepusht.
    Server,
    /// User am Gerät, steht in der Client-Konfiguration.
    Client,
    /// User am Gerät, liegt in der Tresor-Datei und verlässt das Gerät nie.
    Geheim,
}

/// Wie ein Feld im Dialog dargestellt wird (Aufgabe 16).
///
/// Die JSON-Form ist extern getaggt: `"text"`, `"zahl"` … und für die
/// Auswahl `{"auswahl":{"optionen":[{"wert":"a","anzeige":"A"}]}}`.
/// Alte Daten ohne `art` gelten als [`FeldArt::Text`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FeldArt {
    /// Freie Texteingabe (Standard).
    #[default]
    Text,
    /// Wie Text, aber als Passwortfeld eingegeben.
    Passwort,
    /// Nur Ziffern; die Prüfung geschieht beim Speichern.
    Zahl,
    /// Ein Häkchen: Wert `an` oder `aus`.
    Schalter,
    /// Eine Adresse, geprüft auf `http(s)://`.
    Url,
    /// Eine Auswahlliste aus festen Optionen.
    Auswahl { optionen: Vec<FeldOption> },
}

/// Eine Option einer [`FeldArt::Auswahl`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeldOption {
    /// Der gespeicherte Wert.
    pub wert: String,
    /// Was der User in der Liste sieht.
    pub anzeige: String,
}

/// Ein Konfigurationsfeld eines Eintrags.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feld {
    pub name: String,
    #[serde(default)]
    pub beschreibung: String,
    pub quelle: Quelle,
    #[serde(default = "wahr")]
    pub pflicht: bool,
    #[serde(default)]
    pub standard: Option<String>,
    /// Optionaler Hilfe-Link neben dem Feld, z. B. „Token in Outline erstellen“.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<FeldLink>,
    /// Wie das Feld dargestellt wird; fehlt der Wert in alten Daten, gilt Text.
    #[serde(default)]
    pub art: FeldArt,
    /// Anzeige statt des Feldnamens, z. B. „SAP-Benutzer“.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub titel: Option<String>,
}

impl Default for Feld {
    fn default() -> Self {
        Self {
            name: String::new(),
            beschreibung: String::new(),
            quelle: Quelle::Server,
            pflicht: true,
            standard: None,
            link: None,
            art: FeldArt::Text,
            titel: None,
        }
    }
}

/// Ein Link, den das Fenster neben einem Feld als Knopf zeigt und im Browser öffnet.
/// `url` darf Platzhalter `${feld}` anderer Felder enthalten; der Knopf erscheint
/// erst, wenn alle verwendeten Felder einen Wert haben ([`link_aufloesen`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeldLink {
    pub text: String,
    pub url: String,
}

/// Setzt die Werte in die Link-Adresse ein. `None`, solange ein verwendetes Feld
/// fehlt oder das Ergebnis nicht mit `http://` oder `https://` beginnt.
/// Ein Schrägstrich am Ende eines Werts wird vor dem Einsetzen entfernt,
/// damit `${url}/settings` auch bei `https://wiki.example/` passt.
pub fn link_aufloesen(link: &FeldLink, werte: &Werte) -> Option<String> {
    let bereinigt: Werte = werte
        .iter()
        .map(|(k, v)| (k.clone(), v.trim().trim_end_matches('/').to_string()))
        .filter(|(_, v)| !v.is_empty())
        .collect();
    let url = ersetzen(&link.url, &bereinigt);
    let fertig = !url.contains("${");
    let sicher = url.starts_with("https://") || url.starts_with("http://");
    (fertig && sicher).then_some(url)
}

fn wahr() -> bool {
    true
}

/// Ein MCP-Tool in der Bibliothek.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Eintrag {
    pub id: EintragId,
    pub version: String,
    pub titel: String,
    #[serde(default)]
    pub beschreibung: String,
    pub herkunft: Herkunft,
    pub laufzeit: Laufzeit,
    #[serde(default)]
    pub felder: Vec<Feld>,
    /// Abgeschaltet: bleibt in Bibliothek und Sätzen, wird aber auf keinem Gerät
    /// ausgeführt. Fehlt das Feld (ältere Daten), gilt der Eintrag als aktiv.
    #[serde(default = "wahr")]
    pub aktiv: bool,
    /// Braucht das Tool eine Anmeldung des Users (z. B. Microsoft im Browser)?
    /// Dann bietet das Client-Fenster „Anmelden“ an; ein Agent löst sie nie aus.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anmeldung: Option<Anmeldung>,
    /// Hat das Tool eine eigene Einrichtungsseite (Profile, Zugänge)? Dann bietet
    /// das Client-Fenster „Einrichten“ an. Zugangsdaten gehen so nie durch einen Agenten.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub einrichtung: Option<Einrichtung>,
    /// Mehrere gleichartige Zugänge (z. B. SSH-Verbindungen), die der User im
    /// Client-Fenster nach und nach anlegt. Siehe [`Mehrfach`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mehrfach: Option<Mehrfach>,
}

/// Beschreibung einer Liste von Zugängen, die der User selbst pflegt.
///
/// Der Client speichert je Zugang einen Namen und Werte zu `felder`
/// (`Quelle::Geheim` in der Tresor-Datei, sonst in der Client-Konfiguration) und
/// übergibt beim Start **alle** Zugänge als JSON in der Umgebungsvariable
/// [`MEHRFACH_UMGEBUNG`]:
/// `[{"name": "web-01", "werte": {"host": "…", "passwort": "…"}}, …]`.
/// `Quelle::Server` ist hier nicht erlaubt (die Liste gehört dem User).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Mehrfach {
    /// Überschrift der Liste, z. B. „Verbindungen“.
    pub titel: String,
    /// Bezeichnung eines Elements, z. B. „Verbindung“.
    pub einzahl: String,
    pub felder: Vec<Feld>,
}

/// Name der Umgebungsvariable, in der der Client die Zugänge übergibt.
pub const MEHRFACH_UMGEBUNG: &str = "QUIVER_ZUGAENGE";

/// Ein Zugang aus einer [`Mehrfach`]-Liste, wie er dem Tool übergeben wird.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zugang {
    pub name: String,
    pub werte: Werte,
}

/// Namen von Zugängen: `a-z0-9-_`, 1–40 Zeichen, eindeutig.
pub fn zugangsname_gueltig(name: &str) -> bool {
    (1..=40).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Zwei Zugangsnamen meinen denselben Zugang, wenn sie sich nur in Groß- und
/// Kleinschreibung unterscheiden (`BL4_900` = `bl4_900`) – sonst gäbe es zwei
/// Zugänge, die für einen Agenten gleich aussehen.
pub fn zugangsname_gleich(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// Wie der Client die Einrichtung eines `Stdio`-Tools startet (SPEC.md §7).
///
/// `einrichten` ist eine Argumentliste für **dasselbe** Programm mit **derselben**
/// Umgebung wie die Laufzeit. Der Befehl öffnet selbst eine Seite im Browser und
/// endet, wenn der User sie abschließt (Exit 0) oder abbricht (Exit 1).
/// Beispiel quiver-outline: `["einrichten"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Einrichtung {
    pub einrichten: Vec<String>,
}

/// Wie der Client die Anmeldung eines `Stdio`-Tools steuert (SPEC.md §7).
///
/// Jeder Befehl ist eine Argumentliste für **dasselbe** Programm mit **derselben**
/// Umgebung wie die Laufzeit des Eintrags (Platzhalter `${feld}` gelten auch hier).
/// Beispiel quiver-m365: `["anmelden"]`, `["status"]`, `["abmelden"]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anmeldung {
    /// Öffnet den Browser, wartet auf die Anmeldung, Exit 0 = angemeldet.
    pub anmelden: Vec<String>,
    /// Exit 0 = angemeldet, Exit 1 = nicht angemeldet, sonst Fehler.
    /// Die erste Zeile von stdout ist der Anzeigename des Kontos (optional).
    pub status: Vec<String>,
    /// Exit 0 = abgemeldet.
    pub abmelden: Vec<String>,
}

/// Ein Eintrag in einem Satz, mit den Werten, die der Admin vorbelegt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SatzEintrag {
    pub eintrag: EintragId,
    #[serde(default)]
    pub werte: Werte,
}

/// Eine benannte Sammlung von Einträgen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Satz {
    pub id: SatzId,
    pub titel: String,
    /// Bei widersprüchlichen Werten gewinnt die höhere Priorität (SPEC.md §5).
    #[serde(default)]
    pub prioritaet: i32,
    #[serde(default)]
    pub eintraege: Vec<SatzEintrag>,
}

/// Eine Gruppe bekommt Sätze. Im MVP lokal auf dem Server, später aus Entra ID.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gruppe {
    pub id: GruppeId,
    pub titel: String,
    #[serde(default)]
    pub saetze: Vec<SatzId>,
    /// Entra-Objekt-Ids, die auf diese Gruppe abgebildet werden (Aufgabe 20).
    /// Namen sind änderbar und deshalb keine Grundlage für Rechte.
    #[serde(default)]
    pub entra: Vec<String>,
}

/// Ein eingebundenes Gerät.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Geraet {
    pub id: GeraetId,
    pub name: String,
    #[serde(default)]
    pub gruppen: Vec<GruppeId>,
    #[serde(default)]
    pub gesperrt: bool,
}

/// Was auf einem Gerät laufen soll: Eintrag + vom Server gefüllte Werte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SollEintrag {
    pub eintrag: Eintrag,
    /// Nur Felder mit `Quelle::Server`.
    pub werte: Werte,
}

impl SollEintrag {
    /// Fingerabdruck über Eintrag und Werte: ändert er sich, startet der Client neu.
    pub fn fingerabdruck(&self) -> String {
        let json = serde_json::to_vec(self).expect("SollEintrag ist serialisierbar");
        hex::encode(Sha256::digest(json))
    }
}

/// Der Soll-Zustand eines Geräts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Soll {
    /// Fingerabdruck über alle Einträge; gleich = nichts zu tun.
    pub revision: String,
    pub eintraege: Vec<SollEintrag>,
}

impl Soll {
    pub fn neu(mut eintraege: Vec<SollEintrag>) -> Self {
        eintraege.sort_by(|a, b| a.eintrag.id.cmp(&b.eintrag.id));
        let mut h = Sha256::new();
        for e in &eintraege {
            h.update(e.fingerabdruck());
        }
        Self {
            revision: hex::encode(h.finalize()),
            eintraege,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SollFehler {
    #[error("Satz {satz} verweist auf unbekannten Eintrag {eintrag}")]
    UnbekannterEintrag { satz: SatzId, eintrag: EintragId },
    #[error("Feld {feld} von {eintrag}: Sätze {a} und {b} haben gleiche Priorität und verschiedene Werte")]
    Widerspruch {
        eintrag: EintragId,
        feld: String,
        a: SatzId,
        b: SatzId,
    },
}

/// Berechnet den Soll-Zustand eines Geräts (SPEC.md §5).
///
/// Vereinigung aller Sätze aller Gruppen des Geräts. Gesperrte Geräte
/// bekommen nichts. Nur `Quelle::Server`-Felder werden übernommen; bei
/// widersprüchlichen Werten gewinnt die höhere Priorität, Gleichstand ist ein
/// Fehler.
pub fn soll_berechnen(
    geraet: &Geraet,
    gruppen: &[Gruppe],
    saetze: &[Satz],
    eintraege: &[Eintrag],
) -> Result<Soll, SollFehler> {
    if geraet.gesperrt {
        return Ok(Soll::neu(Vec::new()));
    }
    let satz_ids: BTreeSet<&SatzId> = gruppen
        .iter()
        .filter(|g| geraet.gruppen.contains(&g.id))
        .flat_map(|g| g.saetze.iter())
        .collect();

    // Eintrag → Feld → (Priorität, Satz, Wert)
    let mut gewaehlt: BTreeMap<&EintragId, BTreeMap<String, (i32, &SatzId, String)>> =
        BTreeMap::new();
    for satz in saetze.iter().filter(|s| satz_ids.contains(&s.id)) {
        for se in &satz.eintraege {
            let eintrag = eintraege
                .iter()
                .find(|e| e.id == se.eintrag)
                .ok_or_else(|| SollFehler::UnbekannterEintrag {
                    satz: satz.id.clone(),
                    eintrag: se.eintrag.clone(),
                })?;
            if !eintrag.aktiv {
                continue;
            }
            let felder = gewaehlt.entry(&eintrag.id).or_default();
            for (name, wert) in &se.werte {
                let ist_server = eintrag
                    .felder
                    .iter()
                    .any(|f| &f.name == name && f.quelle == Quelle::Server);
                if !ist_server {
                    continue;
                }
                match felder.get(name) {
                    Some((p, _, _)) if *p > satz.prioritaet => {}
                    Some((p, anderer, w)) if *p == satz.prioritaet && w != wert => {
                        return Err(SollFehler::Widerspruch {
                            eintrag: eintrag.id.clone(),
                            feld: name.clone(),
                            a: (*anderer).clone(),
                            b: satz.id.clone(),
                        });
                    }
                    _ => {
                        felder.insert(name.clone(), (satz.prioritaet, &satz.id, wert.clone()));
                    }
                }
            }
        }
    }

    let liste = gewaehlt
        .into_iter()
        .map(|(id, felder)| {
            let eintrag = eintraege
                .iter()
                .find(|e| &e.id == id)
                .expect("oben gefunden");
            SollEintrag {
                eintrag: eintrag.clone(),
                werte: felder.into_iter().map(|(k, (_, _, w))| (k, w)).collect(),
            }
        })
        .collect();
    Ok(Soll::neu(liste))
}

/// Ein Feld, das am Gerät noch fehlt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FehlendesFeld {
    pub name: String,
    pub beschreibung: String,
    pub quelle: Quelle,
}

/// Setzt die Konfiguration in Schichten zusammen (SPEC.md §6) und ersetzt
/// `${feld}` in der Laufzeit:
/// Standard des Felds → Werte des Servers → Werte am Gerät (`lokal`, `geheim`).
///
/// Fehlen Pflichtfelder, kommt die Liste zurück, damit das Fenster genau diese
/// abfragt.
pub fn konfig_zusammensetzen(
    soll: &SollEintrag,
    lokal: &Werte,
    geheim: &Werte,
) -> Result<Laufzeit, Vec<FehlendesFeld>> {
    let mut werte = Werte::new();
    let mut fehlend = Vec::new();
    for feld in &soll.eintrag.felder {
        let wert = match feld.quelle {
            Quelle::Server => soll.werte.get(&feld.name),
            Quelle::Client => lokal.get(&feld.name),
            Quelle::Geheim => geheim.get(&feld.name),
        }
        .cloned()
        .filter(|w| !w.is_empty())
        .or_else(|| feld.standard.clone());
        match wert {
            Some(w) => {
                werte.insert(feld.name.clone(), w);
            }
            None if feld.pflicht => fehlend.push(FehlendesFeld {
                name: feld.name.clone(),
                beschreibung: feld.beschreibung.clone(),
                quelle: feld.quelle,
            }),
            None => {
                werte.insert(feld.name.clone(), String::new());
            }
        }
    }
    if !fehlend.is_empty() {
        return Err(fehlend);
    }
    let r = |s: &String| ersetzen(s, &werte);
    let rm = |m: &Werte| m.iter().map(|(k, v)| (k.clone(), r(v))).collect();
    Ok(match &soll.eintrag.laufzeit {
        Laufzeit::Stdio {
            befehl,
            argumente,
            umgebung,
        } => Laufzeit::Stdio {
            befehl: r(befehl),
            argumente: argumente.iter().map(r).collect(),
            umgebung: rm(umgebung),
        },
        Laufzeit::Remote { url, kopfzeilen } => Laufzeit::Remote {
            url: r(url),
            kopfzeilen: rm(kopfzeilen),
        },
    })
}

/// Ersetzt `${name}` durch den Wert; unbekannte Platzhalter bleiben stehen.
pub fn ersetzen(text: &str, werte: &Werte) -> String {
    let mut aus = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        aus.push_str(&rest[..start]);
        let nach = &rest[start + 2..];
        match nach.find('}') {
            Some(ende) => {
                let name = &nach[..ende];
                match werte.get(name) {
                    Some(w) => aus.push_str(w),
                    None => aus.push_str(&rest[start..start + 2 + ende + 1]),
                }
                rest = &nach[ende + 1..];
            }
            None => {
                aus.push_str(&rest[start..]);
                rest = "";
            }
        }
    }
    aus.push_str(rest);
    aus
}

/// Was der Client tun muss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "art", content = "eintrag", rename_all = "kebab-case")]
pub enum Aktion {
    Starten(EintragId),
    Neustarten(EintragId),
    Stoppen(EintragId),
}

/// Soll gegen Ist (Eintrag → Fingerabdruck dessen, was gerade läuft).
/// Ergebnis ist sortiert: erst Stoppen, dann Neustarten, dann Starten.
pub fn abgleichen(soll: &Soll, ist: &BTreeMap<EintragId, String>) -> Vec<Aktion> {
    let mut stoppen = Vec::new();
    let mut neu = Vec::new();
    let mut starten = Vec::new();
    for id in ist.keys() {
        if !soll.eintraege.iter().any(|e| &e.eintrag.id == id) {
            stoppen.push(Aktion::Stoppen(id.clone()));
        }
    }
    for e in &soll.eintraege {
        match ist.get(&e.eintrag.id) {
            None => starten.push(Aktion::Starten(e.eintrag.id.clone())),
            Some(fa) if *fa != e.fingerabdruck() => {
                neu.push(Aktion::Neustarten(e.eintrag.id.clone()))
            }
            Some(_) => {}
        }
    }
    stoppen.into_iter().chain(neu).chain(starten).collect()
}

/// Tool-Name, wie der Agent ihn sieht: `<eintrag>__<tool>`.
pub fn tool_name(eintrag: &EintragId, tool: &str) -> String {
    format!("{}__{}", eintrag.0, tool)
}

/// Umkehrung von [`tool_name`].
pub fn tool_name_zerlegen(name: &str) -> Option<(EintragId, &str)> {
    let (e, t) = name.split_once("__")?;
    (!e.is_empty() && !t.is_empty()).then(|| (EintragId::neu(e), t))
}

#[cfg(test)]
mod tests;
