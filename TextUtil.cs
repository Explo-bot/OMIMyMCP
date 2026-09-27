// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

using System.Globalization;
using System.Text;

namespace OMIMyMCP;

public static class TextUtil
{
    /// <summary>Normalizza un nome per la ricerca: maiuscolo, senza accenti/apostrofi, spazi singoli.</summary>
    public static string Norm(string? s)
    {
        if (string.IsNullOrEmpty(s)) return "";
        var sb = new StringBuilder(s.Length);
        bool space = false;
        foreach (var ch0 in s)
        {
            char ch = ch0 switch
            {
                'À' or 'Á' or 'Â' or 'Ä' or 'Ã' or 'à' or 'á' or 'â' or 'ä' or 'ã' => 'A',
                'È' or 'É' or 'Ê' or 'Ë' or 'è' or 'é' or 'ê' or 'ë' => 'E',
                'Ì' or 'Í' or 'Î' or 'Ï' or 'ì' or 'í' or 'î' or 'ï' => 'I',
                'Ò' or 'Ó' or 'Ô' or 'Ö' or 'Õ' or 'ò' or 'ó' or 'ô' or 'ö' or 'õ' => 'O',
                'Ù' or 'Ú' or 'Û' or 'Ü' or 'ù' or 'ú' or 'û' or 'ü' => 'U',
                'Ç' or 'ç' => 'C',
                'Ñ' or 'ñ' => 'N',
                var c => char.ToUpperInvariant(c)
            };
            if (ch is '\'' or '`' or '’' or '´' or '"') continue;
            if (char.IsWhiteSpace(ch) || ch is '-' or '/' or '.' or ',')
            {
                space = sb.Length > 0;
                continue;
            }
            if (space) { sb.Append(' '); space = false; }
            sb.Append(ch);
        }
        return sb.ToString();
    }

    public static double? ParseDecimal(string? s)
    {
        if (string.IsNullOrWhiteSpace(s)) return null;
        s = s.Trim().Replace(',', '.');
        return double.TryParse(s, NumberStyles.Float, CultureInfo.InvariantCulture, out var v) ? v : null;
    }

    public static int? ParseInt(string? s)
    {
        if (string.IsNullOrWhiteSpace(s)) return null;
        return int.TryParse(s.Trim(), NumberStyles.Integer, CultureInfo.InvariantCulture, out var v) ? v : null;
    }

    /// <summary>Pulisce un campo CSV OMI: trim, rimozione apici singoli esterni, vuoto -> null.</summary>
    public static string? Field(string s)
    {
        s = s.Trim();
        if (s.Length >= 2 && s[0] == '\'' && s[^1] == '\'') s = s[1..^1].Trim();
        return s.Length == 0 ? null : s;
    }
}
