// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Test della riga di comando di `omi-import`.

#[path = "../../omi-core/tests/support/mod.rs"]
mod support;

use std::path::Path;
use std::process::Output;

use assert_cmd::cargo::cargo_bin_cmd;
use support::*;

const USO: &str = "Uso: omi-import [QI*.zip ...] [database.db]";

fn run(dir: &Path, args: &[&str]) -> Output {
    cargo_bin_cmd!("omi-import")
        .current_dir(dir)
        .args(args)
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn stderr(o: &Output) -> String {
    String::from_utf8(o.stderr.clone()).unwrap()
}

fn intestazione() -> String {
    format!(
        "{}\nFonte dei dati: Agenzia Entrate - OMI\nDatabase: ",
        omi_core::about::attribuzione()
    )
}

fn qip(dir: &Path, nome: &str, sem: i64) {
    write_zip(&dir.join(nome), &base().qip(sem));
}

#[test]
fn aiuto() {
    let dir = tempfile::tempdir().unwrap();
    for flag in ["--help", "-h", "/?"] {
        // L'aiuto ha la precedenza anche su argomenti non validi.
        let o = run(dir.path(), &["pippo", flag]);
        assert_eq!(o.status.code(), Some(0), "{flag}");
        let s = stdout(&o);
        assert!(s.starts_with(&omi_core::about::attribuzione()), "{s}");
        assert!(
            s.contains("  omi-import [QI*.zip ...] [database.db]\n"),
            "{s}"
        );
        assert!(stderr(&o).is_empty());
    }
    assert!(!dir.path().join("omi.db").exists());
}

#[test]
fn versione() {
    let dir = tempfile::tempdir().unwrap();
    let o = run(dir.path(), &["--version"]);
    assert_eq!(o.status.code(), Some(0));
    assert_eq!(stdout(&o), format!("{}\n", omi_core::about::attribuzione()));
}

#[test]
fn parametri_non_validi() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("dati.zip"), b"x").unwrap();
    let casi: [(&[&str], String); 5] = [
        (&["pippo"], format!("Parametro non valido: pippo\n{USO}\n")),
        (
            &["omi.db", "-x"],
            format!("Parametro non valido: -x\n{USO}\n"),
        ),
        (
            &["dati.zip"],
            "File non riconosciuto (atteso QI*.zip): dati.zip\n".to_string(),
        ),
        (
            &["QIP999.ZIP"],
            "File non trovato: QIP999.ZIP\n".to_string(),
        ),
        (
            &["QX*.zip"],
            "Nessun file corrisponde a: QX*.zip\n".to_string(),
        ),
    ];
    for (args, atteso) in casi {
        let o = run(dir.path(), args);
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert_eq!(stderr(&o), atteso, "{args:?}");
        assert!(stdout(&o).is_empty(), "{args:?}");
    }
    assert!(!dir.path().join("omi.db").exists());
}

#[test]
fn cartella_senza_zip() {
    let dir = tempfile::tempdir().unwrap();
    let o = run(dir.path(), &[]);
    assert_eq!(o.status.code(), Some(0));
    let s = stdout(&o);
    assert!(s.starts_with(&intestazione()), "{s}");
    let db = std::path::absolute(dir.path().join("omi.db")).unwrap();
    assert!(s.contains(&format!("Database: {}\n", db.display())), "{s}");
    assert!(
        s.ends_with("omi.db\nIl database non contiene ancora dati OMI.\n"),
        "{s}"
    );
    assert!(db.exists());
}

#[test]
fn solo_database_senza_zip() {
    let dir = tempfile::tempdir().unwrap();
    qip(dir.path(), "QIP1.zip", 20252);
    let o = run(dir.path(), &["ALTRO.DB"]);
    assert_eq!(o.status.code(), Some(0));
    let s = stdout(&o);
    assert!(
        s.contains("Nessun file QI*.zip indicato: nessun caricamento.\nIl database non contiene ancora dati OMI.\n"),
        "{s}"
    );
    assert!(dir.path().join("ALTRO.DB").exists());
    assert!(!dir.path().join("omi.db").exists());
}

#[test]
fn senza_argomenti_carica_la_cartella_in_ordine() {
    let dir = tempfile::tempdir().unwrap();
    qip(dir.path(), "QIP_B.zip", 20252);
    qip(dir.path(), "qip_a.zip", 20251);
    write_zip(&dir.path().join("altro.zip"), &base().qi(20242));
    let o = run(dir.path(), &[]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let s = stdout(&o);
    let a = s.find("Import di qip_a.zip").unwrap();
    let b = s.find("Import di QIP_B.zip").unwrap();
    assert!(
        a < b,
        "ordine alfabetico senza distinzione di maiuscole:\n{s}"
    );
    assert!(!s.contains("altro.zip"));
    assert!(
        s.contains("Aggiornamento anagrafiche e indici...\nCaricati 2/2 file.\n"),
        "{s}"
    );
    assert!(
        s.contains("  Semestri disponibili:     2 (2025/1 - 2025/2), 2 con perimetri\n"),
        "{s}"
    );
}

#[test]
fn caratteri_jolly() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("forniture")).unwrap();
    qip(
        &dir.path().join("forniture"),
        &format!("QIP1_{CF}.zip"),
        20251,
    );
    qip(&dir.path().join("forniture"), "qip2.zip", 20252);
    let o = run(dir.path(), &["dati.db", "forniture/QIP?*.ZIP"]);
    assert_eq!(o.status.code(), Some(0), "{}", stderr(&o));
    let s = stdout(&o);
    assert!(s.contains("Caricati 2/2 file.\n"), "{s}");
    assert!(s.contains("Import di QIP1.zip ("), "nome ripulito:\n{s}");
    assert!(!s.contains(CF));
    assert!(dir.path().join("dati.db").exists());

    // Un file trovato con i caratteri jolly che non è QI*.zip è un errore.
    std::fs::write(dir.path().join("forniture").join("qip3.txt"), b"x").unwrap();
    let o = run(dir.path(), &["forniture/qip*"]);
    assert_eq!(o.status.code(), Some(1));
    assert!(
        stderr(&o).starts_with("File non riconosciuto (atteso QI*.zip): "),
        "{}",
        stderr(&o)
    );
}

#[test]
fn zip_corrotto_tra_due_validi() {
    let dir = tempfile::tempdir().unwrap();
    qip(dir.path(), "QIP_1.zip", 20251);
    std::fs::write(
        dir.path().join(format!("QIP_2_{CF}.zip")),
        b"non e' uno zip",
    )
    .unwrap();
    qip(dir.path(), "QIP_3.zip", 20252);
    let o = run(dir.path(), &[]);
    assert_eq!(o.status.code(), Some(2));
    let (s, e) = (stdout(&o), stderr(&o));
    assert!(e.starts_with("  ERRORE su QIP_2.zip: "), "{e}");
    assert!(!e.contains(CF) && !s.contains(CF));
    assert!(s.contains("Caricati 2/3 file.\n"), "{s}");
    assert!(
        s.contains("  Semestri disponibili:     2 (2025/1 - 2025/2), 2 con perimetri\n"),
        "{s}"
    );

    // Ricaricando solo i due file validi il codice torna 0.
    let o = run(dir.path(), &["QIP_1.zip", "QIP_3.zip"]);
    assert_eq!(o.status.code(), Some(0));
    assert!(stdout(&o).contains("Caricati 2/2 file.\n"));
}
