// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Generatore di forniture OMI sintetiche per i test. Nessun dato reale:
//! comuni, zone, quotazioni e perimetri sono inventati; il codice fiscale è un esempio
//! sintetico. Usato anche dai test di `omi-import` tramite `#[path]`.

#![allow(dead_code)]

use std::io::Write;
use std::path::Path;

use omi_core::geometry::{Geometry, S};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// Codice fiscale sintetico d'esempio.
pub const CF: &str = "RSSMRA80A01H501U";

/// Una voce dello ZIP: nome (anche con cartella) e contenuto.
pub type Voce = (String, Vec<u8>);

/// Anagrafica sintetica di un comune.
#[derive(Clone)]
pub struct Comune {
    pub amm: &'static str,
    pub istat: &'static str,
    pub descr: &'static str,
    pub prov: &'static str,
    pub regione: &'static str,
}

pub const ROMA: Comune = Comune {
    amm: "H501",
    istat: "1058091",
    descr: "ROMA",
    prov: "RM",
    regione: "LAZIO",
};
pub const MILANO: Comune = Comune {
    amm: "F205",
    istat: "1015146",
    descr: "MILANO",
    prov: "MI",
    regione: "LOMBARDIA",
};

#[derive(Clone)]
pub struct Zona {
    pub comune: Comune,
    pub zona: String,
    pub link: String,
    pub fascia: String,
    pub descr: String,
}

pub fn zona(comune: &Comune, zona: &str, link: &str) -> Zona {
    Zona {
        comune: comune.clone(),
        zona: zona.to_string(),
        link: link.to_string(),
        fascia: zona[..1].to_string(),
        descr: format!("ZONA {zona} DI {}", comune.descr),
    }
}

#[derive(Clone)]
pub struct Valore {
    pub link: String,
    pub cod_tip: i64,
    pub descr: String,
    pub stato: String,
    pub stato_prev: String,
    pub compr: (f64, f64),
    pub loc: (f64, f64),
}

pub fn valore(link: &str, cod_tip: i64, stato: &str, compr: (f64, f64), loc: (f64, f64)) -> Valore {
    let descr = match cod_tip {
        20 => "Abitazioni civili",
        13 => "Box",
        _ => "Altro",
    };
    Valore {
        link: link.to_string(),
        cod_tip,
        descr: descr.to_string(),
        stato: stato.to_string(),
        stato_prev: if stato == "NORMALE" { "P" } else { "" }.to_string(),
        compr,
        loc,
    }
}

/// Titolo compatibile con il formato delle forniture.
pub fn titolo(sem: i64) -> String {
    format!(
        "Quotazioni OMI - Semestre {}/{} - elaborazione del 12/03/2026",
        sem / 10,
        sem % 10
    )
}

/// Numero con la virgola decimale.
fn num(v: f64) -> String {
    v.to_string().replace('.', ",")
}

/// CSV con BOM, CRLF, una riga vuota e un `;` finale come nei file dell'Agenzia.
fn csv(titolo: &str, header: &str, rows: &[String]) -> Vec<u8> {
    let mut s = format!("\u{feff}{titolo}\r\n{header};\r\n\r\n");
    for r in rows {
        s.push_str(r);
        s.push_str(";\r\n");
    }
    s.into_bytes()
}

pub fn zone_csv(titolo: &str, zone: &[Zona]) -> Vec<u8> {
    let header = "Area_territoriale;Regione;Prov;Comune_ISTAT;Comune_cat;Sez;Comune_amm;Comune_descrizione;Fascia;Zona_Descr;Zona;LinkZona;Cod_tip_prev;Descr_tip_prev;Stato_prev;Microzona";
    let rows: Vec<String> = zone
        .iter()
        .map(|z| {
            let c = &z.comune;
            format!(
                "CENTRO;{};{};{};{};;{};{};{};'{}';{};{};20;Abitazioni civili;NORMALE;1",
                c.regione,
                c.prov,
                c.istat,
                c.amm,
                c.amm,
                c.descr,
                z.fascia,
                z.descr,
                z.zona,
                z.link
            )
        })
        .collect();
    csv(titolo, header, &rows)
}

pub fn valori_csv(titolo: &str, valori: &[Valore]) -> Vec<u8> {
    let header = "Area_territoriale;Regione;Prov;Comune_ISTAT;Comune_cat;Sez;Comune_amm;Comune_descrizione;Fascia;Zona;LinkZona;Cod_Tip;Descr_Tipologia;Stato;Stato_prev;Compr_min;Compr_max;Sup_NL_compr;Loc_min;Loc_max;Sup_NL_loc";
    let rows: Vec<String> = valori
        .iter()
        .map(|v| {
            format!(
                "CENTRO;LAZIO;RM;1058091;H501;;H501;ROMA;B;B1;{};{};{};{};{};{};{};L;{};{};L",
                v.link,
                v.cod_tip,
                v.descr,
                v.stato,
                v.stato_prev,
                num(v.compr.0),
                num(v.compr.1),
                num(v.loc.0),
                num(v.loc.1)
            )
        })
        .collect();
    csv(titolo, header, &rows)
}

/// Anello in gradi; per i KML va chiuso (primo punto ripetuto in fondo).
pub type Anello = Vec<(f64, f64)>;

/// Rettangolo chiuso.
pub fn rect(lon0: f64, lat0: f64, lon1: f64, lat1: f64) -> Anello {
    vec![
        (lon0, lat0),
        (lon1, lat0),
        (lon1, lat1),
        (lon0, lat1),
        (lon0, lat0),
    ]
}

#[derive(Clone, Default)]
pub struct Placemark {
    pub nome: String,
    pub semestre: Option<i64>,
    pub style: Option<String>,
    pub link: Option<String>,
    pub codcom: Option<String>,
    pub codzona: Option<String>,
    /// Poligoni: il primo anello è il bordo esterno, gli altri buchi.
    pub poligoni: Vec<Vec<Anello>>,
}

/// Placemark con `ExtendedData` completo e nome `"{comune} - Zona OMI {zona}"`.
pub fn placemark(z: &Zona, sem: i64, poligoni: Vec<Vec<Anello>>) -> Placemark {
    Placemark {
        nome: format!("{} - Zona OMI {}", z.comune.descr, z.zona),
        semestre: Some(sem),
        style: None,
        link: Some(z.link.clone()),
        codcom: Some(z.comune.amm.to_string()),
        codzona: Some(z.zona.clone()),
        poligoni,
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn coords(anello: &Anello) -> String {
    anello
        .iter()
        .map(|(x, y)| format!("{x},{y},0"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn placemark_xml(p: &Placemark) -> String {
    let mut s = format!("  <Placemark>\n    <name>{}</name>\n", esc(&p.nome));
    if let Some(sem) = p.semestre {
        s += &format!(
            "    <description><![CDATA[<b>Semestre {}/{}</b><br/>{}]]></description>\n",
            sem / 10,
            sem % 10,
            p.nome
        );
    }
    if let Some(st) = &p.style {
        s += &format!("    <styleUrl>#{st}</styleUrl>\n");
    }
    s += "    <ExtendedData>\n";
    for (k, v) in [
        ("LINKZONA", &p.link),
        ("CODCOM", &p.codcom),
        ("CODZONA", &p.codzona),
    ] {
        if let Some(v) = v {
            s += &format!(
                "      <Data name=\"{k}\"><value>{}</value></Data>\n",
                esc(v)
            );
        }
    }
    s += "    </ExtendedData>\n    <MultiGeometry>\n";
    for pol in &p.poligoni {
        s += "      <Polygon>\n";
        for (i, anello) in pol.iter().enumerate() {
            let tag = if i == 0 {
                "outerBoundaryIs"
            } else {
                "innerBoundaryIs"
            };
            s += &format!(
                "        <{tag}><LinearRing><coordinates>{}</coordinates></LinearRing></{tag}>\n",
                coords(anello)
            );
        }
        s += "      </Polygon>\n";
    }
    s += "    </MultiGeometry>\n  </Placemark>\n";
    s
}

/// KML completo con gli stili `(id, colore)` e i placemark.
pub fn kml(stili: &[(&str, &str)], placemarks: &[Placemark]) -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<kml xmlns=\"http://www.opengis.net/kml/2.2\"><Document>\n",
    );
    for (id, colore) in stili {
        s += &format!(
            "  <Style id=\"{id}\"><LineStyle><color>ff000000</color></LineStyle><PolyStyle><color>{colore}</color></PolyStyle></Style>\n"
        );
    }
    for p in placemarks {
        s += &placemark_xml(p);
    }
    s += "</Document></kml>\n";
    s
}

/// Geometria attesa nel database per i poligoni di un placemark (microgradi, arrotondamento pari).
pub fn geometria(poligoni: &[Vec<Anello>]) -> Geometry {
    Geometry {
        polygons: poligoni
            .iter()
            .map(|pol| {
                pol.iter()
                    .map(|a| {
                        a.iter()
                            .flat_map(|&(x, y)| {
                                [
                                    (x * S).round_ties_even() as i32,
                                    (y * S).round_ties_even() as i32,
                                ]
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect(),
    }
}

/// Voci CSV di una fornitura: nomi `QI_1234_1_{AAAAS}_ZONE.csv` e `…_VALORI.csv` in una cartella.
pub fn voci_csv(sem_nome: i64, titolo: &str, zone: &[Zona], valori: &[Valore]) -> Vec<Voce> {
    vec![
        (
            format!("QI_1234_1/QI_1234_1_{sem_nome}_ZONE.csv"),
            zone_csv(titolo, zone),
        ),
        (
            format!("QI_1234_1/QI_1234_1_{sem_nome}_VALORI.csv"),
            valori_csv(titolo, valori),
        ),
    ]
}

pub fn voce_kml(nome: &str, testo: &str) -> Voce {
    (nome.to_string(), testo.as_bytes().to_vec())
}

/// Scrive uno ZIP con le voci indicate, nell'ordine dato.
pub fn write_zip(path: &Path, voci: &[Voce]) {
    let file = std::fs::File::create(path).unwrap();
    let mut zip = ZipWriter::new(file);
    for (nome, dati) in voci {
        zip.start_file(nome.as_str(), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(dati).unwrap();
    }
    zip.finish().unwrap();
}

/// Fornitura di base del semestre `sem`: 2 comuni, 3 zone, 5 quotazioni (con 2 stati), un KML per
/// comune; la zona B1 di Roma è un multipoligono con un buco e uno stile colorato.
pub struct Base {
    pub zone: Vec<Zona>,
    pub valori: Vec<Valore>,
    pub roma_b1: Vec<Vec<Anello>>,
    pub roma_c2: Vec<Vec<Anello>>,
    pub milano_b1: Vec<Vec<Anello>>,
}

pub fn base() -> Base {
    let zone = vec![
        zona(&ROMA, "B1", "RM00000001"),
        zona(&ROMA, "C2", "RM00000002"),
        zona(&MILANO, "B1", "MI00000001"),
    ];
    let valori = vec![
        valore("RM00000001", 20, "NORMALE", (3500.0, 5200.5), (12.5, 18.3)),
        valore("RM00000001", 20, "OTTIMO", (5000.0, 7000.0), (16.0, 22.0)),
        valore("RM00000002", 20, "NORMALE", (2500.0, 3400.0), (9.1, 12.0)),
        valore("MI00000001", 20, "NORMALE", (6000.0, 9000.0), (20.0, 30.0)),
        valore(
            "MI00000001",
            13,
            "NORMALE",
            (40000.0, 60000.0),
            (150.0, 200.0),
        ),
    ];
    Base {
        zone,
        valori,
        roma_b1: vec![
            vec![
                rect(12.48, 41.89, 12.5, 41.91),
                rect(12.485, 41.895, 12.49, 41.9),
            ],
            vec![rect(12.51, 41.89, 12.52, 41.9)],
        ],
        roma_c2: vec![vec![rect(12.4, 41.8, 12.45, 41.85)]],
        milano_b1: vec![vec![rect(9.18, 45.46, 9.2, 45.47)]],
    }
}

impl Base {
    pub fn kml_roma(&self, sem: i64) -> String {
        let mut b1 = placemark(&self.zone[0], sem, self.roma_b1.clone());
        b1.style = Some("s1".to_string());
        let c2 = placemark(&self.zone[1], sem, self.roma_c2.clone());
        kml(&[("s1", "7f00ff00"), ("s2", "7f0000ff")], &[b1, c2])
    }

    pub fn kml_milano(&self, sem: i64) -> String {
        kml(
            &[],
            &[placemark(&self.zone[2], sem, self.milano_b1.clone())],
        )
    }

    /// Voci di un QIP completo (CSV + KML).
    pub fn qip(&self, sem: i64) -> Vec<Voce> {
        let mut v = voci_csv(sem, &titolo(sem), &self.zone, &self.valori);
        v.push(voce_kml("QI_1234_1/H501.kml", &self.kml_roma(sem)));
        v.push(voce_kml("QI_1234_1/F205.kml", &self.kml_milano(sem)));
        v
    }

    /// Voci di un QI (solo CSV).
    pub fn qi(&self, sem: i64) -> Vec<Voce> {
        voci_csv(sem, &titolo(sem), &self.zone, &self.valori)
    }
}

// ---------------------------------------------------------------------------------------------
// Scenario del server MCP
// ---------------------------------------------------------------------------------------------

/// Due comuni con lo stesso nome in province diverse.
pub const SAMONE_TO: Comune = Comune {
    amm: "H753",
    istat: "1001235",
    descr: "SAMONE",
    prov: "TO",
    regione: "PIEMONTE",
};
pub const SAMONE_TN: Comune = Comune {
    amm: "H754",
    istat: "4022165",
    descr: "SAMONE",
    prov: "TN",
    regione: "TRENTINO-ALTO ADIGE",
};
/// Un comune che cambia nome tra i due semestri dello scenario.
pub const BORGO_VECCHIO: Comune = Comune {
    amm: "B999",
    istat: "1001999",
    descr: "BORGO VECCHIO",
    prov: "TO",
    regione: "PIEMONTE",
};
pub const BORGO_NUOVO: Comune = Comune {
    descr: "BORGO NUOVO",
    ..BORGO_VECCHIO
};

/// Semestre con perimetri (QIP) e ultimo semestre, senza perimetri (QI).
pub const SEM_PERIMETRI: i64 = 20242;
pub const SEM_ULTIMO: i64 = 20251;

/// QIP del semestre 2024/2: Roma B1 (tipologie 20 e 13, stati NORMALE, OTTIMO e SCADENTE) e C2,
/// Milano B1, i due SAMONE e BORGO VECCHIO; perimetri per Roma e Milano.
pub fn scenario_qip_perimetri() -> Vec<Voce> {
    let b = base();
    let sem = SEM_PERIMETRI;
    let zone = vec![
        b.zone[0].clone(),
        b.zone[1].clone(),
        b.zone[2].clone(),
        zona(&SAMONE_TO, "B1", "TO00000001"),
        zona(&SAMONE_TN, "R1", "TN00000001"),
        zona(&BORGO_VECCHIO, "B1", "TO00000099"),
    ];
    let valori = vec![
        valore("RM00000001", 20, "NORMALE", (3500.0, 5200.5), (12.5, 18.3)),
        valore("RM00000001", 20, "OTTIMO", (5000.0, 7000.0), (16.0, 22.0)),
        valore("RM00000001", 13, "NORMALE", (1800.0, 2500.0), (8.0, 11.0)),
        valore("RM00000001", 13, "SCADENTE", (1200.0, 1600.0), (5.5, 7.0)),
        valore("RM00000002", 20, "NORMALE", (2500.0, 3400.0), (9.1, 12.0)),
        valore("MI00000001", 20, "NORMALE", (6000.0, 9000.0), (20.0, 30.0)),
        valore("TO00000001", 20, "NORMALE", (900.0, 1300.0), (3.5, 5.0)),
        valore("TN00000001", 20, "NORMALE", (1100.0, 1500.0), (4.0, 6.0)),
        valore("TO00000099", 20, "NORMALE", (700.0, 1000.0), (3.0, 4.0)),
    ];
    let mut v = voci_csv(sem, &titolo(sem), &zone, &valori);
    v.push(voce_kml("QI_1234_1/H501.kml", &b.kml_roma(sem)));
    v.push(voce_kml("QI_1234_1/F205.kml", &b.kml_milano(sem)));
    v
}

/// QI del semestre 2025/1, senza perimetri: Roma C2 sparisce e compare D5 (che non ha perimetro);
/// BORGO VECCHIO diventa BORGO NUOVO.
pub fn scenario_qi_ultimo() -> Vec<Voce> {
    let b = base();
    let sem = SEM_ULTIMO;
    let zone = vec![
        b.zone[0].clone(),
        zona(&ROMA, "D5", "RM00000005"),
        b.zone[2].clone(),
        zona(&SAMONE_TO, "B1", "TO00000001"),
        zona(&SAMONE_TN, "R1", "TN00000001"),
        zona(&BORGO_NUOVO, "B1", "TO00000099"),
    ];
    let valori = vec![
        valore("RM00000001", 20, "NORMALE", (3600.0, 5300.0), (12.8, 18.6)),
        valore("RM00000001", 20, "OTTIMO", (5100.0, 7100.0), (16.5, 22.5)),
        valore("RM00000001", 13, "NORMALE", (1850.0, 2550.0), (8.2, 11.2)),
        valore("RM00000001", 13, "SCADENTE", (1250.0, 1650.0), (5.6, 7.1)),
        valore("RM00000005", 20, "NORMALE", (2000.0, 2800.0), (7.5, 10.0)),
        valore("MI00000001", 20, "NORMALE", (6100.0, 9100.0), (20.5, 30.5)),
        valore("TO00000001", 20, "NORMALE", (950.0, 1350.0), (3.6, 5.1)),
        valore("TN00000001", 20, "NORMALE", (1150.0, 1550.0), (4.1, 6.1)),
        valore("TO00000099", 20, "NORMALE", (720.0, 1020.0), (3.1, 4.1)),
    ];
    voci_csv(sem, &titolo(sem), &zone, &valori)
}

/// Crea `dir/{nome_db}` importando gli ZIP indicati (`(nome del file, voci)`) e chiamando
/// `completa`. Restituisce il percorso del database.
pub fn importa(dir: &Path, nome_db: &str, zips: &[(&str, Vec<Voce>)]) -> std::path::PathBuf {
    let db = dir.join(nome_db);
    let mut conn = omi_core::db::open(&db, false).unwrap();
    let mut imp = omi_core::import::Importer::new(&conn).unwrap();
    let mut out = Vec::new();
    for (nome, voci) in zips {
        let path = dir.join(nome);
        write_zip(&path, voci);
        let report = imp.import_zip(&mut conn, &path, &mut out).unwrap();
        assert!(!report.ignorato, "{nome}: {report:?}");
    }
    imp.completa(&mut conn, &mut out).unwrap();
    db
}

/// Database dello scenario del server: 2024/2 con perimetri e 2025/1 senza.
pub fn scenario_server(dir: &Path) -> std::path::PathBuf {
    importa(
        dir,
        "omi.db",
        &[
            ("QIP_20242.zip", scenario_qip_perimetri()),
            ("QI_20251.zip", scenario_qi_ultimo()),
        ],
    )
}
