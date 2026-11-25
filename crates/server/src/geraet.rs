// SPDX-License-Identifier: MIT
// Copyright (C) 2026 Patrick Weppelmann

//! Geräte-Seite: Anmelden mit Einmal-Code, Soll holen, melden, Ereignis-Strom.

use std::{convert::Infallible, sync::Arc, time::Duration};

use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use quiver_domain::{Geraet, GeraetId, Laufzeit, Programm, Soll, System};
use quiver_protocol::{Anmelden, AnmeldenAntwort, Meldung, EREIGNIS_SOLL};
use time::OffsetDateTime;
use tokio_stream::wrappers::BroadcastStream;

use crate::fehler::{Ergebnis, FehlerAntwort, Koerper};
use crate::hilfen;
use crate::speicher;
use crate::zustand::{Stromwaechter, Zustand};
use crate::{db, soll_fuer_system};

/// Gerät mit Einmal-Code einbinden; Antwort enthält das Geräte-Token (nur diesmal).
pub(crate) async fn anmelden(
    State(zustand): State<Arc<Zustand>>,
    req: axum::extract::Request,
) -> Ergebnis<Json<AnmeldenAntwort>> {
    if zustand.oidc.is_some() {
        return Err(FehlerAntwort::nicht_gefunden(
            "dieser Server nutzt Entra; Einmal-Codes gibt es hier nicht (siehe /auth/geraet/start)",
        ));
    }
    let ip = crate::sitzung::aufrufer(&req, &zustand);
    let (_, rumpf) = req.into_parts();
    let bytes = axum::body::to_bytes(rumpf, crate::RUMPF_STANDARD)
        .await
        .map_err(|fehler| {
            if ist_zu_gross(&fehler) {
                FehlerAntwort::zu_gross()
            } else if crate::fehler::ursache_ist::<tower_http::timeout::TimeoutError>(&fehler) {
                FehlerAntwort::zeitlimit()
            } else {
                FehlerAntwort::ungueltig("Rumpf nicht lesbar")
            }
        })?;
    let anfrage: Anmelden = serde_json::from_slice(&bytes)
        .map_err(|_| FehlerAntwort::ungueltig("Rumpf ist kein JSON mit code und name"))?;

    let jetzt = zustand.uhr.jetzt();
    let ip_schluessel = crate::drossel::ip_schluessel(ip);
    let code_schluessel = format!(
        "code:{}",
        anfrage
            .code
            .trim()
            .to_ascii_uppercase()
            .replace(['-', ' '], "")
    );
    for schluessel in [&ip_schluessel, &code_schluessel] {
        if let Some(sekunden) = zustand.drossel.wartezeit(schluessel, jetzt) {
            return Err(FehlerAntwort::neu(
                axum::http::StatusCode::TOO_MANY_REQUESTS,
                format!("zu viele Versuche; bitte {sekunden} Sekunden warten"),
            ));
        }
    }

    let ergebnis = anmelden_intern(&zustand, anfrage, jetzt).await;
    for schluessel in [&ip_schluessel, &code_schluessel] {
        if ergebnis.is_ok() {
            zustand.drossel.erfolg(schluessel);
        } else {
            zustand.drossel.fehlschlag(schluessel, jetzt);
        }
    }
    ergebnis
}

/// Ist der Rumpf an der Grenze gescheitert (und nicht an der Leitung)?
fn ist_zu_gross(fehler: &axum::Error) -> bool {
    use std::error::Error as _;
    fehler
        .source()
        .is_some_and(|quelle| quelle.is::<http_body_util::LengthLimitError>())
}

async fn anmelden_intern(
    zustand: &Arc<Zustand>,
    anfrage: Anmelden,
    jetzt: OffsetDateTime,
) -> Ergebnis<Json<AnmeldenAntwort>> {
    let code = hilfen::code_normalisieren(&anfrage.code)
        .ok_or_else(|| FehlerAntwort::ungueltig("Code hat nicht die Form XXXX-XXXX"))?;
    let name = anfrage.name.trim().to_string();
    if name.is_empty() {
        return Err(FehlerAntwort::ungueltig("Name fehlt"));
    }
    let token = hilfen::token_erzeugen();
    let hash = hilfen::hash(&token);
    let kennung = hilfen::geraet_id_erzeugen();

    let antwort = db(zustand, move |conn| {
        let gruppen = match speicher::code_einloesen(conn, &code, jetzt)? {
            speicher::CodeStand::Unbekannt => {
                return Ok(Err(FehlerAntwort::ungueltig(
                    "Code unbekannt oder schon benutzt",
                )))
            }
            speicher::CodeStand::Abgelaufen => {
                return Ok(Err(FehlerAntwort::abgelaufen("Code ist abgelaufen")))
            }
            speicher::CodeStand::Gueltig(gruppen) => gruppen,
        };
        let geraet = Geraet {
            id: GeraetId::neu(kennung),
            name,
            gruppen,
            gesperrt: false,
        };
        speicher::lege_geraet_an(conn, &geraet, &hash, None, None, None)?;
        speicher::pruefspur_schreiben(
            conn,
            jetzt,
            "geraet",
            "Einmal-Code",
            &format!("Gerät {} eingebunden", geraet.id),
        )?;
        Ok(Ok(AnmeldenAntwort {
            geraet,
            token,
            benutzer: None,
        }))
    })
    .await??;
    Ok(Json(antwort))
}

/// `POST /geraet/abmelden`: das eigene Gerät am Server löschen.
///
/// Das Gegenstück zu „Gerät abmelden“ im Client. Es ist absichtlich **kein**
/// Verwaltungsweg: nur das Gerät selbst kann sich mit seinem Token löschen.
/// Ein bereits gelöschtes Gerät ist kein Fehler – der Client kommt ohnehin
/// nicht mehr an diesen Punkt.
pub(crate) async fn abmelden(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
) -> Ergebnis<StatusCode> {
    let kennung = id.clone();
    db(&zustand, move |conn| {
        speicher::loesche_geraet(conn, kennung.als_str())?;
        Ok(())
    })
    .await?;
    zustand.melde_aenderung();
    tracing::info!(geraet = %id, "Gerät hat sich abgemeldet");
    Ok(StatusCode::NO_CONTENT)
}

/// Welches System meldet das Gerät? (Aufgabe 37)
///
/// Beide Angaben sind freiwillig: ein älterer Client nennt sie nicht und
/// bekommt dann kein Programm, sondern sucht wie bisher. Sind sie da, müssen
/// sie bekannt sein – ein unbekannter Wert ist ein Fehler, kein Rateversuch.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub(crate) struct SystemAbfrage {
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    arch: Option<String>,
}

impl SystemAbfrage {
    /// `Ok(None)`: keine Angabe. `Err`: eine Angabe, die es nicht gibt.
    fn lesen(&self) -> Result<Option<System>, FehlerAntwort> {
        match (self.system.as_deref(), self.arch.as_deref()) {
            (None, None) => Ok(None),
            (Some(system), Some(arch)) => {
                System::aus_teilen(system, arch).map(Some).ok_or_else(|| {
                    FehlerAntwort::ungueltig(
                        "system und arch sind unbekannt (windows|macos|linux, x64|arm64)",
                    )
                })
            }
            _ => Err(FehlerAntwort::ungueltig("system und arch gehören zusammen")),
        }
    }
}

/// Soll-Zustand holen und den Zeitstempel „zuletzt gesehen" nachziehen.
pub(crate) async fn soll(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Query(abfrage): Query<SystemAbfrage>,
) -> Ergebnis<Json<Soll>> {
    let system = abfrage.lesen()?;
    let soll = soll_fuer_system(&zustand, &id, system).await?;
    let gesehen = hilfen::als_text(hilfen::jetzt());
    let kennung = id.clone();
    db(&zustand, move |conn| {
        speicher::setze_gesehen(conn, kennung.als_str(), &gesehen)?;
        Ok(())
    })
    .await?;
    Ok(Json(soll))
}

/// `GET /geraet/tools/{name}?system=…&arch=…` (Aufgabe 37): ein Programm.
///
/// Nur mit Geräte-Token (die Wache davor prüft Zugang, Sperre und Entzug),
/// nur für ein Programm aus dem Soll **dieses** Geräts und nur für das
/// gemeldete System. Ausgeliefert wird gestreamt und größenbegrenzt; `ETag`
/// und `If-None-Match` sparen den zweiten Durchlauf.
pub(crate) async fn tools(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Path(name): Path<String>,
    Query(abfrage): Query<SystemAbfrage>,
    kopf: HeaderMap,
) -> Ergebnis<Response> {
    if !quiver_domain::ist_verteilt(&name) {
        return Err(FehlerAntwort::nicht_gefunden(
            "so ein Programm verteilt dieser Server nicht",
        ));
    }
    let Some(system) = abfrage.lesen()? else {
        return Err(FehlerAntwort::ungueltig(
            "system und arch fehlen (windows|macos|linux, x64|arm64)",
        ));
    };

    // Nur, was in diesem Soll steht: Entzug wirkt sofort, ein fremdes Gerät
    // bekommt nichts, und ein nicht zugewiesenes Programm ist ein 403.
    // Verglichen wird der **Name** aus dem Befehl, nicht der Befehl selbst:
    // ein Altbestand mit festem Pfad bekommt sein Programm genauso.
    let soll = soll_fuer_system(&zustand, &id, Some(system.clone())).await?;
    let zugewiesen = soll.eintraege.iter().any(|eintrag| {
        matches!(&eintrag.eintrag.laufzeit, Laufzeit::Stdio { befehl, .. }
            if quiver_domain::verteiltes_programm(befehl).as_deref() == Some(name.as_str()))
    });
    if !zugewiesen {
        tracing::info!(geraet = %id, programm = %name, "nicht zugewiesen – nicht ausgeliefert");
        return Err(FehlerAntwort::verboten(
            "dieses Programm gehört nicht zum Soll dieses Geräts",
        ));
    }
    // Drossel je Gerät, Programm und Stunde: mehrmals ab- und anmelden muss
    // gehen, endlos laden nicht. Erst nach der Zuweisung gezählt – so füllen
    // erfundene Namen die Karte nicht.
    let jetzt = zustand.uhr.jetzt();
    let stunde = jetzt.unix_timestamp() / 3600;
    if let Some(sekunden) = zustand
        .drossel
        .treffer(&format!("programme:{id}:{name}:{stunde}"), jetzt)
    {
        return Err(FehlerAntwort::neu(
            StatusCode::TOO_MANY_REQUESTS,
            format!("zu viele Abrufe; bitte {sekunden} Sekunden warten"),
        ));
    }
    let Some(programm) = soll.programm(&name) else {
        // Zugewiesen, aber für dieses System gibt es das Programm nicht: kein
        // Fehler des Clients, sondern eine Auskunft.
        return Err(FehlerAntwort::nicht_gefunden(
            "für dieses System gibt es das Programm nicht",
        ));
    };
    ausliefern(&zustand, &name, programm, &kopf).await
}

/// Ein Programm ausliefern: geprüft, gestreamt, mit `ETag`.
async fn ausliefern(
    zustand: &Arc<Zustand>,
    name: &str,
    programm: &Programm,
    kopf: &HeaderMap,
) -> Ergebnis<Response> {
    let etag = format!("\"{}\"", programm.sha256);
    if kopf
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|wert| wert.to_str().ok())
        .is_some_and(|wert| wert.split(',').any(|teil| teil.trim() == etag))
    {
        return Ok((
            StatusCode::NOT_MODIFIED,
            [(axum::http::header::ETAG, etag)],
            "",
        )
            .into_response());
    }

    let Some(fund) = zustand.programme.finden(&system_von(programm), name) else {
        // Zwischen Soll und Ausliefern ist das Programm verschwunden.
        return Err(FehlerAntwort::nicht_verfuegbar(
            "das Programm liegt nicht mehr bereit; bitte gleich noch einmal versuchen",
        ));
    };
    if fund.programm.groesse != programm.groesse || fund.programm.sha256 != programm.sha256 {
        // Die Datei hat sich seit dem Soll geändert. Nicht das halbe Programm
        // schicken: der Client lädt nach dem nächsten Abgleich die neue Fassung.
        tracing::warn!(
            programm = %name,
            "Programm hat sich seit dem Soll geändert – nicht ausgeliefert"
        );
        return Err(FehlerAntwort::nicht_verfuegbar(
            "das Programm wurde gerade ersetzt; bitte gleich noch einmal versuchen",
        ));
    }
    let datei = tokio::fs::File::open(&fund.datei).await.map_err(|fehler| {
        tracing::warn!(programm = %name, "Programm nicht lesbar: {fehler}");
        FehlerAntwort::nicht_verfuegbar("das Programm ist nicht lesbar")
    })?;
    let strom = datei_strom(datei, programm.groesse);
    let antwort = Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/octet-stream")
        .header(axum::http::header::CONTENT_LENGTH, programm.groesse)
        .header(axum::http::header::ETAG, etag)
        .header(axum::http::header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::from_stream(strom))
        .map_err(|fehler| FehlerAntwort::intern_mit_kennung(&fehler))?;
    Ok(antwort)
}

/// Das System eines Programms, wie es im Soll stand.
fn system_von(programm: &Programm) -> System {
    System {
        system: programm.system.clone(),
        arch: programm.arch.clone(),
    }
}

/// Eine Datei als Strom, höchstens `rest` Bytes (keine Zeile über die Größe
/// hinaus: was das Soll nennt, ist auch das Ende).
fn datei_strom(
    datei: tokio::fs::File,
    rest: u64,
) -> impl futures_util::Stream<Item = Result<Vec<u8>, std::io::Error>> {
    futures_util::stream::unfold((datei, rest), |(mut datei, rest)| async move {
        if rest == 0 {
            return None;
        }
        use tokio::io::AsyncReadExt;
        let mut puffer = vec![0u8; rest.min(64 * 1024) as usize];
        match datei.read(&mut puffer).await {
            Ok(0) => None,
            Ok(gelesen) => {
                puffer.truncate(gelesen);
                Some((Ok(puffer), (datei, rest - gelesen as u64)))
            }
            Err(fehler) => Some((Err(fehler), (datei, 0))),
        }
    })
}

/// Letzte Meldung des Clients speichern.
///
/// Die Meldung wird vor dem Speichern gekürzt (Härtung, Punkt 13): höchstens
/// 200 Einträge, Texte auf 500 Zeichen, Client-Fassung auf 64 Zeichen. Der
/// Rumpf selbst ist bereits auf 64 KB begrenzt.
pub(crate) async fn meldung(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Koerper(mut meldung): Koerper<Meldung>,
) -> Ergebnis<StatusCode> {
    crate::grenzen::meldung_kuerzen(&mut meldung);
    let gesehen = hilfen::als_text(hilfen::jetzt());
    let kennung = id.clone();
    db(&zustand, move |conn| {
        if !speicher::setze_meldung(conn, kennung.als_str(), &meldung, &gesehen)? {
            anyhow::bail!("Gerät {kennung} gibt es nicht");
        }
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ereignis-Strom: sofort die aktuelle Revision, danach nur bei Änderung.
///
/// **Reihenfolge: abonnieren → Zugang prüfen → Revision rechnen.** Sie ist der
/// Kern von Punkt 8 und der Grund, warum ein Entzug nicht an einem einzelnen
/// Ereignis hängt. Wer den Zugang vor dem Abonnement prüft, verliert eine
/// Änderung, die genau dazwischen passiert: das Ereignis ging an ein
/// Abonnement, das es noch nicht gab, und die danach gerechnete Revision ist
/// dieselbe wie die des Geräts. Ein entzogener Zugang fiele dann überhaupt
/// nicht auf – der Strom bliebe offen, der Client sähe keinen Anlass
/// nachzufragen und liefe mit laufenden Werkzeugen weiter, bis seine nächste
/// Nachfrage fällig ist (bis zu fünf Minuten später).
///
/// Mit dieser Reihenfolge ist die Lücke zu: was **nach** dem Abonnement
/// passiert, ist ein Ereignis und kommt an – notfalls als Rückstand. Was
/// **davor** passiert, sieht die Prüfung direkt und antwortet mit 403 samt
/// Grund; der Client stoppt darauf sofort alle Werkzeuge.
///
/// Die Zahl der offenen Ströme ist begrenzt (je Gerät und insgesamt): ein
/// Client, der immer neu verbindet, ohne zu schließen, bekommt 429.
pub(crate) async fn ereignisse(
    State(zustand): State<Arc<Zustand>>,
    Extension(id): Extension<GeraetId>,
    Query(abfrage): Query<SystemAbfrage>,
) -> Ergebnis<axum::response::Response> {
    let system = abfrage.lesen()?;
    let Some(waechter) = Stromwaechter::neu(zustand.clone(), id.clone()) else {
        return Err(FehlerAntwort::neu(
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "zu viele offene Ereignis-Ströme (je Gerät höchstens {}, insgesamt {})",
                crate::zustand::STROEME_JE_GERAET,
                crate::zustand::STROEME_GESAMT
            ),
        ));
    };
    // 1. Abonnieren, bevor irgendetwas geprüft oder gerechnet wird.
    let empfaenger = zustand.geaendert.subscribe();
    // 2. Zugang prüfen – aber mit Grund (403), damit der Client weiß, warum.
    crate::auth::geraet_zugang(&zustand, &id).await?;
    // 3. Erst jetzt rechnen.
    let start = soll_fuer_system(&zustand, &id, system.clone())
        .await?
        .revision;
    let ende = zustand.ende_abo();

    let weitere = futures_util::stream::unfold(
        (
            BroadcastStream::new(empfaenger),
            zustand.clone(),
            id.clone(),
            start.clone(),
            waechter,
            ende,
            system,
        ),
        |(mut empfaenger, zustand, id, letzte, waechter, mut ende, system)| async move {
            loop {
                // Jede Änderung (auch ein Rückstand) ist ein Anlass, neu zu rechnen.
                let ereignis = tokio::select! {
                    // Der Server fährt herunter: der Strom endet sofort.
                    _ = ende.recv() => None,
                    eintrag = empfaenger.next() => Some(eintrag),
                };
                match ereignis {
                    None | Some(None) => return None,
                    // Ein Rückstand ist kein Ende: neu rechnen genügt.
                    // Auch ein Rückstand ist ein Anlass, neu zu rechnen: dabei
                    // gingen Meldungen verloren.
                    Some(Some(Err(_) | Ok(()))) => {}
                }
                // Entfernt oder gesperrt: der Strom endet. Der Client verbindet
                // neu, bekommt 401/403 und stoppt alle Tools.
                if !crate::auth::geraet_erlaubt(&zustand, &id).await {
                    tracing::info!("Gerät {id} entfernt oder gesperrt – Ereignis-Strom endet");
                    return None;
                }
                match soll_fuer_system(&zustand, &id, system.clone()).await {
                    Ok(soll) if soll.revision != letzte => {
                        let ereignis = Event::default()
                            .event(EREIGNIS_SOLL)
                            .data(soll.revision.clone());
                        let zustand_strom = (
                            empfaenger,
                            zustand,
                            id,
                            soll.revision,
                            waechter,
                            ende,
                            system,
                        );
                        return Some((Ok::<_, Infallible>(ereignis), zustand_strom));
                    }
                    Ok(_) => {}
                    // Der Grund fällt erst beim Rechnen auf (Gerät weg,
                    // Bibliothek widersprüchlich): Der Strom endet trotzdem.
                    // Der Client fragt am Ende des Stroms nach und bekommt
                    // dabei den Grund – statt hier nur eine Zeile zu schreiben
                    // und einen offenen Strom ohne Aussicht auf ein Ereignis
                    // stehen zu lassen.
                    Err(fehler) => {
                        tracing::warn!(
                            "Soll für Gerät {id} nicht berechenbar, Ereignis-Strom endet: {}",
                            fehler.text
                        );
                        return None;
                    }
                }
            }
        },
    );

    let erste = futures_util::stream::once(async move {
        Ok::<_, Infallible>(Event::default().event(EREIGNIS_SOLL).data(start))
    });
    let antwort = Sse::new(erste.chain(weitere))
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response();
    Ok(strom_kopfzeilen(antwort))
}

/// Kopfzeilen, die verhindern, dass ein Vermittler den Strom zurückhält.
///
/// Ein Ereignis-Strom lebt davon, dass jedes Ereignis sofort beim Client
/// ankommt. Ein Proxy, der den Rumpf puffert (nginx und Verwandte tun das ohne
/// `X-Accel-Buffering: no` bis zu einer Seitengröße), oder eine Verdichtung,
/// die auf ein volles Fenster wartet, macht aus dem Entzug eine Nachricht, die
/// zu spät kommt. `no-transform` sagt zusätzlich: diesen Rumpf nicht umformen.
///
/// HAProxy hält den Strom mit `timeout server/client` (30 s) nur dann für tot,
/// wenn er **schweigt**; der Server schickt alle 15 s ein Keep-Alive, also
/// bleibt er offen. Ein `Connection: keep-alive` braucht es dafür nicht.
fn strom_kopfzeilen(mut antwort: axum::response::Response) -> axum::response::Response {
    let kopf = antwort.headers_mut();
    kopf.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store, no-transform"),
    );
    kopf.insert(
        "x-accel-buffering",
        axum::http::HeaderValue::from_static("no"),
    );
    antwort
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Die Reihenfolge „abonnieren → prüfen → rechnen“ ist dicht: eine Änderung,
    /// die nach dem Abonnement passiert, ist im Abonnement – auch wenn die
    /// Prüfung danach noch läuft.
    #[test]
    fn ereignis_nach_dem_abonnieren_kommt_an() {
        let (sender, _) = tokio::sync::broadcast::channel(8);
        let mut empfaenger = sender.subscribe();
        // Genau hier entzieht der Admin den Zugang: nach dem Abonnement,
        // vor der Prüfung.
        sender.send(()).expect("Empfänger ist da");
        assert!(
            empfaenger.try_recv().is_ok(),
            "das Ereignis ist im Abonnement angekommen"
        );
    }

    /// Zum Vergleich: prüfen **vor** dem Abonnieren verliert das Ereignis.
    /// Genau das war die Lücke – der Strom blieb offen und die Revision gleich.
    #[test]
    fn die_alte_reihenfolge_wuerde_das_ereignis_verlieren() {
        let (sender, _) = tokio::sync::broadcast::channel(8);
        // Ein anderer Empfänger hält den Kanal offen; das neue Abonnement
        // bekommt trotzdem nichts von dem, was vorher gesendet wurde.
        let _frueher = sender.subscribe();
        assert!(sender.send(()).is_ok());
        let mut empfaenger = sender.subscribe();
        assert!(empfaenger.try_recv().is_err(), "nichts mehr da");
    }

    /// Der Strom sagt Vermittlern, dass sie ihn nicht puffern und nicht
    /// umformen dürfen: ein zurückgehaltenes Ereignis ist ein zu später
    /// Entzug.
    #[test]
    fn der_strom_laesst_sich_nicht_puffern() {
        let antwort = axum::response::Response::new(axum::body::Body::empty());
        let antwort = strom_kopfzeilen(antwort);
        let kopf = antwort.headers();
        assert_eq!(
            kopf.get(axum::http::header::CACHE_CONTROL)
                .and_then(|wert| wert.to_str().ok()),
            Some("no-store, no-transform")
        );
        assert_eq!(
            kopf.get("x-accel-buffering")
                .and_then(|wert| wert.to_str().ok()),
            Some("no")
        );
    }
}
