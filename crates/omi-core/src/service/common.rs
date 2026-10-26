// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Funzioni comuni agli strumenti e ricerca spaziale.

use std::collections::HashMap;

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OptionalExtension, Params, named_params, params};
use serde_json::{Map, Value, json};

use crate::db::{parse_semestre, sem_index, sem_label};
use crate::error::{OmiError, Result};
use crate::geometry::Geometry;
use crate::json::sql_value;
use crate::text::norm;

/// Una riga di risultato: colonne → valori JSON, nell'ordine della `SELECT`.
pub(crate) type Riga = Map<String, Value>;

pub(crate) fn validazione(msg: impl Into<String>) -> OmiError {
    OmiError::Validazione(msg.into())
}

/// Esegue una query e restituisce le righe come mappe colonna → valore.
pub(crate) fn query_rows(conn: &Connection, sql: &str, p: impl Params) -> Result<Vec<Riga>> {
    let mut stmt = conn.prepare_cached(sql)?;
    let names: Vec<String> = stmt.column_names().map(String::from).collect();
    let rows = stmt.query_map(p, |r| {
        let mut m = Map::with_capacity(names.len());
        for (i, n) in names.iter().enumerate() {
            m.insert(n.clone(), sql_value(r.get_ref(i)?));
        }
        Ok(m)
    })?;
    Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
}

pub(crate) fn txt<'a>(r: &'a Riga, k: &str) -> Option<&'a str> {
    r.get(k).and_then(Value::as_str)
}

pub(crate) fn int(r: &Riga, k: &str) -> Option<i64> {
    r.get(k).and_then(Value::as_i64)
}

pub(crate) fn val(r: &Riga, k: &str) -> Value {
    r.get(k).cloned().unwrap_or(Value::Null)
}

/// Etichetta `AAAA/S` di un semestre facoltativo.
pub(crate) fn label(s: Option<i64>) -> Value {
    s.map_or(Value::Null, |s| Value::from(sem_label(s)))
}

// ---------------------------------------------------------------------------------------------
// Tabelle di decodifica
// ---------------------------------------------------------------------------------------------

pub(crate) fn fascia_descr(fascia: Option<&str>) -> Value {
    match fascia {
        Some("B") => "Centrale".into(),
        Some("C") => "Semicentrale".into(),
        Some("D") => "Periferica".into(),
        Some("E") => "Suburbana".into(),
        Some("R") => "Extraurbana/rurale".into(),
        _ => Value::Null,
    }
}

pub(crate) fn stato_descr(stato: Option<&str>) -> Value {
    match stato {
        Some("N") => "NORMALE".into(),
        Some("O") => "OTTIMO".into(),
        Some("S") => "SCADENTE".into(),
        Some(s) => s.into(),
        None => Value::Null,
    }
}

pub(crate) fn superficie(sup: Option<&str>) -> Value {
    match sup {
        Some("L") => "lorda".into(),
        Some("N") => "netta".into(),
        Some(s) => s.into(),
        None => Value::Null,
    }
}

// ---------------------------------------------------------------------------------------------
// Semestri
// ---------------------------------------------------------------------------------------------

pub(crate) fn resolve_semestre(conn: &Connection, semestre: Option<&str>) -> Result<i64> {
    if let Some(s) = parse_semestre(semestre)? {
        let esiste = conn
            .query_row(
                "SELECT 1 FROM semestre WHERE semestre=? AND n_zone>0",
                [s],
                |_| Ok(()),
            )
            .optional()?
            .is_some();
        if !esiste {
            return Err(validazione(format!(
                "Semestre {} non presente nel database. Usare elenco_semestri.",
                sem_label(s)
            )));
        }
        return Ok(s);
    }
    let ultimo: Option<i64> = conn.query_row(
        "SELECT max(semestre) FROM semestre WHERE n_zone>0",
        [],
        |r| r.get(0),
    )?;
    ultimo.ok_or_else(|| validazione("Il database non contiene dati OMI."))
}

/// Semestre con perimetri più vicino a `sem`.
pub(crate) fn geo_semestre(conn: &Connection, sem: i64) -> Result<i64> {
    conn.query_row(
        "SELECT semestre FROM semestre WHERE n_geometrie>0 ORDER BY abs((semestre/10)*2+semestre%10-@i), semestre DESC LIMIT 1",
        [sem_index(sem)],
        |r| r.get(0),
    )
    .optional()?
    .ok_or_else(|| validazione("Nel database non sono presenti perimetri di zona (file KML)."))
}

// ---------------------------------------------------------------------------------------------
// Comuni
// ---------------------------------------------------------------------------------------------

/// Provincia ripulita: `None` se assente o vuota.
pub(crate) fn provincia_norm(provincia: Option<&str>) -> Option<String> {
    provincia
        .map(|p| p.trim().to_uppercase())
        .filter(|p| !p.is_empty())
}

fn is_codice_catastale(t: &str) -> bool {
    let mut chars = t.chars();
    chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.clone().count() == 3
        && chars.all(|c| c.is_ascii_digit())
}

pub(crate) fn find_comuni(
    conn: &Connection,
    testo: &str,
    provincia: Option<&str>,
) -> Result<Vec<Riga>> {
    let t = testo.trim();
    let tu = t.to_uppercase();
    let p = provincia_norm(provincia);

    let rows = if is_codice_catastale(&tu) {
        query_rows(conn, "SELECT c.* FROM comune c WHERE c.comune_amm=?", [&tu])?
    } else if tu.chars().all(|c| c.is_ascii_digit()) {
        query_rows(
            conn,
            "SELECT c.* FROM comune c WHERE c.comune_istat=?1 OR substr(c.comune_istat,-6)=substr('000000'||?1,-6)",
            [&tu],
        )?
    } else {
        let n = norm(t);
        let mut rows = query_rows(
            conn,
            "SELECT c.* FROM comune c WHERE c.comune_amm IN (SELECT comune_amm FROM comune_nome WHERE nome_norm=?)",
            [&n],
        )?;
        for pattern in [format!("{n}%"), format!("%{n}%")] {
            if !rows.is_empty() {
                break;
            }
            rows = query_rows(
                conn,
                "SELECT c.* FROM comune c WHERE c.comune_amm IN (SELECT comune_amm FROM comune_nome WHERE nome_norm LIKE ?) ORDER BY c.descrizione LIMIT 200",
                [&pattern],
            )?;
        }
        rows
    };

    let Some(p) = p else {
        return Ok(rows);
    };
    let p_norm = norm(&p);
    Ok(rows
        .into_iter()
        .filter(|r| {
            txt(r, "prov").is_some_and(|prov| prov.to_uppercase() == p || norm(prov) == p_norm)
        })
        .collect())
}

pub(crate) fn resolve_comune(
    conn: &Connection,
    comune: &str,
    provincia: Option<&str>,
) -> Result<Riga> {
    let mut rows = find_comuni(conn, comune, provincia)?;
    match rows.len() {
        1 => Ok(rows.swap_remove(0)),
        0 => {
            let in_provincia = if provincia_norm(provincia).is_some() {
                format!(" in provincia {}", provincia.unwrap_or_default())
            } else {
                String::new()
            };
            Err(validazione(format!(
                "Nessun comune trovato per '{comune}'{in_provincia}."
            )))
        }
        _ => {
            let n = norm(comune);
            let esatti: Vec<usize> = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| norm(txt(r, "descrizione").unwrap_or_default()) == n)
                .map(|(i, _)| i)
                .collect();
            if let [i] = esatti[..] {
                return Ok(rows.swap_remove(i));
            }
            let candidati: Vec<String> = rows
                .iter()
                .take(20)
                .map(|r| {
                    format!(
                        "{} ({}) [{}]",
                        txt(r, "descrizione").unwrap_or_default(),
                        txt(r, "prov").unwrap_or_default(),
                        txt(r, "comune_amm").unwrap_or_default()
                    )
                })
                .collect();
            Err(validazione(format!(
                "'{comune}' è ambiguo, specificare la provincia o il codice catastale. Candidati: {}",
                candidati.join(", ")
            )))
        }
    }
}

pub(crate) fn comune_info(r: &Riga) -> Value {
    json!({
        "codice_catastale": val(r, "comune_amm"),
        "codice_istat": val(r, "comune_istat"),
        "nome": val(r, "descrizione"),
        "provincia": val(r, "prov"),
        "regione": val(r, "regione"),
        "area_territoriale": val(r, "area_territoriale"),
        "primo_semestre": label(int(r, "primo_semestre")),
        "ultimo_semestre": label(int(r, "ultimo_semestre")),
    })
}

/// Riga di `comune` per codice catastale, se esiste.
pub(crate) fn comune_by_amm(conn: &Connection, amm: &str) -> Result<Option<Riga>> {
    Ok(
        query_rows(conn, "SELECT * FROM comune WHERE comune_amm=?", [amm])?
            .into_iter()
            .next(),
    )
}

// ---------------------------------------------------------------------------------------------
// Zone, quotazioni, perimetri
// ---------------------------------------------------------------------------------------------

pub(crate) fn zona_info(z: &Riga) -> Value {
    let tipologia = if val(z, "cod_tip_prev").is_null() {
        Value::Null
    } else {
        json!({"codice": val(z, "cod_tip_prev"), "descrizione": val(z, "descr_tip_prev")})
    };
    json!({
        "zona": val(z, "zona"),
        "link_zona": val(z, "link_zona"),
        "fascia": val(z, "fascia"),
        "fascia_descr": fascia_descr(txt(z, "fascia")),
        "descrizione": val(z, "zona_descr"),
        "microzona": val(z, "microzona"),
        "tipologia_prevalente": tipologia,
        "stato_prevalente": stato_descr(txt(z, "stato_prev")),
        "comune_catastale": val(z, "comune_cat"),
        "sezione": val(z, "sez"),
    })
}

/// Oggetto `{min, max, superficie}`, assente se `min` e `max` sono entrambi NULL.
fn intervallo(r: &Riga, min: &str, max: &str, sup: &str) -> Value {
    if val(r, min).is_null() && val(r, max).is_null() {
        return Value::Null;
    }
    json!({"min": val(r, min), "max": val(r, max), "superficie": superficie(txt(r, sup))})
}

pub(crate) fn valori(conn: &Connection, sem: i64, link_zona: &str) -> Result<Vec<Value>> {
    let rows = query_rows(
        conn,
        "SELECT v.*, t.descrizione AS tip_descr FROM valore v LEFT JOIN tipologia t ON t.cod_tip=v.cod_tip
WHERE v.semestre=? AND v.link_zona=? ORDER BY v.cod_tip, v.stato",
        params![sem, link_zona],
    )?;
    Ok(rows
        .iter()
        .map(|r| {
            json!({
                "cod_tipologia": val(r, "cod_tip"),
                "tipologia": val(r, "tip_descr"),
                "stato": txt(r, "stato").filter(|s| !s.is_empty()),
                "stato_prevalente": (txt(r, "stato_prev") == Some("P")).then_some(true),
                "compravendita_eur_mq": intervallo(r, "compr_min", "compr_max", "sup_nl_compr"),
                "locazione_eur_mq_mese": intervallo(r, "loc_min", "loc_max", "sup_nl_loc"),
            })
        })
        .collect())
}

/// Riga della query di `geo_info` con il blob della geometria.
struct GeoRiga {
    campi: Riga,
    dati: Vec<u8>,
}

pub(crate) fn geo_info(
    conn: &Connection,
    comune_amm: &str,
    zona: &str,
    link_zona: Option<&str>,
    sem: i64,
    includi_geometria: bool,
) -> Result<Value> {
    let mut stmt = conn.prepare_cached(
        "SELECT zg.semestre, zg.link_zona, zg.colore, g.id, g.min_lon, g.min_lat, g.max_lon, g.max_lat, g.centro_lon, g.centro_lat,
       g.area_km2, g.n_poligoni, g.n_punti, g.dati
FROM zona_geo zg JOIN geometria g ON g.id=zg.geometria_id
WHERE zg.comune_amm=? AND zg.zona=?
ORDER BY abs((zg.semestre/10)*2+zg.semestre%10-@i), zg.semestre DESC",
    )?;
    let names: Vec<String> = stmt.column_names().map(String::from).collect();
    let rows = stmt
        .query_map(params![comune_amm, zona, sem_index(sem)], |r| {
            let mut campi = Map::with_capacity(names.len());
            for (i, n) in names.iter().enumerate().take(names.len() - 1) {
                campi.insert(n.clone(), sql_value(r.get_ref(i)?));
            }
            Ok(GeoRiga {
                campi,
                dati: r.get("dati")?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;

    let Some(gs) = rows.first().and_then(|r| int(&r.campi, "semestre")) else {
        return Ok(Value::Null);
    };
    let stesso_semestre: Vec<&GeoRiga> = rows
        .iter()
        .filter(|r| int(&r.campi, "semestre") == Some(gs))
        .collect();
    let mut scelte = stesso_semestre.clone();
    if stesso_semestre.len() > 1
        && let Some(link) = link_zona
    {
        let stesso_link: Vec<&GeoRiga> = stesso_semestre
            .iter()
            .copied()
            .filter(|r| txt(&r.campi, "link_zona") == Some(link))
            .collect();
        if !stesso_link.is_empty() {
            scelte = stesso_link;
        }
    }
    // `scelte` contiene almeno la prima riga, che ha semestre `gs`.
    let Some(g) = scelte.first() else {
        return Ok(Value::Null);
    };
    let c = &g.campi;
    let geometria = if includi_geometria {
        Geometry::decode(&g.dati)?.to_geojson()
    } else {
        Value::Null
    };
    Ok(json!({
        "semestre_perimetro": sem_label(gs),
        "centro": {"lat": val(c, "centro_lat"), "lon": val(c, "centro_lon")},
        "bbox": {
            "min_lat": val(c, "min_lat"),
            "min_lon": val(c, "min_lon"),
            "max_lat": val(c, "max_lat"),
            "max_lon": val(c, "max_lon"),
        },
        "area_km2": val(c, "area_km2"),
        "n_poligoni": val(c, "n_poligoni"),
        "n_punti": val(c, "n_punti"),
        "colore_kml": val(c, "colore"),
        "geometria": geometria,
    }))
}

pub(crate) fn zona_completa(
    conn: &Connection,
    z: &Riga,
    sem: i64,
    includi_geometria: bool,
) -> Result<Value> {
    let link = txt(z, "link_zona").unwrap_or_default();
    Ok(json!({
        "info": zona_info(z),
        "valori": valori(conn, sem, link)?,
        "perimetro": geo_info(
            conn,
            txt(z, "comune_amm").unwrap_or_default(),
            txt(z, "zona").unwrap_or_default(),
            txt(z, "link_zona"),
            sem,
            includi_geometria,
        )?,
    }))
}

/// Righe di `zona` per comune, codice zona e semestre.
pub(crate) fn zone_by_codice(
    conn: &Connection,
    comune_amm: &str,
    zona: &str,
    sem: i64,
) -> Result<Vec<Riga>> {
    query_rows(
        conn,
        "SELECT * FROM zona WHERE comune_amm=? AND zona=? AND semestre=?",
        params![comune_amm, zona, sem],
    )
}

// ---------------------------------------------------------------------------------------------
// Ricerca spaziale
// ---------------------------------------------------------------------------------------------

/// Zona candidata per un punto, con la distanza in metri (0 se il punto è interno).
#[derive(Debug, Clone)]
pub(crate) struct Candidato {
    pub comune_amm: String,
    pub zona: String,
    pub link_zona: Option<String>,
    pub distanza: f64,
}

/// Candidati con margine `m` gradi nel semestre di perimetri `gs`, nell'ordine restituito dalla
/// query. `cache` conserva per id la distanza già calcolata, così ogni geometria si decodifica una
/// sola volta per richiesta.
pub(crate) fn candidati(
    conn: &Connection,
    gs: i64,
    lon: f64,
    lat: f64,
    m: f64,
    cache: &mut HashMap<i64, f64>,
) -> Result<Vec<Candidato>> {
    let mut stmt = conn.prepare_cached(
        "SELECT g.id, g.dati, zg.comune_amm, zg.zona, zg.link_zona
FROM geometria_rt rt
JOIN geometria g ON g.id=rt.id
JOIN zona_geo zg ON zg.geometria_id=rt.id AND zg.semestre=@gs
WHERE rt.min_lon<=@x+@m AND rt.max_lon>=@x-@m AND rt.min_lat<=@y+@m AND rt.max_lat>=@y-@m",
    )?;
    let mut rows = stmt.query(named_params! {"@gs": gs, "@x": lon, "@y": lat, "@m": m})?;
    let mut out = Vec::new();
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let distanza = match cache.get(&id) {
            Some(&d) => d,
            None => {
                let ValueRef::Blob(blob) = r.get_ref(1)? else {
                    return Err(OmiError::DatiNonValidi(format!(
                        "geometria {id} senza dati"
                    )));
                };
                let g = Geometry::decode(blob)?;
                let d = if g.contains(lon, lat) {
                    0.0
                } else {
                    g.distance_meters(lon, lat)
                };
                cache.insert(id, d);
                d
            }
        };
        out.push(Candidato {
            comune_amm: r.get(2)?,
            zona: r.get(3)?,
            link_zona: r.get(4)?,
            distanza,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodifiche() {
        assert_eq!(fascia_descr(Some("B")), json!("Centrale"));
        assert_eq!(fascia_descr(Some("R")), json!("Extraurbana/rurale"));
        assert_eq!(fascia_descr(Some("X")), Value::Null);
        assert_eq!(stato_descr(Some("O")), json!("OTTIMO"));
        assert_eq!(stato_descr(Some("NORMALE")), json!("NORMALE"));
        assert_eq!(superficie(Some("N")), json!("netta"));
        assert_eq!(superficie(Some("X")), json!("X"));
        assert_eq!(superficie(None), Value::Null);
    }

    #[test]
    fn codice_catastale() {
        assert!(is_codice_catastale("H501"));
        assert!(!is_codice_catastale("H50"));
        assert!(!is_codice_catastale("H5011"));
        assert!(!is_codice_catastale("1501"));
        assert!(!is_codice_catastale("ROMA"));
    }
}
