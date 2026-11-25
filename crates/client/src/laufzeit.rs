// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Wie ein Eintrag tatsächlich läuft: als Kindprozess mit MCP über
//! stdin/stdout oder als entfernter MCP-Server über Streamable HTTP.
//!
//! Beides endet in derselben Form: ein [`Peer`], über den der Client
//! `tools/list` und `tools/call` spricht. Der Endpunkt für die Agenten reicht
//! die Aufrufe nur weiter – Konfiguration und Zugangsdaten bleiben hier.
//!
//! Grenzen stehen hier, weil hier der fremde Code läuft (Paket 3):
//!
//! - **Nachrichten** über stdio höchstens [`NACHRICHT_BYTES`]; eine größere
//!   Zeile wird gezählt und der Eintrag neu gestartet (die Verbindung ist
//!   danach nicht mehr vertrauenswürdig).
//! - **Werkzeuglisten** höchstens [`WERKZEUGE_MAX`] Werkzeuge mit geprüftem
//!   Namen, gekürzter Beschreibung und begrenztem Schema; Steuer- und
//!   Richtungszeichen fliegen raus.
//! - **Aufrufe** haben eine Frist (Standard [`AUFRUF_FRIST`], je Eintrag bis
//!   [`AUFRUF_FRIST_MAX`]); Fortschrittsmeldungen des Tools verlängern sie.
//!   Ein Abbruch des Agenten bricht auch den Aufruf beim Tool ab.
//! - **Umgebung**: der Prozess erbt **nicht** alles, sondern nur, was in
//!   [`UMGEBUNG_ERLAUBT`] steht, plus die Werte des Eintrags.

use std::collections::BTreeMap;
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;

use anyhow::Context as _;
use quiver_domain::EintragId;
use quiver_domain::Laufzeit;
use rmcp::model::{
    CallToolRequest, CallToolRequestParams, CallToolResult, CancelledNotification,
    CancelledNotificationParam, ClientNotification, ClientRequest, ServerResult, Tool,
};
use rmcp::service::{
    ClientInitializeError, PeerRequestOptions, RunningService, RunningServiceCancellationToken,
};
use rmcp::transport::async_rw::AsyncRwTransport;
use rmcp::transport::streamable_http_client::{
    StreamableHttpClientTransport, StreamableHttpClientTransportConfig,
};
use rmcp::{Peer, RoleClient, ServiceExt};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncRead, BufReader, ReadBuf};
use tokio::sync::Notify;

use crate::protokoll::Protokoll;

/// So groß darf eine Meldung des Tools über stdio höchstens sein.
pub const NACHRICHT_BYTES: usize = 16 * 1024 * 1024;
/// So viele Werkzeuge bietet ein Eintrag höchstens an.
pub const WERKZEUGE_MAX: usize = 500;
/// So lang darf ein Werkzeugname sein.
pub const WERKZEUG_NAME_MAX: usize = 128;
/// So lang darf die Beschreibung eines Werkzeugs sein.
pub const WERKZEUG_BESCHREIBUNG_MAX: usize = 4000;
/// So groß darf das Eingabeschema eines Werkzeugs sein.
pub const WERKZEUG_SCHEMA_BYTES: usize = 64 * 1024;
/// So lange darf ein Werkzeugaufruf höchstens dauern, ohne dass das Tool
/// etwas von sich hören lässt.
pub const AUFRUF_FRIST: Duration = Duration::from_secs(120);
/// So lange darf ein Aufruf höchstens dauern, auch mit Fortschrittsmeldungen.
pub const AUFRUF_FRIST_MAX: Duration = Duration::from_secs(15 * 60);

/// Die Umgebungsvariablen, die ein Werkzeug erben darf. Alles andere – und
/// damit jeder Fremdschlüssel, jedes Cloud-Token der Shell – bleibt draußen.
/// Zusätzlich gelten Präfixe (siehe [`UMGEBUNG_PRAEFIXE`]).
pub const UMGEBUNG_ERLAUBT: [&str; 24] = [
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "LANG",
    "LANGUAGE",
    "TZ",
    "TMPDIR",
    "TMP",
    "TEMP",
    "TERM",
    "COLORTERM",
    "NO_COLOR",
    "FORCE_COLOR",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "NO_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "no_proxy",
    "all_proxy",
    "SSL_CERT_FILE",
];

/// Präfixe, die ein Werkzeug erben darf: Sprache, XDG-Ordner und die
/// Fenster-/Dienstumgebung, ohne die ein Prozess auf macOS und Linux nicht
/// startet. `QUIVER_*` steht **nicht** dabei: das ist die Umgebung des
/// Clients, und die gehört nicht in ein fremdes Werkzeug (Paket 3).
pub const UMGEBUNG_PRAEFIXE: [&str; 5] = ["LC_", "XDG_", "DBUS_", "__CF", "npm_"];

/// Namen, die nur auf bestimmten Plattformen nötig sind (Windows braucht
/// `SystemRoot` für Winsock, `APPDATA` für Profile). Sie werden nicht per
/// `cfg` gesetzt, sondern als Daten mitgegeben – was fehlt, fehlt.
pub const UMGEBUNG_PLATTFORM: [&str; 17] = [
    "SystemRoot",
    "SystemDrive",
    "windir",
    "COMSPEC",
    "PATHEXT",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "PROGRAMDATA",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
    "OS",
    "USERNAME",
];

/// Darf diese Variable ein Werkzeug erben?
///
/// Windows unterscheidet bei Namen nicht zwischen Groß und Klein (`Path` ist
/// `PATH`); dort vergleichen wir ebenso.
pub fn umgebung_erlaubt(name: &str) -> bool {
    umgebung_erlaubt_auf(name, cfg!(windows))
}

fn umgebung_erlaubt_auf(name: &str, ohne_gross_klein: bool) -> bool {
    let gleich = |erlaubt: &&str| {
        if ohne_gross_klein {
            erlaubt.eq_ignore_ascii_case(name)
        } else {
            *erlaubt == name
        }
    };
    UMGEBUNG_ERLAUBT.iter().any(gleich)
        || UMGEBUNG_PLATTFORM.iter().any(gleich)
        || UMGEBUNG_PRAEFIXE
            .iter()
            .any(|praefix| name.starts_with(praefix))
}

/// Die Umgebung eines Werkzeugprozesses setzen: **nur** die erlaubten Werte
/// der Umgebung des Clients und danach die Werte des Eintrags (die gewinnen).
pub fn umgebung_setzen(kommando: &mut tokio::process::Command, eintrag: &quiver_domain::Werte) {
    kommando.env_clear();
    // `vars_os`: ein Wert, der kein UTF-8 ist, darf den Start nicht abbrechen.
    for (name, wert) in std::env::vars_os() {
        if name.to_str().is_some_and(umgebung_erlaubt) {
            kommando.env(name, wert);
        }
    }
    for (name, wert) in eintrag {
        kommando.env(name, wert);
    }
}

/// Eine laufende Verbindung zu genau einem Eintrag.
pub struct EintragVerbindung {
    peer: Peer<RoleClient>,
    /// Wird wahr, wenn der Dienst von selbst endet (Prozess tot, Leitung weg).
    ende: tokio::sync::oneshot::Receiver<()>,
    abbruch: Mutex<Option<RunningServiceCancellationToken>>,
    /// Der Kindprozess (nur bei stdio): `beenden` tötet ihn sofort.
    kind: Mutex<Option<tokio::process::Child>>,
    /// Zählt Meldungen, die die Größengrenze überschritten.
    wache: Arc<GroessenWache>,
}

impl EintragVerbindung {
    /// Die Werkzeuge, die der Eintrag gerade anbietet – geprüft und begrenzt.
    pub async fn werkzeuge(&self) -> anyhow::Result<Vec<Tool>> {
        let roh = self
            .peer
            .list_all_tools()
            .await
            .context("tools/list beim Eintrag")?;
        Ok(werkzeuge_pruefen(roh))
    }

    /// Der Peer für den Endpunkt (Aufrufe der Agenten).
    pub fn peer(&self) -> &Peer<RoleClient> {
        &self.peer
    }

    /// Warten, bis der Dienst endet (Prozess tot oder Leitung zu).
    pub async fn warten(&mut self) {
        let _ = (&mut self.ende).await;
    }

    /// Die Wache für zu große Meldungen (der Aufseher wartet darauf).
    pub(crate) fn groessen_wache(&self) -> Arc<GroessenWache> {
        self.wache.clone()
    }

    /// Die Verbindung schließen – bei stdio beendet das den Kindprozess.
    pub fn beenden(&self) {
        let abbruch = self
            .abbruch
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .take();
        if let Some(abbruch) = abbruch {
            abbruch.cancel();
        }
        let kind = self
            .kind
            .lock()
            .unwrap_or_else(|vergiftet| vergiftet.into_inner())
            .take();
        if let Some(mut kind) = kind {
            let _ = kind.start_kill();
        }
    }
}

/// Zählt, wie oft eine Meldung zu groß war, und weckt den Aufseher.
#[derive(Debug, Default)]
pub(crate) struct GroessenWache {
    anzahl: AtomicU64,
    meldung: Notify,
}

impl GroessenWache {
    fn melden(&self) {
        self.anzahl.fetch_add(1, Ordering::Relaxed);
        self.meldung.notify_waiters();
    }

    /// Warten, bis eine Meldung die Größengrenze überschreitet.
    pub(crate) async fn warten(&self) {
        loop {
            let warten = self.meldung.notified();
            tokio::pin!(warten);
            warten.as_mut().enable();
            if self.anzahl.load(Ordering::Relaxed) > 0 {
                return;
            }
            warten.await;
        }
    }
}

/// Ein Leser, der die Zeilenlänge **zählt**, ohne sie zu kopieren: mehr als
/// [`NACHRICHT_BYTES`] in einer Zeile meldet er der [`GroessenWache`].
struct ZeilenWache<R> {
    innen: R,
    /// Wie lang die laufende Zeile schon ist.
    laenge: usize,
    /// Wurde die laufende Zeile schon gemeldet?
    gemeldet: bool,
    wache: Arc<GroessenWache>,
}

impl<R> ZeilenWache<R> {
    fn neu(innen: R, wache: Arc<GroessenWache>) -> Self {
        Self {
            innen,
            laenge: 0,
            gemeldet: false,
            wache,
        }
    }

    fn zaehlen(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if *byte == b'\n' {
                self.laenge = 0;
                self.gemeldet = false;
                continue;
            }
            self.laenge += 1;
            if self.laenge > NACHRICHT_BYTES && !self.gemeldet {
                self.gemeldet = true;
                self.wache.melden();
            }
        }
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for ZeilenWache<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let vorher = buf.filled().len();
        let ergebnis = Pin::new(&mut self.innen).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &ergebnis {
            let neu = buf.filled()[vorher..].to_vec();
            self.zaehlen(&neu);
        }
        ergebnis
    }
}

/// Eine Laufzeit starten und die MCP-Verbindung aufbauen. Was der Prozess auf
/// stderr schreibt, landet im Protokoll (Aufgabe 17).
///
/// Das Programm wird hier gesucht, nicht beim Zusammensetzen der Laufzeit: so
/// gilt für Start, Anmeldung, Status, Einrichtung und Testen dieselbe Regel
/// (Aufgabe 35, [`crate::tools`]).
pub async fn verbinden(
    id: &EintragId,
    laufzeit: &Laufzeit,
    protokoll: &Arc<Protokoll>,
) -> anyhow::Result<EintragVerbindung> {
    match laufzeit {
        Laufzeit::Stdio {
            befehl,
            argumente,
            umgebung,
        } => stdio(id, befehl, argumente, umgebung, protokoll).await,
        Laufzeit::Remote { url, kopfzeilen } => remote(url, kopfzeilen).await,
    }
}

async fn stdio(
    id: &EintragId,
    befehl: &str,
    argumente: &[String],
    umgebung: &quiver_domain::Werte,
    protokoll: &Arc<Protokoll>,
) -> anyhow::Result<EintragVerbindung> {
    let name = id.als_str().to_string();
    let fund = crate::tools::fuer_eintrag(id, befehl, protokoll)?;
    let mut kommando = tokio::process::Command::new(&fund.pfad);
    kommando
        .args(argumente)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // Ein Werkzeug ist ein Hintergrundprozess: kein eigenes Konsolenfenster
    // (Windows).
    crate::hilfen::ohne_fenster(kommando.as_std_mut());
    umgebung_setzen(&mut kommando, umgebung);

    // Die Meldung nennt nur den Programmnamen; den Pfad kennt das Protokoll.
    let mut kind = kommando
        .spawn()
        .with_context(|| format!("{} lässt sich nicht starten", fund.name()))?;
    let stdin = kind.stdin.take().context("stdin des Kindprozesses")?;
    let stdout = kind.stdout.take().context("stdout des Kindprozesses")?;
    if let Some(stderr) = kind.stderr.take() {
        stderr_ins_log(name, stderr, protokoll.clone());
    }

    let wache = Arc::new(GroessenWache::default());
    let leser = ZeilenWache::neu(stdout, wache.clone());
    let transport =
        AsyncRwTransport::new_client(leser, stdin).with_max_line_length(NACHRICHT_BYTES);
    aufbauen(().serve(transport), Some(kind), wache).await
}

async fn remote(url: &str, kopfzeilen: &quiver_domain::Werte) -> anyhow::Result<EintragVerbindung> {
    let mut kopf = std::collections::HashMap::new();
    for (name, wert) in kopfzeilen {
        let feld = axum::http::HeaderName::from_bytes(name.as_bytes())
            .with_context(|| format!("Kopfzeile {name} ist ungültig"))?;
        let inhalt = axum::http::HeaderValue::from_str(wert)
            .with_context(|| format!("Wert der Kopfzeile {name} ist ungültig"))?;
        kopf.insert(feld, inhalt);
    }
    let config =
        StreamableHttpClientTransportConfig::with_uri(url.to_string()).custom_headers(kopf);
    let transport = StreamableHttpClientTransport::from_config(config);
    aufbauen(
        ().serve(transport),
        None,
        Arc::new(GroessenWache::default()),
    )
    .await
}

/// Der gemeinsame Teil: Verbindung aufbauen und die Aufsicht starten.
async fn aufbauen(
    aufbau: impl Future<Output = Result<RunningService<RoleClient, ()>, ClientInitializeError>>,
    kind: Option<tokio::process::Child>,
    wache: Arc<GroessenWache>,
) -> anyhow::Result<EintragVerbindung> {
    let lauf = aufbau.await.context("MCP-Verbindung zum Eintrag")?;
    let peer = lauf.peer().clone();
    let abbruch = lauf.cancellation_token();
    let (sender, ende) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let _ = lauf.waiting().await;
        let _ = sender.send(());
    });
    Ok(EintragVerbindung {
        peer,
        ende,
        abbruch: Mutex::new(Some(abbruch)),
        kind: Mutex::new(kind),
        wache,
    })
}

/// Einen Werkzeugaufruf beim Eintrag: mit Frist, Verlängerung durch
/// Fortschrittsmeldungen und Abbruch durch den Agenten.
///
/// `frist` ist die Zeit **ohne** ein Lebenszeichen, `gesamt` die harte Grenze.
/// `abbruch` wird wahr, wenn der Agent den Aufruf abbricht (oder die Sitzung
/// endet) – dann bekommt das Tool eine `notifications/cancelled` und der
/// Aufruf endet mit einem Fehler.
pub async fn aufruf<F>(
    peer: &Peer<RoleClient>,
    mut params: CallToolRequestParams,
    frist: Duration,
    gesamt: Duration,
    abbruch: F,
) -> anyhow::Result<CallToolResult>
where
    F: Future<Output = ()>,
{
    // Der Fortschritt verlängert die Frist: ein Tool, das alle 30 s meldet,
    // darf beliebig lange arbeiten – aber nie über `gesamt`.
    params.meta = None;
    let anfrage = ClientRequest::CallToolRequest(CallToolRequest::new(params));
    let mut optionen = PeerRequestOptions::no_options();
    optionen.timeout = Some(frist);
    optionen.reset_timeout_on_progress = true;
    optionen.max_total_timeout = Some(gesamt);
    let griff = peer.send_cancellable_request(anfrage, optionen).await?;
    let id = griff.id.clone();
    let peer_klon = peer.clone();
    tokio::pin!(abbruch);
    let ergebnis = tokio::select! {
        _ = &mut abbruch => {
            let abbruch_meldung = CancelledNotification::new(CancelledNotificationParam::new(
                Some(id),
                Some("Der Agent hat den Aufruf abgebrochen".to_string()),
            ));
            let _ = peer_klon
                .send_notification(ClientNotification::CancelledNotification(abbruch_meldung))
                .await;
            anyhow::bail!("Der Aufruf wurde abgebrochen")
        }
        ergebnis = griff.await_response() => ergebnis?,
    };
    match ergebnis {
        ServerResult::CallToolResult(ergebnis) => Ok(ergebnis),
        anderes => anyhow::bail!("Der Eintrag antwortet mit einer anderen Form: {anderes:?}"),
    }
}

/// Was der Kindprozess auf stderr schreibt, gehört ins Protokoll (Aufgabe 17) –
/// je Eintrag, ohne ANSI-Farbcodes und maskiert. Zusätzlich steht es im Log.
///
/// Gelesen wird **zeilenweise mit Puffer**: eine Zeile über
/// [`ZEILE_PUFFER_BYTES`] wird abgeschnitten (der Rest bis zum Zeilenende
/// verworfen und gezählt), damit ein Prozess, der nie ein Zeilenende schreibt,
/// den Speicher nicht füllt.
fn stderr_ins_log(name: String, stderr: tokio::process::ChildStderr, protokoll: Arc<Protokoll>) {
    let eintrag = EintragId::neu(name.clone());
    tokio::spawn(async move {
        let mut leser = BufReader::new(stderr);
        let mut zeile = Vec::new();
        loop {
            let gekuerzt = match zeile_begrenzt(&mut leser, &mut zeile).await {
                Ok(Some(gekuerzt)) => gekuerzt,
                Ok(None) | Err(_) => return,
            };
            let text = String::from_utf8_lossy(&zeile);
            let text = text.trim_end_matches(['\n', '\r']).to_string();
            tracing::info!(eintrag = %name, "{text}");
            protokoll.zeile_schreiben(&eintrag, &text);
            if gekuerzt {
                protokoll.zeile_verworfen(&eintrag);
            }
        }
    });
}

/// Eine Zeile lesen und dabei **nie** mehr als [`ZEILE_PUFFER_BYTES`] puffern.
///
/// `read_until` würde erst am Zeilenende zurückkehren – eine Zeile ohne Ende
/// füllte den Speicher, bevor irgendetwas sie begrenzen könnte. Hier wird in
/// Stücken gelesen; was über den Puffer hinausgeht, wird verworfen (bis zum
/// Zeilenende) und als „gekürzt“ gemeldet.
pub(crate) async fn zeile_begrenzt<R: AsyncBufRead + Unpin>(
    leser: &mut R,
    puffer: &mut Vec<u8>,
) -> std::io::Result<Option<bool>> {
    puffer.clear();
    let mut gekuerzt = false;
    loop {
        let verfuegbar = leser.fill_buf().await?;
        if verfuegbar.is_empty() {
            return Ok((!puffer.is_empty()).then_some(gekuerzt));
        }
        let zeilenende = verfuegbar.iter().position(|byte| *byte == b'\n');
        let nehmen = zeilenende.map_or(verfuegbar.len(), |stelle| stelle + 1);
        if !gekuerzt {
            let platz = ZEILE_PUFFER_BYTES.saturating_sub(puffer.len());
            let kopieren = nehmen.min(platz);
            puffer.extend_from_slice(&verfuegbar[..kopieren]);
            if kopieren < nehmen {
                gekuerzt = true;
            }
        }
        leser.consume(nehmen);
        if zeilenende.is_some() {
            return Ok(Some(gekuerzt));
        }
    }
}

/// So groß ist der Puffer einer Prozesszeile.
pub const ZEILE_PUFFER_BYTES: usize = 8 * 1024;

/// Die Werkzeuge eines Eintrags prüfen und begrenzen (Paket 3).
///
/// - Name: 1–[`WERKZEUG_NAME_MAX`] Zeichen aus `[A-Za-z0-9_.-]`. Werkzeuge mit
///   anderem Namen werden verworfen – ein Agent kann sie sonst nicht aufrufen,
///   und die Anzeige würde sie umsortieren.
/// - Beschreibung: [`WERKZEUG_BESCHREIBUNG_MAX`] Zeichen.
/// - Schema: [`WERKZEUG_SCHEMA_BYTES`] Byte; größere Werkzeuge fallen weg.
/// - Steuer- und Richtungszeichen fliegen aus allen Texten.
/// - Höchstens [`WERKZEUGE_MAX`]: die weiteren werden verworfen.
pub fn werkzeuge_pruefen(roh: Vec<Tool>) -> Vec<Tool> {
    let mut geprueft = Vec::with_capacity(roh.len().min(WERKZEUGE_MAX));
    let mut verworfen = 0usize;
    for mut tool in roh {
        if geprueft.len() >= WERKZEUGE_MAX {
            verworfen += 1;
            continue;
        }
        let name = crate::protokoll::anzeige_zeichen(&tool.name);
        if name.is_empty()
            || name.chars().count() > WERKZEUG_NAME_MAX
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            verworfen += 1;
            continue;
        }
        let schema_bytes = serde_json::to_vec(&*tool.input_schema)
            .map(|bytes| bytes.len())
            .unwrap_or(usize::MAX);
        if schema_bytes > WERKZEUG_SCHEMA_BYTES {
            verworfen += 1;
            continue;
        }
        tool.name = name.into();
        tool.description = tool.description.map(|text| {
            std::borrow::Cow::Owned(crate::protokoll::kuerzen(
                &crate::protokoll::anzeige_zeichen(&text),
                WERKZEUG_BESCHREIBUNG_MAX,
            ))
        });
        tool.title = tool
            .title
            .map(|text| crate::protokoll::anzeige_zeichen(&text));
        geprueft.push(tool);
    }
    if verworfen > 0 {
        tracing::warn!(
            "{} Werkzeuge verworfen (Name, Schema oder Anzahl)",
            verworfen
        );
    }
    geprueft
}

/// Der Name, unter dem ein Werkzeug eines Eintrags beim Agenten erscheint –
/// nur, wenn der Eintrag eine brauchbare Kennung hat.
pub fn werkzeug_name(eintrag: &EintragId, werkzeug: &str) -> Option<String> {
    if !quiver_domain::kennung_gueltig(eintrag.als_str()) {
        return None;
    }
    Some(quiver_domain::tool_name(eintrag, werkzeug))
}

/// Die Zuordnung Werkzeugname → (Eintrag, Werkzeug), die der Endpunkt benutzt.
/// Eine **Tabelle** statt einer Zerlegung am `__`: nur so kann ein Werkzeug,
/// dessen Name selbst `__` enthält, nicht zu einem anderen Eintrag führen.
pub fn werkzeugtabelle(
    eintraege: &[(EintragId, Vec<String>)],
) -> BTreeMap<String, (EintragId, String)> {
    let mut tabelle: BTreeMap<String, (EintragId, String)> = BTreeMap::new();
    for (id, werkzeuge) in eintraege {
        for werkzeug in werkzeuge {
            let Some(name) = werkzeug_name(id, werkzeug) else {
                continue;
            };
            if let Some(vorher) = tabelle.get(&name) {
                tracing::warn!(
                    "Werkzeug {name} doppelt ({} und {}) – das zweite wird verworfen",
                    vorher.0,
                    id
                );
                continue;
            }
            tabelle.insert(name, (id.clone(), werkzeug.clone()));
        }
    }
    tabelle
}

/// Wie lange ein Aufbau höchstens dauern darf, bevor der Eintrag als fehlerhaft gilt.
pub const AUFBAU_TIMEOUT: Duration = Duration::from_secs(20);

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unbekannter_befehl_ist_ein_fehler() {
        let laufzeit = Laufzeit::Stdio {
            befehl: "gibt-es-garantiert-nicht-quiver".into(),
            argumente: Vec::new(),
            umgebung: quiver_domain::Werte::new(),
        };
        let ordner = std::env::temp_dir().join(format!(
            "quiver-laufzeit-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|dauer| dauer.as_nanos())
                .unwrap_or(0)
        ));
        let protokoll = Arc::new(Protokoll::neu(&ordner));
        let ergebnis = verbinden(&EintragId::neu("test"), &laufzeit, &protokoll).await;
        assert!(ergebnis.is_err(), "Start muss scheitern");
        // Die Meldung nennt den Namen, nicht den Ort des Programms.
        let text = format!("{:#}", ergebnis.err().expect("Fehler"));
        assert!(text.contains("gibt-es-garantiert-nicht-quiver"), "{text}");
        std::fs::remove_dir_all(&ordner).ok();
    }

    fn werkzeug(name: &str, beschreibung: &str) -> Tool {
        Tool::new(
            name.to_string(),
            beschreibung.to_string(),
            serde_json::Map::new(),
        )
    }

    #[test]
    fn die_umgebung_ist_eine_whitelist() {
        assert!(umgebung_erlaubt("PATH"));
        assert!(umgebung_erlaubt("HOME"));
        assert!(umgebung_erlaubt("LC_ALL"));
        assert!(umgebung_erlaubt("XDG_RUNTIME_DIR"));
        assert!(umgebung_erlaubt("HTTPS_PROXY"));
        assert!(umgebung_erlaubt("SystemRoot"));
        // Nicht dabei: was der User sonst in der Shell hat.
        assert!(umgebung_erlaubt_auf("Path", true));
        assert!(umgebung_erlaubt_auf("systemroot", true));
        assert!(!umgebung_erlaubt_auf("Path", false));
        assert!(!umgebung_erlaubt("SSH_AUTH_SOCK"));
        assert!(!umgebung_erlaubt("AWS_SECRET_ACCESS_KEY"));
        assert!(!umgebung_erlaubt("GITHUB_TOKEN"));
        assert!(!umgebung_erlaubt("OPENAI_API_KEY"));
        assert!(!umgebung_erlaubt("SSH_ASKPASS"));
    }

    #[test]
    fn werkzeuge_werden_geprueft_und_begrenzt() {
        let gueltig = werkzeug("echo", "Gibt Text zurück");
        let falscher_name = werkzeug("böses werkzeug", "mit Leerzeichen");
        let steuerzeichen = werkzeug("tab\ttool", "Beschreibung\u{202e}gedreht");
        let riesig = werkzeug("riesig", &"x".repeat(WERKZEUG_BESCHREIBUNG_MAX + 100));
        let geprueft = werkzeuge_pruefen(vec![gueltig, falscher_name, steuerzeichen, riesig]);
        let namen: Vec<&str> = geprueft.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(namen, vec!["echo", "tabtool", "riesig"], "geprüfte Namen");
        let beschreibung = geprueft[2].description.clone().unwrap_or_default();
        assert_eq!(
            beschreibung.chars().count(),
            WERKZEUG_BESCHREIBUNG_MAX,
            "Beschreibung gekürzt"
        );
    }

    #[test]
    fn mehr_als_500_werkzeuge_werden_verworfen() {
        let roh: Vec<Tool> = (0..WERKZEUGE_MAX + 20)
            .map(|i| werkzeug(&format!("w{i}"), "x"))
            .collect();
        assert_eq!(werkzeuge_pruefen(roh).len(), WERKZEUGE_MAX);
    }

    #[test]
    fn ein_zu_grosses_schema_faellt_weg() {
        let mut gross = serde_json::Map::new();
        gross.insert(
            "eigenschaft".to_string(),
            serde_json::Value::String("x".repeat(WERKZEUG_SCHEMA_BYTES + 10)),
        );
        let tool = Tool::new("gross", "zu großes Schema", gross);
        let klein = werkzeug("klein", "passt");
        let geprueft = werkzeuge_pruefen(vec![tool, klein]);
        assert_eq!(geprueft.len(), 1);
        assert_eq!(geprueft[0].name, "klein");
    }

    #[test]
    fn die_zuordnung_ist_eine_tabelle_und_doppelte_fliegen_raus() {
        let tabelle = werkzeugtabelle(&[
            (
                EintragId::neu("echo"),
                vec!["echo".to_string(), "mit__trenner".to_string()],
            ),
            (EintragId::neu("quiver"), vec!["protokoll".to_string()]),
            (EintragId::neu("echo"), vec!["echo".to_string()]),
        ]);
        assert_eq!(
            tabelle
                .get("echo__echo")
                .map(|(id, w)| (id.als_str(), w.as_str())),
            Some(("echo", "echo"))
        );
        assert_eq!(
            tabelle
                .get("echo__mit__trenner")
                .map(|(_, werkzeug)| werkzeug.as_str()),
            Some("mit__trenner"),
            "ein __ im Werkzeugnamen führt nicht zum falschen Eintrag"
        );
        assert!(
            !tabelle
                .keys()
                .any(|name| name.starts_with("quiver__quiver")),
            "quiver ist als Kennung reserviert"
        );
        assert_eq!(tabelle.len(), 2, "der doppelte Name bleibt einmal");
    }

    /// Eine Zeile ohne Ende füllt den Speicher nicht: gelesen wird in Stücken
    /// und nur bis zum Puffer (Paket 3).
    #[tokio::test]
    async fn eine_zeile_ohne_ende_wird_begrenzt() {
        let lang = vec![b'x'; ZEILE_PUFFER_BYTES * 4];
        let mut leser = BufReader::new(std::io::Cursor::new(lang));
        let mut puffer = Vec::new();
        let gekuerzt = zeile_begrenzt(&mut leser, &mut puffer)
            .await
            .expect("lesbar")
            .expect("eine Zeile");
        assert!(gekuerzt, "der Rest wurde verworfen");
        assert_eq!(puffer.len(), ZEILE_PUFFER_BYTES);
    }

    /// Eine Zeile mit Ende kommt vollständig und ungekürzt.
    #[tokio::test]
    async fn eine_kurze_zeile_kommt_ganz() {
        let mut leser = BufReader::new(std::io::Cursor::new(b"hallo\nzweite\n".to_vec()));
        let mut puffer = Vec::new();
        assert_eq!(
            zeile_begrenzt(&mut leser, &mut puffer)
                .await
                .expect("lesbar"),
            Some(false)
        );
        assert_eq!(puffer, b"hallo\n");
        assert_eq!(
            zeile_begrenzt(&mut leser, &mut puffer)
                .await
                .expect("lesbar"),
            Some(false)
        );
        assert_eq!(puffer, b"zweite\n");
        assert_eq!(
            zeile_begrenzt(&mut leser, &mut puffer)
                .await
                .expect("lesbar"),
            None,
            "am Ende kommt keine Zeile mehr"
        );
    }
}
