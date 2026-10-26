// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Identità del programma.

pub const NOME: &str = "OMIMyMCP";
pub const VERSIONE: &str = env!("CARGO_PKG_VERSION");
pub const AUTORE: &str = "Explobot";
/// URL del progetto, impostato a compilazione con la variabile d'ambiente `OMI_URL_PROGETTO`.
pub const URL_PROGETTO: Option<&str> = option_env!("OMI_URL_PROGETTO");
pub const FONTE_DATI: &str = "Agenzia Entrate - OMI";

/// `"{NOME} {VERSIONE} di {AUTORE}[ - {URL_PROGETTO}] (licenza Apache-2.0)"`.
pub fn attribuzione() -> String {
    let mut s = format!("{NOME} {VERSIONE} di {AUTORE}");
    if let Some(url) = URL_PROGETTO {
        s.push_str(" - ");
        s.push_str(url);
    }
    s.push_str(" (licenza Apache-2.0)");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribuzione_formato() {
        let a = attribuzione();
        assert!(a.starts_with("OMIMyMCP 1.0.0 di Explobot"));
        assert!(a.ends_with(" (licenza Apache-2.0)"));
        if URL_PROGETTO.is_none() {
            assert_eq!(a, "OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)");
        }
    }
}
