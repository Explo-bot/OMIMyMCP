// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Controlli trasversali sul repository: nessun `round()` al posto di `round_ties_even()`,
//! intestazione della licenza in ogni `.rs`, file `LICENSE` completo e nessun codice fiscale
//! nei file versionati.

use std::path::{Path, PathBuf};

use regex::Regex;

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// File `.rs` del workspace (esclusi `target/`).
fn sorgenti_rs() -> Vec<PathBuf> {
    fn visita(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                if p.file_name().is_some_and(|n| n != "target") {
                    visita(&p, out);
                }
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    visita(&workspace().join("crates"), &mut out);
    out
}

#[test]
fn arrotondamenti_mai_con_round() {
    let re = Regex::new(r"\.round\(\)").unwrap();
    let mut trovati = Vec::new();
    for p in sorgenti_rs() {
        let testo = std::fs::read_to_string(&p).unwrap();
        for (i, riga) in testo.lines().enumerate() {
            if re.is_match(riga) {
                trovati.push(format!("{}:{}", p.display(), i + 1));
            }
        }
    }
    assert!(
        trovati.is_empty(),
        "round() al posto di round_ties_even(): {trovati:?}"
    );
}

#[test]
fn licenza_e_intestazioni() {
    let intestazione =
        "// OMIMyMCP-rs - Copyright 2026 Explobot\n// SPDX-License-Identifier: Apache-2.0\n";
    let senza: Vec<String> = sorgenti_rs()
        .into_iter()
        .filter(|p| {
            let t = std::fs::read_to_string(p).unwrap().replace("\r\n", "\n");
            !t.starts_with(intestazione)
        })
        .map(|p| p.display().to_string())
        .collect();
    assert!(senza.is_empty(), "file senza intestazione: {senza:?}");

    let license = std::fs::read_to_string(workspace().join("LICENSE")).unwrap();
    assert!(license.contains("Apache License") && license.contains("Version 2.0, January 2004"));
    assert!(license.contains("END OF TERMS AND CONDITIONS"));

    let notice = std::fs::read_to_string(workspace().join("NOTICE")).unwrap();
    assert!(notice.contains("Explobot"));
    assert!(notice.contains("https://github.com/Explo-bot/OMIMyMCP"));
}

/// File del repository da controllare: esclusi build, git, dati reali, log e file ignorati.
fn file_del_repository() -> Vec<PathBuf> {
    fn visita(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            let nome = p
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if p.is_dir() {
                if !["target", ".git", "dati-test", "logs", "dist"].contains(&nome.as_str()) {
                    visita(&p, out);
                }
            } else if ![
                ".zip", ".db", ".db-shm", ".db-wal", ".log", ".exe", ".tgz", ".gz",
            ]
            .iter()
            .any(|x| nome.ends_with(x))
            {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    visita(&workspace(), &mut out);
    out
}

#[test]
fn nessun_codice_fiscale_nel_repository() {
    // Separatori attorno al codice: anche `_` (i nomi degli ZIP sono `QIP…_<codice fiscale>.zip`).
    let re = Regex::new(
        r"(?i)(?:^|[^A-Z0-9])([A-Z]{6}[0-9]{2}[A-Z][0-9]{2}[A-Z][0-9]{3}[A-Z])(?:[^A-Z0-9]|$)",
    )
    .unwrap();
    let mut trovati = Vec::new();
    let files = file_del_repository();
    for p in &files {
        let testo = String::from_utf8_lossy(&std::fs::read(p).unwrap()).into_owned();
        for c in re.captures_iter(&testo) {
            // L'unico ammesso è il codice sintetico d'esempio.
            if !c[1].eq_ignore_ascii_case("RSSMRA80A01H501U") {
                trovati.push(p.display().to_string());
            }
        }
    }
    assert!(files.len() > 30, "trovati solo {} file", files.len());
    assert!(trovati.is_empty(), "codici fiscali in: {trovati:?}");
}
