// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! `omi-mcp`: server MCP sulle quotazioni OMI, avviato da console.

use std::path::PathBuf;
use std::process::ExitCode;

use omi_core::about::attribuzione;
use omi_mcp::auth::read_token_file;
use omi_mcp::server::{self, Config, DatabaseNonTrovato, parse_bind};

const USO: &str = "Uso: omi-mcp [database.db] [porta] [/l] [--bind INDIRIZZO] [--trust-proxy] [--token SEGRETO | --token-file FILE] [--allowed-host HOST ...]";

const AIUTO: &str = "\
Server MCP sulle quotazioni immobiliari OMI (Agenzia delle Entrate).

Uso:
  omi-mcp [database.db] [porta] [/l] [--bind INDIRIZZO] [--trust-proxy]
          [--token SEGRETO | --token-file FILE] [--allowed-host HOST ...]
  omi-mcp service install --db FILE [opzioni del servizio]   (solo Windows, come amministratore)
  omi-mcp service uninstall [--name NOME] [--keep-firewall]
  omi-mcp service start|stop|status [--name NOME]
  omi-mcp --help | -h | /?
  omi-mcp --version

  database.db          database creato con omi-import (predefinito: omi.db), aperto in sola lettura
  porta                porta TCP (predefinita: 8765; 0 = scelta dal sistema)
  /l, -l, --log        registra richieste HTTP e chiamate ai tool nel file <database>.log
                       (in Git Bash usare -l)
  --bind INDIRIZZO     indirizzo di ascolto (predefinito: tutte le interfacce, IPv4 e IPv6);
                       dietro un reverse proxy come Caddy: --bind 127.0.0.1
  --trust-proxy        nel log usa l'IP del client indicato da X-Forwarded-For
  --token SEGRETO      richiede \"Authorization: Bearer SEGRETO\" su /mcp (/ e /health restano liberi)
  --token-file FILE    come --token, con il segreto nella prima riga del file
                       (in alternativa: variabile d'ambiente OMI_MCP_TOKEN)
  --allowed-host HOST  accetta su /mcp solo le richieste con questo Host (oltre a localhost,
                       127.0.0.1 e ::1); ripetibile. Senza l'opzione l'Host non viene controllato.

Endpoint: /mcp (MCP su HTTP streamable), / (informazioni), /health (stato del database).

Opzioni del servizio Windows (service install):
  --db FILE            database (obbligatorio; percorso reso assoluto)
  --name NOME          nome del servizio (predefinito: OMIMyMCP; vale anche per gli altri comandi)
  --display-name TESTO nome visualizzato (predefinito: \"OMIMyMCP - Server MCP quotazioni OMI\")
  --port PORTA         porta TCP (predefinita: 8765)
  --bind INDIRIZZO     indirizzo di ascolto (predefinito: 127.0.0.1)
  --log, --trust-proxy come sopra
  --token-file FILE    file con il token Bearer, leggibile dall'account del servizio
  --start auto|delayed|manual
                       avvio automatico (predefinito), automatico ritardato o manuale
  --account localservice|networkservice|localsystem
                       account del servizio (predefinito: localservice)
  --firewall           aggiunge una regola in ingresso nel firewall di Windows
  --no-acl             non concede all'account il permesso di modifica sulla cartella del database
  --now                avvia subito il servizio e verifica /health
  --keep-firewall      (uninstall) lascia la regola del firewall
Il file di log <database>.log registra sempre avvio, arresto ed errori del servizio.";

#[cfg(not(windows))]
const SOLO_WINDOWS: &str = "Disponibile solo su Windows: usare l'unità systemd in deploy/linux.";

#[derive(Debug)]
enum Comando {
    Aiuto,
    Versione,
    Avvia(Config),
}

fn parametro_non_valido(arg: &str) -> String {
    format!("Parametro non valido: {arg}\n{USO}")
}

/// `--help` e `--version` hanno la precedenza anche su `service`.
fn aiuto_o_versione(args: &[String]) -> bool {
    args.iter()
        .any(|a| matches!(a.as_str(), "--help" | "-h" | "/?" | "--version"))
}

/// Interpreta gli argomenti (senza il nome del programma). `env_token` è `OMI_MCP_TOKEN`.
fn parse_args(args: &[String], env_token: Option<String>) -> Result<Comando, String> {
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "--help" | "-h" | "/?"))
    {
        return Ok(Comando::Aiuto);
    }
    if args.iter().any(|a| a == "--version") {
        return Ok(Comando::Versione);
    }

    let mut config = Config::new(PathBuf::from("omi.db"));
    config.console = true;
    let mut token: Option<String> = None;
    let mut token_file: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let mut valore = |opzione: &str| {
            it.next()
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .ok_or_else(|| format!("Valore mancante per {opzione}\n{USO}"))
        };
        match arg.as_str() {
            "/l" | "/L" | "-l" | "-L" | "--log" => config.log = true,
            "--trust-proxy" => config.trust_proxy = true,
            "--bind" => {
                let v = valore("--bind")?;
                let ip = parse_bind(&v)
                    .ok_or_else(|| format!("Indirizzo non valido per --bind: {v}\n{USO}"))?;
                config.bind = Some(ip);
            }
            "--token" => token = Some(valore("--token")?),
            "--token-file" => token_file = Some(PathBuf::from(valore("--token-file")?)),
            "--allowed-host" => config.allowed_hosts.push(valore("--allowed-host")?),
            a if a.to_ascii_lowercase().ends_with(".db") => config.db_path = PathBuf::from(a),
            a => match a.parse::<u16>() {
                Ok(p) => config.port = p,
                Err(_) => return Err(parametro_non_valido(a)),
            },
        }
    }

    config.token = match (token, token_file) {
        (Some(_), Some(_)) => {
            return Err(format!(
                "Usare --token oppure --token-file, non entrambi.\n{USO}"
            ));
        }
        (Some(t), None) => Some(t.trim().to_string()),
        (None, Some(f)) => Some(read_token_file(&f).map_err(|e| format!("Errore: {e:#}"))?),
        (None, None) => env_token
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty()),
    };
    Ok(Comando::Avvia(config))
}

fn main() -> ExitCode {
    let mut args = Vec::new();
    for a in std::env::args_os().skip(1) {
        match a.into_string() {
            Ok(s) => args.push(s),
            Err(a) => {
                eprintln!("{}", parametro_non_valido(&a.to_string_lossy()));
                return ExitCode::from(1);
            }
        }
    }
    if args.first().is_some_and(|a| a == "service") && !aiuto_o_versione(&args) {
        #[cfg(windows)]
        return omi_mcp::service_windows::main(&args[1..]);
        #[cfg(not(windows))]
        {
            eprintln!("{SOLO_WINDOWS}");
            return ExitCode::from(1);
        }
    }
    let config = match parse_args(&args, std::env::var("OMI_MCP_TOKEN").ok()) {
        Ok(Comando::Aiuto) => {
            println!("{}\n{AIUTO}", attribuzione());
            return ExitCode::SUCCESS;
        }
        Ok(Comando::Versione) => {
            println!("{}", attribuzione());
            return ExitCode::SUCCESS;
        }
        Ok(Comando::Avvia(c)) => c,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Errore: impossibile avviare il runtime: {e}");
            return ExitCode::from(1);
        }
    };
    let shutdown = async {
        // Su Unix systemd (e `kill`) chiedono l'arresto con SIGTERM; Ctrl+C invia SIGINT.
        #[cfg(unix)]
        {
            use tokio::signal::unix::{SignalKind, signal};
            match signal(SignalKind::terminate()) {
                Ok(mut term) => {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {}
                        _ = term.recv() => {}
                    }
                }
                Err(_) => {
                    let _ = tokio::signal::ctrl_c().await;
                }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
    };
    match runtime.block_on(server::run(config, shutdown, None)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            match e.downcast_ref::<DatabaseNonTrovato>() {
                Some(d) => eprintln!("{d}"),
                None => eprintln!("Errore: {e:#}"),
            }
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn avvia(args: &[&str], env: Option<&str>) -> Config {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse_args(&args, env.map(String::from)) {
            Ok(Comando::Avvia(c)) => c,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    fn errore(args: &[&str]) -> String {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        match parse_args(&args, None) {
            Err(m) => m,
            other => panic!("{args:?}: atteso errore, ottenuto {other:?}"),
        }
    }

    #[test]
    fn predefiniti() {
        let c = avvia(&[], None);
        assert_eq!(c.db_path, PathBuf::from("omi.db"));
        assert_eq!(c.port, 8765);
        assert!(!c.log && !c.trust_proxy && c.console);
        assert_eq!(c.bind, None);
        assert_eq!(c.token, None);
        assert!(c.allowed_hosts.is_empty());
    }

    #[test]
    fn posizionali_e_opzioni() {
        let c = avvia(
            &[
                "9000",
                "C:\\dati\\OMI.DB",
                "/L",
                "--bind",
                "127.0.0.1",
                "--trust-proxy",
                "--allowed-host",
                "omi.example.org",
                "--allowed-host",
                "omi.lan:8443",
            ],
            None,
        );
        assert_eq!(c.port, 9000);
        assert_eq!(c.db_path, PathBuf::from("C:\\dati\\OMI.DB"));
        assert!(c.log && c.trust_proxy);
        assert_eq!(c.bind, Some("127.0.0.1".parse().unwrap()));
        assert_eq!(c.allowed_hosts, ["omi.example.org", "omi.lan:8443"]);
        for l in ["/l", "-l", "-L", "--log"] {
            assert!(avvia(&[l], None).log);
        }
        assert_eq!(avvia(&["0"], None).port, 0);
        assert_eq!(
            avvia(&["--bind", "[::1]"], None).bind,
            Some("::1".parse().unwrap())
        );
    }

    #[test]
    fn token() {
        assert_eq!(
            avvia(&["--token", "abc"], Some("env")).token.as_deref(),
            Some("abc")
        );
        assert_eq!(avvia(&[], Some(" env ")).token.as_deref(), Some("env"));
        assert_eq!(avvia(&[], Some("  ")).token, None);
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("token.txt");
        std::fs::write(&f, "dal-file\n").unwrap();
        let f = f.to_string_lossy().into_owned();
        assert_eq!(
            avvia(&["--token-file", &f], Some("env")).token.as_deref(),
            Some("dal-file")
        );
        assert!(errore(&["--token", "a", "--token-file", &f]).starts_with("Usare --token oppure"));
        assert!(errore(&["--token-file", "manca.txt"]).starts_with("Errore: impossibile leggere"));
    }

    #[test]
    fn errori() {
        assert_eq!(
            errore(&["70000"]),
            format!("Parametro non valido: 70000\n{USO}")
        );
        assert_eq!(
            errore(&["omi.sqlite"]),
            format!("Parametro non valido: omi.sqlite\n{USO}")
        );
        assert_eq!(
            errore(&["--bind"]),
            format!("Valore mancante per --bind\n{USO}")
        );
        assert_eq!(
            errore(&["--bind", "localhost"]),
            format!("Indirizzo non valido per --bind: localhost\n{USO}")
        );
        assert_eq!(
            errore(&["--token", ""]),
            format!("Valore mancante per --token\n{USO}")
        );
    }

    #[test]
    fn aiuto_e_versione_hanno_la_precedenza() {
        let a = |v: &[&str]| parse_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>(), None);
        assert!(matches!(a(&["xx", "/?"]), Ok(Comando::Aiuto)));
        assert!(matches!(a(&["-h"]), Ok(Comando::Aiuto)));
        assert!(matches!(a(&["xx", "--version"]), Ok(Comando::Versione)));
    }
}
