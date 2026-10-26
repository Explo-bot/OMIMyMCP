// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Test d'integrazione dell'importazione su forniture sintetiche.

mod support;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use omi_core::db;
use omi_core::geometry::{Geometry, round_half_even};
use omi_core::import::{Importer, ZipReport};
use omi_core::{OmiError, Result};
use rusqlite::Connection;
use rusqlite::types::Value;
use support::*;
use tempfile::TempDir;

struct Env {
    dir: TempDir,
    conn: Connection,
    imp: Importer,
}

impl Env {
    fn new() -> Env {
        let dir = tempfile::tempdir().unwrap();
        let conn = db::open(&dir.path().join("omi.db"), false).unwrap();
        let imp = Importer::new(&conn).unwrap();
        Env { dir, conn, imp }
    }

    fn zip(&self, nome: &str, voci: &[Voce]) -> PathBuf {
        let p = self.dir.path().join(nome);
        write_zip(&p, voci);
        p
    }

    fn import(&mut self, path: &Path) -> (Result<ZipReport>, String) {
        let mut out = Vec::new();
        let r = self.imp.import_zip(&mut self.conn, path, &mut out);
        (r, String::from_utf8(out).unwrap())
    }

    fn ok(&mut self, path: &Path) -> (ZipReport, String) {
        let (r, out) = self.import(path);
        (
            r.unwrap_or_else(|e| panic!("import fallito: {e}\n{out}")),
            out,
        )
    }

    fn completa(&mut self) -> String {
        let mut out = Vec::new();
        self.imp.completa(&mut self.conn, &mut out).unwrap();
        String::from_utf8(out).unwrap()
    }

    fn n(&self, sql: &str) -> i64 {
        self.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn s(&self, sql: &str) -> Option<String> {
        self.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn orfani(&self) -> i64 {
        self.n("SELECT count(*) FROM zona_geo WHERE geometria_id NOT IN (SELECT id FROM geometria)")
    }

    /// Conteggi di tutte le tabelle, per confrontare due stati del database.
    fn conteggi(&self) -> Vec<(&'static str, i64)> {
        [
            "semestre",
            "zona",
            "valore",
            "tipologia",
            "geometria",
            "geometria_rt",
            "zona_geo",
            "file_caricato",
        ]
        .into_iter()
        .map(|t| (t, self.n(&format!("SELECT count(*) FROM {t}"))))
        .collect()
    }
}

fn errore(r: Result<ZipReport>) -> String {
    match r {
        Err(e) => e.to_string(),
        Ok(r) => panic!("atteso un errore, ottenuto {r:?}"),
    }
}

/// Caso 1: QIP completo.
#[test]
fn qip_completo() {
    let mut env = Env::new();
    let b = base();
    let p = env.zip(&format!("QIP1438117_{CF}.zip"), &b.qip(20252));
    let (r, out) = env.ok(&p);
    assert_eq!(
        r,
        ZipReport {
            semestre: Some(20252),
            n_zone: 3,
            n_valori: 5,
            n_kml: 2,
            n_geo: 3,
            ignorato: false
        }
    );
    let righe: Vec<&str> = out.lines().collect();
    assert!(
        righe[0].starts_with("Import di QIP1438117.zip (0.0 MB)..."),
        "{out}"
    );
    assert_eq!(righe[1], "  Semestre 2025/2: 3 zone, 5 valori");
    assert_eq!(
        righe[2],
        "  Semestre 2025/2: 3 perimetri di zona da 2 file KML"
    );
    assert!(righe[3].starts_with("  completato in ") && righe[3].ends_with(" s"));
    assert_eq!(righe.len(), 4);

    // semestre
    let sem: (i64, i64, String, String, String, String, i64, i64, i64) = env
        .conn
        .query_row(
            "SELECT anno, numero, elaborazione_zone, elaborazione_valori, file_csv, file_kml, n_zone, n_valori, n_geometrie
             FROM semestre WHERE semestre=20252",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                    r.get(8)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        sem,
        (
            2025,
            2,
            "12/03/2026".to_string(),
            "12/03/2026".to_string(),
            "QIP1438117.zip".to_string(),
            "QIP1438117.zip".to_string(),
            3,
            5,
            3
        )
    );
    let caricato = env.s("SELECT caricato_csv FROM semestre").unwrap();
    assert!(
        chrono::NaiveDateTime::parse_from_str(&caricato, "%Y-%m-%d %H:%M:%S").is_ok(),
        "{caricato}"
    );

    // zona: apici rimossi, campi vuoti NULL, interi convertiti
    let z: (String, String, String, Option<String>, String, i64, String, i64) = env
        .conn
        .query_row(
            "SELECT comune_amm, comune_istat, zona_descr, sez, fascia, cod_tip_prev, descr_tip_prev, microzona
             FROM zona WHERE semestre=20252 AND link_zona='RM00000001'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        z,
        (
            "H501".to_string(),
            "1058091".to_string(),
            "ZONA B1 DI ROMA".to_string(),
            None,
            "B".to_string(),
            20,
            "Abitazioni civili".to_string(),
            1
        )
    );

    // valore: virgola decimale, stato_prev vuoto → NULL
    let v: (f64, f64, f64, f64, String, String) = env
        .conn
        .query_row(
            "SELECT compr_min, compr_max, loc_min, loc_max, stato_prev, sup_nl_compr FROM valore
             WHERE link_zona='RM00000001' AND cod_tip=20 AND stato='NORMALE'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        v,
        (3500.0, 5200.5, 12.5, 18.3, "P".to_string(), "L".to_string())
    );
    assert_eq!(
        env.n("SELECT count(*) FROM valore WHERE stato='OTTIMO' AND stato_prev IS NULL"),
        1
    );
    assert_eq!(
        env.s("SELECT group_concat(cod_tip || '=' || descrizione, ', ') FROM (SELECT * FROM tipologia ORDER BY cod_tip)"),
        Some("13=Box, 20=Abitazioni civili".to_string())
    );

    // zona_geo: colore dello stile, ExtendedData, nome
    let zg: (String, String, String, String, Option<String>, i64) = env
        .conn
        .query_row(
            "SELECT comune_amm, zona, link_zona, nome, colore, geometria_id FROM zona_geo
             WHERE semestre=20252 AND comune_amm='H501' AND zona='B1'",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(&zg.0[..], "H501", "codice comune dall'ExtendedData");
    assert_eq!(
        (zg.1.as_str(), zg.2.as_str(), zg.3.as_str(), zg.4.as_deref()),
        ("B1", "RM00000001", "ROMA - Zona OMI B1", Some("7f00ff00"))
    );
    assert_eq!(
        env.s("SELECT colore FROM zona_geo WHERE zona='C2'"),
        None,
        "placemark senza stile"
    );

    // geometria letta dal database
    let (blob, n_pol, n_punti): (Vec<u8>, i64, i64) = env
        .conn
        .query_row(
            "SELECT dati, n_poligoni, n_punti FROM geometria WHERE id=?1",
            [zg.5],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    type Misure = ((f64, f64, f64, f64), (f64, f64), f64);
    let (bbox, centro, area): Misure = env
        .conn
        .query_row(
            "SELECT min_lon, min_lat, max_lon, max_lat, centro_lon, centro_lat, area_km2
             FROM geometria WHERE id=?1",
            [zg.5],
            |r| {
                Ok((
                    (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?),
                    (r.get(4)?, r.get(5)?),
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    let g = Geometry::decode(&blob).unwrap();
    assert_eq!(g, geometria(&b.roma_b1));
    assert_eq!((n_pol, n_punti), (2, 15));
    assert_eq!(bbox, (12.48, 41.89, 12.52, 41.91));
    let (cx, cy, a) = g.centroid_and_area();
    assert_eq!(
        (centro, area),
        (
            (round_half_even(cx, 6), round_half_even(cy, 6)),
            round_half_even(a, 4)
        )
    );
    assert!(g.contains(12.481, 41.891), "punto interno");
    assert!(!g.contains(12.487, 41.897), "punto nel buco");
    assert!(g.contains(12.515, 41.895), "secondo poligono");
    let hash: Vec<u8> = env
        .conn
        .query_row("SELECT hash FROM geometria WHERE id=?1", [zg.5], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(hash, omi_core::geometry::hash(&blob));
    let rt: i64 = env
        .conn
        .query_row(
            "SELECT id FROM geometria_rt WHERE min_lon<=12.515 AND max_lon>=12.515 AND min_lat<=41.895 AND max_lat>=41.895",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(rt, zg.5);

    // registro del file
    let (file, dim, contenuto): (String, i64, String) = env
        .conn
        .query_row(
            "SELECT file, dimensione, contenuto FROM file_caricato",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(file, "QIP1438117.zip");
    assert_eq!(dim, std::fs::metadata(&p).unwrap().len() as i64);
    assert_eq!(contenuto, "zone=3; valori=5; kml=2; perimetri=3");
}

/// Caso 2: reimport dello stesso file.
#[test]
fn reimport_senza_duplicati() {
    let mut env = Env::new();
    let p = env.zip("QIP1.zip", &base().qip(20252));
    env.ok(&p);
    let prima = env.conteggi();
    env.ok(&p);
    assert_eq!(env.conteggi(), prima);
    assert_eq!(
        env.n("SELECT n_zone + n_valori + n_geometrie FROM semestre"),
        11
    );
}

/// Caso 3: forniture provinciali che si sommano, poi una nazionale che sostituisce.
#[test]
fn provinciali_si_sommano_nazionale_sostituisce() {
    let mut env = Env::new();
    let b = base();
    let sem = 20252;
    let mut roma = voci_csv(sem, &titolo(sem), &b.zone[..2], &b.valori[..3]);
    roma.push(voce_kml("H501.kml", &b.kml_roma(sem)));
    let mut milano = voci_csv(sem, &titolo(sem), &b.zone[2..], &b.valori[3..]);
    milano.push(voce_kml("F205.kml", &b.kml_milano(sem)));
    let (p1, p2) = (env.zip("QIP1.zip", &roma), env.zip("QIP2.zip", &milano));
    env.ok(&p1);
    assert_eq!(env.n("SELECT n_zone FROM semestre"), 2);
    let (_, out) = env.ok(&p2);
    assert!(
        out.contains("  Semestre 2025/2: 1 zone, 2 valori\n"),
        "{out}"
    );
    assert_eq!(
        env.n("SELECT n_zone * 100 + n_valori * 10 + n_geometrie FROM semestre"),
        353
    );
    assert_eq!(env.n("SELECT count(*) FROM zona_geo"), 3);

    // Nazionale: entrambi i comuni, ma solo la zona B1 di Roma.
    let zone = vec![b.zone[0].clone(), b.zone[2].clone()];
    let valori = vec![b.valori[0].clone(), b.valori[3].clone()];
    let mut nazionale = voci_csv(sem, &titolo(sem), &zone, &valori);
    let roma_b1 = placemark(&b.zone[0], sem, b.roma_b1.clone());
    nazionale.push(voce_kml("H501.kml", &kml(&[], &[roma_b1])));
    nazionale.push(voce_kml("F205.kml", &b.kml_milano(sem)));
    let p3 = env.zip("QIP3.zip", &nazionale);
    env.ok(&p3);
    assert_eq!(
        env.n("SELECT n_zone * 100 + n_valori * 10 + n_geometrie FROM semestre"),
        222
    );
    assert_eq!(
        env.n("SELECT count(*) FROM zona WHERE link_zona='RM00000002'"),
        0
    );
    assert_eq!(
        env.n("SELECT count(*) FROM valore WHERE link_zona='RM00000002'"),
        0
    );
    assert_eq!(env.n("SELECT count(*) FROM zona_geo WHERE zona='C2'"), 0);
    assert_eq!(env.n("SELECT count(*) FROM file_caricato"), 3);
}

/// Caso 4: un QI senza KML non cancella i perimetri del semestre.
#[test]
fn qi_senza_kml_mantiene_perimetri() {
    let mut env = Env::new();
    let b = base();
    let (qip, qi) = (
        env.zip("QIP1.zip", &b.qip(20252)),
        env.zip("QI2.zip", &b.qi(20252)),
    );
    env.ok(&qip);
    let (r, _) = env.ok(&qi);
    assert_eq!((r.n_kml, r.n_geo), (0, 0));
    assert_eq!(env.n("SELECT count(*) FROM zona_geo"), 3);
    assert_eq!(env.n("SELECT n_geometrie FROM semestre"), 3);
    assert_eq!(
        (
            env.s("SELECT file_csv FROM semestre"),
            env.s("SELECT file_kml FROM semestre")
        ),
        (Some("QI2.zip".to_string()), Some("QIP1.zip".to_string()))
    );
    assert_eq!(
        env.s("SELECT contenuto FROM file_caricato WHERE file='QI2.zip'"),
        Some("zone=3; valori=5; kml=0; perimetri=0".to_string())
    );
}

/// Caso 5: KML in Latin-1 con dichiarazione UTF-8; codici dal nome del file e del placemark.
#[test]
fn kml_latin1_con_dichiarazione_utf8() {
    let mut env = Env::new();
    let forli = Comune {
        amm: "D704",
        istat: "1040012",
        descr: "FORLÌ",
        prov: "FC",
        regione: "EMILIA-ROMAGNA",
    };
    let z = zona(&forli, "B1", "FC00000001");
    let mut p = placemark(&z, 20252, vec![vec![rect(12.03, 44.21, 12.05, 44.23)]]);
    p.codcom = None;
    p.codzona = None;
    let testo = kml(&[], &[p]);
    assert!(testo.contains("encoding=\"UTF-8\""));
    let latin1: Vec<u8> = testo
        .chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap())
        .collect();
    assert!(std::str::from_utf8(&latin1).is_err());
    let mut voci = voci_csv(20252, &titolo(20252), std::slice::from_ref(&z), &[]);
    voci.push(("d704.kml".to_string(), latin1));
    let path = env.zip("QIP1.zip", &voci);
    env.ok(&path);
    let r: (String, String, String) = env
        .conn
        .query_row("SELECT comune_amm, zona, nome FROM zona_geo", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(
        r,
        (
            "D704".to_string(),
            "B1".to_string(),
            "FORLÌ - Zona OMI B1".to_string()
        )
    );
    assert_eq!(
        env.s("SELECT comune_descr FROM zona"),
        Some("FORLÌ".to_string())
    );
}

/// Caso 6: un placemark malformato viene scartato, gli altri vengono caricati.
#[test]
fn kml_con_placemark_malformato() {
    let mut env = Env::new();
    let b = base();
    let d3 = zona(&ROMA, "D3", "RM00000003");
    let p1 = placemark(&b.zone[0], 20252, b.roma_b1.clone());
    let p2 = placemark(&b.zone[1], 20252, b.roma_c2.clone());
    let p3 = placemark(&d3, 20252, vec![vec![rect(12.6, 41.7, 12.7, 41.8)]]);
    let rotto = placemark_xml(&p2).replace("</name>", "</nome>");
    let testo = kml(&[], &[p1, p2.clone(), p3]).replacen(&placemark_xml(&p2), &rotto, 1);
    let mut voci = b.qi(20252);
    voci.push(voce_kml("QI_1234_1/H501.kml", &testo));
    let path = env.zip("QIP1.zip", &voci);
    let (r, out) = env.ok(&path);
    assert_eq!(r.n_geo, 2);
    let avviso = out
        .lines()
        .find(|l| l.starts_with("    Attenzione: "))
        .unwrap_or_else(|| panic!("avviso mancante:\n{out}"));
    assert!(
        avviso.starts_with("    Attenzione: H501.kml malformato (XML non valido (byte "),
        "{avviso}"
    );
    assert!(
        avviso.ends_with("); letti 2 placemark, scartati 1"),
        "{avviso}"
    );
    assert_eq!(
        env.s("SELECT group_concat(zona, ',') FROM (SELECT zona FROM zona_geo ORDER BY zona)"),
        Some("B1,D3".to_string())
    );
}

/// Caso 7: KML malformato senza `<Placemark`: lo ZIP fallisce, quelli successivi si caricano.
#[test]
fn kml_malformato_senza_placemark_fa_fallire_lo_zip() {
    let mut env = Env::new();
    let b = base();
    let mut voci = b.qi(20252);
    voci.push(voce_kml(
        "X001.kml",
        "<kml><Document><Folder></Document></kml>",
    ));
    let (p1, p2) = (
        env.zip("QIP1.zip", &voci),
        env.zip("QIP2.zip", &b.qip(20252)),
    );
    let (r, out) = env.import(&p1);
    let msg = errore(r);
    assert!(msg.starts_with("XML non valido (byte "), "{msg}");
    assert!(!out.contains("completato"), "{out}");
    assert!(
        env.conteggi().iter().all(|&(_, n)| n == 0),
        "rollback completo"
    );
    env.ok(&p2);
    assert_eq!(env.n("SELECT count(*) FROM zona"), 3);
    assert_eq!(env.n("SELECT count(*) FROM file_caricato"), 1);
}

/// Caso 9: il codice fiscale non compare in nessuna tabella né nei messaggi.
#[test]
fn nome_con_codice_fiscale() {
    let mut env = Env::new();
    let b = base();
    let p1 = env.zip(&format!("QIP1438117_{CF}.zip"), &b.qip(20252));
    let p2 = env.zip(&format!("QI99_{}.ZIP", CF.to_lowercase()), &b.qi(20251));
    let (_, out1) = env.ok(&p1);
    let (_, out2) = env.ok(&p2);
    env.completa();
    for out in [&out1, &out2] {
        assert!(!out.to_uppercase().contains(CF), "{out}");
    }
    assert_eq!(
        env.s("SELECT group_concat(file, ',') FROM (SELECT file FROM file_caricato ORDER BY file)"),
        Some("QI99.ZIP,QIP1438117.zip".to_string())
    );

    let tabelle: Vec<String> = {
        let mut st = env
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table'")
            .unwrap();
        st.query_map([], |r| r.get(0))
            .unwrap()
            .map(|r| r.unwrap())
            .collect()
    };
    assert!(tabelle.len() >= 10);
    let cf = CF.as_bytes();
    for t in &tabelle {
        let mut st = env.conn.prepare(&format!("SELECT * FROM \"{t}\"")).unwrap();
        let n = st.column_count();
        let mut rows = st.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            for i in 0..n {
                let trovato = match r.get::<_, Value>(i).unwrap() {
                    Value::Text(s) => s.to_uppercase().contains(CF),
                    Value::Blob(b) => b.windows(cf.len()).any(|w| w.eq_ignore_ascii_case(cf)),
                    _ => false,
                };
                assert!(!trovato, "codice fiscale nella tabella {t}");
            }
        }
    }
}

/// Caso 10: la stessa geometria in due semestri è salvata una volta sola.
#[test]
fn geometria_identica_in_due_semestri() {
    let mut env = Env::new();
    let b = base();
    let (p1, p2) = (
        env.zip("QIP1.zip", &b.qip(20251)),
        env.zip("QIP2.zip", &b.qip(20252)),
    );
    env.ok(&p1);
    env.ok(&p2);
    assert_eq!(env.n("SELECT count(*) FROM geometria"), 3);
    assert_eq!(env.n("SELECT count(*) FROM geometria_rt"), 3);
    assert_eq!(env.n("SELECT count(*) FROM zona_geo"), 6);
    assert_eq!(
        env.n("SELECT count(*) FROM zona_geo WHERE comune_amm='H501' AND zona='B1'"),
        2
    );
    assert_eq!(
        env.n("SELECT count(DISTINCT geometria_id) FROM zona_geo WHERE comune_amm='H501' AND zona='B1'"),
        1
    );
}

/// Caso 11: uno ZIP fallito dopo aver inserito una geometria nuova non lascia riferimenti orfani.
#[test]
fn zip_fallito_non_lascia_zona_geo_orfani() {
    let mut env = Env::new();
    let b = base();
    let mut voci = b.qi(20252);
    voci.push(voce_kml("H501.kml", &b.kml_roma(20252)));
    voci.push(voce_kml("ROTTO.kml", "<kml><Document></Folder></kml>"));
    let (p1, p2) = (
        env.zip("QIP1.zip", &voci),
        env.zip("QIP2.zip", &b.qip(20252)),
    );
    errore(env.import(&p1).0);
    assert_eq!(env.n("SELECT count(*) FROM geometria"), 0);
    env.ok(&p2);
    assert_eq!(env.orfani(), 0);
    assert_eq!(env.n("SELECT count(*) FROM geometria"), 3);
    assert_eq!(env.n("SELECT count(*) FROM zona_geo"), 3);
}

/// Caso 12: semestre del titolo diverso da quello del nome del CSV.
#[test]
fn titolo_con_semestre_diverso_dal_nome() {
    let mut env = Env::new();
    let b = base();
    let p = env.zip(
        "QI1.zip",
        &voci_csv(20251, &titolo(20252), &b.zone, &b.valori),
    );
    let (r, out) = env.ok(&p);
    assert!(
        out.contains("  Attenzione: semestre nel nome file (20251) diverso da quello nel titolo (20252); uso 20252\n"),
        "{out}"
    );
    assert_eq!(r.semestre, Some(20252));
    assert_eq!(env.n("SELECT count(*) FROM zona WHERE semestre=20252"), 3);
    assert_eq!(
        env.n("SELECT count(*) FROM semestre WHERE semestre<>20252"),
        0
    );
}

/// Caso 13: ZIP senza CSV né KML.
#[test]
fn zip_senza_csv_ne_kml_ignorato() {
    let mut env = Env::new();
    let p = env.zip(
        "QI1.zip",
        &[("leggimi.txt".to_string(), b"niente".to_vec())],
    );
    let (r, out) = env.ok(&p);
    assert!(r.ignorato);
    assert_eq!(
        out,
        "Import di QI1.zip (0.0 MB)...\n  Nessun CSV o KML OMI riconosciuto nel file: ignorato.\n"
    );
    assert!(env.conteggi().iter().all(|&(_, n)| n == 0));
}

/// Caso 14: `completa()` con nomi storici, geometrie orfane e cache ricaricata.
#[test]
fn completa_anagrafiche_e_orfani() {
    let mut env = Env::new();
    let b = base();
    let vecchio = Comune {
        amm: "I329",
        istat: "1090064",
        descr: "Sant'Antonio",
        prov: "SS",
        regione: "SARDEGNA",
    };
    let nuovo = Comune {
        descr: "SANT'ANTONIO DI GALLURA",
        ..vecchio.clone()
    };
    let z_old = zona(&vecchio, "B1", "SS00000001");
    let z_new = zona(&nuovo, "B1", "SS00000001");
    let p1 = env.zip("QI1.zip", &voci_csv(20241, &titolo(20241), &[z_old], &[]));
    let mut zone = b.zone[..2].to_vec();
    zone.push(z_new);
    let mut voci = voci_csv(20252, &titolo(20252), &zone, &[]);
    voci.push(voce_kml("H501.kml", &b.kml_roma(20252)));
    let p2 = env.zip("QIP2.zip", &voci);
    // Solo KML (semestre dalle description): nuova geometria per B1, C2 sparisce.
    let b1_nuova = placemark(&b.zone[0], 20252, vec![vec![rect(12.3, 41.7, 12.4, 41.8)]]);
    let p3 = env.zip("QIP3.zip", &[voce_kml("H501.kml", &kml(&[], &[b1_nuova]))]);
    env.ok(&p1);
    env.ok(&p2);
    let (r3, out3) = env.ok(&p3);
    assert_eq!(r3.semestre, Some(20252));
    assert!(
        out3.contains("  Semestre 2025/2: 1 perimetri di zona da 1 file KML\n"),
        "{out3}"
    );
    assert_eq!(
        env.n("SELECT count(*) FROM geometria"),
        3,
        "orfane prima di completa"
    );

    assert_eq!(env.completa(), "Aggiornamento anagrafiche e indici...\n");
    assert_eq!(env.n("SELECT count(*) FROM geometria"), 1);
    assert_eq!(env.n("SELECT count(*) FROM geometria_rt"), 1);
    let c: (String, String, i64, i64) = env
        .conn
        .query_row(
            "SELECT descrizione, regione, primo_semestre, ultimo_semestre FROM comune WHERE comune_amm='I329'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        c,
        (
            "SANT'ANTONIO DI GALLURA".to_string(),
            "SARDEGNA".to_string(),
            20241,
            20252
        )
    );
    assert_eq!(
        env.n(
            "SELECT primo_semestre * 100000 + ultimo_semestre FROM comune WHERE comune_amm='H501'"
        ),
        20252 * 100000 + 20252
    );
    assert_eq!(
        env.s(
            "SELECT group_concat(nome || '|' || nome_norm || '|' || prov, ';')
             FROM (SELECT * FROM comune_nome WHERE comune_amm='I329' ORDER BY nome)"
        ),
        Some(
            "SANT'ANTONIO DI GALLURA|SANTANTONIO DI GALLURA|SS;Sant'Antonio|SANTANTONIO|SS"
                .to_string()
        )
    );
    assert_eq!(env.n("SELECT count(*) FROM comune"), 2);

    // Dopo completa la cache non contiene più le geometrie eliminate: reimportando QIP2 si
    // reinseriscono invece di puntare a id inesistenti.
    env.ok(&p2);
    assert_eq!(env.orfani(), 0);
    assert_eq!(env.n("SELECT count(*) FROM zona_geo"), 2);
}

#[test]
fn semestre_mancante_nei_csv_e_nei_kml() {
    let mut env = Env::new();
    let b = base();
    let csv = vec![(
        "ZONE/ANAGRAFE_ZONE.csv".to_string(),
        zone_csv("Quotazioni OMI", &b.zone),
    )];
    let p1 = env.zip("QI1.zip", &csv);
    assert_eq!(
        errore(env.import(&p1).0),
        "Impossibile determinare il semestre dei CSV"
    );
    let mut senza = placemark(&b.zone[0], 20252, b.roma_b1.clone());
    senza.semestre = None;
    let p2 = env.zip("QIP2.zip", &[voce_kml("H501.kml", &kml(&[], &[senza]))]);
    assert_eq!(
        errore(env.import(&p2).0),
        "Impossibile determinare il semestre dei KML"
    );
    assert!(env.conteggi().iter().all(|&(_, n)| n == 0));
}

#[test]
fn zip_corrotto_e_errore() {
    let mut env = Env::new();
    let p = env.dir.path().join("QIP1.zip");
    std::fs::write(&p, b"PK\x03\x04 non e' uno zip").unwrap();
    let (r, _) = env.import(&p);
    assert!(matches!(r, Err(OmiError::Zip(_))), "{r:?}");
}

/// Stima dei tempi su un QIP sintetico di dimensioni nazionali (circa 8000 comuni, 27 000 zone).
#[test]
#[ignore = "lento: cargo test --release -p omi-core --test import -- --ignored --nocapture"]
fn prestazioni_qip_nazionale_sintetico() {
    use zip::write::SimpleFileOptions;

    let mut env = Env::new();
    let sem = 20252;
    let path = env.dir.path().join("QIP_NAZIONALE.zip");
    let opzioni = SimpleFileOptions::default().compression_level(Some(1));
    let generazione = Instant::now();
    {
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let n_comuni = 8000;
        let mut zone = String::new();
        let mut valori = String::new();
        let mut n_zone = 0;
        for c in 0..n_comuni {
            let amm = format!("X{c:03}");
            let (lon0, lat0) = (
                7.0 + f64::from(c % 100) * 0.1,
                37.0 + f64::from(c / 100) * 0.1,
            );
            let mut pm = String::new();
            for k in 0..(3 + c % 2) {
                n_zone += 1;
                let link = format!("{amm}{k:06}");
                zone += &format!(
                    "CENTRO;REGIONE;PR;1000{c};{amm};;{amm};COMUNE {c};B;'ZONA {k}';B{k};{link};20;Abitazioni civili;NORMALE;1;\r\n"
                );
                for (tip, stato) in [
                    (20, "NORMALE"),
                    (20, "OTTIMO"),
                    (13, "NORMALE"),
                    (1, "NORMALE"),
                    (5, "NORMALE"),
                    (6, "NORMALE"),
                    (8, "NORMALE"),
                    (9, "NORMALE"),
                    (10, "NORMALE"),
                    (14, "NORMALE"),
                ] {
                    valori += &format!(
                        "CENTRO;REGIONE;PR;1000{c};{amm};;{amm};COMUNE {c};B;B{k};{link};{tip};Tipologia {tip};{stato};P;1200,5;1800;L;5,2;7,9;L;\r\n"
                    );
                }
                // Anello di 600 punti attorno al centro della zona.
                let (cx, cy) = (lon0 + 0.02 * f64::from(k), lat0 + 0.02);
                let mut coords = String::new();
                for i in 0..600 {
                    let t = f64::from(i) * std::f64::consts::TAU / 600.0;
                    let r = 0.008 * (1.0 + 0.1 * (7.0 * t).sin());
                    coords += &format!("{:.6},{:.6},0 ", cx + r * t.cos(), cy + r * t.sin());
                }
                pm += &format!(
                    "<Placemark><name>COMUNE {c} - Zona OMI B{k}</name><description><![CDATA[Semestre 2025/2]]></description><styleUrl>#s1</styleUrl><ExtendedData><Data name=\"LINKZONA\"><value>{link}</value></Data><Data name=\"CODCOM\"><value>{amm}</value></Data><Data name=\"CODZONA\"><value>B{k}</value></Data></ExtendedData><MultiGeometry><Polygon><outerBoundaryIs><LinearRing><coordinates>{coords}</coordinates></LinearRing></outerBoundaryIs></Polygon></MultiGeometry></Placemark>\n"
                );
            }
            // I KML si scrivono subito (i CSV alla fine): l'ordine delle voci non conta.
            zip.start_file(format!("{amm}.kml"), opzioni).unwrap();
            zip.write_all(format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document><Style id=\"s1\"><PolyStyle><color>7f00ff00</color></PolyStyle></Style>\n{pm}</Document></kml>\n").as_bytes()).unwrap();
        }
        let head_z = "Area_territoriale;Regione;Prov;Comune_ISTAT;Comune_cat;Sez;Comune_amm;Comune_descrizione;Fascia;Zona_Descr;Zona;LinkZona;Cod_tip_prev;Descr_tip_prev;Stato_prev;Microzona;";
        let head_v = "Area_territoriale;Regione;Prov;Comune_ISTAT;Comune_cat;Sez;Comune_amm;Comune_descrizione;Fascia;Zona;LinkZona;Cod_Tip;Descr_Tipologia;Stato;Stato_prev;Compr_min;Compr_max;Sup_NL_compr;Loc_min;Loc_max;Sup_NL_loc;";
        let t = titolo(sem);
        zip.start_file(format!("QIP_{sem}_ZONE.csv"), opzioni)
            .unwrap();
        zip.write_all(format!("{t}\r\n{head_z}\r\n{zone}").as_bytes())
            .unwrap();
        zip.start_file(format!("QIP_{sem}_VALORI.csv"), opzioni)
            .unwrap();
        zip.write_all(format!("{t}\r\n{head_v}\r\n{valori}").as_bytes())
            .unwrap();
        zip.finish().unwrap();
        println!("zone generate: {n_zone}");
    }
    let mb = std::fs::metadata(&path).unwrap().len() as f64 / (1024.0 * 1024.0);
    println!(
        "generazione: {:.1} s, ZIP di {mb:.1} MB",
        generazione.elapsed().as_secs_f64()
    );

    let inizio = Instant::now();
    let (r, out) = env.ok(&path);
    let import = inizio.elapsed().as_secs_f64();
    let inizio = Instant::now();
    env.completa();
    let completa = inizio.elapsed().as_secs_f64();
    println!("{out}");
    println!("import: {import:.1} s, completa: {completa:.1} s, {r:?}");
    assert_eq!(r.n_kml, 8000);
}
