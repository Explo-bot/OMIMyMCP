// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Server HTTP (Axum): rotte `/`, `/health` e `/mcp`, layer, bind e arresto pulito. `run` è l'unico
//! punto d'ingresso, usato sia dalla console sia dal servizio Windows.

use std::fmt;
use std::future::Future;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use axum::Router;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::middleware::from_fn_with_state;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use omi_core::about::{FONTE_DATI, attribuzione};
use omi_core::db::{Pool, print_summary};
use rmcp::transport::streamable_http_server::session::never::NeverSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use socket2::{Domain, Protocol, Socket, Type};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

use crate::auth::{Token, require_bearer};
use crate::log::{HttpLog, Logger, log_http};
use crate::mcp::OmiServer;

pub const PORTA_DEFAULT: u16 = 8765;
/// Limite del corpo delle richieste.
const LIMITE_CORPO: usize = 1024 * 1024;
/// Timeout delle richieste (fino all'invio delle intestazioni della risposta).
const TIMEOUT: Duration = Duration::from_secs(60);
/// Host sempre ammessi quando il controllo dell'intestazione `Host` è attivo.
const HOST_LOCALI: [&str; 3] = ["localhost", "127.0.0.1", "::1"];

/// Configurazione del server.
#[derive(Debug, Clone)]
pub struct Config {
    pub db_path: PathBuf,
    pub port: u16,
    /// `None`: tutte le interfacce, IPv4 e IPv6.
    pub bind: Option<IpAddr>,
    /// Opzione `/l`: righe `HTTP` e `TOOL` su console e nel file `<database>.log`.
    pub log: bool,
    pub trust_proxy: bool,
    pub token: Option<String>,
    /// Vuota: intestazione `Host` non controllata.
    pub allowed_hosts: Vec<String>,
    /// Avvio da console: banner e righe di log anche su stdout/stderr.
    pub console: bool,
    /// Servizio Windows: file `<database>.log` sempre aperto (errori interni) anche senza `log`.
    pub file_log: bool,
}

impl Config {
    pub fn new(db_path: PathBuf) -> Config {
        Config {
            db_path,
            port: PORTA_DEFAULT,
            bind: None,
            log: false,
            trust_proxy: false,
            token: None,
            allowed_hosts: Vec::new(),
            console: false,
            file_log: false,
        }
    }
}

/// Il database indicato non esiste (il server non lo crea mai).
#[derive(Debug)]
pub struct DatabaseNonTrovato(pub PathBuf);

impl fmt::Display for DatabaseNonTrovato {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Database non trovato: {}. Crearlo con omi-import.",
            self.0.display()
        )
    }
}

impl std::error::Error for DatabaseNonTrovato {}

/// Bind della porta non riuscito (di solito una porta già occupata).
#[derive(Debug)]
pub struct PortaNonDisponibile {
    pub porta: u16,
    pub source: io::Error,
}

impl fmt::Display for PortaNonDisponibile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "impossibile mettersi in ascolto sulla porta {}",
            self.porta
        )
    }
}

impl std::error::Error for PortaNonDisponibile {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Avvia il server e lo tiene attivo finché `shutdown` non si completa. Dopo il bind invia
/// l'indirizzo effettivo su `ready` (serve al servizio Windows e ai test).
pub async fn run(
    config: Config,
    shutdown: impl Future<Output = ()> + Send + 'static,
    ready: Option<oneshot::Sender<SocketAddr>>,
) -> anyhow::Result<()> {
    if !config.db_path.is_file() {
        return Err(DatabaseNonTrovato(config.db_path.clone()).into());
    }
    let db_abs = std::path::absolute(&config.db_path).unwrap_or_else(|_| config.db_path.clone());
    let pool = Arc::new(
        Pool::new(&config.db_path, Pool::default_size())
            .with_context(|| format!("impossibile aprire il database {}", db_abs.display()))?,
    );
    let logger = Arc::new(Logger::new(
        (config.log || config.file_log).then(|| db_abs.with_extension("log")),
        config.console,
        config.log,
    ));

    let listener = bind(config.bind, config.port)?;
    let addr = listener.local_addr()?;

    // Banner su stdout; gli errori di scrittura (console chiusa) si ignorano.
    if config.console {
        let mut banner = format!(
            "{}\nFonte dei dati: {FONTE_DATI}\nDatabase: {}\n",
            attribuzione(),
            db_abs.display()
        )
        .into_bytes();
        print_summary(&*pool.get()?, &mut banner)?;
        banner.extend_from_slice(
            format!("Server MCP in ascolto su {}\n", mcp_url(config.bind, addr)).as_bytes(),
        );
        let _ = io::stdout().write_all(&banner);
    }
    if let Some(path) = logger.path() {
        logger.info(&format!("Log attivo, file: {}", path.display()));
    }
    if config.console {
        let _ = writeln!(io::stdout(), "Ctrl+C per terminare.");
    }

    let cancel = CancellationToken::new();
    let app = router(&config, pool, logger, cancel.child_token());
    if let Some(tx) = ready {
        let _ = tx.send(addr);
    }
    let esito = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown)
    .await;
    cancel.cancel();
    esito.context("errore del server HTTP")
}

/// Valore di `--bind`: un indirizzo IP, l'IPv6 anche tra parentesi quadre.
pub fn parse_bind(v: &str) -> Option<IpAddr> {
    v.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse()
        .ok()
}

/// URL di `/mcp` da mostrare all'avvio: `localhost` se il bind è su tutte le interfacce.
pub fn mcp_url(bind: Option<IpAddr>, addr: SocketAddr) -> String {
    match bind {
        Some(ip) if !ip.is_unspecified() => format!("http://{addr}/mcp"),
        _ => format!("http://localhost:{}/mcp", addr.port()),
    }
}

/// Socket in ascolto. Senza `--bind`: `[::]` con `IPV6_V6ONLY = false` (IPv4 e IPv6), oppure
/// `0.0.0.0` se l'IPv6 non è disponibile.
fn bind(bind: Option<IpAddr>, port: u16) -> anyhow::Result<TcpListener> {
    let listener = match bind {
        Some(ip) => listen(SocketAddr::new(ip, port), false),
        None => listen(SocketAddr::new(Ipv6Addr::UNSPECIFIED.into(), port), true)
            .or_else(|_| listen(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), port), false)),
    }
    .map_err(|source| PortaNonDisponibile {
        porta: port,
        source,
    })?;
    listener.set_nonblocking(true)?;
    Ok(TcpListener::from_std(listener)?)
}

fn listen(addr: SocketAddr, dual_stack: bool) -> io::Result<std::net::TcpListener> {
    let socket = Socket::new(Domain::for_address(addr), Type::STREAM, Some(Protocol::TCP))?;
    if dual_stack {
        socket.set_only_v6(false)?;
    }
    // Su Windows SO_REUSEADDR permetterebbe a un altro processo di prendere la stessa porta.
    #[cfg(not(windows))]
    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;
    Ok(socket.into())
}

/// Rotte e layer. Dall'esterno: log HTTP → limite del corpo → timeout → (autenticazione su `/mcp`).
fn router(
    config: &Config,
    pool: Arc<Pool>,
    logger: Arc<Logger>,
    cancel: CancellationToken,
) -> Router {
    let mut mcp_config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_max_request_body_bytes(LIMITE_CORPO)
        .with_cancellation_token(cancel);
    mcp_config = if config.allowed_hosts.is_empty() {
        mcp_config.disable_allowed_hosts()
    } else {
        mcp_config.with_allowed_hosts(
            HOST_LOCALI
                .iter()
                .map(|h| h.to_string())
                .chain(config.allowed_hosts.iter().cloned()),
        )
    };
    let handler = OmiServer::new(pool.clone(), logger.clone());
    let mcp_service = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(NeverSessionManager::default()),
        mcp_config,
    );

    let mut mcp = Router::new().nest_service("/mcp", mcp_service);
    if let Some(token) = &config.token {
        mcp = mcp.layer(from_fn_with_state(
            Arc::new(Token::new(token)),
            require_bearer,
        ));
    }
    let http_log = Arc::new(HttpLog {
        logger,
        trust_proxy: config.trust_proxy,
    });
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .with_state(pool)
        .merge(mcp)
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            TIMEOUT,
        ))
        .layer(RequestBodyLimitLayer::new(LIMITE_CORPO))
        .layer(from_fn_with_state(http_log, log_http))
}

async fn root() -> Response {
    (
        [(CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!(
            "{}\nServer MCP - endpoint: /mcp\nFonte dei dati: {FONTE_DATI}\n",
            attribuzione()
        ),
    )
        .into_response()
}

/// `200 ok` se il database risponde a `SELECT 1`, altrimenti `503`.
async fn health(State(pool): State<Arc<Pool>>) -> Response {
    let ok = tokio::task::spawn_blocking(move || {
        pool.get()
            .ok()
            .and_then(|c| c.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).ok())
            == Some(1)
    })
    .await
    .unwrap_or(false);
    if ok {
        (StatusCode::OK, "ok").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "database non disponibile").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_di_avvio() {
        let a: SocketAddr = "[::]:8765".parse().unwrap();
        assert_eq!(mcp_url(None, a), "http://localhost:8765/mcp");
        let a: SocketAddr = "127.0.0.1:9000".parse().unwrap();
        assert_eq!(
            mcp_url(Some("127.0.0.1".parse().unwrap()), a),
            "http://127.0.0.1:9000/mcp"
        );
        let a: SocketAddr = "[::1]:9000".parse().unwrap();
        assert_eq!(
            mcp_url(Some("::1".parse().unwrap()), a),
            "http://[::1]:9000/mcp"
        );
        let a: SocketAddr = "0.0.0.0:9000".parse().unwrap();
        assert_eq!(
            mcp_url(Some("0.0.0.0".parse().unwrap()), a),
            "http://localhost:9000/mcp"
        );
    }

    #[test]
    fn bind_porta_zero() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let l = bind(Some("127.0.0.1".parse().unwrap()), 0).unwrap();
            assert_ne!(l.local_addr().unwrap().port(), 0);
            let l = bind(None, 0).unwrap();
            assert!(l.local_addr().unwrap().ip().is_unspecified());
        });
    }
}
