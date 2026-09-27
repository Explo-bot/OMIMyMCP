// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

using System.Diagnostics;
using System.Globalization;
using System.Text.Json;
using ModelContextProtocol.Protocol;
using ModelContextProtocol.Server;

namespace OMIMyMCP;

/// <summary>
/// Log del server MCP (attivato dal parametro /l): richieste HTTP e chiamate agli strumenti,
/// scritte sulla console e in append nel file indicato.
/// </summary>
public sealed class ServerLog(string filePath)
{
    readonly Lock _lock = new();

    public string FilePath { get; } = filePath;

    public void Write(string message)
    {
        var line = $"{DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss.fff", CultureInfo.InvariantCulture)} {message}";
        lock (_lock)
        {
            Console.WriteLine(line);
            try { File.AppendAllText(FilePath, line + Environment.NewLine); }
            catch (IOException) { /* il log su file non deve mai bloccare il server */ }
        }
    }

    /// <summary>Middleware HTTP: IP del client, metodo, percorso, esito e durata.</summary>
    public async Task HttpMiddleware(HttpContext ctx, RequestDelegate next)
    {
        var sw = Stopwatch.StartNew();
        try
        {
            await next(ctx);
        }
        finally
        {
            Write($"HTTP {ctx.Connection.RemoteIpAddress} {ctx.Request.Method} {ctx.Request.Path} -> {ctx.Response.StatusCode} ({sw.ElapsedMilliseconds} ms)");
        }
    }

    /// <summary>Filtro MCP: nome dello strumento, argomenti, esito, dimensione della risposta e durata.</summary>
    public McpRequestHandler<CallToolRequestParams, CallToolResult> ToolFilter(McpRequestHandler<CallToolRequestParams, CallToolResult> next) =>
        async (ctx, ct) =>
        {
            var sw = Stopwatch.StartNew();
            var name = ctx.Params?.Name ?? "?";
            var args = ctx.Params?.Arguments is { Count: > 0 } a ? JsonSerializer.Serialize(a, OmiService.Json) : "{}";
            try
            {
                var result = await next(ctx, ct);
                var chars = result.Content?.OfType<TextContentBlock>().Sum(t => t.Text?.Length ?? 0) ?? 0;
                var esito = result.IsError == true
                    ? "ERRORE: " + string.Join(" ", result.Content?.OfType<TextContentBlock>().Select(t => t.Text) ?? [])
                    : $"ok, {chars} caratteri";
                Write($"TOOL {name} {args} -> {esito} ({sw.ElapsedMilliseconds} ms)");
                return result;
            }
            catch (Exception ex)
            {
                Write($"TOOL {name} {args} -> ERRORE: {ex.Message} ({sw.ElapsedMilliseconds} ms)");
                throw;
            }
        };
}
