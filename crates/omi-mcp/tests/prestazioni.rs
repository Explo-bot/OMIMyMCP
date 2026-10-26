// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Prestazioni del server: avvia il binario `omi-mcp` sul database indicato da
//! `OMI_PROVA_DB`, fa una chiamata di prova per ogni strumento e poi 200 chiamate a
//! `zona_da_coordinate` (centri delle geometrie) e a `zone_comune` con le quotazioni. Riporta
//! media, p95 e massimo lato client e lato server (righe `TOOL` del log) e il picco di memoria.
//!
//! ```text
//! cargo test --release -p omi-mcp --test prestazioni -- --ignored --nocapture
//! ```

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use regex::Regex;
use rmcp::model::CallToolRequestParams;
use rmcp::transport::StreamableHttpClientTransport;
use rmcp::{RoleClient, ServiceExt, service::RunningService};
use serde_json::{Value, json};

const N: usize = 200;

/// Processo del server, terminato al drop.
struct Figlio(Child);

impl Drop for Figlio {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Avvia il server con il log su file e restituisce il processo e la porta.
fn avvia(db: &Path) -> (Figlio, u16) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_omi-mcp"))
        .args([db.to_str().unwrap(), "0", "-l", "--bind", "127.0.0.1"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let figlio = Figlio(child);
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let re = Regex::new(r"in ascolto su http://127\.0\.0\.1:(\d+)/mcp").unwrap();
    while let Ok(l) = rx.recv_timeout(Duration::from_secs(60)) {
        if let Some(c) = re.captures(&l) {
            // Il resto dell'output (righe di log a console) viene consumato dal thread.
            std::thread::spawn(move || while rx.recv().is_ok() {});
            return (figlio, c[1].parse().unwrap());
        }
    }
    panic!("server non avviato");
}

/// Picco della memoria residente del processo, in MB.
fn picco_memoria_mb(pid: u32) -> Option<f64> {
    if cfg!(windows) {
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!("(Get-Process -Id {pid}).PeakWorkingSet64"),
            ])
            .output()
            .ok()?;
        let byte: f64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
        Some(byte / (1024.0 * 1024.0))
    } else {
        let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
        let riga = status.lines().find(|l| l.starts_with("VmHWM:"))?;
        let kb: f64 = riga.split_whitespace().nth(1)?.parse().ok()?;
        Some(kb / 1024.0)
    }
}

/// Media, p95 e massimo di una serie di durate in millisecondi.
fn statistiche(v: &[f64]) -> String {
    if v.is_empty() {
        return "nessun dato".to_string();
    }
    let mut s = v.to_vec();
    s.sort_by(f64::total_cmp);
    let media = s.iter().sum::<f64>() / s.len() as f64;
    let p95 = s[(s.len() * 95).div_ceil(100) - 1];
    format!(
        "media {media:.2} ms, p95 {p95:.2} ms, max {:.2} ms (n = {})",
        s[s.len() - 1],
        s.len()
    )
}

fn campione<T: Clone>(v: &[T], k: usize) -> Vec<T> {
    if v.len() <= k {
        return v.to_vec();
    }
    let passo = v.len() / k;
    (0..k).map(|i| v[i * passo].clone()).collect()
}

async fn chiama(c: &RunningService<RoleClient, ()>, tool: &str, args: Value) -> (f64, bool) {
    let mut p = CallToolRequestParams::new(tool.to_string());
    if let Value::Object(m) = args {
        p = p.with_arguments(m);
    }
    let t = Instant::now();
    let r = c.call_tool(p).await.unwrap();
    (t.elapsed().as_secs_f64() * 1000.0, r.is_error == Some(true))
}

/// Durate lato server (ms) delle righe `TOOL {tool} …` del log scritte dopo `offset`.
fn durate_server(log: &Path, offset: usize, tool: &str) -> Vec<f64> {
    let testo = std::fs::read(log).unwrap_or_default();
    let testo = String::from_utf8_lossy(&testo[offset.min(testo.len())..]).into_owned();
    let re = Regex::new(&format!(
        r" TOOL {tool} .* -> ok, \d+ caratteri \((\d+) ms\)$"
    ))
    .unwrap();
    testo
        .lines()
        .filter_map(|l| re.captures(l).map(|c| c[1].parse().unwrap()))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "prestazioni: OMI_PROVA_DB=… cargo test --release -p omi-mcp --test prestazioni -- --ignored --nocapture"]
async fn prestazioni_server() {
    let Some(db) = std::env::var_os("OMI_PROVA_DB").map(PathBuf::from) else {
        println!("OMI_PROVA_DB non impostata: prova saltata");
        return;
    };
    let mb = std::fs::metadata(&db).unwrap().len() as f64 / (1024.0 * 1024.0);
    println!(
        "Database: {mb:.0} MB, profilo {}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );

    // Campioni dal database: centri delle geometrie dell'ultimo semestre con perimetri e comuni
    // dell'ultimo semestre.
    let (punti, comuni, roma) = {
        let conn = omi_core::db::open(&db, true).unwrap();
        let punti: Vec<(f64, f64)> = conn
            .prepare(
                "SELECT g.centro_lat, g.centro_lon FROM zona_geo zg JOIN geometria g ON g.id=zg.geometria_id
                 WHERE zg.semestre=(SELECT max(semestre) FROM semestre WHERE n_geometrie>0) ORDER BY zg.id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let comuni: Vec<String> = conn
            .prepare(
                "SELECT DISTINCT comune_amm FROM zona
                 WHERE semestre=(SELECT max(semestre) FROM semestre WHERE n_zone>0) ORDER BY comune_amm",
            )
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        let roma = comuni.iter().any(|c| c == "H501");
        (campione(&punti, N), campione(&comuni, N), roma)
    };
    assert!(
        !punti.is_empty() && !comuni.is_empty(),
        "database senza dati"
    );

    let log = db.with_extension("log");
    let avvio = Instant::now();
    let (figlio, porta) = avvia(&db);
    println!(
        "Avvio del server: {:.0} ms",
        avvio.elapsed().as_secs_f64() * 1000.0
    );
    let pid = figlio.0.id();
    if let Some(m) = picco_memoria_mb(pid) {
        println!("Memoria dopo l'avvio (picco): {m:.1} MB");
    }

    let c = ()
        .serve(StreamableHttpClientTransport::from_uri(format!(
            "http://127.0.0.1:{porta}/mcp"
        )))
        .await
        .unwrap();

    // Una chiamata di prova per strumento.
    let comune = if roma {
        "H501".to_string()
    } else {
        comuni[0].clone()
    };
    let (lat, lon) = punti[0];
    let zona = {
        let v: Value = serde_json::from_str(
            &c.call_tool(
                CallToolRequestParams::new("zone_comune")
                    .with_arguments(json!({"comune": comune}).as_object().unwrap().clone()),
            )
            .await
            .unwrap()
            .content[0]
                .as_text()
                .unwrap()
                .text,
        )
        .unwrap();
        v["zone"][0]["info"]["zona"].as_str().unwrap().to_string()
    };
    println!("Una chiamata per strumento (comune {comune}, zona {zona}):");
    for (tool, args) in [
        ("elenco_semestri", json!({})),
        ("elenco_tipologie", json!({})),
        ("cerca_comuni", json!({"testo": comune})),
        (
            "zone_comune",
            json!({"comune": comune, "includi_valori": true, "includi_perimetri": true}),
        ),
        (
            "dettaglio_zona",
            json!({"comune": comune, "zona": zona, "includi_geometria": true}),
        ),
        (
            "zona_da_coordinate",
            json!({"latitudine": lat, "longitudine": lon}),
        ),
        (
            "zone_vicine",
            json!({"latitudine": lat, "longitudine": lon, "raggio_m": 5000}),
        ),
        ("storico_zona", json!({"comune": comune, "zona": zona})),
        ("statistiche_comune", json!({"comune": comune})),
    ] {
        let (ms, errore) = chiama(&c, tool, args).await;
        println!(
            "  {tool}: {ms:.1} ms{}",
            if errore { " (ERRORE)" } else { "" }
        );
        assert!(!errore, "{tool} ha restituito un errore");
    }

    // 200 chiamate ciascuno; il log si legge da qui in poi.
    let offset = std::fs::metadata(&log).map_or(0, |m| m.len() as usize);
    let mut zdc = Vec::new();
    for &(lat, lon) in &punti {
        let (ms, errore) = chiama(
            &c,
            "zona_da_coordinate",
            json!({"latitudine": lat, "longitudine": lon}),
        )
        .await;
        assert!(!errore);
        zdc.push(ms);
    }
    let mut zc = Vec::new();
    for comune in &comuni {
        let (ms, errore) = chiama(
            &c,
            "zone_comune",
            json!({"comune": comune, "includi_valori": true}),
        )
        .await;
        assert!(!errore);
        zc.push(ms);
    }
    let picco = picco_memoria_mb(pid);
    c.cancel().await.unwrap();
    drop(figlio);

    println!("zona_da_coordinate, {} punti:", zdc.len());
    println!("  client: {}", statistiche(&zdc));
    println!(
        "  server: {}",
        statistiche(&durate_server(&log, offset, "zona_da_coordinate"))
    );
    println!("zone_comune con valori, {} comuni:", zc.len());
    println!("  client: {}", statistiche(&zc));
    println!(
        "  server: {}",
        statistiche(&durate_server(&log, offset, "zone_comune"))
    );
    if let Some(m) = picco {
        println!("Picco di memoria del server: {m:.1} MB");
    }
}
