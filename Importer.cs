// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

using System.Diagnostics;
using System.Globalization;
using System.IO.Compression;
using System.Security.Cryptography;
using System.Text;
using System.Text.RegularExpressions;
using System.Xml;
using Microsoft.Data.Sqlite;

namespace OMIMyMCP;

/// <summary>
/// Importa i file ZIP delle forniture OMI (QI*.zip / QIP*.zip):
///  - *_ZONE.csv   -> tabella zona
///  - *_VALORI.csv -> tabella valore
///  - *.kml        -> tabelle geometria / zona_geo (perimetri delle zone)
/// I dati di un semestre sostituiscono quelli già presenti per lo stesso semestre
/// (i CSV sostituiscono zone e valori, i KML sostituiscono i perimetri).
/// </summary>
public sealed partial class Importer(SqliteConnection db)
{
    readonly Dictionary<string, long> _geomByHash = new();
    readonly Dictionary<int, string> _tipologie = new();
    bool _geomLoaded;

    [GeneratedRegex(@"(\d{4})(\d)_(ZONE|VALORI)\.csv$", RegexOptions.IgnoreCase)]
    private static partial Regex CsvName();

    [GeneratedRegex(@"Semestre\s+(\d{4})\s*/\s*(\d)", RegexOptions.IgnoreCase)]
    private static partial Regex SemInTitle();

    [GeneratedRegex(@"elaborazione del\s+(.+)$", RegexOptions.IgnoreCase)]
    private static partial Regex ElabInTitle();

    [GeneratedRegex(@"(\d{4})\s*/\s*([12])")]
    private static partial Regex SemInKml();

    [GeneratedRegex(@"Zona OMI\s+(\S+)\s*$", RegexOptions.IgnoreCase)]
    private static partial Regex ZonaInName();

    public void ImportZip(string path)
    {
        var sw = Stopwatch.StartNew();
        var fi = new FileInfo(path);
        var fileName = SafeFileName(fi.Name);
        Console.WriteLine($"Import di {fi.Name} ({fi.Length / 1048576.0:F1} MB)...");
        using var zip = ZipFile.OpenRead(path);

        var zoneEntry = zip.Entries.FirstOrDefault(e => e.Name.EndsWith("_ZONE.csv", StringComparison.OrdinalIgnoreCase));
        var valoriEntry = zip.Entries.FirstOrDefault(e => e.Name.EndsWith("_VALORI.csv", StringComparison.OrdinalIgnoreCase));
        var kmlEntries = zip.Entries.Where(e => e.Name.EndsWith(".kml", StringComparison.OrdinalIgnoreCase)).ToList();

        int? sem = null;
        foreach (var e in new[] { zoneEntry, valoriEntry })
        {
            if (e == null) continue;
            var m = CsvName().Match(e.Name);
            if (m.Success) { sem = int.Parse(m.Groups[1].Value) * 10 + int.Parse(m.Groups[2].Value); break; }
        }

        using var tx = db.BeginTransaction();
        _tipologie.Clear();
        int nZone = 0, nValori = 0, nGeo = 0;
        string? elabZone = null, elabValori = null;

        if (zoneEntry != null || valoriEntry != null)
        {
            // Il semestre può venire anche dalla riga di titolo del CSV
            var title = ReadTitle(zoneEntry ?? valoriEntry!);
            var mt = SemInTitle().Match(title);
            if (mt.Success)
            {
                int st = int.Parse(mt.Groups[1].Value) * 10 + int.Parse(mt.Groups[2].Value);
                if (sem != null && sem != st)
                    Console.WriteLine($"  Attenzione: semestre nel nome file ({sem}) diverso da quello nel titolo ({st}); uso {st}");
                sem = st;
            }
            if (sem == null) throw new InvalidDataException("Impossibile determinare il semestre dei CSV");

            // Si sostituiscono solo i dati dei comuni presenti nel file: una fornitura nazionale sostituisce
            // l'intero semestre, forniture per regione/provincia/comune si sommano tra loro.
            FillTemp(tx, "tmp_comuni", zoneEntry != null ? CsvColumn(zoneEntry, "Comune_amm") : []);
            FillTemp(tx, "tmp_links", valoriEntry != null ? CsvColumn(valoriEntry, "LinkZona") : []);
            Database.Exec(db, $"""
                DELETE FROM valore WHERE semestre={sem} AND (link_zona IN (SELECT v FROM tmp_links)
                    OR link_zona IN (SELECT link_zona FROM zona WHERE semestre={sem} AND comune_amm IN (SELECT v FROM tmp_comuni)));
                DELETE FROM zona WHERE semestre={sem} AND comune_amm IN (SELECT v FROM tmp_comuni);
                """, tx);
            if (zoneEntry != null) (nZone, elabZone) = ImportZone(zoneEntry, sem.Value, tx);
            if (valoriEntry != null) (nValori, elabValori) = ImportValori(valoriEntry, sem.Value, tx);
            SaveTipologie(tx);
            UpsertSemestre(tx, sem.Value, """
                elaborazione_zone=coalesce(@ez,elaborazione_zone), elaborazione_valori=coalesce(@ev,elaborazione_valori),
                file_csv=@f, caricato_csv=@t,
                n_zone=(SELECT count(*) FROM zona WHERE semestre=@s), n_valori=(SELECT count(*) FROM valore WHERE semestre=@s)
                """,
                ("@ez", elabZone), ("@ev", elabValori), ("@f", fileName), ("@t", Now()), ("@s", sem.Value));
            Console.WriteLine($"  Semestre {Database.SemLabel(sem.Value)}: {nZone} zone, {nValori} valori");
        }

        if (kmlEntries.Count > 0)
        {
            if (sem == null) sem = DetectKmlSemester(kmlEntries);
            if (sem == null) throw new InvalidDataException("Impossibile determinare il semestre dei KML");
            EnsureGeomCache();
            nGeo = ImportKml(kmlEntries, sem.Value, tx);
            UpsertSemestre(tx, sem.Value, "file_kml=@f, caricato_kml=@t, n_geometrie=(SELECT count(*) FROM zona_geo WHERE semestre=@s)",
                ("@f", fileName), ("@t", Now()), ("@s", sem.Value));
            Console.WriteLine($"  Semestre {Database.SemLabel(sem.Value)}: {nGeo} perimetri di zona da {kmlEntries.Count} file KML");
        }

        if (sem == null)
        {
            Console.WriteLine("  Nessun CSV o KML OMI riconosciuto nel file: ignorato.");
            return;
        }

        using (var cmd = db.CreateCommand())
        {
            cmd.Transaction = tx;
            cmd.CommandText = """
                INSERT INTO file_caricato(file, dimensione, semestre, contenuto, caricato) VALUES (@f,@d,@s,@c,@t)
                ON CONFLICT(file) DO UPDATE SET dimensione=@d, semestre=@s, contenuto=@c, caricato=@t
                """;
            cmd.Parameters.AddWithValue("@f", fileName);
            cmd.Parameters.AddWithValue("@d", fi.Length);
            cmd.Parameters.AddWithValue("@s", sem);
            cmd.Parameters.AddWithValue("@c", $"zone={nZone}; valori={nValori}; kml={kmlEntries.Count}; perimetri={nGeo}");
            cmd.Parameters.AddWithValue("@t", Now());
            cmd.ExecuteNonQuery();
        }
        tx.Commit();
        Console.WriteLine($"  completato in {sw.Elapsed.TotalSeconds:F1} s");
    }

    [GeneratedRegex(@"_[A-Z]{6}\d{2}[A-Z]\d{2}[A-Z]\d{3}[A-Z](?=\.zip$)", RegexOptions.IgnoreCase)]
    private static partial Regex CodiceFiscale();

    /// <summary>
    /// I file delle forniture OMI sono nominati QI[P]&lt;richiesta&gt;_&lt;codice fiscale del richiedente&gt;.zip:
    /// nel database si registra il nome senza il codice fiscale, per poterlo condividere.
    /// </summary>
    public static string SafeFileName(string name) => CodiceFiscale().Replace(name, "");

    static string Now() => DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture);

    void UpsertSemestre(SqliteTransaction tx, int sem, string setClause, params (string, object?)[] pars)
    {
        using var cmd = db.CreateCommand();
        cmd.Transaction = tx;
        cmd.CommandText = $"""
            INSERT INTO semestre(semestre, anno, numero) VALUES ({sem}, {sem / 10}, {sem % 10}) ON CONFLICT(semestre) DO NOTHING;
            UPDATE semestre SET {setClause} WHERE semestre={sem};
            """;
        foreach (var (n, v) in pars) cmd.Parameters.AddWithValue(n, v ?? DBNull.Value);
        cmd.ExecuteNonQuery();
    }

    // ------------------------------------------------------------------ CSV

    static HashSet<string> CsvColumn(ZipArchiveEntry e, string column)
    {
        var set = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        foreach (var r in ReadCsv(e, _ => { }))
            if (r.GetValueOrDefault(column) is { } v) set.Add(v);
        return set;
    }

    void FillTemp(SqliteTransaction tx, string table, IEnumerable<string> values)
    {
        Database.Exec(db, $"CREATE TEMP TABLE IF NOT EXISTS {table}(v TEXT PRIMARY KEY); DELETE FROM {table};", tx);
        using var ins = Prepare(tx, $"INSERT OR IGNORE INTO {table}(v) VALUES (@v)", "@v");
        foreach (var v in values)
        {
            Set(ins, "@v", v);
            ins.ExecuteNonQuery();
        }
    }

    static string ReadTitle(ZipArchiveEntry e)
    {
        using var r = new StreamReader(e.Open(), Encoding.UTF8);
        return r.ReadLine() ?? "";
    }

    static IEnumerable<Dictionary<string, string?>> ReadCsv(ZipArchiveEntry e, Action<string> onTitle)
    {
        using var r = new StreamReader(e.Open(), Encoding.UTF8);
        onTitle(r.ReadLine() ?? "");
        var header = (r.ReadLine() ?? "").Split(';').Select(h => h.Trim()).ToArray();
        string? line;
        while ((line = r.ReadLine()) != null)
        {
            if (line.Length == 0) continue;
            var f = line.Split(';');
            var d = new Dictionary<string, string?>(header.Length, StringComparer.OrdinalIgnoreCase);
            for (int i = 0; i < header.Length; i++)
                if (header[i].Length > 0) d[header[i]] = i < f.Length ? TextUtil.Field(f[i]) : null;
            yield return d;
        }
    }

    static string? Elab(string title)
    {
        var m = ElabInTitle().Match(title.Trim());
        return m.Success ? m.Groups[1].Value.Trim() : null;
    }

    SqliteCommand Prepare(SqliteTransaction tx, string sql, params string[] names)
    {
        var cmd = db.CreateCommand();
        cmd.Transaction = tx;
        cmd.CommandText = sql;
        foreach (var n in names) cmd.Parameters.Add(new SqliteParameter(n, null));
        cmd.Prepare();
        return cmd;
    }

    static void Set(SqliteCommand c, string n, object? v) => c.Parameters[n].Value = v ?? DBNull.Value;

    (int, string?) ImportZone(ZipArchiveEntry e, int sem, SqliteTransaction tx)
    {
        string? elab = null;
        using var cmd = Prepare(tx, """
            INSERT OR REPLACE INTO zona(semestre, link_zona, area_territoriale, regione, prov, comune_istat, comune_cat, sez,
                comune_amm, comune_descr, fascia, zona, zona_descr, cod_tip_prev, descr_tip_prev, stato_prev, microzona)
            VALUES (@s,@lz,@at,@re,@pr,@ci,@cc,@sz,@ca,@cd,@fa,@zo,@zd,@ct,@dt,@sp,@mz)
            """, "@s", "@lz", "@at", "@re", "@pr", "@ci", "@cc", "@sz", "@ca", "@cd", "@fa", "@zo", "@zd", "@ct", "@dt", "@sp", "@mz");
        int n = 0;
        foreach (var r in ReadCsv(e, t => elab = Elab(t)))
        {
            var lz = r.GetValueOrDefault("LinkZona");
            var ca = r.GetValueOrDefault("Comune_amm");
            var zo = r.GetValueOrDefault("Zona");
            if (lz == null || ca == null || zo == null) continue;
            Set(cmd, "@s", sem);
            Set(cmd, "@lz", lz);
            Set(cmd, "@at", r.GetValueOrDefault("Area_territoriale"));
            Set(cmd, "@re", r.GetValueOrDefault("Regione"));
            Set(cmd, "@pr", r.GetValueOrDefault("Prov"));
            Set(cmd, "@ci", r.GetValueOrDefault("Comune_ISTAT"));
            Set(cmd, "@cc", r.GetValueOrDefault("Comune_cat"));
            Set(cmd, "@sz", r.GetValueOrDefault("Sez"));
            Set(cmd, "@ca", ca);
            Set(cmd, "@cd", r.GetValueOrDefault("Comune_descrizione"));
            Set(cmd, "@fa", r.GetValueOrDefault("Fascia"));
            Set(cmd, "@zo", zo);
            Set(cmd, "@zd", r.GetValueOrDefault("Zona_Descr"));
            var ct = TextUtil.ParseInt(r.GetValueOrDefault("Cod_tip_prev"));
            var dt = r.GetValueOrDefault("Descr_tip_prev");
            Set(cmd, "@ct", ct);
            Set(cmd, "@dt", dt);
            Set(cmd, "@sp", r.GetValueOrDefault("Stato_prev"));
            Set(cmd, "@mz", TextUtil.ParseInt(r.GetValueOrDefault("Microzona")));
            cmd.ExecuteNonQuery();
            if (ct != null && dt != null) _tipologie[ct.Value] = dt;
            n++;
        }
        return (n, elab);
    }

    (int, string?) ImportValori(ZipArchiveEntry e, int sem, SqliteTransaction tx)
    {
        string? elab = null;
        using var cmd = Prepare(tx, """
            INSERT OR REPLACE INTO valore(semestre, link_zona, cod_tip, stato, stato_prev, compr_min, compr_max, sup_nl_compr,
                loc_min, loc_max, sup_nl_loc)
            VALUES (@s,@lz,@ct,@st,@sp,@cmin,@cmax,@cnl,@lmin,@lmax,@lnl)
            """, "@s", "@lz", "@ct", "@st", "@sp", "@cmin", "@cmax", "@cnl", "@lmin", "@lmax", "@lnl");
        int n = 0;
        foreach (var r in ReadCsv(e, t => elab = Elab(t)))
        {
            var lz = r.GetValueOrDefault("LinkZona");
            var ct = TextUtil.ParseInt(r.GetValueOrDefault("Cod_Tip"));
            if (lz == null || ct == null) continue;
            var dt = r.GetValueOrDefault("Descr_Tipologia");
            if (dt != null) _tipologie[ct.Value] = dt;
            Set(cmd, "@s", sem);
            Set(cmd, "@lz", lz);
            Set(cmd, "@ct", ct);
            Set(cmd, "@st", r.GetValueOrDefault("Stato") ?? "");
            Set(cmd, "@sp", r.GetValueOrDefault("Stato_prev"));
            Set(cmd, "@cmin", TextUtil.ParseDecimal(r.GetValueOrDefault("Compr_min")));
            Set(cmd, "@cmax", TextUtil.ParseDecimal(r.GetValueOrDefault("Compr_max")));
            Set(cmd, "@cnl", r.GetValueOrDefault("Sup_NL_compr"));
            Set(cmd, "@lmin", TextUtil.ParseDecimal(r.GetValueOrDefault("Loc_min")));
            Set(cmd, "@lmax", TextUtil.ParseDecimal(r.GetValueOrDefault("Loc_max")));
            Set(cmd, "@lnl", r.GetValueOrDefault("Sup_NL_loc"));
            cmd.ExecuteNonQuery();
            n++;
        }
        return (n, elab);
    }

    void SaveTipologie(SqliteTransaction tx)
    {
        using var cmd = Prepare(tx, "INSERT INTO tipologia(cod_tip, descrizione) VALUES (@c,@d) ON CONFLICT(cod_tip) DO UPDATE SET descrizione=@d", "@c", "@d");
        foreach (var (c, d) in _tipologie)
        {
            Set(cmd, "@c", c);
            Set(cmd, "@d", d);
            cmd.ExecuteNonQuery();
        }
    }

    // ------------------------------------------------------------------ KML

    sealed class Placemark
    {
        public string? Name, StyleUrl, Description, LinkZona, CodCom, CodZona;
        public Geometry Geom = new();
    }

    void EnsureGeomCache()
    {
        if (_geomLoaded) return;
        using var cmd = db.CreateCommand();
        cmd.CommandText = "SELECT id, hash FROM geometria";
        using var rd = cmd.ExecuteReader();
        while (rd.Read()) _geomByHash[Convert.ToHexString((byte[])rd[1])] = rd.GetInt64(0);
        _geomLoaded = true;
    }

    int? DetectKmlSemester(List<ZipArchiveEntry> entries)
    {
        foreach (var e in entries.Take(5))
            foreach (var pm in ReadKml(e, out _))
            {
                var m = SemInKml().Match(pm.Description ?? "");
                if (m.Success) return int.Parse(m.Groups[1].Value) * 10 + int.Parse(m.Groups[2].Value);
            }
        return null;
    }

    int ImportKml(List<ZipArchiveEntry> entries, int sem, SqliteTransaction tx)
    {
        using var insGeo = Prepare(tx, """
            INSERT INTO geometria(hash, min_lon, min_lat, max_lon, max_lat, centro_lon, centro_lat, area_km2, n_poligoni, n_punti, dati)
            VALUES (@h,@x0,@y0,@x1,@y1,@cx,@cy,@a,@np,@npt,@d)
            """, "@h", "@x0", "@y0", "@x1", "@y1", "@cx", "@cy", "@a", "@np", "@npt", "@d");
        using var insRt = Prepare(tx, "INSERT INTO geometria_rt(id, min_lon, max_lon, min_lat, max_lat) VALUES (@id,@x0,@x1,@y0,@y1)",
            "@id", "@x0", "@x1", "@y0", "@y1");
        using var insZg = Prepare(tx, """
            INSERT INTO zona_geo(semestre, comune_amm, zona, link_zona, nome, colore, geometria_id)
            VALUES (@s,@c,@z,@l,@n,@col,@g)
            """, "@s", "@c", "@z", "@l", "@n", "@col", "@g");
        using var lastId = Prepare(tx, "SELECT last_insert_rowid()");
        // si sostituiscono solo i perimetri dei comuni presenti nel file
        using var delZg = Prepare(tx, "DELETE FROM zona_geo WHERE semestre=@s AND comune_amm=@c", "@s", "@c");
        var sostituiti = new HashSet<string>();
        void Sostituisci(string codCom)
        {
            if (!sostituiti.Add(codCom)) return;
            Set(delZg, "@s", sem);
            Set(delZg, "@c", codCom);
            delZg.ExecuteNonQuery();
        }

        int n = 0, done = 0;
        foreach (var e in entries)
        {
            var fileCode = Path.GetFileNameWithoutExtension(e.Name).ToUpperInvariant();
            Sostituisci(fileCode);
            foreach (var pm in ReadKml(e, out var styles))
            {
                if (pm.Geom.IsEmpty) continue;
                var codCom = (pm.CodCom ?? fileCode).Trim().ToUpperInvariant();
                Sostituisci(codCom);
                var codZona = pm.CodZona?.Trim();
                if (string.IsNullOrEmpty(codZona))
                {
                    var m = ZonaInName().Match(pm.Name ?? "");
                    if (!m.Success) continue;
                    codZona = m.Groups[1].Value;
                }

                var blob = pm.Geom.Encode();
                var hash = SHA256.HashData(blob).AsSpan(0, 16).ToArray();
                var key = Convert.ToHexString(hash);
                if (!_geomByHash.TryGetValue(key, out var gid))
                {
                    var bb = pm.Geom.BBox();
                    var (cx, cy, area) = pm.Geom.CentroidAndArea();
                    Set(insGeo, "@h", hash);
                    Set(insGeo, "@x0", bb.MinX); Set(insGeo, "@y0", bb.MinY);
                    Set(insGeo, "@x1", bb.MaxX); Set(insGeo, "@y1", bb.MaxY);
                    Set(insGeo, "@cx", Math.Round(cx, 6)); Set(insGeo, "@cy", Math.Round(cy, 6));
                    Set(insGeo, "@a", Math.Round(area, 4));
                    Set(insGeo, "@np", pm.Geom.Polygons.Count);
                    Set(insGeo, "@npt", pm.Geom.Polygons.Sum(p => p.Sum(r => r.Length / 2)));
                    Set(insGeo, "@d", blob);
                    insGeo.ExecuteNonQuery();
                    gid = (long)lastId.ExecuteScalar()!;
                    Set(insRt, "@id", gid);
                    Set(insRt, "@x0", bb.MinX); Set(insRt, "@x1", bb.MaxX);
                    Set(insRt, "@y0", bb.MinY); Set(insRt, "@y1", bb.MaxY);
                    insRt.ExecuteNonQuery();
                    _geomByHash[key] = gid;
                }

                string? color = null;
                if (pm.StyleUrl != null) styles.TryGetValue(pm.StyleUrl.TrimStart('#'), out color);
                Set(insZg, "@s", sem);
                Set(insZg, "@c", codCom);
                Set(insZg, "@z", codZona);
                Set(insZg, "@l", string.IsNullOrWhiteSpace(pm.LinkZona) ? null : pm.LinkZona.Trim());
                Set(insZg, "@n", pm.Name);
                Set(insZg, "@col", color);
                Set(insZg, "@g", gid);
                insZg.ExecuteNonQuery();
                n++;
            }
            if (++done % 1000 == 0) Console.WriteLine($"    {done}/{entries.Count} file KML...");
        }
        return n;
    }

    /// <summary>
    /// Legge tutti i placemark di un KML. Se il file è malformato (capita in alcune forniture)
    /// lo rilegge placemark per placemark scartando solo quelli non validi.
    /// </summary>
    static List<Placemark> ReadKml(ZipArchiveEntry e, out Dictionary<string, string> styles)
    {
        string text;
        using (var s = e.Open()) text = DecodeKml(s);
        styles = new Dictionary<string, string>();
        try
        {
            return ReadKmlCore(text, styles).ToList();
        }
        catch (XmlException ex)
        {
            styles.Clear();
            var res = new List<Placemark>();
            int first = text.IndexOf("<Placemark", StringComparison.Ordinal);
            if (first < 0) throw;
            var header = text[..first];
            int skipped = 0, pos = first;
            while (pos >= 0 && pos < text.Length)
            {
                int end = text.IndexOf("</Placemark>", pos, StringComparison.Ordinal);
                if (end < 0) break;
                end += "</Placemark>".Length;
                var chunk = header + text[pos..end] + "</Document></kml>";
                try { res.AddRange(ReadKmlCore(chunk, styles)); }
                catch (XmlException) { skipped++; }
                pos = text.IndexOf("<Placemark", end, StringComparison.Ordinal);
            }
            Console.WriteLine($"    Attenzione: {e.Name} malformato ({ex.Message}); letti {res.Count} placemark, scartati {skipped}");
            return res;
        }
    }

    static readonly Encoding StrictUtf8 = new UTF8Encoding(false, throwOnInvalidBytes: true);

    /// <summary>
    /// Alcuni KML OMI dichiarano UTF-8 ma contengono caratteri Latin-1:
    /// si decodifica in UTF-8 stretto e, se fallisce, in Latin-1.
    /// </summary>
    static string DecodeKml(Stream s)
    {
        using var ms = new MemoryStream();
        s.CopyTo(ms);
        var bytes = ms.GetBuffer().AsSpan(0, (int)ms.Length);
        try { return StrictUtf8.GetString(bytes); }
        catch (DecoderFallbackException) { return Encoding.Latin1.GetString(bytes); }
    }

    static IEnumerable<Placemark> ReadKmlCore(string text, Dictionary<string, string> styles)
    {
        using var r = XmlReader.Create(new StringReader(text), new XmlReaderSettings
        {
            DtdProcessing = DtdProcessing.Ignore,
            IgnoreComments = true,
            IgnoreWhitespace = true,
            IgnoreProcessingInstructions = true,
        });
        Placemark? pm = null;
        List<int[]>? poly = null;
        string? styleId = null, dataName = null;
        bool inPolyStyle = false;

        while (!r.EOF)
        {
            if (r.NodeType == XmlNodeType.Element)
            {
                switch (r.LocalName)
                {
                    case "Style":
                        styleId = r.GetAttribute("id");
                        break;
                    case "PolyStyle":
                        inPolyStyle = !r.IsEmptyElement;
                        break;
                    case "color" when inPolyStyle && styleId != null && pm == null:
                        styles[styleId] = r.ReadElementContentAsString().Trim();
                        continue;
                    case "Placemark":
                        pm = new Placemark();
                        break;
                    case "name" when pm != null && pm.Name == null:
                        pm.Name = r.ReadElementContentAsString().Trim();
                        continue;
                    case "description" when pm != null:
                        pm.Description = r.ReadElementContentAsString();
                        continue;
                    case "styleUrl" when pm != null:
                        pm.StyleUrl = r.ReadElementContentAsString().Trim();
                        continue;
                    case "Data" when pm != null:
                        dataName = r.GetAttribute("name")?.ToUpperInvariant();
                        break;
                    case "value" when pm != null && dataName != null:
                        var v = r.ReadElementContentAsString().Trim();
                        switch (dataName)
                        {
                            case "LINKZONA": pm.LinkZona = v; break;
                            case "CODCOM": pm.CodCom = v; break;
                            case "CODZONA": pm.CodZona = v; break;
                        }
                        continue;
                    case "Polygon" when pm != null:
                        poly = new List<int[]>();
                        pm.Geom.Polygons.Add(poly);
                        break;
                    case "coordinates" when poly != null:
                        var ring = ParseCoords(r.ReadElementContentAsString());
                        if (ring.Length >= 6) poly.Add(ring);
                        continue;
                }
            }
            else if (r.NodeType == XmlNodeType.EndElement)
            {
                switch (r.LocalName)
                {
                    case "Placemark":
                        if (pm != null)
                        {
                            pm.Geom.Polygons.RemoveAll(p => p.Count == 0);
                            yield return pm;
                        }
                        pm = null; poly = null;
                        break;
                    case "Polygon": poly = null; break;
                    case "PolyStyle": inPolyStyle = false; break;
                    case "Style": styleId = null; break;
                    case "Data": dataName = null; break;
                }
            }
            r.Read();
        }
    }

    static int[] ParseCoords(string text)
    {
        var list = new List<int>(256);
        var span = text.AsSpan();
        int i = 0;
        while (i < span.Length)
        {
            while (i < span.Length && char.IsWhiteSpace(span[i])) i++;
            int start = i;
            while (i < span.Length && !char.IsWhiteSpace(span[i])) i++;
            if (i == start) break;
            var tok = span[start..i];
            int c1 = tok.IndexOf(',');
            if (c1 < 0) continue;
            var rest = tok[(c1 + 1)..];
            int c2 = rest.IndexOf(',');
            var latS = c2 < 0 ? rest : rest[..c2];
            if (double.TryParse(tok[..c1], NumberStyles.Float, CultureInfo.InvariantCulture, out var lon) &&
                double.TryParse(latS, NumberStyles.Float, CultureInfo.InvariantCulture, out var lat))
            {
                list.Add((int)Math.Round(lon * Geometry.Scale));
                list.Add((int)Math.Round(lat * Geometry.Scale));
            }
        }
        return list.ToArray();
    }

    // ------------------------------------------------------------------ Post-elaborazione

    /// <summary>Rimuove geometrie non più referenziate e ricostruisce l'anagrafica comuni.</summary>
    public void Completa()
    {
        Console.WriteLine("Aggiornamento anagrafiche e indici...");
        using var tx = db.BeginTransaction();
        Database.Exec(db, """
            DELETE FROM geometria_rt WHERE id NOT IN (SELECT geometria_id FROM zona_geo);
            DELETE FROM geometria WHERE id NOT IN (SELECT geometria_id FROM zona_geo);

            DELETE FROM comune;
            INSERT INTO comune(comune_amm, comune_istat, descrizione, prov, regione, area_territoriale, primo_semestre, ultimo_semestre)
            SELECT z.comune_amm, z.comune_istat, z.comune_descr, z.prov, z.regione, z.area_territoriale,
                   (SELECT min(z2.semestre) FROM zona z2 WHERE z2.comune_amm = z.comune_amm), max(z.semestre)
            FROM zona z GROUP BY z.comune_amm;
            DELETE FROM comune_nome;
            """, tx);

        var rows = new List<(string ca, string nome, string? prov)>();
        using (var cmd = db.CreateCommand())
        {
            cmd.Transaction = tx;
            cmd.CommandText = "SELECT DISTINCT comune_amm, comune_descr, prov FROM zona WHERE comune_descr IS NOT NULL";
            using var rd = cmd.ExecuteReader();
            while (rd.Read()) rows.Add((rd.GetString(0), rd.GetString(1), rd.IsDBNull(2) ? null : rd.GetString(2)));
        }
        using (var ins = Prepare(tx, "INSERT OR IGNORE INTO comune_nome(comune_amm, nome, nome_norm, prov) VALUES (@c,@n,@nn,@p)", "@c", "@n", "@nn", "@p"))
            foreach (var (ca, nome, prov) in rows)
            {
                Set(ins, "@c", ca); Set(ins, "@n", nome); Set(ins, "@nn", TextUtil.Norm(nome)); Set(ins, "@p", prov ?? "");
                ins.ExecuteNonQuery();
            }
        tx.Commit();
        Database.Exec(db, "ANALYZE; PRAGMA wal_checkpoint(TRUNCATE);");
    }
}
