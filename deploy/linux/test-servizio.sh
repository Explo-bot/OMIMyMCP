#!/usr/bin/env bash
# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
#
# Prova di installazione del servizio systemd (da eseguire come root).
#
# Installa omi-mcp come servizio di prova (omi-mcp-test, porta 18765, con token) in cartelle e con un
# utente dedicati, poi verifica: unità attiva, /health, 401 senza token, tools/list (9 strumenti),
# arresto e riavvio, log senza il token, disinstallazione completa. Alla fine elimina tutto ed esce con 0
# solo se ogni verifica è riuscita.
#
# Uso:
#   sudo ./test-servizio.sh --db FILE [--source-dir DIR] [--port PORTA] [--keep-files]
#
#   --db FILE         database da usare (viene copiato); si crea con omi-import
#   --source-dir DIR  cartella con omi-mcp e omi-import (predefinita: target/release del repository,
#                     oppure la radice del pacchetto di rilascio)

set -uo pipefail

SOURCE_DIR=""
DB=""
PORT=18765
KEEP=0
NAME="omi-mcp-test"
USER_NAME="omitest"
INSTALL_DIR="/opt/omimymcp-test"
DATA_DIR="/var/lib/omimymcp-test"

errore() { echo "Errore: $*" >&2; exit 1; }
uso() { sed -n '4,19p' "$0" | sed 's/^# \{0,1\}//'; }

while [ $# -gt 0 ]; do
    case "$1" in
        --db)         DB="${2:?valore mancante per $1}"; shift 2 ;;
        --source-dir) SOURCE_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --port)       PORT="${2:?valore mancante per $1}"; shift 2 ;;
        --keep-files) KEEP=1; shift ;;
        -h|--help)    uso; exit 0 ;;
        *)            errore "opzione sconosciuta: $1 (vedere --help)" ;;
    esac
done

[ "$(id -u)" -eq 0 ] || errore "servono i privilegi di root: eseguire con sudo."
[ -n "$DB" ] && [ -f "$DB" ] || errore "indicare un database esistente con --db."
command -v curl >/dev/null 2>&1 || errore "curl non trovato."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -z "$SOURCE_DIR" ]; then
    for cand in "$SCRIPT_DIR/../.." "$SCRIPT_DIR/../../target/release"; do
        if [ -x "$cand/omi-mcp" ]; then SOURCE_DIR="$cand"; break; fi
    done
fi
[ -n "$SOURCE_DIR" ] && [ -x "$SOURCE_DIR/omi-mcp" ] || errore "omi-mcp non trovato: indicare --source-dir."

URL="http://127.0.0.1:$PORT"
FALLITI=0
passo() { # descrizione, comando...
    local desc="$1"; shift
    if "$@" >/dev/null 2>&1; then
        echo "[OK]      $desc"
    else
        FALLITI=$((FALLITI + 1))
        echo "[FALLITO] $desc"
    fi
}
http_code() { curl -s -o /dev/null -w '%{http_code}' --max-time 10 "$@" || true; }
attesa_health() { for _ in 1 2 3 4 5 6 7 8 9 10; do [ "$(http_code "$URL/health")" = "200" ] && return 0; sleep 1; done; return 1; }
tools_list() {
    curl -s --max-time 10 -X POST "$URL/mcp" -H 'Content-Type: application/json' \
        -H 'Accept: application/json, text/event-stream' -H "Authorization: Bearer $TOKEN" \
        -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
}
nove_tool() { [ "$(tools_list | grep -o '"inputSchema"' | wc -l)" -eq 9 ]; }
senza_token() { [ "$(http_code -X POST "$URL/mcp" -H 'Content-Type: application/json' -H 'Accept: application/json, text/event-stream' -d '{}')" = "401" ]; }
attivo() { [ "$(systemctl is-active "$NAME")" = "active" ]; }
fermo() { [ "$(systemctl is-active "$NAME")" != "active" ]; }
pulizia() {
    "$SCRIPT_DIR/uninstall.sh" --install-dir "$INSTALL_DIR" --data-dir "$DATA_DIR" \
        --service-name "$NAME" --user "$USER_NAME" --remove-data --force >/dev/null 2>&1 || true
}

# Residuo di una prova precedente.
pulizia
mkdir -p "$DATA_DIR.tmp" && cp "$DB" "$DATA_DIR.tmp/omi.db"

echo "==> Installazione di $NAME sulla porta $PORT"
useradd --system --home-dir "$DATA_DIR" --no-create-home --shell /usr/sbin/nologin "$USER_NAME" 2>/dev/null || true
install -d -m 750 -o "$USER_NAME" -g "$USER_NAME" "$DATA_DIR"
install -m 640 -o "$USER_NAME" -g "$USER_NAME" "$DATA_DIR.tmp/omi.db" "$DATA_DIR/omi.db"
rm -rf "$DATA_DIR.tmp"
if [ -f "$SOURCE_DIR/omi-import" ]; then :; else errore "omi-import non trovato in $SOURCE_DIR."; fi
"$SCRIPT_DIR/install.sh" --source-dir "$SOURCE_DIR" --install-dir "$INSTALL_DIR" --data-dir "$DATA_DIR" \
    --service-name "$NAME" --user "$USER_NAME" --port "$PORT" --with-token --log >/dev/null
TOKEN="$(tr -d '\n' <"$DATA_DIR/token.txt" 2>/dev/null)"

passo "install.sh completato e unità attiva" attivo
passo "GET /health -> 200" attesa_health
passo "POST /mcp senza token -> 401" senza_token
passo "tools/list -> 9 strumenti" nove_tool
passo "token con permessi 600" test "$(stat -c %a "$DATA_DIR/token.txt")" = "600"

echo "==> Arresto e riavvio"
passo "systemctl stop" systemctl stop "$NAME"
passo "unità ferma" fermo
passo "systemctl start" systemctl start "$NAME"
passo "unità attiva dopo il riavvio" attivo
passo "tools/list dopo il riavvio" bash -c "$(declare -f attesa_health http_code tools_list nove_tool); URL='$URL'; TOKEN='$TOKEN'; attesa_health && nove_tool"
passo "log: richieste registrate e token assente" bash -c "grep -q 'HTTP 127.0.0.1 GET /health' '$DATA_DIR/omi.log' && ! grep -q -F '$TOKEN' '$DATA_DIR/omi.log'"

echo "==> Disinstallazione"
if [ "$KEEP" -eq 1 ]; then
    "$SCRIPT_DIR/uninstall.sh" --install-dir "$INSTALL_DIR" --data-dir "$DATA_DIR" --service-name "$NAME" >/dev/null
    passo "unità rimossa" test ! -f "/etc/systemd/system/$NAME.service"
    passo "database lasciato al suo posto" test -f "$DATA_DIR/omi.db"
    echo "File di prova lasciati in $DATA_DIR (utente $USER_NAME): eliminarli con uninstall.sh --remove-data --force."
else
    cp "$DATA_DIR/omi.log" /tmp/omi-test.log 2>/dev/null || true
    pulizia
    passo "unità rimossa" test ! -f "/etc/systemd/system/$NAME.service"
    passo "cartelle e utente di prova eliminati" bash -c "[ ! -d '$DATA_DIR' ] && [ ! -d '$INSTALL_DIR' ] && ! id -u '$USER_NAME'"
    rm -f /tmp/omi-test.log
fi

if [ "$FALLITI" -eq 0 ]; then
    echo "Prova del servizio: tutte le verifiche sono riuscite."
    exit 0
fi
echo "Prova del servizio: $FALLITI verifiche fallite." >&2
exit 1
