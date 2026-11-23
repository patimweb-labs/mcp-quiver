// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! `server.json` (Format der offiziellen MCP Registry) ↔ [`Eintrag`].
//!
//! [`importieren`] liest, was die Registry liefert, und macht daraus einen
//! [`Eintrag`]; übernommene Server kommen als [`Herkunft::Fremd`] herein (SPEC.md §4).
//! [`exportieren`] schreibt das Format zurück und legt den vollständigen Eintrag
//! unter `_meta[META_SCHLUESSEL]` ab — damit ist der Weg verlustfrei.
//!
//! ```
//! use quiver_bibliothek::{exportieren, importieren};
//! use quiver_domain::{Eintrag, EintragId, Herkunft, Laufzeit};
//!
//! let eintrag = Eintrag {
//!     id: EintragId::neu("wetter"),
//!     version: "1.0.0".into(),
//!     titel: "Wetter".into(),
//!     beschreibung: "Wetterdaten".into(),
//!     herkunft: Herkunft::Eigen,
//!     laufzeit: Laufzeit::Remote {
//!         url: "https://example.com/mcp".into(),
//!         kopfzeilen: Default::default(),
//!     },
//!     felder: Vec::new(),
//! };
//! assert_eq!(importieren(&exportieren(&eintrag)).unwrap(), eintrag);
//! ```

use quiver_domain::{Eintrag, EintragId, Feld, Herkunft, Laufzeit, Quelle, Werte};
use serde_json::{json, Map, Value};

/// Namensraum, unter dem quiver den vollständigen Eintrag in `server.json` ablegt.
pub const META_SCHLUESSEL: &str = "io.github.patimweb-labs.quiver/eintrag";

/// Namensraum, unter dem quiver seine eigenen Einträge in der Registry anmeldet.
const NAMENSRAUM: &str = "io.github.patimweb-labs.quiver";

/// Warum aus einem `server.json` kein [`Eintrag`] wurde.
#[derive(Debug, thiserror::Error)]
pub enum ImportFehler {
    #[error("server.json ist kein JSON-Objekt")]
    KeinObjekt,
    #[error("Eintrag unter _meta[{META_SCHLUESSEL}] ist ungültig: {0}")]
    MetaUngueltig(#[source] serde_json::Error),
    #[error("Feld name fehlt oder ist kein Text")]
    NameFehlt,
    #[error("aus dem Namen {0:?} wird keine gültige id")]
    LeereId(String),
    #[error("weder ein stdio-Paket noch ein streamable-http-Remote vorhanden")]
    KeineLaufzeit,
    #[error("Paket {paket:?} hat den unbekannten registryType {typ:?}")]
    UnbekannterPakettyp { paket: String, typ: String },
    #[error("Paket {0:?} hat keine identifier")]
    PaketOhneIdentifier(String),
    #[error("Remote hat keine url")]
    RemoteOhneUrl,
}

/// `server.json` (als JSON-Wert) → [`Eintrag`].
///
/// Gibt es unter `_meta[META_SCHLUESSEL]` einen Eintrag, gilt nur dieser.
/// Sonst wird aus Name, Paket bzw. Remote ein [`Herkunft::Fremd`]-Eintrag gebaut.
pub fn importieren(server_json: &Value) -> Result<Eintrag, ImportFehler> {
    let obj = server_json.as_object().ok_or(ImportFehler::KeinObjekt)?;

    if let Some(meta) = obj
        .get("_meta")
        .and_then(Value::as_object)
        .and_then(|m| m.get(META_SCHLUESSEL))
        .filter(|m| !m.is_null())
    {
        return serde_json::from_value(meta.clone()).map_err(ImportFehler::MetaUngueltig);
    }

    let name = obj
        .get("name")
        .and_then(Value::as_str)
        .ok_or(ImportFehler::NameFehlt)?;
    let id = id_aus_name(name).ok_or_else(|| ImportFehler::LeereId(name.to_string()))?;
    let titel = text(obj.get("title"))
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| id.clone());
    let version = text(obj.get("version")).unwrap_or_default();

    let pakete = liste(obj.get("packages"));
    let paket = pakete.into_iter().find(|p| {
        p.get("transport")
            .and_then(|t| t.get("type"))
            .and_then(Value::as_str)
            == Some("stdio")
    });

    let (laufzeit, felder) = match paket {
        Some(paket) => laufzeit_aus_paket(paket, &version)?,
        None => {
            let remote = liste(obj.get("remotes"))
                .into_iter()
                .find(|r| r.get("type").and_then(Value::as_str) == Some("streamable-http"))
                .ok_or(ImportFehler::KeineLaufzeit)?;
            laufzeit_aus_remote(remote)?
        }
    };

    Ok(Eintrag {
        id: EintragId::neu(id),
        version,
        titel,
        beschreibung: text(obj.get("description")).unwrap_or_default(),
        herkunft: Herkunft::Fremd,
        laufzeit,
        felder,
    })
}

/// [`Eintrag`] → `server.json`.
///
/// Die Standardfelder werden so gut wie möglich gefüllt; der vollständige
/// Eintrag steht unter `_meta[META_SCHLUESSEL]`, sodass
/// `importieren(&exportieren(&e)) == e` gilt.
pub fn exportieren(eintrag: &Eintrag) -> Value {
    let mut obj = Map::new();
    obj.insert(
        "name".into(),
        Value::String(format!("{NAMENSRAUM}/{}", eintrag.id)),
    );
    obj.insert("title".into(), Value::String(eintrag.titel.clone()));
    obj.insert(
        "description".into(),
        Value::String(eintrag.beschreibung.clone()),
    );
    obj.insert("version".into(), Value::String(eintrag.version.clone()));

    match &eintrag.laufzeit {
        Laufzeit::Stdio {
            befehl,
            argumente,
            umgebung,
        } => {
            if let Some(paket) = paket_aus_stdio(befehl, argumente, umgebung, &eintrag.felder) {
                obj.insert("packages".into(), Value::Array(vec![paket]));
            }
        }
        Laufzeit::Remote { url, kopfzeilen } => {
            let headers = kopfzeilen
                .keys()
                .map(|name| {
                    let feld = eintrag.felder.iter().find(|f| &f.name == name);
                    kopfzeile_json(name, feld)
                })
                .collect::<Vec<_>>();
            obj.insert(
                "remotes".into(),
                Value::Array(vec![json!({
                    "type": "streamable-http",
                    "url": url,
                    "headers": headers,
                })]),
            );
        }
    }

    let mut meta = Map::new();
    meta.insert(
        META_SCHLUESSEL.to_string(),
        serde_json::to_value(eintrag).expect("Eintrag ist serialisierbar"),
    );
    obj.insert("_meta".into(), Value::Object(meta));
    Value::Object(obj)
}

/// Leitet die `id` aus dem Registry-Namen ab: letzter Teil nach `/`, klein,
/// nur `a-z0-9-`. `None`, wenn nichts übrig bleibt.
fn id_aus_name(name: &str) -> Option<String> {
    let mut aus = String::new();
    for c in name
        .rsplit('/')
        .next()
        .unwrap_or(name)
        .to_lowercase()
        .chars()
    {
        let zeichen = if c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' {
            c
        } else {
            '-'
        };
        if zeichen == '-' && (aus.is_empty() || aus.ends_with('-')) {
            continue;
        }
        aus.push(zeichen);
    }
    while aus.ends_with('-') {
        aus.pop();
    }
    (!aus.is_empty()).then_some(aus)
}

fn laufzeit_aus_paket(
    paket: &Value,
    eintrag_version: &str,
) -> Result<(Laufzeit, Vec<Feld>), ImportFehler> {
    let typ = paket
        .get("registryType")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let identifier = paket
        .get("identifier")
        .and_then(Value::as_str)
        .filter(|i| !i.is_empty())
        .ok_or_else(|| ImportFehler::PaketOhneIdentifier(typ.to_string()))?;
    let version = text(paket.get("version")).unwrap_or_else(|| eintrag_version.to_string());

    let (felder, umgebung) = felder_aus_variablen(liste(paket.get("environmentVariables")));
    let argumente = paketargumente(liste(paket.get("packageArguments")));
    let mit_version = |trenner: &str| {
        if version.is_empty() {
            identifier.to_string()
        } else {
            format!("{identifier}{trenner}{version}")
        }
    };

    let (befehl, argumente) = match typ {
        "npm" => {
            let mut a = vec!["-y".to_string(), mit_version("@")];
            a.extend(argumente);
            ("npx", a)
        }
        "pypi" => {
            let mut a = vec![mit_version("==")];
            a.extend(argumente);
            ("uvx", a)
        }
        "oci" => {
            let mut a = vec!["run".to_string(), "-i".to_string(), "--rm".to_string()];
            for name in umgebung.keys() {
                a.push("-e".to_string());
                a.push(name.clone());
            }
            a.push(mit_version(":"));
            a.extend(argumente);
            ("docker", a)
        }
        _ => {
            return Err(ImportFehler::UnbekannterPakettyp {
                paket: identifier.to_string(),
                typ: typ.to_string(),
            })
        }
    };

    Ok((
        Laufzeit::Stdio {
            befehl: befehl.to_string(),
            argumente,
            umgebung,
        },
        felder,
    ))
}

fn laufzeit_aus_remote(remote: &Value) -> Result<(Laufzeit, Vec<Feld>), ImportFehler> {
    let url = remote
        .get("url")
        .and_then(Value::as_str)
        .filter(|u| !u.is_empty())
        .ok_or(ImportFehler::RemoteOhneUrl)?;
    let (felder, kopfzeilen) = felder_aus_variablen(liste(remote.get("headers")));
    Ok((
        Laufzeit::Remote {
            url: url.to_string(),
            kopfzeilen,
        },
        felder,
    ))
}

/// Umgebungsvariablen bzw. Header → Felder, und je Feld ein Platzhalter in
/// `umgebung` / `kopfzeilen` (`NAME = "${NAME}"`).
fn felder_aus_variablen(variablen: Vec<&Value>) -> (Vec<Feld>, Werte) {
    let mut felder = Vec::new();
    let mut werte = Werte::new();
    for v in variablen {
        let Some(name) = v
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.is_empty())
        else {
            continue;
        };
        felder.push(Feld {
            name: name.to_string(),
            beschreibung: text(v.get("description")).unwrap_or_default(),
            quelle: if v.get("isSecret").and_then(Value::as_bool).unwrap_or(false) {
                Quelle::Geheim
            } else {
                Quelle::Client
            },
            pflicht: v.get("isRequired").and_then(Value::as_bool).unwrap_or(true),
            standard: text(v.get("default")),
        });
        werte.insert(name.to_string(), format!("${{{name}}}"));
    }
    (felder, werte)
}

/// `packageArguments`: `positional` → `value`; `named` → `name`, dann `value`.
fn paketargumente(argumente: Vec<&Value>) -> Vec<String> {
    let mut aus = Vec::new();
    for a in argumente {
        let name = a.get("name").and_then(Value::as_str);
        let wert = a.get("value").and_then(Value::as_str);
        if a.get("type").and_then(Value::as_str) == Some("named") {
            if let Some(name) = name {
                aus.push(name.to_string());
            }
        }
        if let Some(wert) = wert {
            aus.push(wert.to_string());
        }
    }
    aus
}

fn liste(wert: Option<&Value>) -> Vec<&Value> {
    wert.and_then(Value::as_array)
        .map(|liste| liste.iter().collect())
        .unwrap_or_default()
}

fn text(wert: Option<&Value>) -> Option<String> {
    wert.and_then(Value::as_str).map(str::to_string)
}

fn paket_aus_stdio(
    befehl: &str,
    argumente: &[String],
    umgebung: &Werte,
    felder: &[Feld],
) -> Option<Value> {
    let (registry_type, identifier, version, paketargumente) = stdio_zerlegen(befehl, argumente)?;
    let variablen = umgebung
        .keys()
        .map(|name| {
            let feld = felder.iter().find(|f| &f.name == name);
            variable_json(name, feld)
        })
        .collect::<Vec<_>>();
    let argumente = paketargumente
        .iter()
        .map(|a| json!({ "type": "positional", "value": a }))
        .collect::<Vec<_>>();
    Some(json!({
        "registryType": registry_type,
        "identifier": identifier,
        "version": version,
        "transport": { "type": "stdio" },
        "packageArguments": argumente,
        "environmentVariables": variablen,
    }))
}

/// Zerlegt eine bekannte Stdio-Laufzeit in Registry-Bausteine. `None`, wenn der
/// Befehl nicht zu `npx`/`uvx`/`docker` passt — dann trägt allein `_meta`.
fn stdio_zerlegen<'a>(
    befehl: &str,
    argumente: &'a [String],
) -> Option<(&'static str, String, String, &'a [String])> {
    match befehl {
        "npx" => {
            let rest = if argumente.first().map(String::as_str) == Some("-y") {
                &argumente[1..]
            } else {
                argumente
            };
            let (kopf, rest) = rest.split_first()?;
            let (identifier, version) = spec_zerlegen(kopf, "@");
            Some(("npm", identifier, version, rest))
        }
        "uvx" => {
            let (kopf, rest) = argumente.split_first()?;
            let (identifier, version) = spec_zerlegen(kopf, "==");
            Some(("pypi", identifier, version, rest))
        }
        "docker" => {
            let mut rest = argumente;
            for weg in ["run", "-i", "--rm"] {
                if rest.first().map(String::as_str) != Some(weg) {
                    return None;
                }
                rest = &rest[1..];
            }
            while rest.first().map(String::as_str) == Some("-e") {
                rest = rest.get(2..)?;
            }
            let (bild, rest) = rest.split_first()?;
            let (identifier, version) = spec_zerlegen(bild, ":");
            Some(("oci", identifier, version, rest))
        }
        _ => None,
    }
}

/// `"pkg@1.2.0"` → (`"pkg"`, `"1.2.0"`); ohne brauchbaren Zusatz → (ganzer Text, leer).
fn spec_zerlegen(spec: &str, trenner: &str) -> (String, String) {
    match spec.rsplit_once(trenner) {
        Some((kopf, version)) if !kopf.is_empty() && !version.is_empty() => {
            (kopf.to_string(), version.to_string())
        }
        _ => (spec.to_string(), String::new()),
    }
}

fn variable_json(name: &str, feld: Option<&Feld>) -> Value {
    json!({
        "name": name,
        "description": feld.map(|f| f.beschreibung.clone()).unwrap_or_default(),
        "isRequired": feld.is_none_or(|f| f.pflicht),
        "isSecret": feld.is_some_and(|f| f.quelle == Quelle::Geheim),
        "default": feld.and_then(|f| f.standard.clone()),
    })
}

fn kopfzeile_json(name: &str, feld: Option<&Feld>) -> Value {
    json!({
        "name": name,
        "description": feld.map(|f| f.beschreibung.clone()).unwrap_or_default(),
        "isRequired": feld.is_none_or(|f| f.pflicht),
        "isSecret": feld.is_some_and(|f| f.quelle == Quelle::Geheim),
    })
}
