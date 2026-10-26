#!/usr/bin/env bash
# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
#
# Disinstalla il servizio systemd OMIMyMCP. Da eseguire come root (sudo).
#
# Uso:
#   sudo ./uninstall.sh [opzioni]
#
# Opzioni:
#   --install-dir DIR     cartella dei programmi (predefinita: /opt/omimymcp)
#   --data-dir DIR        cartella dei dati (predefinita: /var/lib/omimymcp)
#   --service-name NOME   nome dell'unità systemd (predefinito: omi-mcp)
#   --user UTENTE         utente di sistema del servizio (predefinito: omi)
#   --remove-data         elimina anche i dati (database, log, token) e l'utente di sistema, con conferma
#   --remove-caddy        ferma e disabilita Caddy e rimuove il Caddyfile generato da install-caddy.sh
#   --force               con --remove-data non chiede conferma
#   --dry-run             mostra le azioni senza eseguirle
#   -h, --help            mostra questo aiuto
#
# Senza --remove-data i dati restano dove sono: il database è il frutto di ore di download e non si
# recupera. Lo script è idempotente.

set -euo pipefail

INSTALL_DIR="/opt/omimymcp"
DATA_DIR="/var/lib/omimymcp"
SERVICE_NAME="omi-mcp"
SERVICE_USER="omi"
REMOVE_DATA=0
REMOVE_CADDY=0
FORCE=0
DRY_RUN=0

uso() { sed -n '4,22p' "$0" | sed 's/^# \{0,1\}//'; }
errore() { echo "Errore: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --install-dir)  INSTALL_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --data-dir)     DATA_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --service-name) SERVICE_NAME="${2:?valore mancante per $1}"; shift 2 ;;
        --user)         SERVICE_USER="${2:?valore mancante per $1}"; shift 2 ;;
        --remove-data)  REMOVE_DATA=1; shift ;;
        --remove-caddy) REMOVE_CADDY=1; shift ;;
        --force)        FORCE=1; shift ;;
        --dry-run)      DRY_RUN=1; shift ;;
        -h|--help)      uso; exit 0 ;;
        *)              errore "opzione sconosciuta: $1 (vedere --help)" ;;
    esac
done

# Protezione contro percorsi pericolosi passati per errore a rm -rf: servono almeno due livelli
# (come /opt/omimymcp) e non sono ammesse le cartelle di sistema.
for dir in "$INSTALL_DIR" "$DATA_DIR"; do
    case "$dir" in
        /*/*) ;;
        *) errore "percorso non consentito (atteso un percorso assoluto di almeno due livelli): $dir" ;;
    esac
    case "$dir" in
        /usr|/usr/bin|/usr/lib|/usr/local|/usr/local/bin|/etc|/var|/var/lib|/home|/root|/opt|/bin|/sbin|/lib|/boot|/dev|/proc|/sys|*/..|*/../*|*/.|*/./*)
            errore "percorso non consentito: $dir" ;;
    esac
done

run() {
    echo "> $*"
    if [ "$DRY_RUN" -eq 0 ]; then "$@"; fi
}

if [ "$DRY_RUN" -eq 0 ] && [ "$(id -u)" -ne 0 ]; then
    errore "servono i privilegi di root: eseguire con sudo."
fi

UNIT="/etc/systemd/system/${SERVICE_NAME}.service"

# 1. Servizio OMIMyMCP.
echo "==> Servizio $SERVICE_NAME"
if command -v systemctl >/dev/null 2>&1; then
    if [ -f "$UNIT" ] || systemctl list-unit-files "${SERVICE_NAME}.service" 2>/dev/null | grep -q "^${SERVICE_NAME}.service"; then
        run systemctl disable --now "$SERVICE_NAME" || true
    else
        echo "    Il servizio $SERVICE_NAME non è installato."
    fi
    if [ -f "$UNIT" ]; then
        run rm -f "$UNIT"
        run systemctl daemon-reload
    fi
fi

# 2. Caddy (facoltativo): solo ciò che ha creato install-caddy.sh.
if [ "$REMOVE_CADDY" -eq 1 ]; then
    echo "==> Caddy"
    if command -v systemctl >/dev/null 2>&1; then
        run systemctl disable --now caddy || true
    fi
    if [ -f /etc/caddy/Caddyfile ] && grep -q "OMIMyMCP" /etc/caddy/Caddyfile; then
        run rm -f /etc/caddy/Caddyfile
    fi
    if [ -f /etc/systemd/system/caddy.service ] && grep -q "OMIMyMCP" /etc/systemd/system/caddy.service; then
        run rm -f /etc/systemd/system/caddy.service
        run systemctl daemon-reload
    fi
    echo "    Il pacchetto di Caddy (apt/dnf) o il binario in /usr/local/bin/caddy non vengono rimossi: usare il gestore di pacchetti."
fi

# 3. Programmi e collegamenti.
echo "==> Programmi in $INSTALL_DIR"
for link in /usr/local/bin/omi-mcp /usr/local/bin/omi-import; do
    if [ -L "$link" ] && [ "$(readlink "$link")" = "$INSTALL_DIR/$(basename "$link")" ]; then
        run rm -f "$link"
    fi
done
if [ -d "$INSTALL_DIR" ]; then
    run rm -rf "$INSTALL_DIR"
else
    echo "    $INSTALL_DIR non esiste."
fi

# 4. Dati (facoltativo, con conferma).
if [ "$REMOVE_DATA" -eq 1 ]; then
    echo "==> Dati in $DATA_DIR"
    if [ ! -d "$DATA_DIR" ]; then
        echo "    $DATA_DIR non esiste."
    else
        conferma=0
        if [ "$FORCE" -eq 1 ] || [ "$DRY_RUN" -eq 1 ]; then
            conferma=1
        else
            read -r -p "Eliminare definitivamente $DATA_DIR (database, log e token)? [s/N] " risposta
            case "$risposta" in s|S|si|SI|sì|y|Y|yes) conferma=1 ;; esac
        fi
        if [ "$conferma" -eq 1 ]; then
            run rm -rf "$DATA_DIR"
            if id -u "$SERVICE_USER" >/dev/null 2>&1; then run userdel "$SERVICE_USER"; fi
        else
            echo "    Dati lasciati in $DATA_DIR."
        fi
    fi
else
    echo "I dati restano in $DATA_DIR (database, log e token): per eliminarli usare --remove-data."
fi
echo "Disinstallazione completata."
