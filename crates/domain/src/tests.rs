use super::*;

fn feld(name: &str, quelle: Quelle) -> Feld {
    Feld {
        name: name.into(),
        beschreibung: String::new(),
        quelle,
        pflicht: true,
        standard: None,
        link: None,
        ..Default::default()
    }
}

fn eintrag(id: &str) -> Eintrag {
    Eintrag {
        id: EintragId::neu(id),
        version: "1.0.0".into(),
        titel: id.into(),
        beschreibung: String::new(),
        herkunft: Herkunft::Eigen,
        laufzeit: Laufzeit::Stdio {
            befehl: "${pfad}/tool".into(),
            argumente: vec!["--tenant".into(), "${tenant}".into()],
            umgebung: [("TOKEN".to_string(), "${token}".to_string())].into(),
        },
        felder: vec![
            feld("tenant", Quelle::Server),
            feld("pfad", Quelle::Client),
            feld("token", Quelle::Geheim),
        ],
        aktiv: true,
        anmeldung: None,
        einrichtung: None,
        mehrfach: None,
    }
}

fn satz(id: &str, prio: i32, werte: &[(&str, &str)]) -> Satz {
    Satz {
        id: SatzId::neu(id),
        titel: id.into(),
        prioritaet: prio,
        eintraege: vec![SatzEintrag {
            eintrag: EintragId::neu("mail"),
            werte: werte
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            fest: Default::default(),
        }],
    }
}

fn welt(saetze: Vec<Satz>) -> (Geraet, Vec<Gruppe>, Vec<Satz>, Vec<Eintrag>) {
    let gruppe = Gruppe {
        id: GruppeId::neu("team"),
        titel: "Team".into(),
        saetze: saetze.iter().map(|s| s.id.clone()).collect(),
        entra: Vec::new(),
    };
    let geraet = Geraet {
        id: GeraetId::neu("g1"),
        name: "Laptop".into(),
        gruppen: vec![gruppe.id.clone()],
        gesperrt: false,
    };
    (geraet, vec![gruppe], saetze, vec![eintrag("mail")])
}

#[test]
fn soll_nimmt_nur_server_felder() {
    let (g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "x"), ("token", "leck")])]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    assert_eq!(soll.eintraege.len(), 1);
    assert_eq!(soll.eintraege[0].werte.get("tenant").unwrap(), "x");
    assert!(
        !soll.eintraege[0].werte.contains_key("token"),
        "Geheimes nie vom Server"
    );
}

#[test]
fn hoehere_prioritaet_gewinnt() {
    let (g, gr, s, e) = welt(vec![
        satz("a", 1, &[("tenant", "x")]),
        satz("b", 5, &[("tenant", "y")]),
    ]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    assert_eq!(soll.eintraege[0].werte["tenant"], "y");
}

#[test]
fn gleichstand_mit_verschiedenen_werten_ist_fehler() {
    let (g, gr, s, e) = welt(vec![
        satz("a", 1, &[("tenant", "x")]),
        satz("b", 1, &[("tenant", "y")]),
    ]);
    assert!(matches!(
        soll_berechnen(&g, &gr, &s, &e),
        Err(SollFehler::Widerspruch { .. })
    ));
}

#[test]
fn gesperrtes_geraet_bekommt_nichts() {
    let (mut g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "x")])]);
    g.gesperrt = true;
    assert!(soll_berechnen(&g, &gr, &s, &e)
        .unwrap()
        .eintraege
        .is_empty());
}

#[test]
fn fremde_gruppe_bekommt_nichts() {
    let (mut g, gr, s, e) = welt(vec![satz("a", 0, &[])]);
    g.gruppen.clear();
    assert!(soll_berechnen(&g, &gr, &s, &e)
        .unwrap()
        .eintraege
        .is_empty());
}

#[test]
fn unbekannter_eintrag_ist_fehler() {
    let (g, gr, s, _) = welt(vec![satz("a", 0, &[])]);
    assert!(matches!(
        soll_berechnen(&g, &gr, &s, &[]),
        Err(SollFehler::UnbekannterEintrag { .. })
    ));
}

#[test]
fn konfig_meldet_fehlende_felder() {
    let (g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "x")])]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    let fehlt =
        konfig_zusammensetzen(&soll.eintraege[0], &Werte::new(), &Werte::new()).unwrap_err();
    let namen: Vec<_> = fehlt.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(namen, ["pfad", "token"]);
}

#[test]
fn konfig_ersetzt_platzhalter() {
    let (g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "x")])]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    let lokal = [("pfad".to_string(), "/opt".to_string())].into();
    let geheim = [("token".to_string(), "t0p".to_string())].into();
    let lz = konfig_zusammensetzen(&soll.eintraege[0], &lokal, &geheim).unwrap();
    assert_eq!(
        lz,
        Laufzeit::Stdio {
            befehl: "/opt/tool".into(),
            argumente: vec!["--tenant".into(), "x".into()],
            umgebung: [("TOKEN".to_string(), "t0p".to_string())].into(),
        }
    );
}

#[test]
fn ersetzen_laesst_unbekanntes_stehen() {
    let w = [("a".to_string(), "1".to_string())].into();
    assert_eq!(ersetzen("${a}-${b}-${", &w), "1-${b}-${");
}

#[test]
fn abgleich_reihenfolge() {
    let (g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "x")])]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    let mail = EintragId::neu("mail");
    let alt = EintragId::neu("alt");

    assert_eq!(
        abgleichen(&soll, &BTreeMap::new()),
        [Aktion::Starten(mail.clone())]
    );
    let fa = soll.eintraege[0].fingerabdruck();
    let ist: BTreeMap<_, _> = [(mail.clone(), fa), (alt.clone(), "x".into())].into();
    assert_eq!(abgleichen(&soll, &ist), [Aktion::Stoppen(alt.clone())]);
    let ist: BTreeMap<_, _> = [(mail.clone(), "veraltet".into())].into();
    assert_eq!(abgleichen(&soll, &ist), [Aktion::Neustarten(mail)]);
}

/// Kommt ein Programm dazu, ändert es sich oder fällt es weg, startet der
/// Eintrag neu – auch wenn seine Konfiguration gleich bleibt.
#[test]
fn ein_anderes_programm_ist_ein_neustart() {
    let (g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "x")])]);
    let mut soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    soll.eintraege[0].eintrag.laufzeit = Laufzeit::Stdio {
        befehl: "quiver-mail".into(),
        argumente: Vec::new(),
        umgebung: Werte::new(),
    };
    let id = soll.eintraege[0].eintrag.id.clone();
    let mit = |sha: &str| {
        soll.clone().mit_programmen(vec![Programm {
            name: "quiver-mail".into(),
            version: "1".into(),
            groesse: 1,
            sha256: sha.repeat(64),
            system: "linux".into(),
            arch: "x64".into(),
        }])
    };
    let laeuft =
        |soll: &Soll| BTreeMap::from([(id.clone(), soll.fingerabdruck(&soll.eintraege[0]))]);
    let neu = [Aktion::Neustarten(id.clone())];
    assert_eq!(abgleichen(&mit("a"), &laeuft(&soll)), neu, "kommt dazu");
    assert_eq!(
        abgleichen(&mit("b"), &laeuft(&mit("a"))),
        neu,
        "ändert sich"
    );
    assert_eq!(abgleichen(&soll, &laeuft(&mit("a"))), neu, "fällt weg");
    assert_eq!(abgleichen(&mit("a"), &laeuft(&mit("a"))), []);
}

#[test]
fn revision_ist_stabil_und_aendert_sich() {
    let (g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "x")])]);
    let r1 = soll_berechnen(&g, &gr, &s, &e).unwrap().revision;
    let r2 = soll_berechnen(&g, &gr, &s, &e).unwrap().revision;
    assert_eq!(r1, r2);
    let (g, gr, s, e) = welt(vec![satz("a", 0, &[("tenant", "y")])]);
    assert_ne!(r1, soll_berechnen(&g, &gr, &s, &e).unwrap().revision);
}

#[test]
fn tool_namen_rundlauf() {
    let id = EintragId::neu("microsoft-mail");
    let n = tool_name(&id, "send_mail");
    assert_eq!(n, "microsoft-mail__send_mail");
    // Die Umkehrung ist keine Zerlegung mehr (Paket 3): der Client ordnet
    // über eine Tabelle zu und trennt nur das eigene Präfix ab.
    assert_eq!(n.strip_prefix("microsoft-mail__"), Some("send_mail"));
}

#[test]
fn laufzeit_json_form() {
    let lz: Laufzeit = serde_json::from_str(r#"{"art":"remote","url":"https://x/mcp"}"#).unwrap();
    assert_eq!(
        lz,
        Laufzeit::Remote {
            url: "https://x/mcp".into(),
            kopfzeilen: Werte::new()
        }
    );
}

#[test]
fn abgeschalteter_eintrag_laeuft_nirgends() {
    let (g, gr, s, mut e) = welt(vec![satz("a", 0, &[("tenant", "x")])]);
    e[0].aktiv = false;
    assert!(soll_berechnen(&g, &gr, &s, &e)
        .unwrap()
        .eintraege
        .is_empty());
}

#[test]
fn aktiv_fehlt_in_alten_daten_gilt_als_an() {
    let json = r#"{"id":"x","version":"1","titel":"X","herkunft":"eigen","laufzeit":{"art":"remote","url":"u"}}"#;
    let e: Eintrag = serde_json::from_str(json).unwrap();
    assert!(e.aktiv);
}

#[test]
fn link_wird_aus_feldern_gebaut() {
    let link = FeldLink {
        text: "Token in Outline erstellen".into(),
        url: "${url}/settings/api-and-access".into(),
    };
    let leer = Werte::new();
    assert_eq!(link_aufloesen(&link, &leer), None, "ohne URL kein Knopf");
    let w: Werte = [("url".to_string(), " https://wiki.example.de/ ".to_string())].into();
    assert_eq!(
        link_aufloesen(&link, &w).as_deref(),
        Some("https://wiki.example.de/settings/api-and-access")
    );
    let boese: Werte = [("url".to_string(), "javascript:alert(1)".to_string())].into();
    assert_eq!(link_aufloesen(&link, &boese), None, "nur http(s)");
}

#[test]
fn zugangsnamen() {
    assert!(zugangsname_gueltig("web-01"));
    assert!(!zugangsname_gueltig(""));
    assert!(!zugangsname_gueltig("Web 01"));
    // SAP-Systeme heißen groß: erlaubt, und gleich zur kleinen Schreibweise.
    assert!(zugangsname_gueltig("BL4_900"));
    assert!(zugangsname_gleich("BL4_900", "bl4_900"));
    assert!(!zugangsname_gleich("bl4_900", "bl4_901"));
    assert!(!zugangsname_gueltig("wäb"));
    assert!(!zugangsname_gueltig(&"a".repeat(41)));
}

#[test]
fn zugaenge_json_form() {
    let z = vec![Zugang {
        name: "web-01".into(),
        werte: [("host".to_string(), "10.0.0.1".to_string())].into(),
    }];
    assert_eq!(
        serde_json::to_string(&z).unwrap(),
        r#"[{"name":"web-01","werte":{"host":"10.0.0.1"}}]"#
    );
}

#[test]
fn feldart_und_titel_gehen_durch_json() {
    let mut schalter = feld("pruefen", Quelle::Client);
    schalter.art = FeldArt::Schalter;
    schalter.titel = Some("Zertifikat prüfen".into());
    let json = serde_json::to_string(&schalter).unwrap();
    assert_eq!(serde_json::from_str::<Feld>(&json).unwrap(), schalter);

    let auswahl = Feld {
        name: "farbe".into(),
        art: FeldArt::Auswahl {
            optionen: vec![FeldOption {
                wert: "rot".into(),
                anzeige: "Rot".into(),
            }],
        },
        ..Default::default()
    };
    let json = serde_json::to_string(&auswahl).unwrap();
    assert!(json.contains(r#""auswahl""#), "{json}");
    assert_eq!(serde_json::from_str::<Feld>(&json).unwrap(), auswahl);
}

#[test]
fn altes_feld_ohne_art_und_titel_ist_text() {
    let json = r#"{"name":"pfad","beschreibung":"Ordner","quelle":"client","pflicht":true}"#;
    let feld: Feld = serde_json::from_str(json).unwrap();
    assert_eq!(feld.art, FeldArt::Text);
    assert_eq!(feld.titel, None);
}

/// Kennungen werden Teil jedes Werkzeugnamens (`<kennung>__<werkzeug>`) und
/// müssen deshalb eng bleiben (Paket 3).
#[test]
fn kennungen_sind_eng_und_reserviert() {
    for gut in ["echo", "microsoft-mail", "a", "m365", &"a".repeat(40)] {
        assert!(kennung_gueltig(gut), "{gut}");
        assert_eq!(kennung_fehler(gut), None, "{gut}");
    }
    for schlecht in [
        "",
        "Echo",
        "mit leerzeichen",
        "mit__trenner",
        "quiver",
        "unter_strich",
        &"a".repeat(41),
        "größer",
    ] {
        assert!(!kennung_gueltig(schlecht), "{schlecht}");
        assert!(kennung_fehler(schlecht).is_some(), "{schlecht}");
    }
    // Die Begründungen sind für das Fenster gedacht.
    assert!(kennung_fehler("quiver")
        .unwrap_or_default()
        .contains("reserviert"));
    assert!(kennung_fehler("mit__trenner")
        .unwrap_or_default()
        .contains("__"));
}

/// Aufgabe 37: Nur die eigenen Programme werden verteilt – fremde Läufer
/// (`npx`, `uvx`, `docker`) und Pfade laufen auf dem Gerät wie bisher.
#[test]
fn nur_eigene_namen_werden_verteilt() {
    for verteilt in ["quiver-devops", "quiver-m365", "quiver-ssh_x"] {
        assert!(ist_verteilt(verteilt), "{verteilt}");
    }
    for fremd in [
        "npx",
        "uvx",
        "docker",
        "quiver.devops",
        "/usr/lib/mcp-quiver/tools/quiver-devops",
        "C:\\Programme\\quiver-devops.exe",
        "quiver-",
        "",
    ] {
        assert!(!ist_verteilt(fremd), "{fremd}");
    }
}

/// Aufgabe 37: Der **Name** eines verteilten Programms kommt aus dem Befehl –
/// als bloßer Name oder als Dateiname eines Pfades, ohne `.exe`. So bekommt
/// auch ein Altbestand mit festem Pfad sein Programm, und Server und Client
/// urteilen gleich.
#[test]
fn der_programmname_kommt_aus_dem_befehl() {
    for (befehl, name) in [
        ("quiver-devops", "quiver-devops"),
        ("  quiver-devops  ", "quiver-devops"),
        ("quiver-devops.exe", "quiver-devops"),
        ("/pfad/auf/anderem/rechner/quiver-devops", "quiver-devops"),
        ("/usr/lib/mcp-quiver/tools/quiver-m365", "quiver-m365"),
        (r"C:\Programme\mcp-quiver\quiver-m365.exe", "quiver-m365"),
        ("relativ/quiver-ssh", "quiver-ssh"),
    ] {
        assert_eq!(
            verteiltes_programm(befehl).as_deref(),
            Some(name),
            "{befehl}"
        );
    }
    // Fremde Programme bleiben fremd – auch als Pfad.
    for fremd in [
        "npx",
        "uvx",
        "docker",
        "/opt/eigenes-programm",
        "/pfad/zu/quiver-tools/npx",
        "quiver-",
        "",
        "   ",
        // Ein Name mit Leerzeichen am Rand ist **kein** Programmname (er wird
        // Teil eines Ordners). Im Befehl eines Eintrags sind Leerzeichen
        // dagegen harmlos und werden abgeschnitten.
    ] {
        assert_eq!(verteiltes_programm(fremd), None, "{fremd}");
    }
    for mit_rand in ["quiver-devops ", " quiver-devops"] {
        assert!(!ist_verteilt(mit_rand), "{mit_rand}");
    }
}

/// Aufgabe 37: Ein Programm gehört in die Revision. Ändert sich seine
/// Prüfsumme, ist das eine neue Revision – sonst fragte kein Client nach.
#[test]
fn programme_aendern_die_revision() {
    let geraet = Geraet {
        id: GeraetId::neu("d1"),
        name: "Rechner".into(),
        gruppen: vec![GruppeId::neu("team")],
        gesperrt: false,
    };
    let e = eintrag("echo");
    let gruppen = vec![Gruppe {
        id: GruppeId::neu("team"),
        titel: "Team".into(),
        saetze: vec![SatzId::neu("basis")],
        entra: Vec::new(),
    }];
    let saetze = vec![Satz {
        id: SatzId::neu("basis"),
        titel: "Basis".into(),
        prioritaet: 0,
        eintraege: vec![SatzEintrag {
            eintrag: e.id.clone(),
            werte: [("tenant".to_string(), "acme".to_string())].into(),
            fest: Default::default(),
        }],
    }];
    let soll = soll_berechnen(&geraet, &gruppen, &saetze, &[e]).expect("Soll");
    let programm = |sha: &str| Programm {
        name: "quiver-devops".into(),
        version: "1.0.0".into(),
        groesse: 10,
        sha256: sha.into(),
        system: "linux".into(),
        arch: "x64".into(),
    };
    let ohne = soll.clone().mit_programmen(Vec::new());
    let mit = soll.clone().mit_programmen(vec![programm("aa")]);
    let anders = soll.clone().mit_programmen(vec![programm("bb")]);
    assert_ne!(soll.revision, mit.revision);
    assert_ne!(mit.revision, anders.revision);
    assert_eq!(ohne.programme.len(), 0);
    assert!(mit.programm("quiver-devops").is_some());
    assert!(mit.programm("quiver-gibtsnicht").is_none());
    // Ein Soll ohne Programme ist lesbar (älterer Server).
    let alt: Soll = serde_json::from_str(r#"{"revision":"r","eintraege":[]}"#).unwrap();
    assert!(alt.programme.is_empty());
}

/// Aufgabe 37: System und Architektur kommen geprüft aus der Abfrage – kein
/// Wert aus dem Netz wird je ein Ordnername.
#[test]
fn system_und_architektur_sind_eng() {
    let dieses = System::dieses();
    assert!(!dieses.system.is_empty());
    assert!(!dieses.arch.is_empty());
    assert_eq!(
        System::aus_teilen(" Linux ", "X64"),
        Some(System {
            system: "linux".into(),
            arch: "x64".into()
        })
    );
    assert_eq!(
        System::aus_teilen("macos", "arm64").map(|s| s.arch),
        Some("arm64".into())
    );
    for schlecht in [
        ("linux", "x86"),
        ("../../etc", "x64"),
        ("linux/../..", "x64"),
        ("", ""),
        ("windows", "amd64"),
    ] {
        assert_eq!(System::aus_teilen(schlecht.0, schlecht.1), None);
    }
}

// ---------------------------------------------------------------- Aufgabe 36

fn werte(paare: &[(&str, &str)]) -> Werte {
    paare
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

fn satz_vorbelegt(id: &str, prio: i32, paare: &[(&str, &str)], fest: &[&str]) -> Satz {
    Satz {
        id: SatzId::neu(id),
        titel: id.into(),
        prioritaet: prio,
        eintraege: vec![SatzEintrag {
            eintrag: EintragId::neu("mail"),
            werte: werte(paare),
            fest: fest.iter().map(|name| name.to_string()).collect(),
        }],
    }
}

/// Ein Vorschlag füllt ein Feld mit Quelle „Gerät“, das am Gerät leer ist.
#[test]
fn vorschlag_fuellt_ein_leeres_geraetefeld() {
    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("pfad", "/opt")],
        &[],
    )]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    assert_eq!(
        soll.eintraege[0].vorgabe("pfad"),
        Some((Vorgabe::Vorschlag, "/opt"))
    );
    let geheim = werte(&[("token", "t0p")]);
    let lz = konfig_zusammensetzen(&soll.eintraege[0], &Werte::new(), &geheim).unwrap();
    match lz {
        Laufzeit::Stdio { befehl, .. } => assert_eq!(befehl, "/opt/tool"),
        _ => panic!("Stdio erwartet"),
    }
}

/// Reihenfolge (Aufgabe 36): fest vom Server → Gerät → Vorschlag → Standard.
#[test]
fn geraet_schlaegt_vorschlag_und_fest_schlaegt_geraet() {
    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("pfad", "/opt")],
        &[],
    )]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    let geheim = werte(&[("token", "t0p")]);
    let lokal = werte(&[("pfad", "/usr/local")]);
    let befehl = |soll: &SollEintrag| match konfig_zusammensetzen(soll, &lokal, &geheim).unwrap() {
        Laufzeit::Stdio { befehl, .. } => befehl,
        _ => panic!("Stdio erwartet"),
    };
    assert_eq!(
        befehl(&soll.eintraege[0]),
        "/usr/local/tool",
        "Gerät gewinnt"
    );

    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("pfad", "/opt")],
        &["pfad"],
    )]);
    let fest = soll_berechnen(&g, &gr, &s, &e).unwrap();
    assert_eq!(
        fest.eintraege[0].vorgabe("pfad"),
        Some((Vorgabe::Fest, "/opt"))
    );
    assert_eq!(befehl(&fest.eintraege[0]), "/opt/tool", "fest gewinnt");
}

/// Ein vorbelegtes Pflichtfeld gilt als gesetzt: der Dialog fragt es nicht ab.
#[test]
fn vorbelegtes_pflichtfeld_fehlt_nicht() {
    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("pfad", "/opt")],
        &[],
    )]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    let fehlt =
        konfig_zusammensetzen(&soll.eintraege[0], &Werte::new(), &Werte::new()).unwrap_err();
    let namen: Vec<_> = fehlt.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(namen, ["token"], "nur das Geheimnis fehlt");
}

/// Geheimes kommt nie vom Server (Aufgabe 36), auch wenn es in alten Daten steht.
#[test]
fn geheimes_wird_nie_vorbelegt() {
    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("token", "leck")],
        &["token"],
    )]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    assert!(!soll.eintraege[0].werte.contains_key("token"));
    assert!(soll.eintraege[0].fest.is_empty());
    let lokal = werte(&[("pfad", "/opt")]);
    let lz =
        konfig_zusammensetzen(&soll.eintraege[0], &lokal, &werte(&[("token", "t0p")])).unwrap();
    match lz {
        Laufzeit::Stdio { umgebung, .. } => assert_eq!(umgebung["TOKEN"], "t0p"),
        _ => panic!("Stdio erwartet"),
    }
}

/// Eine Änderung der Vorgabe ist ein neuer Fingerabdruck – der Client startet
/// neu, ohne dass sich jemand anmeldet.
#[test]
fn geaenderte_vorgabe_aendert_den_fingerabdruck() {
    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("pfad", "/opt")],
        &[],
    )]);
    let alt = soll_berechnen(&g, &gr, &s, &e).unwrap();
    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("pfad", "/opt")],
        &["pfad"],
    )]);
    let neu = soll_berechnen(&g, &gr, &s, &e).unwrap();
    assert_ne!(alt.revision, neu.revision);
    assert_ne!(
        alt.eintraege[0].fingerabdruck(),
        neu.eintraege[0].fingerabdruck()
    );
    assert_eq!(
        abgleichen(
            &neu,
            &BTreeMap::from([(EintragId::neu("mail"), alt.eintraege[0].fingerabdruck())])
        ),
        [Aktion::Neustarten(EintragId::neu("mail"))]
    );
}

/// Die Vorgaben getrennt nach Art – so zeigt sie das Fenster.
#[test]
fn vorgaben_trennen_fest_und_vorschlag() {
    let (g, gr, s, e) = welt(vec![satz_vorbelegt(
        "a",
        0,
        &[("tenant", "x"), ("pfad", "/opt")],
        &["pfad"],
    )]);
    let soll = soll_berechnen(&g, &gr, &s, &e).unwrap();
    let vorgaben = soll.eintraege[0].vorgaben();
    assert_eq!(vorgaben.fest["pfad"], "/opt");
    assert_eq!(vorgaben.vorschlag["tenant"], "x");
    assert!(!vorgaben.hat("token"));
}

fn eintrag_mit_schalter() -> Eintrag {
    let mut e = eintrag("mail");
    e.felder = vec![
        feld("schreiben", Quelle::Client),
        feld("pfad", Quelle::Client),
        feld("token", Quelle::Geheim),
    ];
    e.felder[0].art = FeldArt::Schalter;
    e
}

fn satz_eintrag(werte: &[(&str, &str)], fest: &[&str]) -> SatzEintrag {
    SatzEintrag {
        eintrag: EintragId::neu("mail"),
        werte: self::werte(werte),
        fest: fest.iter().map(|name| name.to_string()).collect(),
    }
}

/// Der Server lehnt Geheimes, feste Schalter auf „an“ und „fest“ ohne Wert ab.
#[test]
fn vorgaben_werden_geprueft() {
    let e = eintrag_mit_schalter();
    assert_eq!(
        vorgaben_pruefen(&e, &satz_eintrag(&[("pfad", "/opt")], &["pfad"])),
        Ok(())
    );
    assert_eq!(
        vorgaben_pruefen(&e, &satz_eintrag(&[("schreiben", "aus")], &["schreiben"])),
        Ok(())
    );
    let geheimnis = vorgaben_pruefen(&e, &satz_eintrag(&[("token", "t0p")], &[])).unwrap_err();
    assert!(geheimnis.contains("geheim"), "{geheimnis}");
    let freigabe =
        vorgaben_pruefen(&e, &satz_eintrag(&[("schreiben", "an")], &["schreiben"])).unwrap_err();
    assert!(freigabe.contains("nur auf „aus“"), "{freigabe}");
    let ohne_wert = vorgaben_pruefen(&e, &satz_eintrag(&[], &["pfad"])).unwrap_err();
    assert!(ohne_wert.contains("keinen Wert"), "{ohne_wert}");
    // Auch als Vorschlag nicht: wer das Feld nie anfasst, hätte sonst still
    // freigegeben. Freigeben tut nur der User.
    assert!(vorgaben_pruefen(&e, &satz_eintrag(&[("schreiben", "an")], &[])).is_err());
    // Ein Schalter, dessen Standard „an“ ist (etwa „Zertifikat prüfen“), lässt
    // sich nicht auf „aus“ vorbelegen – das schwächte die Sicherheit.
    let mut pruefen = eintrag_mit_schalter();
    pruefen.felder[0].standard = Some("an".into());
    assert!(vorgaben_pruefen(&pruefen, &satz_eintrag(&[("schreiben", "aus")], &[])).is_err());
    assert_eq!(
        vorgaben_pruefen(
            &pruefen,
            &satz_eintrag(&[("schreiben", "an")], &["schreiben"])
        ),
        Ok(())
    );
    // Unbekannte Felder (ältere Daten) stören nicht.
    assert_eq!(
        vorgaben_pruefen(&e, &satz_eintrag(&[("gibtsnicht", "x")], &["gibtsnicht"])),
        Ok(())
    );
}

/// Ältere Daten ohne `fest` gelten als Vorschlag; ältere Clients lesen ein
/// Soll mit Vorgaben unverändert.
#[test]
fn alte_daten_ohne_fest_sind_vorschlaege() {
    let satz: Satz = serde_json::from_str(
        r#"{"id":"a","titel":"A","eintraege":[{"eintrag":"mail","werte":{"pfad":"/opt"}}]}"#,
    )
    .unwrap();
    assert!(satz.eintraege[0].fest.is_empty());

    let soll: Soll = serde_json::from_str(
        r#"{"revision":"r","eintraege":[{"eintrag":{"id":"mail","version":"1","titel":"M",
            "herkunft":"eigen","laufzeit":{"art":"stdio","befehl":"echo"}},
            "werte":{"pfad":"/opt"}}]}"#,
    )
    .unwrap();
    assert_eq!(
        soll.eintraege[0].vorgabe("pfad"),
        Some((Vorgabe::Vorschlag, "/opt"))
    );
    // Ein Soll ohne Vorgaben bleibt lesbar.
    let schlicht: SollEintrag = serde_json::from_str(
        r#"{"eintrag":{"id":"mail","version":"1","titel":"M","herkunft":"eigen",
            "laufzeit":{"art":"stdio","befehl":"echo"}},"werte":{}}"#,
    )
    .unwrap();
    assert!(schlicht.vorgaben().fest.is_empty());
}
