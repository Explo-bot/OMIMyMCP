// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Pulizia dei campi CSV, numeri, normalizzazione dei nomi e nome ripulito dei file
//!.

use std::sync::LazyLock;

use regex::Regex;

/// `_` + codice fiscale immediatamente prima del `.zip` finale.
static RE_CODICE_FISCALE: LazyLock<Regex> = LazyLock::new(|| {
    // Invariante: il pattern è costante e valido (verificato dai test di `safe_file_name`).
    Regex::new(r"(?i)_[A-Z]{6}\d{2}[A-Z]\d{2}[A-Z]\d{3}[A-Z](\.zip)$")
        .expect("espressione regolare costante valida")
});

/// Normalizza un nome per la ricerca: maiuscolo, senza accenti né apostrofi, separatori ridotti
/// a un solo spazio.
pub fn norm(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending_sep = false;
    for c in s.chars() {
        if matches!(c, '\'' | '`' | '’' | '´' | '"') {
            continue;
        }
        if c.is_whitespace() || matches!(c, '-' | '/' | '.' | ',') {
            pending_sep = true;
            continue;
        }
        if pending_sep && !out.is_empty() {
            out.push(' ');
        }
        pending_sep = false;
        push_folded(&mut out, c);
    }
    out
}

fn push_folded(out: &mut String, c: char) {
    let r = match c {
        'À' | 'Á' | 'Â' | 'Ä' | 'Ã' | 'à' | 'á' | 'â' | 'ä' | 'ã' => 'A',
        'È' | 'É' | 'Ê' | 'Ë' | 'è' | 'é' | 'ê' | 'ë' => 'E',
        'Ì' | 'Í' | 'Î' | 'Ï' | 'ì' | 'í' | 'î' | 'ï' => 'I',
        'Ò' | 'Ó' | 'Ô' | 'Ö' | 'Õ' | 'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'O',
        'Ù' | 'Ú' | 'Û' | 'Ü' | 'ù' | 'ú' | 'û' | 'ü' => 'U',
        'Ç' | 'ç' => 'C',
        'Ñ' | 'ñ' => 'N',
        _ => {
            let mut up = c.to_uppercase();
            match (up.next(), up.next()) {
                (Some(u), None) => u,
                _ => c,
            }
        }
    };
    out.push(r);
}

/// Pulizia di un campo CSV: trim, rimozione degli apici esterni, stringa vuota → `None`.
pub fn field(raw: &str) -> Option<String> {
    let mut s = raw.trim();
    if s.chars().count() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        s = s[1..s.len() - 1].trim();
    }
    if s.is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Numero decimale con virgola o punto; non valido o non finito → `None`.
///
/// Il separatore delle migliaia non è previsto: `"1.234,5"` diventa `"1.234.5"` e restituisce `None`.
pub fn parse_decimal(raw: &str) -> Option<f64> {
    raw.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
}

/// Intero con segno facoltativo; non valido → `None`.
pub fn parse_int(raw: &str) -> Option<i64> {
    raw.trim().parse::<i64>().ok()
}

/// Rimuove `_` + codice fiscale prima del `.zip` finale: `QIP1438117_RSSMRA80A01H501U.zip` → `QIP1438117.zip`.
pub fn safe_file_name(name: &str) -> String {
    RE_CODICE_FISCALE.replace(name, "$1").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn norm_casi_noti() {
        assert_eq!(norm("Sant'Antonio di Gallura"), "SANTANTONIO DI GALLURA");
        assert_eq!(norm("Forlì-Cesena"), "FORLI CESENA");
        assert_eq!(norm("  Reggio   nell'Emilia "), "REGGIO NELLEMILIA");
        assert_eq!(norm("L'Aquila"), "LAQUILA");
    }

    #[test]
    fn norm_casi_limite() {
        assert_eq!(norm(""), "");
        assert_eq!(norm(" - / . , "), "");
        assert_eq!(norm("straße"), "STRAßE");
        assert_eq!(norm("Città\u{00A0}di Castello"), "CITTA DI CASTELLO");
    }

    #[test]
    fn safe_file_name_rimuove_codice_fiscale() {
        assert_eq!(
            safe_file_name("QIP1438117_RSSMRA80A01H501U.zip"),
            "QIP1438117.zip"
        );
        assert_eq!(
            safe_file_name("qip1438117_rssmra80a01h501u.zip"),
            "qip1438117.zip"
        );
        assert_eq!(
            safe_file_name("QI1438117_RSSMRA80A01H501U.ZIP"),
            "QI1438117.ZIP"
        );
        assert_eq!(safe_file_name("QIP1438117.zip"), "QIP1438117.zip");
        assert_eq!(safe_file_name("QI_ABC.zip"), "QI_ABC.zip");
    }

    #[test]
    fn field_pulizia() {
        assert_eq!(field(" 'Centro' "), Some("Centro".to_string()));
        assert_eq!(field("''"), None);
        assert_eq!(field("' '"), None);
        assert_eq!(field("'"), Some("'".to_string()));
        assert_eq!(field("   "), None);
        assert_eq!(field("B1"), Some("B1".to_string()));
    }

    #[test]
    fn parse_decimal_formati() {
        assert_eq!(parse_decimal("1234,5"), Some(1234.5));
        assert_eq!(parse_decimal(" 1500 "), Some(1500.0));
        assert_eq!(parse_decimal("2.75"), Some(2.75));
        // Il separatore delle migliaia non fa parte del formato dei file OMI: il valore è NULL.
        assert_eq!(parse_decimal("1.234,5"), None);
        assert_eq!(parse_decimal(""), None);
        assert_eq!(parse_decimal("abc"), None);
        assert_eq!(parse_decimal("NaN"), None);
        assert_eq!(parse_decimal("inf"), None);
    }

    #[test]
    fn parse_int_formati() {
        assert_eq!(parse_int(" 20 "), Some(20));
        assert_eq!(parse_int("+7"), Some(7));
        assert_eq!(parse_int("-3"), Some(-3));
        assert_eq!(parse_int("2,0"), None);
        assert_eq!(parse_int(""), None);
    }
}
