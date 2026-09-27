// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

using System.Globalization;
using Microsoft.AspNetCore.Hosting.Server;
using Microsoft.AspNetCore.Hosting.Server.Features;
using OMIMyMCP;

// Uso: OMIMyMCP [QI*.zip ...] [database.db] [porta] [/l]
//  - QI*.zip : fornitura OMI da caricare (sostituisce i dati dello stesso semestre)
//  - *.db    : database SQLite (default omi.db)
//  - porta   : 0..65535, avvia il server MCP (HTTP streamable su /mcp) al termine
//  - /l      : abilita il log del server (console + file <database>.log); accetta anche -l e --log
//  Senza parametri carica tutti i QI*.zip della cartella corrente.

var zips = new List<string>();
string dbPath = "omi.db";
int? port = null;
bool log = false;

foreach (var a in args)
{
    if (a is "/l" or "/L" or "-l" or "-L" || a.Equals("--log", StringComparison.OrdinalIgnoreCase))
        log = true;
    else if (a.EndsWith(".zip", StringComparison.OrdinalIgnoreCase))
    {
        if (!Path.GetFileName(a).StartsWith("QI", StringComparison.OrdinalIgnoreCase))
            return Fail($"File non riconosciuto (atteso QI*.zip): {a}");
        if (!File.Exists(a)) return Fail($"File non trovato: {a}");
        zips.Add(a);
    }
    else if (a.EndsWith(".db", StringComparison.OrdinalIgnoreCase))
        dbPath = a;
    else if (int.TryParse(a, NumberStyles.None, CultureInfo.InvariantCulture, out var p) && p is >= 0 and <= 65535)
        port = p;
    else
        return Fail($"Parametro non valido: {a}\nUso: OMIMyMCP [QI*.zip] [database.db] [porta 0-65535] [/l]");
}

if (args.Length == 0)
    zips.AddRange(Directory.GetFiles(Directory.GetCurrentDirectory(), "QI*.zip").Order(StringComparer.OrdinalIgnoreCase));

dbPath = Path.GetFullPath(dbPath);
Console.WriteLine(About.Attribuzione);
Console.WriteLine($"Fonte dei dati: {About.FonteDati}");
Console.WriteLine($"Database: {dbPath}");

using (var db = Database.Open(dbPath))
{
    Database.EnsureSchema(db);
    if (zips.Count > 0)
    {
        var imp = new Importer(db);
        int ok = 0;
        foreach (var z in zips)
        {
            try { imp.ImportZip(z); ok++; }
            catch (Exception ex) { Console.Error.WriteLine($"  ERRORE su {Path.GetFileName(z)}: {ex.Message}"); }
        }
        imp.Completa();
        Console.WriteLine($"Caricati {ok}/{zips.Count} file.");
    }
    else if (args.Length > 0)
        Console.WriteLine("Nessun file QI*.zip indicato: nessun caricamento.");
    Database.PrintSummary(db);
}

if (port == null)
{
    if (log) Console.WriteLine("Il parametro /l ha effetto solo con il server MCP (indicare una porta).");
    return 0;
}

var serverLog = log ? new ServerLog(Path.ChangeExtension(dbPath, ".log")) : null;

var builder = WebApplication.CreateSlimBuilder();
builder.Logging.SetMinimumLevel(LogLevel.Warning);
builder.WebHost.ConfigureKestrel(o => o.ListenAnyIP(port.Value));
builder.Services.AddSingleton(new OmiService(dbPath));
var mcp = builder.Services.AddMcpServer(o =>
    {
        o.ServerInfo = new()
        {
            Name = About.Nome,
            Title = "OMIMyMCP - Quotazioni immobiliari OMI",
            Version = About.Versione,
            Description = About.Attribuzione,
            WebsiteUrl = About.WebsiteUrl,
        };
        o.ServerInstructions = "Server MCP sulle quotazioni immobiliari OMI (Agenzia delle Entrate): zone OMI per comune, " +
                               "ricerca per coordinate, quotazioni di compravendita e locazione per semestre, serie storiche. " +
                               $"Quando presenti risultati basati su questi dati cita la fonte \"{About.FonteDati}\". " +
                               $"Software: {About.Attribuzione}.";
    })
    .WithHttpTransport(o => o.Stateless = true)
    .WithTools<OmiTools>();
if (serverLog != null)
    mcp.WithRequestFilters(f => f.AddCallToolFilter(serverLog.ToolFilter));

var app = builder.Build();
if (serverLog != null) app.Use(serverLog.HttpMiddleware);
app.MapMcp("/mcp");
app.MapGet("/", () => Results.Text($"{About.Attribuzione}\nServer MCP - endpoint: /mcp\nFonte dei dati: {About.FonteDati}\n"));
await app.StartAsync();
var addrs = app.Services.GetRequiredService<IServer>().Features.Get<IServerAddressesFeature>()?.Addresses ?? [];
foreach (var a in addrs) Console.WriteLine($"Server MCP in ascolto su {a.Replace("[::]", "localhost").Replace("0.0.0.0", "localhost")}/mcp");
if (serverLog != null)
    serverLog.Write($"Log attivo, file: {serverLog.FilePath}");
Console.WriteLine("Ctrl+C per terminare.");
await app.WaitForShutdownAsync();
return 0;

static int Fail(string msg)
{
    Console.Error.WriteLine(msg);
    return 1;
}
