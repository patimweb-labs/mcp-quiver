// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Das Protokoll der Werkzeug-Aufrufe und der Werkzeug-Prozesse (Aufgabe 17).
//!
//! Der Endpunkt schreibt **jeden** `tools/call` mit: Zeit, Agent, Eintrag,
//! Werkzeug, Dauer, Ergebnis, gekürzte Argumente und eine Vorschau der
//! Antwort. Was ein Eintrag auf stderr schreibt, landet je Eintrag daneben.
//!
//! Zwei Dinge sind Pflicht und stehen deshalb hier an einer Stelle:
//!
//! - **Maskieren.** Schlüssel, deren Name `pass`, `token`, `secret`, `key`,
//!   `pat`, `auth` oder `cookie` enthält, bekommen `<verborgen>` als Wert.
//!   Und **jeder** Wert aus dem Tresor wird ersetzt – auch als Teilstring, in
//!   Argumenten, in der Vorschau, im Fehlertext und in den Prozesszeilen.
//! - **Kürzen.** Je Wert 200 Zeichen, Argumente zusammen 4 KB, Vorschau 500,
//!   Fehlertext 2000. Was nicht gekürzt wird, wächst unbegrenzt.
//!
//! Der Speicher ist ein Ringpuffer (2000 Aufrufe, 5000 Zeilen). Auf der Platte
//! liegen `<profil>/protokoll/aufrufe-JJJJ-MM-TT.jsonl` und
//! `prozesse-JJJJ-MM-TT.jsonl` mit Rechten 0600; was älter als 7 Tage ist,
//! fliegt weg, und insgesamt bleiben höchstens 50 MB.

use std::collections::VecDeque;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use quiver_domain::EintragId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// So viele Aufrufe bleiben im Arbeitsspeicher.
pub const AUFRUFE_MAX: usize = 2000;
/// So viele Prozesszeilen bleiben im Arbeitsspeicher.
pub const ZEILEN_MAX: usize = 5000;
/// Was in einem Aufruf statt eines Geheimnisses steht.
pub const VERBORGEN: &str = "<verborgen>";
/// So viele Zeichen eines einzelnen Werts bleiben stehen.
pub const WERT_ZEICHEN: usize = 200;
/// So groß dürfen die Argumente zusammen werden.
pub const ARGUMENTE_BYTES: usize = 4 * 1024;
/// So lang ist die Vorschau einer Antwort.
pub const VORSCHAU_ZEICHEN: usize = 500;
/// So lang darf ein Fehlertext sein.
pub const FEHLER_ZEICHEN: usize = 2000;
/// So alt dürfen Protokolldateien werden.
pub const TAGE: u64 = 7;
/// So viel Platz dürfen alle Protokolldateien zusammen brauchen.
pub const GESAMT_BYTES: u64 = 50 * 1024 * 1024;
/// Der Ordner im Profil.
pub const ORDNER: &str = "protokoll";

/// Wie ein Aufruf ausging. `abgelehnt` heißt: der Client hat ihn nicht
/// weitergereicht (unbekanntes Werkzeug, Eintrag läuft nicht).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    Fehler,
    Abgelehnt,
}

impl Status {
    pub fn als_str(self) -> &'static str {
        match self {
            Status::Ok => "ok",
            Status::Fehler => "fehler",
            Status::Abgelehnt => "abgelehnt",
        }
    }

    /// Nur Fehler und Ablehnungen – für den Schalter „nur Fehler“.
    pub fn ist_fehler(self) -> bool {
        !matches!(self, Status::Ok)
    }
}

/// Ein aufgezeichneter Werkzeug-Aufruf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Aufruf {
    /// RFC 3339 in UTC (`2026-01-02T03:04:05Z`) – als Text vergleichbar.
    pub zeit: String,
    /// Der Agent aus seinem Token; leer, wenn keiner bekannt ist.
    #[serde(default)]
    pub agent: String,
    pub eintrag: EintragId,
    /// Der Werkzeugname, wie der Agent ihn aufgerufen hat (`echo__echo`).
    pub tool: String,
    pub dauer_ms: u64,
    pub status: Status,
    /// Warum es schiefging – maskiert und gekürzt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fehler: Option<String>,
    /// Die Argumente als gekürztes, maskiertes JSON.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub argumente: String,
    /// Größe der Antwort in Zeichen.
    #[serde(default)]
    pub antwort_groesse: usize,
    /// Die ersten 500 Zeichen der Antwort – maskiert.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub vorschau: String,
}

/// Eine Zeile, die ein Eintrag auf stderr geschrieben hat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Zeile {
    pub zeit: String,
    pub eintrag: EintragId,
    pub text: String,
}

/// Ein Zeitraum für die Filterleiste.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Zeitraum {
    Stunde,
    Heute,
    Tage,
}

impl Zeitraum {
    pub const ALLE: [Zeitraum; 3] = [Zeitraum::Stunde, Zeitraum::Heute, Zeitraum::Tage];
}

/// Was die Abfrage der Aufrufe einschränkt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Filter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eintrag: Option<EintragId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default)]
    pub nur_fehler: bool,
    /// Anfang als RFC 3339 (`2026-01-02T03:04:05Z`); älter zählt nicht.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seit: Option<String>,
    /// Höchstens so viele, die neuesten zuerst.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anzahl: Option<usize>,
}

impl Filter {
    /// Passt dieser Aufruf?
    pub fn passt(&self, aufruf: &Aufruf) -> bool {
        if let Some(eintrag) = &self.eintrag {
            if &aufruf.eintrag != eintrag {
                return false;
            }
        }
        if let Some(tool) = &self.tool {
            if &aufruf.tool != tool {
                return false;
            }
        }
        if let Some(agent) = &self.agent {
            if &aufruf.agent != agent {
                return false;
            }
        }
        if self.nur_fehler && !aufruf.status.ist_fehler() {
            return false;
        }
        if let Some(seit) = &self.seit {
            // Beide Zeiten sind UTC im festen Format: Textvergleich genügt.
            if aufruf.zeit.as_str() < seit.as_str() {
                return false;
            }
        }
        true
    }
}

/// Was die Abfrage der Prozesszeilen einschränkt.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogFilter {
    /// Nur Zeilen, die diesen Text enthalten (ohne Groß-/Kleinschreibung).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enthaelt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anzahl: Option<usize>,
}

impl LogFilter {
    pub fn passt(&self, zeile: &Zeile) -> bool {
        if let Some(enthaelt) = &self.enthaelt {
            let gesucht = enthaelt.trim().to_lowercase();
            if !gesucht.is_empty() && !zeile.text.to_lowercase().contains(&gesucht) {
                return false;
            }
        }
        if let Some(seit) = &self.seit {
            if zeile.zeit.as_str() < seit.as_str() {
                return false;
            }
        }
        true
    }
}

/// Alles, was zu einem Aufruf gehört, so wie es aufgezeichnet wird.
pub struct Aufzeichnung<'a> {
    pub eintrag: &'a EintragId,
    pub tool: &'a str,
    pub agent: &'a str,
    pub dauer: Duration,
    pub status: Status,
    pub fehler: Option<&'a str>,
    pub argumente: Option<&'a serde_json::Map<String, Value>>,
    pub antwort: Option<&'a str>,
}

/// Das Protokoll: Ringpuffer, Dateien, Maske.
pub struct Protokoll {
    ordner: PathBuf,
    aufrufe: Mutex<VecDeque<Aufruf>>,
    zeilen: Mutex<VecDeque<Zeile>>,
    /// Die Werte aus dem Tresor, die nirgends auftauchen dürfen.
    geheimnisse: Mutex<Vec<String>>,
    /// Zählt jede neue Zeile – das Fenster lädt bei Änderung neu.
    zaehler: tokio::sync::watch::Sender<u64>,
}

impl std::fmt::Debug for Protokoll {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Protokoll")
            .field("ordner", &self.ordner)
            .finish_non_exhaustive()
    }
}

impl Protokoll {
    /// Das Protokoll eines Profils; alte Dateien werden gleich aufgeräumt.
    pub fn neu(profil: &Path) -> Self {
        let (zaehler, _) = tokio::sync::watch::channel(0);
        let protokoll = Self {
            ordner: profil.join(ORDNER),
            aufrufe: Mutex::new(VecDeque::new()),
            zeilen: Mutex::new(VecDeque::new()),
            geheimnisse: Mutex::new(Vec::new()),
            zaehler,
        };
        protokoll.aufraeumen();
        protokoll
    }

    /// Der Ordner der Protokolldateien.
    pub fn ordner(&self) -> &Path {
        &self.ordner
    }

    /// Ein Abonnement des Zählers (weckt das Fenster bei neuen Einträgen).
    pub fn zaehler(&self) -> tokio::sync::watch::Receiver<u64> {
        self.zaehler.subscribe()
    }

    /// Der aktuelle Stand des Zählers.
    pub fn stand(&self) -> u64 {
        *self.zaehler.borrow()
    }

    fn zaehlen(&self) {
        let neu = self.stand().wrapping_add(1);
        self.zaehler.send_replace(neu);
    }

    /// Die Werte, die nie im Protokoll stehen dürfen (aus dem Tresor).
    pub fn geheimnisse_setzen(&self, werte: Vec<String>) {
        let mut gespeichert = self
            .geheimnisse
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        // Lange zuerst ersetzen: sonst bliebe ein Rest eines langen Werts
        // stehen, wenn ein kurzer Wert darin enthalten ist.
        let mut werte: Vec<String> = werte
            .into_iter()
            .map(|wert| wert.trim().to_string())
            .filter(|wert| !wert.is_empty())
            .collect();
        werte.sort_by_key(|wert| std::cmp::Reverse(wert.chars().count()));
        werte.dedup();
        *gespeichert = werte;
    }

    /// Alle Geheimnisse durch `<verborgen>` ersetzen – auch als Teilstring.
    pub fn maskieren(&self, text: &str) -> String {
        self.maskieren_mit(text, &[])
    }

    /// Wie [`Protokoll::maskieren`], zusätzlich mit Werten, die nur für diesen
    /// einen Aufruf geheim sind (Argumente unter einem Geheimschlüssel).
    fn maskieren_mit(&self, text: &str, zusatz: &[String]) -> String {
        let mut text = text.to_string();
        for geheimnis in zusatz {
            if !geheimnis.is_empty() && text.contains(geheimnis.as_str()) {
                text = text.replace(geheimnis.as_str(), VERBORGEN);
            }
        }
        let geheimnisse = self
            .geheimnisse
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        for geheimnis in geheimnisse.iter() {
            if text.contains(geheimnis.as_str()) {
                text = text.replace(geheimnis.as_str(), VERBORGEN);
            }
        }
        text
    }

    /// Einen Aufruf aufzeichnen: in den Ringpuffer und in die Tagesdatei.
    pub fn aufruf_schreiben(&self, aufzeichnung: Aufzeichnung<'_>) {
        // Werte unter Geheimschlüsseln gelten für **diesen** Aufruf als
        // Geheimnis: sie dürfen auch in der Vorschau und im Fehlertext nicht
        // auftauchen (das Tool gibt seine Argumente oft zurück).
        let mut zusatz = Vec::new();
        if let Some(argumente) = aufzeichnung.argumente {
            geheime_werte(argumente, &mut zusatz);
        }
        let aufruf = Aufruf {
            zeit: jetzt(),
            agent: aufzeichnung.agent.to_string(),
            eintrag: aufzeichnung.eintrag.clone(),
            tool: aufzeichnung.tool.to_string(),
            dauer_ms: u64::try_from(aufzeichnung.dauer.as_millis()).unwrap_or(u64::MAX),
            status: aufzeichnung.status,
            fehler: aufzeichnung
                .fehler
                .map(|text| kuerzen(&self.maskieren_mit(text, &zusatz), FEHLER_ZEICHEN)),
            argumente: aufzeichnung
                .argumente
                .map(|argumente| self.argumente(argumente))
                .unwrap_or_default(),
            antwort_groesse: aufzeichnung.antwort.map(zeichenzahl).unwrap_or(0),
            vorschau: aufzeichnung
                .antwort
                .map(|antwort| kuerzen(&self.maskieren_mit(antwort, &zusatz), VORSCHAU_ZEICHEN))
                .unwrap_or_default(),
        };
        {
            let mut aufrufe = self
                .aufrufe
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            aufrufe.push_back(aufruf.clone());
            while aufrufe.len() > AUFRUFE_MAX {
                aufrufe.pop_front();
            }
        }
        if let Ok(zeile) = serde_json::to_string(&aufruf) {
            self.anhaengen("aufrufe", &zeile);
        }
    }

    /// Eine Prozesszeile eines Eintrags aufzeichnen (ohne ANSI-Farbcodes).
    pub fn zeile_schreiben(&self, eintrag: &EintragId, text: &str) {
        let text = self.maskieren(&ansi_entfernen(text));
        let zeile = Zeile {
            zeit: jetzt(),
            eintrag: eintrag.clone(),
            text,
        };
        {
            let mut zeilen = self
                .zeilen
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            zeilen.push_back(zeile.clone());
            while zeilen.len() > ZEILEN_MAX {
                zeilen.pop_front();
            }
        }
        if let Ok(text) = serde_json::to_string(&zeile) {
            self.anhaengen("prozesse", &text);
        }
    }

    /// Die Aufrufe, die zum Filter passen – die neuesten zuerst.
    pub fn abfragen(&self, filter: &Filter) -> Vec<Aufruf> {
        let grenze = filter.anzahl.unwrap_or(AUFRUFE_MAX).min(AUFRUFE_MAX);
        let aufrufe = self
            .aufrufe
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        aufrufe
            .iter()
            .rev()
            .filter(|aufruf| filter.passt(aufruf))
            .take(grenze)
            .cloned()
            .collect()
    }

    /// Die Prozesszeilen eines Eintrags – die neuesten zuerst.
    pub fn zeilen(&self, eintrag: &EintragId, filter: &LogFilter) -> Vec<Zeile> {
        let grenze = filter.anzahl.unwrap_or(ZEILEN_MAX).min(ZEILEN_MAX);
        let zeilen = self
            .zeilen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        zeilen
            .iter()
            .rev()
            .filter(|zeile| &zeile.eintrag == eintrag && filter.passt(zeile))
            .take(grenze)
            .cloned()
            .collect()
    }

    /// Speicher und Dateien leeren.
    pub fn leeren(&self) -> anyhow::Result<()> {
        self.aufrufe
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clear();
        self.zeilen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .clear();
        if self.ordner.is_dir() {
            for eintrag in std::fs::read_dir(&self.ordner)? {
                let eintrag = eintrag?;
                if ist_protokolldatei(&eintrag.path()) {
                    std::fs::remove_file(eintrag.path())?;
                }
            }
        }
        self.zaehlen();
        Ok(())
    }

    /// Alle Dateien löschen, die älter als 7 Tage sind, und danach so viele
    /// der ältesten, bis 50 MB nicht mehr überschritten sind.
    pub fn aufraeumen(&self) {
        let Ok(eintraege) = std::fs::read_dir(&self.ordner) else {
            return;
        };
        let heute = tage_seit_1970(SystemTime::now());
        let mut dateien: Vec<(i64, u64, PathBuf)> = Vec::new();
        for eintrag in eintraege.flatten() {
            let pfad = eintrag.path();
            if !ist_protokolldatei(&pfad) {
                continue;
            }
            let Some(tag) = tag_aus_datei(&pfad) else {
                continue;
            };
            let groesse = eintrag.metadata().map(|m| m.len()).unwrap_or(0);
            let alter = heute - tag;
            if alter > i64::try_from(TAGE).unwrap_or(i64::MAX) {
                let _ = std::fs::remove_file(&pfad);
                continue;
            }
            dateien.push((tag, groesse, pfad));
        }
        let mut gesamt: u64 = dateien.iter().map(|(_, groesse, _)| *groesse).sum();
        if gesamt <= GESAMT_BYTES {
            return;
        }
        dateien.sort();
        for (_, groesse, pfad) in dateien {
            if gesamt <= GESAMT_BYTES {
                break;
            }
            if std::fs::remove_file(&pfad).is_ok() {
                gesamt = gesamt.saturating_sub(groesse);
            }
        }
    }

    /// Eine Zeile an die Tagesdatei anhängen (Fehler nur ins Log: das
    /// Protokoll darf einen Werkzeug-Aufruf nie scheitern lassen).
    fn anhaengen(&self, art: &str, zeile: &str) {
        let tag = tag_heute();
        let pfad = self.ordner.join(format!("{art}-{tag}.jsonl"));
        if let Err(fehler) = crate::rechte::ordner_anlegen(&self.ordner) {
            tracing::warn!("Protokollordner nicht anlegbar: {fehler}");
            return;
        }
        let mut optionen = OpenOptions::new();
        optionen.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            optionen.mode(0o600);
        }
        match optionen.open(&pfad) {
            Ok(mut datei) => {
                // Eine schon vorhandene Datei könnte offener sein; angleichen.
                crate::rechte::datei_schuetzen(&pfad);
                if let Err(fehler) = writeln!(datei, "{zeile}") {
                    tracing::warn!("Protokoll nicht schreibbar: {fehler}");
                }
            }
            Err(fehler) => tracing::warn!("{pfad:?} nicht schreibbar: {fehler}"),
        }
        self.zaehlen();
    }

    /// Argumente als gekürztes JSON: Geheimschlüssel und Tresorwerte maskiert.
    pub fn argumente(&self, argumente: &serde_json::Map<String, Value>) -> String {
        let mut kopie = serde_json::Map::new();
        for (schluessel, wert) in argumente {
            if schluessel_geheim(schluessel) {
                kopie.insert(schluessel.clone(), Value::String(VERBORGEN.to_string()));
            } else {
                kopie.insert(schluessel.clone(), self.wert_kuerzen(wert));
            }
        }
        let text = serde_json::to_string(&Value::Object(kopie)).unwrap_or_default();
        kuerzen_auf_bytes(&text, ARGUMENTE_BYTES)
    }

    /// Ein einzelner Wert: Text kürzen, Kinder durchgehen.
    fn wert_kuerzen(&self, wert: &Value) -> Value {
        match wert {
            Value::String(text) => Value::String(kuerzen(&self.maskieren(text), WERT_ZEICHEN)),
            Value::Array(werte) => {
                Value::Array(werte.iter().map(|w| self.wert_kuerzen(w)).collect())
            }
            Value::Object(inhalt) => Value::Object(
                inhalt
                    .iter()
                    .map(|(schluessel, wert)| {
                        let wert = if schluessel_geheim(schluessel) {
                            Value::String(VERBORGEN.to_string())
                        } else {
                            self.wert_kuerzen(wert)
                        };
                        (schluessel.clone(), wert)
                    })
                    .collect(),
            ),
            andere => andere.clone(),
        }
    }
}

/// Enthält der Feldname etwas, das nach Geheimnis klingt?
/// `pat` steht in der Liste der Aufgabe – deshalb zählt auch `path` dazu.
pub fn schluessel_geheim(schluessel: &str) -> bool {
    let name = schluessel.to_lowercase();
    ["pass", "token", "secret", "key", "pat", "auth", "cookie"]
        .iter()
        .any(|teil| name.contains(teil))
}

/// Die Werte unter Geheimschlüsseln (auch verschachtelt) einsammeln.
fn geheime_werte(argumente: &serde_json::Map<String, Value>, aus: &mut Vec<String>) {
    for (schluessel, wert) in argumente {
        if schluessel_geheim(schluessel) {
            match wert {
                Value::String(text) if !text.trim().is_empty() => aus.push(text.clone()),
                Value::Object(inhalt) => geheime_werte(inhalt, aus),
                Value::Array(werte) => {
                    for wert in werte {
                        if let Value::String(text) = wert {
                            if !text.trim().is_empty() {
                                aus.push(text.clone());
                            }
                        }
                    }
                }
                _ => {}
            }
        } else if let Value::Object(inhalt) = wert {
            geheime_werte(inhalt, aus);
        }
    }
    // Lange zuerst: sonst bliebe der Rest eines längeren Werts stehen.
    aus.sort_by_key(|wert| std::cmp::Reverse(wert.chars().count()));
}

/// Wie viele Zeichen hat der Text?
pub fn zeichenzahl(text: &str) -> usize {
    text.chars().count()
}

/// Auf höchstens `grenze` Zeichen kürzen; ein `…` zeigt an, dass es fehlt.
pub fn kuerzen(text: &str, grenze: usize) -> String {
    if zeichenzahl(text) <= grenze {
        return text.to_string();
    }
    let mut gekuerzt: String = text.chars().take(grenze.saturating_sub(1)).collect();
    gekuerzt.push('…');
    gekuerzt
}

/// Auf höchstens `grenze` Bytes kürzen, ohne einen Codepoint zu zerteilen.
pub fn kuerzen_auf_bytes(text: &str, grenze: usize) -> String {
    if text.len() <= grenze {
        return text.to_string();
    }
    let mut ende = grenze.saturating_sub(3);
    while !text.is_char_boundary(ende) {
        ende = ende.saturating_sub(1);
    }
    format!("{}…", &text[..ende])
}

/// ANSI-Farbcodes entfernen (`ESC [ … Buchstabe`, `ESC ] … BEL`).
pub fn ansi_entfernen(text: &str) -> String {
    let mut aus = String::with_capacity(text.len());
    let mut zeichen = text.chars().peekable();
    while let Some(c) = zeichen.next() {
        if c != '\u{1b}' {
            aus.push(c);
            continue;
        }
        match zeichen.peek() {
            Some('[') => {
                zeichen.next();
                for c in zeichen.by_ref() {
                    if ('@'..='~').contains(&c) {
                        break;
                    }
                }
            }
            Some(']') => {
                zeichen.next();
                for c in zeichen.by_ref() {
                    if c == '\u{7}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    aus
}

/// Jetzt als RFC 3339 in UTC (`2026-01-02T03:04:05Z`).
pub fn jetzt() -> String {
    zeit_aus(SystemTime::now())
}

/// Ein Zeitpunkt als RFC 3339 in UTC.
pub fn zeit_aus(zeitpunkt: SystemTime) -> String {
    let sekunden = zeitpunkt
        .duration_since(UNIX_EPOCH)
        .map(|dauer| dauer.as_secs())
        .unwrap_or(0);
    format_zeit(sekunden)
}

/// Der Tag von heute (`JJJJ-MM-TT`).
pub fn tag_heute() -> String {
    tag_aus(&jetzt())
}

/// Der Tag eines RFC-3339-Zeitpunkts (`2026-01-02T…` → `2026-01-02`).
pub fn tag_aus(zeit: &str) -> String {
    zeit.split('T').next().unwrap_or("").to_string()
}

/// Der Anfang eines Zeitraums als RFC 3339 – Grundlage der Filter.
pub fn seit(zeitraum: Zeitraum) -> String {
    let jetzt = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let anfang = match zeitraum {
        Zeitraum::Stunde => jetzt.saturating_sub(3_600),
        Zeitraum::Heute => jetzt - jetzt % 86_400,
        Zeitraum::Tage => jetzt.saturating_sub(TAGE * 86_400),
    };
    format_zeit(anfang)
}

fn format_zeit(sekunden: u64) -> String {
    let tage = i64::try_from(sekunden / 86_400).unwrap_or(i64::MAX);
    let rest = sekunden % 86_400;
    let (jahr, monat, tag) = zivil_aus_tagen(tage);
    format!(
        "{jahr:04}-{monat:02}-{tag:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        (rest % 3_600) / 60,
        rest % 60
    )
}

/// Tage seit dem 1. Januar 1970.
fn tage_seit_1970(zeitpunkt: SystemTime) -> i64 {
    match zeitpunkt.duration_since(UNIX_EPOCH) {
        Ok(dauer) => i64::try_from(dauer.as_secs() / 86_400).unwrap_or(i64::MAX),
        Err(fehler) => -i64::try_from(fehler.duration().as_secs() / 86_400).unwrap_or(i64::MAX),
    }
}

/// Tag (Jahr, Monat, Tag) aus Tagen seit 1970 – nach Howard Hinnant.
fn zivil_aus_tagen(tage: i64) -> (i64, u32, u32) {
    let tage = tage + 719_468;
    let ara = tage.div_euclid(146_097);
    let jahr_des_ara = tage - ara * 146_097;
    let jahr = (jahr_des_ara - jahr_des_ara / 1_460 + jahr_des_ara / 36_524
        - jahr_des_ara / 146_096)
        / 365;
    let jahr_des_ara = jahr_des_ara - (365 * jahr + jahr / 4 - jahr / 100);
    let monat = (5 * jahr_des_ara + 2) / 153;
    let tag = jahr_des_ara - (153 * monat + 2) / 5 + 1;
    let monat = if monat < 10 { monat + 3 } else { monat - 9 };
    let jahr = jahr + ara * 400 + i64::from(monat <= 2);
    (
        jahr,
        u32::try_from(monat).unwrap_or(1),
        u32::try_from(tag).unwrap_or(1),
    )
}

/// Tage seit 1970 aus einem `JJJJ-MM-TT`.
fn tag_als_zahl(tag: &str) -> Option<i64> {
    let mut teile = tag.split('-');
    let jahr: i64 = teile.next()?.parse().ok()?;
    let monat: i64 = teile.next()?.parse().ok()?;
    let tag_des_monats: i64 = teile.next()?.parse().ok()?;
    // Umkehrung von `zivil_aus_tagen` (Howard Hinnant).
    let jahr = if monat <= 2 { jahr - 1 } else { jahr };
    let ara = jahr.div_euclid(400);
    let jahr_des_ara = jahr - ara * 400;
    let monat_ab_maerz = if monat > 2 { monat - 3 } else { monat + 9 };
    let tag_des_jahres = (153 * monat_ab_maerz + 2) / 5 + tag_des_monats - 1;
    let tag_des_ara = jahr_des_ara * 365 + jahr_des_ara / 4 - jahr_des_ara / 100 + tag_des_jahres;
    Some(ara * 146_097 + tag_des_ara - 719_468)
}

fn ist_protokolldatei(pfad: &Path) -> bool {
    pfad.extension().is_some_and(|ende| ende == "jsonl")
        && pfad
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("aufrufe-") || name.starts_with("prozesse-"))
}

/// Der Tag aus `aufrufe-2026-01-02.jsonl`.
fn tag_aus_datei(pfad: &Path) -> Option<i64> {
    let name = pfad.file_name()?.to_str()?;
    let (_, rest) = name.split_once('-')?;
    let tag = rest.strip_suffix(".jsonl")?;
    tag_als_zahl(tag)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ordner() -> PathBuf {
        // Der Zeitstempel allein reicht nicht (Mikrosekunden-Takt): gleichzeitig
        // startende Tests bekämen denselben Ordner.
        static ZAEHLER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let stempel = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let nummer = ZAEHLER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pfad = std::env::temp_dir().join(format!(
            "quiver-protokoll-{}-{stempel}-{nummer}",
            std::process::id()
        ));
        std::fs::create_dir_all(&pfad).expect("Ordner");
        pfad
    }

    fn aufzeichnung<'a>(
        eintrag: &'a EintragId,
        argumente: &'a serde_json::Map<String, Value>,
    ) -> Aufzeichnung<'a> {
        Aufzeichnung {
            eintrag,
            tool: "echo__echo",
            agent: "pi",
            dauer: Duration::from_millis(12),
            status: Status::Ok,
            fehler: None,
            argumente: Some(argumente),
            antwort: Some("hallo"),
        }
    }

    #[test]
    fn ein_aufruf_wird_mit_agent_dauer_und_status_aufgezeichnet() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let id = EintragId::neu("echo");
        let argumente = json!({ "text": "hallo" });
        protokoll.aufruf_schreiben(aufzeichnung(&id, argumente.as_object().unwrap()));

        let aufrufe = protokoll.abfragen(&Filter::default());
        assert_eq!(aufrufe.len(), 1);
        let aufruf = &aufrufe[0];
        assert_eq!(aufruf.agent, "pi");
        assert_eq!(aufruf.tool, "echo__echo");
        assert_eq!(aufruf.eintrag, id);
        assert_eq!(aufruf.dauer_ms, 12);
        assert_eq!(aufruf.status, Status::Ok);
        assert!(
            aufruf.zeit.ends_with('Z'),
            "RFC 3339 in UTC: {}",
            aufruf.zeit
        );
        assert_eq!(aufruf.argumente, r#"{"text":"hallo"}"#);
        assert_eq!(aufruf.vorschau, "hallo");
        assert_eq!(aufruf.antwort_groesse, 5);

        // Die Datei liegt im Ordner und wird fortgeschrieben.
        let datei = ordner
            .join(ORDNER)
            .join(format!("aufrufe-{}.jsonl", tag_heute()));
        let inhalt = std::fs::read_to_string(&datei).expect("Datei");
        assert!(inhalt.contains("\"agent\":\"pi\""), "{inhalt}");
        assert_eq!(inhalt.lines().count(), 1);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let rechte = std::fs::metadata(&datei)
                .expect("Datei")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(
                rechte, 0o600,
                "Protokolldateien sind nur für den Benutzer lesbar"
            );
        }
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn ein_fehler_wird_festgehalten() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let id = EintragId::neu("echo");
        protokoll.aufruf_schreiben(Aufzeichnung {
            status: Status::Fehler,
            fehler: Some("Der Eintrag hat die Verbindung beendet"),
            ..aufzeichnung(&id, &serde_json::Map::new())
        });
        let aufruf = &protokoll.abfragen(&Filter::default())[0];
        assert_eq!(aufruf.status, Status::Fehler);
        assert_eq!(
            aufruf.fehler.as_deref(),
            Some("Der Eintrag hat die Verbindung beendet")
        );
        assert_eq!(protokoll.abfragen(&Filter::default()).len(), 1);
        let nur_fehler = Filter {
            nur_fehler: true,
            ..Default::default()
        };
        assert_eq!(protokoll.abfragen(&nur_fehler).len(), 1);
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn argumente_mit_passwort_oder_api_token_sind_maskiert() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let argumente = json!({
            "passwort": "streng-geheim",
            "api_token": "qvr-token-123",
            "tief": { "PassWort": "auch-geheim", "sichtbar": "ja" },
            "liste": ["offen", "geheim"]
        });
        let text = protokoll.argumente(argumente.as_object().unwrap());
        assert!(!text.contains("streng-geheim"), "{text}");
        assert!(!text.contains("qvr-token-123"), "{text}");
        assert!(!text.contains("auch-geheim"), "{text}");
        assert!(text.contains(VERBORGEN), "{text}");
        assert!(text.contains("sichtbar"), "andere Werte bleiben: {text}");
        assert!(text.contains("offen"), "{text}");
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn ein_tresorwert_verschwindet_auch_als_teilstring() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        protokoll.geheimnisse_setzen(vec!["super-geheim-42".into(), "42".into()]);
        let id = EintragId::neu("echo");
        let argumente = json!({ "text": "davor super-geheim-42 danach" });
        protokoll.aufruf_schreiben(Aufzeichnung {
            fehler: Some("Abbruch mit super-geheim-42"),
            antwort: Some("Antwort nennt super-geheim-42"),
            ..aufzeichnung(&id, argumente.as_object().unwrap())
        });
        let aufruf = &protokoll.abfragen(&Filter::default())[0];
        for text in [
            aufruf.argumente.as_str(),
            aufruf.vorschau.as_str(),
            aufruf.fehler.as_deref().unwrap_or_default(),
        ] {
            assert!(!text.contains("super-geheim-42"), "{text}");
            assert!(!text.contains("42"), "{text}");
        }

        // Auch die Prozesszeile.
        protokoll.zeile_schreiben(&id, "gestartet mit super-geheim-42");
        let zeilen = protokoll.zeilen(&id, &LogFilter::default());
        assert_eq!(zeilen.len(), 1);
        assert!(!zeilen[0].text.contains("super-geheim"), "{:?}", zeilen[0]);

        // Und keine Datei.
        for datei in std::fs::read_dir(ordner.join(ORDNER)).expect("Ordner") {
            let text = std::fs::read_to_string(datei.expect("Datei").path()).expect("lesbar");
            assert!(!text.contains("super-geheim"), "{text}");
        }
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn die_ringe_behalten_nur_die_letzten_eintraege() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let id = EintragId::neu("echo");
        let argumente = serde_json::Map::new();
        for i in 0..(AUFRUFE_MAX + 5) {
            protokoll.aufruf_schreiben(Aufzeichnung {
                agent: "pi",
                tool: &format!("echo__{i}"),
                ..aufzeichnung(&id, &argumente)
            });
        }
        let aufrufe = protokoll.abfragen(&Filter::default());
        assert_eq!(aufrufe.len(), AUFRUFE_MAX);
        assert_eq!(aufrufe[0].tool, format!("echo__{}", AUFRUFE_MAX + 4));
        for i in 0..(ZEILEN_MAX + 5) {
            protokoll.zeile_schreiben(&id, &format!("Zeile {i}"));
        }
        let zeilen = protokoll.zeilen(&id, &LogFilter::default());
        assert_eq!(zeilen.len(), ZEILEN_MAX);
        assert_eq!(zeilen[0].text, format!("Zeile {}", ZEILEN_MAX + 4));
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn der_filter_wirkt_auf_eintrag_tool_agent_zeitraum_und_anzahl() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let argumente = serde_json::Map::new();
        let echo = EintragId::neu("echo");
        let mail = EintragId::neu("mail");
        protokoll.aufruf_schreiben(Aufzeichnung {
            tool: "echo__echo",
            agent: "pi",
            ..aufzeichnung(&echo, &argumente)
        });
        protokoll.aufruf_schreiben(Aufzeichnung {
            tool: "mail__lesen",
            agent: "hermes",
            status: Status::Fehler,
            ..aufzeichnung(&mail, &argumente)
        });

        let nach_eintrag = Filter {
            eintrag: Some(mail.clone()),
            ..Default::default()
        };
        assert_eq!(protokoll.abfragen(&nach_eintrag).len(), 1);
        let nach_tool = Filter {
            tool: Some("echo__echo".into()),
            ..Default::default()
        };
        assert_eq!(protokoll.abfragen(&nach_tool)[0].agent, "pi");
        let nach_agent = Filter {
            agent: Some("hermes".into()),
            ..Default::default()
        };
        assert_eq!(protokoll.abfragen(&nach_agent).len(), 1);
        let neueste = Filter {
            anzahl: Some(1),
            ..Default::default()
        };
        assert_eq!(protokoll.abfragen(&neueste)[0].tool, "mail__lesen");

        // Alles vor „jetzt“ liegt in der Zukunft: nichts passt.
        let spaeter = Filter {
            seit: Some("2999-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        assert!(protokoll.abfragen(&spaeter).is_empty());
        let frueher = Filter {
            seit: Some("1970-01-01T00:00:00Z".into()),
            ..Default::default()
        };
        assert_eq!(protokoll.abfragen(&frueher).len(), 2);
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn prozesszeilen_haengen_am_eintrag_und_verlieren_ansi_codes() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let echo = EintragId::neu("echo");
        let mail = EintragId::neu("mail");
        protokoll.zeile_schreiben(&echo, "\u{1b}[32mgrün\u{1b}[0m gestartet");
        protokoll.zeile_schreiben(&mail, "mail gestartet");
        let zeilen = protokoll.zeilen(&echo, &LogFilter::default());
        assert_eq!(zeilen.len(), 1);
        assert_eq!(zeilen[0].text, "grün gestartet");
        assert!(!zeilen[0].text.contains('\u{1b}'));

        let gefiltert = protokoll.zeilen(
            &echo,
            &LogFilter {
                enthaelt: Some("GRÜN".into()),
                ..Default::default()
            },
        );
        assert_eq!(gefiltert.len(), 1, "Groß-/Kleinschreibung zählt nicht");
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn alte_dateien_und_zu_viel_platz_fliegen_raus() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let unterordner = ordner.join(ORDNER);
        std::fs::create_dir_all(&unterordner).expect("Ordner");

        let heute = tage_seit_1970(SystemTime::now());
        let alt = {
            let (jahr, monat, tag) = zivil_aus_tagen(heute - 9);
            format!("{jahr:04}-{monat:02}-{tag:02}")
        };
        let alt_pfad = unterordner.join(format!("aufrufe-{alt}.jsonl"));
        std::fs::write(&alt_pfad, "{}\n").expect("schreibbar");
        let frisch = unterordner.join(format!("aufrufe-{}.jsonl", tag_heute()));
        std::fs::write(&frisch, "{}\n").expect("schreibbar");

        protokoll.aufraeumen();
        assert!(!alt_pfad.exists(), "älter als 7 Tage fliegt raus");
        assert!(frisch.exists(), "die frische Datei bleibt");
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn die_groessengrenze_loescht_die_aeltesten_zuerst() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let unterordner = ordner.join(ORDNER);
        std::fs::create_dir_all(&unterordner).expect("Ordner");
        let heute = tage_seit_1970(SystemTime::now());
        // Drei Dateien mit je 20 MB: zusammen 60 MB, also 10 MB zu viel.
        let mut pfade = Vec::new();
        for abstand in [0, 1, 2] {
            let (jahr, monat, tag) = zivil_aus_tagen(heute - abstand);
            let pfad = unterordner.join(format!("aufrufe-{jahr:04}-{monat:02}-{tag:02}.jsonl"));
            let datei = std::fs::File::create(&pfad).expect("Datei");
            datei.set_len(20 * 1024 * 1024).expect("Größe");
            pfade.push(pfad);
        }
        protokoll.aufraeumen();
        assert!(!pfade[2].exists(), "die älteste Datei fliegt zuerst raus");
        assert!(pfade[0].exists() && pfade[1].exists(), "40 MB bleiben");
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn leeren_raeumt_speicher_und_dateien() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let id = EintragId::neu("echo");
        let argumente = serde_json::Map::new();
        protokoll.aufruf_schreiben(aufzeichnung(&id, &argumente));
        protokoll.zeile_schreiben(&id, "gestartet");
        assert!(ordner.join(ORDNER).is_dir());
        protokoll.leeren().expect("leerbar");
        assert!(protokoll.abfragen(&Filter::default()).is_empty());
        assert!(protokoll.zeilen(&id, &LogFilter::default()).is_empty());
        let uebrig = std::fs::read_dir(ordner.join(ORDNER))
            .map(|eintraege| eintraege.flatten().count())
            .unwrap_or(0);
        assert_eq!(uebrig, 0, "keine Datei bleibt stehen");
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn der_zaehler_steigt_bei_jedem_eintrag() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let empfaenger = protokoll.zaehler();
        let anfang = protokoll.stand();
        let id = EintragId::neu("echo");
        let argumente = serde_json::Map::new();
        protokoll.aufruf_schreiben(aufzeichnung(&id, &argumente));
        assert_eq!(protokoll.stand(), anfang + 1);
        assert!(empfaenger.has_changed().unwrap_or(false));
        protokoll.leeren().expect("leerbar");
        assert_eq!(protokoll.stand(), anfang + 2);
        std::fs::remove_dir_all(&ordner).ok();
    }

    #[test]
    fn kuerzen_zaehlt_zeichen_und_setzt_einen_punkt() {
        assert_eq!(kuerzen("kurz", 10), "kurz");
        assert_eq!(kuerzen("0123456789", 5), "0123…");
        assert_eq!(zeichenzahl("äöü"), 3);
        let lang = "ä".repeat(100);
        let gekuerzt = kuerzen_auf_bytes(&lang, 10);
        assert!(gekuerzt.len() <= 10, "{gekuerzt:?}");
        assert!(gekuerzt.ends_with('…'));
    }

    #[test]
    fn ansi_codes_werden_entfernt() {
        assert_eq!(ansi_entfernen("\u{1b}[31mrot\u{1b}[0m"), "rot");
        assert_eq!(ansi_entfernen("\u{1b}[1;32mfett\u{1b}[0m ok"), "fett ok");
        assert_eq!(ansi_entfernen("ohne"), "ohne");
        assert_eq!(ansi_entfernen("\u{1b}]0;Titel\u{7}Text"), "Text");
    }

    #[test]
    fn zeit_ist_rfc3339_und_tage_rechnen_richtig() {
        // 2026-01-02T03:04:05Z
        let zeitpunkt = UNIX_EPOCH + Duration::from_secs(1_767_323_045);
        assert_eq!(zeit_aus(zeitpunkt), "2026-01-02T03:04:05Z");
        assert_eq!(tag_aus("2026-01-02T03:04:05Z"), "2026-01-02");
        assert_eq!(tag_als_zahl("1970-01-01"), Some(0));
        assert_eq!(zivil_aus_tagen(0), (1970, 1, 1));
        // Schaltjahr: der 29. Februar 2024 liegt 19782 Tage nach 1970.
        assert_eq!(zivil_aus_tagen(19_782), (2024, 2, 29));
        assert_eq!(tag_als_zahl("2024-02-29"), Some(19_782));
        for tag in ["1970-01-01", "2000-03-01", "2026-12-31", "2024-02-29"] {
            assert_eq!(zivil_aus_tagen(tag_als_zahl(tag).unwrap()), {
                let mut teile = tag.split('-');
                (
                    teile.next().unwrap().parse::<i64>().unwrap(),
                    teile.next().unwrap().parse::<u32>().unwrap(),
                    teile.next().unwrap().parse::<u32>().unwrap(),
                )
            });
        }
        assert!(seit(Zeitraum::Tage) < seit(Zeitraum::Stunde));
        assert!(seit(Zeitraum::Heute).ends_with("T00:00:00Z"));
        assert!(seit(Zeitraum::Heute).starts_with(&tag_heute()));
    }

    #[test]
    fn dateinamen_werden_erkannt() {
        assert!(ist_protokolldatei(Path::new("aufrufe-2026-01-02.jsonl")));
        assert!(ist_protokolldatei(Path::new("prozesse-2026-01-02.jsonl")));
        assert!(!ist_protokolldatei(Path::new("anderes-2026-01-02.jsonl")));
        assert!(!ist_protokolldatei(Path::new("aufrufe-2026-01-02.txt")));
        assert_eq!(
            tag_aus_datei(Path::new("prozesse-2026-01-02.jsonl")),
            tag_als_zahl("2026-01-02")
        );
    }
}
