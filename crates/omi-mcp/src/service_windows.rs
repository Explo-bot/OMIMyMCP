// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! `omi-mcp service …`: installazione, avvio, arresto, stato e disinstallazione del servizio Windows,
//! più il punto d'ingresso `service run` usato dal Service Control Manager. Il servizio esegue lo
//! stesso [`server::run`] della console.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use omi_core::about::{FONTE_DATI, attribuzione};
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use windows_service::service::{
    Service, ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
    ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, service_dispatcher};
use windows_sys::Win32::Foundation::{
    ERROR_ACCESS_DENIED, ERROR_FAILED_SERVICE_CONTROLLER_CONNECT, ERROR_SERVICE_ALREADY_RUNNING,
    ERROR_SERVICE_DOES_NOT_EXIST, ERROR_SERVICE_MARKED_FOR_DELETE, ERROR_SERVICE_NOT_ACTIVE,
};
use windows_sys::Win32::System::Services::{
    QueryServiceConfig2W, SERVICE_CONFIG_DELAYED_AUTO_START_INFO, SERVICE_DELAYED_AUTO_START_INFO,
};

use crate::auth::read_token_file;
use crate::log::Logger;
use crate::server::{
    self, Config, DatabaseNonTrovato, PORTA_DEFAULT, PortaNonDisponibile, mcp_url, parse_bind,
};

const NOME_DEFAULT: &str = "OMIMyMCP";
const DISPLAY_NAME_DEFAULT: &str = "OMIMyMCP - Server MCP quotazioni OMI";
const USO: &str =
    "Uso: omi-mcp service install|uninstall|start|stop|status [opzioni] (dettagli: omi-mcp --help)";
const MSG_ADMIN: &str = "Servono i privilegi di amministratore: aprire il Prompt dei comandi o PowerShell con \"Esegui come amministratore\".";
const MSG_SCM: &str = "Questo comando è riservato al Service Control Manager. Per la console usare: omi-mcp <db> <porta>.";
/// Attesa massima dei cambi di stato (avvio, arresto, eliminazione).
const ATTESA: Duration = Duration::from_secs(30);
/// Dopo la richiesta di arresto, tempo concesso alle richieste in corso.
const ATTESA_ARRESTO: Duration = Duration::from_secs(20);
/// Codici d'uscita del servizio (`ServiceSpecific`).
const USCITA_DATABASE: u32 = 1;
const USCITA_PORTA: u32 = 2;
const USCITA_ALTRO: u32 = 3;

/// Il processo non ha i privilegi di amministratore (`ERROR_ACCESS_DENIED` dal SCM).
#[derive(Debug)]
struct ServonoPrivilegi;

impl fmt::Display for ServonoPrivilegi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(MSG_ADMIN)
    }
}

impl std::error::Error for ServonoPrivilegi {}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Avvio {
    Auto,
    Delayed,
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Account {
    LocalService,
    NetworkService,
    LocalSystem,
}

impl Account {
    /// Nome per `ServiceInfo::account_name` (`None` = LocalSystem).
    fn nome(self) -> Option<&'static str> {
        match self {
            Account::LocalService => Some(r"NT AUTHORITY\LocalService"),
            Account::NetworkService => Some(r"NT AUTHORITY\NetworkService"),
            Account::LocalSystem => None,
        }
    }

    /// SID a cui concedere la modifica sulla cartella del database (LocalSystem ha già accesso).
    fn sid(self) -> Option<&'static str> {
        match self {
            Account::LocalService => Some("S-1-5-19"),
            Account::NetworkService => Some("S-1-5-20"),
            Account::LocalSystem => None,
        }
    }

    fn descrizione(self) -> &'static str {
        self.nome().unwrap_or("LocalSystem")
    }
}

/// Argomenti di `service run`: sono anche quelli registrati nel servizio da `install`.
#[derive(Debug, Clone, PartialEq)]
struct RunArgs {
    name: String,
    db: PathBuf,
    port: u16,
    bind: Option<IpAddr>,
    log: bool,
    trust_proxy: bool,
    token_file: Option<PathBuf>,
}

impl RunArgs {
    /// `launch_arguments` del servizio.
    fn to_args(&self) -> Vec<OsString> {
        let mut v: Vec<OsString> = vec![
            "service".into(),
            "run".into(),
            "--name".into(),
            self.name.clone().into(),
            "--db".into(),
            self.db.clone().into_os_string(),
            "--port".into(),
            self.port.to_string().into(),
        ];
        if let Some(ip) = self.bind {
            v.push("--bind".into());
            v.push(ip.to_string().into());
        }
        if self.log {
            v.push("--log".into());
        }
        if self.trust_proxy {
            v.push("--trust-proxy".into());
        }
        if let Some(f) = &self.token_file {
            v.push("--token-file".into());
            v.push(f.clone().into_os_string());
        }
        v
    }

    fn log_path(&self) -> PathBuf {
        self.db.with_extension("log")
    }

    /// Indirizzo per la verifica di `/health`: l'IP di bind, oppure il loopback se è generico.
    fn health_addr(&self) -> SocketAddr {
        let ip = match self.bind {
            Some(IpAddr::V6(ip)) if ip.is_unspecified() => IpAddr::V6(Ipv6Addr::LOCALHOST),
            Some(ip) if !ip.is_unspecified() => ip,
            _ => IpAddr::V4(Ipv4Addr::LOCALHOST),
        };
        SocketAddr::new(ip, self.port)
    }

    fn mcp_url(&self) -> String {
        let ip = self.bind.unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED));
        mcp_url(self.bind, SocketAddr::new(ip, self.port))
    }

    /// Riga di configurazione per il log: il token non compare mai.
    fn descrizione(&self) -> String {
        let si_no = |b: bool| if b { "sì" } else { "no" };
        let bind = self
            .bind
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "tutte le interfacce".to_string());
        let token = match &self.token_file {
            Some(f) => format!("******** (dal file {})", f.display()),
            None => "nessuno".to_string(),
        };
        format!(
            "Configurazione: database {}, porta {}, bind {bind}, log HTTP/TOOL {}, trust-proxy {}, token {token}",
            self.db.display(),
            self.port,
            si_no(self.log),
            si_no(self.trust_proxy)
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Installa {
    run: RunArgs,
    display_name: String,
    start: Avvio,
    account: Account,
    firewall: bool,
    acl: bool,
    now: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Azione {
    Install(Installa),
    Uninstall { name: String, keep_firewall: bool },
    Start(String),
    Stop(String),
    Status(String),
    Run(RunArgs),
}

fn parametro_non_valido(arg: &str) -> String {
    format!("Parametro non valido: {arg}\n{USO}")
}

/// Interpreta gli argomenti che seguono `service`.
fn parse(args: &[String]) -> Result<Azione, String> {
    let Some((sub, resto)) = args.split_first() else {
        return Err(format!("Comando mancante dopo service.\n{USO}"));
    };
    let consentite: &[&str] = match sub.as_str() {
        "install" => &[
            "--name",
            "--display-name",
            "--db",
            "--port",
            "--bind",
            "--log",
            "--trust-proxy",
            "--token-file",
            "--start",
            "--account",
            "--firewall",
            "--no-acl",
            "--now",
        ],
        "uninstall" => &["--name", "--keep-firewall"],
        "start" | "stop" | "status" => &["--name"],
        "run" => &[
            "--name",
            "--db",
            "--port",
            "--bind",
            "--log",
            "--trust-proxy",
            "--token-file",
        ],
        altro => return Err(parametro_non_valido(altro)),
    };

    let mut name = NOME_DEFAULT.to_string();
    let mut display_name = DISPLAY_NAME_DEFAULT.to_string();
    let mut db: Option<PathBuf> = None;
    let mut port = PORTA_DEFAULT;
    // `install` ascolta di default solo sul loopback; `run` usa ciò che è registrato.
    let mut bind = (sub == "install").then_some(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let mut token_file: Option<PathBuf> = None;
    let mut start = Avvio::Auto;
    let mut account = Account::LocalService;
    let (mut log, mut trust_proxy, mut firewall, mut no_acl, mut now, mut keep_firewall) =
        (false, false, false, false, false, false);

    let mut it = resto.iter();
    while let Some(arg) = it.next() {
        if !consentite.contains(&arg.as_str()) {
            return Err(parametro_non_valido(arg));
        }
        let mut valore = || {
            it.next()
                .filter(|v| !v.trim().is_empty())
                .cloned()
                .ok_or_else(|| format!("Valore mancante per {arg}\n{USO}"))
        };
        match arg.as_str() {
            "--name" => {
                let v = valore()?;
                if v.contains(['"', '/', '\\']) {
                    return Err(format!("Nome del servizio non valido: {v}\n{USO}"));
                }
                name = v.trim().to_string();
            }
            "--display-name" => display_name = valore()?.trim().to_string(),
            "--db" => db = Some(PathBuf::from(valore()?)),
            "--port" => {
                let v = valore()?;
                port = v
                    .trim()
                    .parse()
                    .ok()
                    .filter(|p| *p != 0)
                    .ok_or_else(|| format!("Porta non valida: {v}\n{USO}"))?;
            }
            "--bind" => {
                let v = valore()?;
                bind = Some(
                    parse_bind(&v)
                        .ok_or_else(|| format!("Indirizzo non valido per --bind: {v}\n{USO}"))?,
                );
            }
            "--token-file" => token_file = Some(PathBuf::from(valore()?)),
            "--start" => {
                let v = valore()?;
                start = match v.trim().to_ascii_lowercase().as_str() {
                    "auto" => Avvio::Auto,
                    "delayed" => Avvio::Delayed,
                    "manual" => Avvio::Manual,
                    _ => return Err(format!("Valore non valido per --start: {v}\n{USO}")),
                };
            }
            "--account" => {
                let v = valore()?;
                account = match v.trim().to_ascii_lowercase().as_str() {
                    "localservice" => Account::LocalService,
                    "networkservice" => Account::NetworkService,
                    "localsystem" => Account::LocalSystem,
                    _ => return Err(format!("Valore non valido per --account: {v}\n{USO}")),
                };
            }
            "--log" => log = true,
            "--trust-proxy" => trust_proxy = true,
            "--firewall" => firewall = true,
            "--no-acl" => no_acl = true,
            "--now" => now = true,
            "--keep-firewall" => keep_firewall = true,
            // Ogni opzione consentita ha il suo ramo.
            altro => return Err(parametro_non_valido(altro)),
        }
    }

    let run = |db: Option<PathBuf>| -> Result<RunArgs, String> {
        Ok(RunArgs {
            name: name.clone(),
            db: db.ok_or_else(|| format!("Parametro obbligatorio mancante: --db\n{USO}"))?,
            port,
            bind,
            log,
            trust_proxy,
            token_file: token_file.clone(),
        })
    };
    Ok(match sub.as_str() {
        "install" => Azione::Install(Installa {
            run: run(db)?,
            display_name,
            start,
            account,
            firewall,
            acl: !no_acl,
            now,
        }),
        "uninstall" => Azione::Uninstall {
            name,
            keep_firewall,
        },
        "start" => Azione::Start(name),
        "stop" => Azione::Stop(name),
        "status" => Azione::Status(name),
        _ => Azione::Run(run(db)?),
    })
}

/// Punto d'ingresso di `omi-mcp service …` (argomenti dopo `service`).
pub fn main(args: &[String]) -> ExitCode {
    let azione = match parse(args) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::from(1);
        }
    };
    let esito = match azione {
        Azione::Install(o) => install(o),
        Azione::Uninstall {
            name,
            keep_firewall,
        } => uninstall(&name, keep_firewall),
        Azione::Start(name) => start(&name),
        Azione::Stop(name) => stop(&name),
        Azione::Status(name) => status(&name),
        Azione::Run(r) => run(r),
    };
    match esito {
        Ok(code) => code,
        Err(e) => {
            if let Some(p) = e.downcast_ref::<ServonoPrivilegi>() {
                eprintln!("{p}");
            } else if let Some(d) = e.downcast_ref::<DatabaseNonTrovato>() {
                eprintln!("{d}");
            } else {
                eprintln!("Errore: {e:#}");
            }
            ExitCode::from(1)
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Service Control Manager

fn codice(e: &windows_service::Error) -> Option<u32> {
    match e {
        windows_service::Error::Winapi(io) => io.raw_os_error().map(|c| c as u32),
        _ => None,
    }
}

/// Errore del SCM con contesto; `ERROR_ACCESS_DENIED` diventa [`ServonoPrivilegi`].
fn errore_scm(e: windows_service::Error, cosa: String) -> anyhow::Error {
    if codice(&e) == Some(ERROR_ACCESS_DENIED) {
        return ServonoPrivilegi.into();
    }
    match e {
        windows_service::Error::Winapi(io) => anyhow::Error::new(io).context(cosa),
        altro => anyhow::Error::new(altro).context(cosa),
    }
}

fn scm(access: ServiceManagerAccess) -> anyhow::Result<ServiceManager> {
    ServiceManager::local_computer(None::<&str>, access)
        .map_err(|e| errore_scm(e, "impossibile aprire il Service Control Manager".into()))
}

/// Apre il servizio; `None` se non esiste.
fn apri(
    manager: &ServiceManager,
    name: &str,
    access: ServiceAccess,
) -> anyhow::Result<Option<Service>> {
    match manager.open_service(name, access) {
        Ok(s) => Ok(Some(s)),
        Err(e) if codice(&e) == Some(ERROR_SERVICE_DOES_NOT_EXIST) => Ok(None),
        Err(e) => Err(errore_scm(
            e,
            format!("impossibile aprire il servizio {name}"),
        )),
    }
}

fn non_installato(name: &str) -> String {
    format!("Il servizio {name} non è installato.")
}

/// Avvio ritardato (non esposto da `windows-service`).
fn avvio_ritardato(service: &Service) -> bool {
    let mut info = SERVICE_DELAYED_AUTO_START_INFO {
        fDelayedAutostart: 0,
    };
    let mut necessari = 0u32;
    // SAFETY: il buffer è una SERVICE_DELAYED_AUTO_START_INFO della dimensione dichiarata, che è il
    // tipo restituito per questo livello; l'handle resta valido per tutta la durata di `service`.
    let ok = unsafe {
        QueryServiceConfig2W(
            service.raw_handle(),
            SERVICE_CONFIG_DELAYED_AUTO_START_INFO,
            (&mut info as *mut SERVICE_DELAYED_AUTO_START_INFO).cast::<u8>(),
            std::mem::size_of::<SERVICE_DELAYED_AUTO_START_INFO>() as u32,
            &mut necessari,
        )
    };
    ok != 0 && info.fDelayedAutostart != 0
}

fn descr_stato(s: ServiceState) -> &'static str {
    match s {
        ServiceState::Stopped => "fermo",
        ServiceState::StartPending => "in avvio",
        ServiceState::StopPending => "in arresto",
        ServiceState::Running => "in esecuzione",
        ServiceState::ContinuePending => "in ripresa",
        ServiceState::PausePending => "in sospensione",
        ServiceState::Paused => "sospeso",
    }
}

fn descr_uscita(c: ServiceExitCode) -> String {
    match c {
        ServiceExitCode::ServiceSpecific(USCITA_DATABASE) => {
            format!("codice {USCITA_DATABASE}: database non trovato")
        }
        ServiceExitCode::ServiceSpecific(USCITA_PORTA) => {
            format!("codice {USCITA_PORTA}: porta non disponibile")
        }
        ServiceExitCode::ServiceSpecific(n) => format!("codice {n}: errore del server"),
        ServiceExitCode::Win32(0) => "nessun errore".to_string(),
        ServiceExitCode::Win32(n) => format!(
            "errore di Windows {n}: {}",
            io::Error::from_raw_os_error(n as i32)
        ),
    }
}

/// Attende lo stato `obiettivo` (al massimo [`ATTESA`]). In attesa di `Running`, un servizio che
/// torna `Stopped` è un errore.
fn attendi(service: &Service, obiettivo: ServiceState) -> anyhow::Result<ServiceStatus> {
    let inizio = Instant::now();
    loop {
        let st = service
            .query_status()
            .map_err(|e| errore_scm(e, "impossibile leggere lo stato del servizio".into()))?;
        if st.current_state == obiettivo {
            return Ok(st);
        }
        if obiettivo == ServiceState::Running && st.current_state == ServiceState::Stopped {
            bail!(
                "il servizio si è fermato durante l'avvio ({}); i dettagli sono nel file di log",
                descr_uscita(st.exit_code)
            );
        }
        if inizio.elapsed() > ATTESA {
            bail!(
                "dopo {} s il servizio è ancora {}",
                ATTESA.as_secs(),
                descr_stato(st.current_state)
            );
        }
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn avvia(service: &Service) -> anyhow::Result<ServiceStatus> {
    match service.start::<&OsStr>(&[]) {
        Ok(()) => {}
        Err(e) if codice(&e) == Some(ERROR_SERVICE_ALREADY_RUNNING) => {}
        Err(e) => return Err(errore_scm(e, "impossibile avviare il servizio".into())),
    }
    attendi(service, ServiceState::Running)
}

fn ferma(service: &Service) -> anyhow::Result<()> {
    match service.stop() {
        Ok(_) => {}
        Err(e) if codice(&e) == Some(ERROR_SERVICE_NOT_ACTIVE) => {}
        Err(e) => return Err(errore_scm(e, "impossibile fermare il servizio".into())),
    }
    attendi(service, ServiceState::Stopped).map(|_| ())
}

/// Argomenti `service run …` registrati nel servizio (dalla riga di comando di `query_config`).
fn argomenti_registrati(riga: &OsStr) -> (String, Vec<String>, Option<RunArgs>) {
    let mut parti = split_command_line(&riga.to_string_lossy());
    let exe = if parti.is_empty() {
        String::new()
    } else {
        parti.remove(0)
    };
    let run = match parti.split_first() {
        Some((s, resto)) if s == "service" => match parse(resto) {
            Ok(Azione::Run(r)) => Some(r),
            _ => None,
        },
        _ => None,
    };
    (exe, parti, run)
}

/// Divide una riga di comando con le regole di `CommandLineToArgvW` (le stesse usate da
/// `windows-service` per comporla).
fn split_command_line(riga: &str) -> Vec<String> {
    let mut parti = Vec::new();
    let mut corrente = String::new();
    let mut in_parte = false;
    let mut tra_virgolette = false;
    let mut barre = 0usize;
    for c in riga.chars() {
        match c {
            '\\' => {
                barre += 1;
                in_parte = true;
            }
            '"' => {
                corrente.extend(std::iter::repeat_n('\\', barre / 2));
                if barre % 2 == 1 {
                    corrente.push('"');
                } else {
                    tra_virgolette = !tra_virgolette;
                }
                barre = 0;
                in_parte = true;
            }
            ' ' | '\t' if !tra_virgolette => {
                corrente.extend(std::iter::repeat_n('\\', barre));
                barre = 0;
                if in_parte {
                    parti.push(std::mem::take(&mut corrente));
                    in_parte = false;
                }
            }
            c => {
                corrente.extend(std::iter::repeat_n('\\', barre));
                barre = 0;
                corrente.push(c);
                in_parte = true;
            }
        }
    }
    corrente.extend(std::iter::repeat_n('\\', barre));
    if in_parte {
        parti.push(corrente);
    }
    parti
}

fn quota(s: &str) -> String {
    if s.is_empty() || s.contains([' ', '\t']) {
        format!("\"{s}\"")
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Comandi esterni (icacls, netsh) e verifica di /health

fn system32(programma: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    Path::new(&root).join("System32").join(programma)
}

/// Esegue un programma di sistema con la riga di argomenti indicata, la stampa e ne stampa l'esito.
fn esegui(programma: &str, argomenti: &str) -> bool {
    println!("> {} {argomenti}", programma.trim_end_matches(".exe"));
    let _ = io::stdout().flush();
    match Command::new(system32(programma))
        .raw_arg(argomenti)
        .status()
    {
        Ok(s) if s.success() => {
            println!("  esito: ok");
            true
        }
        Ok(s) => {
            println!("  esito: errore (codice {})", s.code().unwrap_or(-1));
            false
        }
        Err(e) => {
            println!("  esito: impossibile eseguire {programma}: {e}");
            false
        }
    }
}

/// Come [`esegui`], senza stampare nulla.
fn esegui_muto(programma: &str, argomenti: &str) -> bool {
    Command::new(system32(programma))
        .raw_arg(argomenti)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Percorso tra virgolette per una riga di comando; una barra finale chiuderebbe le virgolette.
fn tra_virgolette(p: &Path) -> String {
    let s = p.display().to_string();
    if s.ends_with('\\') {
        format!("\"{s}.\"")
    } else {
        format!("\"{s}\"")
    }
}

fn icacls_args(cartella: &Path, sid: &str) -> String {
    format!("{} /grant \"*{sid}:(OI)(CI)M\"", tra_virgolette(cartella))
}

fn firewall_add_args(name: &str, port: u16, exe: &Path) -> String {
    format!(
        "advfirewall firewall add rule name=\"{name}\" dir=in action=allow protocol=TCP localport={port} program={}",
        tra_virgolette(exe)
    )
}

/// Solo le regole con quel nome **e** quel programma: netsh confronta `name=` con il nome
/// visualizzato, che può appartenere anche a regole di altri programmi (per esempio quelle che
/// Windows crea quando l'utente consente l'accesso a un eseguibile con la stessa descrizione).
fn firewall_delete_args(name: &str, exe: &Path) -> String {
    format!(
        "advfirewall firewall delete rule name=\"{name}\" program={}",
        tra_virgolette(exe)
    )
}

/// `GET /health`: restituisce codice di stato e corpo della risposta.
fn health(addr: SocketAddr) -> io::Result<(u16, String)> {
    let timeout = Duration::from_secs(5);
    let mut s = TcpStream::connect_timeout(&addr, timeout)?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;
    // Una sola scrittura: una richiesta letta a metà farebbe chiudere la connessione con un reset.
    let richiesta = format!("GET /health HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n");
    s.write_all(richiesta.as_bytes())?;
    let mut buf = Vec::new();
    match s.take(64 * 1024).read_to_end(&mut buf) {
        Ok(_) => {}
        // Un reset dopo la risposta non cambia l'esito.
        Err(_) if !buf.is_empty() => {}
        Err(e) => return Err(e),
    }
    let testo = String::from_utf8_lossy(&buf);
    let status = testo
        .split_whitespace()
        .nth(1)
        .and_then(|c| c.parse().ok())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "risposta HTTP non valida"))?;
    let corpo = testo
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.trim().to_string())
        .unwrap_or_default();
    Ok((status, corpo))
}

/// Stampa l'esito di `/health`; `true` se la risposta è `200`.
fn stampa_health(run: &RunArgs) -> bool {
    let addr = run.health_addr();
    match health(addr) {
        Ok((200, corpo)) => {
            println!("Verifica: GET http://{addr}/health -> 200 {corpo}");
            true
        }
        Ok((status, corpo)) => {
            println!("Verifica: GET http://{addr}/health -> {status} {corpo}");
            false
        }
        Err(e) => {
            println!("Verifica: GET http://{addr}/health non riuscita: {e}");
            false
        }
    }
}

// ---------------------------------------------------------------------------------------------
// install, uninstall, start, stop, status

fn install(o: Installa) -> anyhow::Result<ExitCode> {
    // Primo passo: senza privilegi di amministratore il SCM nega CREATE_SERVICE.
    let manager = scm(ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE)?;
    let name = o.run.name.clone();

    let exe =
        std::env::current_exe().context("impossibile determinare il percorso dell'eseguibile")?;
    if let Some(profilo) = std::env::var_os("USERPROFILE") {
        let profilo = profilo.to_string_lossy().to_lowercase();
        if !profilo.is_empty() && exe.to_string_lossy().to_lowercase().starts_with(&profilo) {
            println!(
                "Attenzione: l'eseguibile si trova in una cartella utente ({}). Il servizio non deve dipendere dal Desktop o dal profilo: copiare i programmi in C:\\Program Files\\OMIMyMCP\\ (deploy\\windows\\install.ps1) e installare il servizio da lì.",
                exe.display()
            );
        }
    }
    let db = std::path::absolute(&o.run.db)
        .with_context(|| format!("percorso non valido: {}", o.run.db.display()))?;
    if !db.is_file() {
        return Err(DatabaseNonTrovato(db).into());
    }
    let token_file = match &o.run.token_file {
        Some(f) => {
            let f = std::path::absolute(f)
                .with_context(|| format!("percorso non valido: {}", f.display()))?;
            read_token_file(&f)?;
            Some(f)
        }
        None => None,
    };
    let run = RunArgs {
        db,
        token_file,
        ..o.run.clone()
    };

    if apri(&manager, &name, ServiceAccess::QUERY_STATUS)?.is_some() {
        bail!(
            "il servizio {name} è già installato: per reinstallarlo eseguire prima omi-mcp service uninstall --name {name}"
        );
    }
    let info = ServiceInfo {
        name: name.clone().into(),
        display_name: o.display_name.clone().into(),
        service_type: ServiceType::OWN_PROCESS,
        start_type: match o.start {
            Avvio::Auto | Avvio::Delayed => ServiceStartType::AutoStart,
            Avvio::Manual => ServiceStartType::OnDemand,
        },
        error_control: ServiceErrorControl::Normal,
        executable_path: exe.clone(),
        launch_arguments: run.to_args(),
        dependencies: vec![],
        account_name: o.account.nome().map(OsString::from),
        account_password: None,
    };
    let service = manager
        .create_service(
            &info,
            ServiceAccess::QUERY_STATUS
                | ServiceAccess::QUERY_CONFIG
                | ServiceAccess::CHANGE_CONFIG
                | ServiceAccess::START
                | ServiceAccess::STOP
                | ServiceAccess::DELETE,
        )
        .map_err(|e| errore_scm(e, format!("impossibile creare il servizio {name}")))?;
    if let Err(e) = configura(&service, o.start) {
        // Un servizio configurato a metà non deve restare registrato.
        let _ = service.delete();
        return Err(e);
    }
    println!("Servizio {name} creato ({}).", exe.display());

    let cartella = run.db.parent().unwrap_or(Path::new("."));
    match o.account.sid() {
        Some(sid) if o.acl => {
            println!(
                "Permesso di modifica sulla cartella del database per {}:",
                o.account.descrizione()
            );
            if !esegui("icacls.exe", &icacls_args(cartella, sid)) {
                println!(
                    "Attenzione: senza questo permesso il servizio non può creare {}-shm e il file di log.",
                    run.db.display()
                );
            }
        }
        _ => {}
    }

    let bind = run.bind.unwrap_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED));
    if o.firewall {
        println!("Regola del firewall di Windows:");
        esegui_muto("netsh.exe", &firewall_delete_args(&name, &exe));
        esegui("netsh.exe", &firewall_add_args(&name, run.port, &exe));
    } else if !bind.is_loopback() {
        println!(
            "Attenzione: il servizio ascolta su {bind}; il firewall di Windows può bloccare le connessioni dalla rete (aggiungere la regola con --firewall)."
        );
    }

    let mut esito = ExitCode::SUCCESS;
    if o.now {
        println!("Avvio del servizio {name}...");
        avvia(&service)?;
        println!("Servizio {name} in esecuzione.");
        if !stampa_health(&run) {
            esito = ExitCode::from(1);
        }
    }

    println!();
    println!("Servizio:          {name} ({})", o.display_name);
    println!("Account:           {}", o.account.descrizione());
    println!("URL MCP:           {}", run.mcp_url());
    if run.token_file.is_some() {
        println!("Autenticazione:    Authorization: Bearer <contenuto del file del token>");
    }
    println!("File di log:       {}", run.log_path().display());
    if !o.now {
        println!("Per avviarlo:      omi-mcp service start --name {name}");
    }
    println!("Per fermarlo:      omi-mcp service stop --name {name}");
    println!("Per disinstallarlo: omi-mcp service uninstall --name {name}");
    Ok(esito)
}

/// Avvio ritardato, descrizione e azioni in caso di errore.
fn configura(service: &Service, start: Avvio) -> anyhow::Result<()> {
    let err = |cosa: &str| {
        let cosa = cosa.to_string();
        move |e| errore_scm(e, cosa)
    };
    if start == Avvio::Delayed {
        service
            .set_delayed_auto_start(true)
            .map_err(err("impossibile impostare l'avvio ritardato"))?;
    }
    service
        .set_description(format!(
            "Server MCP sulle quotazioni immobiliari OMI. Fonte dei dati: {FONTE_DATI}. {}",
            attribuzione()
        ))
        .map_err(err("impossibile impostare la descrizione"))?;
    let riavvio = |s| ServiceAction {
        action_type: ServiceActionType::Restart,
        delay: Duration::from_secs(s),
    };
    service
        .update_failure_actions(ServiceFailureActions {
            reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 60 * 60)),
            reboot_msg: None,
            command: None,
            actions: Some(vec![riavvio(5), riavvio(30), riavvio(60)]),
        })
        .map_err(err("impossibile impostare le azioni in caso di errore"))?;
    // Anche un arresto con codice d'uscita diverso da 0 conta come errore.
    service
        .set_failure_actions_on_non_crash_failures(true)
        .map_err(err("impossibile impostare le azioni in caso di errore"))?;
    Ok(())
}

fn uninstall(name: &str, keep_firewall: bool) -> anyhow::Result<ExitCode> {
    let manager = scm(ServiceManagerAccess::CONNECT)?;
    let Some(service) = apri(
        &manager,
        name,
        ServiceAccess::QUERY_STATUS
            | ServiceAccess::QUERY_CONFIG
            | ServiceAccess::STOP
            | ServiceAccess::DELETE,
    )?
    else {
        println!("{}", non_installato(name));
        return Ok(ExitCode::SUCCESS);
    };
    let (exe, registrati) = match service.query_config() {
        Ok(c) => {
            let (exe, _, registrati) = argomenti_registrati(c.executable_path.as_os_str());
            (Some(exe).filter(|e| !e.is_empty()), registrati)
        }
        Err(_) => (None, None),
    };

    let st = service
        .query_status()
        .map_err(|e| errore_scm(e, "impossibile leggere lo stato del servizio".into()))?;
    if st.current_state != ServiceState::Stopped {
        println!("Arresto del servizio {name}...");
        ferma(&service)?;
        println!("Servizio {name} fermato.");
    }
    match service.delete() {
        Ok(()) => {}
        Err(e) if codice(&e) == Some(ERROR_SERVICE_MARKED_FOR_DELETE) => {}
        Err(e) => {
            return Err(errore_scm(
                e,
                format!("impossibile eliminare il servizio {name}"),
            ));
        }
    }
    drop(service);

    // L'eliminazione è effettiva quando si chiude l'ultimo handle al servizio.
    let inizio = Instant::now();
    let mut eliminato = false;
    while inizio.elapsed() <= ATTESA {
        if apri(&manager, name, ServiceAccess::QUERY_STATUS)?.is_none() {
            eliminato = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    if eliminato {
        println!("Servizio {name} eliminato.");
    } else {
        println!(
            "Il servizio {name} è segnato per l'eliminazione: sparirà quando verranno chiusi i programmi che lo tengono aperto (ad esempio la console Servizi) o al riavvio."
        );
    }

    if !keep_firewall {
        match &exe {
            Some(exe) if esegui_muto("netsh.exe", &firewall_delete_args(name, Path::new(exe))) => {
                println!("Regola del firewall {name} rimossa.");
            }
            Some(_) => println!("Nessuna regola del firewall {name} da rimuovere."),
            None => println!(
                "Regola del firewall {name} non rimossa: percorso del programma non ricavabile dalla configurazione del servizio."
            ),
        }
    }

    println!("Database, log e token NON vengono cancellati:");
    match registrati {
        Some(r) => {
            println!("  database: {}", r.db.display());
            println!("  log:      {}", r.log_path().display());
            if let Some(t) = &r.token_file {
                println!("  token:    {}", t.display());
            }
        }
        None => println!("  (percorsi non ricavabili dalla configurazione del servizio)"),
    }
    Ok(ExitCode::SUCCESS)
}

fn start(name: &str) -> anyhow::Result<ExitCode> {
    let manager = scm(ServiceManagerAccess::CONNECT)?;
    let Some(service) = apri(
        &manager,
        name,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS,
    )?
    else {
        eprintln!("{}", non_installato(name));
        return Ok(ExitCode::from(1));
    };
    println!("Avvio del servizio {name}...");
    avvia(&service)?;
    println!("Servizio {name} in esecuzione.");
    Ok(ExitCode::SUCCESS)
}

fn stop(name: &str) -> anyhow::Result<ExitCode> {
    let manager = scm(ServiceManagerAccess::CONNECT)?;
    let Some(service) = apri(
        &manager,
        name,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
    )?
    else {
        eprintln!("{}", non_installato(name));
        return Ok(ExitCode::from(1));
    };
    let st = service
        .query_status()
        .map_err(|e| errore_scm(e, "impossibile leggere lo stato del servizio".into()))?;
    if st.current_state == ServiceState::Stopped {
        println!("Il servizio {name} è già fermo.");
        return Ok(ExitCode::SUCCESS);
    }
    println!("Arresto del servizio {name}...");
    ferma(&service)?;
    println!("Servizio {name} fermato.");
    Ok(ExitCode::SUCCESS)
}

fn status(name: &str) -> anyhow::Result<ExitCode> {
    let manager = scm(ServiceManagerAccess::CONNECT)?;
    let Some(service) = apri(
        &manager,
        name,
        ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
    )?
    else {
        eprintln!("{}", non_installato(name));
        return Ok(ExitCode::from(1));
    };
    let st = service
        .query_status()
        .map_err(|e| errore_scm(e, "impossibile leggere lo stato del servizio".into()))?;
    let config = service.query_config().map_err(|e| {
        errore_scm(
            e,
            "impossibile leggere la configurazione del servizio".into(),
        )
    })?;
    let avvio = match config.start_type {
        ServiceStartType::AutoStart if avvio_ritardato(&service) => "automatico (ritardato)",
        ServiceStartType::AutoStart => "automatico",
        ServiceStartType::OnDemand => "manuale",
        ServiceStartType::Disabled => "disabilitato",
        ServiceStartType::SystemStart | ServiceStartType::BootStart => "di sistema",
    };
    let account = config
        .account_name
        .as_ref()
        .map(|a| a.to_string_lossy().into_owned())
        .unwrap_or_else(|| "LocalSystem".to_string());
    let (exe, argomenti, registrati) = argomenti_registrati(config.executable_path.as_os_str());

    println!(
        "Servizio:      {name} ({})",
        config.display_name.to_string_lossy()
    );
    println!("Stato:         {}", descr_stato(st.current_state));
    println!("Tipo di avvio: {avvio}");
    println!("Account:       {account}");
    println!("Percorso:      {exe}");
    println!(
        "Argomenti:     {}",
        if argomenti.is_empty() {
            "(nessuno)".to_string()
        } else {
            argomenti
                .iter()
                .map(|a| quota(a))
                .collect::<Vec<_>>()
                .join(" ")
        }
    );
    if let Some(pid) = st.process_id.filter(|p| *p != 0) {
        println!("PID:           {pid}");
    }
    if st.current_state == ServiceState::Stopped && st.exit_code != ServiceExitCode::NO_ERROR {
        println!("Ultima uscita: {}", descr_uscita(st.exit_code));
    }
    match &registrati {
        Some(r) => {
            println!("URL MCP:       {}", r.mcp_url());
            println!("File di log:   {}", r.log_path().display());
            stampa_health(r);
        }
        None => println!("Argomenti non riconosciuti: /health non verificato."),
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------------------------
// run: punto d'ingresso del Service Control Manager

/// Argomenti di `service run`, letti da `main` prima di passare il controllo al dispatcher.
static RUN_ARGS: OnceLock<RunArgs> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

fn run(args: RunArgs) -> anyhow::Result<ExitCode> {
    let name = args.name.clone();
    let _ = RUN_ARGS.set(args);
    match service_dispatcher::start(&name, ffi_service_main) {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(e) if codice(&e) == Some(ERROR_FAILED_SERVICE_CONTROLLER_CONNECT) => {
            eprintln!("{MSG_SCM}");
            Ok(ExitCode::from(1))
        }
        Err(e) => Err(errore_scm(
            e,
            "impossibile collegarsi al Service Control Manager".into(),
        )),
    }
}

fn stato(
    current_state: ServiceState,
    exit_code: ServiceExitCode,
    wait_hint: Duration,
) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state,
        controls_accepted: if current_state == ServiceState::Running {
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN
        } else {
            ServiceControlAccept::empty()
        },
        exit_code,
        checkpoint: 0,
        wait_hint,
        process_id: None,
    }
}

/// Eseguito dal dispatcher su un thread dedicato. Il log del ciclo di vita è sempre attivo.
fn service_main(_arguments: Vec<OsString>) {
    let Some(args) = RUN_ARGS.get().cloned() else {
        return;
    };
    let logger = Arc::new(Logger::new(Some(args.log_path()), false, false));
    let l = logger.clone();
    std::panic::set_hook(Box::new(move |info| l.error(&format!("PANIC: {info}"))));
    if let Err(e) = esegui_servizio(&args, &logger) {
        logger.error(&format!("Errore del servizio {}: {e:#}", args.name));
    }
}

fn esegui_servizio(args: &RunArgs, logger: &Arc<Logger>) -> windows_service::Result<()> {
    let cancel = CancellationToken::new();
    let c = cancel.clone();
    let l = logger.clone();
    let handler = move |evento| match evento {
        ServiceControl::Stop | ServiceControl::Shutdown => {
            l.info("Richiesta di arresto dal Service Control Manager");
            c.cancel();
            ServiceControlHandlerResult::NoError
        }
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let handle = service_control_handler::register(&args.name, handler)?;
    handle.set_service_status(stato(
        ServiceState::StartPending,
        ServiceExitCode::NO_ERROR,
        ATTESA,
    ))?;
    logger.info(&format!(
        "Avvio del servizio {} - {}",
        args.name,
        attribuzione()
    ));
    logger.info(&args.descrizione());

    let uscita = esegui_server(args, logger, &cancel, &|s| {
        let _ = handle.set_service_status(s);
    });

    handle.set_service_status(stato(ServiceState::Stopped, uscita, Duration::ZERO))?;
    Ok(())
}

/// Esegue il server finché il SCM non chiede l'arresto. Restituisce il codice d'uscita del servizio.
fn esegui_server(
    args: &RunArgs,
    logger: &Logger,
    cancel: &CancellationToken,
    segnala: &dyn Fn(ServiceStatus),
) -> ServiceExitCode {
    let errore = |msg: String| {
        logger.error(&msg);
        ServiceExitCode::ServiceSpecific(USCITA_ALTRO)
    };
    let token = match &args.token_file {
        Some(f) => match read_token_file(f) {
            Ok(t) => Some(t),
            Err(e) => return errore(format!("Errore: {e:#}")),
        },
        None => None,
    };
    let mut config = Config::new(args.db.clone());
    config.port = args.port;
    config.bind = args.bind;
    config.log = args.log;
    config.trust_proxy = args.trust_proxy;
    config.token = token;
    config.file_log = true;

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => return errore(format!("Errore: impossibile avviare il runtime: {e}")),
    };
    let (tx, rx) = oneshot::channel();
    let esito = runtime.block_on(async {
        let mut server = tokio::spawn(server::run(
            config,
            cancel.clone().cancelled_owned(),
            Some(tx),
        ));
        // `Running` solo dopo il bind riuscito della porta.
        if let Ok(addr) = rx.await {
            logger.info(&format!(
                "Server MCP in ascolto su {addr} ({})",
                args.mcp_url()
            ));
            segnala(stato(ServiceState::Running, ServiceExitCode::NO_ERROR, Duration::ZERO));
        }
        tokio::select! {
            r = &mut server => Some(r),
            _ = cancel.cancelled() => {
                segnala(stato(ServiceState::StopPending, ServiceExitCode::NO_ERROR, ATTESA_ARRESTO + Duration::from_secs(5)));
                tokio::time::timeout(ATTESA_ARRESTO, server).await.ok()
            }
        }
    });
    runtime.shutdown_timeout(Duration::from_secs(5));

    match esito {
        Some(Ok(Ok(()))) => {
            logger.info(&format!("Servizio {} arrestato", args.name));
            ServiceExitCode::NO_ERROR
        }
        None => {
            logger.info(&format!(
                "Servizio {} arrestato senza attendere le richieste in corso ({} s)",
                args.name,
                ATTESA_ARRESTO.as_secs()
            ));
            ServiceExitCode::NO_ERROR
        }
        Some(Ok(Err(e))) => {
            logger.error(&format!("Errore: {e:#}"));
            if e.downcast_ref::<DatabaseNonTrovato>().is_some() {
                ServiceExitCode::ServiceSpecific(USCITA_DATABASE)
            } else if e.downcast_ref::<PortaNonDisponibile>().is_some() {
                ServiceExitCode::ServiceSpecific(USCITA_PORTA)
            } else {
                ServiceExitCode::ServiceSpecific(USCITA_ALTRO)
            }
        }
        Some(Err(e)) => errore(format!(
            "Errore: il server è terminato in modo anomalo: {e}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn p(args: &[&str]) -> Result<Azione, String> {
        parse(&args.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn installa(args: &[&str]) -> Installa {
        match p(args) {
            Ok(Azione::Install(i)) => i,
            other => panic!("{args:?}: {other:?}"),
        }
    }

    #[test]
    fn install_predefiniti() {
        let i = installa(&["install", "--db", r"C:\ProgramData\OMIMyMCP\omi.db"]);
        assert_eq!(i.run.name, "OMIMyMCP");
        assert_eq!(i.display_name, "OMIMyMCP - Server MCP quotazioni OMI");
        assert_eq!(i.run.port, 8765);
        assert_eq!(i.run.bind, Some("127.0.0.1".parse().unwrap()));
        assert!(!i.run.log && !i.run.trust_proxy && i.run.token_file.is_none());
        assert_eq!(i.start, Avvio::Auto);
        assert_eq!(i.account, Account::LocalService);
        assert!(i.acl && !i.firewall && !i.now);
    }

    #[test]
    fn install_tutte_le_opzioni() {
        let i = installa(&[
            "install",
            "--name",
            "OMIMyMCP-Test",
            "--display-name",
            "Prova",
            "--db",
            "omi.db",
            "--port",
            "18765",
            "--bind",
            "0.0.0.0",
            "--log",
            "--trust-proxy",
            "--token-file",
            "token.txt",
            "--start",
            "DELAYED",
            "--account",
            "networkservice",
            "--firewall",
            "--no-acl",
            "--now",
        ]);
        assert_eq!(i.run.name, "OMIMyMCP-Test");
        assert_eq!(i.display_name, "Prova");
        assert_eq!(i.run.port, 18765);
        assert_eq!(i.run.bind, Some("0.0.0.0".parse().unwrap()));
        assert!(i.run.log && i.run.trust_proxy);
        assert_eq!(i.run.token_file, Some(PathBuf::from("token.txt")));
        assert_eq!(i.start, Avvio::Delayed);
        assert_eq!(i.account, Account::NetworkService);
        assert!(!i.acl && i.firewall && i.now);
        assert_eq!(
            installa(&[
                "install",
                "--db",
                "x.db",
                "--start",
                "manual",
                "--account",
                "localsystem"
            ])
            .account,
            Account::LocalSystem
        );
    }

    #[test]
    fn altri_comandi() {
        assert_eq!(
            p(&["uninstall"]),
            Ok(Azione::Uninstall {
                name: "OMIMyMCP".into(),
                keep_firewall: false
            })
        );
        assert_eq!(
            p(&["uninstall", "--name", "X", "--keep-firewall"]),
            Ok(Azione::Uninstall {
                name: "X".into(),
                keep_firewall: true
            })
        );
        assert_eq!(p(&["start"]), Ok(Azione::Start("OMIMyMCP".into())));
        assert_eq!(p(&["stop", "--name", "Y"]), Ok(Azione::Stop("Y".into())));
        assert_eq!(p(&["status"]), Ok(Azione::Status("OMIMyMCP".into())));
    }

    #[test]
    fn errori_dei_parametri() {
        let e = |a: &[&str]| p(a).unwrap_err();
        assert_eq!(e(&[]), format!("Comando mancante dopo service.\n{USO}"));
        assert_eq!(e(&["boh"]), format!("Parametro non valido: boh\n{USO}"));
        assert_eq!(
            e(&["install"]),
            format!("Parametro obbligatorio mancante: --db\n{USO}")
        );
        assert_eq!(
            e(&["install", "--db"]),
            format!("Valore mancante per --db\n{USO}")
        );
        assert_eq!(
            e(&["install", "--db", "x.db", "--port", "0"]),
            format!("Porta non valida: 0\n{USO}")
        );
        assert_eq!(
            e(&["install", "--db", "x.db", "--bind", "localhost"]),
            format!("Indirizzo non valido per --bind: localhost\n{USO}")
        );
        assert_eq!(
            e(&["install", "--db", "x.db", "--start", "subito"]),
            format!("Valore non valido per --start: subito\n{USO}")
        );
        assert_eq!(
            e(&["install", "--db", "x.db", "--account", "admin"]),
            format!("Valore non valido per --account: admin\n{USO}")
        );
        assert_eq!(
            e(&["install", "--db", "x.db", "--name", r"a\b"]),
            format!("Nome del servizio non valido: a\\b\n{USO}")
        );
        // Opzioni non previste dal sottocomando.
        assert_eq!(
            e(&["status", "--db", "x.db"]),
            format!("Parametro non valido: --db\n{USO}")
        );
        assert_eq!(
            e(&["run", "--db", "x.db", "--now"]),
            format!("Parametro non valido: --now\n{USO}")
        );
    }

    #[test]
    fn argomenti_del_servizio_e_ritorno() {
        let r = RunArgs {
            name: "OMIMyMCP".into(),
            db: PathBuf::from(r"C:\Dati OMI\omi.db"),
            port: 8765,
            bind: Some("127.0.0.1".parse().unwrap()),
            log: true,
            trust_proxy: false,
            token_file: Some(PathBuf::from(r"C:\ProgramData\OMIMyMCP\token.txt")),
        };
        let args: Vec<String> = r
            .to_args()
            .into_iter()
            .map(|a| a.into_string().unwrap())
            .collect();
        assert_eq!(
            args,
            [
                "service",
                "run",
                "--name",
                "OMIMyMCP",
                "--db",
                r"C:\Dati OMI\omi.db",
                "--port",
                "8765",
                "--bind",
                "127.0.0.1",
                "--log",
                "--token-file",
                r"C:\ProgramData\OMIMyMCP\token.txt"
            ]
        );
        assert_eq!(parse(&args[1..]), Ok(Azione::Run(r.clone())));

        // Riga di comando come la compone windows-service (percorsi con spazi tra virgolette).
        let riga = r#""C:\Program Files\OMIMyMCP\omi-mcp.exe" service run --name OMIMyMCP --db "C:\Dati OMI\omi.db" --port 8765 --bind 127.0.0.1 --log --token-file C:\ProgramData\OMIMyMCP\token.txt"#;
        let (exe, resto, run) = argomenti_registrati(OsStr::new(riga));
        assert_eq!(exe, r"C:\Program Files\OMIMyMCP\omi-mcp.exe");
        assert_eq!(resto, args);
        assert_eq!(run, Some(r));
        assert_eq!(argomenti_registrati(OsStr::new("altro.exe -x")).2, None);
    }

    #[test]
    fn divisione_della_riga_di_comando() {
        let s = |r: &str| split_command_line(r);
        assert_eq!(s(""), Vec::<String>::new());
        assert_eq!(s("  a   b\tc "), ["a", "b", "c"]);
        assert_eq!(s(r#""a b" c"#), ["a b", "c"]);
        assert_eq!(s(r#""""#), [""]);
        assert_eq!(s(r#""C:\dir con\spazi\\" x"#), [r"C:\dir con\spazi\", "x"]);
        assert_eq!(s(r#"a\\\"b"#), [r#"a\"b"#]);
        assert_eq!(s(r"C:\a\b"), [r"C:\a\b"]);
        assert_eq!(quota("a b"), "\"a b\"");
        assert_eq!(quota("ab"), "ab");
    }

    #[test]
    fn indirizzi_e_url() {
        let mut r = match p(&["run", "--db", "x.db", "--port", "9000"]) {
            Ok(Azione::Run(r)) => r,
            other => panic!("{other:?}"),
        };
        assert_eq!(r.bind, None);
        assert_eq!(r.health_addr(), "127.0.0.1:9000".parse().unwrap());
        assert_eq!(r.mcp_url(), "http://localhost:9000/mcp");
        r.bind = Some("::".parse().unwrap());
        assert_eq!(r.health_addr(), "[::1]:9000".parse().unwrap());
        r.bind = Some("192.168.1.5".parse().unwrap());
        assert_eq!(r.health_addr(), "192.168.1.5:9000".parse().unwrap());
        assert_eq!(r.mcp_url(), "http://192.168.1.5:9000/mcp");
        assert_eq!(r.log_path(), PathBuf::from("x.log"));
    }

    #[test]
    fn configurazione_senza_token_in_chiaro() {
        let r = RunArgs {
            name: "OMIMyMCP".into(),
            db: PathBuf::from(r"C:\d\omi.db"),
            port: 8765,
            bind: None,
            log: false,
            trust_proxy: true,
            token_file: Some(PathBuf::from(r"C:\d\token.txt")),
        };
        assert_eq!(
            r.descrizione(),
            r"Configurazione: database C:\d\omi.db, porta 8765, bind tutte le interfacce, log HTTP/TOOL no, trust-proxy sì, token ******** (dal file C:\d\token.txt)"
        );
    }

    #[test]
    fn righe_di_comando_esterne() {
        assert_eq!(
            icacls_args(Path::new(r"C:\ProgramData\OMIMyMCP"), "S-1-5-19"),
            r#""C:\ProgramData\OMIMyMCP" /grant "*S-1-5-19:(OI)(CI)M""#
        );
        assert_eq!(
            icacls_args(Path::new(r"D:\"), "S-1-5-20"),
            r#""D:\." /grant "*S-1-5-20:(OI)(CI)M""#
        );
        assert_eq!(
            firewall_add_args(
                "OMIMyMCP",
                8765,
                Path::new(r"C:\Program Files\OMIMyMCP\omi-mcp.exe")
            ),
            r#"advfirewall firewall add rule name="OMIMyMCP" dir=in action=allow protocol=TCP localport=8765 program="C:\Program Files\OMIMyMCP\omi-mcp.exe""#
        );
        assert_eq!(
            firewall_delete_args(
                "OMIMyMCP-Test",
                Path::new(r"C:\ProgramData\OMIMyMCP-Test\bin\omi-mcp.exe")
            ),
            r#"advfirewall firewall delete rule name="OMIMyMCP-Test" program="C:\ProgramData\OMIMyMCP-Test\bin\omi-mcp.exe""#
        );
    }

    #[test]
    fn codici_d_uscita() {
        assert_eq!(
            descr_uscita(ServiceExitCode::ServiceSpecific(1)),
            "codice 1: database non trovato"
        );
        assert_eq!(
            descr_uscita(ServiceExitCode::ServiceSpecific(2)),
            "codice 2: porta non disponibile"
        );
        assert_eq!(descr_uscita(ServiceExitCode::NO_ERROR), "nessun errore");
    }

    #[test]
    fn verifica_health() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap();
        let t = std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut req = String::new();
            let mut buf = [0u8; 1024];
            while !req.contains("\r\n\r\n") {
                let n = s.read(&mut buf).unwrap();
                assert_ne!(n, 0);
                req.push_str(&String::from_utf8_lossy(&buf[..n]));
            }
            s.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\n\r\nok")
                .unwrap();
            req
        });
        assert_eq!(health(addr).unwrap(), (200, "ok".to_string()));
        assert!(t.join().unwrap().starts_with("GET /health HTTP/1.1\r\n"));

        // Porta chiusa: errore, nessun panic.
        let libera = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        assert!(health(libera).is_err());
    }
}
