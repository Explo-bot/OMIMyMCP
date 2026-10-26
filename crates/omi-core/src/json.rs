// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Serializzazione JSON dei risultati degli strumenti.
//!
//! L'ordine delle chiavi è quello di inserimento (`serde_json` con `preserve_order`). serde_json non
//! applica escape ai caratteri non ASCII e lo applica solo a `"`, `\` e ai caratteri di controllo; il
//! formatter ridefinisce soltanto la scrittura dei reali, che serde_json produrrebbe con un algoritmo
//! diverso da ryu (`1e+21` invece di `1e21`).

use std::io;

use rusqlite::types::ValueRef;
use serde::Serialize;
use serde_json::Value;
use serde_json::ser::{CompactFormatter, Formatter};

use crate::error::{OmiError, Result};

/// 2^53: sotto questa soglia un reale con parte decimale nulla si scrive come intero.
const LIMITE_INTERI: f64 = 9_007_199_254_740_992.0;

/// Formatter compatto: `1500.0` → `1500`, gli altri reali con la rappresentazione più breve (ryu).
struct OmiFormatter;

impl Formatter for OmiFormatter {
    fn write_f64<W: ?Sized + io::Write>(&mut self, writer: &mut W, value: f64) -> io::Result<()> {
        if value.fract() == 0.0 && value.abs() < LIMITE_INTERI {
            if value == 0.0 && value.is_sign_negative() {
                return writer.write_all(b"-0");
            }
            // |value| < 2^53 e parte decimale nulla: la conversione in i64 è esatta.
            write!(writer, "{}", value as i64)
        } else if value.is_finite() {
            // ryu, come richiesto dal formato (serde_json usa un altro algoritmo: `1e+21`).
            writer.write_all(ryu::Buffer::new().format_finite(value).as_bytes())
        } else {
            CompactFormatter.write_f64(writer, value)
        }
    }
}

/// JSON compatto (le proprietà `null` vanno tolte prima con
/// [`strip_nulls`]).
pub fn to_string(value: &Value) -> Result<String> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, OmiFormatter);
    value
        .serialize(&mut ser)
        .map_err(|e| OmiError::DatiNonValidi(format!("serializzazione JSON: {e}")))?;
    String::from_utf8(buf).map_err(|e| OmiError::DatiNonValidi(format!("JSON non UTF-8: {e}")))
}

/// Toglie a tutti i livelli le proprietà con valore `null` (gli elementi degli array restano).
pub fn strip_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, v)| !v.is_null())
                .map(|(k, v)| (k, strip_nulls(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(strip_nulls).collect()),
        v => v,
    }
}

/// Valore di una colonna SQLite: gli INTEGER restano interi, i REAL diventano reali; i BLOB non
/// compaiono mai nei risultati e valgono `null`.
pub fn sql_value(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null | ValueRef::Blob(_) => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn s(v: Value) -> String {
        to_string(&v).unwrap()
    }

    #[test]
    fn numeri() {
        assert_eq!(s(json!(1500.0)), "1500");
        assert_eq!(s(json!(12.5)), "12.5");
        assert_eq!(s(json!(0.1)), "0.1");
        assert_eq!(s(json!(-0.0)), "-0");
        assert_eq!(s(json!(0.0)), "0");
        assert_eq!(s(json!(1e21)), "1e21");
        assert_eq!(s(json!(1.5e-7)), "1.5e-7");
        assert_eq!(s(json!(123456.789)), "123456.789");
        assert_eq!(s(json!(9_007_199_254_740_991.0)), "9007199254740991");
        assert_eq!(s(json!(9_007_199_254_740_992.0)), "9007199254740992.0");
        assert_eq!(s(json!(-3500.0)), "-3500");
        assert_eq!(s(json!(41.9028)), "41.9028");
        // Gli interi restano interi.
        assert_eq!(s(json!(20)), "20");
        assert_eq!(s(json!(-7)), "-7");
        // Un reale non finito non è rappresentabile: serde_json lo trasforma in null.
        assert_eq!(s(json!(f64::NAN)), "null");
    }

    #[test]
    fn omissione_dei_null_a_tutti_i_livelli() {
        let v = json!({"a": 1, "b": null, "c": {"d": null, "e": [ {"f": null, "g": 2.0}, null ]}});
        assert_eq!(s(strip_nulls(v)), r#"{"a":1,"c":{"e":[{"g":2},null]}}"#);
    }

    #[test]
    fn eccezione_elenco_tipologie() {
        // Le righe di elenco_tipologie non passano da strip_nulls e mantengono il null.
        let v = json!([{"codice": 1, "descrizione": null}]);
        assert_eq!(s(v), r#"[{"codice":1,"descrizione":null}]"#);
    }

    #[test]
    fn escape_e_ordine_delle_chiavi() {
        let v = json!({"z": "città €/m²", "a": "virgolette \" e \\ e\ttab\n", "m": "\u{1}"});
        assert_eq!(
            s(v),
            "{\"z\":\"città €/m²\",\"a\":\"virgolette \\\" e \\\\ e\\ttab\\n\",\"m\":\"\\u0001\"}"
        );
    }

    #[test]
    fn valori_sqlite() {
        assert_eq!(sql_value(ValueRef::Integer(5)), json!(5));
        assert_eq!(s(sql_value(ValueRef::Real(3500.0))), "3500");
        assert_eq!(sql_value(ValueRef::Text(b"RM")), json!("RM"));
        assert_eq!(sql_value(ValueRef::Null), Value::Null);
        assert_eq!(sql_value(ValueRef::Blob(&[1, 2])), Value::Null);
    }
}
