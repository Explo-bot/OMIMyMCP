// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Token Bearer facoltativo su `/mcp`: senza `Authorization: Bearer <token>` corretto la risposta è
//! `401` con `WWW-Authenticate: Bearer`. Il confronto avviene a tempo costante sui digest SHA-256, così
//! non dipende né dal contenuto né dalla lunghezza del token ricevuto.

use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, bail};
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::http::header::{AUTHORIZATION, WWW_AUTHENTICATE};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

pub struct Token {
    digest: [u8; 32],
}

impl Token {
    pub fn new(secret: &str) -> Token {
        Token {
            digest: Sha256::digest(secret.as_bytes()).into(),
        }
    }

    pub fn matches(&self, candidate: &str) -> bool {
        let d: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
        d.iter()
            .zip(self.digest.iter())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b))
            == 0
    }

    /// Verifica il valore dell'intestazione `Authorization` (schema `Bearer`, senza distinzione di
    /// maiuscole).
    pub fn check_header(&self, value: Option<&str>) -> bool {
        value
            .and_then(|v| v.split_once(' '))
            .filter(|(schema, _)| schema.eq_ignore_ascii_case("Bearer"))
            .is_some_and(|(_, t)| self.matches(t.trim()))
    }
}

/// Prima riga del file, ripulita; errore se il file non si legge o la riga è vuota.
pub fn read_token_file(path: &Path) -> anyhow::Result<String> {
    let testo = std::fs::read_to_string(path)
        .with_context(|| format!("impossibile leggere il file del token {}", path.display()))?;
    let token = testo.lines().next().unwrap_or_default().trim();
    if token.is_empty() {
        bail!("il file del token {} è vuoto", path.display());
    }
    Ok(token.to_string())
}

/// Middleware di autenticazione per `/mcp`.
pub async fn require_bearer(State(token): State<Arc<Token>>, req: Request, next: Next) -> Response {
    let value = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if token.check_header(value) {
        next.run(req).await
    } else {
        (
            StatusCode::UNAUTHORIZED,
            [(WWW_AUTHENTICATE, "Bearer")],
            "Token mancante o non valido.\n",
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confronto_del_token() {
        let t = Token::new("segreto");
        assert!(t.matches("segreto"));
        assert!(!t.matches("segret"));
        assert!(!t.matches(""));
        assert!(t.check_header(Some("Bearer segreto")));
        assert!(t.check_header(Some("bearer  segreto ")));
        assert!(!t.check_header(Some("Basic segreto")));
        assert!(!t.check_header(Some("Bearer")));
        assert!(!t.check_header(Some("Bearer altro")));
        assert!(!t.check_header(None));
    }

    #[test]
    fn file_del_token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("token.txt");
        std::fs::write(&path, "  abc123 \r\nseconda riga\n").unwrap();
        assert_eq!(read_token_file(&path).unwrap(), "abc123");
        std::fs::write(&path, "\n").unwrap();
        assert!(read_token_file(&path).is_err());
        assert!(read_token_file(&dir.path().join("manca.txt")).is_err());
    }
}
