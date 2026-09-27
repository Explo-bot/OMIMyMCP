// OMIMyMCP - Copyright 2026 Explobot - https://github.com/Explo-bot/OMIMyMCP
// SPDX-License-Identifier: Apache-2.0

namespace OMIMyMCP;

/// <summary>
/// Geometria di una zona OMI: multipoligono (lista di poligoni, ognuno lista di anelli;
/// il primo anello è il bordo esterno, gli altri sono buchi).
/// Le coordinate sono in microgradi WGS84 (lon/lat * 1e6), come nei KML OMI (6 decimali).
/// </summary>
public sealed class Geometry
{
    public const double Scale = 1_000_000.0;

    public List<List<int[]>> Polygons { get; } = new(); // ring = [x0,y0,x1,y1,...]

    public bool IsEmpty => Polygons.Count == 0;

    // ---------- Codifica binaria compatta (varint zigzag delta) ----------

    public byte[] Encode()
    {
        using var ms = new MemoryStream();
        WriteVar(ms, (ulong)Polygons.Count);
        long px = 0, py = 0;
        foreach (var poly in Polygons)
        {
            WriteVar(ms, (ulong)poly.Count);
            foreach (var ring in poly)
            {
                int n = ring.Length / 2;
                WriteVar(ms, (ulong)n);
                for (int i = 0; i < n; i++)
                {
                    long x = ring[2 * i], y = ring[2 * i + 1];
                    WriteVar(ms, ZigZag(x - px));
                    WriteVar(ms, ZigZag(y - py));
                    px = x; py = y;
                }
            }
        }
        return ms.ToArray();
    }

    public static Geometry Decode(byte[] data)
    {
        var g = new Geometry();
        int pos = 0;
        long px = 0, py = 0;
        int np = (int)ReadVar(data, ref pos);
        for (int p = 0; p < np; p++)
        {
            int nr = (int)ReadVar(data, ref pos);
            var poly = new List<int[]>(nr);
            for (int r = 0; r < nr; r++)
            {
                int n = (int)ReadVar(data, ref pos);
                var ring = new int[n * 2];
                for (int i = 0; i < n; i++)
                {
                    px += UnZigZag(ReadVar(data, ref pos));
                    py += UnZigZag(ReadVar(data, ref pos));
                    ring[2 * i] = (int)px;
                    ring[2 * i + 1] = (int)py;
                }
                poly.Add(ring);
            }
            g.Polygons.Add(poly);
        }
        return g;
    }

    static ulong ZigZag(long v) => (ulong)((v << 1) ^ (v >> 63));
    static long UnZigZag(ulong v) => (long)(v >> 1) ^ -(long)(v & 1);

    static void WriteVar(Stream s, ulong v)
    {
        while (v >= 0x80) { s.WriteByte((byte)(v | 0x80)); v >>= 7; }
        s.WriteByte((byte)v);
    }

    static ulong ReadVar(byte[] d, ref int pos)
    {
        ulong r = 0; int shift = 0;
        while (true)
        {
            byte b = d[pos++];
            r |= (ulong)(b & 0x7F) << shift;
            if (b < 0x80) return r;
            shift += 7;
        }
    }

    // ---------- Misure ----------

    public (double MinX, double MinY, double MaxX, double MaxY) BBox()
    {
        int minx = int.MaxValue, miny = int.MaxValue, maxx = int.MinValue, maxy = int.MinValue;
        foreach (var poly in Polygons)
            foreach (var ring in poly)
                for (int i = 0; i < ring.Length; i += 2)
                {
                    if (ring[i] < minx) minx = ring[i];
                    if (ring[i] > maxx) maxx = ring[i];
                    if (ring[i + 1] < miny) miny = ring[i + 1];
                    if (ring[i + 1] > maxy) maxy = ring[i + 1];
                }
        return (minx / Scale, miny / Scale, maxx / Scale, maxy / Scale);
    }

    /// <summary>Centroide (media pesata sulle aree degli anelli, buchi sottratti) e area in km².</summary>
    public (double Lon, double Lat, double AreaKm2) CentroidAndArea()
    {
        double a = 0, cx = 0, cy = 0;
        foreach (var poly in Polygons)
            for (int r = 0; r < poly.Count; r++)
            {
                var ring = poly[r];
                double ra = 0, rcx = 0, rcy = 0;
                int n = ring.Length / 2;
                if (n < 3) continue;
                double x0 = ring[0] / Scale, y0 = ring[1] / Scale;
                for (int i = 0; i < n; i++)
                {
                    int j = (i + 1) % n;
                    double xi = ring[2 * i] / Scale - x0, yi = ring[2 * i + 1] / Scale - y0;
                    double xj = ring[2 * j] / Scale - x0, yj = ring[2 * j + 1] / Scale - y0;
                    double cr = xi * yj - xj * yi;
                    ra += cr; rcx += (xi + xj) * cr; rcy += (yi + yj) * cr;
                }
                ra /= 2;
                if (Math.Abs(ra) < 1e-15) continue;
                rcx = rcx / (6 * ra) + x0; rcy = rcy / (6 * ra) + y0;
                double sa = Math.Abs(ra) * (r == 0 ? 1 : -1);
                a += sa; cx += rcx * sa; cy += rcy * sa;
            }
        if (Math.Abs(a) < 1e-15)
        {
            var b = BBox();
            return ((b.MinX + b.MaxX) / 2, (b.MinY + b.MaxY) / 2, 0);
        }
        cx /= a; cy /= a;
        double kmPerDegLat = 111.32, kmPerDegLon = 111.32 * Math.Cos(cy * Math.PI / 180);
        return (cx, cy, a * kmPerDegLat * kmPerDegLon);
    }

    /// <summary>Punto nel multipoligono (regola pari/dispari su tutti gli anelli di ciascun poligono).</summary>
    public bool Contains(double lon, double lat)
    {
        double x = lon * Scale, y = lat * Scale;
        foreach (var poly in Polygons)
        {
            bool inside = false;
            foreach (var ring in poly)
            {
                int n = ring.Length / 2;
                for (int i = 0, j = n - 1; i < n; j = i++)
                {
                    double xi = ring[2 * i], yi = ring[2 * i + 1], xj = ring[2 * j], yj = ring[2 * j + 1];
                    if ((yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi)
                        inside = !inside;
                }
            }
            if (inside) return true;
        }
        return false;
    }

    /// <summary>Distanza approssimata (metri) dal punto al bordo più vicino.</summary>
    public double DistanceMeters(double lon, double lat)
    {
        double kx = 111320 * Math.Cos(lat * Math.PI / 180), ky = 110540;
        double best = double.MaxValue;
        foreach (var poly in Polygons)
            foreach (var ring in poly)
            {
                int n = ring.Length / 2;
                for (int i = 0, j = n - 1; i < n; j = i++)
                {
                    double ax = (ring[2 * j] / Scale - lon) * kx, ay = (ring[2 * j + 1] / Scale - lat) * ky;
                    double bx = (ring[2 * i] / Scale - lon) * kx, by = (ring[2 * i + 1] / Scale - lat) * ky;
                    double dx = bx - ax, dy = by - ay, len = dx * dx + dy * dy;
                    double t = len > 0 ? Math.Clamp(-(ax * dx + ay * dy) / len, 0, 1) : 0;
                    double px = ax + t * dx, py = ay + t * dy;
                    double d = px * px + py * py;
                    if (d < best) best = d;
                }
            }
        return Math.Sqrt(best);
    }

    /// <summary>Coordinate GeoJSON (MultiPolygon): [[[[lon,lat],...]]].</summary>
    public object ToGeoJson()
    {
        var coords = Polygons.Select(p => p.Select(r =>
        {
            var pts = new double[r.Length / 2][];
            for (int i = 0; i < pts.Length; i++) pts[i] = [r[2 * i] / Scale, r[2 * i + 1] / Scale];
            return pts;
        }).ToArray()).ToArray();
        return new { type = "MultiPolygon", coordinates = coords };
    }
}
