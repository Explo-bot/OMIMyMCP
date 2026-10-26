// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Strumenti MCP: adattatore rmcp → `omi_core::service`.
//!
//! API di rmcp 3.7.0 usate, verificate sui sorgenti della versione risolta in `Cargo.lock`:
//! - `#[tool_router(router = tool_router)]` su un `impl` genera `fn tool_router() -> ToolRouter<Self>`
//!   con una rotta per ogni metodo `#[tool(name = …, description = …, annotations(read_only_hint = …,
//!   idempotent_hint = …))]`. Lo schema di input viene da `Parameters<T>` (`T: Deserialize +
//!   JsonSchema`), generato da `schema_for_type` con schemars draft 2020-12 senza `nullable`; titolo e
//!   descrizione della struct vengono tolti. `ToolRouter::list_all` ordina i tool per nome.
//! - `#[tool_handler(router = self.tool_router)]` su `impl ServerHandler` genera `list_tools` e
//!   `get_tool`; **non** genera `call_tool` né `get_info` se sono già nell'`impl` (qui lo sono:
//!   `call_tool` scrive la riga `TOOL` del log con gli argomenti grezzi del client).
//! - `ServerHandler::get_info() -> ServerConfig` (alias di `InitializeResult`):
//!   `ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
//!   .with_server_info(Implementation::new(nome, versione).with_title(…).with_description(…)
//!   .with_website_url(…)).with_instructions(…)`.
//! - `ServerHandler::call_tool(CallToolRequestParams, RequestContext<RoleServer>) ->
//!   Result<CallToolResponse, ErrorData>`: si delega con
//!   `ToolRouter::call(ToolCallContext::new(self, request, context))`. `CallToolResponse::Complete`
//!   contiene il `CallToolResult`. Il router trasforma gli errori di deserializzazione dei parametri in
//!   `CallToolResult::error` con il testo `failed to deserialize parameters: …`.
//! - `CallToolResult::success(vec![ContentBlock::text(…)])` e `CallToolResult::error(…)` (che imposta
//!   `isError: true`); `ContentBlock::as_text()` restituisce `TextContent { text, .. }`.
//! - Trasporto (in `server.rs`): `StreamableHttpService::new(factory, Arc<NeverSessionManager>,
//!   StreamableHttpServerConfig)`. Senza stato: `with_legacy_session_mode(false)`, più
//!   `with_json_response(true)` per rispondere in `application/json`. Con `allowed_hosts` vuoto
//!   (`disable_allowed_hosts`) l'intestazione `Host` non viene controllata; il default di rmcp accetta
//!   solo `localhost`, `127.0.0.1` e `::1`. Ci sono anche `with_max_request_body_bytes` e
//!   `with_cancellation_token`.

use std::any::Any;
use std::sync::Arc;
use std::time::Instant;

use omi_core::OmiError;
use omi_core::about::{NOME, URL_PROGETTO, VERSIONE, attribuzione};
use omi_core::db::Pool;
use omi_core::service::{
    CercaComuni, DettaglioZona, Service, StatisticheComune, StoricoZona, ZonaDaCoordinate,
    ZoneComune, ZoneVicine,
};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::handler::server::tool::ToolCallContext;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ServerCapabilities, ServerConfig,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, tool, tool_handler, tool_router};

use crate::log::{Logger, utf16_len};

pub const TITOLO: &str = "OMIMyMCP - Quotazioni immobiliari OMI";

/// Prefisso del messaggio di rmcp per i parametri non deserializzabili.
const PREFISSO_PARAMETRI_RMCP: &str = "failed to deserialize parameters:";

pub fn instructions() -> String {
    format!(
        "Server MCP sulle quotazioni immobiliari OMI (Agenzia delle Entrate): zone OMI per comune, ricerca per coordinate, quotazioni di compravendita e locazione per semestre, serie storiche. Quando presenti risultati basati su questi dati cita la fonte \"Agenzia Entrate - OMI\". Software: {}.",
        attribuzione()
    )
}

/// Gestore MCP: un'istanza per richiesta (trasporto senza stato), con pool e log condivisi.
#[derive(Clone)]
pub struct OmiServer {
    pool: Arc<Pool>,
    logger: Arc<Logger>,
    tool_router: ToolRouter<OmiServer>,
}

#[tool_router(router = tool_router)]
impl OmiServer {
    pub fn new(pool: Arc<Pool>, logger: Arc<Logger>) -> Self {
        OmiServer {
            pool,
            logger,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "elenco_semestri",
        description = "Elenca i semestri OMI caricati nel database con numero di zone, quotazioni e perimetri disponibili.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn elenco_semestri(&self) -> CallToolResult {
        self.run("elenco_semestri", |s| s.elenco_semestri()).await
    }

    #[tool(
        name = "elenco_tipologie",
        description = "Elenca le tipologie edilizie OMI (codice e descrizione), es. 20 = Abitazioni civili, 13 = Box.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn elenco_tipologie(&self) -> CallToolResult {
        self.run("elenco_tipologie", |s| s.elenco_tipologie()).await
    }

    #[tool(
        name = "cerca_comuni",
        description = "Cerca comuni per nome (anche parziale, senza accenti), codice catastale (es. H501) o codice ISTAT. Restituisce codici, provincia, regione e semestri disponibili.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn cerca_comuni(&self, Parameters(p): Parameters<CercaComuni>) -> CallToolResult {
        self.run("cerca_comuni", move |s| s.cerca_comuni(&p)).await
    }

    #[tool(
        name = "zone_comune",
        description = "Elenca le zone OMI di un comune in un semestre: codice zona, LinkZona, fascia, descrizione, microzona, tipologia e stato prevalenti. Facoltativamente include quotazioni e dati del perimetro (centro, bbox, area).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn zone_comune(&self, Parameters(p): Parameters<ZoneComune>) -> CallToolResult {
        self.run("zone_comune", move |s| s.zone_comune(&p)).await
    }

    #[tool(
        name = "dettaglio_zona",
        description = "Tutti i dati di una zona OMI in un semestre: anagrafica zona e comune, quotazioni di compravendita (€/m²) e locazione (€/m² mese) per tipologia e stato conservativo, dati del perimetro e, se richiesto, la geometria GeoJSON.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn dettaglio_zona(&self, Parameters(p): Parameters<DettaglioZona>) -> CallToolResult {
        self.run("dettaglio_zona", move |s| s.dettaglio_zona(&p))
            .await
    }

    #[tool(
        name = "zona_da_coordinate",
        description = "Individua la zona OMI che contiene un punto (latitudine/longitudine WGS84) e restituisce comune, zona, quotazioni e perimetro. Se il punto non ricade in alcuna zona restituisce la più vicina entro 2 km.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn zona_da_coordinate(
        &self,
        Parameters(p): Parameters<ZonaDaCoordinate>,
    ) -> CallToolResult {
        self.run("zona_da_coordinate", move |s| s.zona_da_coordinate(&p))
            .await
    }

    #[tool(
        name = "zone_vicine",
        description = "Elenca le zone OMI entro un raggio (metri) da un punto, ordinate per distanza, con le quotazioni delle abitazioni civili.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn zone_vicine(&self, Parameters(p): Parameters<ZoneVicine>) -> CallToolResult {
        self.run("zone_vicine", move |s| s.zone_vicine(&p)).await
    }

    #[tool(
        name = "storico_zona",
        description = "Serie storica delle quotazioni di una zona OMI su tutti i semestri caricati, per tipologia e stato conservativo.",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn storico_zona(&self, Parameters(p): Parameters<StoricoZona>) -> CallToolResult {
        self.run("storico_zona", move |s| s.storico_zona(&p)).await
    }

    #[tool(
        name = "statistiche_comune",
        description = "Riepilogo delle quotazioni di un comune in un semestre: minimo, massimo e medio per tipologia e medie delle abitazioni civili per fascia (centrale, semicentrale, periferica, ...).",
        annotations(read_only_hint = true, idempotent_hint = true)
    )]
    async fn statistiche_comune(
        &self,
        Parameters(p): Parameters<StatisticheComune>,
    ) -> CallToolResult {
        self.run("statistiche_comune", move |s| s.statistiche_comune(&p))
            .await
    }
}

impl OmiServer {
    /// Esegue uno strumento in `spawn_blocking` (rusqlite è sincrono) con una connessione del pool.
    /// Gli errori di validazione tornano al client così come sono; gli altri errori, un panic o un
    /// `JoinError` diventano un errore interno generico, con il dettaglio solo nel log.
    async fn run<F>(&self, tool: &'static str, f: F) -> CallToolResult
    where
        F: FnOnce(&Service<'_>) -> omi_core::Result<String> + Send + 'static,
    {
        let pool = self.pool.clone();
        let esito = tokio::task::spawn_blocking(move || {
            let conn = pool.get()?;
            f(&Service::new(&conn))
        })
        .await;
        let dettaglio = match esito {
            Ok(Ok(json)) => return CallToolResult::success(vec![ContentBlock::text(json)]),
            Ok(Err(OmiError::Validazione(m))) => {
                return CallToolResult::error(vec![ContentBlock::text(m)]);
            }
            Ok(Err(e)) => e.to_string(),
            Err(e) if e.is_panic() => format!("panic: {}", panic_message(e.into_panic())),
            Err(e) => e.to_string(),
        };
        self.logger
            .error(&format!("ERRORE INTERNO in '{tool}': {dettaglio}"));
        CallToolResult::error(vec![ContentBlock::text(format!(
            "Errore interno durante l'esecuzione di '{tool}'."
        ))])
    }
}

fn panic_message(payload: Box<dyn Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "(messaggio non disponibile)".to_string())
}

/// Testo della risposta di uno strumento (i blocchi di testo concatenati).
fn testo(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
        .collect()
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for OmiServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = Implementation::new(NOME, VERSIONE)
            .with_title(TITOLO)
            .with_description(attribuzione());
        if let Some(url) = URL_PROGETTO {
            info = info.with_website_url(url);
        }
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(info)
            .with_instructions(instructions())
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let inizio = Instant::now();
        let nome = request.name.to_string();
        let argomenti = match &request.arguments {
            Some(a) => serde_json::to_string(a).unwrap_or_else(|_| "{}".to_string()),
            None => "{}".to_string(),
        };
        let mut esito = self
            .tool_router
            .call(ToolCallContext::new(self, request, context))
            .await;

        // Messaggio in italiano per i parametri non validi (il dettaglio di serde resta com'è).
        if let Ok(CallToolResponse::Complete(r)) = &mut esito
            && r.is_error == Some(true)
            && let Some(dettaglio) = testo(r).strip_prefix(PREFISSO_PARAMETRI_RMCP)
        {
            r.content = vec![ContentBlock::text(format!(
                "Parametri non validi per '{nome}':{dettaglio}"
            ))];
        }

        if self.logger.traffic_enabled() {
            let ms = inizio.elapsed().as_millis();
            let esito_log = match &esito {
                Ok(CallToolResponse::Complete(r)) if r.is_error == Some(true) => {
                    format!("ERRORE: {}", testo(r))
                }
                Ok(CallToolResponse::Complete(r)) => {
                    format!("ok, {} caratteri", utf16_len(&testo(r)))
                }
                Ok(_) => "ok".to_string(),
                Err(e) => format!("ERRORE: {}", e.message),
            };
            self.logger
                .traffic(&format!("TOOL {nome} {argomenti} -> {esito_log} ({ms} ms)"));
        }
        esito
    }
}
