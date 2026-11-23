use super::*;

fn feld(name: &str, quelle: Quelle) -> Feld {
    Feld {
        name: name.into(),
        beschreibung: String::new(),
        quelle,
        pflicht: true,
        standard: None,
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
        }],
    }
}

fn welt(saetze: Vec<Satz>) -> (Geraet, Vec<Gruppe>, Vec<Satz>, Vec<Eintrag>) {
    let gruppe = Gruppe {
        id: GruppeId::neu("team"),
        titel: "Team".into(),
        saetze: saetze.iter().map(|s| s.id.clone()).collect(),
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
    assert_eq!(tool_name_zerlegen(&n), Some((id, "send_mail")));
    assert_eq!(tool_name_zerlegen("ohne"), None);
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
