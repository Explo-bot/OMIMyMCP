// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Errori della libreria.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OmiError {
    /// Errore di validazione: il messaggio va mostrato così com'è al client MCP.
    #[error("{0}")]
    Validazione(String),
    /// Dati in ingresso o nel database non validi (es. blob di geometria troncato).
    #[error("dati non validi: {0}")]
    DatiNonValidi(String),
    /// Errore dell'importazione (formato delle forniture): il messaggio è già completo.
    #[error("{0}")]
    Import(String),
    #[error("errore SQLite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("errore di I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("errore ZIP: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("errore XML: {0}")]
    Xml(#[from] quick_xml::Error),
}

pub type Result<T> = std::result::Result<T, OmiError>;
