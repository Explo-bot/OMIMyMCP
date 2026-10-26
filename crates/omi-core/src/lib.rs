// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Libreria condivisa da `omi-import` e `omi-mcp`: identità del programma, database SQLite,
//! codifica e misure delle geometrie, normalizzazione dei testi, importazione delle forniture,
//! serializzazione JSON e logica degli strumenti MCP.

pub mod about;
pub mod db;
pub mod error;
pub mod geometry;
pub mod import;
pub mod json;
pub mod service;
pub mod text;

pub use error::{OmiError, Result};
