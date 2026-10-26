// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Multipoligoni in microgradi: codifica binaria, hash, bbox, centroide, area, appartenenza di un
//! punto, distanza dal bordo e GeoJSON.

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::error::{OmiError, Result};

/// Fattore di scala tra gradi e microgradi.
pub const S: f64 = 1_000_000.0;

/// Anello come vettore piatto `[x0, y0, x1, y1, …]` in microgradi.
pub type Ring = Vec<i32>;

/// Multipoligono: lista di poligoni, ognuno lista di anelli (il primo è il bordo esterno, gli altri buchi).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Geometry {
    pub polygons: Vec<Vec<Ring>>,
}

/// Arrotonda `x` a `decimali` cifre decimali, all'intero pari in caso di metà esatta.
pub fn round_half_even(x: f64, decimali: i32) -> f64 {
    let f = 10f64.powi(decimali);
    (x * f).round_ties_even() / f
}

/// SHA-256 del blob troncato ai primi 16 byte (hash di deduplicazione).
pub fn hash(blob: &[u8]) -> [u8; 16] {
    let digest = Sha256::digest(blob);
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(z: u64) -> i64 {
    (z >> 1) as i64 ^ -((z & 1) as i64)
}

fn write_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8 & 0x7f) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    fn varint(&mut self) -> Result<u64> {
        let mut value = 0u64;
        let mut shift = 0u32;
        loop {
            let Some(&b) = self.data.get(self.pos) else {
                return Err(OmiError::DatiNonValidi("geometria troncata".to_string()));
            };
            self.pos += 1;
            if shift == 63 && b > 1 {
                return Err(OmiError::DatiNonValidi(
                    "varint oltre i 64 bit nella geometria".to_string(),
                ));
            }
            value |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift > 63 {
                return Err(OmiError::DatiNonValidi(
                    "varint oltre i 64 bit nella geometria".to_string(),
                ));
            }
        }
    }

    /// Legge un conteggio; la capacità iniziale è limitata ai byte residui per non allocare troppo
    /// su un blob corrotto.
    fn count(&mut self) -> Result<(usize, usize)> {
        let n = usize::try_from(self.varint()?).map_err(|_| {
            OmiError::DatiNonValidi("conteggio non valido nella geometria".to_string())
        })?;
        Ok((n, n.min(self.remaining())))
    }

    fn coord(&mut self, prev: &mut i64) -> Result<i32> {
        let delta = unzigzag(self.varint()?);
        let v = prev.checked_add(delta).ok_or_else(|| {
            OmiError::DatiNonValidi("coordinata fuori intervallo nella geometria".to_string())
        })?;
        *prev = v;
        i32::try_from(v).map_err(|_| {
            OmiError::DatiNonValidi("coordinata fuori intervallo nella geometria".to_string())
        })
    }
}

impl Geometry {
    /// Codifica binaria: varint LEB128 per i conteggi, zigzag dei delta per le coordinate.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let (mut px, mut py) = (0i64, 0i64);
        write_varint(&mut out, self.polygons.len() as u64);
        for polygon in &self.polygons {
            write_varint(&mut out, polygon.len() as u64);
            for ring in polygon {
                write_varint(&mut out, (ring.len() / 2) as u64);
                for p in ring.as_chunks::<2>().0.iter() {
                    let (x, y) = (i64::from(p[0]), i64::from(p[1]));
                    write_varint(&mut out, zigzag(x - px));
                    write_varint(&mut out, zigzag(y - py));
                    px = x;
                    py = y;
                }
            }
        }
        out
    }

    /// Decodifica di un blob; errore (mai panic) su dati troncati o non validi.
    pub fn decode(blob: &[u8]) -> Result<Geometry> {
        let mut r = Reader { data: blob, pos: 0 };
        let (mut px, mut py) = (0i64, 0i64);
        let (n_polygons, cap) = r.count()?;
        let mut polygons = Vec::with_capacity(cap);
        for _ in 0..n_polygons {
            let (n_rings, cap) = r.count()?;
            let mut polygon = Vec::with_capacity(cap);
            for _ in 0..n_rings {
                let (n_points, cap) = r.count()?;
                let mut ring = Vec::with_capacity(cap.saturating_mul(2));
                for _ in 0..n_points {
                    ring.push(r.coord(&mut px)?);
                    ring.push(r.coord(&mut py)?);
                }
                polygon.push(ring);
            }
            polygons.push(polygon);
        }
        Ok(Geometry { polygons })
    }

    fn rings(&self) -> impl Iterator<Item = &Ring> {
        self.polygons.iter().flatten()
    }

    /// Numero totale dei punti di tutti gli anelli.
    pub fn n_punti(&self) -> usize {
        self.rings().map(|r| r.len() / 2).sum()
    }

    /// `(min_lon, min_lat, max_lon, max_lat)` in gradi; `(0, 0, 0, 0)` se la geometria non ha punti.
    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        let mut min = (i32::MAX, i32::MAX);
        let mut max = (i32::MIN, i32::MIN);
        let mut any = false;
        for p in self.rings().flat_map(|r| r.as_chunks::<2>().0.iter()) {
            any = true;
            min = (min.0.min(p[0]), min.1.min(p[1]));
            max = (max.0.max(p[0]), max.1.max(p[1]));
        }
        if !any {
            return (0.0, 0.0, 0.0, 0.0);
        }
        (
            f64::from(min.0) / S,
            f64::from(min.1) / S,
            f64::from(max.0) / S,
            f64::from(max.1) / S,
        )
    }

    /// `(centro_lon, centro_lat, area_km2)`, non arrotondati.
    pub fn centroid_and_area(&self) -> (f64, f64, f64) {
        let (mut a, mut cx, mut cy) = (0.0f64, 0.0f64, 0.0f64);
        for polygon in &self.polygons {
            for (k, ring) in polygon.iter().enumerate() {
                let n = ring.len() / 2;
                if n < 3 {
                    continue;
                }
                let x0 = f64::from(ring[0]) / S;
                let y0 = f64::from(ring[1]) / S;
                let (mut ra, mut rcx, mut rcy) = (0.0f64, 0.0f64, 0.0f64);
                for i in 0..n {
                    let j = (i + 1) % n;
                    let xi = f64::from(ring[2 * i]) / S - x0;
                    let yi = f64::from(ring[2 * i + 1]) / S - y0;
                    let xj = f64::from(ring[2 * j]) / S - x0;
                    let yj = f64::from(ring[2 * j + 1]) / S - y0;
                    let cr = xi * yj - xj * yi;
                    ra += cr;
                    rcx += (xi + xj) * cr;
                    rcy += (yi + yj) * cr;
                }
                ra /= 2.0;
                if ra.abs() < 1e-15 {
                    continue;
                }
                rcx = rcx / (6.0 * ra) + x0;
                rcy = rcy / (6.0 * ra) + y0;
                let sa = ra.abs() * if k == 0 { 1.0 } else { -1.0 };
                a += sa;
                cx += rcx * sa;
                cy += rcy * sa;
            }
        }
        if a.abs() < 1e-15 {
            let (min_lon, min_lat, max_lon, max_lat) = self.bbox();
            return ((min_lon + max_lon) / 2.0, (min_lat + max_lat) / 2.0, 0.0);
        }
        cx /= a;
        cy /= a;
        let km_lat = 111.32;
        let km_lon = 111.32 * (cy * std::f64::consts::PI / 180.0).cos();
        let area_km2 = a * km_lat * km_lon;
        (cx, cy, area_km2)
    }

    /// Vero se il punto ricade in almeno un poligono (regola pari-dispari, i buchi escludono).
    pub fn contains(&self, lon: f64, lat: f64) -> bool {
        let x = lon * S;
        let y = lat * S;
        for polygon in &self.polygons {
            let mut inside = false;
            for ring in polygon {
                let n = ring.len() / 2;
                if n == 0 {
                    continue;
                }
                let mut j = n - 1;
                for i in 0..n {
                    let xi = f64::from(ring[2 * i]);
                    let yi = f64::from(ring[2 * i + 1]);
                    let xj = f64::from(ring[2 * j]);
                    let yj = f64::from(ring[2 * j + 1]);
                    if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
                        inside = !inside;
                    }
                    j = i;
                }
            }
            if inside {
                return true;
            }
        }
        false
    }

    /// Distanza approssimata in metri dal punto al bordo più vicino (tutti gli anelli).
    pub fn distance_meters(&self, lon: f64, lat: f64) -> f64 {
        let kx = 111320.0 * (lat * std::f64::consts::PI / 180.0).cos();
        let ky = 110540.0;
        let mut best = f64::MAX;
        for ring in self.rings() {
            let n = ring.len() / 2;
            if n == 0 {
                continue;
            }
            let mut j = n - 1;
            for i in 0..n {
                let ax = (f64::from(ring[2 * j]) / S - lon) * kx;
                let ay = (f64::from(ring[2 * j + 1]) / S - lat) * ky;
                let bx = (f64::from(ring[2 * i]) / S - lon) * kx;
                let by = (f64::from(ring[2 * i + 1]) / S - lat) * ky;
                let dx = bx - ax;
                let dy = by - ay;
                let len = dx * dx + dy * dy;
                let t = if len > 0.0 {
                    (-(ax * dx + ay * dy) / len).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let px = ax + t * dx;
                let py = ay + t * dy;
                let d = px * px + py * py;
                best = best.min(d);
                j = i;
            }
        }
        best.sqrt()
    }

    /// `{"type":"MultiPolygon","coordinates":[…]}` con punti `[lon, lat]` in gradi.
    pub fn to_geojson(&self) -> Value {
        let coordinates: Vec<Value> = self
            .polygons
            .iter()
            .map(|polygon| {
                Value::Array(
                    polygon
                        .iter()
                        .map(|ring| {
                            Value::Array(
                                ring.as_chunks::<2>()
                                    .0
                                    .iter()
                                    .map(|p| {
                                        Value::Array(vec![
                                            Value::from(f64::from(p[0]) / S),
                                            Value::from(f64::from(p[1]) / S),
                                        ])
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                )
            })
            .collect();
        let mut m = Map::new();
        m.insert("type".to_string(), Value::from("MultiPolygon"));
        m.insert("coordinates".to_string(), Value::Array(coordinates));
        Value::Object(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anello rettangolare (in gradi) convertito in microgradi.
    fn rect(lon0: f64, lat0: f64, lon1: f64, lat1: f64) -> Ring {
        [(lon0, lat0), (lon1, lat0), (lon1, lat1), (lon0, lat1)]
            .iter()
            .flat_map(|&(x, y)| {
                [
                    round_half_even(x * S, 0) as i32,
                    round_half_even(y * S, 0) as i32,
                ]
            })
            .collect()
    }

    #[test]
    fn vettore_di_test() {
        let g = Geometry {
            polygons: vec![vec![vec![1, 2, 3, 5, 0, 0]]],
        };
        let blob = g.encode();
        assert_eq!(blob, [0x01, 0x01, 0x03, 0x02, 0x04, 0x04, 0x06, 0x05, 0x09]);
        assert_eq!(Geometry::decode(&blob).unwrap(), g);
    }

    #[test]
    fn andata_e_ritorno_multipoligono_con_buchi() {
        let g = Geometry {
            polygons: vec![
                vec![
                    vec![
                        12_496_400, 41_902_800, 12_498_150, 41_902_650, 12_499_020, 41_904_310,
                        12_497_005, 41_905_870, 12_495_310, 41_904_002,
                    ],
                    vec![
                        12_497_000, 41_903_500, 12_497_400, 41_903_500, 12_497_200, 41_903_900,
                    ],
                ],
                vec![vec![
                    12_401_000, 41_850_000, 12_402_000, 41_850_000, 12_401_500, 41_849_000,
                ]],
                vec![vec![-1_000, -2_000, 5, -7, 0, 0]],
            ],
        };
        let blob = g.encode();
        // La prima coordinata richiede un varint su più byte.
        assert!(blob.len() > 4 && blob[3] & 0x80 != 0);
        assert_eq!(Geometry::decode(&blob).unwrap(), g);
        assert_eq!(g.n_punti(), 14);
    }

    #[test]
    fn decode_blob_non_validi() {
        let g = Geometry {
            polygons: vec![vec![vec![
                12_496_400, 41_902_800, 12_496_500, 41_902_900, 1, 2,
            ]]],
        };
        let blob = g.encode();
        for len in 0..blob.len() {
            assert!(Geometry::decode(&blob[..len]).is_err(), "lunghezza {len}");
        }
        assert!(Geometry::decode(&[0xff; 11]).is_err());
        assert!(
            Geometry::decode(&[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02])
                .is_err()
        );
        // Conteggio enorme su un blob corto: errore, nessuna allocazione eccessiva.
        assert!(Geometry::decode(&[0xff, 0xff, 0xff, 0xff, 0x0f]).is_err());
        // Coordinata fuori dall'intervallo i32.
        assert!(Geometry::decode(&[0x01, 0x01, 0x01, 0x80, 0x80, 0x80, 0x80, 0x20, 0x00]).is_err());
    }

    #[test]
    fn hash_sedici_byte() {
        let blob = [0x01, 0x01, 0x03, 0x02, 0x04, 0x04, 0x06, 0x05, 0x09];
        let h = hash(&blob);
        let full = Sha256::digest(blob);
        assert_eq!(h[..], full[..16]);
    }

    #[test]
    fn contains_casi() {
        let g = Geometry {
            polygons: vec![
                vec![
                    rect(12.0, 41.0, 12.1, 41.1),
                    rect(12.04, 41.04, 12.06, 41.06),
                ],
                vec![rect(13.0, 42.0, 13.1, 42.1)],
            ],
        };
        assert!(g.contains(12.01, 41.01), "interno");
        assert!(!g.contains(11.9, 41.05), "esterno");
        assert!(!g.contains(12.05, 41.05), "dentro il buco");
        assert!(g.contains(13.05, 42.05), "secondo poligono");
    }

    #[test]
    fn distanza_da_segmento_orizzontale() {
        let g = Geometry {
            polygons: vec![vec![rect(12.0, 41.9, 12.01, 41.91)]],
        };
        // Il punto è 0,001° sotto il lato inferiore: distanza = 0,001 × 110 540 m.
        let d = g.distance_meters(12.005, 41.899);
        assert!((d - 110.54).abs() < 1e-6, "distanza {d}");
    }

    #[test]
    fn centroide_e_area_quadrato() {
        let g = Geometry {
            polygons: vec![vec![rect(10.0, 45.0, 10.01, 45.01)]],
        };
        let (cx, cy, area) = g.centroid_and_area();
        assert!((cx - 10.005).abs() < 1e-9);
        assert!((cy - 45.005).abs() < 1e-9);
        let attesa = 0.0001 * 111.32 * 111.32 * (45.005f64 * std::f64::consts::PI / 180.0).cos();
        assert!((area - attesa).abs() < 1e-9, "area {area} attesa {attesa}");
    }

    #[test]
    fn centroide_e_area_quadrato_con_buco() {
        let g = Geometry {
            polygons: vec![vec![
                rect(10.0, 45.0, 10.02, 45.02),
                rect(10.01, 45.01, 10.015, 45.015),
            ]],
        };
        let (cx, cy, area) = g.centroid_and_area();
        let (a_est, a_buco) = (0.0004, 0.000025);
        let a = a_est - a_buco;
        let cx_att = (10.01 * a_est - 10.0125 * a_buco) / a;
        let cy_att = (45.01 * a_est - 45.0125 * a_buco) / a;
        assert!((cx - cx_att).abs() < 1e-9, "cx {cx} atteso {cx_att}");
        assert!((cy - cy_att).abs() < 1e-9, "cy {cy} atteso {cy_att}");
        let attesa = a * 111.32 * 111.32 * (cy_att * std::f64::consts::PI / 180.0).cos();
        assert!((area - attesa).abs() < 1e-9, "area {area} attesa {attesa}");
    }

    #[test]
    fn centroide_geometria_degenere() {
        let g = Geometry {
            polygons: vec![vec![vec![10_000_000, 45_000_000, 10_020_000, 45_010_000]]],
        };
        let (cx, cy, area) = g.centroid_and_area();
        assert!((cx - 10.01).abs() < 1e-12 && (cy - 45.005).abs() < 1e-12);
        assert_eq!(area, 0.0);
    }

    #[test]
    fn bbox_e_geojson() {
        let g = Geometry {
            polygons: vec![vec![vec![1_500_000, 2_000_000, 3_000_000, 5_250_000, 0, 0]]],
        };
        assert_eq!(g.bbox(), (0.0, 0.0, 3.0, 5.25));
        assert_eq!(
            g.to_geojson().to_string(),
            r#"{"type":"MultiPolygon","coordinates":[[[[1.5,2.0],[3.0,5.25],[0.0,0.0]]]]}"#
        );
    }

    #[test]
    fn arrotondamento_pari() {
        assert_eq!(round_half_even(0.5, 0), 0.0);
        assert_eq!(round_half_even(1.5, 0), 2.0);
        assert_eq!(round_half_even(2.5, 0), 2.0);
        let z = round_half_even(-0.5, 0);
        assert_eq!(z, 0.0);
        assert!(z.is_sign_negative());
        assert_eq!(round_half_even(12.34567, 2), 12.35);
        assert_eq!(round_half_even(0.125, 2), 0.12);
    }
}
