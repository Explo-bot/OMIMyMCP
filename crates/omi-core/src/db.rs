// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Database SQLite `omi.db`: apertura, schema, semestri e riepilogo.

use std::io::Write;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use chrono::NaiveDateTime;
use rusqlite::{Connection, OpenFlags, params};

use crate::error::{OmiError, Result};

/// DDL dello schema, da eseguire così com'è a ogni apertura in scrittura.
pub const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS semestre (
    semestre            INTEGER PRIMARY KEY,   -- AAAAS
    anno                INTEGER NOT NULL,
    numero              INTEGER NOT NULL,      -- 1 o 2
    elaborazione_zone   TEXT,
    elaborazione_valori TEXT,
    file_csv            TEXT,
    caricato_csv        TEXT,
    file_kml            TEXT,
    caricato_kml        TEXT,
    n_zone              INTEGER,
    n_valori            INTEGER,
    n_geometrie         INTEGER
);

CREATE TABLE IF NOT EXISTS zona (
    semestre          INTEGER NOT NULL,
    link_zona         TEXT    NOT NULL,
    area_territoriale TEXT,
    regione           TEXT,
    prov              TEXT,
    comune_istat      TEXT,
    comune_cat        TEXT,
    sez               TEXT,
    comune_amm        TEXT    NOT NULL,   -- codice catastale (Belfiore) del comune
    comune_descr      TEXT,
    fascia            TEXT,               -- B centrale, C semicentrale, D periferica, E suburbana, R rurale
    zona              TEXT    NOT NULL,
    zona_descr        TEXT,
    cod_tip_prev      INTEGER,
    descr_tip_prev    TEXT,
    stato_prev        TEXT,
    microzona         INTEGER,
    PRIMARY KEY (semestre, link_zona)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS ix_zona_comune ON zona(comune_amm, semestre, zona);
CREATE INDEX IF NOT EXISTS ix_zona_link ON zona(link_zona, semestre);

CREATE TABLE IF NOT EXISTS valore (
    semestre     INTEGER NOT NULL,
    link_zona    TEXT    NOT NULL,
    cod_tip      INTEGER NOT NULL,
    stato        TEXT    NOT NULL,   -- OTTIMO / NORMALE / SCADENTE (può essere vuoto)
    stato_prev   TEXT,               -- 'P' = stato conservativo prevalente
    compr_min    REAL,               -- €/m² compravendita
    compr_max    REAL,
    sup_nl_compr TEXT,               -- L lorda / N netta
    loc_min      REAL,               -- €/m² x mese locazione
    loc_max      REAL,
    sup_nl_loc   TEXT,
    PRIMARY KEY (semestre, link_zona, cod_tip, stato)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS tipologia (
    cod_tip     INTEGER PRIMARY KEY,
    descrizione TEXT
);

CREATE TABLE IF NOT EXISTS geometria (
    id        INTEGER PRIMARY KEY,
    hash      BLOB NOT NULL UNIQUE,
    min_lon   REAL, min_lat REAL, max_lon REAL, max_lat REAL,
    centro_lon REAL, centro_lat REAL,
    area_km2  REAL,
    n_poligoni INTEGER,
    n_punti   INTEGER,
    dati      BLOB NOT NULL        -- codifica descritta nel manuale tecnico (§11.2)
);
CREATE VIRTUAL TABLE IF NOT EXISTS geometria_rt USING rtree(id, min_lon, max_lon, min_lat, max_lat);

CREATE TABLE IF NOT EXISTS zona_geo (
    id           INTEGER PRIMARY KEY,
    semestre     INTEGER NOT NULL,
    comune_amm   TEXT    NOT NULL,
    zona         TEXT    NOT NULL,
    link_zona    TEXT,
    nome         TEXT,
    colore       TEXT,             -- colore del poligono nel KML (AABBGGRR)
    geometria_id INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_zona_geo_zona ON zona_geo(comune_amm, zona, semestre);
CREATE INDEX IF NOT EXISTS ix_zona_geo_geom ON zona_geo(geometria_id, semestre);
CREATE INDEX IF NOT EXISTS ix_zona_geo_sem ON zona_geo(semestre);

CREATE TABLE IF NOT EXISTS comune (
    comune_amm        TEXT PRIMARY KEY,
    comune_istat      TEXT,
    descrizione       TEXT,
    prov              TEXT,
    regione           TEXT,
    area_territoriale TEXT,
    primo_semestre    INTEGER,
    ultimo_semestre   INTEGER
);
CREATE INDEX IF NOT EXISTS ix_comune_istat ON comune(comune_istat);

CREATE TABLE IF NOT EXISTS comune_nome (
    comune_amm TEXT NOT NULL,
    nome       TEXT NOT NULL,
    nome_norm  TEXT NOT NULL,
    prov       TEXT,
    PRIMARY KEY (comune_amm, nome, prov)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS ix_comune_nome_norm ON comune_nome(nome_norm);

CREATE TABLE IF NOT EXISTS file_caricato (
    file        TEXT PRIMARY KEY,
    dimensione  INTEGER,
    semestre    INTEGER,
    contenuto   TEXT,
    caricato    TEXT
);
"#;

/// Apre il database.
///
/// - `read_only = false` (importatore): read-write-create, `journal_mode=WAL`, `foreign_keys=OFF`, schema.
/// - `read_only = true` (server): sola lettura e `query_only=1`; il file non viene mai creato.
///
/// In entrambi i casi `busy_timeout` di 5 secondi.
pub fn open(path: &Path, read_only: bool) -> Result<Connection> {
    let base = OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    if read_only {
        let conn = Connection::open_with_flags(path, base | OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "query_only", 1)?;
        Ok(conn)
    } else {
        let conn = Connection::open_with_flags(
            path,
            base | OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )?;
        conn.busy_timeout(Duration::from_secs(5))?;
        // journal_mode restituisce una riga con la modalità effettiva.
        conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        ensure_schema(&conn)?;
        Ok(conn)
    }
}

/// Pool di connessioni in sola lettura per il server: al massimo `size` connessioni, aperte quando
/// servono e riusate. La prima si apre subito, così un database non apribile emerge all'avvio.
pub struct Pool {
    path: PathBuf,
    size: usize,
    state: Mutex<PoolState>,
    available: Condvar,
}

struct PoolState {
    idle: Vec<Connection>,
    opened: usize,
}

impl Pool {
    pub fn new(path: &Path, size: usize) -> Result<Pool> {
        let conn = open(path, true)?;
        Ok(Pool {
            path: path.to_path_buf(),
            size: size.max(1),
            state: Mutex::new(PoolState {
                idle: vec![conn],
                opened: 1,
            }),
            available: Condvar::new(),
        })
    }

    /// Dimensione consigliata: numero di CPU, minimo 4.
    pub fn default_size() -> usize {
        std::thread::available_parallelism().map_or(4, |n| n.get().max(4))
    }

    /// Prende una connessione libera, ne apre una nuova se il pool non è pieno, altrimenti attende.
    /// Da chiamare fuori dal runtime asincrono (es. in `spawn_blocking`).
    pub fn get(&self) -> Result<PooledConnection<'_>> {
        let mut state = self.lock();
        loop {
            if let Some(conn) = state.idle.pop() {
                return Ok(PooledConnection {
                    pool: self,
                    conn: Some(conn),
                });
            }
            if state.opened < self.size {
                state.opened += 1;
                drop(state);
                return match open(&self.path, true) {
                    Ok(conn) => Ok(PooledConnection {
                        pool: self,
                        conn: Some(conn),
                    }),
                    Err(e) => {
                        self.lock().opened -= 1;
                        self.available.notify_one();
                        Err(e)
                    }
                };
            }
            state = self
                .available
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Un panic con il lucchetto preso non rende inutilizzabile il pool: lo stato resta coerente.
    fn lock(&self) -> MutexGuard<'_, PoolState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Connessione presa dal pool; al drop torna fra quelle libere.
pub struct PooledConnection<'p> {
    pool: &'p Pool,
    conn: Option<Connection>,
}

impl Deref for PooledConnection<'_> {
    type Target = Connection;

    fn deref(&self) -> &Connection {
        // Invariante: `conn` è `Some` dalla creazione fino al drop, che è l'unico punto che lo svuota.
        self.conn
            .as_ref()
            .expect("connessione del pool già restituita")
    }
}

impl Drop for PooledConnection<'_> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            self.pool.lock().idle.push(conn);
            self.pool.available.notify_one();
        }
    }
}

/// Esegue il DDL (idempotente).
pub fn ensure_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(SCHEMA)?;
    Ok(())
}

/// Etichetta `AAAA/S` di un semestre `AAAAS`.
pub fn sem_label(s: i64) -> String {
    format!("{}/{}", s / 10, s % 10)
}

/// Indice progressivo del semestre, per le distanze tra semestri.
pub fn sem_index(s: i64) -> i64 {
    s / 10 * 2 + s % 10
}

/// Interpreta un semestre fornito dall'utente: assente o vuoto → `None` (si userà l'ultimo).
pub fn parse_semestre(input: Option<&str>) -> Result<Option<i64>> {
    let Some(raw) = input else {
        return Ok(None);
    };
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    if digits.len() == 5
        && (digits.ends_with('1') || digits.ends_with('2'))
        && let Ok(s) = digits.parse::<i64>()
    {
        return Ok(Some(s));
    }
    Err(OmiError::Validazione(format!(
        "Semestre non valido: '{raw}'. Usare il formato AAAA/S, es. 2025/1."
    )))
}

/// Stampa il riepilogo del contenuto del database.
pub fn print_summary(conn: &Connection, out: &mut impl Write) -> Result<()> {
    let ult: Option<i64> = conn.query_row(
        "SELECT max(semestre) FROM semestre WHERE n_zone>0",
        [],
        |r| r.get(0),
    )?;
    let Some(ult) = ult else {
        writeln!(out, "Il database non contiene ancora dati OMI.")?;
        return Ok(());
    };
    let elab: Option<String> = conn.query_row(
        "SELECT coalesce(elaborazione_valori, elaborazione_zone) FROM semestre WHERE semestre=?1",
        params![ult],
        |r| r.get(0),
    )?;
    let caricato: Option<String> =
        conn.query_row("SELECT max(caricato) FROM file_caricato", [], |r| r.get(0))?;
    let primo: Option<i64> = conn.query_row(
        "SELECT min(semestre) FROM semestre WHERE n_zone>0",
        [],
        |r| r.get(0),
    )?;
    let n: i64 = conn.query_row("SELECT count(*) FROM semestre WHERE n_zone>0", [], |r| {
        r.get(0)
    })?;
    let n_geo: i64 = conn.query_row(
        "SELECT count(*) FROM semestre WHERE n_geometrie>0",
        [],
        |r| r.get(0),
    )?;
    let comuni_ult: i64 = conn.query_row(
        "SELECT count(DISTINCT comune_amm) FROM zona WHERE semestre=?1",
        params![ult],
        |r| r.get(0),
    )?;
    let comuni_tot: i64 = conn.query_row("SELECT count(*) FROM comune", [], |r| r.get(0))?;
    let zone_ult: Option<i64> = conn.query_row(
        "SELECT n_zone FROM semestre WHERE semestre=?1",
        params![ult],
        |r| r.get(0),
    )?;

    let data = caricato
        .and_then(|c| NaiveDateTime::parse_from_str(&c, "%Y-%m-%d %H:%M:%S").ok())
        .map_or_else(
            || "n.d.".to_string(),
            |d| d.format("%d/%m/%Y %H:%M").to_string(),
        );
    let elab = elab
        .map(|e| format!(" (elaborazione Agenzia del {e})"))
        .unwrap_or_default();

    writeln!(out, "Contenuto del database:")?;
    writeln!(out, "  Ultimo aggiornamento:     {data}")?;
    writeln!(out, "  Ultimo semestre caricato: {}{elab}", sem_label(ult))?;
    writeln!(
        out,
        "  Semestri disponibili:     {n} ({} - {}), {n_geo} con perimetri",
        sem_label(primo.unwrap_or(ult)),
        sem_label(ult)
    )?;
    writeln!(
        out,
        "  Comuni censiti:           {comuni_ult} nell'ultimo semestre ({} zone), {comuni_tot} in totale nella serie storica",
        zone_ult.unwrap_or(0)
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(conn: &Connection) -> String {
        let mut buf = Vec::new();
        print_summary(conn, &mut buf).unwrap();
        String::from_utf8(buf).unwrap()
    }

    #[test]
    fn schema_idempotente_e_rtree() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        ensure_schema(&conn).unwrap();
        let rtree: bool = conn
            .query_row(
                "SELECT count(*) FROM pragma_compile_options WHERE compile_options='ENABLE_RTREE'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
            > 0;
        assert!(rtree, "SQLite incluso senza modulo R*Tree");
        conn.execute(
            "INSERT INTO geometria_rt(id, min_lon, max_lon, min_lat, max_lat) VALUES (?,?,?,?,?)",
            params![1, 12.4, 12.5, 41.8, 41.9],
        )
        .unwrap();
        let id: i64 = conn
            .query_row(
                "SELECT id FROM geometria_rt WHERE min_lon<=12.45 AND max_lon>=12.45 AND min_lat<=41.85 AND max_lat>=41.85",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(id, 1);
    }

    #[test]
    fn open_scrittura_e_sola_lettura() {
        let dir = std::env::temp_dir().join(format!("omi-core-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.db");
        let _ = std::fs::remove_file(&path);

        // In sola lettura un file inesistente non viene creato.
        assert!(open(&path, true).is_err());
        assert!(!path.exists());

        {
            let conn = open(&path, false).unwrap();
            let mode: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .unwrap();
            assert_eq!(mode.to_lowercase(), "wal");
            let fk: i64 = conn
                .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
                .unwrap();
            assert_eq!(fk, 0);
            let busy: i64 = conn
                .query_row("PRAGMA busy_timeout", [], |r| r.get(0))
                .unwrap();
            assert_eq!(busy, 5000);
            conn.execute(
                "INSERT INTO tipologia(cod_tip, descrizione) VALUES (20, 'Abitazioni civili')",
                [],
            )
            .unwrap();
        }
        {
            let conn = open(&path, true).unwrap();
            let d: String = conn
                .query_row(
                    "SELECT descrizione FROM tipologia WHERE cod_tip=20",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(d, "Abitazioni civili");
            let (busy, query_only): (i64, i64) = (
                conn.query_row("PRAGMA busy_timeout", [], |r| r.get(0))
                    .unwrap(),
                conn.query_row("PRAGMA query_only", [], |r| r.get(0))
                    .unwrap(),
            );
            assert_eq!((busy, query_only), (5000, 1));
            assert!(conn.execute("DELETE FROM tipologia", []).is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn pool_limita_e_riusa_le_connessioni() {
        let dir = std::env::temp_dir().join(format!("omi-core-pool-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pool.db");
        let _ = std::fs::remove_file(&path);
        assert!(Pool::new(&path, 2).is_err());
        drop(open(&path, false).unwrap());

        let pool = std::sync::Arc::new(Pool::new(&path, 2).unwrap());
        let a = pool.get().unwrap();
        let b = pool.get().unwrap();
        assert_eq!(pool.lock().opened, 2);
        // La terza richiesta attende che una connessione torni libera.
        let p = pool.clone();
        let t = std::thread::spawn(move || {
            let c = p.get().unwrap();
            c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).unwrap()
        });
        std::thread::sleep(Duration::from_millis(50));
        drop(a);
        assert_eq!(t.join().unwrap(), 1);
        drop(b);
        assert_eq!(pool.lock().opened, 2);
        assert_eq!(pool.lock().idle.len(), 2);
        // In sola lettura.
        assert!(
            pool.get()
                .unwrap()
                .execute("DELETE FROM tipologia", [])
                .is_err()
        );
        drop(pool);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn etichette_e_indici() {
        assert_eq!(sem_label(20252), "2025/2");
        assert_eq!(sem_index(20252), 2025 * 2 + 2);
        assert_eq!(sem_index(20261) - sem_index(20252), 1);
    }

    #[test]
    fn parse_semestre_casi() {
        assert_eq!(parse_semestre(Some("2025/1")).unwrap(), Some(20251));
        assert_eq!(parse_semestre(Some("2025-2")).unwrap(), Some(20252));
        assert_eq!(parse_semestre(Some("20252")).unwrap(), Some(20252));
        assert_eq!(parse_semestre(Some("2025 S1")).unwrap(), Some(20251));
        assert_eq!(parse_semestre(Some("2025 1")).unwrap(), Some(20251));
        assert_eq!(parse_semestre(Some("")).unwrap(), None);
        assert_eq!(parse_semestre(Some("  ")).unwrap(), None);
        assert_eq!(parse_semestre(None).unwrap(), None);
        for bad in ["2025/3", "2025", "202511", "abc"] {
            match parse_semestre(Some(bad)) {
                Err(OmiError::Validazione(m)) => assert_eq!(
                    m,
                    format!("Semestre non valido: '{bad}'. Usare il formato AAAA/S, es. 2025/1.")
                ),
                other => panic!("{bad}: atteso errore di validazione, ottenuto {other:?}"),
            }
        }
    }

    #[test]
    fn riepilogo_database_vuoto() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        assert_eq!(
            summary(&conn),
            "Il database non contiene ancora dati OMI.\n"
        );
    }

    #[test]
    fn riepilogo_con_dati() {
        let conn = Connection::open_in_memory().unwrap();
        ensure_schema(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO semestre(semestre, anno, numero, elaborazione_valori, n_zone, n_geometrie) VALUES
                 (20242, 2024, 2, NULL, 3, 0), (20252, 2025, 2, '12/03/2026', 2, 2);
             INSERT INTO zona(semestre, link_zona, comune_amm, zona) VALUES
                 (20252, 'RM1', 'H501', 'B1'), (20252, 'MI1', 'F205', 'B1');
             INSERT INTO comune(comune_amm) VALUES ('H501'), ('F205'), ('A001');
             INSERT INTO file_caricato(file, caricato) VALUES ('QIP1.zip', '2026-03-20 18:05:42');",
        )
        .unwrap();
        assert_eq!(
            summary(&conn),
            "Contenuto del database:\n\
             \x20 Ultimo aggiornamento:     20/03/2026 18:05\n\
             \x20 Ultimo semestre caricato: 2025/2 (elaborazione Agenzia del 12/03/2026)\n\
             \x20 Semestri disponibili:     2 (2024/2 - 2025/2), 1 con perimetri\n\
             \x20 Comuni censiti:           2 nell'ultimo semestre (2 zone), 3 in totale nella serie storica\n"
        );
    }
}
