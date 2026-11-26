// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Konfiguration aus der Umgebung und die wenigen Regeln, die schon beim Start
//! gelten: Basis-URL, Entra, vertraute Proxies, Altgeräte, Uhr.
//!
//! Alles, was hier abgelehnt wird, führt zu einem Startfehler mit klarem Text –
//! nie zu einem Server, der halb konfiguriert läuft.

use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use time::OffsetDateTime;
use url::Url;

use crate::krypto;

/// Die Uhr des Servers. Im Betrieb die Systemzeit, in Tests einstellbar.
#[derive(Clone, Default)]
pub struct Uhr(Arc<Mutex<Option<OffsetDateTime>>>);

impl std::fmt::Debug for Uhr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Uhr")
    }
}

impl Uhr {
    pub fn jetzt(&self) -> OffsetDateTime {
        let fest = crate::sperre(&self.0);
        fest.unwrap_or_else(OffsetDateTime::now_utc)
    }

    /// Die Uhr auf einen festen Zeitpunkt stellen (nur für Tests gedacht).
    pub fn setze(&self, zeit: OffsetDateTime) {
        *crate::sperre(&self.0) = Some(zeit);
    }

    /// Wieder die Systemzeit nehmen.
    pub fn zuruecksetzen(&self) {
        *crate::sperre(&self.0) = None;
    }
}

/// Die Laufzeit des Servers seit dem Start – **nicht** die Wanduhr.
///
/// Das Zeitfenster der Ersteinrichtung (15 Minuten, Aufgabe 25) hängt daran:
/// Wer die Systemzeit verstellt, verlängert es nicht. In Tests lässt es sich
/// mit [`Laufzeit::vorspulen`] stellen, damit das Ablaufen prüfbar ist.
#[derive(Clone)]
pub struct Laufzeit {
    start: Arc<Instant>,
    versatz: Arc<Mutex<Duration>>,
}

impl std::fmt::Debug for Laufzeit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Laufzeit")
            .field("seit_start_ms", &self.seit_start().as_millis())
            .finish()
    }
}

impl Default for Laufzeit {
    fn default() -> Self {
        Self::neu()
    }
}

impl Laufzeit {
    /// Der Start ist jetzt (der Server baut seine Konfiguration beim Start).
    pub fn neu() -> Self {
        Self {
            start: Arc::new(Instant::now()),
            versatz: Arc::new(Mutex::new(Duration::ZERO)),
        }
    }

    /// Wie lange der Server schon läuft.
    pub fn seit_start(&self) -> Duration {
        self.start.elapsed() + *crate::sperre(&self.versatz)
    }

    /// Nur für Tests: die Laufzeit um `dauer` weiterstellen.
    pub fn vorspulen(&self, dauer: Duration) {
        *crate::sperre(&self.versatz) += dauer;
    }
}

/// Womit sich der Server bei Entra ausweist (genau eins von beidem).
#[derive(Clone)]
pub enum Ausweis {
    /// Client-Geheimnis (`client_secret`). Wird nie geloggt und nie ausgeliefert.
    Geheimnis(String),
    /// Privater Schlüssel mit Zertifikat (`private_key_jwt`, Aufgabe 23).
    Schluessel(Arc<crate::entra_schluessel::SchluesselHalter>),
}

impl std::fmt::Debug for Ausweis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Weder Geheimnis noch Schlüssel gehören in ein Debug-Protokoll.
        match self {
            Ausweis::Geheimnis(_) => f.write_str("Geheimnis(<verborgen>)"),
            Ausweis::Schluessel(halter) => write!(f, "Schluessel({:?})", halter.pfad()),
        }
    }
}

impl Ausweis {
    /// Nur für Tests und Aufrufer, die ein Geheimnis im Speicher haben.
    pub fn geheimnis(wert: impl Into<String>) -> Self {
        Self::Geheimnis(wert.into())
    }

    pub fn ist_schluessel(&self) -> bool {
        matches!(self, Ausweis::Schluessel(_))
    }
}

/// Der Host, bei dem Gruppennamen geholt werden (Aufgabe 30).
pub const GRAPH_BASIS: &str = "https://graph.microsoft.com";
/// Der eine Lesebereich für Gruppennamen; nur mit `QUIVER_ENTRA_GRUPPENNAMEN=an`.
pub const GRAPH_SCOPE: &str = "https://graph.microsoft.com/GroupMember.Read.All";

/// Entra-Konfiguration. Fehlt sie ganz, gilt der alte Weg mit Einmal-Codes.
#[derive(Clone)]
pub struct EntraKonfig {
    /// Mandant (GUID, nie `common` oder `organizations`).
    pub tenant: String,
    pub client_id: String,
    /// Geheimnis **oder** Schlüssel, nie beides.
    pub ausweis: Ausweis,
    /// Authority ohne abschließenden Schrägstrich (Loopback nur im Test).
    pub authority: String,
    /// Der erwartete `iss`; muss exakt der Authority entsprechen.
    pub issuer: String,
    /// `Authorization Code` + PKCE; die Rückkehr landet hier.
    pub redirect_uri: String,
    /// Gruppennamen über Graph holen (Aufgabe 30)? Standard: nein.
    pub gruppen_namen: bool,
    /// Basis für Graph; im Betrieb fest, in Tests Loopback.
    pub graph_basis: String,
}

impl EntraKonfig {
    /// Eine Konfiguration mit Client-Geheimnis (Tests, kleine Installationen).
    pub fn mit_geheimnis(
        tenant: impl Into<String>,
        client_id: impl Into<String>,
        authority: impl Into<String>,
        redirect_uri: impl Into<String>,
        geheimnis: impl Into<String>,
    ) -> Self {
        let authority = authority.into();
        Self {
            tenant: tenant.into(),
            client_id: client_id.into(),
            ausweis: Ausweis::Geheimnis(geheimnis.into()),
            issuer: authority.clone(),
            authority,
            redirect_uri: redirect_uri.into(),
            gruppen_namen: false,
            graph_basis: String::from(GRAPH_BASIS),
        }
    }

    /// Die Bereiche für Anmeldung **und** Refresh (Aufgabe 30).
    ///
    /// `GroupMember.Read.All` kommt nur dazu, wenn der Betreiber die Namen
    /// einschaltet: ohne Zustimmung des Mandanten scheiterte sonst jede
    /// Anmeldung.
    pub fn scopes(&self) -> String {
        let mut scopes = String::from("openid profile email offline_access");
        if self.gruppen_namen {
            scopes.push(' ');
            scopes.push_str(GRAPH_SCOPE);
        }
        scopes
    }
}

impl std::fmt::Debug for EntraKonfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Geheimnis und Schlüssel gehören nicht in ein Debug-Protokoll.
        f.debug_struct("EntraKonfig")
            .field("tenant", &self.tenant)
            .field("client_id", &self.client_id)
            .field("authority", &self.authority)
            .field("issuer", &self.issuer)
            .field("redirect_uri", &self.redirect_uri)
            .field("ausweis", &self.ausweis)
            .field("gruppen_namen", &self.gruppen_namen)
            .field("graph_basis", &self.graph_basis)
            .finish()
    }
}

/// Woher der Server die Veröffentlichungen erfährt (Aufgabe 34).
///
/// **Keine Abfrage ohne Wissen des Betreibers:** [`UpdateKonfig::an`] ist der
/// Schalter `QUIVER_UPDATES=an|aus` (Standard an). Gefragt wird höchstens alle
/// sechs Stunden, mit Streuung, ohne Kennung und ohne Nutzdaten. Der Client
/// fragt **nur diesen Server**, nie das Internet.
#[derive(Clone)]
pub struct UpdateKonfig {
    /// Ist die Abfrage der Veröffentlichungen eingeschaltet?
    pub an: bool,
    /// Die Basis der Veröffentlichungen, z. B.
    /// `https://github.com/patimweb-labs/mcp-quiver`. In Tests Loopback.
    pub quelle: String,
    /// Abstand zwischen zwei Abfragen.
    pub abstand: Duration,
    /// Zusätzliche Streuung (damit viele Server nicht im Takt fragen).
    pub streuung: Duration,
}

impl std::fmt::Debug for UpdateKonfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UpdateKonfig")
            .field("an", &self.an)
            .field("quelle", &self.quelle)
            .field("abstand", &self.abstand)
            .field("streuung", &self.streuung)
            .finish()
    }
}

impl Default for UpdateKonfig {
    fn default() -> Self {
        Self {
            an: true,
            quelle: String::from(UPDATE_QUELLE),
            abstand: UPDATE_ABSTAND,
            streuung: UPDATE_STREUUNG,
        }
    }
}

/// Wo die Veröffentlichungen liegen (GitHub).
pub const UPDATE_QUELLE: &str = "https://github.com/patimweb-labs/mcp-quiver";
/// Höchstens alle sechs Stunden fragen.
pub const UPDATE_ABSTAND: Duration = Duration::from_secs(6 * 60 * 60);
/// Bis zu 30 Minuten Streuung dazu.
pub const UPDATE_STREUUNG: Duration = Duration::from_secs(30 * 60);

/// Woher der Server seine Daten nimmt.
#[derive(Debug, Clone)]
pub struct Konfig {
    pub daten: PathBuf,
    /// Gebaute Admin-Oberfläche; fehlt sie, gibt es nur die API.
    pub admin_dist: Option<PathBuf>,
    /// Adresse, auf der der Server lauscht (`QUIVER_ADRESSE`).
    pub adresse: SocketAddr,
    /// Basis-URL, unter der der Server erreichbar ist (ohne Schrägstrich).
    pub oeffentlich: String,
    /// `None` heißt: Einmal-Codes statt Entra (MVP ohne Azure).
    pub entra: Option<EntraKonfig>,
    /// Nur von diesen Netzen wird `X-Forwarded-For` geglaubt.
    pub proxies: Vec<Netz>,
    /// Dürfen Geräte ohne Entra-Benutzer weiterlaufen?
    pub altgeraete_erlaubt: bool,
    /// `QUIVER_EINRICHTUNG=ohne-code`: die Ersteinrichtung kommt ohne Code aus.
    /// Nur zusammen mit einer Loopback-Basis-URL erlaubt.
    pub einrichtung_ohne_code: bool,
    pub uhr: Uhr,
    /// Die Laufzeit seit dem Start (Fenster der Ersteinrichtung).
    pub laufzeit: Laufzeit,
    /// So lange darf ein Client an einem Rumpf senden (`RUMPF_ZEIT`).
    ///
    /// Steht hier und nicht als fester Wert in der Schicht, damit ein Test ihn
    /// kurz stellen kann – 30 Sekunden wären im Test eine Ewigkeit.
    pub rumpf_zeit: Duration,
    /// So viele Anmeldevorgänge dürfen gleichzeitig offen sein
    /// ([`crate::anmeldung::OFFENE_VORGAENGE_MAX`]). Im Test kleiner.
    pub offene_vorgaenge_max: usize,
    /// Update-Prüfung (Aufgabe 34).
    pub updates: UpdateKonfig,
}

impl Konfig {
    /// Eine Konfiguration für Tests und den alten Weg ohne Entra.
    pub fn neu(daten: impl Into<PathBuf>) -> Self {
        Self {
            daten: daten.into(),
            admin_dist: None,
            // Die Vorgabe von `Konfig::neu` ist Loopback: Tests und die
            // Entwicklung laufen so, ohne dass ein Code nötig wäre.
            adresse: SocketAddr::from(([127, 0, 0, 1], 8740)),
            oeffentlich: String::from("http://127.0.0.1:8740"),
            entra: None,
            proxies: Vec::new(),
            altgeraete_erlaubt: true,
            einrichtung_ohne_code: false,
            uhr: Uhr::default(),
            laufzeit: Laufzeit::neu(),
            rumpf_zeit: crate::RUMPF_ZEIT,
            offene_vorgaenge_max: crate::anmeldung::OFFENE_VORGAENGE_MAX,
            updates: UpdateKonfig::default(),
        }
    }

    /// Lauscht der Server ausschließlich auf Loopback?
    pub fn lauscht_nur_loopback(&self) -> bool {
        self.adresse.ip().is_loopback()
    }

    /// Braucht die Ersteinrichtung den Einmal-Code aus dem Log?
    ///
    /// Ja, sobald der Server **nicht nur auf Loopback** lauscht – unabhängig
    /// von der Basis-URL, also auch im Container. Auch eine öffentliche
    /// Basis-URL bei Loopback-Bindung verlangt den Code (davor steht dann ein
    /// Proxy). Ausnahme ist nur die ausdrückliche Angabe
    /// `QUIVER_EINRICHTUNG=ohne-code` **und** eine Loopback-Basis-URL.
    pub fn einrichtung_mit_code(&self) -> bool {
        let ausnahme = self.einrichtung_ohne_code && ist_loopback_url(&self.oeffentlich);
        if ausnahme {
            return false;
        }
        !self.lauscht_nur_loopback() || !ist_loopback_url(&self.oeffentlich)
    }

    /// Alles aus der Umgebung lesen und prüfen.
    ///
    /// Ohne `QUIVER_OEFFENTLICH` gilt `http://localhost:<Port>` (Aufgabe 26,
    /// Punkt 9). Ob die Ersteinrichtung einen Code braucht, hängt an der
    /// Lauschadresse und nicht an der Basis-URL (Härtung, Punkt 2).
    pub fn aus_umgebung(daten: PathBuf) -> anyhow::Result<Self> {
        Self::aus_umgebung_mit_adresse(daten, SocketAddr::from(([127, 0, 0, 1], 8740)))
    }

    /// Wie [`Konfig::aus_umgebung`], mit der Lauschadresse aus `QUIVER_ADRESSE`.
    pub fn aus_umgebung_mit_adresse(daten: PathBuf, adresse: SocketAddr) -> anyhow::Result<Self> {
        let port = adresse.port();
        let roh = std::env::var("QUIVER_OEFFENTLICH").ok();
        if roh.as_deref().is_none_or(|wert| wert.trim().is_empty()) {
            let vorgabe = oeffentlich_aus(None, port)?;
            if adresse.ip().is_loopback() {
                tracing::info!(
                    "QUIVER_OEFFENTLICH fehlt; es gilt {vorgabe} (nur lokal erreichbar, \
                     der Server lauscht nur auf Loopback)"
                );
            } else {
                tracing::info!(
                    "QUIVER_OEFFENTLICH fehlt; es gilt {vorgabe} – der Server lauscht aber auf \
                     {adresse} und ist damit auch von außen erreichbar"
                );
            }
        }
        let oeffentlich = oeffentlich_aus(roh.as_deref(), port)?;

        let tenant = std::env::var("QUIVER_ENTRA_TENANT").unwrap_or_default();
        let client_id = std::env::var("QUIVER_ENTRA_CLIENT_ID").unwrap_or_default();
        let secret_pfad = std::env::var_os("QUIVER_ENTRA_SECRET_DATEI")
            .map(PathBuf::from)
            .unwrap_or_else(|| daten.join("entra-secret"));
        let schluessel_pfad = crate::entra_schluessel::pfad_aus_umgebung(&daten);
        let secret_gesetzt = std::env::var_os("QUIVER_ENTRA_SECRET_DATEI").is_some();
        let schluessel_gesetzt = std::env::var_os("QUIVER_ENTRA_SCHLUESSEL_DATEI").is_some();
        let secret_da = secret_pfad.exists();
        let schluessel_da = schluessel_pfad.exists();

        // Genau eins von beidem: Geheimnis oder Schlüssel (Aufgabe 23).
        if (secret_da || secret_gesetzt) && (schluessel_da || schluessel_gesetzt) {
            anyhow::bail!(
                "Entra hat Geheimnis ({}) und Schlüssel ({}); es gilt genau eins von beidem. \
                 Bitte eine der beiden Angaben entfernen.",
                secret_pfad.display(),
                schluessel_pfad.display()
            );
        }
        let geheimnis_weg = secret_da || secret_gesetzt;
        let schluessel_weg = schluessel_da || schluessel_gesetzt;
        let tenant_da = !tenant.trim().is_empty();
        let client_da = !client_id.trim().is_empty();
        let ausweis_da = geheimnis_weg || schluessel_weg;
        let etwas_gesetzt = tenant_da || client_da || ausweis_da;
        let vollstaendig = tenant_da && client_da && ausweis_da;
        if etwas_gesetzt && !vollstaendig {
            anyhow::bail!(
                "Entra ist nur teilweise gesetzt: QUIVER_ENTRA_TENANT und \
                 QUIVER_ENTRA_CLIENT_ID gehören mit genau einem Ausweis zusammen – \
                 QUIVER_ENTRA_SECRET_DATEI (Geheimnis) oder \
                 QUIVER_ENTRA_SCHLUESSEL_DATEI (Zertifikat); alle oder keine."
            );
        }

        // Auch ohne Entra gelesen: ein falscher Wert ist immer ein Startfehler.
        let gruppen_namen =
            gruppen_namen_aus(std::env::var("QUIVER_ENTRA_GRUPPENNAMEN").ok().as_deref())?;
        let graph_basis = graph_basis_aus(std::env::var("QUIVER_ENTRA_GRAPH").ok().as_deref())?;

        let entra = if vollstaendig {
            let tenant = tenant.trim().to_string();
            if !ist_guid(&tenant) {
                anyhow::bail!("QUIVER_ENTRA_TENANT ist keine GUID (und nie common/organizations)");
            }
            let ausweis = if geheimnis_weg {
                let secret = std::fs::read_to_string(&secret_pfad)
                    .map_err(|fehler| {
                        anyhow::anyhow!(
                            "Entra-Geheimnis {} nicht lesbar: {fehler}",
                            secret_pfad.display()
                        )
                    })?
                    .trim()
                    .to_string();
                if secret.is_empty() {
                    anyhow::bail!("Entra-Geheimnis in {} ist leer", secret_pfad.display());
                }
                if krypto::rechte_zu_offen(&secret_pfad) {
                    tracing::warn!(
                        "{} ist für Gruppe oder alle lesbar; bitte auf 0600 setzen",
                        secret_pfad.display()
                    );
                }
                Ausweis::Geheimnis(secret)
            } else {
                if krypto::rechte_zu_offen(&schluessel_pfad) {
                    tracing::warn!(
                        "{} ist für Gruppe oder alle lesbar; bitte auf 0600 setzen",
                        schluessel_pfad.display()
                    );
                }
                let halter = crate::entra_schluessel::SchluesselHalter::laden(schluessel_pfad)
                    .map_err(|fehler| {
                        anyhow::anyhow!("Entra-Schlüssel nicht brauchbar: {fehler:#}")
                    })?;
                Ausweis::Schluessel(Arc::new(halter))
            };
            let authority = match std::env::var("QUIVER_ENTRA_AUTHORITY") {
                Ok(wert) if !wert.trim().is_empty() => {
                    let wert = basis_url_pruefen(&wert)?;
                    if !ist_loopback_url(&wert) {
                        anyhow::bail!(
                            "QUIVER_ENTRA_AUTHORITY ist nur für Tests auf Loopback erlaubt"
                        );
                    }
                    wert
                }
                _ => format!("https://login.microsoftonline.com/{tenant}/v2.0"),
            };
            Some(EntraKonfig {
                tenant,
                client_id: client_id.trim().to_string(),
                ausweis,
                issuer: authority.clone(),
                authority,
                redirect_uri: format!("{oeffentlich}/auth/callback"),
                gruppen_namen,
                graph_basis,
            })
        } else {
            // Der Schalter ohne Entra wäre ein stiller Fehlschlag.
            if gruppen_namen {
                anyhow::bail!(
                    "QUIVER_ENTRA_GRUPPENNAMEN=an braucht Entra (Tenant, Client-Id und Ausweis); \
                     ohne Entra gibt es keine Gruppennamen zu holen."
                );
            }
            None
        };

        let proxies = match std::env::var("QUIVER_PROXIES") {
            Ok(text) if !text.trim().is_empty() => text
                .split(',')
                .map(str::trim)
                .filter(|teil| !teil.is_empty())
                .map(Netz::lesen)
                .collect::<anyhow::Result<Vec<_>>>()?,
            _ => Vec::new(),
        };

        // Mit eingerichtetem Entra sind Geräte ohne Benutzer (aus der Zeit der
        // Einmal-Codes) standardmäßig gesperrt – es gibt heute keine mehr im
        // Einsatz. Ohne Entra sind **alle** Geräte solche Geräte; dort bleibt
        // der alte Weg offen. Eine ausdrückliche Angabe gewinnt immer.
        let altgeraete_erlaubt = altgeraete_aus(
            std::env::var("QUIVER_ALTGERAETE").ok().as_deref(),
            entra.is_some(),
        )?;

        let einrichtung_ohne_code = einrichtung_aus(
            std::env::var("QUIVER_EINRICHTUNG").ok().as_deref(),
            &oeffentlich,
            adresse,
        )?;

        let updates = UpdateKonfig {
            an: updates_an_aus(std::env::var("QUIVER_UPDATES").ok().as_deref())?,
            ..UpdateKonfig::default()
        };
        let updates = UpdateKonfig {
            quelle: update_quelle_aus(std::env::var("QUIVER_UPDATE_QUELLE").ok().as_deref())?,
            ..updates
        };

        Ok(Self {
            daten,
            admin_dist: None,
            adresse,
            oeffentlich,
            entra,
            proxies,
            altgeraete_erlaubt,
            einrichtung_ohne_code,
            uhr: Uhr::default(),
            laufzeit: Laufzeit::neu(),
            rumpf_zeit: crate::RUMPF_ZEIT,
            offene_vorgaenge_max: crate::anmeldung::OFFENE_VORGAENGE_MAX,
            updates,
        })
    }

    pub fn mit_admin_dist(mut self, dist: Option<PathBuf>) -> Self {
        self.admin_dist = dist;
        self
    }

    /// Ist die Basis-URL `https` (dann bekommen Cookies `Secure` und `__Host-`)?
    pub fn ist_https(&self) -> bool {
        self.oeffentlich.starts_with("https://")
    }
}

/// Den Schalter `QUIVER_ENTRA_GRUPPENNAMEN` lesen: Standard aus, sonst Startfehler.
pub fn gruppen_namen_aus(wert: Option<&str>) -> anyhow::Result<bool> {
    match wert.map(str::trim).filter(|wert| !wert.is_empty()) {
        None => Ok(false),
        Some(wert) => match wert.to_ascii_lowercase().as_str() {
            "an" => Ok(true),
            "aus" => Ok(false),
            anderes => anyhow::bail!(
                "QUIVER_ENTRA_GRUPPENNAMEN kennt nur an|aus, nicht {anderes}; \
                 ohne Angabe gilt aus"
            ),
        },
    }
}

/// Den Schalter `QUIVER_UPDATES` lesen: Standard an, sonst Startfehler.
pub fn updates_an_aus(wert: Option<&str>) -> anyhow::Result<bool> {
    match wert.map(str::trim).filter(|wert| !wert.is_empty()) {
        None => Ok(true),
        Some(wert) => match wert.to_ascii_lowercase().as_str() {
            "an" => Ok(true),
            "aus" => Ok(false),
            anderes => anyhow::bail!(
                "QUIVER_UPDATES kennt nur an|aus, nicht {anderes}; ohne Angabe gilt an"
            ),
        },
    }
}

/// Die Quelle der Veröffentlichungen: im Betrieb GitHub, in Tests Loopback.
pub fn update_quelle_aus(wert: Option<&str>) -> anyhow::Result<String> {
    let Some(wert) = wert.map(str::trim).filter(|wert| !wert.is_empty()) else {
        return Ok(String::from(UPDATE_QUELLE));
    };
    let basis = basis_url_pruefen(wert)?.trim_end_matches('/').to_string();
    if basis != UPDATE_QUELLE && !ist_loopback_url(&basis) {
        anyhow::bail!(
            "QUIVER_UPDATE_QUELLE ist nur für Tests auf Loopback erlaubt; \
             im Betrieb gilt {UPDATE_QUELLE}"
        );
    }
    Ok(basis)
}

/// Die Basis für Graph: im Betrieb der feste Host, in Tests Loopback.
pub fn graph_basis_aus(wert: Option<&str>) -> anyhow::Result<String> {
    let Some(wert) = wert.map(str::trim).filter(|wert| !wert.is_empty()) else {
        return Ok(String::from(GRAPH_BASIS));
    };
    let basis = basis_url_pruefen(wert)?;
    if basis != GRAPH_BASIS && !ist_loopback_url(&basis) {
        anyhow::bail!(
            "QUIVER_ENTRA_GRAPH ist nur für Tests auf Loopback erlaubt; \
             im Betrieb gilt {GRAPH_BASIS}"
        );
    }
    Ok(basis)
}

/// Die Basis-URL aus der Umgebung: gesetzt und geprüft, sonst
/// `http://localhost:<Port>` (Aufgabe 26, Punkt 9).
pub fn oeffentlich_aus(wert: Option<&str>, port: u16) -> anyhow::Result<String> {
    match wert {
        Some(wert) if !wert.trim().is_empty() => basis_url_pruefen(wert),
        _ => basis_url_pruefen(&format!("http://localhost:{port}")),
    }
}

/// Eine Basis-URL prüfen und ohne abschließenden Schrägstrich zurückgeben.///
/// `https` ist Pflicht; `http` nur auf Loopback, damit Tests und die Entwicklung
/// ohne Zertifikat laufen können.
pub fn basis_url_pruefen(wert: &str) -> anyhow::Result<String> {
    let wert = wert.trim().trim_end_matches('/');
    let url = Url::parse(wert)
        .map_err(|fehler| anyhow::anyhow!("{wert} ist keine gültige URL: {fehler}"))?;
    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("{wert} darf weder Abfrage noch Sprungmarke enthalten");
    }
    if !url.path().is_empty() && url.path() != "/" {
        anyhow::bail!("{wert} darf keinen Pfad enthalten");
    }
    match url.scheme() {
        "https" => {}
        "http" => {
            if !ist_loopback_url(wert) {
                anyhow::bail!(
                    "{wert} ist http, aber nicht Loopback – ohne TLS startet der Server nicht"
                );
            }
        }
        anderes => {
            anyhow::bail!("{wert}: nur https (oder http auf Loopback) ist erlaubt, nicht {anderes}")
        }
    }
    Ok(wert.to_string())
}

/// Loopback: `localhost`, `127.0.0.1`, `[::1]` (auch `127.x.x.x`).
pub fn ist_loopback_url(wert: &str) -> bool {
    let Ok(url) = Url::parse(wert) else {
        return false;
    };
    match url.host() {
        Some(url::Host::Domain(domain)) => domain.eq_ignore_ascii_case("localhost"),
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}

/// Eine GUID in der Form 8-4-4-4-12.
pub fn ist_guid(wert: &str) -> bool {
    let wert = wert.trim();
    let teile: Vec<&str> = wert.split('-').collect();
    [8, 4, 4, 4, 12]
        .iter()
        .zip(&teile)
        .all(|(laenge, teil)| teil.len() == *laenge && teil.chars().all(|c| c.is_ascii_hexdigit()))
        && teile.len() == 5
}

/// Ein Netz in CIDR-Schreibweise, z. B. `10.0.0.0/8` oder `2001:db8::/32`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Netz {
    netz: IpAddr,
    bits: u8,
}

impl Netz {
    pub fn lesen(text: &str) -> anyhow::Result<Self> {
        let (adresse, bits) = text
            .split_once('/')
            .ok_or_else(|| anyhow::anyhow!("{text} ist kein CIDR (Beispiel: 10.0.0.0/8)"))?;
        let netz: IpAddr = adresse
            .parse()
            .map_err(|_| anyhow::anyhow!("{text}: keine gültige Adresse"))?;
        let bits: u8 = bits
            .parse()
            .map_err(|_| anyhow::anyhow!("{text}: keine gültige Bitzahl"))?;
        let max = if netz.is_ipv4() { 32 } else { 128 };
        if bits > max {
            anyhow::bail!("{text}: höchstens {max} Bit");
        }
        Ok(Self { netz, bits })
    }

    pub fn enthaelt(&self, ip: IpAddr) -> bool {
        match (self.netz, ip) {
            (IpAddr::V4(netz), IpAddr::V4(ip)) => {
                let netz = u32::from(netz);
                let ip = u32::from(ip);
                let maske = if self.bits == 0 {
                    0
                } else {
                    u32::MAX << (32 - u32::from(self.bits))
                };
                netz & maske == ip & maske
            }
            (IpAddr::V6(netz), IpAddr::V6(ip)) => {
                let netz = u128::from(netz);
                let ip = u128::from(ip);
                let maske = if self.bits == 0 {
                    0
                } else {
                    u128::MAX << (128 - u32::from(self.bits))
                };
                netz & maske == ip & maske
            }
            _ => false,
        }
    }
}

/// Dürfen Geräte ohne Entra-Benutzer weiterlaufen?
///
/// Mit eingerichtetem Entra ist die Antwort ohne Angabe **nein** (`gesperrt`):
/// solche Geräte stammen aus der Zeit der Einmal-Codes, und es gibt heute keine
/// mehr im Einsatz. Ohne Entra sind alle Geräte solche Geräte, dort bleibt der
/// alte Weg offen. Eine ausdrückliche Angabe gewinnt immer.
pub fn altgeraete_aus(wert: Option<&str>, entra_da: bool) -> anyhow::Result<bool> {
    match wert.map(str::trim).filter(|wert| !wert.is_empty()) {
        None => Ok(!entra_da),
        Some(wert) => match wert.to_ascii_lowercase().as_str() {
            "erlaubt" => Ok(true),
            "gesperrt" => Ok(false),
            anderes => {
                anyhow::bail!("QUIVER_ALTGERAETE kennt nur erlaubt|gesperrt, nicht {anderes}")
            }
        },
    }
}

/// Den Schalter `QUIVER_EINRICHTUNG` lesen.
///
/// `mit-code` (Vorgabe) verlangt den Code, sobald der Server nicht nur auf
/// Loopback lauscht. `ohne-code` ist nur mit Loopback-Basis-URL erlaubt und
/// wird beim Start deutlich gemeldet; alles andere ist ein Startfehler, damit
/// niemand versehentlich ohne Code im Netz steht.
pub fn einrichtung_aus(
    wert: Option<&str>,
    oeffentlich: &str,
    adresse: SocketAddr,
) -> anyhow::Result<bool> {
    match wert.map(str::trim).filter(|wert| !wert.is_empty()) {
        None | Some("mit-code") => Ok(false),
        Some("ohne-code") => {
            if !ist_loopback_url(oeffentlich) {
                anyhow::bail!(
                    "QUIVER_EINRICHTUNG=ohne-code ist nur mit einer Loopback-Basis-URL \
                     erlaubt ({oeffentlich} ist keine); sonst ist der Code Pflicht"
                );
            }
            tracing::warn!(
                "QUIVER_EINRICHTUNG=ohne-code: die Ersteinrichtung braucht keinen Code. \
                 Der Server lauscht auf {adresse}; wer ihn erreicht, kann in den ersten \
                 Minuten den Root anlegen – bitte nur auf 127.0.0.1 veröffentlichen."
            );
            Ok(true)
        }
        Some(anderes) => {
            anyhow::bail!("QUIVER_EINRICHTUNG kennt nur mit-code|ohne-code, nicht {anderes}")
        }
    }
}

/// Die IP des Aufrufers.
///
/// Die Gültigkeit hängt an der Kette, nicht am ersten Eintrag: der Kopf wird
/// **von rechts** gelesen, und genommen wird die erste Adresse, die in
/// **keinem** vertrauten Netz liegt – der entfernteste Absender, den der letzte
/// vertraute Proxy gesehen hat. Adressen davor sind nicht belegt und werden
/// übergangen.
///
/// Alle Kopfzeilen dieses Namens werden in ihrer Reihenfolge zu einer Kette
/// zusammengefasst: ein Proxy darf eine zweite Zeile anhängen, statt die erste
/// zu erweitern (`headers.get` würde nur die erste sehen).
pub fn aufrufer_ip(
    peer: Option<SocketAddr>,
    x_forwarded_for: &[&str],
    proxies: &[Netz],
) -> Option<IpAddr> {
    let peer = peer?;
    if !proxies.iter().any(|netz| netz.enthaelt(peer.ip())) {
        // Der letzte Absender ist kein vertrauter Proxy: seinen Kopf glauben
        // wir nicht.
        return Some(peer.ip());
    }
    let mut kette: Vec<IpAddr> = Vec::new();
    for kopf in x_forwarded_for {
        for teil in kopf.split(',') {
            if let Ok(ip) = teil.trim().parse::<IpAddr>() {
                kette.push(ip);
            }
        }
    }
    kette
        .iter()
        .rev()
        .find(|ip| !proxies.iter().any(|netz| netz.enthaelt(**ip)))
        .copied()
        .or_else(|| kette.first().copied())
        .or(Some(peer.ip()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schalter_fuer_gruppennamen() {
        assert!(!gruppen_namen_aus(None).unwrap());
        assert!(!gruppen_namen_aus(Some("")).unwrap());
        assert!(!gruppen_namen_aus(Some(" aus ")).unwrap());
        assert!(gruppen_namen_aus(Some("an")).unwrap());
        assert!(gruppen_namen_aus(Some("AN")).unwrap());
        for falsch in ["ja", "1", "true", "ein"] {
            assert!(gruppen_namen_aus(Some(falsch)).is_err(), "{falsch}");
        }
    }

    #[test]
    fn graph_basis_nur_fester_host_oder_loopback() {
        assert_eq!(graph_basis_aus(None).unwrap(), GRAPH_BASIS);
        assert_eq!(graph_basis_aus(Some("")).unwrap(), GRAPH_BASIS);
        assert_eq!(graph_basis_aus(Some(GRAPH_BASIS)).unwrap(), GRAPH_BASIS);
        assert_eq!(
            graph_basis_aus(Some("http://127.0.0.1:5123")).unwrap(),
            "http://127.0.0.1:5123"
        );
        assert!(graph_basis_aus(Some("https://graph.example.test")).is_err());
        assert!(graph_basis_aus(Some("http://10.0.0.5:5123")).is_err());
    }

    #[test]
    fn scopes_haengen_am_schalter() {
        let mut konfig = EntraKonfig::mit_geheimnis(
            "11111111-2222-3333-4444-555555555555",
            "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
            "https://login.example.test/tenant/v2.0",
            "https://quiver.example.test/auth/callback",
            "kein-echtes-geheimnis-nur-test",
        );
        assert_eq!(
            konfig.scopes(),
            "openid profile email offline_access",
            "ohne Schalter kein Graph-Bereich"
        );
        konfig.gruppen_namen = true;
        assert!(konfig.scopes().contains(GRAPH_SCOPE));
        assert!(konfig
            .scopes()
            .starts_with("openid profile email offline_access "));
        // Im Debug steht nichts Geheimes.
        assert!(!format!("{konfig:?}").contains("kein-echtes-geheimnis-nur-test"));
    }

    #[test]
    fn basis_url_regeln() {
        assert!(basis_url_pruefen("https://quiver.example.test").is_ok());
        assert_eq!(
            basis_url_pruefen("https://quiver.example.test/").unwrap(),
            "https://quiver.example.test"
        );
        assert!(basis_url_pruefen("http://127.0.0.1:8740").is_ok());
        assert!(basis_url_pruefen("http://localhost:8740").is_ok());
        assert!(basis_url_pruefen("http://[::1]:8740").is_ok());
        assert!(basis_url_pruefen("http://quiver.example.test").is_err());
        assert!(basis_url_pruefen("http://10.0.0.5:8740").is_err());
        assert!(basis_url_pruefen("ftp://quiver.example.test").is_err());
        assert!(basis_url_pruefen("https://quiver.example.test/pfad").is_err());
        assert!(basis_url_pruefen("https://quiver.example.test/?x=1").is_err());
        assert!(basis_url_pruefen("keine url").is_err());
    }

    #[test]
    fn guids() {
        assert!(ist_guid("11111111-2222-3333-4444-555555555555"));
        assert!(ist_guid("AAAAAAAA-BBBB-CCCC-DDDD-EEEEEEEEEEEE"));
        assert!(!ist_guid("common"));
        assert!(!ist_guid("organizations"));
        assert!(!ist_guid("11111111-2222-3333-4444"));
    }

    #[test]
    fn cidr_und_forwarded_for() {
        let netz = Netz::lesen("10.0.0.0/8").unwrap();
        assert!(netz.enthaelt("10.1.2.3".parse().unwrap()));
        assert!(!netz.enthaelt("11.0.0.1".parse().unwrap()));
        let v6 = Netz::lesen("2001:db8::/32").unwrap();
        assert!(v6.enthaelt("2001:db8::1".parse().unwrap()));
        assert!(!v6.enthaelt("2001:db9::1".parse().unwrap()));
        assert!(Netz::lesen("10.0.0.0/33").is_err());
        assert!(Netz::lesen("kein-netz").is_err());

        let peer: SocketAddr = "10.0.0.5:1234".parse().unwrap();
        let alle = vec![Netz::lesen("10.0.0.0/8").unwrap()];
        // Von rechts: die erste Adresse, die in keinem vertrauten Netz liegt.
        assert_eq!(
            aufrufer_ip(Some(peer), &["203.0.113.9, 10.0.0.5"], &alle),
            Some("203.0.113.9".parse().unwrap())
        );
        // Der Client schreibt links, was er will: geglaubt wird von rechts.
        assert_eq!(
            aufrufer_ip(Some(peer), &["1.1.1.1, 203.0.113.9, 10.0.0.5"], &alle),
            Some("203.0.113.9".parse().unwrap()),
            "die linke Adresse ist nicht belegt"
        );
        // Zwei Kopfzeilen: eine hängt hinten an, statt die erste zu erweitern.
        assert_eq!(
            aufrufer_ip(Some(peer), &["1.1.1.1", "203.0.113.9"], &alle),
            Some("203.0.113.9".parse().unwrap()),
            "beide Zeilen in Reihenfolge zusammengefasst"
        );
        // Nur vertraute Adressen in der Kette: der Peer bleibt.
        assert_eq!(
            aufrufer_ip(Some(peer), &["10.0.0.7"], &alle),
            Some("10.0.0.7".parse().unwrap())
        );
        // Nicht vertrauter Proxy: der Kopf wird ignoriert.
        assert_eq!(
            aufrufer_ip(Some(peer), &["203.0.113.9"], &[]),
            Some("10.0.0.5".parse().unwrap())
        );
        assert_eq!(aufrufer_ip(None, &["203.0.113.9"], &alle), None);
    }

    /// Härtung Punkt 10: mit Entra sind Altgeräte ohne Angabe gesperrt.
    #[test]
    fn altgeraete_haengen_an_entra() {
        assert!(!altgeraete_aus(None, true).unwrap(), "mit Entra: gesperrt");
        assert!(altgeraete_aus(None, false).unwrap(), "ohne Entra: offen");
        assert!(altgeraete_aus(Some("erlaubt"), true).unwrap());
        assert!(!altgeraete_aus(Some("gesperrt"), false).unwrap());
        assert!(!altgeraete_aus(Some(""), true).unwrap());
        assert!(altgeraete_aus(Some("vielleicht"), true).is_err());
    }

    #[test]
    fn einrichtung_braucht_den_code_ausser_auf_loopback() {
        let loopback: SocketAddr = "127.0.0.1:8740".parse().unwrap();
        let offen: SocketAddr = "0.0.0.0:8740".parse().unwrap();
        // Loopback-Bindung und Loopback-URL: kein Code nötig.
        let mut konfig = Konfig::neu(".");
        assert!(!konfig.einrichtung_mit_code());
        // Sobald der Server nicht nur auf Loopback lauscht: Code, auch bei
        // Loopback-Basis-URL (so läuft der Container).
        konfig.adresse = offen;
        assert!(konfig.einrichtung_mit_code());
        // Öffentliche Basis-URL bei Loopback-Bindung: ebenfalls Code.
        let mut konfig = Konfig::neu(".");
        konfig.oeffentlich = String::from("https://quiver.example.test");
        assert!(konfig.einrichtung_mit_code());
        // Ausnahme nur ausdrücklich und nur auf Loopback.
        assert!(einrichtung_aus(Some("ohne-code"), "http://localhost:8740", loopback).unwrap());
        assert!(
            einrichtung_aus(Some("ohne-code"), "https://quiver.example.test", loopback).is_err()
        );
        assert!(!einrichtung_aus(Some("mit-code"), "http://localhost:8740", loopback).unwrap());
        assert!(!einrichtung_aus(None, "http://localhost:8740", loopback).unwrap());
        assert!(einrichtung_aus(Some("vielleicht"), "http://localhost:8740", loopback).is_err());
        let mut konfig = Konfig::neu(".");
        konfig.einrichtung_ohne_code = true;
        assert!(!konfig.einrichtung_mit_code(), "ohne-code auf Loopback");
        konfig.adresse = offen;
        assert!(
            !konfig.einrichtung_mit_code(),
            "ausdrücklich abgeschaltet gilt auch im Container"
        );
        konfig.oeffentlich = String::from("https://quiver.example.test");
        assert!(
            konfig.einrichtung_mit_code(),
            "mit öffentlicher Basis-URL ist der Code immer Pflicht"
        );
    }

    #[test]
    fn vorgabe_ist_loopback_localhost() {
        assert_eq!(
            oeffentlich_aus(None, 8740).unwrap(),
            "http://localhost:8740"
        );
        assert_eq!(
            oeffentlich_aus(Some("  "), 9000).unwrap(),
            "http://localhost:9000"
        );
        assert_eq!(
            oeffentlich_aus(Some("https://quiver.example.test"), 8740).unwrap(),
            "https://quiver.example.test"
        );
        // Eine fremde http-Adresse bleibt ein Startfehler.
        assert!(oeffentlich_aus(Some("http://quiver.example.test"), 8740).is_err());
    }

    #[test]
    fn laufzeit_laesst_sich_vorspulen() {
        let laufzeit = Laufzeit::neu();
        let vorher = laufzeit.seit_start();
        laufzeit.vorspulen(Duration::from_secs(16 * 60));
        assert!(laufzeit.seit_start() >= vorher + Duration::from_secs(16 * 60));
    }

    #[test]
    fn uhr_ist_einstellbar() {
        let uhr = Uhr::default();
        let fest = OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap();
        uhr.setze(fest);
        assert_eq!(uhr.jetzt().unix_timestamp(), fest.unix_timestamp());
        uhr.zuruecksetzen();
        assert_ne!(uhr.jetzt().unix_timestamp(), fest.unix_timestamp());
    }
}
