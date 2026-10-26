// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Prova sui dati reali: import degli ZIP di `dati-test/`, controlli d'integrità,
//! reimport, strumenti confrontati con l'SQL diretto e caso noto di `C302.kml` (2011/1).
//!
//! ```text
//! cargo test --release -p omi-core --test dati_reali -- --ignored --nocapture
//! ```
//!
//! `OMI_DATI_TEST` sceglie la cartella degli ZIP (predefinita `dati-test/` del repository) e
//! `OMI_PROVA_DIR` la cartella in cui resta `prova.db` (predefinita: temporanea, poi
//! eliminata). I nomi dei file contengono il codice fiscale del richiedente: nell'output compaiono
//! solo gli alias `reale-1.zip`, `reale-2.zip`, … in ordine alfabetico.

use std::path::{Path, PathBuf};
use std::time::Instant;

use omi_core::db;
use omi_core::geometry::{Geometry, hash, round_half_even};
use omi_core::import::{Importer, ZipReport};
use omi_core::service::*;
use omi_core::text::safe_file_name;
use regex::Regex;
use rusqlite::{Connection, params};
use serde_json::Value;

const CAMPIONE: usize = 200;

/// Uno ZIP reale con il suo alias.
struct Zip {
    alias: String,
    path: PathBuf,
}

/// ZIP `QI*.zip` della cartella, in ordine alfabetico senza distinzione di maiuscole.
fn zip_reali(dir: &Path) -> Vec<Zip> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            n.starts_with("qi") && n.ends_with(".zip")
        })
        .collect();
    paths.sort_by_key(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase()));
    paths
        .into_iter()
        .enumerate()
        .map(|(i, path)| Zip {
            alias: format!("reale-{}.zip", i + 1),
            path,
        })
        .collect()
}

/// Testo dell'importatore con i nomi dei file sostituiti dagli alias.
fn con_alias(testo: &str, zips: &[Zip]) -> String {
    let mut t = testo.to_string();
    for z in zips {
        let nome = z.path.file_name().unwrap().to_string_lossy().into_owned();
        t = t.replace(&nome, &z.alias);
        t = t.replace(&safe_file_name(&nome), &z.alias);
    }
    t
}

fn cf_regex() -> Regex {
    Regex::new(r"(?i)[A-Z]{6}\d{2}[A-Z]\d{2}[A-Z]\d{3}[A-Z]").unwrap()
}

fn n(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0))
        .unwrap_or_else(|e| panic!("{sql}: {e}"))
}

/// Elementi distribuiti in modo uniforme (passo costante), al massimo `k`.
fn campione<T: Clone>(v: &[T], k: usize) -> Vec<T> {
    if v.len() <= k {
        return v.to_vec();
    }
    let passo = v.len() / k;
    (0..k).map(|i| v[i * passo].clone()).collect()
}

fn json(s: &str) -> Value {
    serde_json::from_str(s).unwrap()
}

/// Errori e segnalazioni raccolti durante la prova.
#[derive(Default)]
struct Esito {
    errori: Vec<String>,
}

impl Esito {
    fn verifica(&mut self, ok: bool, msg: impl Into<String>) {
        let msg = msg.into();
        if ok {
            println!("  ok    {msg}");
        } else {
            println!("  ERRORE {msg}");
            self.errori.push(msg);
        }
    }
}

/// Importa tutti gli ZIP e chiama `completa`; restituisce i report e l'output (con gli alias).
fn importa(
    conn: &mut Connection,
    zips: &[Zip],
    esito: &mut Esito,
    titolo: &str,
) -> Vec<(ZipReport, String)> {
    println!("\n== {titolo}");
    let cf = cf_regex();
    let mut imp = Importer::new(conn).unwrap();
    let mut risultati = Vec::new();
    for z in zips {
        let mut out = Vec::new();
        let t = Instant::now();
        let r = imp.import_zip(conn, &z.path, &mut out);
        let secondi = t.elapsed().as_secs_f64();
        let testo = String::from_utf8_lossy(&out).into_owned();
        esito.verifica(
            !cf.is_match(&testo),
            format!(
                "{}: nessun codice fiscale nei messaggi dell'importatore",
                z.alias
            ),
        );
        let testo = con_alias(&testo, zips);
        print!("{testo}");
        match r {
            Ok(rep) => {
                println!(
                    "  -> {}: {secondi:.2} s, semestre {:?}, zone {}, valori {}, kml {}, perimetri {}",
                    z.alias, rep.semestre, rep.n_zone, rep.n_valori, rep.n_kml, rep.n_geo
                );
                risultati.push((rep, testo));
            }
            Err(e) => esito.verifica(false, format!("{}: import fallito: {e}", z.alias)),
        }
    }
    let mut out = Vec::new();
    let t = Instant::now();
    imp.completa(conn, &mut out).unwrap();
    print!("{}", String::from_utf8_lossy(&out));
    println!("  -> completa: {:.2} s", t.elapsed().as_secs_f64());
    risultati
}

fn conteggi(conn: &Connection, esito: &mut Esito) {
    println!("\n== Conteggi per semestre");
    let mut stmt = conn
        .prepare(
            "SELECT s.semestre, s.n_zone, (SELECT count(*) FROM zona z WHERE z.semestre=s.semestre),
                    s.n_valori, (SELECT count(*) FROM valore v WHERE v.semestre=s.semestre),
                    s.n_geometrie, (SELECT count(*) FROM zona_geo g WHERE g.semestre=s.semestre)
             FROM semestre s ORDER BY s.semestre",
        )
        .unwrap();
    let righe: Vec<[Option<i64>; 7]> = stmt
        .query_map([], |r| {
            Ok([
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get(4)?,
                r.get(5)?,
                r.get(6)?,
            ])
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    for [sem, nz, cz, nv, cv, ng, cg] in righe {
        let sem = sem.unwrap_or_default();
        println!(
            "  {}: n_zone {nz:?} (count {cz:?}), n_valori {nv:?} (count {cv:?}), n_geometrie {ng:?} (count {cg:?})",
            db::sem_label(sem)
        );
        esito.verifica(
            nz == cz && nv == cv && ng.unwrap_or(0) == cg.unwrap_or(0),
            format!(
                "{}: conteggi di semestre uguali ai count(*)",
                db::sem_label(sem)
            ),
        );
    }
    for (descr, sql) in [
        ("comuni", "SELECT count(*) FROM comune"),
        ("nomi di comune", "SELECT count(*) FROM comune_nome"),
        ("tipologie", "SELECT count(*) FROM tipologia"),
        ("geometrie distinte", "SELECT count(*) FROM geometria"),
        ("righe zona_geo", "SELECT count(*) FROM zona_geo"),
        (
            "geometrie condivise da più semestri",
            "SELECT count(*) FROM (SELECT geometria_id FROM zona_geo GROUP BY geometria_id HAVING count(DISTINCT semestre)>1)",
        ),
        (
            "valori senza zona (informativo)",
            "SELECT count(*) FROM valore v WHERE NOT EXISTS (SELECT 1 FROM zona z WHERE z.semestre=v.semestre AND z.link_zona=v.link_zona)",
        ),
        (
            "zona_geo senza zona nel semestre (informativo)",
            "SELECT count(*) FROM zona_geo g WHERE NOT EXISTS (SELECT 1 FROM zona z WHERE z.semestre=g.semestre AND z.comune_amm=g.comune_amm AND z.zona=g.zona)",
        ),
        (
            "zone senza perimetro nei semestri con KML (informativo)",
            "SELECT count(*) FROM zona z JOIN semestre s ON s.semestre=z.semestre AND s.n_geometrie>0 WHERE NOT EXISTS (SELECT 1 FROM zona_geo g WHERE g.semestre=z.semestre AND g.comune_amm=z.comune_amm AND g.zona=z.zona)",
        ),
    ] {
        println!("  {descr}: {}", n(conn, sql));
    }
}

fn integrita(conn: &Connection, esito: &mut Esito) {
    println!("\n== Integrità");
    let ic: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    esito.verifica(ic == "ok", format!("PRAGMA integrity_check = {ic}"));
    for (descr, sql) in [
        (
            "nessun zona_geo senza geometria",
            "SELECT count(*) FROM zona_geo WHERE geometria_id NOT IN (SELECT id FROM geometria)",
        ),
        (
            "nessuna geometria senza zona_geo",
            "SELECT count(*) FROM geometria WHERE id NOT IN (SELECT geometria_id FROM zona_geo)",
        ),
        (
            "nessuna riga di geometria_rt senza geometria",
            "SELECT count(*) FROM geometria_rt WHERE id NOT IN (SELECT id FROM geometria)",
        ),
        (
            "nessuna geometria senza riga di geometria_rt",
            "SELECT count(*) FROM geometria WHERE id NOT IN (SELECT id FROM geometria_rt)",
        ),
        (
            "bbox dell'R*Tree che contiene il bbox della geometria",
            "SELECT count(*) FROM geometria g JOIN geometria_rt r ON r.id=g.id
             WHERE r.min_lon>g.min_lon OR r.max_lon<g.max_lon OR r.min_lat>g.min_lat OR r.max_lat<g.max_lat",
        ),
    ] {
        let k = n(conn, sql);
        esito.verifica(k == 0, format!("{descr} ({k} casi)"));
    }

    // decode → encode, hash e misure su un campione di geometrie.
    let ids: Vec<i64> = conn
        .prepare("SELECT id FROM geometria ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut stmt = conn
        .prepare(
            "SELECT hash, dati, min_lon, min_lat, max_lon, max_lat, centro_lon, centro_lat, area_km2,
                    n_poligoni, n_punti FROM geometria WHERE id=?",
        )
        .unwrap();
    let (mut blob_ko, mut misure_ko) = (0, 0);
    let sample = campione(&ids, CAMPIONE);
    for id in &sample {
        let (h, dati, misure, np, npt): (Vec<u8>, Vec<u8>, [f64; 7], i64, i64) = stmt
            .query_row([id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    [
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                        r.get(6)?,
                        r.get(7)?,
                        r.get(8)?,
                    ],
                    r.get(9)?,
                    r.get(10)?,
                ))
            })
            .unwrap();
        let g = Geometry::decode(&dati).unwrap();
        let blob = g.encode();
        if blob != dati || hash(&blob)[..] != h[..] {
            blob_ko += 1;
        }
        let (a, b, c, d) = g.bbox();
        let (cx, cy, area) = g.centroid_and_area();
        let attese = [
            a,
            b,
            c,
            d,
            round_half_even(cx, 6),
            round_half_even(cy, 6),
            round_half_even(area, 4),
        ];
        if attese != misure || np != g.polygons.len() as i64 || npt != g.n_punti() as i64 {
            misure_ko += 1;
        }
    }
    esito.verifica(
        blob_ko == 0,
        format!(
            "decode → encode: stesso blob e stesso hash su {} geometrie ({blob_ko} diverse)",
            sample.len()
        ),
    );
    esito.verifica(
        misure_ko == 0,
        format!(
            "bbox, centro, area, n_poligoni e n_punti ricalcolati uguali su {} geometrie ({misure_ko} diverse)",
            sample.len()
        ),
    );

    // Centro dentro il perimetro su un campione di zone.
    let zg: Vec<i64> = conn
        .prepare("SELECT id FROM zona_geo ORDER BY id")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut stmt = conn
        .prepare(
            "SELECT zg.semestre, zg.comune_amm, zg.zona, g.dati, g.centro_lon, g.centro_lat, g.n_poligoni
             FROM zona_geo zg JOIN geometria g ON g.id=zg.geometria_id WHERE zg.id=?",
        )
        .unwrap();
    let sample = campione(&zg, CAMPIONE);
    let mut fuori = Vec::new();
    for id in &sample {
        let (sem, amm, zona, dati, lon, lat, np): (i64, String, String, Vec<u8>, f64, f64, i64) =
            stmt.query_row([id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            })
            .unwrap();
        let g = Geometry::decode(&dati).unwrap();
        if !g.contains(lon, lat) {
            let d = g.distance_meters(lon, lat);
            fuori.push(format!(
                "{} {amm} {zona} ({np} poligoni, centro a {d:.0} m dal bordo)",
                db::sem_label(sem)
            ));
        }
    }
    println!(
        "  info  centro dentro il perimetro: {}/{} zone; fuori (poligoni concavi o multipoligoni, non errori): {}",
        sample.len() - fuori.len(),
        sample.len(),
        fuori.len()
    );
    for f in &fuori {
        println!("        - {f}");
    }

    // Nessun codice fiscale nelle colonne di testo.
    let cf = cf_regex();
    let tabelle = tabelle(conn);
    let mut trovati = 0;
    let mut valori = 0usize;
    for t in &tabelle {
        for col in colonne(conn, t) {
            let sql = format!("SELECT \"{col}\" FROM \"{t}\" WHERE typeof(\"{col}\")='text'");
            let mut stmt = conn.prepare(&sql).unwrap();
            let mut rows = stmt.query([]).unwrap();
            while let Some(r) = rows.next().unwrap() {
                let s: String = r.get(0).unwrap();
                valori += 1;
                if cf.is_match(&s) {
                    trovati += 1;
                    println!("        codice fiscale in {t}.{col}");
                }
            }
        }
    }
    esito.verifica(
        trovati == 0,
        format!(
            "nessun codice fiscale nel database ({valori} valori di testo in {} tabelle)",
            tabelle.len()
        ),
    );
}

/// Tabelle dello schema, senza quelle interne di SQLite e dell'R*Tree.
fn tabelle(conn: &Connection) -> Vec<String> {
    conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'
         AND name NOT LIKE 'geometria_rt_%' ORDER BY name",
    )
    .unwrap()
    .query_map([], |r| r.get(0))
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn colonne(conn: &Connection, tabella: &str) -> Vec<String> {
    conn.prepare("SELECT name FROM pragma_table_info(?1) ORDER BY cid")
        .unwrap()
        .query_map([tabella], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

/// Colonne escluse dal confronto dopo il reimport: le date di caricamento e l'id di `zona_geo`,
/// che il `DELETE` + `INSERT` dei perimetri di un comune può riassegnare.
fn esclusa(tabella: &str, colonna: &str) -> bool {
    matches!(
        (tabella, colonna),
        ("semestre", "caricato_csv")
            | ("semestre", "caricato_kml")
            | ("file_caricato", "caricato")
            | ("zona_geo", "id")
    )
}

/// Confronta ogni tabella di `main` con la stessa tabella di `prima` (righe con molteplicità).
fn confronta_con_prima(conn: &Connection, esito: &mut Esito) {
    for t in tabelle(conn) {
        let cols: Vec<String> = colonne(conn, &t)
            .into_iter()
            .filter(|c| !esclusa(&t, c))
            .map(|c| format!("\"{c}\""))
            .collect();
        let cols = cols.join(", ");
        let gruppi =
            |db: &str| format!("SELECT {cols}, count(*) FROM {db}.\"{t}\" GROUP BY {cols}");
        let diff = |a: &str, b: &str| {
            n(
                conn,
                &format!("SELECT count(*) FROM ({} EXCEPT {})", gruppi(a), gruppi(b)),
            )
        };
        let (dopo, prima) = (diff("main", "prima"), diff("prima", "main"));
        let righe = n(conn, &format!("SELECT count(*) FROM main.\"{t}\""));
        esito.verifica(
            dopo == 0 && prima == 0,
            format!("reimport: {t} identica ({righe} righe; differenze {dopo}/{prima})"),
        );
    }
    // Le date di caricamento sono state aggiornate.
    let date = n(
        conn,
        "SELECT count(*) FROM main.file_caricato f JOIN prima.file_caricato p ON p.file=f.file WHERE f.caricato>=p.caricato",
    );
    println!("  info  file_caricato con data di caricamento aggiornata o uguale: {date}");
}

/// Strumenti sul database reale, confrontati con l'SQL diretto.
fn strumenti(path: &Path, esito: &mut Esito) {
    println!("\n== Strumenti sul database reale");
    let conn = db::open(path, true).unwrap();
    let svc = Service::new(&conn);
    let ult = n(&conn, "SELECT max(semestre) FROM semestre WHERE n_zone>0");
    let ult_label = db::sem_label(ult);

    // elenco_semestri, elenco_tipologie
    let v = json(&svc.elenco_semestri().unwrap());
    let semestri = v["semestri"].as_array().unwrap();
    esito.verifica(
        semestri.len() as i64 == n(&conn, "SELECT count(*) FROM semestre"),
        format!("elenco_semestri: {} semestri", semestri.len()),
    );
    for s in semestri {
        println!(
            "        {} zone {} valori {} perimetri {} elaborazione {}",
            s["semestre"], s["n_zone"], s["n_valori"], s["n_perimetri"], s["elaborazione"]
        );
    }
    let v = json(&svc.elenco_tipologie().unwrap());
    esito.verifica(
        v.as_array().unwrap().len() as i64 == n(&conn, "SELECT count(*) FROM tipologia"),
        format!(
            "elenco_tipologie: {} tipologie",
            v.as_array().unwrap().len()
        ),
    );

    // cerca_comuni
    for (testo, atteso) in [
        ("Roma", "H501"),
        ("h501", "H501"),
        ("058091", "H501"),
        ("Milano", "F205"),
        ("Reggio nell'Emilia", "H223"),
        ("Forlì", "D704"),
    ] {
        let v = json(
            &svc.cerca_comuni(&CercaComuni {
                testo: testo.into(),
                provincia: None,
            })
            .unwrap(),
        );
        let codici: Vec<&str> = v["comuni"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|c| c["codice_catastale"].as_str())
            .collect();
        esito.verifica(
            codici.contains(&atteso),
            format!(
                "cerca_comuni {testo:?}: trovati {} {codici:?}",
                v["trovati"]
            ),
        );
    }

    // zone_comune con valori e perimetri: zone e quotazioni come nell'SQL.
    for amm in ["H501", "F205"] {
        let v = json(
            &svc.zone_comune(&ZoneComune {
                comune: amm.into(),
                includi_valori: true,
                includi_perimetri: true,
                ..Default::default()
            })
            .unwrap(),
        );
        let zone = v["zone"].as_array().unwrap();
        let count = conn
            .query_row(
                "SELECT count(*) FROM zona WHERE comune_amm=? AND semestre=?",
                params![amm, ult],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        let con_perimetro = zone.iter().filter(|z| z.get("perimetro").is_some()).count();
        esito.verifica(
            v["n_zone"] == count && zone.len() as i64 == count,
            format!("zone_comune {amm} {ult_label}: {count} zone, {con_perimetro} con perimetro"),
        );
        // Query d'esempio: abitazioni civili, stato prevalente.
        let mut stmt = conn
            .prepare(
                "SELECT z.comune_descr, z.zona, z.zona_descr, v.compr_min, v.compr_max, v.loc_min, v.loc_max
FROM zona z JOIN valore v ON v.semestre = z.semestre AND v.link_zona = z.link_zona
WHERE z.semestre = ?1 AND z.comune_amm = ?2 AND v.cod_tip = 20 AND v.stato_prev = 'P'
ORDER BY v.compr_max DESC",
            )
            .unwrap();
        let attese: Vec<(String, [Option<f64>; 4])> = stmt
            .query_map(params![ult, amm], |r| {
                Ok((r.get(1)?, [r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?]))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let mut diverse = 0;
        for (zona, [cmin, cmax, lmin, lmax]) in &attese {
            let z = zone.iter().find(|z| z["info"]["zona"] == zona.as_str());
            let civ = z.and_then(|z| {
                z["valori"].as_array().unwrap().iter().find(|x| {
                    x["cod_tipologia"] == 20
                        && x.get("stato_prevalente") == Some(&Value::Bool(true))
                })
            });
            let ok = civ.is_some_and(|c| {
                c["compravendita_eur_mq"]["min"].as_f64() == *cmin
                    && c["compravendita_eur_mq"]["max"].as_f64() == *cmax
                    && c["locazione_eur_mq_mese"]["min"].as_f64() == *lmin
                    && c["locazione_eur_mq_mese"]["max"].as_f64() == *lmax
            });
            if !ok {
                diverse += 1;
            }
        }
        esito.verifica(
            diverse == 0 && !attese.is_empty(),
            format!(
                "zone_comune {amm}: abitazioni civili prevalenti uguali alla query della spec 01 §6 ({} zone, {diverse} diverse)",
                attese.len()
            ),
        );
    }

    // dettaglio_zona: la prima zona di Milano, per codice e per LinkZona.
    let (zona, link): (String, String) = conn
        .query_row(
            "SELECT zona, link_zona FROM zona WHERE comune_amm='F205' AND semestre=? ORDER BY fascia, zona LIMIT 1",
            [ult],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let per_codice = svc
        .dettaglio_zona(&DettaglioZona {
            comune: "milano".into(),
            zona: zona.clone(),
            includi_geometria: true,
            ..Default::default()
        })
        .unwrap();
    let per_link = svc
        .dettaglio_zona(&DettaglioZona {
            comune: "milano".into(),
            zona: link.to_lowercase(),
            includi_geometria: true,
            ..Default::default()
        })
        .unwrap();
    let v = json(&per_codice);
    let n_valori = conn
        .query_row(
            "SELECT count(*) FROM valore WHERE semestre=? AND link_zona=?",
            params![ult, link],
            |r| r.get::<_, i64>(0),
        )
        .unwrap();
    esito.verifica(
        per_codice == per_link
            && v["zone"][0]["valori"].as_array().unwrap().len() as i64 == n_valori,
        format!(
            "dettaglio_zona F205 {zona}: uguale per codice e per LinkZona, {n_valori} quotazioni, geometria {}",
            v["zone"][0]["perimetro"].get("geometria").is_some()
        ),
    );

    // zona_da_coordinate: il centro di una zona di Roma contenuto nel perimetro restituisce la zona.
    let gs: i64 = n(
        &conn,
        "SELECT max(semestre) FROM semestre WHERE n_geometrie>0",
    );
    let mut stmt = conn
        .prepare(
            "SELECT zg.zona, g.dati, g.centro_lon, g.centro_lat FROM zona_geo zg JOIN geometria g ON g.id=zg.geometria_id
             WHERE zg.semestre=? AND zg.comune_amm='H501' ORDER BY zg.zona",
        )
        .unwrap();
    let zone_roma: Vec<(String, Vec<u8>, f64, f64)> = stmt
        .query_map([gs], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut provate = 0;
    let mut sbagliate = 0;
    for (zona, dati, lon, lat) in campione(&zone_roma, 20) {
        if !Geometry::decode(&dati).unwrap().contains(lon, lat) {
            continue;
        }
        provate += 1;
        let v = json(
            &svc.zona_da_coordinate(&ZonaDaCoordinate {
                latitudine: lat,
                longitudine: lon,
                semestre: Some(db::sem_label(gs)),
                ..Default::default()
            })
            .unwrap(),
        );
        let trovate: Vec<&Value> = v["risultati"]
            .as_array()
            .map(|a| a.iter().map(|r| &r["zona"]).collect())
            .unwrap_or_default();
        if v["trovato"] != true || !trovate.iter().any(|z| **z == zona.as_str()) {
            sbagliate += 1;
        }
    }
    esito.verifica(
        provate > 0 && sbagliate == 0,
        format!("zona_da_coordinate: {provate} centri di zone di Roma, {sbagliate} sbagliati"),
    );
    // Piazza Venezia (Roma) e il Duomo di Milano: punti noti nel centro.
    for (luogo, lat, lon, amm) in [
        ("Piazza Venezia", 41.8960, 12.4823, "H501"),
        ("Duomo di Milano", 45.4642, 9.1900, "F205"),
    ] {
        let v = json(
            &svc.zona_da_coordinate(&ZonaDaCoordinate {
                latitudine: lat,
                longitudine: lon,
                ..Default::default()
            })
            .unwrap(),
        );
        let r = &v["risultati"][0];
        esito.verifica(
            v["trovato"] == true && r["comune"]["codice_catastale"] == amm,
            format!(
                "zona_da_coordinate {luogo}: zona {} fascia {}",
                r["zona"], r["dati"][0]["info"]["fascia"]
            ),
        );
    }

    // zone_vicine attorno a Piazza Venezia: ordinate per distanza ed entro il raggio.
    let v = json(
        &svc.zone_vicine(&ZoneVicine {
            latitudine: 41.8960,
            longitudine: 12.4823,
            raggio_m: Some(1500.0),
            semestre: None,
        })
        .unwrap(),
    );
    let d: Vec<f64> = v["zone"]
        .as_array()
        .unwrap()
        .iter()
        .map(|z| z["distanza_m"].as_f64().unwrap())
        .collect();
    esito.verifica(
        !d.is_empty() && d.windows(2).all(|w| w[0] <= w[1]) && d.iter().all(|&x| x <= 1500.0),
        format!(
            "zone_vicine Piazza Venezia 1500 m: {} zone, distanze {d:?}",
            d.len()
        ),
    );

    // storico_zona: una zona di Roma presente in tutti i semestri.
    let n_sem = n(&conn, "SELECT count(*) FROM semestre WHERE n_zone>0");
    let zona: Option<String> = conn
        .query_row(
            "SELECT zona FROM zona WHERE comune_amm='H501' GROUP BY zona HAVING count(DISTINCT semestre)=? ORDER BY zona LIMIT 1",
            [n_sem],
            |r| r.get(0),
        )
        .ok();
    if let Some(zona) = zona {
        let v = json(
            &svc.storico_zona(&StoricoZona {
                comune: "roma".into(),
                zona: zona.clone(),
                ..Default::default()
            })
            .unwrap(),
        );
        let righe = conn
            .query_row(
                "SELECT count(*) FROM zona z JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona
                 WHERE z.comune_amm='H501' AND (z.zona=?1 OR z.link_zona=?1)",
                [&zona],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        let valori: usize = v["serie"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["valori"].as_array().unwrap().len())
            .sum();
        esito.verifica(
            valori as i64 == righe,
            format!(
                "storico_zona H501 {zona}: {} serie, {valori} valori, {} descrizioni",
                v["n_serie"],
                v["descrizioni_zona"].as_array().unwrap().len()
            ),
        );
    } else {
        println!("  info  nessuna zona di Roma presente in tutti i semestri");
    }

    // statistiche_comune: zone per fascia e minimo/massimo delle abitazioni civili.
    let v = json(
        &svc.statistiche_comune(&StatisticheComune {
            comune: "F205".into(),
            ..Default::default()
        })
        .unwrap(),
    );
    let mut ok = true;
    for f in v["per_fascia"].as_array().unwrap() {
        let count = conn
            .query_row(
                "SELECT count(*) FROM zona WHERE comune_amm='F205' AND semestre=? AND fascia=?",
                params![ult, f["fascia"].as_str()],
                |r| r.get::<_, i64>(0),
            )
            .unwrap();
        ok &= f["n_zone"] == count;
    }
    let civ = v["per_tipologia"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["cod_tipologia"] == 20)
        .cloned()
        .unwrap_or_default();
    let (min, max): (Option<f64>, Option<f64>) = conn
        .query_row(
            "SELECT min(v.compr_min), max(v.compr_max) FROM zona z JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona
             WHERE z.comune_amm='F205' AND z.semestre=? AND v.cod_tip=20 AND v.stato_prev='P'",
            [ult],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    esito.verifica(
        ok && civ["compravendita"]["min"].as_f64() == min
            && civ["compravendita"]["max"].as_f64() == max,
        format!(
            "statistiche_comune F205 {ult_label}: zone per fascia uguali, abitazioni civili {} – {} (medio {})",
            civ["compravendita"]["min"], civ["compravendita"]["max"], civ["compravendita"]["medio"]
        ),
    );
}

/// Caso noto: `C302.kml` del 2011/1, zona B1 senza geometria valida.
fn caso_c302(conn: &Connection, risultati: &[(ZipReport, String)], esito: &mut Esito) {
    let Some((_, out)) = risultati
        .iter()
        .find(|(r, _)| r.semestre == Some(20111) && r.n_kml > 0)
    else {
        println!("\n== C302 2011/1: nessun QIP del 2011/1 tra i dati");
        return;
    };
    println!("\n== C302 2011/1");
    let avvisi: Vec<&str> = out
        .lines()
        .filter(|l| l.contains("C302.kml malformato"))
        .collect();
    esito.verifica(!avvisi.is_empty(), format!("avviso stampato: {avvisi:?}"));
    let zone: Vec<String> = conn
        .prepare(
            "SELECT zona FROM zona_geo WHERE semestre=20111 AND comune_amm='C302' ORDER BY zona",
        )
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    esito.verifica(
        !zone.is_empty() && !zone.iter().any(|z| z == "B1"),
        format!("altri placemark di C302 caricati, B1 assente: {zone:?}"),
    );
}

#[test]
#[ignore = "dati reali: cargo test --release -p omi-core --test dati_reali -- --ignored --nocapture"]
fn prova_dati_reali() {
    let dati = std::env::var_os("OMI_DATI_TEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../dati-test"));
    let zips = zip_reali(&dati);
    if zips.is_empty() {
        println!("Nessuno ZIP reale in {}: prova saltata", dati.display());
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let dir = std::env::var_os("OMI_PROVA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| tmp.path().to_path_buf());
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("prova.db");
    for suffisso in ["", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{suffisso}", path.display()));
    }

    println!("== Dati");
    for z in &zips {
        let mb = std::fs::metadata(&z.path).unwrap().len() as f64 / (1024.0 * 1024.0);
        println!("  {}: {mb:.1} MB", z.alias);
    }
    let mut esito = Esito::default();

    // 1. Import e conteggi.
    let mut conn = db::open(&path, false).unwrap();
    let inizio = Instant::now();
    let risultati = importa(&mut conn, &zips, &mut esito, "Import");
    println!("  -> totale: {:.2} s", inizio.elapsed().as_secs_f64());
    conteggi(&conn, &mut esito);
    caso_c302(&conn, &risultati, &mut esito);

    // 2. Integrità.
    integrita(&conn, &mut esito);
    drop(conn);

    // 4. Strumenti (prima del reimport, sul database appena caricato).
    strumenti(&path, &mut esito);

    // 3. Reimport degli stessi file nello stesso ordine e confronto con la copia.
    let copia = dir.join("prova-prima.db");
    std::fs::copy(&path, &copia).unwrap();
    let mut conn = db::open(&path, false).unwrap();
    importa(&mut conn, &zips, &mut esito, "Reimport degli stessi file");
    println!("\n== Confronto dopo il reimport");
    conn.execute("ATTACH DATABASE ?1 AS prima", [copia.to_string_lossy()])
        .unwrap();
    confronta_con_prima(&conn, &mut esito);
    conn.execute("DETACH DATABASE prima", []).unwrap();
    drop(conn);
    let _ = std::fs::remove_file(&copia);

    println!("\n== Esito: {} errori", esito.errori.len());
    for e in &esito.errori {
        println!("  - {e}");
    }
    println!("Database: {}", path.display());
    assert!(esito.errori.is_empty(), "{:#?}", esito.errori);
}
