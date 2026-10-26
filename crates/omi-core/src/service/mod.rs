// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Logica degli strumenti MCP, senza dipendenze da MCP: ogni metodo riceve i
//! parametri tipizzati e restituisce il testo JSON del risultato. Gli errori di validazione sono
//! `OmiError::Validazione` con il messaggio previsto; gli altri errori sono interni.

mod common;
pub mod params;

use std::collections::HashMap;
use std::f64::consts::PI;

use rusqlite::{Connection, OptionalExtension, named_params, params};
use serde_json::{Value, json};

use crate::about::{FONTE_DATI, attribuzione};
use crate::db::sem_label;
use crate::error::Result;
use crate::geometry::round_half_even;
use crate::json::{strip_nulls, to_string};
use common::{
    Candidato, Riga, candidati, comune_by_amm, comune_info, fascia_descr, geo_info, geo_semestre,
    int, label, query_rows, resolve_comune, resolve_semestre, txt, val, validazione, valori,
    zona_completa, zona_info, zone_by_codice,
};
pub use params::*;

const NOTA_SEMESTRI: &str = "I perimetri (KML) sono disponibili solo per alcuni semestri; per gli altri la ricerca per coordinate usa i perimetri del semestre più vicino.";
const NOTA_NESSUNA_ZONA: &str = "Nessuna zona per il comune nel semestre indicato.";
const MSG_NESSUNA_ZONA_VICINA: &str = "Nessuna zona OMI contiene il punto né si trova entro 2 km.";
const NOTA_APPROSSIMATO: &str =
    "Il punto non ricade in alcun perimetro OMI: restituita la zona più vicina.";
const NOTA_PERIMETRI_ALTRO_SEMESTRE: &str = "Perimetri non disponibili per il semestre richiesto: usati quelli del semestre più vicino, collegati per codice zona.";
const NOTA_STORICO_VUOTO: &str = "Nessun dato: verificare codice zona (zone_comune) o filtri.";
const NOTA_STORICO: &str = "I codici zona OMI possono essere stati ridefiniti nelle revisioni della zonizzazione: confrontare le descrizioni.";
const NOTA_STATISTICHE: &str = "Valori €/m² (locazione €/m² mese) calcolati sullo stato conservativo prevalente di ciascuna zona.";

/// Distanza massima (metri) della zona più vicina in `zona_da_coordinate`.
const DISTANZA_MAX_M: f64 = 2000.0;
/// Margine (gradi) della ricerca della zona più vicina.
const MARGINE_VICINA: f64 = 0.02;
/// Raggio predefinito e massimo (metri) di `zone_vicine`.
const RAGGIO_DEFAULT_M: f64 = 1000.0;
const RAGGIO_MAX_M: f64 = 20000.0;
/// Numero massimo di zone restituite da `zone_vicine`.
const MAX_ZONE_VICINE: usize = 100;
/// Comuni restituiti da `cerca_comuni`.
const MAX_COMUNI: usize = 50;

/// Risultato JSON con le proprietà `null` omesse.
fn risultato(v: Value) -> Result<String> {
    to_string(&strip_nulls(v))
}

/// Gli strumenti, su una connessione in sola lettura.
pub struct Service<'c> {
    conn: &'c Connection,
}

impl<'c> Service<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Service { conn }
    }

    pub fn elenco_semestri(&self) -> Result<String> {
        let rows = query_rows(
            self.conn,
            "SELECT * FROM semestre ORDER BY semestre DESC",
            [],
        )?;
        let semestri: Vec<Value> = rows
            .iter()
            .map(|r| {
                let elaborazione = match val(r, "elaborazione_valori") {
                    Value::Null => val(r, "elaborazione_zone"),
                    v => v,
                };
                json!({
                    "semestre": label(int(r, "semestre")),
                    "n_zone": val(r, "n_zone"),
                    "n_valori": val(r, "n_valori"),
                    "n_perimetri": val(r, "n_geometrie"),
                    "elaborazione": elaborazione,
                    "file_csv": val(r, "file_csv"),
                    "file_kml": val(r, "file_kml"),
                })
            })
            .collect();
        risultato(json!({
            "semestri": semestri,
            "nota": NOTA_SEMESTRI,
            "fonte_dati": FONTE_DATI,
            "software": attribuzione(),
        }))
    }

    /// Unica eccezione alla regola dei `null`: le righe mantengono `"descrizione": null`.
    pub fn elenco_tipologie(&self) -> Result<String> {
        let rows = query_rows(
            self.conn,
            "SELECT cod_tip AS codice, descrizione FROM tipologia ORDER BY cod_tip",
            [],
        )?;
        to_string(&Value::Array(rows.into_iter().map(Value::Object).collect()))
    }

    pub fn cerca_comuni(&self, p: &CercaComuni) -> Result<String> {
        let rows = common::find_comuni(self.conn, &p.testo, p.provincia.as_deref())?;
        let comuni: Vec<Value> = rows.iter().take(MAX_COMUNI).map(comune_info).collect();
        risultato(json!({"trovati": rows.len(), "comuni": comuni}))
    }

    pub fn zone_comune(&self, p: &ZoneComune) -> Result<String> {
        let c = resolve_comune(self.conn, &p.comune, p.provincia.as_deref())?;
        let sem = resolve_semestre(self.conn, p.semestre.as_deref())?;
        let amm = txt(&c, "comune_amm").unwrap_or_default();
        let zone = query_rows(
            self.conn,
            "SELECT * FROM zona WHERE comune_amm=? AND semestre=? ORDER BY fascia, zona",
            params![amm, sem],
        )?;
        let mut items = Vec::with_capacity(zone.len());
        for z in &zone {
            let mut item = serde_json::Map::new();
            item.insert("info".into(), zona_info(z));
            if p.includi_perimetri {
                item.insert(
                    "perimetro".into(),
                    geo_info(
                        self.conn,
                        txt(z, "comune_amm").unwrap_or_default(),
                        txt(z, "zona").unwrap_or_default(),
                        txt(z, "link_zona"),
                        sem,
                        false,
                    )?,
                );
            }
            if p.includi_valori {
                item.insert(
                    "valori".into(),
                    Value::Array(valori(
                        self.conn,
                        sem,
                        txt(z, "link_zona").unwrap_or_default(),
                    )?),
                );
            }
            items.push(Value::Object(item));
        }
        risultato(json!({
            "comune": comune_info(&c),
            "semestre": sem_label(sem),
            "n_zone": zone.len(),
            "zone": items,
            "nota": zone.is_empty().then_some(NOTA_NESSUNA_ZONA),
        }))
    }

    pub fn dettaglio_zona(&self, p: &DettaglioZona) -> Result<String> {
        let sem = resolve_semestre(self.conn, p.semestre.as_deref())?;
        let z_code = p.zona.trim().to_uppercase();
        let mut zone = query_rows(
            self.conn,
            "SELECT * FROM zona WHERE link_zona=? AND semestre=?",
            params![z_code, sem],
        )?;
        let mut comune: Option<Riga> = None;
        if zone.is_empty() {
            let c = resolve_comune(self.conn, &p.comune, p.provincia.as_deref())?;
            zone = zone_by_codice(
                self.conn,
                txt(&c, "comune_amm").unwrap_or_default(),
                &z_code,
                sem,
            )?;
            comune = Some(c);
        }
        let Some(prima) = zone.first() else {
            return Err(validazione(format!(
                "Zona '{}' non trovata per il comune indicato nel semestre {}. Usare zone_comune per l'elenco.",
                p.zona,
                sem_label(sem)
            )));
        };
        if comune.is_none() {
            comune = comune_by_amm(self.conn, txt(prima, "comune_amm").unwrap_or_default())?;
        }
        let dati = zone
            .iter()
            .map(|z| zona_completa(self.conn, z, sem, p.includi_geometria))
            .collect::<Result<Vec<_>>>()?;
        risultato(json!({
            "comune": comune.as_ref().map(comune_info),
            "semestre": sem_label(sem),
            "zone": dati,
        }))
    }

    pub fn zona_da_coordinate(&self, p: &ZonaDaCoordinate) -> Result<String> {
        let (lat, lon) = (p.latitudine, p.longitudine);
        let sem = resolve_semestre(self.conn, p.semestre.as_deref())?;
        let gs = geo_semestre(self.conn, sem)?;
        let mut cache = HashMap::new();

        let mut interni: Vec<Candidato> = candidati(self.conn, gs, lon, lat, 0.0, &mut cache)?
            .into_iter()
            .filter(|c| c.distanza == 0.0)
            .collect();
        let mut approssimato = false;
        if interni.is_empty() {
            let mut vicini: Vec<Candidato> =
                candidati(self.conn, gs, lon, lat, MARGINE_VICINA, &mut cache)?
                    .into_iter()
                    .filter(|c| c.distanza <= DISTANZA_MAX_M)
                    .collect();
            vicini.sort_by(|a, b| a.distanza.total_cmp(&b.distanza));
            let Some(primo) = vicini.into_iter().next() else {
                return risultato(json!({
                    "trovato": false,
                    "lat": lat,
                    "lon": lon,
                    "semestre": sem_label(sem),
                    "messaggio": MSG_NESSUNA_ZONA_VICINA,
                }));
            };
            interni = vec![primo];
            approssimato = true;
        }

        let risultati = interni
            .iter()
            .map(|h| self.dati_zona_per_hit(h, sem, p.includi_geometria))
            .collect::<Result<Vec<_>>>()?;
        let nota = if approssimato {
            Some(NOTA_APPROSSIMATO)
        } else if gs != sem {
            Some(NOTA_PERIMETRI_ALTRO_SEMESTRE)
        } else {
            None
        };
        risultato(json!({
            "trovato": true,
            "lat": lat,
            "lon": lon,
            "semestre": sem_label(sem),
            "semestre_perimetri": sem_label(gs),
            "approssimato": approssimato.then_some(true),
            "nota": nota,
            "risultati": risultati,
        }))
    }

    fn dati_zona_per_hit(&self, h: &Candidato, sem: i64, includi_geometria: bool) -> Result<Value> {
        let mut zone = zone_by_codice(self.conn, &h.comune_amm, &h.zona, sem)?;
        if zone.len() > 1
            && let Some(link) = &h.link_zona
        {
            let stesso_link: Vec<Riga> = zone
                .iter()
                .filter(|z| txt(z, "link_zona") == Some(link.as_str()))
                .cloned()
                .collect();
            if !stesso_link.is_empty() {
                zone = stesso_link;
            }
        }
        let comune = match comune_by_amm(self.conn, &h.comune_amm)? {
            Some(c) => comune_info(&c),
            None => json!({"codice_catastale": h.comune_amm}),
        };
        let dati = zone
            .iter()
            .map(|z| zona_completa(self.conn, z, sem, includi_geometria))
            .collect::<Result<Vec<_>>>()?;
        let nota = zone.is_empty().then(|| {
            format!(
                "La zona {} individuata dal perimetro non esiste nei dati del semestre {} (codici zona cambiati tra le revisioni).",
                h.zona,
                sem_label(sem)
            )
        });
        Ok(json!({
            "comune": comune,
            "zona": h.zona,
            "distanza_m": (h.distanza > 0.0).then(|| round_half_even(h.distanza, 0)),
            "dati": dati,
            "nota": nota,
        }))
    }

    pub fn zone_vicine(&self, p: &ZoneVicine) -> Result<String> {
        let (lat, lon) = (p.latitudine, p.longitudine);
        let sem = resolve_semestre(self.conn, p.semestre.as_deref())?;
        let gs = geo_semestre(self.conn, sem)?;
        let r = p
            .raggio_m
            .unwrap_or(RAGGIO_DEFAULT_M)
            .clamp(0.0, RAGGIO_MAX_M);
        let margine = r / 111000.0 / f64::max(0.2, (lat * PI / 180.0).cos());

        // Per ogni coppia (comune, zona) resta il candidato più vicino, al posto del primo trovato.
        let mut scelti: Vec<Candidato> = Vec::new();
        let mut indice: HashMap<(String, String), usize> = HashMap::new();
        let mut cache = HashMap::new();
        for c in candidati(self.conn, gs, lon, lat, margine, &mut cache)? {
            if c.distanza > r {
                continue;
            }
            let key = (c.comune_amm.clone(), c.zona.clone());
            match indice.get(&key) {
                Some(&i) if c.distanza < scelti[i].distanza => scelti[i] = c,
                Some(_) => {}
                None => {
                    indice.insert(key, scelti.len());
                    scelti.push(c);
                }
            }
        }
        scelti.sort_by(|a, b| a.distanza.total_cmp(&b.distanza));
        scelti.truncate(MAX_ZONE_VICINE);

        let mut zone = Vec::with_capacity(scelti.len());
        for c in &scelti {
            let z = zone_by_codice(self.conn, &c.comune_amm, &c.zona, sem)?
                .into_iter()
                .next();
            let nome: Option<(Option<String>, Option<String>)> = self
                .conn
                .query_row(
                    "SELECT descrizione, prov FROM comune WHERE comune_amm=?",
                    [&c.comune_amm],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let comune = match nome {
                Some((descr, prov)) => format!(
                    "{} ({})",
                    descr.unwrap_or_default(),
                    prov.unwrap_or_default()
                ),
                None => c.comune_amm.clone(),
            };
            let abitazioni = match &z {
                Some(z) => query_rows(
                    self.conn,
                    "SELECT compr_min, compr_max, loc_min, loc_max, stato FROM valore WHERE semestre=? AND link_zona=? AND cod_tip=20
ORDER BY (stato_prev='P') DESC LIMIT 1",
                    params![sem, txt(z, "link_zona")],
                )?
                .into_iter()
                .next()
                .map(|v| {
                    json!({
                        "stato": val(&v, "stato"),
                        "compr_min": val(&v, "compr_min"),
                        "compr_max": val(&v, "compr_max"),
                        "loc_min": val(&v, "loc_min"),
                        "loc_max": val(&v, "loc_max"),
                    })
                }),
                None => None,
            };
            let link = match &z {
                Some(z) => val(z, "link_zona"),
                None => json!(c.link_zona),
            };
            zone.push(json!({
                "comune": comune,
                "codice_catastale": c.comune_amm,
                "zona": c.zona,
                "link_zona": link,
                "fascia": z.as_ref().map(|z| val(z, "fascia")),
                "descrizione": z.as_ref().map(|z| val(z, "zona_descr")),
                "distanza_m": round_half_even(c.distanza, 0),
                "abitazioni_civili": abitazioni,
            }));
        }
        risultato(json!({
            "lat": lat,
            "lon": lon,
            "raggio_m": r,
            "semestre": sem_label(sem),
            "semestre_perimetri": sem_label(gs),
            "n": zone.len(),
            "zone": zone,
        }))
    }

    pub fn storico_zona(&self, p: &StoricoZona) -> Result<String> {
        let c = resolve_comune(self.conn, &p.comune, p.provincia.as_deref())?;
        let z_code = p.zona.trim().to_uppercase();
        let st = p.stato.as_deref().map(|s| s.trim().to_uppercase());
        let rows = query_rows(
            self.conn,
            "SELECT z.semestre, z.link_zona, z.zona_descr, z.fascia, v.cod_tip, t.descrizione AS tip_descr, v.stato, v.stato_prev,
       v.compr_min, v.compr_max, v.loc_min, v.loc_max
FROM zona z JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona
LEFT JOIN tipologia t ON t.cod_tip=v.cod_tip
WHERE z.comune_amm=@c AND (z.zona=@z OR z.link_zona=@z)
  AND (@t IS NULL OR v.cod_tip=@t) AND (@st IS NULL OR v.stato=@st)
ORDER BY v.cod_tip, v.stato, z.semestre",
            named_params! {
                "@c": txt(&c, "comune_amm"),
                "@z": z_code,
                "@t": p.cod_tipologia,
                "@st": st,
            },
        )?;

        // Serie per (cod_tip, tip_descr, stato) e descrizioni della zona, nell'ordine di comparsa.
        let mut serie: Vec<(Value, Value, String, Vec<Value>)> = Vec::new();
        let mut descrizioni: Vec<(Value, i64, i64)> = Vec::new();
        for r in &rows {
            let (cod, tip) = (val(r, "cod_tip"), val(r, "tip_descr"));
            let stato = txt(r, "stato").unwrap_or_default().to_string();
            let semestre = int(r, "semestre").unwrap_or_default();
            let valore = json!({
                "semestre": sem_label(semestre),
                "compr_min": val(r, "compr_min"),
                "compr_max": val(r, "compr_max"),
                "loc_min": val(r, "loc_min"),
                "loc_max": val(r, "loc_max"),
                "prevalente": (txt(r, "stato_prev") == Some("P")).then_some(true),
            });
            match serie
                .iter_mut()
                .find(|(c, t, s, _)| *c == cod && *t == tip && *s == stato)
            {
                Some((_, _, _, valori)) => valori.push(valore),
                None => serie.push((cod, tip, stato, vec![valore])),
            }
            let descr = val(r, "zona_descr");
            match descrizioni.iter_mut().find(|(d, _, _)| *d == descr) {
                Some((_, dal, al)) => {
                    *dal = (*dal).min(semestre);
                    *al = (*al).max(semestre);
                }
                None => descrizioni.push((descr, semestre, semestre)),
            }
        }
        let n_serie = serie.len();
        let serie: Vec<Value> = serie
            .into_iter()
            .map(|(cod, tip, stato, valori)| {
                json!({
                    "cod_tipologia": cod,
                    "tipologia": tip,
                    "stato": (!stato.is_empty()).then_some(stato),
                    "valori": valori,
                })
            })
            .collect();
        let descrizioni: Vec<Value> = descrizioni
            .into_iter()
            .map(|(d, dal, al)| json!({"descrizione": d, "dal": sem_label(dal), "al": sem_label(al)}))
            .collect();
        risultato(json!({
            "comune": comune_info(&c),
            "zona": z_code,
            "descrizioni_zona": descrizioni,
            "n_serie": n_serie,
            "serie": serie,
            "nota": if n_serie == 0 { NOTA_STORICO_VUOTO } else { NOTA_STORICO },
        }))
    }

    pub fn statistiche_comune(&self, p: &StatisticheComune) -> Result<String> {
        let c = resolve_comune(self.conn, &p.comune, p.provincia.as_deref())?;
        let sem = resolve_semestre(self.conn, p.semestre.as_deref())?;
        let amm = txt(&c, "comune_amm");
        let per_tipologia: Vec<Value> = query_rows(
            self.conn,
            "SELECT v.cod_tip, t.descrizione AS tip_descr, count(DISTINCT v.link_zona) AS n_zone,
       min(v.compr_min) AS compr_min, max(v.compr_max) AS compr_max, round(avg((v.compr_min+v.compr_max)/2.0),1) AS compr_medio,
       min(v.loc_min) AS loc_min, max(v.loc_max) AS loc_max, round(avg((v.loc_min+v.loc_max)/2.0),2) AS loc_medio
FROM zona z JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona
LEFT JOIN tipologia t ON t.cod_tip=v.cod_tip
WHERE z.comune_amm=@c AND z.semestre=@s AND (v.stato_prev='P' OR NOT EXISTS (
    SELECT 1 FROM valore v2 WHERE v2.semestre=v.semestre AND v2.link_zona=v.link_zona AND v2.cod_tip=v.cod_tip AND v2.stato_prev='P'))
GROUP BY v.cod_tip ORDER BY v.cod_tip",
            named_params! {"@c": amm, "@s": sem},
        )?
        .iter()
        .map(|r| {
            json!({
                "cod_tipologia": val(r, "cod_tip"),
                "tipologia": val(r, "tip_descr"),
                "n_zone": val(r, "n_zone"),
                "compravendita": {
                    "min": val(r, "compr_min"),
                    "max": val(r, "compr_max"),
                    "medio": val(r, "compr_medio"),
                },
                "locazione": {
                    "min": val(r, "loc_min"),
                    "max": val(r, "loc_max"),
                    "medio": val(r, "loc_medio"),
                },
            })
        })
        .collect();
        let per_fascia: Vec<Value> = query_rows(
            self.conn,
            "SELECT z.fascia, count(*) AS n_zone,
       round(avg((v.compr_min+v.compr_max)/2.0),1) AS abitazioni_civili_compr_medio,
       round(avg((v.loc_min+v.loc_max)/2.0),2) AS abitazioni_civili_loc_medio
FROM zona z LEFT JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona AND v.cod_tip=20 AND v.stato_prev='P'
WHERE z.comune_amm=@c AND z.semestre=@s GROUP BY z.fascia ORDER BY z.fascia",
            named_params! {"@c": amm, "@s": sem},
        )?
        .iter()
        .map(|r| {
            json!({
                "fascia": val(r, "fascia"),
                "fascia_descr": fascia_descr(txt(r, "fascia")),
                "n_zone": val(r, "n_zone"),
                "abitazioni_civili_compr_medio": val(r, "abitazioni_civili_compr_medio"),
                "abitazioni_civili_loc_medio": val(r, "abitazioni_civili_loc_medio"),
            })
        })
        .collect();
        risultato(json!({
            "comune": comune_info(&c),
            "semestre": sem_label(sem),
            "nota": NOTA_STATISTICHE,
            "per_tipologia": per_tipologia,
            "per_fascia": per_fascia,
        }))
    }
}
