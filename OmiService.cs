// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.Encodings.Web;
using System.Text.RegularExpressions;
using Microsoft.Data.Sqlite;

namespace OMIMyMCP;

/// <summary>Interrogazioni sul database OMI usate dagli strumenti MCP.</summary>
public sealed partial class OmiService(string dbPath)
{
    public static readonly JsonSerializerOptions Json = new()
    {
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        WriteIndented = false,
    };

    public static string ToJson(object o) => JsonSerializer.Serialize(o, Json);

    SqliteConnection Open() => Database.Open(dbPath, readOnly: true);

    static readonly Dictionary<string, string> Fasce = new()
    {
        ["B"] = "Centrale", ["C"] = "Semicentrale", ["D"] = "Periferica", ["E"] = "Suburbana", ["R"] = "Extraurbana/rurale",
    };
    static readonly Dictionary<string, string> Stati = new()
    {
        ["N"] = "NORMALE", ["O"] = "OTTIMO", ["S"] = "SCADENTE",
    };

    [GeneratedRegex(@"^[A-Z]\d{3}$")] private static partial Regex Belfiore();

    // ------------------------------------------------------------------ helper SQL

    static List<Dictionary<string, object?>> Query(SqliteConnection c, string sql, params (string, object?)[] p)
    {
        using var cmd = c.CreateCommand();
        cmd.CommandText = sql;
        foreach (var (n, v) in p) cmd.Parameters.AddWithValue(n, v ?? DBNull.Value);
        using var rd = cmd.ExecuteReader();
        var res = new List<Dictionary<string, object?>>();
        while (rd.Read())
        {
            var d = new Dictionary<string, object?>(rd.FieldCount);
            for (int i = 0; i < rd.FieldCount; i++) d[rd.GetName(i)] = rd.IsDBNull(i) ? null : rd.GetValue(i);
            res.Add(d);
        }
        return res;
    }

    static object? Scalar(SqliteConnection c, string sql, params (string, object?)[] p)
    {
        using var cmd = c.CreateCommand();
        cmd.CommandText = sql;
        foreach (var (n, v) in p) cmd.Parameters.AddWithValue(n, v ?? DBNull.Value);
        var r = cmd.ExecuteScalar();
        return r is DBNull ? null : r;
    }

    static string? S(Dictionary<string, object?> d, string k) => d.TryGetValue(k, out var v) ? v?.ToString() : null;
    static long L(Dictionary<string, object?> d, string k) => Convert.ToInt64(d[k]);

    // ------------------------------------------------------------------ semestri

    int ResolveSemestre(SqliteConnection c, string? semestre)
    {
        var s = Database.ParseSemestre(semestre);
        if (s != null)
        {
            if (Scalar(c, "SELECT 1 FROM semestre WHERE semestre=@s AND n_zone>0", ("@s", s)) == null)
                throw new ArgumentException($"Semestre {Database.SemLabel(s.Value)} non presente nel database. Usare elenco_semestri.");
            return s.Value;
        }
        var last = Scalar(c, "SELECT max(semestre) FROM semestre WHERE n_zone>0")
                   ?? throw new InvalidOperationException("Il database non contiene dati OMI.");
        return Convert.ToInt32(last);
    }

    static int SemIndex(long s) => (int)(s / 10 * 2 + s % 10);

    /// <summary>Semestre con perimetri (KML) più vicino a quello richiesto.</summary>
    int? GeoSemestre(SqliteConnection c, int sem)
    {
        var r = Scalar(c, "SELECT semestre FROM semestre WHERE n_geometrie>0 ORDER BY abs((semestre/10)*2+semestre%10-@i), semestre DESC LIMIT 1",
            ("@i", SemIndex(sem)));
        return r == null ? null : Convert.ToInt32(r);
    }

    public string ElencoSemestri()
    {
        using var c = Open();
        var rows = Query(c, "SELECT * FROM semestre ORDER BY semestre DESC");
        return ToJson(new
        {
            semestri = rows.Select(r => new
            {
                semestre = Database.SemLabel(L(r, "semestre")),
                n_zone = r["n_zone"], n_valori = r["n_valori"], n_perimetri = r["n_geometrie"],
                elaborazione = r["elaborazione_valori"] ?? r["elaborazione_zone"],
                file_csv = r["file_csv"], file_kml = r["file_kml"],
            }),
            nota = "I perimetri (KML) sono disponibili solo per alcuni semestri; per gli altri la ricerca per coordinate usa i perimetri del semestre più vicino.",
            fonte_dati = About.FonteDati,
            software = About.Attribuzione,
        });
    }

    public string Tipologie()
    {
        using var c = Open();
        return ToJson(Query(c, "SELECT cod_tip AS codice, descrizione FROM tipologia ORDER BY cod_tip"));
    }

    // ------------------------------------------------------------------ comuni

    List<Dictionary<string, object?>> FindComuni(SqliteConnection c, string testo, string? provincia)
    {
        testo = testo.Trim();
        var prov = string.IsNullOrWhiteSpace(provincia) ? null : provincia.Trim().ToUpperInvariant();
        const string sel = "SELECT c.* FROM comune c ";
        var up = testo.ToUpperInvariant();
        List<Dictionary<string, object?>> res;
        if (Belfiore().IsMatch(up))
            res = Query(c, sel + "WHERE c.comune_amm=@t", ("@t", up));
        else if (up.All(char.IsDigit))
        {
            // Comune_ISTAT nei CSV OMI è prefissato dal codice regione (es. 1006003 = 006003)
            res = Query(c, sel + "WHERE c.comune_istat=@t OR substr(c.comune_istat,-6)=substr('000000'||@t,-6)", ("@t", up));
        }
        else
        {
            var n = TextUtil.Norm(testo);
            res = Query(c, sel + "WHERE c.comune_amm IN (SELECT comune_amm FROM comune_nome WHERE nome_norm=@n)", ("@n", n));
            if (res.Count == 0)
                res = Query(c, sel + "WHERE c.comune_amm IN (SELECT comune_amm FROM comune_nome WHERE nome_norm LIKE @p) ORDER BY c.descrizione LIMIT 200", ("@p", n + "%"));
            if (res.Count == 0)
                res = Query(c, sel + "WHERE c.comune_amm IN (SELECT comune_amm FROM comune_nome WHERE nome_norm LIKE @p) ORDER BY c.descrizione LIMIT 200", ("@p", "%" + n + "%"));
        }
        if (prov != null)
            res = res.Where(r => string.Equals(S(r, "prov"), prov, StringComparison.OrdinalIgnoreCase)
                              || TextUtil.Norm(S(r, "prov")) == TextUtil.Norm(prov)).ToList();
        return res;
    }

    static object ComuneInfo(Dictionary<string, object?> r) => new
    {
        codice_catastale = r["comune_amm"],
        codice_istat = r["comune_istat"],
        nome = r["descrizione"],
        provincia = r["prov"],
        regione = r["regione"],
        area_territoriale = r["area_territoriale"],
        primo_semestre = r["primo_semestre"] is { } p ? Database.SemLabel(Convert.ToInt64(p)) : null,
        ultimo_semestre = r["ultimo_semestre"] is { } u ? Database.SemLabel(Convert.ToInt64(u)) : null,
    };

    Dictionary<string, object?> ResolveComune(SqliteConnection c, string comune, string? provincia)
    {
        var res = FindComuni(c, comune, provincia);
        if (res.Count == 1) return res[0];
        if (res.Count == 0) throw new ArgumentException($"Nessun comune trovato per '{comune}'" + (provincia != null ? $" in provincia {provincia}" : "") + ".");
        // se c'è una corrispondenza esatta unica sul nome, usala
        var exact = res.Where(r => TextUtil.Norm(S(r, "descrizione")) == TextUtil.Norm(comune)).ToList();
        if (exact.Count == 1) return exact[0];
        throw new ArgumentException($"'{comune}' è ambiguo, specificare la provincia o il codice catastale. Candidati: " +
            string.Join(", ", res.Take(20).Select(r => $"{S(r, "descrizione")} ({S(r, "prov")}) [{S(r, "comune_amm")}]")));
    }

    public string CercaComuni(string testo, string? provincia)
    {
        using var c = Open();
        var res = FindComuni(c, testo, provincia);
        return ToJson(new
        {
            trovati = res.Count,
            comuni = res.Take(50).Select(r => ComuneInfo(r)),
        });
    }

    // ------------------------------------------------------------------ zone

    static object ZonaInfo(Dictionary<string, object?> z) => new
    {
        zona = z["zona"],
        link_zona = z["link_zona"],
        fascia = z["fascia"],
        fascia_descr = S(z, "fascia") is { } f && Fasce.TryGetValue(f, out var fd) ? fd : null,
        descrizione = z["zona_descr"],
        microzona = z["microzona"],
        tipologia_prevalente = z["cod_tip_prev"] == null ? null : new { codice = z["cod_tip_prev"], descrizione = z["descr_tip_prev"] },
        stato_prevalente = S(z, "stato_prev") is { } sp ? (Stati.TryGetValue(sp, out var sd) ? sd : sp) : null,
        comune_catastale = z["comune_cat"],
        sezione = z["sez"],
    };

    static IEnumerable<object> Valori(SqliteConnection c, int sem, string linkZona) =>
        Query(c, """
            SELECT v.*, t.descrizione AS tip_descr FROM valore v LEFT JOIN tipologia t ON t.cod_tip=v.cod_tip
            WHERE v.semestre=@s AND v.link_zona=@l ORDER BY v.cod_tip, v.stato
            """, ("@s", sem), ("@l", linkZona))
        .Select(v => new
        {
            cod_tipologia = v["cod_tip"],
            tipologia = v["tip_descr"],
            stato = S(v, "stato") is { Length: > 0 } st ? st : null,
            stato_prevalente = S(v, "stato_prev") == "P" ? true : (bool?)null,
            compravendita_eur_mq = v["compr_min"] == null && v["compr_max"] == null ? null
                : new { min = v["compr_min"], max = v["compr_max"], superficie = SupDescr(S(v, "sup_nl_compr")) },
            locazione_eur_mq_mese = v["loc_min"] == null && v["loc_max"] == null ? null
                : new { min = v["loc_min"], max = v["loc_max"], superficie = SupDescr(S(v, "sup_nl_loc")) },
        });

    static string? SupDescr(string? s) => s switch { "L" => "lorda", "N" => "netta", null => null, _ => s };

    /// <summary>Informazioni sul perimetro di una zona (dal semestre KML più vicino).</summary>
    object? GeoInfo(SqliteConnection c, string comuneAmm, string zona, string? linkZona, int sem, bool includiGeometria)
    {
        var rows = Query(c, """
            SELECT zg.semestre, zg.link_zona, zg.colore, g.id, g.min_lon, g.min_lat, g.max_lon, g.max_lat, g.centro_lon, g.centro_lat,
                   g.area_km2, g.n_poligoni, g.n_punti, g.dati
            FROM zona_geo zg JOIN geometria g ON g.id=zg.geometria_id
            WHERE zg.comune_amm=@c AND zg.zona=@z
            ORDER BY abs((zg.semestre/10)*2+zg.semestre%10-@i), zg.semestre DESC
            """, ("@c", comuneAmm), ("@z", zona), ("@i", SemIndex(sem)));
        if (rows.Count == 0) return null;
        var gs = L(rows[0], "semestre");
        var sel = rows.Where(r => L(r, "semestre") == gs).ToList();
        if (sel.Count > 1 && linkZona != null)
        {
            var byLink = sel.Where(r => S(r, "link_zona") == linkZona).ToList();
            if (byLink.Count > 0) sel = byLink;
        }
        var g = sel[0];
        return new
        {
            semestre_perimetro = Database.SemLabel(gs),
            centro = new { lat = g["centro_lat"], lon = g["centro_lon"] },
            bbox = new { min_lat = g["min_lat"], min_lon = g["min_lon"], max_lat = g["max_lat"], max_lon = g["max_lon"] },
            area_km2 = g["area_km2"],
            n_poligoni = g["n_poligoni"],
            n_punti = g["n_punti"],
            colore_kml = g["colore"],
            geometria = includiGeometria ? Geometry.Decode((byte[])g["dati"]!).ToGeoJson() : null,
        };
    }

    public string ZoneComune(string comune, string? provincia, string? semestre, bool includiValori, bool includiPerimetri)
    {
        using var c = Open();
        var com = ResolveComune(c, comune, provincia);
        int sem = ResolveSemestre(c, semestre);
        var ca = S(com, "comune_amm")!;
        var zone = Query(c, "SELECT * FROM zona WHERE comune_amm=@c AND semestre=@s ORDER BY fascia, zona", ("@c", ca), ("@s", sem));
        var list = zone.Select(z => new
        {
            info = ZonaInfo(z),
            perimetro = includiPerimetri ? GeoInfo(c, ca, S(z, "zona")!, S(z, "link_zona"), sem, false) : null,
            valori = includiValori ? Valori(c, sem, S(z, "link_zona")!).ToList() : null,
        });
        return ToJson(new
        {
            comune = ComuneInfo(com),
            semestre = Database.SemLabel(sem),
            n_zone = zone.Count,
            zone = list,
            nota = zone.Count == 0 ? "Nessuna zona per il comune nel semestre indicato." : null,
        });
    }

    public string DettaglioZona(string comune, string zona, string? provincia, string? semestre, bool includiGeometria)
    {
        using var c = Open();
        int sem = ResolveSemestre(c, semestre);
        List<Dictionary<string, object?>> zrows;
        Dictionary<string, object?>? com = null;
        // accetta anche direttamente un LinkZona (es. AL00000001)
        zrows = Query(c, "SELECT * FROM zona WHERE link_zona=@l AND semestre=@s", ("@l", zona.Trim().ToUpperInvariant()), ("@s", sem));
        if (zrows.Count == 0)
        {
            com = ResolveComune(c, comune, provincia);
            zrows = Query(c, "SELECT * FROM zona WHERE comune_amm=@c AND zona=@z AND semestre=@s",
                ("@c", S(com, "comune_amm")), ("@z", zona.Trim().ToUpperInvariant()), ("@s", sem));
        }
        if (zrows.Count == 0)
            throw new ArgumentException($"Zona '{zona}' non trovata per il comune indicato nel semestre {Database.SemLabel(sem)}. Usare zone_comune per l'elenco.");
        com ??= Query(c, "SELECT * FROM comune WHERE comune_amm=@c", ("@c", S(zrows[0], "comune_amm"))).FirstOrDefault();
        return ToJson(new
        {
            comune = com != null ? ComuneInfo(com) : null,
            semestre = Database.SemLabel(sem),
            zone = zrows.Select(z => ZonaCompleta(c, z, sem, includiGeometria)).ToList(),
        });
    }

    object ZonaCompleta(SqliteConnection c, Dictionary<string, object?> z, int sem, bool includiGeometria) => new
    {
        info = ZonaInfo(z),
        valori = Valori(c, sem, S(z, "link_zona")!).ToList(),
        perimetro = GeoInfo(c, S(z, "comune_amm")!, S(z, "zona")!, S(z, "link_zona"), sem, includiGeometria),
    };

    // ------------------------------------------------------------------ coordinate

    sealed record Hit(long GeomId, Geometry Geom, string ComuneAmm, string Zona, string? LinkZona, double DistM);

    List<Hit> Candidates(SqliteConnection c, int geoSem, double lat, double lon, double marginDeg)
    {
        var rows = Query(c, """
            SELECT g.id, g.dati, zg.comune_amm, zg.zona, zg.link_zona
            FROM geometria_rt rt
            JOIN geometria g ON g.id=rt.id
            JOIN zona_geo zg ON zg.geometria_id=rt.id AND zg.semestre=@gs
            WHERE rt.min_lon<=@x+@m AND rt.max_lon>=@x-@m AND rt.min_lat<=@y+@m AND rt.max_lat>=@y-@m
            """, ("@gs", geoSem), ("@x", lon), ("@y", lat), ("@m", marginDeg));
        var cache = new Dictionary<long, Geometry>();
        return rows.Select(r =>
        {
            var id = L(r, "id");
            if (!cache.TryGetValue(id, out var g)) cache[id] = g = Geometry.Decode((byte[])r["dati"]!);
            var d = g.Contains(lon, lat) ? 0 : g.DistanceMeters(lon, lat);
            return new Hit(id, g, S(r, "comune_amm")!, S(r, "zona")!, S(r, "link_zona"), d);
        }).ToList();
    }

    object DatiZonaPerHit(SqliteConnection c, Hit h, int sem, bool includiGeometria)
    {
        var zrows = Query(c, "SELECT * FROM zona WHERE comune_amm=@c AND zona=@z AND semestre=@s", ("@c", h.ComuneAmm), ("@z", h.Zona), ("@s", sem));
        if (zrows.Count > 1 && h.LinkZona != null)
        {
            var byLink = zrows.Where(z => S(z, "link_zona") == h.LinkZona).ToList();
            if (byLink.Count > 0) zrows = byLink;
        }
        var com = Query(c, "SELECT * FROM comune WHERE comune_amm=@c", ("@c", h.ComuneAmm)).FirstOrDefault();
        return new
        {
            comune = com != null ? ComuneInfo(com) : new { codice_catastale = h.ComuneAmm } as object,
            zona = h.Zona,
            distanza_m = h.DistM > 0 ? Math.Round(h.DistM) : (double?)null,
            dati = zrows.Select(z => ZonaCompleta(c, z, sem, includiGeometria)).ToList(),
            nota = zrows.Count == 0
                ? $"La zona {h.Zona} individuata dal perimetro non esiste nei dati del semestre {Database.SemLabel(sem)} (codici zona cambiati tra le revisioni)."
                : null,
        };
    }

    public string ZonaDaCoordinate(double lat, double lon, string? semestre, bool includiGeometria)
    {
        using var c = Open();
        int sem = ResolveSemestre(c, semestre);
        var gs = GeoSemestre(c, sem) ?? throw new InvalidOperationException("Nel database non sono presenti perimetri di zona (file KML).");
        var cands = Candidates(c, gs, lat, lon, 0);
        var inside = cands.Where(h => h.DistM == 0).ToList();
        bool approx = false;
        if (inside.Count == 0)
        {
            // punto fuori da ogni perimetro: zona più vicina entro ~2 km
            cands = Candidates(c, gs, lat, lon, 0.02).Where(h => h.DistM <= 2000).OrderBy(h => h.DistM).ToList();
            if (cands.Count == 0)
                return ToJson(new { trovato = false, lat, lon, semestre = Database.SemLabel(sem), messaggio = "Nessuna zona OMI contiene il punto né si trova entro 2 km." });
            inside = [cands[0]];
            approx = true;
        }
        return ToJson(new
        {
            trovato = true,
            lat, lon,
            semestre = Database.SemLabel(sem),
            semestre_perimetri = Database.SemLabel(gs),
            approssimato = approx ? true : (bool?)null,
            nota = approx ? "Il punto non ricade in alcun perimetro OMI: restituita la zona più vicina." :
                   gs != sem ? "Perimetri non disponibili per il semestre richiesto: usati quelli del semestre più vicino, collegati per codice zona." : null,
            risultati = inside.Select(h => DatiZonaPerHit(c, h, sem, includiGeometria)).ToList(),
        });
    }

    public string ZoneVicine(double lat, double lon, double raggioM, string? semestre)
    {
        using var c = Open();
        int sem = ResolveSemestre(c, semestre);
        var gs = GeoSemestre(c, sem) ?? throw new InvalidOperationException("Nel database non sono presenti perimetri di zona (file KML).");
        raggioM = Math.Clamp(raggioM, 0, 20000);
        var marg = raggioM / 111000.0 / Math.Max(0.2, Math.Cos(lat * Math.PI / 180));
        var hits = Candidates(c, gs, lat, lon, marg).Where(h => h.DistM <= raggioM)
            .GroupBy(h => (h.ComuneAmm, h.Zona)).Select(g => g.OrderBy(h => h.DistM).First())
            .OrderBy(h => h.DistM).Take(100).ToList();
        var res = hits.Select(h =>
        {
            var z = Query(c, "SELECT * FROM zona WHERE comune_amm=@c AND zona=@z AND semestre=@s", ("@c", h.ComuneAmm), ("@z", h.Zona), ("@s", sem)).FirstOrDefault();
            var com = Query(c, "SELECT descrizione, prov FROM comune WHERE comune_amm=@c", ("@c", h.ComuneAmm)).FirstOrDefault();
            var abit = z == null ? null : Query(c, """
                SELECT compr_min, compr_max, loc_min, loc_max, stato FROM valore WHERE semestre=@s AND link_zona=@l AND cod_tip=20
                ORDER BY (stato_prev='P') DESC LIMIT 1
                """, ("@s", sem), ("@l", S(z, "link_zona"))).FirstOrDefault();
            return new
            {
                comune = com == null ? h.ComuneAmm : $"{S(com, "descrizione")} ({S(com, "prov")})",
                codice_catastale = h.ComuneAmm,
                zona = h.Zona,
                link_zona = z?["link_zona"],
                fascia = z?["fascia"],
                descrizione = z?["zona_descr"],
                distanza_m = Math.Round(h.DistM),
                abitazioni_civili = abit == null ? null : new
                {
                    stato = abit["stato"], compr_min = abit["compr_min"], compr_max = abit["compr_max"],
                    loc_min = abit["loc_min"], loc_max = abit["loc_max"],
                },
            };
        }).ToList();
        return ToJson(new { lat, lon, raggio_m = raggioM, semestre = Database.SemLabel(sem), semestre_perimetri = Database.SemLabel(gs), n = res.Count, zone = res });
    }

    // ------------------------------------------------------------------ serie storiche / statistiche

    public string StoricoZona(string comune, string zona, string? provincia, int? codTipologia, string? stato)
    {
        using var c = Open();
        var com = ResolveComune(c, comune, provincia);
        var ca = S(com, "comune_amm")!;
        var z = zona.Trim().ToUpperInvariant();
        var rows = Query(c, """
            SELECT z.semestre, z.link_zona, z.zona_descr, z.fascia, v.cod_tip, t.descrizione AS tip_descr, v.stato, v.stato_prev,
                   v.compr_min, v.compr_max, v.loc_min, v.loc_max
            FROM zona z JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona
            LEFT JOIN tipologia t ON t.cod_tip=v.cod_tip
            WHERE z.comune_amm=@c AND (z.zona=@z OR z.link_zona=@z)
              AND (@t IS NULL OR v.cod_tip=@t) AND (@st IS NULL OR v.stato=@st)
            ORDER BY v.cod_tip, v.stato, z.semestre
            """, ("@c", ca), ("@z", z), ("@t", codTipologia), ("@st", stato?.Trim().ToUpperInvariant()));
        var serie = rows.GroupBy(r => (tip: L(r, "cod_tip"), descr: S(r, "tip_descr"), stato: S(r, "stato")))
            .Select(g => new
            {
                cod_tipologia = g.Key.tip,
                tipologia = g.Key.descr,
                stato = g.Key.stato is { Length: > 0 } s ? s : null,
                valori = g.Select(r => new
                {
                    semestre = Database.SemLabel(L(r, "semestre")),
                    compr_min = r["compr_min"], compr_max = r["compr_max"],
                    loc_min = r["loc_min"], loc_max = r["loc_max"],
                    prevalente = S(r, "stato_prev") == "P" ? true : (bool?)null,
                }).ToList(),
            }).ToList();
        var descr = rows.GroupBy(r => S(r, "zona_descr")).Select(g => new
        {
            descrizione = g.Key,
            dal = Database.SemLabel(g.Min(r => L(r, "semestre"))),
            al = Database.SemLabel(g.Max(r => L(r, "semestre"))),
        }).ToList();
        return ToJson(new
        {
            comune = ComuneInfo(com),
            zona = z,
            descrizioni_zona = descr,
            n_serie = serie.Count,
            serie,
            nota = serie.Count == 0 ? "Nessun dato: verificare codice zona (zone_comune) o filtri." :
                   "I codici zona OMI possono essere stati ridefiniti nelle revisioni della zonizzazione: confrontare le descrizioni.",
        });
    }

    public string StatisticheComune(string comune, string? provincia, string? semestre)
    {
        using var c = Open();
        var com = ResolveComune(c, comune, provincia);
        int sem = ResolveSemestre(c, semestre);
        var rows = Query(c, """
            SELECT v.cod_tip, t.descrizione AS tip_descr, count(DISTINCT v.link_zona) AS n_zone,
                   min(v.compr_min) AS compr_min, max(v.compr_max) AS compr_max, round(avg((v.compr_min+v.compr_max)/2.0),1) AS compr_medio,
                   min(v.loc_min) AS loc_min, max(v.loc_max) AS loc_max, round(avg((v.loc_min+v.loc_max)/2.0),2) AS loc_medio
            FROM zona z JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona
            LEFT JOIN tipologia t ON t.cod_tip=v.cod_tip
            WHERE z.comune_amm=@c AND z.semestre=@s AND (v.stato_prev='P' OR NOT EXISTS (
                SELECT 1 FROM valore v2 WHERE v2.semestre=v.semestre AND v2.link_zona=v.link_zona AND v2.cod_tip=v.cod_tip AND v2.stato_prev='P'))
            GROUP BY v.cod_tip ORDER BY v.cod_tip
            """, ("@c", S(com, "comune_amm")), ("@s", sem));
        var fasce = Query(c, """
            SELECT z.fascia, count(*) AS n_zone,
                   round(avg((v.compr_min+v.compr_max)/2.0),1) AS abitazioni_civili_compr_medio,
                   round(avg((v.loc_min+v.loc_max)/2.0),2) AS abitazioni_civili_loc_medio
            FROM zona z LEFT JOIN valore v ON v.semestre=z.semestre AND v.link_zona=z.link_zona AND v.cod_tip=20 AND v.stato_prev='P'
            WHERE z.comune_amm=@c AND z.semestre=@s GROUP BY z.fascia ORDER BY z.fascia
            """, ("@c", S(com, "comune_amm")), ("@s", sem));
        return ToJson(new
        {
            comune = ComuneInfo(com),
            semestre = Database.SemLabel(sem),
            nota = "Valori €/m² (locazione €/m² mese) calcolati sullo stato conservativo prevalente di ciascuna zona.",
            per_tipologia = rows.Select(r => new
            {
                cod_tipologia = r["cod_tip"], tipologia = r["tip_descr"], n_zone = r["n_zone"],
                compravendita = new { min = r["compr_min"], max = r["compr_max"], medio = r["compr_medio"] },
                locazione = new { min = r["loc_min"], max = r["loc_max"], medio = r["loc_medio"] },
            }),
            per_fascia = fasce.Select(f => new
            {
                fascia = f["fascia"],
                fascia_descr = S(f, "fascia") is { } fs && Fasce.TryGetValue(fs, out var fd) ? fd : null,
                n_zone = f["n_zone"],
                abitazioni_civili_compr_medio = f["abitazioni_civili_compr_medio"],
                abitazioni_civili_loc_medio = f["abitazioni_civili_loc_medio"],
            }),
        });
    }
}
