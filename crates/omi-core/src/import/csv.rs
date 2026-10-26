// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! CSV `*_ZONE.csv` e `*_VALORI.csv`: decodifica, righe, intestazioni, titolo e import nelle tabelle
//! `zona` e `valore`.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use regex::Regex;
use rusqlite::{Connection, params};

use crate::error::Result;
use crate::text::{field, parse_decimal, parse_int};

// Invariante: i pattern sono costanti e validi (verificati dai test di questo modulo).
static RE_SEMESTRE_TITOLO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Semestre\s+(\d{4})\s*/\s*(\d)").expect("espressione regolare costante valida")
});
static RE_ELABORAZIONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)elaborazione del\s+(.+)$").expect("espressione regolare costante valida")
});

const SQL_ZONA: &str = "INSERT OR REPLACE INTO zona(semestre, link_zona, area_territoriale, regione, prov, comune_istat, comune_cat, sez,
    comune_amm, comune_descr, fascia, zona, zona_descr, cod_tip_prev, descr_tip_prev, stato_prev, microzona)
VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";

const SQL_VALORE: &str = "INSERT OR REPLACE INTO valore(semestre, link_zona, cod_tip, stato, stato_prev, compr_min, compr_max, sup_nl_compr,
    loc_min, loc_max, sup_nl_loc)
VALUES (?,?,?,?,?,?,?,?,?,?,?)";

/// Testo UTF-8 senza BOM; i byte non validi diventano U+FFFD.
pub(super) fn decode(bytes: Vec<u8>) -> String {
    let mut s = String::from_utf8(bytes)
        .unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned());
    if s.starts_with('\u{feff}') {
        s.drain(..'\u{feff}'.len_utf8());
    }
    s
}

/// Semestre `AAAAS` dal titolo, se presente.
pub(super) fn title_semestre(title: &str) -> Option<i64> {
    let c = RE_SEMESTRE_TITOLO.captures(title)?;
    let anno: i64 = c[1].parse().ok()?;
    let s: i64 = c[2].parse().ok()?;
    Some(anno * 10 + s)
}

/// Data di elaborazione dal titolo ripulito, se presente.
pub(super) fn title_elaborazione(title: &str) -> Option<String> {
    let c = RE_ELABORAZIONE.captures(title.trim())?;
    let v = c[1].trim();
    (!v.is_empty()).then(|| v.to_string())
}

/// Un CSV già letto: titolo, colonne dell'intestazione e righe di dati (non ancora divise).
pub(super) struct Csv<'a> {
    pub title: &'a str,
    columns: HashMap<String, usize>,
    rows: Vec<&'a str>,
}

impl<'a> Csv<'a> {
    /// Righe separate da `\n`, `\r\n` o `\r`; le righe vuote si saltano. La prima riga è il titolo,
    /// la seconda l'intestazione.
    pub fn parse(text: &'a str) -> Csv<'a> {
        let mut lines = text.split(['\r', '\n']).filter(|l| !l.trim().is_empty());
        let title = lines.next().unwrap_or("");
        let header = lines.next().unwrap_or("");
        let mut columns = HashMap::new();
        for (i, name) in header.split(';').enumerate() {
            let name = name.trim();
            if !name.is_empty() {
                // Se un nome compare due volte vale l'ultima colonna.
                columns.insert(name.to_lowercase(), i);
            }
        }
        Csv {
            title,
            columns,
            rows: lines.collect(),
        }
    }

    /// Indice di una colonna, senza distinzione di maiuscole.
    pub fn col(&self, name: &str) -> Option<usize> {
        self.columns.get(&name.to_lowercase()).copied()
    }

    /// Valori ripuliti di una colonna, nell'ordine delle righe.
    fn values(&self, name: &str) -> impl Iterator<Item = String> + '_ {
        let idx = self.col(name);
        self.rows
            .iter()
            .filter_map(move |line| idx.and_then(|i| line.split(';').nth(i)).and_then(field))
    }

    /// Valori distinti di una colonna; con `ignore_case` resta la prima forma incontrata.
    pub fn distinct(&self, name: &str, ignore_case: bool) -> Vec<String> {
        let mut seen = HashSet::new();
        self.values(name)
            .filter(|v| {
                if ignore_case {
                    seen.insert(v.to_uppercase())
                } else {
                    seen.insert(v.clone())
                }
            })
            .collect()
    }
}

/// Campi di una riga divisa su `;`; i campi mancanti in coda sono assenti.
struct Row<'a>(Vec<&'a str>);

impl Row<'_> {
    fn get(&self, idx: Option<usize>) -> Option<String> {
        idx.and_then(|i| self.0.get(i)).and_then(|s| field(s))
    }

    fn int(&self, idx: Option<usize>) -> Option<i64> {
        self.get(idx).and_then(|s| parse_int(&s))
    }

    fn decimal(&self, idx: Option<usize>) -> Option<f64> {
        self.get(idx).and_then(|s| parse_decimal(&s))
    }
}

/// ZONE → `zona`. Restituisce le righe importate e la data di elaborazione del titolo.
pub(super) fn import_zone(
    conn: &Connection,
    csv: &Csv,
    sem: i64,
    tipologie: &mut HashMap<i64, String>,
) -> Result<(usize, Option<String>)> {
    let c = |n: &str| csv.col(n);
    let (link, area, regione, prov, istat, cat, sez) = (
        c("LinkZona"),
        c("Area_territoriale"),
        c("Regione"),
        c("Prov"),
        c("Comune_ISTAT"),
        c("Comune_cat"),
        c("Sez"),
    );
    let (amm, descr, fascia, zona, zona_descr) = (
        c("Comune_amm"),
        c("Comune_descrizione"),
        c("Fascia"),
        c("Zona"),
        c("Zona_Descr"),
    );
    let (cod_tip_prev, descr_tip_prev, stato_prev, microzona) = (
        c("Cod_tip_prev"),
        c("Descr_tip_prev"),
        c("Stato_prev"),
        c("Microzona"),
    );

    let mut stmt = conn.prepare_cached(SQL_ZONA)?;
    let mut n = 0;
    for line in &csv.rows {
        let row = Row(line.split(';').collect());
        let (Some(link_zona), Some(comune_amm), Some(zona_v)) =
            (row.get(link), row.get(amm), row.get(zona))
        else {
            continue;
        };
        let tip = row.int(cod_tip_prev);
        let tip_descr = row.get(descr_tip_prev);
        if let (Some(t), Some(d)) = (tip, &tip_descr) {
            tipologie.insert(t, d.clone());
        }
        stmt.execute(params![
            sem,
            link_zona,
            row.get(area),
            row.get(regione),
            row.get(prov),
            row.get(istat),
            row.get(cat),
            row.get(sez),
            comune_amm,
            row.get(descr),
            row.get(fascia),
            zona_v,
            row.get(zona_descr),
            tip,
            tip_descr,
            row.get(stato_prev),
            row.int(microzona),
        ])?;
        n += 1;
    }
    Ok((n, title_elaborazione(csv.title)))
}

/// VALORI → `valore`. Restituisce le righe importate e la data di elaborazione del titolo.
pub(super) fn import_valori(
    conn: &Connection,
    csv: &Csv,
    sem: i64,
    tipologie: &mut HashMap<i64, String>,
) -> Result<(usize, Option<String>)> {
    let c = |n: &str| csv.col(n);
    let (link, cod_tip, descr_tip, stato, stato_prev) = (
        c("LinkZona"),
        c("Cod_Tip"),
        c("Descr_Tipologia"),
        c("Stato"),
        c("Stato_prev"),
    );
    let (compr_min, compr_max, sup_compr, loc_min, loc_max, sup_loc) = (
        c("Compr_min"),
        c("Compr_max"),
        c("Sup_NL_compr"),
        c("Loc_min"),
        c("Loc_max"),
        c("Sup_NL_loc"),
    );

    let mut stmt = conn.prepare_cached(SQL_VALORE)?;
    let mut n = 0;
    for line in &csv.rows {
        let row = Row(line.split(';').collect());
        let (Some(link_zona), Some(tip)) = (row.get(link), row.int(cod_tip)) else {
            continue;
        };
        if let Some(d) = row.get(descr_tip) {
            tipologie.insert(tip, d);
        }
        stmt.execute(params![
            sem,
            link_zona,
            tip,
            // Lo stato fa parte della chiave: se manca si usa la stringa vuota.
            row.get(stato).unwrap_or_default(),
            row.get(stato_prev),
            row.decimal(compr_min),
            row.decimal(compr_max),
            row.get(sup_compr),
            row.decimal(loc_min),
            row.decimal(loc_max),
            row.get(sup_loc),
        ])?;
        n += 1;
    }
    Ok((n, title_elaborazione(csv.title)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodifica_bom_e_byte_non_validi() {
        assert_eq!(decode(b"\xEF\xBB\xBFabc".to_vec()), "abc");
        assert_eq!(decode(b"a\xFFb".to_vec()), "a\u{FFFD}b");
        assert_eq!(decode("Città".as_bytes().to_vec()), "Città");
    }

    #[test]
    fn righe_e_intestazioni() {
        let text = "Titolo\r\n\r\n A ; b ;;a;\rx1;x2;x3;x4\n\n  \ny1;y2";
        let csv = Csv::parse(text);
        assert_eq!(csv.title, "Titolo");
        assert_eq!(csv.rows, vec!["x1;x2;x3;x4", "y1;y2"]);
        // Nome duplicato: vale l'ultima colonna; nomi vuoti ignorati; maiuscole indifferenti.
        assert_eq!(csv.col("a"), Some(3));
        assert_eq!(csv.col("B"), Some(1));
        assert_eq!(csv.col(""), None);
        assert_eq!(csv.col("c"), None);
        let row = Row(csv.rows[1].split(';').collect());
        assert_eq!(row.get(csv.col("b")), Some("y2".to_string()));
        assert_eq!(row.get(csv.col("a")), None, "campo mancante in coda");
    }

    #[test]
    fn valori_distinti() {
        let csv = Csv::parse("T\nComune_amm;LinkZona\nh501;L1\nH501;L1\n'F205';L2\n;L3");
        assert_eq!(csv.distinct("Comune_amm", true), vec!["h501", "F205"]);
        assert_eq!(
            csv.distinct("comune_AMM", false),
            vec!["h501", "H501", "F205"]
        );
        assert_eq!(csv.distinct("LinkZona", false), vec!["L1", "L2", "L3"]);
        assert!(csv.distinct("Assente", false).is_empty());
    }

    #[test]
    fn titolo() {
        let t = "Quotazioni OMI - Semestre 2025/2 - elaborazione del 12/03/2026 ";
        assert_eq!(title_semestre(t), Some(20252));
        assert_eq!(title_semestre("SEMESTRE 2011 / 1"), Some(20111));
        assert_eq!(title_semestre("Anno 2025"), None);
        assert_eq!(title_elaborazione(t), Some("12/03/2026".to_string()));
        assert_eq!(
            title_elaborazione("ELABORAZIONE DEL  1 marzo 2026"),
            Some("1 marzo 2026".to_string())
        );
        assert_eq!(title_elaborazione("Semestre 2025/2"), None);
    }
}
