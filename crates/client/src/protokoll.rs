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
//!   `pat` gilt nur als **Wort** (`github_pat` ja, `path` nein). Und **jeder**
//!   Wert aus dem Tresor wird ersetzt – in Argumenten, in der Vorschau, im
//!   Fehlertext und in den Prozesszeilen. Werte ab sechs Zeichen auch als
//!   Teilstring, kürzere nur als ganzes Wort (sonst verschwände jedes `a`),
//!   zusätzlich in Base64- und URL-kodierter Form.
//! - **Kürzen.** Je Wert 200 Zeichen, Argumente zusammen 4 KB, Vorschau 500,
//!   Fehlertext 2000, Prozesszeile 2000. Was nicht gekürzt wird, wächst unbegrenzt.
//! - **Begrenzen.** Je Eintrag höchstens 200 Prozesszeilen je Minute auf der
//!   Platte, je Tagesdatei eine feste Größe – geprüft **vor** dem Schreiben.
//!
//! Der Speicher ist ein Ringpuffer (2000 Aufrufe, 5000 Zeilen). Auf der Platte
//! liegen `<profil>/protokoll/aufrufe-JJJJ-MM-TT.jsonl` und
//! `prozesse-JJJJ-MM-TT.jsonl` mit Rechten 0600; was älter als 7 Tage ist,
//! fliegt weg, und insgesamt bleiben höchstens 50 MB.

use std::collections::{BTreeMap, VecDeque};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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
/// So lang darf eine Prozesszeile sein (Puffer einer Zeile: 8 KB).
pub const ZEILE_ZEICHEN: usize = 2000;
/// So viele Prozesszeilen eines Eintrags gehen je Minute auf die Platte.
pub const ZEILEN_JE_MINUTE: usize = 200;
/// So groß darf eine Tagesdatei werden – geprüft **vor** dem Schreiben.
pub const DATEI_BYTES: u64 = 10 * 1024 * 1024;
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
    /// Zählt je Eintrag die Zeilen der laufenden Minute auf der Platte.
    zeilen_je_minute: Mutex<BTreeMap<EintragId, (Instant, usize)>>,
    /// Wie viele Zeilen wegen der Grenzen **nicht** auf die Platte gingen.
    verworfen: Mutex<u64>,
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
            zeilen_je_minute: Mutex::new(BTreeMap::new()),
            verworfen: Mutex::new(0),
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

    /// Wie viele Zeilen die Grenzen abgewiesen haben (für Prüfungen).
    pub fn verworfene_zeilen(&self) -> u64 {
        *self
            .verworfen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
    }

    /// Eine Zeile, die schon gekürzt wurde, als verworfen zählen (der Rest
    /// hinter dem Puffer erreicht das Protokoll nicht).
    pub fn zeile_verworfen(&self, _eintrag: &EintragId) {
        let mut verworfen = self
            .verworfen
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        *verworfen = verworfen.saturating_add(1);
    }

    /// Alle Geheimnisse durch `<verborgen>` ersetzen.
    ///
    /// Lange Werte (ab sechs Zeichen) verschwinden auch als Teilstring, kurze
    /// nur als **ganzes Wort** – sonst würde ein Geheimnis wie `a` jeden Text
    /// unlesbar machen. Zusätzlich verschwinden die Base64- und die
    /// URL-kodierte Form: ein Geheimnis, das in einer Adresse oder in einer
    /// JSON-Nutzlast steckt, ist genauso geheim.
    pub fn maskieren(&self, text: &str) -> String {
        self.maskieren_mit(text, &[])
    }

    /// Wie [`Protokoll::maskieren`], zusätzlich mit Werten, die nur für diesen
    /// einen Aufruf geheim sind (Argumente unter einem Geheimschlüssel).
    fn maskieren_mit(&self, text: &str, zusatz: &[String]) -> String {
        let mut text = text.to_string();
        for geheimnis in zusatz {
            text = maskieren_wert(&text, geheimnis);
        }
        let geheimnisse = self
            .geheimnisse
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        for geheimnis in geheimnisse.iter() {
            text = maskieren_wert(&text, geheimnis);
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
            fehler: aufzeichnung.fehler.map(|text| {
                kuerzen(
                    &self.maskieren_mit(&anzeige_zeichen(text), &zusatz),
                    FEHLER_ZEICHEN,
                )
            }),
            argumente: aufzeichnung
                .argumente
                .map(|argumente| self.argumente(argumente))
                .unwrap_or_default(),
            antwort_groesse: aufzeichnung.antwort.map(zeichenzahl).unwrap_or(0),
            vorschau: aufzeichnung
                .antwort
                .map(|antwort| {
                    kuerzen(
                        &self.maskieren_mit(&anzeige_zeichen(antwort), &zusatz),
                        VORSCHAU_ZEICHEN,
                    )
                })
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
        self.zaehlen();
    }

    /// Eine Prozesszeile eines Eintrags aufzeichnen (ohne ANSI-Farbcodes).
    ///
    /// Die Zeile wird maskiert (Tresorwerte **und** Schlüsselwörter wie
    /// `token`), auf [`ZEILE_ZEICHEN`] gekürzt und nur dann auf die Platte
    /// geschrieben, wenn der Eintrag in dieser Minute noch nicht
    /// [`ZEILEN_JE_MINUTE`] Zeilen geschrieben hat und die Tagesdatei noch
    /// Platz hat. Im Speicher steht sie trotzdem – das Fenster soll zeigen,
    /// was der Prozess sagt.
    pub fn zeile_schreiben(&self, eintrag: &EintragId, text: &str) {
        let text = kuerzen(
            &crate::anmeldung::maskieren(&self.maskieren(&anzeige_zeichen(text))),
            ZEILE_ZEICHEN,
        );
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
        let platz = self.platz_fuer_zeile(eintrag);
        if !platz {
            return;
        }
        if let Ok(text) = serde_json::to_string(&zeile) {
            self.anhaengen("prozesse", &text);
        }
        self.zaehlen();
    }

    /// Darf dieser Eintrag in dieser Minute noch eine Zeile auf die Platte
    /// schreiben? Zählt mit und sagt `false`, wenn die Grenze erreicht ist.
    fn platz_fuer_zeile(&self, eintrag: &EintragId) -> bool {
        let mut zaehler = self
            .zeilen_je_minute
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        let jetzt = Instant::now();
        let stand = zaehler.entry(eintrag.clone()).or_insert_with(|| (jetzt, 0));
        if jetzt.duration_since(stand.0) >= Duration::from_secs(60) {
            *stand = (jetzt, 0);
        }
        if stand.1 >= ZEILEN_JE_MINUTE {
            let mut verworfen = self
                .verworfen
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            *verworfen = verworfen.saturating_add(1);
            return false;
        }
        stand.1 += 1;
        true
    }

    /// Hat dieser Agent den Eintrag selbst benutzt? Nur dann darf er dessen
    /// Prozesszeilen lesen (Paket 3).
    pub fn von_agent_benutzt(&self, agent: Option<&str>, eintrag: &EintragId) -> bool {
        let Some(agent) = agent else {
            return false;
        };
        let aufrufe = self
            .aufrufe
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner());
        aufrufe
            .iter()
            .any(|aufruf| aufruf.agent == agent && &aufruf.eintrag == eintrag)
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
    ///
    /// Die Größe der Datei wird **vor** dem Schreiben geprüft: eine Datei
    /// wächst nie über [`DATEI_BYTES`] hinaus.
    fn anhaengen(&self, art: &str, zeile: &str) {
        let tag = tag_heute();
        let pfad = self.ordner.join(format!("{art}-{tag}.jsonl"));
        if let Err(fehler) = crate::rechte::ordner_anlegen(&self.ordner) {
            tracing::warn!("Protokollordner nicht anlegbar: {fehler}");
            return;
        }
        let groesse = std::fs::metadata(&pfad).map(|meta| meta.len()).unwrap_or(0);
        let neu = u64::try_from(zeile.len() + 1).unwrap_or(u64::MAX);
        if groesse.saturating_add(neu) > DATEI_BYTES {
            let mut verworfen = self
                .verworfen
                .lock()
                .unwrap_or_else(|vergiftet| vergiftet.into_inner());
            *verworfen = verworfen.saturating_add(1);
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
            Value::String(text) => Value::String(kuerzen(
                &self.maskieren(&anzeige_zeichen(text)),
                WERT_ZEICHEN,
            )),
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
///
/// `pat` gilt nur als **Wort**: `github_pat` und `pat_wert` ja, `path` und
/// `pattern` nein – die stehen in fast jeder Konfiguration und wären sonst
/// ständig `verborgen`.
pub fn schluessel_geheim(schluessel: &str) -> bool {
    let name = schluessel.to_lowercase();
    if ["pass", "token", "secret", "key", "auth", "cookie"]
        .iter()
        .any(|teil| name.contains(teil))
    {
        return true;
    }
    name.split(|c: char| !c.is_alphanumeric())
        .any(|wort| wort == "pat")
}

/// Ist das Zeichen ein Teil eines Worts? (Für „nur als ganzes Wort“.)
fn wortzeichen(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Ein Geheimnis im Text ersetzen – mit den Regeln aus der Moduldoku.
pub fn maskieren_wert(text: &str, geheimnis: &str) -> String {
    if geheimnis.is_empty() || text.is_empty() {
        return text.to_string();
    }
    let kurz = geheimnis.chars().count() < 6;
    let mut aus = text.to_string();
    // Die kodierten Formen zuerst: sie enthalten oft das Original als Teil.
    for form in kodierte_formen(geheimnis) {
        if aus.contains(&form) {
            aus = aus.replace(&form, VERBORGEN);
        }
    }
    if kurz {
        ganzes_wort_ersetzen(&aus, geheimnis)
    } else {
        aus.replace(geheimnis, VERBORGEN)
    }
}

/// Alle Vorkommen von `wort` ersetzen, die ein **ganzes Wort** bilden.
fn ganzes_wort_ersetzen(text: &str, wort: &str) -> String {
    let mut aus = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(stelle) = rest.find(wort) {
        let davor = &rest[..stelle];
        let danach = &rest[stelle + wort.len()..];
        let grenze_vorn = davor.chars().next_back().is_none_or(|c| !wortzeichen(c));
        let grenze_hinten = danach.chars().next().is_none_or(|c| !wortzeichen(c));
        aus.push_str(davor);
        if grenze_vorn && grenze_hinten {
            aus.push_str(VERBORGEN);
        } else {
            aus.push_str(wort);
        }
        rest = danach;
    }
    aus.push_str(rest);
    aus
}

/// Die Base64- (Standard und URL-safe) und die URL-kodierte Form eines Werts.
fn kodierte_formen(wert: &str) -> Vec<String> {
    use base64::Engine as _;
    let mut formen = vec![
        base64::engine::general_purpose::STANDARD.encode(wert.as_bytes()),
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(wert.as_bytes()),
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(wert.as_bytes()),
        url_kodiert(wert),
    ];
    formen.retain(|form| form != wert && form.chars().count() >= 6);
    formen.sort_by_key(|form| std::cmp::Reverse(form.len()));
    formen.dedup();
    formen
}

/// Prozentkodierung nach RFC 3986: alles außer `A-Za-z0-9-._~` wird kodiert.
fn url_kodiert(wert: &str) -> String {
    let mut aus = String::with_capacity(wert.len());
    for byte in wert.as_bytes() {
        let zeichen = *byte as char;
        if zeichen.is_ascii_alphanumeric() || matches!(zeichen, '-' | '.' | '_' | '~') {
            aus.push(zeichen);
        } else {
            aus.push_str(&format!("%{byte:02X}"));
        }
    }
    aus
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

/// ANSI-Farbcodes entfernen (`ESC [ … Buchstabe`, `ESC ] … BEL`) und
/// Steuerzeichen sowie Richtungszeichen (bidi) wegnehmen. Ein Werkzeug darf
/// mit seiner Ausgabe keine Anzeige umsortieren oder eine Zeile zerlegen.
pub fn anzeige_zeichen(text: &str) -> String {
    let mut aus = String::with_capacity(text.len());
    for c in ansi_entfernen(text).chars() {
        if c.is_control() || ist_richtungszeichen(c) {
            continue;
        }
        aus.push(c);
    }
    aus
}

/// Richtungszeichen (bidi): sie drehen die Anzeige um.
pub fn ist_richtungszeichen(c: char) -> bool {
    matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
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
        use std::sync::atomic::{AtomicUsize, Ordering};
        static ZAEHLER: AtomicUsize = AtomicUsize::new(0);
        let nummer = ZAEHLER.fetch_add(1, Ordering::SeqCst);
        let stempel = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Systemzeit nach 1970")
            .as_nanos();
        let pfad = std::env::temp_dir().join(format!(
            "quiver-protokoll-{}-{nummer}-{stempel}",
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

    // ---------------------------------------------------------- Paket 3

    /// Kurze Werte (unter sechs Zeichen) verschwinden nur als **ganzes Wort**
    /// – sonst würde ein Geheimnis wie `a` jeden Text unlesbar machen.
    #[test]
    fn kurze_werte_verschwinden_nur_als_ganzes_wort() {
        assert_eq!(
            maskieren_wert("davor 42 danach", "42"),
            "davor <verborgen> danach"
        );
        assert_eq!(maskieren_wert("wert=42", "42"), "wert=<verborgen>");
        assert_eq!(
            maskieren_wert("super-geheim-42", "42"),
            "super-geheim-<verborgen>",
            "ein Bindestrich trennt Wörter"
        );
        assert_eq!(
            maskieren_wert("die Zahl 4210 ist groß", "42"),
            "die Zahl 4210 ist groß",
            "mitten im Wort bleibt sie stehen"
        );
        assert_eq!(maskieren_wert("x42y", "42"), "x42y");
        // Ab sechs Zeichen auch als Teilstring.
        assert_eq!(
            maskieren_wert("davor super-geheim danach", "super-geheim"),
            "davor <verborgen> danach"
        );
        assert_eq!(
            maskieren_wert("xsuper-geheimy", "super-geheim"),
            "x<verborgen>y"
        );
    }

    /// Base64- und URL-kodiert ist ein Geheimnis genauso geheim.
    #[test]
    fn kodierte_formen_verschwinden_auch() {
        let wert = "streng/geheim?42";
        let base64 = {
            use base64::Engine as _;
            base64::engine::general_purpose::STANDARD.encode(wert.as_bytes())
        };
        let url = "streng%2Fgeheim%3F42";
        assert_eq!(url_kodiert(wert), url);
        for text in [format!("wert={base64}"), format!("adresse?t={url}")] {
            let maskiert = maskieren_wert(&text, wert);
            assert!(maskiert.contains(VERBORGEN), "{maskiert}");
            assert!(!maskiert.contains(&base64), "{maskiert}");
            assert!(!maskiert.contains(url), "{maskiert}");
        }
    }

    /// `pat` gilt nur als Wort: `path` und `pattern` sind keine Geheimnisse.
    #[test]
    fn pat_ist_ein_wort_und_path_nicht() {
        for geheim in ["pat", "github_pat", "pat_wert", "MY-PAT"] {
            assert!(schluessel_geheim(geheim), "{geheim}");
        }
        for offen in ["path", "file_path", "pattern", "patch", "sympathy"] {
            assert!(!schluessel_geheim(offen), "{offen}");
        }
        assert!(schluessel_geheim("api_token"));
        assert!(schluessel_geheim("passwort"));
    }

    /// Eine Prozesszeile wird auf 2000 Zeichen gekürzt und behält keine
    /// Steuer- oder Richtungszeichen.
    #[test]
    fn prozesszeilen_sind_gekuerzt_und_ohne_steuerzeichen() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let id = EintragId::neu("echo");
        let lang = format!("Anfang\u{1b}[31m{}\u{202e}Ende", "x".repeat(5000));
        protokoll.zeile_schreiben(&id, &lang);
        let zeile = &protokoll.zeilen(&id, &LogFilter::default())[0];
        assert_eq!(zeile.text.chars().count(), ZEILE_ZEICHEN, "gekürzt");
        assert!(!zeile.text.contains('\u{1b}'), "{}", zeile.text);
        assert!(!zeile.text.contains('\u{202e}'), "keine Richtungszeichen");
        std::fs::remove_dir_all(&ordner).ok();
    }

    /// Je Eintrag gehen höchstens 200 Zeilen je Minute auf die Platte; im
    /// Speicher stehen sie trotzdem.
    #[test]
    fn je_minute_gehen_hoechstens_200_zeilen_auf_die_platte() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let id = EintragId::neu("echo");
        for nummer in 0..(ZEILEN_JE_MINUTE + 50) {
            protokoll.zeile_schreiben(&id, &format!("Zeile {nummer}"));
        }
        assert_eq!(
            protokoll.zeilen(&id, &LogFilter::default()).len(),
            ZEILEN_JE_MINUTE + 50,
            "im Speicher steht alles"
        );
        let datei = ordner
            .join(ORDNER)
            .join(format!("prozesse-{}.jsonl", tag_heute()));
        let inhalt = std::fs::read_to_string(&datei).expect("Datei");
        assert_eq!(inhalt.lines().count(), ZEILEN_JE_MINUTE, "{inhalt}");
        assert_eq!(protokoll.verworfene_zeilen(), 50);
        std::fs::remove_dir_all(&ordner).ok();
    }

    /// Die Größe der Tagesdatei wird **vor** dem Schreiben geprüft: sie wächst
    /// nie über die Grenze.
    #[test]
    fn eine_volle_tagesdatei_waechst_nicht_weiter() {
        let ordner = ordner();
        let protokoll = Protokoll::neu(&ordner);
        let id = EintragId::neu("echo");
        let datei = ordner
            .join(ORDNER)
            .join(format!("prozesse-{}.jsonl", tag_heute()));
        std::fs::create_dir_all(datei.parent().expect("Ordner")).expect("Ordner");
        std::fs::File::create(&datei)
            .expect("Datei")
            .set_len(DATEI_BYTES)
            .expect("Größe");
        protokoll.zeile_schreiben(&id, "passt nicht mehr");
        assert_eq!(
            std::fs::metadata(&datei).expect("Datei").len(),
            DATEI_BYTES,
            "nichts wurde angehängt"
        );
        assert_eq!(protokoll.verworfene_zeilen(), 1);
        std::fs::remove_dir_all(&ordner).ok();
    }
}
