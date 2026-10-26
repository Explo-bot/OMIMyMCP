// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Server MCP sulle quotazioni OMI: server HTTP (Axum), strumenti MCP (rmcp), log e token. Il binario
//! `omi-mcp` legge la riga di comando e chiama [`server::run`], da console o dal servizio Windows.

pub mod auth;
pub mod log;
pub mod mcp;
pub mod server;
#[cfg(windows)]
pub mod service_windows;
