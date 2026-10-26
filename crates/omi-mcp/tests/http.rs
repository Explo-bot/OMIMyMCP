// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Test d'integrazione HTTP: server su porta 0 e client rmcp. `initialize`, `tools/list`, una
//! chiamata per tool, rotte `/` e `/health`, token, controllo dell'Host, log e concorrenza.

#[path = "../../omi-core/tests/support/mod.rs"]
mod support;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use omi_core::about::attribuzione;
use omi_core::service::*;
use omi_mcp::log::utf16_len;
use omi_mcp::server::{self, Config};
use regex::Regex;
use rmcp::model::{CallToolRequestParams, CallToolResult};
use rmcp::service::RunningService;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::transport::streamable_http_client::StreamableHttpClientTransportConfig;
use rmcp::{RoleClient, ServiceExt};
use serde_json::{Value, json};
use tempfile::TempDir;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

type Client = RunningService<RoleClient, ()>;

fn scenario() -> &'static PathBuf {
    static DB: OnceLock<(TempDir, PathBuf)> = OnceLock::new();
    &DB.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let db = support::scenario_server(dir.path());
        (dir, db)
    })
    .1
}

/// Copia del database dello scenario in una cartella propria (per log e scritture).
fn copia_scenario() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("omi.db");
    std::fs::copy(scenario(), &db).unwrap();
    (dir, db)
}

/// Server in esecuzione: indirizzo, segnale di arresto e task.
struct Server {
    addr: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    task: JoinHandle<anyhow::Result<()>>,
}

impl Server {
    async fn start(config: Config) -> Server {
        let (stop_tx, stop_rx) = oneshot::channel::<()>();
        let (ready_tx, ready_rx) = oneshot::channel();
        let task = tokio::spawn(server::run(
            config,
            async {
                let _ = stop_rx.await;
            },
            Some(ready_tx),
        ));
        let addr = ready_rx.await.expect("server non avviato");
        Server {
            addr,
            stop: Some(stop_tx),
            task,
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    async fn client(&self) -> Client {
        ().serve(StreamableHttpClientTransport::from_uri(self.url("/mcp")))
            .await
            .unwrap()
    }

    async fn stop(mut self) {
        if let Some(s) = self.stop.take() {
            let _ = s.send(());
        }
        self.task.await.unwrap().unwrap();
    }
}

fn config(db: &Path) -> Config {
    let mut c = Config::new(db.to_path_buf());
    c.port = 0;
    c.bind = Some("127.0.0.1".parse().unwrap());
    c
}

fn testo(r: &CallToolResult) -> String {
    r.content
        .iter()
        .filter_map(|c| c.as_text().map(|t| t.text.as_str()))
        .collect()
}

/// Chiama un tool: (`isError`, testo della risposta).
async fn call(c: &Client, name: &str, args: Value) -> (bool, String) {
    let mut p = CallToolRequestParams::new(name.to_string());
    if let Value::Object(m) = args {
        p = p.with_arguments(m);
    }
    let r = c.call_tool(p).await.unwrap();
    (r.is_error == Some(true), testo(&r))
}

/// `initialize` grezzo, per i test su token e intestazione Host.
async fn post_initialize(url: &str, headers: &[(&str, &str)]) -> reqwest::Response {
    let mut req = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream")
        .body(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        );
    for (k, v) in headers {
        req = req.header(*k, *v);
    }
    req.send().await.unwrap()
}

// ---------------------------------------------------------------------------------------------
// initialize e tools/list
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn initialize_server_info_e_instructions() {
    let s = Server::start(config(scenario())).await;
    let c = s.client().await;
    let info = c.peer_info().unwrap();
    let v = serde_json::to_value(&*info).unwrap();
    let mut atteso = json!({
        "name": "OMIMyMCP",
        "title": "OMIMyMCP - Quotazioni immobiliari OMI",
        "version": "1.0.0",
        "description": attribuzione(),
    });
    if let Some(url) = omi_core::about::URL_PROGETTO {
        atteso["websiteUrl"] = json!(url);
    }
    assert_eq!(v["serverInfo"], atteso);
    assert_eq!(
        v["instructions"],
        json!(format!(
            "Server MCP sulle quotazioni immobiliari OMI (Agenzia delle Entrate): zone OMI per comune, ricerca per coordinate, quotazioni di compravendita e locazione per semestre, serie storiche. Quando presenti risultati basati su questi dati cita la fonte \"Agenzia Entrate - OMI\". Software: {}.",
            attribuzione()
        ))
    );
    // Solo la capacità tools.
    let caps = v["capabilities"].as_object().unwrap();
    assert_eq!(caps.keys().collect::<Vec<_>>(), ["tools"]);
    c.cancel().await.unwrap();
    s.stop().await;
}

const DC: &str = "Nome del comune, codice catastale o codice ISTAT";
const DP: &str = "Sigla provincia, utile se il nome è ambiguo";
const DS: &str = "Semestre AAAA/S; se omesso l'ultimo disponibile";

/// Descrizione e parametri (nome, tipo, obbligatorio, descrizione) di ogni tool.
#[allow(clippy::type_complexity)]
fn strumenti_attesi() -> Vec<(
    &'static str,
    &'static str,
    Vec<(&'static str, &'static str, bool, &'static str)>,
)> {
    vec![
        (
            "cerca_comuni",
            "Cerca comuni per nome (anche parziale, senza accenti), codice catastale (es. H501) o codice ISTAT. Restituisce codici, provincia, regione e semestri disponibili.",
            vec![
                ("testo", "string", true, DC),
                (
                    "provincia",
                    "string",
                    false,
                    "Sigla della provincia per filtrare (es. RM), facoltativa",
                ),
            ],
        ),
        (
            "dettaglio_zona",
            "Tutti i dati di una zona OMI in un semestre: anagrafica zona e comune, quotazioni di compravendita (€/m²) e locazione (€/m² mese) per tipologia e stato conservativo, dati del perimetro e, se richiesto, la geometria GeoJSON.",
            vec![
                ("comune", "string", true, DC),
                (
                    "zona",
                    "string",
                    true,
                    "Codice zona OMI (es. B1, C2, R1) oppure LinkZona (es. RM00000123)",
                ),
                ("provincia", "string", false, DP),
                ("semestre", "string", false, DS),
                (
                    "includi_geometria",
                    "boolean",
                    false,
                    "Includere le coordinate del perimetro in formato GeoJSON MultiPolygon",
                ),
            ],
        ),
        (
            "elenco_semestri",
            "Elenca i semestri OMI caricati nel database con numero di zone, quotazioni e perimetri disponibili.",
            vec![],
        ),
        (
            "elenco_tipologie",
            "Elenca le tipologie edilizie OMI (codice e descrizione), es. 20 = Abitazioni civili, 13 = Box.",
            vec![],
        ),
        (
            "statistiche_comune",
            "Riepilogo delle quotazioni di un comune in un semestre: minimo, massimo e medio per tipologia e medie delle abitazioni civili per fascia (centrale, semicentrale, periferica, ...).",
            vec![
                ("comune", "string", true, DC),
                ("provincia", "string", false, DP),
                ("semestre", "string", false, DS),
            ],
        ),
        (
            "storico_zona",
            "Serie storica delle quotazioni di una zona OMI su tutti i semestri caricati, per tipologia e stato conservativo.",
            vec![
                ("comune", "string", true, DC),
                (
                    "zona",
                    "string",
                    true,
                    "Codice zona OMI (es. B1) o LinkZona",
                ),
                ("provincia", "string", false, DP),
                (
                    "cod_tipologia",
                    "integer",
                    false,
                    "Codice tipologia (es. 20 abitazioni civili); se omesso tutte",
                ),
                (
                    "stato",
                    "string",
                    false,
                    "Stato conservativo: OTTIMO, NORMALE o SCADENTE; se omesso tutti",
                ),
            ],
        ),
        (
            "zona_da_coordinate",
            "Individua la zona OMI che contiene un punto (latitudine/longitudine WGS84) e restituisce comune, zona, quotazioni e perimetro. Se il punto non ricade in alcuna zona restituisce la più vicina entro 2 km.",
            vec![
                (
                    "latitudine",
                    "number",
                    true,
                    "Latitudine in gradi decimali WGS84 (es. 41.9028)",
                ),
                (
                    "longitudine",
                    "number",
                    true,
                    "Longitudine in gradi decimali WGS84 (es. 12.4964)",
                ),
                ("semestre", "string", false, DS),
                (
                    "includi_geometria",
                    "boolean",
                    false,
                    "Includere la geometria GeoJSON della zona",
                ),
            ],
        ),
        (
            "zone_comune",
            "Elenca le zone OMI di un comune in un semestre: codice zona, LinkZona, fascia, descrizione, microzona, tipologia e stato prevalenti. Facoltativamente include quotazioni e dati del perimetro (centro, bbox, area).",
            vec![
                ("comune", "string", true, DC),
                ("provincia", "string", false, DP),
                (
                    "semestre",
                    "string",
                    false,
                    "Semestre nel formato AAAA/S (es. 2025/1); se omesso l'ultimo disponibile",
                ),
                (
                    "includi_valori",
                    "boolean",
                    false,
                    "Includere le quotazioni di tutte le tipologie per ogni zona",
                ),
                (
                    "includi_perimetri",
                    "boolean",
                    false,
                    "Includere centro, bbox e area del perimetro di ogni zona",
                ),
            ],
        ),
        (
            "zone_vicine",
            "Elenca le zone OMI entro un raggio (metri) da un punto, ordinate per distanza, con le quotazioni delle abitazioni civili.",
            vec![
                ("latitudine", "number", true, "Latitudine WGS84"),
                ("longitudine", "number", true, "Longitudine WGS84"),
                ("raggio_m", "number", false, "Raggio in metri (max 20000)"),
                ("semestre", "string", false, DS),
            ],
        ),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_list_con_descrizioni_e_parametri() {
    let s = Server::start(config(scenario())).await;
    let c = s.client().await;
    let tools = c.list_all_tools().await.unwrap();
    let attesi = strumenti_attesi();
    assert_eq!(tools.len(), 9);
    for (tool, (nome, descrizione, parametri)) in tools.iter().zip(&attesi) {
        assert_eq!(tool.name, *nome);
        assert_eq!(tool.description.as_deref(), Some(*descrizione), "{nome}");
        let ann = tool.annotations.as_ref().unwrap();
        assert_eq!(ann.read_only_hint, Some(true), "{nome}");
        assert_eq!(ann.idempotent_hint, Some(true), "{nome}");

        let schema = &tool.input_schema;
        assert_eq!(schema["type"], "object", "{nome}");
        let props = schema["properties"].as_object().unwrap();
        let nomi: Vec<&str> = props.keys().map(String::as_str).collect();
        let nomi_attesi: Vec<&str> = parametri.iter().map(|p| p.0).collect();
        assert_eq!(nomi, nomi_attesi, "{nome}");
        for (p, tipo, _, descr) in parametri {
            assert_eq!(props[*p]["type"], *tipo, "{nome}.{p}");
            assert_eq!(props[*p]["description"], *descr, "{nome}.{p}");
        }
        let required: Vec<&str> = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let required_attesi: Vec<&str> = parametri.iter().filter(|p| p.2).map(|p| p.0).collect();
        assert_eq!(required, required_attesi, "{nome}");
    }
    // Default dichiarati nello schema.
    let zv = tools.iter().find(|t| t.name == "zone_vicine").unwrap();
    assert_eq!(zv.input_schema["properties"]["raggio_m"]["default"], 1000);
    c.cancel().await.unwrap();
    s.stop().await;
}

// ---------------------------------------------------------------------------------------------
// tools/call
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn una_chiamata_per_tool() {
    let s = Server::start(config(scenario())).await;
    let c = s.client().await;
    let conn = omi_core::db::open(scenario(), true).unwrap();
    let svc = Service::new(&conn);

    // Il testo restituito via MCP è esattamente quello del servizio; numeri e booleani anche come
    // stringa.
    let casi: Vec<(&str, Value, String)> = vec![
        (
            "elenco_semestri",
            Value::Null,
            svc.elenco_semestri().unwrap(),
        ),
        (
            "elenco_tipologie",
            json!({}),
            svc.elenco_tipologie().unwrap(),
        ),
        (
            "cerca_comuni",
            json!({"testo": "samone"}),
            svc.cerca_comuni(&CercaComuni {
                testo: "samone".into(),
                provincia: None,
            })
            .unwrap(),
        ),
        (
            "zone_comune",
            json!({"comune": "roma", "includi_valori": "true", "includi_perimetri": true}),
            svc.zone_comune(&ZoneComune {
                comune: "roma".into(),
                includi_valori: true,
                includi_perimetri: true,
                ..Default::default()
            })
            .unwrap(),
        ),
        (
            "dettaglio_zona",
            json!({"comune": "roma", "zona": "B1", "semestre": "2024/2", "includi_geometria": true}),
            svc.dettaglio_zona(&DettaglioZona {
                comune: "roma".into(),
                zona: "B1".into(),
                semestre: Some("2024/2".into()),
                includi_geometria: true,
                ..Default::default()
            })
            .unwrap(),
        ),
        (
            "zona_da_coordinate",
            json!({"latitudine": "41.905", "longitudine": 12.495}),
            svc.zona_da_coordinate(&ZonaDaCoordinate {
                latitudine: 41.905,
                longitudine: 12.495,
                ..Default::default()
            })
            .unwrap(),
        ),
        (
            "zone_vicine",
            json!({"latitudine": 41.905, "longitudine": 12.495, "raggio_m": "50000"}),
            svc.zone_vicine(&ZoneVicine {
                latitudine: 41.905,
                longitudine: 12.495,
                raggio_m: Some(50000.0),
                semestre: None,
            })
            .unwrap(),
        ),
        (
            "storico_zona",
            json!({"comune": "roma", "zona": "b1", "cod_tipologia": "20", "stato": "normale"}),
            svc.storico_zona(&StoricoZona {
                comune: "roma".into(),
                zona: "b1".into(),
                cod_tipologia: Some(20),
                stato: Some("normale".into()),
                ..Default::default()
            })
            .unwrap(),
        ),
        (
            "statistiche_comune",
            json!({"comune": "roma", "semestre": null}),
            svc.statistiche_comune(&StatisticheComune {
                comune: "roma".into(),
                ..Default::default()
            })
            .unwrap(),
        ),
    ];
    for (nome, args, atteso) in casi {
        let (errore, t) = call(&c, nome, args).await;
        assert!(!errore, "{nome}: {t}");
        assert_eq!(t, atteso, "{nome}");
    }

    // Errore di validazione: isError con il messaggio d'errore.
    let (errore, t) = call(
        &c,
        "dettaglio_zona",
        json!({"zona": "R1", "comune": "samone"}),
    )
    .await;
    assert!(errore);
    assert_eq!(
        t,
        "'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754]"
    );
    // Parametri non validi o mancanti.
    let (errore, t) = call(
        &c,
        "zone_vicine",
        json!({"latitudine": "x", "longitudine": 1}),
    )
    .await;
    assert!(errore);
    assert_eq!(
        t,
        "Parametri non validi per 'zone_vicine': atteso un numero, ricevuto \"x\""
    );
    let (errore, t) = call(&c, "cerca_comuni", json!({})).await;
    assert!(
        errore && t.starts_with("Parametri non validi per 'cerca_comuni':"),
        "{t}"
    );

    c.cancel().await.unwrap();
    s.stop().await;
}

// ---------------------------------------------------------------------------------------------
// Rotte, token, Host
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn rotte_radice_e_health() {
    let s = Server::start(config(scenario())).await;
    let r = reqwest::get(s.url("/")).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(
        r.headers()["content-type"].to_str().unwrap(),
        "text/plain; charset=utf-8"
    );
    assert_eq!(
        r.text().await.unwrap(),
        format!(
            "{}\nServer MCP - endpoint: /mcp\nFonte dei dati: Agenzia Entrate - OMI\n",
            attribuzione()
        )
    );
    let r = reqwest::get(s.url("/health")).await.unwrap();
    assert_eq!(r.status(), 200);
    assert_eq!(r.text().await.unwrap(), "ok");
    assert_eq!(reqwest::get(s.url("/nulla")).await.unwrap().status(), 404);
    // Corpo oltre 1 MiB. Il server risponde 413 senza leggere il corpo e chiude la connessione: su
    // Windows il client che sta ancora inviando può ricevere un reset al posto della risposta, quindi
    // si ritenta qualche volta.
    let mut status = None;
    for _ in 0..5 {
        let invio = reqwest::Client::new()
            .post(s.url("/mcp"))
            .header("content-type", "application/json")
            .header("accept", "application/json, text/event-stream")
            .body(vec![b' '; 1024 * 1024 + 10])
            .send()
            .await;
        if let Ok(r) = invio {
            status = Some(r.status());
            break;
        }
    }
    assert_eq!(status.map(|s| s.as_u16()), Some(413));
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn token_bearer() {
    let mut cfg = config(scenario());
    cfg.token = Some("segreto-di-prova".into());
    let s = Server::start(cfg).await;
    let url = s.url("/mcp");

    for headers in [vec![], vec![("authorization", "Bearer sbagliato")]] {
        let r = post_initialize(&url, &headers).await;
        assert_eq!(r.status(), 401);
        assert_eq!(r.headers()["www-authenticate"], "Bearer");
    }
    let r = post_initialize(&url, &[("authorization", "Bearer segreto-di-prova")]).await;
    assert_eq!(r.status(), 200);

    // Client rmcp con il token corretto.
    let transport = StreamableHttpClientTransport::from_config(
        StreamableHttpClientTransportConfig::with_uri(url.clone()).auth_header("segreto-di-prova"),
    );
    let c = ().serve(transport).await.unwrap();
    let (errore, t) = call(&c, "elenco_tipologie", json!({})).await;
    assert!(!errore && t.starts_with("[{\"codice\":13"), "{t}");
    c.cancel().await.unwrap();

    // `/` e `/health` restano liberi.
    assert_eq!(reqwest::get(s.url("/")).await.unwrap().status(), 200);
    assert_eq!(reqwest::get(s.url("/health")).await.unwrap().status(), 200);
    s.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn controllo_host() {
    // Senza --allowed-host qualunque Host è accettato (LAN, Caddy, tunnel).
    let s = Server::start(config(scenario())).await;
    let r = post_initialize(&s.url("/mcp"), &[("host", "omi.example.org")]).await;
    assert_eq!(r.status(), 200);
    s.stop().await;

    let mut cfg = config(scenario());
    cfg.allowed_hosts = vec!["omi.example.org".into()];
    let s = Server::start(cfg).await;
    let url = s.url("/mcp");
    assert_eq!(post_initialize(&url, &[]).await.status(), 200);
    assert_eq!(
        post_initialize(&url, &[("host", "omi.example.org")])
            .await
            .status(),
        200
    );
    assert_eq!(
        post_initialize(&url, &[("host", "localhost")])
            .await
            .status(),
        200
    );
    assert_eq!(
        post_initialize(&url, &[("host", "attaccante.example")])
            .await
            .status(),
        403
    );
    s.stop().await;
}

// ---------------------------------------------------------------------------------------------
// Log
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn log_righe_http_e_tool() {
    let (dir, db) = copia_scenario();
    // Una geometria corrotta per provocare un errore interno.
    {
        let w = omi_core::db::open(&db, false).unwrap();
        w.execute(
            "UPDATE geometria SET dati=x'05' WHERE id IN (SELECT geometria_id FROM zona_geo WHERE comune_amm='F205')",
            [],
        )
        .unwrap();
    }
    let mut cfg = config(&db);
    cfg.log = true;
    let s = Server::start(cfg).await;
    let c = s.client().await;

    let (_, semestri) = call(&c, "elenco_semestri", Value::Null).await;
    let (_, comuni) = call(&c, "cerca_comuni", json!({"testo": "Róma"})).await;
    let (errore, _) = call(
        &c,
        "dettaglio_zona",
        json!({"zona": "R1", "comune": "samone"}),
    )
    .await;
    assert!(errore);
    let (errore, interno) = call(
        &c,
        "dettaglio_zona",
        json!({"comune": "milano", "zona": "B1", "semestre": "2024/2", "includi_geometria": true}),
    )
    .await;
    assert!(errore);
    assert_eq!(
        interno,
        "Errore interno durante l'esecuzione di 'dettaglio_zona'."
    );
    reqwest::get(s.url("/health")).await.unwrap();
    c.cancel().await.unwrap();
    s.stop().await;

    let log = std::fs::read_to_string(dir.path().join("omi.log")).unwrap();
    let ts = r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3}";
    let righe: Vec<&str> = log.lines().collect();
    let cerca = |re: &str| -> Vec<regex::Captures<'_>> {
        let re = Regex::new(&format!("^{ts} {re}$")).unwrap();
        righe.iter().filter_map(|r| re.captures(r)).collect()
    };

    assert_eq!(cerca(r"Log attivo, file: .*omi\.log").len(), 1, "{log}");
    let ok = cerca(r"TOOL elenco_semestri \{\} -> ok, (\d+) caratteri \(\d+ ms\)");
    assert_eq!(ok.len(), 1, "{log}");
    assert_eq!(ok[0][1].parse::<usize>().unwrap(), utf16_len(&semestri));
    // "più" nella nota: unità UTF-16 diverse dai byte.
    assert_ne!(utf16_len(&semestri), semestri.len());
    let ok = cerca(r#"TOOL cerca_comuni \{"testo":"Róma"\} -> ok, (\d+) caratteri \(\d+ ms\)"#);
    assert_eq!(ok[0][1].parse::<usize>().unwrap(), utf16_len(&comuni));
    assert_eq!(
        cerca(
            r#"TOOL dettaglio_zona \{"zona":"R1","comune":"samone"\} -> ERRORE: 'samone' è ambiguo, specificare la provincia o il codice catastale\. Candidati: SAMONE \(TO\) \[H753\], SAMONE \(TN\) \[H754\] \(\d+ ms\)"#
        )
        .len(),
        1,
        "{log}"
    );
    assert_eq!(
        cerca(r"TOOL dettaglio_zona \{.*\} -> ERRORE: Errore interno durante l'esecuzione di 'dettaglio_zona'\. \(\d+ ms\)").len(),
        1,
        "{log}"
    );
    assert_eq!(
        cerca(r"ERRORE INTERNO in 'dettaglio_zona': dati non validi: .*").len(),
        1,
        "{log}"
    );
    assert!(
        cerca(r"HTTP 127\.0\.0\.1 POST /mcp -> 200 \(\d+ ms\)").len() >= 4,
        "{log}"
    );
    assert_eq!(
        cerca(r"HTTP 127\.0\.0\.1 GET /health -> 200 \(\d+ ms\)").len(),
        1,
        "{log}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn senza_log_nessun_file() {
    let (dir, db) = copia_scenario();
    let s = Server::start(config(&db)).await;
    let c = s.client().await;
    call(&c, "elenco_semestri", Value::Null).await;
    c.cancel().await.unwrap();
    s.stop().await;
    assert!(!dir.path().join("omi.log").exists());
}

// ---------------------------------------------------------------------------------------------
// Concorrenza
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn chiamate_parallele_durante_un_import() {
    let (dir, db) = copia_scenario();
    let s = Server::start(config(&db)).await;
    let c = s.client().await;

    // Import di altri semestri sullo stesso database, in un thread a parte e in ciclo finché le
    // chiamate non sono finite. Le chiamate partono dopo il primo ciclo: almeno il secondo si
    // sovrappone a loro.
    let dir_path = dir.path().to_path_buf();
    let db_import = db.clone();
    let fine = Arc::new(AtomicBool::new(false));
    let fine_import = fine.clone();
    let (primo_tx, primo_rx) = oneshot::channel::<()>();
    let import = std::thread::spawn(move || {
        let mut conn = omi_core::db::open(&db_import, false).unwrap();
        let mut imp = omi_core::import::Importer::new(&conn).unwrap();
        let mut out = Vec::new();
        let mut primo = Some(primo_tx);
        let mut cicli = 0;
        while !fine_import.load(Ordering::SeqCst) {
            for sem in [20252, 20261, 20262] {
                let path = dir_path.join(format!("QIP_{sem}.zip"));
                support::write_zip(&path, &support::base().qip(sem));
                imp.import_zip(&mut conn, &path, &mut out).unwrap();
            }
            imp.completa(&mut conn, &mut out).unwrap();
            cicli += 1;
            if let Some(tx) = primo.take() {
                let _ = tx.send(());
            }
        }
        cicli
    });
    primo_rx.await.unwrap();

    let mut tasks = tokio::task::JoinSet::new();
    for i in 0..50 {
        let peer = c.peer().clone();
        tasks.spawn(async move {
            let lat = 41.901 + f64::from(i % 5) * 0.001;
            let p = CallToolRequestParams::new("zona_da_coordinate").with_arguments(
                json!({"latitudine": lat, "longitudine": 12.495})
                    .as_object()
                    .unwrap()
                    .clone(),
            );
            peer.call_tool(p).await.unwrap()
        });
    }
    let mut n = 0;
    while let Some(r) = tasks.join_next().await {
        let r = r.unwrap();
        assert_ne!(r.is_error, Some(true), "{}", testo(&r));
        assert!(testo(&r).starts_with(r#"{"trovato":true"#), "{}", testo(&r));
        n += 1;
    }
    assert_eq!(n, 50);
    fine.store(true, Ordering::SeqCst);
    assert!(import.join().unwrap() >= 2);

    // Dopo l'import il server vede i nuovi semestri.
    let (_, t) = call(&c, "elenco_semestri", Value::Null).await;
    assert!(t.starts_with(r#"{"semestri":[{"semestre":"2026/2""#), "{t}");
    c.cancel().await.unwrap();
    s.stop().await;
}
