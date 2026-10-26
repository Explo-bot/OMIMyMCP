// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Importazione delle forniture OMI (`QI*.zip`, `QIP*.zip`) nel database.

mod csv;
mod kml;

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read, Seek, Write};
use std::path::Path;
use std::sync::LazyLock;
use std::time::Instant;

use regex::Regex;
use rusqlite::{Connection, ToSql, named_params, params};
use zip::ZipArchive;

use crate::db::sem_label;
use crate::error::{OmiError, Result};
use crate::geometry::{self, round_half_even};
use crate::text::{norm, safe_file_name};

use self::csv::Csv;
use self::kml::Kml;

// Invariante: i pattern sono costanti e validi (verificati dai test d'integrazione dell'import).
static RE_SEMESTRE_CSV: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d{4})(\d)_(ZONE|VALORI)\.csv$")
        .expect("espressione regolare costante valida")
});
static RE_ZONA_OMI: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)Zona OMI\s+(\S+)\s*$").expect("espressione regolare costante valida")
});

const SQL_DELETE_VALORE: &str = "DELETE FROM valore WHERE semestre=?1 AND (link_zona IN (SELECT v FROM tmp_links)
    OR link_zona IN (SELECT link_zona FROM zona WHERE semestre=?1 AND comune_amm IN (SELECT v FROM tmp_comuni)))";
const SQL_DELETE_ZONA: &str =
    "DELETE FROM zona WHERE semestre=?1 AND comune_amm IN (SELECT v FROM tmp_comuni)";
const SQL_TIPOLOGIA: &str = "INSERT INTO tipologia(cod_tip, descrizione) VALUES (?,?) ON CONFLICT(cod_tip) DO UPDATE SET descrizione=excluded.descrizione";
const SET_SEMESTRE_CSV: &str = "elaborazione_zone=coalesce(:ez,elaborazione_zone), elaborazione_valori=coalesce(:ev,elaborazione_valori), file_csv=:f, caricato_csv=:t, n_zone=(SELECT count(*) FROM zona WHERE semestre=:s), n_valori=(SELECT count(*) FROM valore WHERE semestre=:s)";
const SET_SEMESTRE_KML: &str =
    "file_kml=:f, caricato_kml=:t, n_geometrie=(SELECT count(*) FROM zona_geo WHERE semestre=:s)";
const SQL_SEMESTRE: &str = "INSERT INTO semestre(semestre, anno, numero) VALUES (?1, ?2, ?3) ON CONFLICT(semestre) DO NOTHING";
const SQL_GEOMETRIA: &str = "INSERT INTO geometria(hash, min_lon, min_lat, max_lon, max_lat, centro_lon, centro_lat, area_km2, n_poligoni, n_punti, dati)
VALUES (?,?,?,?,?,?,?,?,?,?,?)";
const SQL_GEOMETRIA_RT: &str =
    "INSERT INTO geometria_rt(id, min_lon, max_lon, min_lat, max_lat) VALUES (?,?,?,?,?)";
const SQL_ZONA_GEO: &str = "INSERT INTO zona_geo(semestre, comune_amm, zona, link_zona, nome, colore, geometria_id) VALUES (?,?,?,?,?,?,?)";
const SQL_DELETE_ZONA_GEO: &str = "DELETE FROM zona_geo WHERE semestre=? AND comune_amm=?";
const SQL_FILE_CARICATO: &str = "INSERT INTO file_caricato(file, dimensione, semestre, contenuto, caricato) VALUES (:f,:d,:s,:c,:t)
ON CONFLICT(file) DO UPDATE SET dimensione=:d, semestre=:s, contenuto=:c, caricato=:t";

const SQL_COMPLETA: &str = "
DELETE FROM geometria_rt WHERE id NOT IN (SELECT geometria_id FROM zona_geo);
DELETE FROM geometria WHERE id NOT IN (SELECT geometria_id FROM zona_geo);

DELETE FROM comune;
INSERT INTO comune(comune_amm, comune_istat, descrizione, prov, regione, area_territoriale, primo_semestre, ultimo_semestre)
SELECT z.comune_amm, z.comune_istat, z.comune_descr, z.prov, z.regione, z.area_territoriale,
       (SELECT min(z2.semestre) FROM zona z2 WHERE z2.comune_amm = z.comune_amm), max(z.semestre)
FROM zona z GROUP BY z.comune_amm;
DELETE FROM comune_nome;
";
const SQL_NOMI: &str =
    "SELECT DISTINCT comune_amm, comune_descr, prov FROM zona WHERE comune_descr IS NOT NULL";
const SQL_COMUNE_NOME: &str =
    "INSERT OR IGNORE INTO comune_nome(comune_amm, nome, nome_norm, prov) VALUES (?,?,?,?)";

/// Esito di `import_zip`.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ZipReport {
    pub semestre: Option<i64>,
    /// Righe importate da `*_ZONE.csv` e `*_VALORI.csv`.
    pub n_zone: usize,
    pub n_valori: usize,
    /// Voci KML e righe inserite in `zona_geo`.
    pub n_kml: usize,
    pub n_geo: usize,
    /// Nessun CSV o KML OMI nello ZIP: non è stato scritto nulla.
    pub ignorato: bool,
}

/// Importatore: mantiene tra un file e l'altro la cache `hash → id` delle geometrie.
pub struct Importer {
    cache: HashMap<[u8; 16], i64>,
}

/// Voci rilevanti di uno ZIP: indice e nome senza cartella.
#[derive(Default)]
struct Voci {
    zone: Option<(usize, String)>,
    valori: Option<(usize, String)>,
    kml: Vec<(usize, String)>,
}

impl Importer {
    /// Carica la cache delle geometrie già presenti.
    pub fn new(conn: &Connection) -> Result<Self> {
        // Tutti gli statement dell'import restano preparati per l'intera esecuzione.
        conn.set_prepared_statement_cache_capacity(64);
        Ok(Importer {
            cache: load_cache(conn)?,
        })
    }

    /// Importa un file ZIP in un'unica transazione; in caso di errore il database resta com'era.
    pub fn import_zip(
        &mut self,
        conn: &mut Connection,
        path: &Path,
        out: &mut impl Write,
    ) -> Result<ZipReport> {
        let inizio = Instant::now();
        let nome = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let nome_registro = safe_file_name(&nome);
        let dimensione = std::fs::metadata(path)?.len();
        writeln!(
            out,
            "Import di {nome_registro} ({:.1} MB)...",
            round_half_even(dimensione as f64 / (1024.0 * 1024.0), 1)
        )?;

        let mut zip = ZipArchive::new(BufReader::new(File::open(path)?))?;
        let voci = trova_voci(&zip)?;
        let ha_csv = voci.zone.is_some() || voci.valori.is_some();
        if !ha_csv && voci.kml.is_empty() {
            writeln!(
                out,
                "  Nessun CSV o KML OMI riconosciuto nel file: ignorato."
            )?;
            return Ok(ZipReport {
                ignorato: true,
                ..ZipReport::default()
            });
        }

        let ora = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let tx = conn.transaction()?;
        let mut report = ZipReport::default();
        let mut nuove = HashMap::new();

        if ha_csv {
            let sem = import_csv(&tx, &mut zip, &voci, &nome_registro, &ora, &mut report, out)?;
            report.semestre = Some(sem);
        }
        if !voci.kml.is_empty() {
            let mut kml = KmlImport {
                conn: &tx,
                cache: &self.cache,
                nuove: &mut nuove,
                sostituiti: HashSet::new(),
            };
            let (sem, n_geo) = kml.run(&mut zip, &voci.kml, report.semestre, out)?;
            aggiorna_semestre(
                &tx,
                sem,
                SET_SEMESTRE_KML,
                named_params! {":f": nome_registro, ":t": ora, ":s": sem},
            )?;
            writeln!(
                out,
                "  Semestre {}: {n_geo} perimetri di zona da {} file KML",
                sem_label(sem),
                voci.kml.len()
            )?;
            report.semestre = Some(sem);
            report.n_kml = voci.kml.len();
            report.n_geo = n_geo;
        }

        let contenuto = format!(
            "zone={}; valori={}; kml={}; perimetri={}",
            report.n_zone, report.n_valori, report.n_kml, report.n_geo
        );
        tx.prepare_cached(SQL_FILE_CARICATO)?
            .execute(named_params! {
                ":f": nome_registro,
                ":d": i64::try_from(dimensione).unwrap_or(i64::MAX),
                ":s": report.semestre,
                ":c": contenuto,
                ":t": ora,
            })?;
        tx.commit()?;
        // Solo ora le geometrie nuove esistono davvero nel database.
        self.cache.extend(nuove);
        writeln!(
            out,
            "  completato in {:.1} s",
            round_half_even(inizio.elapsed().as_secs_f64(), 1)
        )?;
        Ok(report)
    }

    /// Completamento da eseguire una volta dopo tutti i file (§9): geometrie orfane, `comune`,
    /// `comune_nome`, statistiche e checkpoint del WAL.
    pub fn completa(&mut self, conn: &mut Connection, out: &mut impl Write) -> Result<()> {
        writeln!(out, "Aggiornamento anagrafiche e indici...")?;
        let tx = conn.transaction()?;
        tx.execute_batch(SQL_COMPLETA)?;
        {
            let mut sel = tx.prepare(SQL_NOMI)?;
            let mut ins = tx.prepare(SQL_COMUNE_NOME)?;
            let mut rows = sel.query([])?;
            while let Some(r) = rows.next()? {
                let comune: String = r.get(0)?;
                let nome: String = r.get(1)?;
                let prov: Option<String> = r.get(2)?;
                ins.execute(params![comune, nome, norm(&nome), prov.unwrap_or_default()])?;
            }
        }
        tx.commit()?;
        conn.execute_batch("ANALYZE;")?;
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        // Le geometrie orfane sono state eliminate: la cache non deve più farvi riferimento.
        self.cache = load_cache(conn)?;
        Ok(())
    }
}

fn load_cache(conn: &Connection) -> Result<HashMap<[u8; 16], i64>> {
    let mut stmt = conn.prepare("SELECT id, hash FROM geometria")?;
    let mut rows = stmt.query([])?;
    let mut cache = HashMap::new();
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let hash: Vec<u8> = r.get(1)?;
        if let Ok(h) = <[u8; 16]>::try_from(hash.as_slice()) {
            cache.insert(h, id);
        }
    }
    Ok(cache)
}

/// Prima voce `*_ZONE.csv`, prima `*_VALORI.csv` e tutte le `*.kml`, per nome senza cartella.
fn trova_voci<R: Read + Seek>(zip: &ZipArchive<R>) -> Result<Voci> {
    let mut voci = Voci::default();
    for i in 0..zip.len() {
        let Some(name) = zip.name_for_index(i) else {
            continue;
        };
        let name = name?;
        let base = name.rsplit(['/', '\\']).next().unwrap_or_default();
        let lower = base.to_ascii_lowercase();
        if lower.ends_with("_zone.csv") {
            voci.zone.get_or_insert_with(|| (i, base.to_string()));
        } else if lower.ends_with("_valori.csv") {
            voci.valori.get_or_insert_with(|| (i, base.to_string()));
        } else if lower.ends_with(".kml") {
            voci.kml.push((i, base.to_string()));
        }
    }
    Ok(voci)
}

fn read_entry<R: Read + Seek>(zip: &mut ZipArchive<R>, index: usize) -> Result<Vec<u8>> {
    let mut f = zip.by_index(index)?;
    // La dimensione dichiarata serve solo come stima: limitata per non allocare troppo su uno ZIP corrotto.
    let cap = usize::try_from(f.size()).unwrap_or(0).min(256 << 20);
    let mut buf = Vec::with_capacity(cap);
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

fn semestre_da_nome(name: &str) -> Option<i64> {
    let c = RE_SEMESTRE_CSV.captures(name)?;
    let anno: i64 = c[1].parse().ok()?;
    let s: i64 = c[2].parse().ok()?;
    Some(anno * 10 + s)
}

/// `INSERT … ON CONFLICT DO NOTHING` + `UPDATE semestre SET {assegnazioni}` (§7). I parametri
/// devono comprendere `:s`.
fn aggiorna_semestre(
    conn: &Connection,
    sem: i64,
    assegnazioni: &str,
    params: &[(&str, &dyn ToSql)],
) -> Result<()> {
    conn.prepare_cached(SQL_SEMESTRE)?
        .execute(params![sem, sem / 10, sem % 10])?;
    conn.prepare_cached(&format!(
        "UPDATE semestre SET {assegnazioni} WHERE semestre=:s"
    ))?
    .execute(params)?;
    Ok(())
}

/// `CREATE TEMP TABLE IF NOT EXISTS {t}(v TEXT PRIMARY KEY); DELETE FROM {t};` e inserimento dei valori.
fn riempi_temp(conn: &Connection, table: &str, values: &[String]) -> Result<()> {
    conn.execute_batch(&format!(
        "CREATE TEMP TABLE IF NOT EXISTS {table}(v TEXT PRIMARY KEY); DELETE FROM {table};"
    ))?;
    let mut stmt = conn.prepare_cached(&format!("INSERT OR IGNORE INTO {table}(v) VALUES (?)"))?;
    for v in values {
        stmt.execute([v])?;
    }
    Ok(())
}

/// Parte CSV di `import_zip` (§4 punto 5); restituisce il semestre.
fn import_csv<R: Read + Seek>(
    conn: &Connection,
    zip: &mut ZipArchive<R>,
    voci: &Voci,
    nome_registro: &str,
    ora: &str,
    report: &mut ZipReport,
    out: &mut impl Write,
) -> Result<i64> {
    let zone_text = match &voci.zone {
        Some((i, _)) => Some(csv::decode(read_entry(zip, *i)?)),
        None => None,
    };
    let valori_text = match &voci.valori {
        Some((i, _)) => Some(csv::decode(read_entry(zip, *i)?)),
        None => None,
    };
    let zone = zone_text.as_deref().map(Csv::parse);
    let valori = valori_text.as_deref().map(Csv::parse);

    let sem_nome = voci
        .zone
        .as_ref()
        .and_then(|(_, n)| semestre_da_nome(n))
        .or_else(|| voci.valori.as_ref().and_then(|(_, n)| semestre_da_nome(n)));
    let sem_titolo = zone
        .as_ref()
        .or(valori.as_ref())
        .and_then(|c| csv::title_semestre(c.title));
    let sem = match (sem_nome, sem_titolo) {
        (Some(n), Some(t)) if n != t => {
            writeln!(
                out,
                "  Attenzione: semestre nel nome file ({n}) diverso da quello nel titolo ({t}); uso {t}"
            )?;
            t
        }
        (_, Some(t)) => t,
        (Some(n), None) => n,
        (None, None) => {
            return Err(OmiError::Import(
                "Impossibile determinare il semestre dei CSV".to_string(),
            ));
        }
    };

    let comuni = zone
        .as_ref()
        .map(|c| c.distinct("Comune_amm", true))
        .unwrap_or_default();
    let links = valori
        .as_ref()
        .map(|c| c.distinct("LinkZona", false))
        .unwrap_or_default();
    riempi_temp(conn, "tmp_comuni", &comuni)?;
    riempi_temp(conn, "tmp_links", &links)?;
    conn.prepare_cached(SQL_DELETE_VALORE)?.execute([sem])?;
    conn.prepare_cached(SQL_DELETE_ZONA)?.execute([sem])?;

    let mut tipologie = HashMap::new();
    let (n_zone, ez) = match &zone {
        Some(c) => csv::import_zone(conn, c, sem, &mut tipologie)?,
        None => (0, None),
    };
    let (n_valori, ev) = match &valori {
        Some(c) => csv::import_valori(conn, c, sem, &mut tipologie)?,
        None => (0, None),
    };
    let mut stmt = conn.prepare_cached(SQL_TIPOLOGIA)?;
    for (cod, descr) in &tipologie {
        stmt.execute(params![cod, descr])?;
    }

    aggiorna_semestre(
        conn,
        sem,
        SET_SEMESTRE_CSV,
        named_params! {":ez": ez, ":ev": ev, ":f": nome_registro, ":t": ora, ":s": sem},
    )?;
    writeln!(
        out,
        "  Semestre {}: {n_zone} zone, {n_valori} valori",
        sem_label(sem)
    )?;
    report.n_zone = n_zone;
    report.n_valori = n_valori;
    Ok(sem)
}

/// Parte KML di `import_zip` (§4 punto 6 e §6.4).
struct KmlImport<'a> {
    conn: &'a Connection,
    /// Cache delle geometrie già nel database (prima di questo file).
    cache: &'a HashMap<[u8; 16], i64>,
    /// Geometrie inserite da questo file: si uniscono alla cache solo dopo il commit.
    nuove: &'a mut HashMap<[u8; 16], i64>,
    sostituiti: HashSet<String>,
}

impl KmlImport<'_> {
    /// Restituisce il semestre e il numero di righe inserite in `zona_geo`.
    fn run<R: Read + Seek>(
        &mut self,
        zip: &mut ZipArchive<R>,
        voci: &[(usize, String)],
        sem_csv: Option<i64>,
        out: &mut impl Write,
    ) -> Result<(i64, usize)> {
        // Senza CSV il semestre viene dalle prime 5 voci KML, che restano lette per l'import.
        let mut lette = Vec::new();
        let sem = match sem_csv {
            Some(s) => s,
            None => {
                let mut trovato = None;
                for (i, _) in voci.iter().take(5) {
                    let doc = kml::read_kml(&kml::decode(read_entry(zip, *i)?))?;
                    trovato = doc.semestre();
                    lette.push(doc);
                    if trovato.is_some() {
                        break;
                    }
                }
                trovato.ok_or_else(|| {
                    OmiError::Import("Impossibile determinare il semestre dei KML".to_string())
                })?
            }
        };

        let mut lette = lette.into_iter();
        let mut n_geo = 0;
        for (k, (i, nome_voce)) in voci.iter().enumerate() {
            let doc = match lette.next() {
                Some(d) => d,
                None => kml::read_kml(&kml::decode(read_entry(zip, *i)?))?,
            };
            if let Some((msg, scartati)) = &doc.recupero {
                writeln!(
                    out,
                    "    Attenzione: {nome_voce} malformato ({msg}); letti {} placemark, scartati {scartati}",
                    doc.placemarks.len()
                )?;
            }
            let codice_file = nome_voce
                .get(..nome_voce.len().saturating_sub(4))
                .unwrap_or(nome_voce)
                .to_uppercase();
            n_geo += self.import_doc(&doc, sem, &codice_file)?;
            if (k + 1) % 1000 == 0 {
                writeln!(out, "    {}/{} file KML...", k + 1, voci.len())?;
            }
        }
        Ok((sem, n_geo))
    }

    fn sostituisci(&mut self, sem: i64, comune: &str) -> Result<()> {
        if self.sostituiti.insert(comune.to_string()) {
            self.conn
                .prepare_cached(SQL_DELETE_ZONA_GEO)?
                .execute(params![sem, comune])?;
        }
        Ok(())
    }

    fn import_doc(&mut self, doc: &Kml, sem: i64, codice_file: &str) -> Result<usize> {
        self.sostituisci(sem, codice_file)?;
        let mut n = 0;
        for p in &doc.placemarks {
            if p.geometria.polygons.is_empty() {
                continue;
            }
            let cod_com = p
                .cod_com
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .unwrap_or(codice_file)
                .trim()
                .to_uppercase();
            self.sostituisci(sem, &cod_com)?;
            let cod_zona = p
                .cod_zona
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    let nome = p.nome.as_deref()?;
                    Some(RE_ZONA_OMI.captures(nome)?[1].to_string())
                });
            let Some(cod_zona) = cod_zona else {
                continue;
            };

            let blob = p.geometria.encode();
            let hash = geometry::hash(&blob);
            let id = match self.cache.get(&hash).or_else(|| self.nuove.get(&hash)) {
                Some(&id) => id,
                None => self.insert_geometria(&p.geometria, hash, blob)?,
            };
            let colore = p
                .style_url
                .as_deref()
                .and_then(|u| doc.stili.get(u.trim_start_matches('#')));
            let link_zona = p
                .link_zona
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty());
            self.conn.prepare_cached(SQL_ZONA_GEO)?.execute(params![
                sem, cod_com, cod_zona, link_zona, p.nome, colore, id
            ])?;
            n += 1;
        }
        Ok(n)
    }

    fn insert_geometria(
        &mut self,
        g: &geometry::Geometry,
        hash: [u8; 16],
        blob: Vec<u8>,
    ) -> Result<i64> {
        let (min_lon, min_lat, max_lon, max_lat) = g.bbox();
        let (cx, cy, area) = g.centroid_and_area();
        let n_poligoni = i64::try_from(g.polygons.len()).unwrap_or(i64::MAX);
        let n_punti = i64::try_from(g.n_punti()).unwrap_or(i64::MAX);
        self.conn.prepare_cached(SQL_GEOMETRIA)?.execute(params![
            &hash[..],
            min_lon,
            min_lat,
            max_lon,
            max_lat,
            round_half_even(cx, 6),
            round_half_even(cy, 6),
            round_half_even(area, 4),
            n_poligoni,
            n_punti,
            blob,
        ])?;
        let id = self.conn.last_insert_rowid();
        self.conn
            .prepare_cached(SQL_GEOMETRIA_RT)?
            .execute(params![id, min_lon, max_lon, min_lat, max_lat])?;
        self.nuove.insert(hash, id);
        Ok(id)
    }
}
