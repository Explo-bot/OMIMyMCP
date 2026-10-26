// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Log del server: righe `{yyyy-MM-dd HH:mm:ss.fff} {messaggio}` in ora locale, su
//! console (se c'è) e sul file `<database>.log` in append. Le scritture sono serializzate da un mutex;
//! un errore di scrittura viene ignorato e non blocca mai il server.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;

pub struct Logger {
    console: bool,
    /// Righe `HTTP` e `TOOL` attive (opzione `/l`).
    traffico: bool,
    path: Option<PathBuf>,
    file: Mutex<Option<File>>,
}

impl Logger {
    /// `file`: percorso del log (aperto in append e creato se manca); `console`: righe anche su
    /// stdout/stderr; `traffico`: righe `HTTP` e `TOOL`.
    pub fn new(file: Option<PathBuf>, console: bool, traffico: bool) -> Logger {
        let handle = file.as_ref().and_then(|p| {
            match OpenOptions::new().create(true).append(true).open(p) {
                Ok(f) => Some(f),
                Err(e) => {
                    if console {
                        let _ = writeln!(
                            io::stderr(),
                            "{} Impossibile aprire il file di log {}: {e}",
                            timestamp(),
                            p.display()
                        );
                    }
                    None
                }
            }
        });
        Logger {
            console,
            traffico,
            path: file,
            file: Mutex::new(handle),
        }
    }

    /// Logger muto (per i test).
    pub fn silent() -> Logger {
        Logger::new(None, false, false)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn traffic_enabled(&self) -> bool {
        self.traffico
    }

    /// Riga informativa (avvio, arresto): su stdout e su file.
    pub fn info(&self, msg: &str) {
        self.write(msg, false);
    }

    /// Riga `HTTP` o `TOOL`: solo con `/l`.
    pub fn traffic(&self, msg: &str) {
        if self.traffico {
            self.write(msg, false);
        }
    }

    /// Errore interno: sempre registrato, su stderr (a console) e su file se c'è.
    pub fn error(&self, msg: &str) {
        self.write(msg, true);
    }

    fn write(&self, msg: &str, errore: bool) {
        let line = format!("{} {msg}\n", timestamp());
        let mut file = self.file.lock().unwrap_or_else(PoisonError::into_inner);
        // Niente println!/eprintln!: andrebbero in panic con la console chiusa.
        if self.console {
            let _ = if errore {
                io::stderr().write_all(line.as_bytes())
            } else {
                io::stdout().write_all(line.as_bytes())
            };
        }
        // Una sola scrittura per riga: nel servizio due logger scrivono in append sullo stesso file.
        if let Some(f) = file.as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

fn timestamp() -> String {
    chrono::Local::now()
        .format("%Y-%m-%d %H:%M:%S%.3f")
        .to_string()
}

/// Lunghezza di un testo in unità UTF-16, come richiesto per la riga `TOOL`.
pub fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// IP del client per la riga `HTTP`: un IPv4 mappato su IPv6 si mostra come IPv4; con
/// `--trust-proxy` vale il primo valore di `X-Forwarded-For`, seguito da `(via {peer})`.
pub fn client_ip(peer: SocketAddr, forwarded_for: Option<&str>, trust_proxy: bool) -> String {
    let peer_ip = peer.ip().to_canonical();
    let inoltrato = forwarded_for
        .filter(|_| trust_proxy)
        .and_then(|v| v.split(',').next())
        .map(str::trim)
        .filter(|v| !v.is_empty());
    match inoltrato {
        Some(ip) => format!("{ip} (via {peer_ip})"),
        None => peer_ip.to_string(),
    }
}

/// Stato del middleware di log HTTP.
pub struct HttpLog {
    pub logger: Arc<Logger>,
    pub trust_proxy: bool,
}

/// Middleware: `HTTP {ip client} {metodo} {percorso} -> {status} ({ms} ms)`.
pub async fn log_http(
    State(st): State<Arc<HttpLog>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    if !st.logger.traffic_enabled() {
        return next.run(req).await;
    }
    let inizio = Instant::now();
    let metodo = req.method().clone();
    let percorso = req.uri().path().to_string();
    let forwarded_for = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(String::from);
    let resp = next.run(req).await;
    st.logger.traffic(&format!(
        "HTTP {} {metodo} {percorso} -> {} ({} ms)",
        client_ip(peer, forwarded_for.as_deref(), st.trust_proxy),
        resp.status().as_u16(),
        inizio.elapsed().as_millis()
    ));
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lunghezza_utf16() {
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("città €"), 7);
        // Un carattere fuori dal piano di base vale due unità.
        assert_eq!(utf16_len("𝄞"), 2);
    }

    #[test]
    fn ip_del_client() {
        let v4: SocketAddr = "127.0.0.1:5000".parse().unwrap();
        let mapped: SocketAddr = "[::ffff:192.168.1.7]:5000".parse().unwrap();
        let v6: SocketAddr = "[::1]:5000".parse().unwrap();
        assert_eq!(client_ip(v4, None, false), "127.0.0.1");
        assert_eq!(client_ip(mapped, None, false), "192.168.1.7");
        assert_eq!(client_ip(v6, None, false), "::1");
        // X-Forwarded-For si usa solo con --trust-proxy.
        assert_eq!(client_ip(v4, Some("203.0.113.9"), false), "127.0.0.1");
        assert_eq!(
            client_ip(v4, Some(" 203.0.113.9 , 10.0.0.1"), true),
            "203.0.113.9 (via 127.0.0.1)"
        );
        assert_eq!(client_ip(v4, Some(" "), true), "127.0.0.1");
    }

    #[test]
    fn righe_su_file_in_append() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("omi.log");
        std::fs::write(&path, "riga precedente\n").unwrap();
        let log = Logger::new(Some(path.clone()), false, false);
        log.traffic("TOOL ignorato senza /l");
        log.info("avvio");
        log.error("errore interno");
        drop(log);
        let testo = std::fs::read_to_string(&path).unwrap();
        let righe: Vec<&str> = testo.lines().collect();
        assert_eq!(righe.len(), 3);
        assert_eq!(righe[0], "riga precedente");
        let re = regex::Regex::new(r"^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}\.\d{3} ").unwrap();
        assert!(re.is_match(righe[1]) && righe[1].ends_with(" avvio"));
        assert!(re.is_match(righe[2]) && righe[2].ends_with(" errore interno"));
    }

    #[test]
    fn file_non_apribile_non_blocca() {
        let dir = tempfile::tempdir().unwrap();
        // Una cartella al posto del file: l'apertura fallisce, le scritture vengono ignorate.
        let log = Logger::new(Some(dir.path().to_path_buf()), false, true);
        log.traffic("HTTP 127.0.0.1 GET / -> 200 (0 ms)");
        log.error("niente panic");
    }
}
