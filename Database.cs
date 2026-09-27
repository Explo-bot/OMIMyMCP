// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

using System.Globalization;
using Microsoft.Data.Sqlite;

namespace OMIMyMCP;

public static class Database
{
    public static SqliteConnection Open(string path, bool readOnly = false)
    {
        var cs = new SqliteConnectionStringBuilder
        {
            DataSource = path,
            Mode = readOnly ? SqliteOpenMode.ReadOnly : SqliteOpenMode.ReadWriteCreate,
            Pooling = true,
        }.ToString();
        var c = new SqliteConnection(cs);
        c.Open();
        Exec(c, readOnly ? "PRAGMA query_only=1;" : "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=OFF;");
        return c;
    }

    public static void Exec(SqliteConnection c, string sql, SqliteTransaction? tx = null)
    {
        using var cmd = c.CreateCommand();
        cmd.Transaction = tx;
        cmd.CommandText = sql;
        cmd.ExecuteNonQuery();
    }

    /// <summary>
    /// Schema:
    ///  semestre      – un record per semestre caricato (codice AAAAS, es. 20251 = 2025/1)
    ///  zona          – file *_ZONE.csv: una riga per zona OMI per semestre (chiave LinkZona)
    ///  valore        – file *_VALORI.csv: quotazioni per zona/tipologia/stato conservativo
    ///  tipologia     – anagrafica tipologie edilizie (Cod_Tip)
    ///  geometria     – poligoni delle zone (KML) deduplicati tra i semestri, con bbox/centroide/area
    ///  geometria_rt  – indice R*Tree sui bbox delle geometrie (ricerca per coordinate)
    ///  zona_geo      – collegamento semestre + comune + zona -> geometria (dai file KML)
    ///  comune        – anagrafica comuni (dati dell'ultimo semestre disponibile)
    ///  comune_nome   – nomi (anche storici) normalizzati per la ricerca dei comuni
    ///  file_caricato – registro dei file importati
    /// </summary>
    public static void EnsureSchema(SqliteConnection c)
    {
        Exec(c, """
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
            dati      BLOB NOT NULL        -- vedi Geometry.Encode
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
        """);
    }

    public static string SemLabel(long s) => $"{s / 10}/{s % 10}";

    /// <summary>Riepilogo del contenuto del database, mostrato all'avvio.</summary>
    public static void PrintSummary(SqliteConnection c)
    {
        object? Scalar(string sql)
        {
            using var cmd = c.CreateCommand();
            cmd.CommandText = sql;
            var r = cmd.ExecuteScalar();
            return r is DBNull ? null : r;
        }

        var ultimo = Scalar("SELECT max(semestre) FROM semestre WHERE n_zone>0");
        if (ultimo == null)
        {
            Console.WriteLine("Il database non contiene ancora dati OMI.");
            return;
        }
        long ult = Convert.ToInt64(ultimo);
        var elab = Scalar($"SELECT coalesce(elaborazione_valori, elaborazione_zone) FROM semestre WHERE semestre={ult}");
        var aggiornato = Scalar("SELECT max(caricato) FROM file_caricato") as string;
        if (DateTime.TryParseExact(aggiornato, "yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture, DateTimeStyles.None, out var dt))
            aggiornato = dt.ToString("dd/MM/yyyy HH:mm", CultureInfo.InvariantCulture);
        var primo = Convert.ToInt64(Scalar("SELECT min(semestre) FROM semestre WHERE n_zone>0"));
        var nSem = Scalar("SELECT count(*) FROM semestre WHERE n_zone>0");
        var nSemGeo = Scalar("SELECT count(*) FROM semestre WHERE n_geometrie>0");
        var comuniUltimo = Scalar($"SELECT count(DISTINCT comune_amm) FROM zona WHERE semestre={ult}");
        var comuniTotali = Scalar("SELECT count(*) FROM comune");
        var zoneUltimo = Scalar($"SELECT n_zone FROM semestre WHERE semestre={ult}");

        Console.WriteLine("Contenuto del database:");
        Console.WriteLine($"  Ultimo aggiornamento:     {aggiornato ?? "n.d."}");
        Console.WriteLine($"  Ultimo semestre caricato: {SemLabel(ult)}" + (elab != null ? $" (elaborazione Agenzia del {elab})" : ""));
        Console.WriteLine($"  Semestri disponibili:     {nSem} ({SemLabel(primo)} - {SemLabel(ult)}), {nSemGeo} con perimetri");
        Console.WriteLine($"  Comuni censiti:           {comuniUltimo} nell'ultimo semestre ({zoneUltimo} zone), {comuniTotali} in totale nella serie storica");
    }

    /// <summary>Accetta "2025/1", "2025-1", "20251", "2025 1", "2025S1".</summary>
    public static int? ParseSemestre(string? s)
    {
        if (string.IsNullOrWhiteSpace(s)) return null;
        var digits = new string(s.Where(char.IsDigit).ToArray());
        if (digits.Length == 5 && int.TryParse(digits, out var v) && v % 10 is 1 or 2) return v;
        throw new ArgumentException($"Semestre non valido: '{s}'. Usare il formato AAAA/S, es. 2025/1.");
    }
}
