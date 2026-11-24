// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Der Server als einziger OIDC-Client (ADR 0002): Discovery, JWKS, ID-Token
//! prüfen, Code einlösen, Refresh-Token einlösen.
//!
//! Keine eigene Kryptografie: Signatur und Claims prüft `jsonwebtoken`, die
//! HTTP-Aufrufe macht `reqwest` ohne Weiterleitungen, mit Zeitlimit und einer
//! festen Obergrenze für die Antwortgröße. Zeitpunkte kommen immer von der Uhr
//! des Servers, damit Tests die Zeit stellen können.

use std::collections::HashSet;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use jsonwebtoken::jwk::{AlgorithmParameters, JwkSet};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use serde_json::Value;
use time::OffsetDateTime;
use url::Url;

use crate::config::{ist_guid, Ausweis, EntraKonfig};

/// Obergrenze für jede JSON-Antwort von Entra (Discovery, JWKS, Token).
pub const MAX_ANTWORT: usize = 64 * 1024;
/// Zeitlimit für einen Aufruf bei Entra.
pub const ZEITLIMIT: Duration = Duration::from_secs(10);
/// Toleranz für `exp`, `nbf`, `iat`.
pub const TOLERANZ_SEKUNDEN: i64 = 60;
/// Frühestens nach dieser Zeit wird die JWKS bei unbekannter `kid` neu geholt.
pub const JWKS_SPERRE: Duration = Duration::from_secs(60);
/// So lange gilt eine Discovery; danach wird sie neu geholt (Aufgabe 26, 6).
pub const DISCOVERY_STUNDEN: i64 = 24;
/// `client_assertion_type` für `private_key_jwt` (RFC 7523).
pub const CLIENT_ASSERTION_TYP: &str = "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";

/// Was im ID-Token stehen muss.
#[derive(Debug, Clone)]
pub struct Erwartet {
    pub issuer: String,
    pub audience: String,
    pub tenant: String,
    pub nonce: String,
}

/// Die geprüfte Identität. `(tid, oid)` ist der Schlüssel, E-Mail und Name sind
/// nur Anzeige (ADR 0002).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identitaet {
    pub tid: String,
    pub oid: String,
    pub email: Option<String>,
    pub name: Option<String>,
    /// Entra-Objekt-Ids der Gruppen (nur der App zugewiesene).
    pub gruppen: Vec<String>,
}

/// Die Endpunkte aus der Discovery.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

/// Fehler beim Einlösen bei Entra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenFehler {
    /// Entra hat mit einem OAuth-Fehler geantwortet, z. B. `invalid_grant`.
    Oauth(String),
    /// Netz, Zeitlimit, 5xx oder unlesbare Antwort.
    Netz(String),
}

impl TokenFehler {
    /// Muss sich der Benutzer neu anmelden?
    pub fn neu_anmelden(&self) -> bool {
        match self {
            TokenFehler::Oauth(code) => matches!(
                code.as_str(),
                "invalid_grant" | "interaction_required" | "login_required" | "consent_required"
            ),
            TokenFehler::Netz(_) => false,
        }
    }
}

/// Die Antwort des Token-Endpunkts.
///
/// `Debug` zeigt **keinen** Tokeninhalt (Aufgabe 30): weder ID-Token noch
/// Refresh- noch Zugriffstoken dürfen in einer Ausgabe landen.
#[derive(Clone, PartialEq, Eq)]
pub struct Tokens {
    pub id_token: String,
    pub refresh_token: Option<String>,
    /// Nur mit `QUIVER_ENTRA_GRUPPENNAMEN=an` und nur im Speicher dieses Laufs.
    pub access_token: Option<String>,
}

impl std::fmt::Debug for Tokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tokens")
            .field("id_token", &"<verborgen>")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<verborgen>"),
            )
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "<verborgen>"),
            )
            .finish()
    }
}

struct Cache {
    set: Option<JwkSet>,
    geholt: Option<OffsetDateTime>,
}

/// Der OIDC-Client. Discovery und JWKS werden beim ersten Bedarf geholt.
pub struct Oidc {
    pub konfig: EntraKonfig,
    client: reqwest::Client,
    /// Die zuletzt geprüfte Discovery und wann sie geholt wurde.
    discovery: Mutex<Option<(Discovery, OffsetDateTime)>>,
    jwks: Mutex<Cache>,
}

impl std::fmt::Debug for Oidc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Oidc")
            .field("tenant", &self.konfig.tenant)
            .field("authority", &self.konfig.authority)
            .finish()
    }
}

impl Oidc {
    pub fn neu(konfig: EntraKonfig) -> anyhow::Result<Self> {
        let client = reqwest::Client::builder()
            // Keine Weiterleitungen: das Ziel steht in der Discovery und wird
            // geprüft, nicht vom Server umgebogen.
            .redirect(reqwest::redirect::Policy::none())
            .timeout(ZEITLIMIT)
            .build()
            .context("HTTP-Client für Entra")?;
        Ok(Self {
            konfig,
            client,
            discovery: Mutex::new(None),
            jwks: Mutex::new(Cache {
                set: None,
                geholt: None,
            }),
        })
    }

    /// Discovery holen und prüfen; höchstens 24 Stunden alt (Aufgabe 26, 6).
    ///
    /// Schlägt das Neuholen nach Ablauf fehl, gilt der alte Stand weiter –
    /// mit einer Warnung im Log. Besser eine veraltete Discovery als ein
    /// Server, der ohne Grund niemanden mehr anmeldet.
    pub async fn discovery(&self, jetzt: OffsetDateTime) -> anyhow::Result<Discovery> {
        let vorhanden = crate::sperre(&self.discovery).clone();
        if let Some((entdeckt, geholt)) = vorhanden {
            if jetzt - geholt < time::Duration::hours(DISCOVERY_STUNDEN) {
                return Ok(entdeckt);
            }
            match self.discovery_holen().await {
                Ok(neu) => {
                    *crate::sperre(&self.discovery) = Some((neu.clone(), jetzt));
                    return Ok(neu);
                }
                Err(fehler) => {
                    tracing::warn!(
                        "Discovery ist älter als {DISCOVERY_STUNDEN} Stunden und nicht \
                         erneuerbar ({fehler}); der alte Stand gilt weiter"
                    );
                    return Ok(entdeckt);
                }
            }
        }
        let entdeckt = self.discovery_holen().await?;
        *crate::sperre(&self.discovery) = Some((entdeckt.clone(), jetzt));
        Ok(entdeckt)
    }

    /// Die Discovery wirklich holen (ohne Weiterleitungen, mit Zeitlimit).
    async fn discovery_holen(&self) -> anyhow::Result<Discovery> {
        let url = format!("{}/.well-known/openid-configuration", self.konfig.authority);
        let roh = holen_json(&self.client, &url).await?;
        discovery_pruefen(&roh, &self.konfig.authority)
    }

    /// Die Adresse, an die der Browser zur Anmeldung geschickt wird.
    pub async fn anmelde_url(
        &self,
        state: &str,
        nonce: &str,
        challenge: &str,
        jetzt: OffsetDateTime,
    ) -> anyhow::Result<String> {
        let d = self.discovery(jetzt).await?;
        let mut url = Url::parse(&d.authorization_endpoint)?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.konfig.client_id)
            .append_pair("response_type", "code")
            .append_pair("redirect_uri", &self.konfig.redirect_uri)
            .append_pair("response_mode", "query")
            .append_pair("scope", &self.konfig.scopes())
            .append_pair("state", state)
            .append_pair("nonce", nonce)
            .append_pair("code_challenge", challenge)
            .append_pair("code_challenge_method", "S256");
        Ok(url.to_string())
    }

    /// Code einlösen (Authorization Code + PKCE) und das ID-Token prüfen.
    pub async fn code_einloesen(
        &self,
        code: &str,
        verifier: &str,
        nonce: &str,
        jetzt: OffsetDateTime,
    ) -> anyhow::Result<(Identitaet, Tokens)> {
        let d = self.discovery(jetzt).await?;
        let mut formular = std::collections::HashMap::from([
            (String::from("client_id"), self.konfig.client_id.clone()),
            (
                String::from("grant_type"),
                String::from("authorization_code"),
            ),
            (String::from("code"), code.to_string()),
            (
                String::from("redirect_uri"),
                self.konfig.redirect_uri.clone(),
            ),
            (String::from("code_verifier"), verifier.to_string()),
        ]);
        formular.extend(self.ausweis_felder(&d, jetzt)?);
        let antwort = self
            .formular(&d.token_endpoint, &formular)
            .await
            .map_err(|fehler| anyhow::anyhow!("{fehler:?}"))?;
        let tokens = tokens_aus(&antwort)?;
        let identitaet = self
            .id_token_pruefen(
                &tokens.id_token,
                &Erwartet {
                    issuer: self.konfig.issuer.clone(),
                    audience: self.konfig.client_id.clone(),
                    tenant: self.konfig.tenant.clone(),
                    nonce: nonce.to_string(),
                },
                jetzt,
            )
            .await?;
        Ok((identitaet, tokens))
    }

    /// Refresh-Token einlösen; das neue ID-Token bestimmt die Gruppen.
    pub async fn erneuern(
        &self,
        refresh: &str,
        erwartet: &Erwartet,
        jetzt: OffsetDateTime,
    ) -> Result<(Identitaet, Tokens), TokenFehler> {
        let d = self
            .discovery(jetzt)
            .await
            .map_err(|fehler| TokenFehler::Netz(format!("Discovery nicht erreichbar: {fehler}")))?;
        let mut formular = std::collections::HashMap::from([
            (String::from("client_id"), self.konfig.client_id.clone()),
            (String::from("grant_type"), String::from("refresh_token")),
            (String::from("refresh_token"), refresh.to_string()),
            (String::from("scope"), self.konfig.scopes()),
        ]);
        formular.extend(
            self.ausweis_felder(&d, jetzt)
                .map_err(|fehler| TokenFehler::Netz(format!("{fehler:#}")))?,
        );
        let antwort = self.formular(&d.token_endpoint, &formular).await?;
        let tokens =
            tokens_aus(&antwort).map_err(|fehler| TokenFehler::Netz(format!("{fehler:#}")))?;
        let identitaet = self
            .id_token_pruefen(&tokens.id_token, erwartet, jetzt)
            .await
            .map_err(|fehler| TokenFehler::Netz(format!("ID-Token unbrauchbar: {fehler}")))?;
        Ok((identitaet, tokens))
    }

    /// ID-Token prüfen: Signatur, Algorithmus, `iss`, `aud`, `tid`, `nonce`,
    /// Zeitfenster, `oid`, Gruppen.
    pub async fn id_token_pruefen(
        &self,
        token: &str,
        erwartet: &Erwartet,
        jetzt: OffsetDateTime,
    ) -> anyhow::Result<Identitaet> {
        let kopf = decode_header(token).context("ID-Token ist kein JWT")?;
        if kopf.alg != Algorithm::RS256 {
            anyhow::bail!("ID-Token hat {alg:?} statt RS256", alg = kopf.alg);
        }
        let kid = kopf
            .kid
            .ok_or_else(|| anyhow::anyhow!("ID-Token hat keine kid"))?;
        let jwks = self.jwks_fuer(&kid, jetzt).await?;
        let schluessel = jwks
            .find(&kid)
            .ok_or_else(|| anyhow::anyhow!("JWKS kennt kid {kid} nicht"))?;
        if !matches!(schluessel.algorithm, AlgorithmParameters::RSA(_)) {
            anyhow::bail!("kid {kid} ist kein RSA-Schlüssel");
        }
        let decoding = DecodingKey::from_jwk(schluessel).context("JWK nicht brauchbar")?;

        let mut pruefung = Validation::new(Algorithm::RS256);
        pruefung.leeway = TOLERANZ_SEKUNDEN.unsigned_abs();
        // Die Zeitfenster prüfen wir selbst gegen die Uhr des Servers.
        pruefung.validate_exp = false;
        pruefung.validate_nbf = false;
        pruefung.set_audience(std::slice::from_ref(&erwartet.audience));
        pruefung.set_issuer(std::slice::from_ref(&erwartet.issuer));
        pruefung.required_spec_claims = ["exp", "iss", "aud"]
            .iter()
            .map(|s| s.to_string())
            .collect::<HashSet<_>>();

        let daten = decode::<Value>(token, &decoding, &pruefung)
            .map_err(|fehler| anyhow::anyhow!("ID-Token abgelehnt: {fehler}"))?;
        claims_pruefen(&daten.claims, erwartet, jetzt)
    }

    /// JWKS holen; eine unbekannte `kid` löst höchstens einmal pro Minute ein
    /// neues Holen aus.
    async fn jwks_fuer(&self, kid: &str, jetzt: OffsetDateTime) -> anyhow::Result<JwkSet> {
        {
            let cache = crate::sperre(&self.jwks);
            if let Some(set) = &cache.set {
                if set.find(kid).is_some() {
                    return Ok(set.clone());
                }
                if let Some(geholt) = cache.geholt {
                    if jetzt - geholt < time::Duration::seconds(JWKS_SPERRE.as_secs() as i64) {
                        anyhow::bail!("kid {kid} unbekannt; JWKS wurde gerade geholt");
                    }
                }
            }
        }
        let d = self.discovery(jetzt).await?;
        let roh = holen_json(&self.client, &d.jwks_uri).await?;
        let set: JwkSet = serde_json::from_value(roh).context("JWKS nicht lesbar")?;
        if set.find(kid).is_none() {
            // Auch der neue Stand kennt die kid nicht; Sperre setzen.
            let mut cache = crate::sperre(&self.jwks);
            cache.geholt = Some(jetzt);
            anyhow::bail!("kid {kid} steht nicht in der JWKS");
        }
        {
            let mut cache = crate::sperre(&self.jwks);
            cache.set = Some(set.clone());
            cache.geholt = Some(jetzt);
        }
        Ok(set)
    }

    /// Die Felder des Ausweises: Geheimnis **oder** Client-Assertion.
    ///
    /// Die Assertion entsteht je Anfrage neu (`jti`, `nbf`, `exp`); `aud` ist
    /// der Token-Endpunkt aus der Discovery (Aufgabe 23).
    fn ausweis_felder(
        &self,
        discovery: &Discovery,
        jetzt: OffsetDateTime,
    ) -> anyhow::Result<std::collections::HashMap<String, String>> {
        let mut felder = std::collections::HashMap::new();
        match &self.konfig.ausweis {
            Ausweis::Geheimnis(geheimnis) => {
                felder.insert(String::from("client_secret"), geheimnis.clone());
            }
            Ausweis::Schluessel(halter) => {
                let schluessel = halter.jetzt();
                let assertion = schluessel.assertion(
                    &self.konfig.client_id,
                    &discovery.token_endpoint,
                    jetzt,
                )?;
                felder.insert(
                    String::from("client_assertion_type"),
                    String::from(CLIENT_ASSERTION_TYP),
                );
                felder.insert(String::from("client_assertion"), assertion);
            }
        }
        Ok(felder)
    }

    /// Formular an den Token-Endpunkt; HTTP-Fehler werden zu
    /// [`TokenFehler::Oauth`], damit `invalid_grant` erkennbar bleibt.
    async fn formular(
        &self,
        url: &str,
        karte: &std::collections::HashMap<String, String>,
    ) -> Result<Value, TokenFehler> {
        let antwort = self
            .client
            .post(url)
            .form(karte)
            .send()
            .await
            .map_err(|fehler| {
                TokenFehler::Netz(format!("Token-Endpunkt nicht erreichbar: {fehler}"))
            })?;
        let status = antwort.status();
        let roh = antwort_json(antwort)
            .await
            .map_err(|fehler| TokenFehler::Netz(format!("{fehler:#}")))?;
        if !status.is_success() {
            let code = roh
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unbekannt")
                .to_string();
            return Err(TokenFehler::Oauth(code));
        }
        Ok(roh)
    }
}

fn tokens_aus(wert: &Value) -> anyhow::Result<Tokens> {
    let id_token = wert
        .get("id_token")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("Antwort ohne id_token"))?
        .to_string();
    let refresh_token = wert
        .get("refresh_token")
        .and_then(Value::as_str)
        .map(str::to_string);
    let access_token = wert
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|wert| !wert.is_empty())
        .map(str::to_string);
    Ok(Tokens {
        id_token,
        refresh_token,
        access_token,
    })
}

fn claims_pruefen(
    claims: &Value,
    erwartet: &Erwartet,
    jetzt: OffsetDateTime,
) -> anyhow::Result<Identitaet> {
    let text = |name: &str| claims.get(name).and_then(Value::as_str).map(str::to_string);
    let zahl = |name: &str| claims.get(name).and_then(Value::as_i64);

    if text("iss").as_deref() != Some(erwartet.issuer.as_str()) {
        anyhow::bail!("ID-Token: falscher Aussteller");
    }
    if text("tid").as_deref() != Some(erwartet.tenant.as_str()) {
        anyhow::bail!("ID-Token: falscher Mandant (tid)");
    }
    // Beim Refresh-Token gibt es keinen nonce; dann ist `nonce` leer und die
    // Prüfung entfällt (der Refresh ist selbst das Geheimnis).
    if !erwartet.nonce.is_empty() {
        let nonce = text("nonce").ok_or_else(|| anyhow::anyhow!("ID-Token: nonce fehlt"))?;
        if !crate::hilfen::gleich_konstant(&nonce, &erwartet.nonce) {
            anyhow::bail!("ID-Token: nonce passt nicht");
        }
    }
    let oid = text("oid")
        .filter(|wert| !wert.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("ID-Token: oid fehlt"))?;

    let jetzt_sekunden = jetzt.unix_timestamp();
    let exp = zahl("exp").ok_or_else(|| anyhow::anyhow!("ID-Token: exp fehlt"))?;
    if jetzt_sekunden > exp + TOLERANZ_SEKUNDEN {
        anyhow::bail!("ID-Token: abgelaufen");
    }
    if let Some(nbf) = zahl("nbf") {
        if nbf > jetzt_sekunden + TOLERANZ_SEKUNDEN {
            anyhow::bail!("ID-Token: noch nicht gültig (nbf)");
        }
    }
    if let Some(iat) = zahl("iat") {
        if iat > jetzt_sekunden + TOLERANZ_SEKUNDEN {
            anyhow::bail!("ID-Token: aus der Zukunft (iat)");
        }
    }

    // Zu viele Gruppen: Entra legt dann `_claim_names`/`hasgroups` statt `groups`.
    // Das ist **nicht** „keine Gruppen“, sondern ein Grund abzulehnen.
    if claims
        .get("hasgroups")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || claims
            .get("_claim_names")
            .and_then(Value::as_object)
            .is_some_and(|objekt| objekt.contains_key("groups"))
    {
        anyhow::bail!(
            "ID-Token: zu viele Gruppen im Token (groups-Überlauf); \
             die App-ID-Zuweisung muss die Gruppen begrenzen"
        );
    }

    let gruppen = match claims.get("groups") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(liste)) => {
            let mut aus = Vec::with_capacity(liste.len());
            for wert in liste {
                let id = wert
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("ID-Token: groups enthält keinen Text"))?;
                if !ist_guid(id) {
                    anyhow::bail!("ID-Token: groups enthält keine Objekt-Id");
                }
                aus.push(id.to_ascii_lowercase());
            }
            aus
        }
        Some(_) => anyhow::bail!("ID-Token: groups ist keine Liste"),
    };

    let email = text("preferred_username").or_else(|| text("email"));
    Ok(Identitaet {
        tid: erwartet.tenant.clone(),
        oid,
        email,
        name: text("name"),
        gruppen,
    })
}

/// Discovery prüfen: `issuer` exakt, Endpunkte `https` (Loopback ausgenommen)
/// und auf demselben Host wie die Authority.
pub fn discovery_pruefen(wert: &Value, authority: &str) -> anyhow::Result<Discovery> {
    let text = |name: &str| {
        wert.get(name)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| anyhow::anyhow!("Discovery ohne {name}"))
    };
    let issuer = text("issuer")?;
    if issuer != authority {
        anyhow::bail!("Discovery: issuer {issuer} ist nicht exakt die Authority {authority}");
    }
    let authority_url =
        Url::parse(authority).with_context(|| format!("Authority {authority} ist keine URL"))?;
    let loopback = crate::config::ist_loopback_url(authority);
    let mut endpunkte = Vec::new();
    for name in ["authorization_endpoint", "token_endpoint", "jwks_uri"] {
        let wert = text(name)?;
        let url = Url::parse(&wert).with_context(|| format!("Discovery: {name} ist keine URL"))?;
        if url.host_str() != authority_url.host_str() {
            anyhow::bail!("Discovery: {name} zeigt auf einen anderen Host");
        }
        if url.scheme() != "https" && !loopback {
            anyhow::bail!("Discovery: {name} ist nicht https");
        }
        endpunkte.push(wert);
    }
    Ok(Discovery {
        issuer,
        authorization_endpoint: endpunkte[0].clone(),
        token_endpoint: endpunkte[1].clone(),
        jwks_uri: endpunkte[2].clone(),
    })
}

async fn holen_json(client: &reqwest::Client, url: &str) -> anyhow::Result<Value> {
    let antwort = client
        .get(url)
        .send()
        .await
        .with_context(|| format!("{url} nicht erreichbar"))?;
    if !antwort.status().is_success() {
        anyhow::bail!("{url} antwortet mit {}", antwort.status());
    }
    antwort_json(antwort).await
}

async fn antwort_json(antwort: reqwest::Response) -> anyhow::Result<Value> {
    if let Some(laenge) = antwort.content_length() {
        if laenge > MAX_ANTWORT as u64 {
            anyhow::bail!("Antwort ist größer als {MAX_ANTWORT} Byte");
        }
    }
    let bytes = antwort.bytes().await.context("Antwort nicht lesbar")?;
    if bytes.len() > MAX_ANTWORT {
        anyhow::bail!("Antwort ist größer als {MAX_ANTWORT} Byte");
    }
    serde_json::from_slice(&bytes).context("Antwort ist kein JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    use jsonwebtoken::jwk::{CommonParameters, Jwk};
    use jsonwebtoken::{encode, EncodingKey, Header};
    use rsa::pkcs1::EncodeRsaPrivateKey;
    use rsa::traits::PublicKeyParts;
    use rsa::PrivateKey;
    use serde_json::json;

    const TENANT: &str = "11111111-2222-3333-4444-555555555555";
    const CLIENT: &str = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
    const OID: &str = "99999999-8888-7777-6666-555555555555";
    const GRUPPE: &str = "12345678-1234-1234-1234-123456789012";
    const NONCE: &str = "nonce-fuer-den-test";

    struct Schluessel {
        privat: PrivateKey,
        kid: String,
    }

    impl Schluessel {
        fn neu(kid: &str) -> Self {
            let mut rng = getrandom::SysRng;
            let privat = PrivateKey::new(&mut rng, 2048).expect("RSA-Schlüssel");
            Self {
                privat,
                kid: kid.to_string(),
            }
        }

        fn pem(&self) -> Vec<u8> {
            self.privat
                .to_pkcs1_pem(rsa::pkcs1::LineEnding::LF)
                .expect("PEM")
                .as_bytes()
                .to_vec()
        }

        fn jwk(&self) -> Jwk {
            let n = URL_SAFE_NO_PAD.encode(self.privat.n().to_be_bytes_trimmed_vartime());
            let e = URL_SAFE_NO_PAD.encode(self.privat.e().to_be_bytes_trimmed_vartime());
            Jwk {
                common: CommonParameters {
                    key_id: Some(self.kid.clone()),
                    ..Default::default()
                },
                algorithm: AlgorithmParameters::RSA(jsonwebtoken::jwk::RSAKeyParameters {
                    key_type: jsonwebtoken::jwk::RSAKeyType::RSA,
                    n,
                    e,
                }),
            }
        }

        fn jwks(&self) -> JwkSet {
            JwkSet {
                keys: vec![self.jwk()],
            }
        }
    }

    fn jetzt() -> OffsetDateTime {
        OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap()
    }

    fn claims(zeit: OffsetDateTime) -> Value {
        json!({
            "iss": "https://login.example.test/tenant/v2.0",
            "aud": CLIENT,
            "tid": TENANT,
            "oid": OID,
            "nonce": NONCE,
            "exp": zeit.unix_timestamp() + 3600,
            "nbf": zeit.unix_timestamp() - 10,
            "iat": zeit.unix_timestamp() - 10,
            "preferred_username": "test@example.test",
            "name": "Test Person",
            "groups": [GRUPPE]
        })
    }

    fn oidc() -> Oidc {
        let konfig = EntraKonfig::mit_geheimnis(
            TENANT,
            CLIENT,
            "https://login.example.test/tenant/v2.0",
            "https://quiver.example.test/auth/callback",
            "kein-echtes-geheimnis-nur-test",
        );
        Oidc::neu(konfig).unwrap()
    }

    /// Setzt JWKS direkt in den Zwischenspeicher (kein Netz im Unit-Test).
    fn mit_jwks(oidc: &Oidc, schluessel: &Schluessel) {
        let mut cache = crate::sperre(&oidc.jwks);
        cache.set = Some(schluessel.jwks());
        cache.geholt = Some(jetzt());
    }

    fn erwartet() -> Erwartet {
        Erwartet {
            issuer: "https://login.example.test/tenant/v2.0".into(),
            audience: CLIENT.into(),
            tenant: TENANT.into(),
            nonce: NONCE.into(),
        }
    }

    fn token(schluessel: &Schluessel, claims: &Value) -> String {
        let mut kopf = Header::new(Algorithm::RS256);
        kopf.kid = Some(schluessel.kid.clone());
        let key = EncodingKey::from_rsa_pem(&schluessel.pem()).unwrap();
        encode(&kopf, claims, &key).unwrap()
    }

    #[tokio::test]
    async fn gueltiges_token_wird_gelesen() {
        let oidc = oidc();
        let schluessel = Schluessel::neu("kid-1");
        mit_jwks(&oidc, &schluessel);
        let ident = oidc
            .id_token_pruefen(&token(&schluessel, &claims(jetzt())), &erwartet(), jetzt())
            .await
            .expect("Token gilt");
        assert_eq!(ident.oid, OID);
        assert_eq!(ident.tid, TENANT);
        assert_eq!(ident.gruppen, vec![GRUPPE.to_ascii_lowercase()]);
        assert_eq!(ident.email.as_deref(), Some("test@example.test"));
    }

    #[tokio::test]
    async fn falsche_signatur_wird_abgelehnt() {
        let oidc = oidc();
        let echt = Schluessel::neu("kid-1");
        mit_jwks(&oidc, &echt);
        let fremd = Schluessel::neu("kid-1");
        let fehler = oidc
            .id_token_pruefen(&token(&fremd, &claims(jetzt())), &erwartet(), jetzt())
            .await
            .expect_err("fremder Schlüssel");
        assert!(format!("{fehler:#}").contains("abgelehnt"));
    }

    #[tokio::test]
    async fn alg_none_und_hs256_werden_abgelehnt() {
        let oidc = oidc();
        let schluessel = Schluessel::neu("kid-1");
        mit_jwks(&oidc, &schluessel);
        // alg=none: Header und Rumpf ohne Signatur.
        let kopf = URL_SAFE_NO_PAD.encode(b"{\"alg\":\"none\",\"kid\":\"kid-1\"}");
        let rumpf = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims(jetzt())).unwrap());
        let ohne = format!("{kopf}.{rumpf}.");
        let fehler = oidc
            .id_token_pruefen(&ohne, &erwartet(), jetzt())
            .await
            .expect_err("alg none");
        assert!(format!("{fehler:#}").contains("RS256"));

        // HS256 mit dem öffentlichen Schlüssel als Geheimnis.
        let mut hs = Header::new(Algorithm::HS256);
        hs.kid = Some("kid-1".into());
        let hs_token = encode(
            &hs,
            &claims(jetzt()),
            &EncodingKey::from_secret(schluessel.pem().as_slice()).unwrap(),
        )
        .unwrap();
        let fehler = oidc
            .id_token_pruefen(&hs_token, &erwartet(), jetzt())
            .await
            .expect_err("HS256");
        assert!(format!("{fehler:#}").contains("RS256"));
    }

    #[tokio::test]
    async fn fremder_aussteller_und_audience_werden_abgelehnt() {
        let oidc = oidc();
        let schluessel = Schluessel::neu("kid-1");
        mit_jwks(&oidc, &schluessel);
        for (feld, wert) in [
            ("iss", json!("https://login.example.test/andere/v2.0")),
            ("aud", json!("fremde-client-id")),
            ("tid", json!("00000000-0000-0000-0000-000000000000")),
            ("nonce", json!("vertauscht")),
        ] {
            let mut c = claims(jetzt());
            c[feld] = wert;
            let fehler = oidc
                .id_token_pruefen(&token(&schluessel, &c), &erwartet(), jetzt())
                .await
                .expect_err(feld);
            let text = format!("{fehler:#}");
            assert!(
                text.contains("abgelehnt")
                    || text.contains("falsch")
                    || text.contains("nonce")
                    || text.contains("Mandant"),
                "{feld}: {text}"
            );
        }
    }

    #[tokio::test]
    async fn zeitfenster_mit_toleranz() {
        let oidc = oidc();
        let schluessel = Schluessel::neu("kid-1");
        mit_jwks(&oidc, &schluessel);
        let zeit = jetzt();

        let mut abgelaufen = claims(zeit);
        abgelaufen["exp"] = json!(zeit.unix_timestamp() - TOLERANZ_SEKUNDEN - 1);
        assert!(oidc
            .id_token_pruefen(&token(&schluessel, &abgelaufen), &erwartet(), zeit)
            .await
            .is_err());

        // Genau an der Grenze gilt es noch.
        let mut grenze = claims(zeit);
        grenze["exp"] = json!(zeit.unix_timestamp() - TOLERANZ_SEKUNDEN);
        assert!(oidc
            .id_token_pruefen(&token(&schluessel, &grenze), &erwartet(), zeit)
            .await
            .is_ok());

        let mut zukunft = claims(zeit);
        zukunft["nbf"] = json!(zeit.unix_timestamp() + TOLERANZ_SEKUNDEN + 1);
        assert!(oidc
            .id_token_pruefen(&token(&schluessel, &zukunft), &erwartet(), zeit)
            .await
            .is_err());

        let mut iat = claims(zeit);
        iat["iat"] = json!(zeit.unix_timestamp() + TOLERANZ_SEKUNDEN + 1);
        assert!(oidc
            .id_token_pruefen(&token(&schluessel, &iat), &erwartet(), zeit)
            .await
            .is_err());
    }

    #[tokio::test]
    async fn oid_ist_pflicht() {
        let oidc = oidc();
        let schluessel = Schluessel::neu("kid-1");
        mit_jwks(&oidc, &schluessel);
        let mut c = claims(jetzt());
        c.as_object_mut().unwrap().remove("oid");
        let fehler = oidc
            .id_token_pruefen(&token(&schluessel, &c), &erwartet(), jetzt())
            .await
            .expect_err("oid fehlt");
        assert!(format!("{fehler:#}").contains("oid"));
    }

    #[tokio::test]
    async fn gruppen_ueberlauf_ist_kein_leeres_ergebnis() {
        let oidc = oidc();
        let schluessel = Schluessel::neu("kid-1");
        mit_jwks(&oidc, &schluessel);
        for feld in ["_claim_names", "hasgroups"] {
            let mut c = claims(jetzt());
            c.as_object_mut().unwrap().remove("groups");
            if feld == "_claim_names" {
                c["_claim_names"] = json!({"groups": "src1"});
            } else {
                c["hasgroups"] = json!(true);
            }
            let fehler = oidc
                .id_token_pruefen(&token(&schluessel, &c), &erwartet(), jetzt())
                .await
                .expect_err(feld);
            assert!(format!("{fehler:#}").contains("Gruppen"), "{feld}");
        }
        // Fehlt der Claim ganz, gibt es keine Gruppen – das ist erlaubt.
        let mut c = claims(jetzt());
        c.as_object_mut().unwrap().remove("groups");
        let ident = oidc
            .id_token_pruefen(&token(&schluessel, &c), &erwartet(), jetzt())
            .await
            .expect("ohne groups");
        assert!(ident.gruppen.is_empty());
        // Eine Nicht-GUID ist ein Fehler.
        let mut c = claims(jetzt());
        c["groups"] = json!(["keine-guid"]);
        assert!(oidc
            .id_token_pruefen(&token(&schluessel, &c), &erwartet(), jetzt())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn unbekannte_kid_holt_jwks_hoechstens_einmal_pro_minute() {
        let oidc = oidc();
        let schluessel = Schluessel::neu("kid-1");
        {
            let mut cache = crate::sperre(&oidc.jwks);
            cache.set = Some(schluessel.jwks());
            cache.geholt = Some(jetzt());
        }
        // Die Discovery ist nicht erreichbar; der zweite Versuch darf deshalb
        // gar nicht erst loslaufen und muss sofort an der Sperre scheitern.
        let unbekannt = Schluessel::neu("kid-2");
        let fehler = oidc
            .id_token_pruefen(&token(&unbekannt, &claims(jetzt())), &erwartet(), jetzt())
            .await
            .expect_err("kid unbekannt");
        assert!(
            format!("{fehler:#}").contains("gerade geholt"),
            "{fehler:#}"
        );
    }

    #[test]
    fn discovery_wird_geprueft() {
        let gueltig = json!({
            "issuer": "https://login.example.test/tenant/v2.0",
            "authorization_endpoint": "https://login.example.test/tenant/oauth2/authorize",
            "token_endpoint": "https://login.example.test/tenant/oauth2/token",
            "jwks_uri": "https://login.example.test/tenant/discovery/keys"
        });
        let d = discovery_pruefen(&gueltig, "https://login.example.test/tenant/v2.0").unwrap();
        assert!(d.token_endpoint.ends_with("/token"));

        let mut fremder_issuer = gueltig.clone();
        fremder_issuer["issuer"] = json!("https://login.example.test/tenant/v1.0");
        assert!(
            discovery_pruefen(&fremder_issuer, "https://login.example.test/tenant/v2.0").is_err()
        );

        let mut fremder_host = gueltig.clone();
        fremder_host["jwks_uri"] = json!("https://evil.example.test/keys");
        assert!(
            discovery_pruefen(&fremder_host, "https://login.example.test/tenant/v2.0").is_err()
        );

        let mut unsicher = gueltig.clone();
        unsicher["token_endpoint"] = json!("http://login.example.test/token");
        assert!(discovery_pruefen(&unsicher, "https://login.example.test/tenant/v2.0").is_err());

        // Loopback darf http (nur im Test).
        let loopback = json!({
            "issuer": "http://127.0.0.1:9999",
            "authorization_endpoint": "http://127.0.0.1:9999/authorize",
            "token_endpoint": "http://127.0.0.1:9999/token",
            "jwks_uri": "http://127.0.0.1:9999/keys"
        });
        assert!(discovery_pruefen(&loopback, "http://127.0.0.1:9999").is_ok());
    }

    /// Aufgabe 30: kein Tokeninhalt in einer Ausgabe.
    #[test]
    fn debug_zeigt_keine_token() {
        let tokens = Tokens {
            id_token: String::from("id-token-geheim"),
            refresh_token: Some(String::from("refresh-token-geheim")),
            access_token: Some(String::from("zugriffstoken-geheim")),
        };
        let text = format!("{tokens:?}");
        for geheim in [
            "id-token-geheim",
            "refresh-token-geheim",
            "zugriffstoken-geheim",
        ] {
            assert!(!text.contains(geheim), "{text}");
        }
        assert!(text.contains("verborgen"));
    }

    #[test]
    fn token_fehler_entscheidet_ueber_neu_anmelden() {
        assert!(TokenFehler::Oauth("invalid_grant".into()).neu_anmelden());
        assert!(TokenFehler::Oauth("interaction_required".into()).neu_anmelden());
        assert!(!TokenFehler::Oauth("temporarily_unavailable".into()).neu_anmelden());
        assert!(!TokenFehler::Netz("5xx".into()).neu_anmelden());
    }
}
