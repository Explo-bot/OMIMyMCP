// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Test degli strumenti (`omi_core::service`) sullo scenario sintetico, senza HTTP: JSON completo
//! (ordine delle chiavi e valori) e messaggi d'errore.

mod support;

use std::path::PathBuf;
use std::sync::OnceLock;

use omi_core::about::attribuzione;
use omi_core::service::*;
use omi_core::{OmiError, Result};
use rusqlite::Connection;
use tempfile::TempDir;

/// Database dello scenario, costruito una volta per binario di test.
fn scenario() -> &'static PathBuf {
    static DB: OnceLock<(TempDir, PathBuf)> = OnceLock::new();
    &DB.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let db = support::scenario_server(dir.path());
        (dir, db)
    })
    .1
}

fn conn() -> Connection {
    omi_core::db::open(scenario(), true).unwrap()
}

/// Database in memoria con il solo schema.
fn vuoto() -> Connection {
    let c = Connection::open_in_memory().unwrap();
    omi_core::db::ensure_schema(&c).unwrap();
    c
}

fn errore(r: Result<String>) -> String {
    match r {
        Err(OmiError::Validazione(m)) => m,
        other => panic!("atteso un errore di validazione, ottenuto {other:?}"),
    }
}

fn s(v: &str) -> Option<String> {
    Some(v.to_string())
}

/// Per le prove manuali del server: crea il database dello scenario nella cartella indicata da
/// `OMI_FIXTURE_DIR` (`cargo test -p omi-core --test service -- --ignored genera_database_di_prova`).
#[test]
#[ignore]
fn genera_database_di_prova() {
    let Ok(dir) = std::env::var("OMI_FIXTURE_DIR") else {
        eprintln!("OMI_FIXTURE_DIR non impostata: nessun database generato");
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let db = support::scenario_server(std::path::Path::new(&dir));
    println!("{}", db.display());
}

const ROMA: &str = r#"{"codice_catastale":"H501","codice_istat":"1058091","nome":"ROMA","provincia":"RM","regione":"LAZIO","area_territoriale":"CENTRO","primo_semestre":"2024/2","ultimo_semestre":"2025/1"}"#;
const INFO_B1: &str = r#"{"zona":"B1","link_zona":"RM00000001","fascia":"B","fascia_descr":"Centrale","descrizione":"ZONA B1 DI ROMA","microzona":1,"tipologia_prevalente":{"codice":20,"descrizione":"Abitazioni civili"},"stato_prevalente":"NORMALE","comune_catastale":"H501"}"#;
const PERIMETRO_B1: &str = r#"{"semestre_perimetro":"2024/2","centro":{"lat":41.899079,"lon":12.495395},"bbox":{"min_lat":41.89,"min_lon":12.48,"max_lat":41.91,"max_lon":12.52},"area_km2":4.3813,"n_poligoni":2,"n_punti":15,"colore_kml":"7f00ff00"}"#;
const VALORI_B1_20242: &str = r#"[{"cod_tipologia":13,"tipologia":"Box","stato":"NORMALE","stato_prevalente":true,"compravendita_eur_mq":{"min":1800,"max":2500,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":8,"max":11,"superficie":"lorda"}},{"cod_tipologia":13,"tipologia":"Box","stato":"SCADENTE","compravendita_eur_mq":{"min":1200,"max":1600,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":5.5,"max":7,"superficie":"lorda"}},{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"NORMALE","stato_prevalente":true,"compravendita_eur_mq":{"min":3500,"max":5200.5,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":12.5,"max":18.3,"superficie":"lorda"}},{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"OTTIMO","compravendita_eur_mq":{"min":5000,"max":7000,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":16,"max":22,"superficie":"lorda"}}]"#;
const VALORI_B1_20251: &str = r#"[{"cod_tipologia":13,"tipologia":"Box","stato":"NORMALE","stato_prevalente":true,"compravendita_eur_mq":{"min":1850,"max":2550,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":8.2,"max":11.2,"superficie":"lorda"}},{"cod_tipologia":13,"tipologia":"Box","stato":"SCADENTE","compravendita_eur_mq":{"min":1250,"max":1650,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":5.6,"max":7.1,"superficie":"lorda"}},{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"NORMALE","stato_prevalente":true,"compravendita_eur_mq":{"min":3600,"max":5300,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":12.8,"max":18.6,"superficie":"lorda"}},{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"OTTIMO","compravendita_eur_mq":{"min":5100,"max":7100,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":16.5,"max":22.5,"superficie":"lorda"}}]"#;
const INFO_D5: &str = r#"{"zona":"D5","link_zona":"RM00000005","fascia":"D","fascia_descr":"Periferica","descrizione":"ZONA D5 DI ROMA","microzona":1,"tipologia_prevalente":{"codice":20,"descrizione":"Abitazioni civili"},"stato_prevalente":"NORMALE","comune_catastale":"H501"}"#;
const VALORI_D5: &str = r#"[{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"NORMALE","stato_prevalente":true,"compravendita_eur_mq":{"min":2000,"max":2800,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":7.5,"max":10,"superficie":"lorda"}}]"#;
const NOTA_ALTRO_SEMESTRE: &str = "Perimetri non disponibili per il semestre richiesto: usati quelli del semestre più vicino, collegati per codice zona.";

// ---------------------------------------------------------------------------------------------
// elenco_semestri, elenco_tipologie
// ---------------------------------------------------------------------------------------------

#[test]
fn elenco_semestri() {
    let c = conn();
    let atteso = [
        r#"{"semestri":[{"semestre":"2025/1","n_zone":6,"n_valori":9,"elaborazione":"12/03/2026","file_csv":"QI_20251.zip"},"#,
        r#"{"semestre":"2024/2","n_zone":6,"n_valori":9,"n_perimetri":3,"elaborazione":"12/03/2026","file_csv":"QIP_20242.zip","file_kml":"QIP_20242.zip"}],"#,
        r#""nota":"I perimetri (KML) sono disponibili solo per alcuni semestri; per gli altri la ricerca per coordinate usa i perimetri del semestre più vicino.","#,
        r#""fonte_dati":"Agenzia Entrate - OMI","software":""#,
        &attribuzione(),
        r#""}"#,
    ]
    .concat();
    assert_eq!(Service::new(&c).elenco_semestri().unwrap(), atteso);
}

#[test]
fn elenco_tipologie_mantiene_i_null() {
    let c = conn();
    assert_eq!(
        Service::new(&c).elenco_tipologie().unwrap(),
        r#"[{"codice":13,"descrizione":"Box"},{"codice":20,"descrizione":"Abitazioni civili"}]"#
    );
    let v = vuoto();
    v.execute_batch(
        "INSERT INTO tipologia VALUES (99, NULL); INSERT INTO tipologia VALUES (1, 'Uffici');",
    )
    .unwrap();
    assert_eq!(
        Service::new(&v).elenco_tipologie().unwrap(),
        r#"[{"codice":1,"descrizione":"Uffici"},{"codice":99,"descrizione":null}]"#
    );
    assert_eq!(Service::new(&vuoto()).elenco_semestri().unwrap(), {
        let mut a = r#"{"semestri":[],"nota":"I perimetri (KML) sono disponibili solo per alcuni semestri; per gli altri la ricerca per coordinate usa i perimetri del semestre più vicino.","fonte_dati":"Agenzia Entrate - OMI","software":""#.to_string();
        a.push_str(&attribuzione());
        a.push_str("\"}");
        a
    });
}

// ---------------------------------------------------------------------------------------------
// cerca_comuni
// ---------------------------------------------------------------------------------------------

fn cerca(testo: &str, provincia: Option<&str>) -> String {
    let c = conn();
    Service::new(&c)
        .cerca_comuni(&CercaComuni {
            testo: testo.into(),
            provincia: provincia.map(String::from),
        })
        .unwrap()
}

#[test]
fn cerca_comuni_omonimi() {
    assert_eq!(
        cerca("samone", None),
        r#"{"trovati":2,"comuni":[{"codice_catastale":"H753","codice_istat":"1001235","nome":"SAMONE","provincia":"TO","regione":"PIEMONTE","area_territoriale":"CENTRO","primo_semestre":"2024/2","ultimo_semestre":"2025/1"},{"codice_catastale":"H754","codice_istat":"4022165","nome":"SAMONE","provincia":"TN","regione":"TRENTINO-ALTO ADIGE","area_territoriale":"CENTRO","primo_semestre":"2024/2","ultimo_semestre":"2025/1"}]}"#
    );
    // Filtro per provincia, senza distinzione di maiuscole.
    let tn = cerca(" Samone ", Some(" tn "));
    assert!(
        tn.starts_with(r#"{"trovati":1,"comuni":[{"codice_catastale":"H754""#),
        "{tn}"
    );
}

#[test]
fn cerca_comuni_per_codici_e_nome_parziale() {
    let roma = format!(r#"{{"trovati":1,"comuni":[{ROMA}]}}"#);
    assert_eq!(cerca("h501", None), roma);
    assert_eq!(cerca("1058091", None), roma);
    // Codice ISTAT a 6 cifre senza zeri iniziali.
    assert_eq!(cerca("58091", None), roma);
    assert_eq!(cerca("Róma", None), roma);
    // Prefisso, poi sottostringa.
    assert!(cerca("sam", None).starts_with(r#"{"trovati":2,"#));
    assert!(cerca("ilan", None).contains(r#""nome":"MILANO""#));
    assert_eq!(cerca("atlantide", None), r#"{"trovati":0,"comuni":[]}"#);
    assert_eq!(cerca("roma", Some("MI")), r#"{"trovati":0,"comuni":[]}"#);
}

#[test]
fn cerca_comuni_nome_storico() {
    // BORGO VECCHIO è il nome del 2024/2: si trova il comune con il nome attuale.
    assert_eq!(
        cerca("Borgo Vecchio", None),
        r#"{"trovati":1,"comuni":[{"codice_catastale":"B999","codice_istat":"1001999","nome":"BORGO NUOVO","provincia":"TO","regione":"PIEMONTE","area_territoriale":"CENTRO","primo_semestre":"2024/2","ultimo_semestre":"2025/1"}]}"#
    );
}

// ---------------------------------------------------------------------------------------------
// zone_comune
// ---------------------------------------------------------------------------------------------

#[test]
fn zone_comune_con_valori_e_perimetri() {
    let c = conn();
    let r = Service::new(&c)
        .zone_comune(&ZoneComune {
            comune: "roma".into(),
            includi_valori: true,
            includi_perimetri: true,
            ..Default::default()
        })
        .unwrap();
    let atteso = [
        r#"{"comune":"#,
        ROMA,
        r#","semestre":"2025/1","n_zone":2,"zone":[{"info":"#,
        INFO_B1,
        r#","perimetro":"#,
        PERIMETRO_B1,
        r#","valori":"#,
        VALORI_B1_20251,
        r#"},{"info":"#,
        INFO_D5,
        r#","valori":"#,
        VALORI_D5,
        "}]}",
    ]
    .concat();
    assert_eq!(r, atteso);
}

#[test]
fn zone_comune_semplice_e_nome_storico() {
    let c = conn();
    let svc = Service::new(&c);
    let r = svc
        .zone_comune(&ZoneComune {
            comune: "ROMA".into(),
            semestre: s("2024-2"),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        r,
        [
            r#"{"comune":"#,
            ROMA,
            r#","semestre":"2024/2","n_zone":2,"zone":[{"info":"#,
            INFO_B1,
            r#"},{"info":{"zona":"C2","link_zona":"RM00000002","fascia":"C","fascia_descr":"Semicentrale","descrizione":"ZONA C2 DI ROMA","microzona":1,"tipologia_prevalente":{"codice":20,"descrizione":"Abitazioni civili"},"stato_prevalente":"NORMALE","comune_catastale":"H501"}}]}"#,
        ]
        .concat()
    );
    let r = svc
        .zone_comune(&ZoneComune {
            comune: "borgo vecchio".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(r.contains(r#""nome":"BORGO NUOVO""#) && r.contains(r#""n_zone":1"#));
}

#[test]
fn zone_comune_errori_e_precedenza() {
    let c = conn();
    let svc = Service::new(&c);
    let zc = |comune: &str, provincia: Option<&str>, semestre: Option<&str>| {
        svc.zone_comune(&ZoneComune {
            comune: comune.into(),
            provincia: provincia.map(String::from),
            semestre: semestre.map(String::from),
            ..Default::default()
        })
    };
    assert_eq!(
        errore(zc("samone", None, None)),
        "'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754]"
    );
    assert!(zc("samone", Some("TN"), None).unwrap().contains("H754"));
    assert!(
        zc("H753", None, None)
            .unwrap()
            .contains(r#""provincia":"TO""#)
    );
    assert_eq!(
        errore(zc("Atlantide", None, None)),
        "Nessun comune trovato per 'Atlantide'."
    );
    assert_eq!(
        errore(zc("Roma", Some("MI"), None)),
        "Nessun comune trovato per 'Roma' in provincia MI."
    );
    // Il comune si risolve prima del semestre.
    assert_eq!(
        errore(zc("samone", None, Some("2025/3"))),
        "'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754]"
    );
    assert_eq!(
        errore(zc("roma", None, Some("2025/3"))),
        "Semestre non valido: '2025/3'. Usare il formato AAAA/S, es. 2025/1."
    );
    assert_eq!(
        errore(zc("roma", None, Some("2023/1"))),
        "Semestre 2023/1 non presente nel database. Usare elenco_semestri."
    );
}

// ---------------------------------------------------------------------------------------------
// dettaglio_zona
// ---------------------------------------------------------------------------------------------

#[test]
fn dettaglio_zona_con_geometria() {
    let c = conn();
    let r = Service::new(&c)
        .dettaglio_zona(&DettaglioZona {
            comune: "roma".into(),
            zona: " b1 ".into(),
            semestre: s("2024/2"),
            includi_geometria: true,
            ..Default::default()
        })
        .unwrap();
    let perimetro = PERIMETRO_B1.trim_end_matches('}').to_string()
        + r#","geometria":{"type":"MultiPolygon","coordinates":[[[[12.48,41.89],[12.5,41.89],[12.5,41.91],[12.48,41.91],[12.48,41.89]],[[12.485,41.895],[12.49,41.895],[12.49,41.9],[12.485,41.9],[12.485,41.895]]],[[[12.51,41.89],[12.52,41.89],[12.52,41.9],[12.51,41.9],[12.51,41.89]]]]}}"#;
    let atteso = [
        r#"{"comune":"#,
        ROMA,
        r#","semestre":"2024/2","zone":[{"info":"#,
        INFO_B1,
        r#","valori":"#,
        VALORI_B1_20242,
        r#","perimetro":"#,
        &perimetro,
        "}]}",
    ]
    .concat();
    assert_eq!(r, atteso);
}

#[test]
fn dettaglio_zona_per_link_zona_senza_perimetro() {
    let c = conn();
    // Trovata per LinkZona: il comune non viene risolto (qui non esisterebbe) e si legge dalla zona.
    let r = Service::new(&c)
        .dettaglio_zona(&DettaglioZona {
            comune: "xxx".into(),
            zona: "rm00000005".into(),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        r,
        [
            r#"{"comune":"#,
            ROMA,
            r#","semestre":"2025/1","zone":[{"info":"#,
            INFO_D5,
            r#","valori":"#,
            VALORI_D5,
            "}]}"
        ]
        .concat()
    );
}

#[test]
fn dettaglio_zona_errori() {
    let c = conn();
    let svc = Service::new(&c);
    let dz = |comune: &str, zona: &str, semestre: Option<&str>| {
        svc.dettaglio_zona(&DettaglioZona {
            comune: comune.into(),
            zona: zona.into(),
            semestre: semestre.map(String::from),
            ..Default::default()
        })
    };
    assert_eq!(
        errore(dz("roma", "Z9", None)),
        "Zona 'Z9' non trovata per il comune indicato nel semestre 2025/1. Usare zone_comune per l'elenco."
    );
    // C2 non esiste nel 2025/1.
    assert_eq!(
        errore(dz("roma", "C2", None)),
        "Zona 'C2' non trovata per il comune indicato nel semestre 2025/1. Usare zone_comune per l'elenco."
    );
    // Qui il semestre si risolve prima del comune.
    assert_eq!(
        errore(dz("samone", "B1", Some("1999/1"))),
        "Semestre 1999/1 non presente nel database. Usare elenco_semestri."
    );
    assert_eq!(
        errore(dz("samone", "B1", None)),
        "'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754]"
    );
}

// ---------------------------------------------------------------------------------------------
// zona_da_coordinate
// ---------------------------------------------------------------------------------------------

fn coord(lat: f64, lon: f64, semestre: Option<&str>) -> Result<String> {
    let c = conn();
    Service::new(&c).zona_da_coordinate(&ZonaDaCoordinate {
        latitudine: lat,
        longitudine: lon,
        semestre: semestre.map(String::from),
        includi_geometria: false,
    })
}

#[test]
fn zona_da_coordinate_punto_interno() {
    // Nel 2024/2 i perimetri sono dello stesso semestre: nessuna nota.
    let atteso = [
        r#"{"trovato":true,"lat":41.905,"lon":12.495,"semestre":"2024/2","semestre_perimetri":"2024/2","risultati":[{"comune":"#,
        ROMA,
        r#","zona":"B1","dati":[{"info":"#,
        INFO_B1,
        r#","valori":"#,
        VALORI_B1_20242,
        r#","perimetro":"#,
        PERIMETRO_B1,
        "}]}]}",
    ]
    .concat();
    assert_eq!(coord(41.905, 12.495, Some("2024/2")).unwrap(), atteso);
}

#[test]
fn zona_da_coordinate_semestre_senza_perimetri() {
    let atteso = [
        r#"{"trovato":true,"lat":41.905,"lon":12.495,"semestre":"2025/1","semestre_perimetri":"2024/2","nota":""#,
        NOTA_ALTRO_SEMESTRE,
        r#"","risultati":[{"comune":"#,
        ROMA,
        r#","zona":"B1","dati":[{"info":"#,
        INFO_B1,
        r#","valori":"#,
        VALORI_B1_20251,
        r#","perimetro":"#,
        PERIMETRO_B1,
        "}]}]}",
    ]
    .concat();
    assert_eq!(coord(41.905, 12.495, None).unwrap(), atteso);
}

#[test]
fn zona_da_coordinate_punto_vicino_approssimato() {
    // Il punto cade nel buco del primo poligono di B1, a circa 207 m dal bordo.
    let atteso = [
        r#"{"trovato":true,"lat":41.8975,"lon":12.4875,"semestre":"2024/2","semestre_perimetri":"2024/2","approssimato":true,"nota":"Il punto non ricade in alcun perimetro OMI: restituita la zona più vicina.","risultati":[{"comune":"#,
        ROMA,
        r#","zona":"B1","distanza_m":207,"dati":[{"info":"#,
        INFO_B1,
        r#","valori":"#,
        VALORI_B1_20242,
        r#","perimetro":"#,
        PERIMETRO_B1,
        "}]}]}",
    ]
    .concat();
    assert_eq!(coord(41.8975, 12.4875, Some("20242")).unwrap(), atteso);
}

#[test]
fn zona_da_coordinate_codice_zona_cambiato() {
    let atteso = [
        r#"{"trovato":true,"lat":41.825,"lon":12.425,"semestre":"2025/1","semestre_perimetri":"2024/2","nota":""#,
        NOTA_ALTRO_SEMESTRE,
        r#"","risultati":[{"comune":"#,
        ROMA,
        r#","zona":"C2","dati":[],"nota":"La zona C2 individuata dal perimetro non esiste nei dati del semestre 2025/1 (codici zona cambiati tra le revisioni)."}]}"#,
    ]
    .concat();
    assert_eq!(coord(41.825, 12.425, None).unwrap(), atteso);
}

#[test]
fn zona_da_coordinate_oltre_2_km() {
    assert_eq!(
        coord(45.0, 10.0, None).unwrap(),
        r#"{"trovato":false,"lat":45,"lon":10,"semestre":"2025/1","messaggio":"Nessuna zona OMI contiene il punto né si trova entro 2 km."}"#
    );
    // Circa 2,7 km a est di Milano B1: oltre la soglia anche se dentro il margine di 0,02°.
    assert!(
        coord(45.465, 9.235, None)
            .unwrap()
            .starts_with(r#"{"trovato":false"#)
    );
}

#[test]
fn zona_da_coordinate_errori() {
    assert_eq!(
        errore(coord(41.9, 12.5, Some("2025/9"))),
        "Semestre non valido: '2025/9'. Usare il formato AAAA/S, es. 2025/1."
    );
    let v = vuoto();
    assert_eq!(
        errore(Service::new(&v).zona_da_coordinate(&ZonaDaCoordinate {
            latitudine: 41.9,
            longitudine: 12.5,
            ..Default::default()
        })),
        "Il database non contiene dati OMI."
    );
    // Database con il solo QI (nessun KML).
    let dir = tempfile::tempdir().unwrap();
    let db = support::importa(
        dir.path(),
        "qi.db",
        &[("QI_20251.zip", support::scenario_qi_ultimo())],
    );
    let c = omi_core::db::open(&db, true).unwrap();
    let svc = Service::new(&c);
    assert_eq!(
        errore(svc.zona_da_coordinate(&ZonaDaCoordinate {
            latitudine: 41.9,
            longitudine: 12.5,
            ..Default::default()
        })),
        "Nel database non sono presenti perimetri di zona (file KML)."
    );
    assert_eq!(
        errore(svc.zone_vicine(&ZoneVicine {
            latitudine: 41.9,
            longitudine: 12.5,
            ..Default::default()
        })),
        "Nel database non sono presenti perimetri di zona (file KML)."
    );
}

// ---------------------------------------------------------------------------------------------
// zone_vicine
// ---------------------------------------------------------------------------------------------

#[test]
fn zone_vicine_raggio_oltre_il_massimo() {
    let c = conn();
    let r = Service::new(&c)
        .zone_vicine(&ZoneVicine {
            latitudine: 41.905,
            longitudine: 12.495,
            raggio_m: Some(50000.0),
            semestre: None,
        })
        .unwrap();
    // C2 non esiste nel 2025/1: restano comune, codice, zona, LinkZona del perimetro e distanza.
    assert_eq!(
        r,
        r#"{"lat":41.905,"lon":12.495,"raggio_m":20000,"semestre":"2025/1","semestre_perimetri":"2024/2","n":2,"zone":[{"comune":"ROMA (RM)","codice_catastale":"H501","zona":"B1","link_zona":"RM00000001","fascia":"B","descrizione":"ZONA B1 DI ROMA","distanza_m":0,"abitazioni_civili":{"stato":"NORMALE","compr_min":3600,"compr_max":5300,"loc_min":12.8,"loc_max":18.6}},{"comune":"ROMA (RM)","codice_catastale":"H501","zona":"C2","link_zona":"RM00000002","distanza_m":7132}]}"#
    );
}

#[test]
fn zone_vicine_raggio_predefinito_e_negativo() {
    let c = conn();
    let svc = Service::new(&c);
    let r = svc
        .zone_vicine(&ZoneVicine {
            latitudine: 41.905,
            longitudine: 12.495,
            raggio_m: None,
            semestre: s("2024/2"),
        })
        .unwrap();
    assert_eq!(
        r,
        r#"{"lat":41.905,"lon":12.495,"raggio_m":1000,"semestre":"2024/2","semestre_perimetri":"2024/2","n":1,"zone":[{"comune":"ROMA (RM)","codice_catastale":"H501","zona":"B1","link_zona":"RM00000001","fascia":"B","descrizione":"ZONA B1 DI ROMA","distanza_m":0,"abitazioni_civili":{"stato":"NORMALE","compr_min":3500,"compr_max":5200.5,"loc_min":12.5,"loc_max":18.3}}]}"#
    );
    // Raggio negativo → 0: restano solo le zone che contengono il punto.
    let r = svc
        .zone_vicine(&ZoneVicine {
            latitudine: 41.8975,
            longitudine: 12.4875,
            raggio_m: Some(-5.0),
            semestre: None,
        })
        .unwrap();
    assert!(
        r.contains(r#""raggio_m":0,"#) && r.contains(r#""n":0,"zone":[]"#),
        "{r}"
    );
}

// ---------------------------------------------------------------------------------------------
// storico_zona
// ---------------------------------------------------------------------------------------------

#[test]
fn storico_zona_stato_in_minuscolo() {
    let c = conn();
    let r = Service::new(&c)
        .storico_zona(&StoricoZona {
            comune: "roma".into(),
            zona: "b1".into(),
            stato: s(" normale "),
            ..Default::default()
        })
        .unwrap();
    let atteso = [
        r#"{"comune":"#,
        ROMA,
        r#","zona":"B1","descrizioni_zona":[{"descrizione":"ZONA B1 DI ROMA","dal":"2024/2","al":"2025/1"}],"n_serie":2,"serie":["#,
        r#"{"cod_tipologia":13,"tipologia":"Box","stato":"NORMALE","valori":[{"semestre":"2024/2","compr_min":1800,"compr_max":2500,"loc_min":8,"loc_max":11,"prevalente":true},{"semestre":"2025/1","compr_min":1850,"compr_max":2550,"loc_min":8.2,"loc_max":11.2,"prevalente":true}]},"#,
        r#"{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"NORMALE","valori":[{"semestre":"2024/2","compr_min":3500,"compr_max":5200.5,"loc_min":12.5,"loc_max":18.3,"prevalente":true},{"semestre":"2025/1","compr_min":3600,"compr_max":5300,"loc_min":12.8,"loc_max":18.6,"prevalente":true}]}],"#,
        r#""nota":"I codici zona OMI possono essere stati ridefiniti nelle revisioni della zonizzazione: confrontare le descrizioni."}"#,
    ]
    .concat();
    assert_eq!(r, atteso);
}

#[test]
fn storico_zona_filtri_e_link_zona() {
    let c = conn();
    let svc = Service::new(&c);
    // Per LinkZona e tipologia: una sola serie, senza "prevalente" per lo stato OTTIMO.
    let r = svc
        .storico_zona(&StoricoZona {
            comune: "H501".into(),
            zona: "rm00000001".into(),
            cod_tipologia: Some(20),
            stato: s("OTTIMO"),
            ..Default::default()
        })
        .unwrap();
    assert!(
        r.contains(r#""zona":"RM00000001","#)
            && r.contains(r#""n_serie":1,"serie":[{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"OTTIMO","valori":[{"semestre":"2024/2","compr_min":5000,"compr_max":7000,"loc_min":16,"loc_max":22},{"semestre":"2025/1""#),
        "{r}"
    );
    let r = svc
        .storico_zona(&StoricoZona {
            comune: "roma".into(),
            zona: "Z9".into(),
            ..Default::default()
        })
        .unwrap();
    assert!(
        r.ends_with(r#","zona":"Z9","descrizioni_zona":[],"n_serie":0,"serie":[],"nota":"Nessun dato: verificare codice zona (zone_comune) o filtri."}"#),
        "{r}"
    );
    assert_eq!(
        errore(svc.storico_zona(&StoricoZona {
            comune: "samone".into(),
            zona: "B1".into(),
            ..Default::default()
        })),
        "'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754]"
    );
}

// ---------------------------------------------------------------------------------------------
// statistiche_comune
// ---------------------------------------------------------------------------------------------

#[test]
fn statistiche_comune() {
    let c = conn();
    let svc = Service::new(&c);
    let r = svc
        .statistiche_comune(&StatisticheComune {
            comune: "roma".into(),
            ..Default::default()
        })
        .unwrap();
    let atteso = [
        r#"{"comune":"#,
        ROMA,
        r#","semestre":"2025/1","nota":"Valori €/m² (locazione €/m² mese) calcolati sullo stato conservativo prevalente di ciascuna zona.","#,
        r#""per_tipologia":[{"cod_tipologia":13,"tipologia":"Box","n_zone":1,"compravendita":{"min":1850,"max":2550,"medio":2200},"locazione":{"min":8.2,"max":11.2,"medio":9.7}},"#,
        r#"{"cod_tipologia":20,"tipologia":"Abitazioni civili","n_zone":2,"compravendita":{"min":2000,"max":5300,"medio":3425},"locazione":{"min":7.5,"max":18.6,"medio":12.23}}],"#,
        r#""per_fascia":[{"fascia":"B","fascia_descr":"Centrale","n_zone":1,"abitazioni_civili_compr_medio":4450,"abitazioni_civili_loc_medio":15.7},"#,
        r#"{"fascia":"D","fascia_descr":"Periferica","n_zone":1,"abitazioni_civili_compr_medio":2400,"abitazioni_civili_loc_medio":8.75}]}"#,
    ]
    .concat();
    assert_eq!(r, atteso);
    assert_eq!(
        errore(svc.statistiche_comune(&StatisticheComune {
            comune: "samone".into(),
            semestre: s("bad"),
            ..Default::default()
        })),
        "'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754]"
    );
    assert_eq!(
        errore(svc.statistiche_comune(&StatisticheComune {
            comune: "samone".into(),
            provincia: s("to"),
            semestre: s("bad"),
        })),
        "Semestre non valido: 'bad'. Usare il formato AAAA/S, es. 2025/1."
    );
}
