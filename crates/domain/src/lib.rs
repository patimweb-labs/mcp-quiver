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
    /// Ein Ordner auf dem Gerät, gewählt über den Datei-Dialog.
    Ordner,
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

/// Betriebssystem und Architektur eines Geräts (Aufgabe 37).
///
/// Der Client meldet beides; der Server sucht danach das Programm aus. Die
/// Namen sind die der Release-Archive aus quiver-tools (Aufgabe 38):
/// `windows`, `macos`, `linux` und `x64`, `arm64`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct System {
    pub system: String,
    pub arch: String,
}

impl System {
    /// Die Namen, die der Server kennt. Alles andere wird abgelehnt – kein
    /// Pfad, kein Name aus dem Netz wird je zu einem Ordner.
    pub const SYSTEME: [&'static str; 3] = ["windows", "macos", "linux"];
    /// Die Architekturen, die der Server kennt.
    pub const ARCHITEKTUREN: [&'static str; 2] = ["x64", "arm64"];

    /// Das System dieses Prozesses.
    pub fn dieses() -> Self {
        Self {
            system: match std::env::consts::OS {
                "linux" => "linux",
                "macos" => "macos",
                "windows" => "windows",
                anderes => anderes,
            }
            .to_string(),
            arch: match std::env::consts::ARCH {
                "x86_64" | "x86" => "x64",
                "aarch64" | "arm64" => "arm64",
                anderes => anderes,
            }
            .to_string(),
        }
    }

    /// Aus zwei Angaben (etwa aus einer Abfrage) ein geprüftes System bauen.
    ///
    /// `None` heißt: eine der beiden Angaben ist unbekannt. Der Server nimmt
    /// dann keine Programme an – lieber nichts als das falsche.
    pub fn aus_teilen(system: &str, arch: &str) -> Option<Self> {
        let system = system.trim().to_ascii_lowercase();
        let arch = arch.trim().to_ascii_lowercase();
        if Self::SYSTEME.contains(&system.as_str()) && Self::ARCHITEKTUREN.contains(&arch.as_str())
        {
            return Some(Self { system, arch });
        }
        None
    }
}

/// Ein Programm, das der Server für die Geräte bereithält (Aufgabe 37).
///
/// Der Client lädt genau diese Datei und prüft sie gegen `sha256` und
/// `groesse`. Was hier steht, ist die einzige Wahrheit über das Programm –
/// kein Pfad vom Server wird je als Pfad benutzt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Programm {
    /// Der Programmname, z. B. `quiver-devops`.
    pub name: String,
    /// Die Fassung aus dem Release; `unbekannt` bei einem Programm, das der
    /// Betreiber selbst abgelegt hat.
    pub version: String,
    /// Größe in Bytes.
    pub groesse: u64,
    /// SHA-256 über die Datei, klein und hexadezimal.
    pub sha256: String,
    /// Betriebssystem (`linux`, `macos`, `windows`).
    pub system: String,
    /// Architektur (`x64`, `arm64`).
    pub arch: String,
}

impl Programm {
    /// Passt dieses Programm zu diesem System?
    pub fn passt(&self, system: &System) -> bool {
        self.system == system.system && self.arch == system.arch
    }
}

/// Ist dieser Programmname einer, den der Server verteilt (Aufgabe 37)?
///
/// Verteilt werden nur die eigenen Programme (`quiver-*`), und nur als
/// reiner Name: `npx`, `uvx`, `docker` und Pfade zu eigenen Programmen
/// laufen unverändert auf dem Gerät. Leerzeichen am Rand sind **kein**
/// Programmname: der Name wird Teil eines Ordners, deshalb gilt er nur, wie er
/// dasteht.
pub fn ist_verteilt(name: &str) -> bool {
    name.len() > "quiver-".len()
        && !name.contains('/')
        && !name.contains('\\')
        && name.starts_with("quiver-")
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Der Name eines verteilten Programms aus einem Befehl (Aufgabe 37).
///
/// `Some(name)` heißt: der Eintrag erwartet ein Programm, das der Server
/// verteilt. Das ist ein bloßer Name (`quiver-devops`) oder der Dateiname
/// eines Pfades (`/pfad/auf/anderem/rechner/quiver-devops`,
/// `C:\…\quiver-devops.exe`) – ein Altbestand mit festem Pfad heilt sich damit.
/// `None` heißt: ein fremdes Programm (`npx`, `uvx`, `docker`, ein Pfad zu
/// einem eigenen Programm), das auf dem Gerät läuft, wie es dasteht.
///
/// Server und Client benutzen **diese** Funktion: was hier `Some` ist, wird
/// verteilt und nach dem Namen nachgeschlagen – nie nach dem Pfad.
pub fn verteiltes_programm(befehl: &str) -> Option<String> {
    let befehl = befehl.trim();
    if befehl.is_empty() {
        return None;
    }
    let dateiname = befehl.rsplit(['/', '\\']).next().unwrap_or(befehl).trim();
    // `.exe` fällt weg (Windows) – der Vergleich ist unabhängig von der
    // Schreibweise, die Länge bleibt dabei gleich.
    let ohne_exe = if dateiname.to_ascii_lowercase().ends_with(".exe") {
        &dateiname[..dateiname.len() - 4]
    } else {
        dateiname
    };
    ist_verteilt(ohne_exe).then(|| ohne_exe.to_string())
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
    /// Fingerabdruck über alle Einträge und Programme; gleich = nichts zu tun.
    pub revision: String,
    pub eintraege: Vec<SollEintrag>,
    /// Die Programme, die der Server für dieses Gerät und **sein** System
    /// bereithält (Aufgabe 37). Welches Programm zu welchem Eintrag gehört,
    /// sagt der Programmname im Befehl des Eintrags. Ein Eintrag, dessen
    /// Programm hier fehlt, gibt es für dieses System nicht.
    #[serde(default)]
    pub programme: Vec<Programm>,
}

impl Soll {
    pub fn neu(eintraege: Vec<SollEintrag>) -> Self {
        Self::neu_mit(eintraege, Vec::new())
    }

    /// Ein Soll mit den Programmen, die der Server für dieses System hat.
    ///
    /// Die Programme gehören in die Revision: ändert sich ein Programm (neue
    /// Fassung, andere Prüfsumme), ist das eine neue Revision – und damit ein
    /// Anlass für jeden Client, nachzusehen (Aufgabe 37).
    pub fn neu_mit(mut eintraege: Vec<SollEintrag>, mut programme: Vec<Programm>) -> Self {
        eintraege.sort_by(|a, b| a.eintrag.id.cmp(&b.eintrag.id));
        programme.sort_by(|a, b| a.name.cmp(&b.name));
        let revision = revision_aus(&eintraege, &programme);
        Self {
            revision,
            eintraege,
            programme,
        }
    }

    /// Dieselben Einträge mit einer anderen Programmliste (der Server rechnet
    /// das Soll ohne System, weil ein Gerät keins nennt).
    pub fn mit_programmen(mut self, mut programme: Vec<Programm>) -> Self {
        programme.sort_by(|a, b| a.name.cmp(&b.name));
        self.revision = revision_aus(&self.eintraege, &programme);
        self.programme = programme;
        self
    }

    /// Fingerabdruck eines Eintrags **samt seinem Programm** (Aufgabe 37): der
    /// Eintrag startet auch dann neu, wenn sich nur das Programm ändert, neu
    /// hinzukommt oder wegfällt. Ohne verteiltes Programm ist es der
    /// Fingerabdruck des Eintrags.
    pub fn fingerabdruck(&self, eintrag: &SollEintrag) -> String {
        let eigener = eintrag.fingerabdruck();
        let programm = match &eintrag.eintrag.laufzeit {
            Laufzeit::Stdio { befehl, .. } => {
                verteiltes_programm(befehl).and_then(|name| self.programm(&name))
            }
            _ => None,
        };
        match programm {
            Some(programm) => format!("{eigener}:{}", programm.sha256),
            None => eigener,
        }
    }

    /// Das Programm zu einem Programmnamen, falls der Server es hat.
    pub fn programm(&self, name: &str) -> Option<&Programm> {
        self.programme.iter().find(|programm| programm.name == name)
    }
}

/// Der Fingerabdruck eines Solls: Einträge und Programme.
fn revision_aus(eintraege: &[SollEintrag], programme: &[Programm]) -> String {
    let mut h = Sha256::new();
    for e in eintraege {
        h.update(e.fingerabdruck());
    }
    for p in programme {
        h.update(p.name.as_bytes());
        h.update(p.version.as_bytes());
        h.update(p.groesse.to_le_bytes());
        h.update(p.sha256.as_bytes());
        h.update(p.system.as_bytes());
        h.update(p.arch.as_bytes());
    }
    hex::encode(h.finalize())
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
            Some(fa) if *fa != soll.fingerabdruck(e) => {
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

/// Wie lang eine Kennung höchstens sein darf.
pub const KENNUNG_MAX: usize = 40;

/// Das Präfix der eigenen Werkzeuge des Clients – kein Eintrag darf so heißen.
pub const RESERVIERTE_KENNUNG: &str = "quiver";

/// Warum eine Kennung nicht taugt – `None` heißt: sie taugt.
///
/// Eine Kennung wird Teil jedes Werkzeugnamens (`<kennung>__<werkzeug>`).
/// Deshalb nur Kleinbuchstaben, Ziffern und `-`, höchstens [`KENNUNG_MAX`]
/// Zeichen, kein `__` (das trennt Kennung und Werkzeug) und nicht
/// [`RESERVIERTE_KENNUNG`] (so heißen die Werkzeuge des Clients selbst).
pub fn kennung_fehler(id: &str) -> Option<&'static str> {
    if id.is_empty() {
        return Some("Die Kennung ist leer");
    }
    if id.chars().count() > KENNUNG_MAX {
        return Some("Die Kennung ist länger als 40 Zeichen");
    }
    if id.contains("__") {
        return Some("Die Kennung darf kein __ enthalten");
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Some("Die Kennung darf nur a–z, 0–9 und - enthalten");
    }
    if id == RESERVIERTE_KENNUNG {
        return Some("Die Kennung quiver ist für die Werkzeuge des Clients selbst reserviert");
    }
    None
}

/// Ist die Kennung eines Eintrags brauchbar ([`kennung_fehler`])?
pub fn kennung_gueltig(id: &str) -> bool {
    kennung_fehler(id).is_none()
}

#[cfg(test)]
mod tests;
