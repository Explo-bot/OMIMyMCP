// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

using System.ComponentModel;
using ModelContextProtocol;
using ModelContextProtocol.Server;

namespace OMIMyMCP;

[McpServerToolType]
public sealed class OmiTools(OmiService omi)
{
    /// <summary>Riporta al client MCP il messaggio degli errori di validazione (comune ambiguo, zona inesistente, ...).</summary>
    static string Run(Func<string> f)
    {
        try { return f(); }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException)
        {
            throw new McpException(ex.Message);
        }
    }

    [McpServerTool(Name = "elenco_semestri", ReadOnly = true, Idempotent = true),
     Description("Elenca i semestri OMI caricati nel database con numero di zone, quotazioni e perimetri disponibili.")]
    public string ElencoSemestri() => Run(() => omi.ElencoSemestri());

    [McpServerTool(Name = "elenco_tipologie", ReadOnly = true, Idempotent = true),
     Description("Elenca le tipologie edilizie OMI (codice e descrizione), es. 20 = Abitazioni civili, 13 = Box.")]
    public string ElencoTipologie() => Run(() => omi.Tipologie());

    [McpServerTool(Name = "cerca_comuni", ReadOnly = true, Idempotent = true),
     Description("Cerca comuni per nome (anche parziale, senza accenti), codice catastale (es. H501) o codice ISTAT. Restituisce codici, provincia, regione e semestri disponibili.")]
    public string CercaComuni(
        [Description("Nome del comune, codice catastale o codice ISTAT")] string testo,
        [Description("Sigla della provincia per filtrare (es. RM), facoltativa")] string? provincia = null)
        => Run(() => omi.CercaComuni(testo, provincia));

    [McpServerTool(Name = "zone_comune", ReadOnly = true, Idempotent = true),
     Description("Elenca le zone OMI di un comune in un semestre: codice zona, LinkZona, fascia, descrizione, microzona, tipologia e stato prevalenti. Facoltativamente include quotazioni e dati del perimetro (centro, bbox, area).")]
    public string ZoneComune(
        [Description("Nome del comune, codice catastale o codice ISTAT")] string comune,
        [Description("Sigla provincia, utile se il nome è ambiguo")] string? provincia = null,
        [Description("Semestre nel formato AAAA/S (es. 2025/1); se omesso l'ultimo disponibile")] string? semestre = null,
        [Description("Includere le quotazioni di tutte le tipologie per ogni zona")] bool includi_valori = false,
        [Description("Includere centro, bbox e area del perimetro di ogni zona")] bool includi_perimetri = false)
        => Run(() => omi.ZoneComune(comune, provincia, semestre, includi_valori, includi_perimetri));

    [McpServerTool(Name = "dettaglio_zona", ReadOnly = true, Idempotent = true),
     Description("Tutti i dati di una zona OMI in un semestre: anagrafica zona e comune, quotazioni di compravendita (€/m²) e locazione (€/m² mese) per tipologia e stato conservativo, dati del perimetro e, se richiesto, la geometria GeoJSON.")]
    public string DettaglioZona(
        [Description("Nome del comune, codice catastale o codice ISTAT")] string comune,
        [Description("Codice zona OMI (es. B1, C2, R1) oppure LinkZona (es. RM00000123)")] string zona,
        [Description("Sigla provincia, utile se il nome è ambiguo")] string? provincia = null,
        [Description("Semestre AAAA/S; se omesso l'ultimo disponibile")] string? semestre = null,
        [Description("Includere le coordinate del perimetro in formato GeoJSON MultiPolygon")] bool includi_geometria = false)
        => Run(() => omi.DettaglioZona(comune, zona, provincia, semestre, includi_geometria));

    [McpServerTool(Name = "zona_da_coordinate", ReadOnly = true, Idempotent = true),
     Description("Individua la zona OMI che contiene un punto (latitudine/longitudine WGS84) e restituisce comune, zona, quotazioni e perimetro. Se il punto non ricade in alcuna zona restituisce la più vicina entro 2 km.")]
    public string ZonaDaCoordinate(
        [Description("Latitudine in gradi decimali WGS84 (es. 41.9028)")] double latitudine,
        [Description("Longitudine in gradi decimali WGS84 (es. 12.4964)")] double longitudine,
        [Description("Semestre AAAA/S; se omesso l'ultimo disponibile")] string? semestre = null,
        [Description("Includere la geometria GeoJSON della zona")] bool includi_geometria = false)
        => Run(() => omi.ZonaDaCoordinate(latitudine, longitudine, semestre, includi_geometria));

    [McpServerTool(Name = "zone_vicine", ReadOnly = true, Idempotent = true),
     Description("Elenca le zone OMI entro un raggio (metri) da un punto, ordinate per distanza, con le quotazioni delle abitazioni civili.")]
    public string ZoneVicine(
        [Description("Latitudine WGS84")] double latitudine,
        [Description("Longitudine WGS84")] double longitudine,
        [Description("Raggio in metri (max 20000)")] double raggio_m = 1000,
        [Description("Semestre AAAA/S; se omesso l'ultimo disponibile")] string? semestre = null)
        => Run(() => omi.ZoneVicine(latitudine, longitudine, raggio_m, semestre));

    [McpServerTool(Name = "storico_zona", ReadOnly = true, Idempotent = true),
     Description("Serie storica delle quotazioni di una zona OMI su tutti i semestri caricati, per tipologia e stato conservativo.")]
    public string StoricoZona(
        [Description("Nome del comune, codice catastale o codice ISTAT")] string comune,
        [Description("Codice zona OMI (es. B1) o LinkZona")] string zona,
        [Description("Sigla provincia, utile se il nome è ambiguo")] string? provincia = null,
        [Description("Codice tipologia (es. 20 abitazioni civili); se omesso tutte")] int? cod_tipologia = null,
        [Description("Stato conservativo: OTTIMO, NORMALE o SCADENTE; se omesso tutti")] string? stato = null)
        => Run(() => omi.StoricoZona(comune, zona, provincia, cod_tipologia, stato));

    [McpServerTool(Name = "statistiche_comune", ReadOnly = true, Idempotent = true),
     Description("Riepilogo delle quotazioni di un comune in un semestre: minimo, massimo e medio per tipologia e medie delle abitazioni civili per fascia (centrale, semicentrale, periferica, ...).")]
    public string StatisticheComune(
        [Description("Nome del comune, codice catastale o codice ISTAT")] string comune,
        [Description("Sigla provincia, utile se il nome è ambiguo")] string? provincia = null,
        [Description("Semestre AAAA/S; se omesso l'ultimo disponibile")] string? semestre = null)
        => Run(() => omi.StatisticheComune(comune, provincia, semestre));
}
