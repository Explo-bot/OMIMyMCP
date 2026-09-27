// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

namespace OMIMyMCP;

/// <summary>
/// Attribuzione del progetto. In base alla licenza Apache-2.0 e al file NOTICE,
/// chi ridistribuisce il software deve conservare autore e link al progetto originale.
/// </summary>
public static class About
{
    public const string Nome = "OMIMyMCP";
    public const string Versione = "1.0.0";
    public const string Autore = "Explobot";
    public const string GitHubUrl = "https://github.com/Explo-bot/OMIMyMCP";
    public const string FonteDati = "Agenzia Entrate - OMI";

    public static string Attribuzione => $"{Nome} {Versione} di {Autore} - {GitHubUrl} (licenza Apache-2.0)";

    /// <summary>URL del progetto solo se valido (i segnaposto non lo sono).</summary>
    public static string? WebsiteUrl => Uri.TryCreate(GitHubUrl, UriKind.Absolute, out var u) ? u.ToString() : null;
}
