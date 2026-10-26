// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! `omi-import`: carica le forniture OMI (`QI*.zip`, `QIP*.zip`) nel database SQLite. Qui c'è solo la
//! riga di comando; l'import è in `omi_core::import`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use glob::MatchOptions;
use omi_core::about::{FONTE_DATI, attribuzione};
use omi_core::db;
use omi_core::import::Importer;
use omi_core::text::safe_file_name;

const USO: &str = "Uso: omi-import [QI*.zip ...] [database.db]";

fn aiuto() -> String {
    format!(
        "{}
Carica nel database SQLite le quotazioni immobiliari OMI dell'Agenzia delle Entrate.

Uso:
  omi-import [QI*.zip ...] [database.db]
  omi-import --help | -h | /?
  omi-import --version

Argomenti, in qualunque ordine:
  QI*.zip, QIP*.zip  forniture scaricate dal sito dell'Agenzia delle Entrate, senza
                     rinominarle; sono ammessi i caratteri jolly * e ?
  database.db        database da aggiornare (predefinito: omi.db nella cartella corrente);
                     se non esiste viene creato

Senza argomenti carica tutti i file QI*.zip e QIP*.zip della cartella corrente.

Codici d'uscita: 0 caricamento riuscito, 1 parametri non validi o errore grave,
2 almeno un file non caricato.",
        attribuzione()
    )
}

enum Comando {
    Aiuto,
    Versione,
    Import {
        zips: Vec<PathBuf>,
        db: PathBuf,
        con_argomenti: bool,
    },
}

/// Errore di parametro: il messaggio va su stderr e il codice d'uscita è 1.
struct ErroreParametro(String);

fn termina_con(s: &str, suffisso: &str) -> bool {
    s.to_ascii_lowercase().ends_with(suffisso)
}

/// Il nome del file deve essere `QI*.zip` (senza distinzione di maiuscole) e il file deve esistere.
fn verifica_zip(arg: &str, path: &Path) -> Result<(), ErroreParametro> {
    let nome = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if !nome.starts_with("qi") || !nome.ends_with(".zip") {
        return Err(ErroreParametro(format!(
            "File non riconosciuto (atteso QI*.zip): {arg}"
        )));
    }
    if !path.is_file() {
        return Err(ErroreParametro(format!("File non trovato: {arg}")));
    }
    Ok(())
}

/// Ordine alfabetico senza distinzione di maiuscole.
fn ordina(paths: &mut [PathBuf]) {
    paths.sort_by_cached_key(|p| p.to_string_lossy().to_lowercase());
}

fn parse_args(args: Vec<String>) -> Result<Comando, ErroreParametro> {
    if args
        .iter()
        .any(|a| matches!(a.as_str(), "--help" | "-h" | "/?"))
    {
        return Ok(Comando::Aiuto);
    }
    if args.iter().any(|a| a == "--version") {
        return Ok(Comando::Versione);
    }
    let mut zips = Vec::new();
    let mut db = PathBuf::from("omi.db");
    for arg in &args {
        if arg.contains(['*', '?']) {
            // Il Prompt dei comandi e PowerShell non espandono i caratteri jolly.
            let opzioni = MatchOptions {
                case_sensitive: false,
                require_literal_separator: false,
                require_literal_leading_dot: false,
            };
            let paths = glob::glob_with(arg, opzioni)
                .map_err(|_| ErroreParametro(format!("Parametro non valido: {arg}\n{USO}")))?;
            let mut trovati: Vec<PathBuf> = paths.filter_map(|p| p.ok()).collect();
            if trovati.is_empty() {
                return Err(ErroreParametro(format!("Nessun file corrisponde a: {arg}")));
            }
            ordina(&mut trovati);
            for p in trovati {
                verifica_zip(&p.display().to_string(), &p)?;
                zips.push(p);
            }
        } else if termina_con(arg, ".zip") {
            let p = PathBuf::from(arg);
            verifica_zip(arg, &p)?;
            zips.push(p);
        } else if termina_con(arg, ".db") {
            db = PathBuf::from(arg);
        } else {
            return Err(ErroreParametro(format!(
                "Parametro non valido: {arg}\n{USO}"
            )));
        }
    }
    let con_argomenti = !args.is_empty();
    if !con_argomenti {
        zips = zip_della_cartella().map_err(|e| {
            ErroreParametro(format!("Impossibile leggere la cartella corrente: {e}"))
        })?;
    }
    Ok(Comando::Import {
        zips,
        db,
        con_argomenti,
    })
}

/// Tutti i `QI*.zip` della cartella corrente (quindi anche i `QIP*.zip`).
fn zip_della_cartella() -> std::io::Result<Vec<PathBuf>> {
    let mut zips = Vec::new();
    for entry in std::fs::read_dir(".")? {
        let entry = entry?;
        let nome = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if nome.starts_with("qi") && nome.ends_with(".zip") && entry.file_type()?.is_file() {
            zips.push(PathBuf::from(entry.file_name()));
        }
    }
    ordina(&mut zips);
    Ok(zips)
}

/// Esegue l'import; restituisce il codice d'uscita (0 o 2).
fn importa(zips: &[PathBuf], db_path: &Path, con_argomenti: bool) -> Result<ExitCode> {
    let mut out = std::io::stdout().lock();
    writeln!(out, "{}", attribuzione())?;
    writeln!(out, "Fonte dei dati: {FONTE_DATI}")?;
    let db_path = std::path::absolute(db_path)
        .with_context(|| format!("percorso del database non valido: {}", db_path.display()))?;
    writeln!(out, "Database: {}", db_path.display())?;
    // `open` in scrittura esegue anche `ensure_schema`.
    let mut conn = db::open(&db_path, false)
        .with_context(|| format!("impossibile aprire il database {}", db_path.display()))?;

    let mut falliti = 0;
    if !zips.is_empty() {
        let mut importer = Importer::new(&conn)?;
        for zip in zips {
            if let Err(e) = importer.import_zip(&mut conn, zip, &mut out) {
                out.flush()?;
                // Nel messaggio compare solo il nome ripulito dal codice fiscale.
                let nome = zip
                    .file_name()
                    .map(|n| safe_file_name(&n.to_string_lossy()))
                    .unwrap_or_default();
                eprintln!("  ERRORE su {nome}: {e}");
                falliti += 1;
            }
        }
        importer
            .completa(&mut conn, &mut out)
            .context("aggiornamento delle anagrafiche non riuscito")?;
        writeln!(
            out,
            "Caricati {}/{} file.",
            zips.len() - falliti,
            zips.len()
        )?;
    } else if con_argomenti {
        writeln!(out, "Nessun file QI*.zip indicato: nessun caricamento.")?;
    }
    db::print_summary(&conn, &mut out)?;
    out.flush()?;
    Ok(if falliti > 0 {
        ExitCode::from(2)
    } else {
        ExitCode::SUCCESS
    })
}

fn main() -> ExitCode {
    let mut args = Vec::new();
    for a in std::env::args_os().skip(1) {
        match a.into_string() {
            Ok(s) => args.push(s),
            Err(a) => {
                eprintln!("Parametro non valido: {}\n{USO}", a.to_string_lossy());
                return ExitCode::from(1);
            }
        }
    }
    match parse_args(args) {
        Err(ErroreParametro(msg)) => {
            eprintln!("{msg}");
            ExitCode::from(1)
        }
        Ok(Comando::Aiuto) => {
            println!("{}", aiuto());
            ExitCode::SUCCESS
        }
        Ok(Comando::Versione) => {
            println!("{}", attribuzione());
            ExitCode::SUCCESS
        }
        Ok(Comando::Import {
            zips,
            db,
            con_argomenti,
        }) => match importa(&zips, &db, con_argomenti) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("Errore: {e:#}");
                ExitCode::from(1)
            }
        },
    }
}
