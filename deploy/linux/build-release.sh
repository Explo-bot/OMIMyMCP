#!/usr/bin/env bash
# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
#
# Compila e crea il pacchetto di rilascio per Linux x86_64 (o per l'architettura della macchina).
#
# Uso:
#   ./deploy/linux/build-release.sh [--skip-tests] [--out-dir DIR] [--target TRIPLA]
#
# Passi:
#   1. cargo test --workspace (salvo --skip-tests);
#   2. cargo build --release --workspace;
#   3. crea OUT_DIR/OMIMyMCP-rs-<versione>-linux-<arch>/ con omi-import, omi-mcp, LICENSE, NOTICE,
#      MANUALE_TECNICO.md, README.md e deploy/linux/*.sh, omi-mcp.service e deploy/caddy/*;
#   4. comprime la cartella in OMIMyMCP-rs-<versione>-linux-<arch>.tar.gz e scrive SHA256SUMS.txt.
# La versione è quella di [workspace.package] in Cargo.toml. Non servono privilegi di root.

set -euo pipefail

SKIP_TESTS=0
OUT_DIR=""
TARGET=""

errore() { echo "Errore: $*" >&2; exit 1; }
uso() { sed -n '4,16p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
    case "$1" in
        --skip-tests) SKIP_TESTS=1; shift ;;
        --out-dir)    OUT_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --target)     TARGET="${2:?valore mancante per $1}"; shift 2 ;;
        -h|--help)    uso; exit 0 ;;
        *)            errore "opzione sconosciuta: $1 (vedere --help)" ;;
    esac
done

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
[ -n "$OUT_DIR" ] || OUT_DIR="$REPO/dist"
command -v cargo >/dev/null 2>&1 || errore "cargo non trovato: installare Rust (https://rustup.rs)."

VERSIONE="$(awk '/^\[workspace\.package\]/{f=1;next} f&&/^version[ ]*=/{gsub(/[" ]/,"",$0); sub(/version=/,"",$0); print; exit}' "$REPO/Cargo.toml")"
[ -n "$VERSIONE" ] || errore "versione non trovata in [workspace.package] di Cargo.toml."

cd "$REPO"
if [ "$SKIP_TESTS" -eq 1 ]; then
    echo "Avviso: test saltati (--skip-tests)." >&2
else
    echo "==> Test"
    cargo test --workspace
fi

echo "==> Compilazione"
if [ -n "$TARGET" ]; then
    cargo build --release --workspace --target "$TARGET"
    BIN="$REPO/target/$TARGET/release"
    ARCH="${TARGET%%-*}"
else
    cargo build --release --workspace
    BIN="$REPO/target/release"
    ARCH="$(uname -m)"
fi
case "$ARCH" in x86_64) ARCH="x64" ;; aarch64) ARCH="arm64" ;; esac

NOME="OMIMyMCP-rs-$VERSIONE-linux-$ARCH"
STAGE="$OUT_DIR/$NOME"
ARCHIVIO="$OUT_DIR/$NOME.tar.gz"
echo "==> Pacchetto $NOME"
rm -rf "$STAGE"
mkdir -p "$STAGE/deploy/linux" "$STAGE/deploy/caddy"
for exe in omi-import omi-mcp; do
    [ -x "$BIN/$exe" ] || errore "file mancante: $BIN/$exe"
    install -m 755 "$BIN/$exe" "$STAGE/$exe"
done
install -m 644 "$REPO/LICENSE" "$REPO/NOTICE" "$REPO/README.md" "$STAGE/"
install -m 644 "$REPO/docs/MANUALE_TECNICO.md" "$STAGE/"
install -m 755 "$REPO"/deploy/linux/*.sh "$STAGE/deploy/linux/"
install -m 644 "$REPO/deploy/linux/omi-mcp.service" "$STAGE/deploy/linux/"
install -m 644 "$REPO"/deploy/caddy/*.example "$REPO/deploy/caddy/LEGGIMI.md" "$STAGE/deploy/caddy/"

echo "==> $ARCHIVIO"
tar -C "$OUT_DIR" -czf "$ARCHIVIO" "$NOME"
HASH="$(sha256sum "$ARCHIVIO" | awk '{print $1}')"
printf '%s  %s\n' "$HASH" "$NOME.tar.gz" >"$OUT_DIR/SHA256SUMS-linux.txt"
echo "    SHA-256: $HASH"
echo "    $(wc -c <"$ARCHIVIO") byte"
echo "Pacchetto completato."
