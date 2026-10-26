#!/usr/bin/env bash
# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
#
# Installa OMIMyMCP come servizio systemd. Da eseguire come root (sudo).
#
# Uso:
#   sudo ./install.sh --source-dir DIR [opzioni]
#
# Opzioni:
#   --source-dir DIR      cartella con i binari omi-mcp e omi-import (obbligatoria; ad esempio
#                         target/release o il pacchetto di rilascio estratto)
#   --install-dir DIR     cartella dei programmi (predefinita: /opt/omimymcp)
#   --data-dir DIR        cartella dei dati: omi.db, omi.log, token.txt (predefinita: /var/lib/omimymcp)
#   --port PORTA          porta TCP (predefinita: 8765)
#   --bind INDIRIZZO      indirizzo di ascolto (predefinito: 127.0.0.1, cioè solo locale o dietro Caddy)
#   --service-name NOME   nome dell'unità systemd (predefinito: omi-mcp)
#   --user UTENTE         utente di sistema del servizio (predefinito: omi; viene creato se manca)
#   --log                 registra anche le richieste HTTP e le chiamate ai tool in <database>.log
#   --with-token          richiede il token Bearer su /mcp (file DATA_DIR/token.txt, generato se manca)
#   --trust-proxy         nel log usa l'IP indicato da X-Forwarded-For (solo dietro Caddy, con --bind 127.0.0.1)
#   --allowed-host HOST   accetta su /mcp solo l'intestazione Host indicata (ripetibile)
#   --add-to-path         crea i collegamenti omi-mcp e omi-import in /usr/local/bin
#   --dry-run             mostra le azioni senza eseguirle
#   -h, --help            mostra questo aiuto
#
# Lo script è idempotente: se il servizio esiste già lo ferma, aggiorna binari e unità e lo riavvia,
# conservando database, log e token. Se il database non esiste si ferma dopo aver copiato i programmi:
# caricarlo con omi-import e rilanciare lo script.

set -euo pipefail

SOURCE_DIR=""
INSTALL_DIR="/opt/omimymcp"
DATA_DIR="/var/lib/omimymcp"
PORT=8765
BIND="127.0.0.1"
SERVICE_NAME="omi-mcp"
SERVICE_USER="omi"
LOG=0
WITH_TOKEN=0
TRUST_PROXY=0
ADD_TO_PATH=0
DRY_RUN=0
ALLOWED_HOSTS=()

uso() { sed -n '4,28p' "$0" | sed 's/^# \{0,1\}//'; }
errore() { echo "Errore: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --source-dir)   SOURCE_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --install-dir)  INSTALL_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --data-dir)     DATA_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --port)         PORT="${2:?valore mancante per $1}"; shift 2 ;;
        --bind)         BIND="${2:?valore mancante per $1}"; shift 2 ;;
        --service-name) SERVICE_NAME="${2:?valore mancante per $1}"; shift 2 ;;
        --user)         SERVICE_USER="${2:?valore mancante per $1}"; shift 2 ;;
        --allowed-host) ALLOWED_HOSTS+=("${2:?valore mancante per $1}"); shift 2 ;;
        --log)          LOG=1; shift ;;
        --with-token)   WITH_TOKEN=1; shift ;;
        --trust-proxy)  TRUST_PROXY=1; shift ;;
        --add-to-path)  ADD_TO_PATH=1; shift ;;
        --dry-run)      DRY_RUN=1; shift ;;
        -h|--help)      uso; exit 0 ;;
        *)              errore "opzione sconosciuta: $1 (vedere --help)" ;;
    esac
done

[ -n "$SOURCE_DIR" ] || errore "indicare --source-dir (vedere --help)."
[[ "$PORT" =~ ^[0-9]+$ ]] && [ "$PORT" -ge 1 ] && [ "$PORT" -le 65535 ] || errore "porta non valida: $PORT"
[[ "$SERVICE_NAME" =~ ^[A-Za-z0-9_.@-]+$ ]] || errore "nome del servizio non valido: $SERVICE_NAME"
[[ "$SERVICE_USER" =~ ^[a-z_][a-z0-9_-]*$ ]] || errore "nome utente non valido: $SERVICE_USER"
for dir in "$INSTALL_DIR" "$DATA_DIR"; do
    case "$dir" in
        *[[:space:]]*|*\"*|*\'*|*\\*) errore "il percorso non può contenere spazi o apici: $dir" ;;
    esac
done

run() {
    echo "> $*"
    if [ "$DRY_RUN" -eq 0 ]; then "$@"; fi
}

if [ "$DRY_RUN" -eq 0 ] && [ "$(id -u)" -ne 0 ]; then
    errore "servono i privilegi di root: eseguire con sudo."
fi
command -v systemctl >/dev/null 2>&1 || errore "systemctl non trovato: questo script richiede systemd."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"
SOURCE_DIR="$(cd "$SOURCE_DIR" && pwd)" || errore "cartella non trovata: $SOURCE_DIR"
for exe in omi-mcp omi-import; do
    [ -x "$SOURCE_DIR/$exe" ] || errore "file mancante o non eseguibile: $SOURCE_DIR/$exe"
done

UNIT="/etc/systemd/system/${SERVICE_NAME}.service"
DB="$DATA_DIR/omi.db"
TOKEN_FILE="$DATA_DIR/token.txt"

# 0. Un servizio già installato va fermato prima di sovrascrivere i binari.
if systemctl list-unit-files "${SERVICE_NAME}.service" 2>/dev/null | grep -q "^${SERVICE_NAME}.service"; then
    echo "==> Il servizio $SERVICE_NAME esiste già: lo fermo prima di aggiornare i file."
    run systemctl stop "$SERVICE_NAME"
fi

# 1. Utente di sistema e cartelle.
echo "==> Utente $SERVICE_USER e cartelle"
if ! id -u "$SERVICE_USER" >/dev/null 2>&1; then
    run useradd --system --home-dir "$DATA_DIR" --no-create-home --shell /usr/sbin/nologin "$SERVICE_USER"
fi
run install -d -m 755 "$INSTALL_DIR"
run install -d -m 750 -o "$SERVICE_USER" -g "$SERVICE_USER" "$DATA_DIR"

# 2. Programmi e documenti.
echo "==> Copia dei programmi in $INSTALL_DIR"
run install -m 755 "$SOURCE_DIR/omi-mcp" "$SOURCE_DIR/omi-import" "$INSTALL_DIR/"
for f in LICENSE NOTICE MANUALE_TECNICO.md; do
    origine=""
    for cand in "$SOURCE_DIR/$f" "$REPO_DIR/$f" "$REPO_DIR/docs/$f"; do
        if [ -f "$cand" ]; then origine="$cand"; break; fi
    done
    if [ -n "$origine" ]; then
        run install -m 644 "$origine" "$INSTALL_DIR/$f"
    else
        echo "Avviso: $f non trovato: non copiato." >&2
    fi
done
if [ "$ADD_TO_PATH" -eq 1 ]; then
    run ln -sf "$INSTALL_DIR/omi-mcp" /usr/local/bin/omi-mcp
    run ln -sf "$INSTALL_DIR/omi-import" /usr/local/bin/omi-import
fi

# 3. Token (facoltativo): leggibile solo dall'utente del servizio.
if [ "$WITH_TOKEN" -eq 1 ]; then
    if [ -f "$TOKEN_FILE" ]; then
        echo "==> Token già presente: $TOKEN_FILE (non modificato)"
    else
        echo "==> Generazione del token in $TOKEN_FILE"
        if [ "$DRY_RUN" -eq 0 ]; then
            umask 077
            head -c 32 /dev/urandom | base64 | tr '+/' '-_' | tr -d '=\n' >"$TOKEN_FILE"
            echo >>"$TOKEN_FILE"
        else
            echo "> (genera un token casuale in $TOKEN_FILE)"
        fi
    fi
    run chown "$SERVICE_USER:$SERVICE_USER" "$TOKEN_FILE"
    run chmod 600 "$TOKEN_FILE"
fi

# 4. Il database va caricato prima con omi-import.
if [ "$DRY_RUN" -eq 0 ] && [ ! -f "$DB" ]; then
    echo "Avviso: database non trovato: $DB. Caricarlo prima con omi-import, ad esempio:" >&2
    echo "  sudo -u $SERVICE_USER $INSTALL_DIR/omi-import '$DB' /percorso/QI*.zip" >&2
    echo "Poi rilanciare questo script per installare il servizio." >&2
    exit 1
fi

# 5. Unità systemd.
EXEC="$INSTALL_DIR/omi-mcp $DB $PORT --bind $BIND"
[ "$LOG" -eq 1 ] && EXEC+=" --log"
[ "$TRUST_PROXY" -eq 1 ] && EXEC+=" --trust-proxy"
[ "$WITH_TOKEN" -eq 1 ] && EXEC+=" --token-file $TOKEN_FILE"
for h in "${ALLOWED_HOSTS[@]+"${ALLOWED_HOSTS[@]}"}"; do EXEC+=" --allowed-host $h"; done

echo "==> Unità systemd $UNIT"
UNIT_TESTO="# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
# Generato da deploy/linux/install.sh: per cambiare le opzioni rilanciare lo script.

[Unit]
Description=OMIMyMCP - Server MCP quotazioni OMI
Documentation=file://$INSTALL_DIR/MANUALE_TECNICO.md
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_USER
ExecStart=$EXEC
Restart=on-failure
RestartSec=5
ProtectSystem=strict
ReadWritePaths=$DATA_DIR
ProtectHome=true
PrivateTmp=true
NoNewPrivileges=true

[Install]
WantedBy=multi-user.target
"
if [ "$DRY_RUN" -eq 0 ]; then
    printf '%s' "$UNIT_TESTO" >"$UNIT"
    chmod 644 "$UNIT"
else
    echo "> (scrive $UNIT)"
    printf '%s' "$UNIT_TESTO" | sed 's/^/    /'
fi
run systemctl daemon-reload
run systemctl enable "$SERVICE_NAME"
run systemctl restart "$SERVICE_NAME"

# 6. Verifica di /health.
if [ "$DRY_RUN" -eq 0 ] && command -v curl >/dev/null 2>&1; then
    check_host="$BIND"
    case "$BIND" in 0.0.0.0|::|"[::]") check_host="127.0.0.1" ;; esac
    echo "==> Verifica di http://$check_host:$PORT/health"
    ok=0
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        if [ "$(curl -s -o /dev/null -w '%{http_code}' "http://$check_host:$PORT/health" || true)" = "200" ]; then ok=1; break; fi
        sleep 1
    done
    if [ "$ok" -eq 1 ]; then
        echo "    /health risponde 200."
    else
        echo "Avviso: /health non risponde. Controllare: journalctl -u $SERVICE_NAME -n 50" >&2
        exit 1
    fi
fi

if [ "$WITH_TOKEN" -eq 1 ]; then
    echo "Il client MCP deve inviare 'Authorization: Bearer <token>' con il contenuto di $TOKEN_FILE."
fi
echo "Installazione completata. Stato: systemctl status $SERVICE_NAME"
