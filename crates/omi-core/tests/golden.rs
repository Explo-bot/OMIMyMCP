// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Risultati di riferimento: per ogni caso, il testo prodotto dallo strumento sul
//! database sintetico (`support::scenario_server`) deve coincidere con il file
//! `tests/golden/{caso}.json`, oppure il messaggio d'errore con `tests/golden/{caso}.errore.txt`.
//! I file attesi sono scritti a mano: vedi `tests/golden/LEGGIMI.md`.

mod support;

use std::path::{Path, PathBuf};

use omi_core::about::{URL_PROGETTO, attribuzione};
use omi_core::service::*;
use omi_core::{OmiError, Result};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

/// `(caso, strumento, argomenti)`: gli argomenti passano dagli stessi tipi del server MCP.
fn casi() -> Vec<(&'static str, &'static str, Value)> {
    vec![
        ("elenco_semestri", "elenco_semestri", json!({})),
        ("elenco_tipologie", "elenco_tipologie", json!({})),
        // cerca_comuni
        (
            "cerca_comuni_nome_esatto",
            "cerca_comuni",
            json!({"testo": "Roma"}),
        ),
        (
            "cerca_comuni_prefisso",
            "cerca_comuni",
            json!({"testo": "bor"}),
        ),
        (
            "cerca_comuni_parte_del_nome",
            "cerca_comuni",
            json!({"testo": "ilan"}),
        ),
        (
            "cerca_comuni_codice_catastale",
            "cerca_comuni",
            json!({"testo": " h501 "}),
        ),
        (
            "cerca_comuni_istat_con_prefisso",
            "cerca_comuni",
            json!({"testo": "1015146"}),
        ),
        (
            "cerca_comuni_istat_senza_prefisso",
            "cerca_comuni",
            json!({"testo": "015146"}),
        ),
        (
            "cerca_comuni_istat_senza_zeri",
            "cerca_comuni",
            json!({"testo": "15146"}),
        ),
        (
            "cerca_comuni_ambiguo",
            "cerca_comuni",
            json!({"testo": "samone"}),
        ),
        (
            "cerca_comuni_ambiguo_con_provincia",
            "cerca_comuni",
            json!({"testo": "Samone", "provincia": "tn"}),
        ),
        (
            "cerca_comuni_accenti",
            "cerca_comuni",
            json!({"testo": "Mìlàno"}),
        ),
        (
            "cerca_comuni_nome_storico",
            "cerca_comuni",
            json!({"testo": "Borgo-Vecchio"}),
        ),
        (
            "cerca_comuni_nessuno",
            "cerca_comuni",
            json!({"testo": "Atlantide"}),
        ),
        // zone_comune
        (
            "zone_comune_semplice",
            "zone_comune",
            json!({"comune": "roma"}),
        ),
        (
            "zone_comune_valori_e_perimetri",
            "zone_comune",
            json!({"comune": "H501", "semestre": "2024/2", "includi_valori": true, "includi_perimetri": "true"}),
        ),
        (
            "zone_comune_semestre_non_valido",
            "zone_comune",
            json!({"comune": "roma", "semestre": "2025/3"}),
        ),
        (
            "zone_comune_semestre_assente",
            "zone_comune",
            json!({"comune": "roma", "semestre": "2020/1"}),
        ),
        (
            "zone_comune_comune_prima_del_semestre",
            "zone_comune",
            json!({"comune": "Atlantide", "provincia": "RM", "semestre": "2025/3"}),
        ),
        // dettaglio_zona
        (
            "dettaglio_zona_per_codice",
            "dettaglio_zona",
            json!({"comune": "roma", "zona": "b1"}),
        ),
        (
            "dettaglio_zona_per_link_zona",
            "dettaglio_zona",
            json!({"comune": "roma", "zona": "rm00000005"}),
        ),
        (
            "dettaglio_zona_comune_ambiguo",
            "dettaglio_zona",
            json!({"comune": "samone", "zona": "R1"}),
        ),
        (
            "dettaglio_zona_ambiguo_con_provincia",
            "dettaglio_zona",
            json!({"comune": "samone", "provincia": "TN", "zona": "R1"}),
        ),
        (
            "dettaglio_zona_inesistente",
            "dettaglio_zona",
            json!({"comune": "roma", "zona": "Z9"}),
        ),
        (
            "dettaglio_zona_con_geometria",
            "dettaglio_zona",
            json!({"comune": "roma", "zona": "B1", "semestre": "2024/2", "includi_geometria": true}),
        ),
        // zona_da_coordinate
        (
            "zona_da_coordinate_punto_interno",
            "zona_da_coordinate",
            json!({"latitudine": 41.905, "longitudine": 12.495, "semestre": "2024/2"}),
        ),
        (
            "zona_da_coordinate_entro_2_km",
            "zona_da_coordinate",
            json!({"latitudine": "41.895", "longitudine": "12.503", "semestre": "2024/2"}),
        ),
        (
            "zona_da_coordinate_lontano",
            "zona_da_coordinate",
            json!({"latitudine": 40, "longitudine": 15}),
        ),
        (
            "zona_da_coordinate_semestre_senza_perimetri",
            "zona_da_coordinate",
            json!({"latitudine": 41.905, "longitudine": 12.495}),
        ),
        (
            "zona_da_coordinate_zona_cambiata",
            "zona_da_coordinate",
            json!({"latitudine": 41.825, "longitudine": 12.425}),
        ),
        // zone_vicine
        (
            "zone_vicine_raggio_normale",
            "zone_vicine",
            json!({"latitudine": 41.905, "longitudine": 12.495, "raggio_m": 10000}),
        ),
        (
            "zone_vicine_raggio_oltre_il_massimo",
            "zone_vicine",
            json!({"latitudine": 41.905, "longitudine": 12.495, "raggio_m": "50000", "semestre": "2024/2"}),
        ),
        // storico_zona
        (
            "storico_zona_tutte_le_serie",
            "storico_zona",
            json!({"comune": "roma", "zona": "B1"}),
        ),
        (
            "storico_zona_per_tipologia",
            "storico_zona",
            json!({"comune": "roma", "zona": "b1", "cod_tipologia": "13"}),
        ),
        (
            "storico_zona_stato_minuscolo",
            "storico_zona",
            json!({"comune": "roma", "zona": "B1", "stato": " ottimo "}),
        ),
        (
            "storico_zona_per_link_zona",
            "storico_zona",
            json!({"comune": "roma", "zona": "RM00000002"}),
        ),
        (
            "storico_zona_descrizioni_cambiate",
            "storico_zona",
            json!({"comune": "borgo vecchio", "zona": "B1"}),
        ),
        (
            "storico_zona_inesistente",
            "storico_zona",
            json!({"comune": "roma", "zona": "Z9"}),
        ),
        // statistiche_comune
        (
            "statistiche_comune_semestre_predefinito",
            "statistiche_comune",
            json!({"comune": "roma"}),
        ),
        (
            "statistiche_comune_semestre_esplicito",
            "statistiche_comune",
            json!({"comune": "roma", "semestre": "2024-2"}),
        ),
    ]
}

fn parametri<T: DeserializeOwned>(args: Value) -> T {
    serde_json::from_value(args).unwrap()
}

fn chiama(svc: &Service, tool: &str, args: Value) -> Result<String> {
    match tool {
        "elenco_semestri" => svc.elenco_semestri(),
        "elenco_tipologie" => svc.elenco_tipologie(),
        "cerca_comuni" => svc.cerca_comuni(&parametri(args)),
        "zone_comune" => svc.zone_comune(&parametri(args)),
        "dettaglio_zona" => svc.dettaglio_zona(&parametri(args)),
        "zona_da_coordinate" => svc.zona_da_coordinate(&parametri(args)),
        "zone_vicine" => svc.zone_vicine(&parametri(args)),
        "storico_zona" => svc.storico_zona(&parametri(args)),
        "statistiche_comune" => svc.statistiche_comune(&parametri(args)),
        _ => panic!("strumento sconosciuto: {tool}"),
    }
}

fn cartella() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

/// Contenuto atteso senza l'a-capo finale. I file riportano l'attribuzione senza URL del
/// progetto: se è stato impostato a compilazione (`OMI_URL_PROGETTO`) la si sostituisce.
fn leggi(path: &Path) -> String {
    let testo = std::fs::read_to_string(path).unwrap();
    let testo = testo
        .strip_suffix('\n')
        .map(|t| t.strip_suffix('\r').unwrap_or(t))
        .unwrap_or(&testo)
        .to_string();
    if URL_PROGETTO.is_some() {
        testo.replace(
            "OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)",
            &attribuzione(),
        )
    } else {
        testo
    }
}

/// Prima posizione (in caratteri) in cui i due testi differiscono, con un po' di contesto.
fn differenza(atteso: &str, ottenuto: &str) -> String {
    let a: Vec<char> = atteso.chars().collect();
    let o: Vec<char> = ottenuto.chars().collect();
    let i = a.iter().zip(&o).take_while(|(x, y)| x == y).count();
    let da = i.saturating_sub(60);
    let pezzo = |v: &[char]| {
        v[da.min(v.len())..(i + 60).min(v.len())]
            .iter()
            .collect::<String>()
    };
    format!(
        "prima differenza al carattere {i}\n    atteso:   …{}…\n    ottenuto: …{}…",
        pezzo(&a),
        pezzo(&o)
    )
}

#[test]
fn risultati_di_riferimento() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::scenario_server(dir.path());
    let conn = omi_core::db::open(&db, true).unwrap();
    let svc = Service::new(&conn);
    let golden = cartella();

    let casi = casi();
    let mut errori = Vec::new();
    for (caso, tool, args) in &casi {
        let ok = golden.join(format!("{caso}.json"));
        let err = golden.join(format!("{caso}.errore.txt"));
        let (atteso, errore_atteso) = match (ok.exists(), err.exists()) {
            (true, false) => (leggi(&ok), false),
            (false, true) => (leggi(&err), true),
            _ => {
                errori.push(format!(
                    "{caso}: serve esattamente uno tra .json e .errore.txt"
                ));
                continue;
            }
        };
        let ottenuto = match chiama(&svc, tool, args.clone()) {
            Ok(t) if !errore_atteso => t,
            Err(OmiError::Validazione(m)) if errore_atteso => m,
            altro => {
                errori.push(format!("{caso}: esito inatteso {altro:?}"));
                continue;
            }
        };
        if ottenuto != atteso {
            errori.push(format!("{caso}: {}", differenza(&atteso, &ottenuto)));
        }
    }

    // Nessun file atteso senza il suo caso.
    let nomi: Vec<&str> = casi.iter().map(|c| c.0).collect();
    for e in std::fs::read_dir(&golden).unwrap() {
        let nome = e.unwrap().file_name().to_string_lossy().into_owned();
        if nome == "LEGGIMI.md" {
            continue;
        }
        let caso = nome
            .strip_suffix(".errore.txt")
            .or_else(|| nome.strip_suffix(".json"))
            .unwrap_or(&nome);
        if !nomi.contains(&caso) {
            errori.push(format!("{nome}: file senza caso"));
        }
    }

    assert!(
        errori.is_empty(),
        "{} casi su {} diversi dall'atteso:\n{}",
        errori.len(),
        casi.len(),
        errori.join("\n")
    );
}
