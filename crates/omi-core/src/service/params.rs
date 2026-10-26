// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Parametri degli strumenti. Le descrizioni finiscono nello schema JSON di input;
//! numeri e booleani si accettano anche in forma di stringa (`"20"`, `"true"`).

use schemars::JsonSchema;
use serde::Deserialize;

const DESCR_COMUNE: &str = "Nome del comune, codice catastale o codice ISTAT";
const DESCR_PROVINCIA: &str = "Sigla provincia, utile se il nome è ambiguo";
const DESCR_SEMESTRE: &str = "Semestre AAAA/S; se omesso l'ultimo disponibile";

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct CercaComuni {
    #[schemars(description = DESCR_COMUNE)]
    pub testo: String,
    #[serde(default)]
    #[schemars(
        with = "String",
        description = "Sigla della provincia per filtrare (es. RM), facoltativa"
    )]
    pub provincia: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ZoneComune {
    #[schemars(description = DESCR_COMUNE)]
    pub comune: String,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_PROVINCIA)]
    pub provincia: Option<String>,
    #[serde(default)]
    #[schemars(
        with = "String",
        description = "Semestre nel formato AAAA/S (es. 2025/1); se omesso l'ultimo disponibile"
    )]
    pub semestre: Option<String>,
    #[serde(default, deserialize_with = "de::bool")]
    #[schemars(description = "Includere le quotazioni di tutte le tipologie per ogni zona")]
    pub includi_valori: bool,
    #[serde(default, deserialize_with = "de::bool")]
    #[schemars(description = "Includere centro, bbox e area del perimetro di ogni zona")]
    pub includi_perimetri: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct DettaglioZona {
    #[schemars(description = DESCR_COMUNE)]
    pub comune: String,
    #[schemars(description = "Codice zona OMI (es. B1, C2, R1) oppure LinkZona (es. RM00000123)")]
    pub zona: String,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_PROVINCIA)]
    pub provincia: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_SEMESTRE)]
    pub semestre: Option<String>,
    #[serde(default, deserialize_with = "de::bool")]
    #[schemars(
        description = "Includere le coordinate del perimetro in formato GeoJSON MultiPolygon"
    )]
    pub includi_geometria: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ZonaDaCoordinate {
    #[serde(deserialize_with = "de::f64")]
    #[schemars(description = "Latitudine in gradi decimali WGS84 (es. 41.9028)")]
    pub latitudine: f64,
    #[serde(deserialize_with = "de::f64")]
    #[schemars(description = "Longitudine in gradi decimali WGS84 (es. 12.4964)")]
    pub longitudine: f64,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_SEMESTRE)]
    pub semestre: Option<String>,
    #[serde(default, deserialize_with = "de::bool")]
    #[schemars(description = "Includere la geometria GeoJSON della zona")]
    pub includi_geometria: bool,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct ZoneVicine {
    #[serde(deserialize_with = "de::f64")]
    #[schemars(description = "Latitudine WGS84")]
    pub latitudine: f64,
    #[serde(deserialize_with = "de::f64")]
    #[schemars(description = "Longitudine WGS84")]
    pub longitudine: f64,
    #[serde(default, deserialize_with = "de::opt_f64")]
    #[schemars(
        with = "f64",
        description = "Raggio in metri (max 20000)",
        extend("default" = 1000)
    )]
    pub raggio_m: Option<f64>,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_SEMESTRE)]
    pub semestre: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct StoricoZona {
    #[schemars(description = DESCR_COMUNE)]
    pub comune: String,
    #[schemars(description = "Codice zona OMI (es. B1) o LinkZona")]
    pub zona: String,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_PROVINCIA)]
    pub provincia: Option<String>,
    #[serde(default, deserialize_with = "de::opt_i64")]
    #[schemars(
        with = "i64",
        description = "Codice tipologia (es. 20 abitazioni civili); se omesso tutte"
    )]
    pub cod_tipologia: Option<i64>,
    #[serde(default)]
    #[schemars(
        with = "String",
        description = "Stato conservativo: OTTIMO, NORMALE o SCADENTE; se omesso tutti"
    )]
    pub stato: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
pub struct StatisticheComune {
    #[schemars(description = DESCR_COMUNE)]
    pub comune: String,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_PROVINCIA)]
    pub provincia: Option<String>,
    #[serde(default)]
    #[schemars(with = "String", description = DESCR_SEMESTRE)]
    pub semestre: Option<String>,
}

/// Deserializzatori tolleranti: un numero o un booleano si accetta anche come stringa (con spazi
/// attorno); `null` vale come parametro assente.
mod de {
    use serde::de::Error;
    use serde::{Deserialize, Deserializer};
    use serde_json::Value;

    fn number(v: &Value) -> Option<f64> {
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse::<f64>().ok().filter(|x| x.is_finite()),
            _ => None,
        }
    }

    fn integer(v: &Value) -> Option<i64> {
        match v {
            // Un reale con parte decimale nulla (`20.0`) vale come intero.
            Value::Number(n) => n.as_i64().or_else(|| {
                n.as_f64()
                    .filter(|f| f.fract() == 0.0 && f.abs() < 9e15)
                    .map(|f| f as i64)
            }),
            Value::String(s) => s.trim().parse::<i64>().ok(),
            _ => None,
        }
    }

    pub fn f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        let v = Value::deserialize(d)?;
        number(&v).ok_or_else(|| D::Error::custom(format!("atteso un numero, ricevuto {v}")))
    }

    pub fn opt_f64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        match Value::deserialize(d)? {
            Value::Null => Ok(None),
            v => number(&v)
                .map(Some)
                .ok_or_else(|| D::Error::custom(format!("atteso un numero, ricevuto {v}"))),
        }
    }

    pub fn opt_i64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
        match Value::deserialize(d)? {
            Value::Null => Ok(None),
            v => integer(&v)
                .map(Some)
                .ok_or_else(|| D::Error::custom(format!("atteso un intero, ricevuto {v}"))),
        }
    }

    pub fn bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
        match Value::deserialize(d)? {
            Value::Null => Ok(false),
            Value::Bool(b) => Ok(b),
            Value::String(s) if s.trim().eq_ignore_ascii_case("true") => Ok(true),
            Value::String(s) if s.trim().eq_ignore_ascii_case("false") => Ok(false),
            v => Err(D::Error::custom(format!(
                "atteso un booleano, ricevuto {v}"
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeri_e_booleani_come_stringa() {
        let p: ZoneVicine = serde_json::from_value(
            json!({"latitudine": "41.9", "longitudine": 12.5, "raggio_m": " 1500 "}),
        )
        .unwrap();
        assert_eq!(
            (p.latitudine, p.longitudine, p.raggio_m),
            (41.9, 12.5, Some(1500.0))
        );

        let p: ZoneComune = serde_json::from_value(
            json!({"comune": "Roma", "includi_valori": "true", "includi_perimetri": "FALSE"}),
        )
        .unwrap();
        assert!(p.includi_valori && !p.includi_perimetri);

        let p: StoricoZona =
            serde_json::from_value(json!({"comune": "Roma", "zona": "B1", "cod_tipologia": "20"}))
                .unwrap();
        assert_eq!(p.cod_tipologia, Some(20));
    }

    #[test]
    fn null_e_assenti() {
        let p: ZoneVicine = serde_json::from_value(
            json!({"latitudine": 41.9, "longitudine": 12.5, "raggio_m": null, "semestre": null}),
        )
        .unwrap();
        assert_eq!((p.raggio_m, p.semestre), (None, None));

        let p: DettaglioZona = serde_json::from_value(
            json!({"comune": "Roma", "zona": "B1", "includi_geometria": null}),
        )
        .unwrap();
        assert!(!p.includi_geometria && p.provincia.is_none());

        let p: StoricoZona =
            serde_json::from_value(json!({"comune": "Roma", "zona": "B1", "cod_tipologia": null}))
                .unwrap();
        assert_eq!(p.cod_tipologia, None);
    }

    #[test]
    fn valori_non_validi() {
        for v in [
            json!({"latitudine": "abc", "longitudine": 12.5}),
            json!({"latitudine": true, "longitudine": 12.5}),
            json!({"latitudine": "NaN", "longitudine": 12.5}),
            json!({"longitudine": 12.5}),
        ] {
            assert!(serde_json::from_value::<ZonaDaCoordinate>(v).is_err());
        }
        assert!(
            serde_json::from_value::<ZoneComune>(json!({"comune": "Roma", "includi_valori": "si"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<StoricoZona>(
                json!({"comune": "Roma", "zona": "B1", "cod_tipologia": "20.5"})
            )
            .is_err()
        );
    }
}
