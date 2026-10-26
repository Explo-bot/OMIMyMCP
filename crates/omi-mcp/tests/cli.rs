// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Test della riga di comando di `omi-mcp`.

#[path = "../../omi-core/tests/support/mod.rs"]
mod support;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use assert_cmd::cargo::{cargo_bin, cargo_bin_cmd};
use omi_core::about::attribuzione;
use regex::Regex;

fn run(args: &[&str]) -> Output {
    cargo_bin_cmd!("omi-mcp").args(args).output().unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

#[test]
fn database_mancante() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("manca.db");
    let o = run(&[db.to_str().unwrap()]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        stderr(&o),
        format!(
            "Database non trovato: {}. Crearlo con omi-import.\n",
            db.display()
        )
    );
    // Il server non crea mai il database.
    assert!(!db.exists());
}

#[test]
fn versione_aiuto_e_parametri_non_validi() {
    let o = run(&["--version"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(stdout(&o), format!("{}\n", attribuzione()));

    let o = run(&["/?"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).contains("omi-mcp --help | -h | /?"));

    let o = run(&["omi.db", "abc"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).starts_with("Parametro non valido: abc\nUso: omi-mcp"));
}

/// Processo del server, terminato al drop anche se il test fallisce (altrimenti resterebbe vivo
/// con le pipe ereditate aperte).
struct Figlio(Child);

impl Drop for Figlio {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Avvia `omi-mcp` sul database con `-l` e porta 0; restituisce il processo, le righe del banner
/// fino a "Ctrl+C per terminare." e il canale con le righe successive. Con `chiudi_stdout` la
/// pipe di stdout viene chiusa subito dopo il banner.
fn avvia(db: &Path, chiudi_stdout: bool) -> (Figlio, Vec<String>, mpsc::Receiver<String>) {
    let mut child = Command::new(cargo_bin!("omi-mcp"))
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
            let fine = line == "Ctrl+C per terminare.";
            if tx.send(line).is_err() || (fine && chiudi_stdout) {
                break;
            }
        }
    });
    let mut righe = Vec::new();
    while let Ok(l) = rx.recv_timeout(Duration::from_secs(30)) {
        let fine = l == "Ctrl+C per terminare.";
        righe.push(l);
        if fine {
            break;
        }
    }
    (figlio, righe, rx)
}

/// `GET {path}` grezzo: restituisce la risposta completa.
fn get(porta: u16, path: &str) -> String {
    let mut s = TcpStream::connect(("127.0.0.1", porta)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut risposta = String::new();
    s.read_to_string(&mut risposta).unwrap();
    risposta
}

fn porta(riga: &str) -> u16 {
    let re = Regex::new(r"^Server MCP in ascolto su http://127\.0\.0\.1:(\d+)/mcp$").unwrap();
    re.captures(riga).expect(riga)[1].parse().unwrap()
}

#[test]
fn avvio_a_console() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::scenario_server(dir.path());
    let (_figlio, righe, rx) = avvia(&db, false);

    let db_abs = std::path::absolute(&db).unwrap();
    assert_eq!(righe[0], attribuzione(), "{righe:#?}");
    assert_eq!(righe[1], "Fonte dei dati: Agenzia Entrate - OMI");
    assert_eq!(righe[2], format!("Database: {}", db_abs.display()));
    assert_eq!(righe[3], "Contenuto del database:");
    assert_eq!(
        righe[5],
        "  Ultimo semestre caricato: 2025/1 (elaborazione Agenzia del 12/03/2026)"
    );
    assert_eq!(
        righe[6],
        "  Semestri disponibili:     2 (2024/2 - 2025/1), 1 con perimetri"
    );
    assert_eq!(
        righe[7],
        "  Comuni censiti:           5 nell'ultimo semestre (6 zone), 5 in totale nella serie storica"
    );
    let porta = porta(&righe[8]);
    assert_ne!(porta, 0);
    let re =
        Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} Log attivo, file: (.+)$").unwrap();
    assert_eq!(
        &re.captures(&righe[9]).expect(&righe[9])[1],
        db_abs.with_extension("log").display().to_string()
    );
    assert_eq!(righe[10], "Ctrl+C per terminare.");
    assert_eq!(righe.len(), 11);

    // Il server risponde sulla porta stampata e la richiesta compare nel log a console.
    let risposta = get(porta, "/health");
    assert!(risposta.starts_with("HTTP/1.1 200"), "{risposta}");
    assert!(risposta.ends_with("ok"), "{risposta}");
    let riga = rx.recv_timeout(Duration::from_secs(30)).unwrap();
    let re = Regex::new(
        r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} HTTP 127\.0\.0\.1 GET /health -> 200 \(\d+ ms\)$",
    )
    .unwrap();
    assert!(re.is_match(&riga), "{riga}");
}

#[test]
fn console_chiusa_non_blocca_il_server() {
    let dir = tempfile::tempdir().unwrap();
    let db = support::scenario_server(dir.path());
    let (_figlio, righe, _rx) = avvia(&db, true);
    let porta = porta(&righe[8]);
    // Con /l ogni richiesta scrive su uno stdout ormai chiuso: il server deve continuare a rispondere.
    std::thread::sleep(Duration::from_millis(200));
    for _ in 0..3 {
        let risposta = get(porta, "/health");
        assert!(risposta.starts_with("HTTP/1.1 200"), "{risposta}");
    }
    // Le righe vanno comunque nel file di log.
    let log = std::fs::read_to_string(db.with_extension("log")).unwrap();
    assert_eq!(log.matches("GET /health -> 200").count(), 3, "{log}");
}

#[cfg(not(windows))]
#[test]
fn service_solo_su_windows() {
    let o = run(&["service", "install", "--db", "omi.db"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        stderr(&o),
        "Disponibile solo su Windows: usare l'unità systemd in deploy/linux.\n"
    );
}

/// I comandi che seguono non richiedono privilegi né modificano il sistema.
#[cfg(windows)]
#[test]
fn service_senza_privilegi() {
    // `run` lanciato da console: il dispatcher non trova il Service Control Manager.
    let o = run(&["service", "run", "--db", "omi.db"]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        stderr(&o),
        "Questo comando è riservato al Service Control Manager. Per la console usare: omi-mcp <db> <porta>.\n"
    );

    let nome = format!("OMIMyMCP-Inesistente-{}", std::process::id());
    let o = run(&["service", "status", "--name", &nome]);
    assert_eq!(o.status.code(), Some(1));
    assert_eq!(
        stderr(&o),
        format!("Il servizio {nome} non è installato.\n")
    );

    let o = run(&["service", "uninstall", "--name", &nome]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(
        stdout(&o),
        format!("Il servizio {nome} non è installato.\n")
    );

    let o = run(&["service", "riavvia"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(stderr(&o).starts_with("Parametro non valido: riavvia\nUso: omi-mcp service"));

    // L'aiuto ha la precedenza e descrive i sottocomandi, ma non `run`.
    let o = run(&["service", "install", "--help"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).contains("omi-mcp service uninstall [--name NOME] [--keep-firewall]"));
    assert!(!stdout(&o).contains("service run"));
}
