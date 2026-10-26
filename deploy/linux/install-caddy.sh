#!/usr/bin/env bash
# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
#
# Configura Caddy come reverse proxy HTTPS davanti a OMIMyMCP. Da eseguire come root (sudo).
#
# Uso:
#   sudo ./install-caddy.sh --domain omi.esempio.it [--email nome@esempio.it] [opzioni]
#
# Opzioni:
#   --domain DOMINIO      nome DNS pubblico che punta a questa macchina (obbligatorio)
#   --email EMAIL         contatto per Let's Encrypt (consigliato; se omesso la riga "email" viene tolta)
#   --caddy-bin FILE      caddy già scaricato da https://caddyserver.com/download: viene copiato in
#                         /usr/local/bin e registrato come servizio. Se omesso si usa il "caddy"
#                         installato dal gestore di pacchetti (apt, dnf, ...), che porta con sé l'unità systemd
#   --config-dir DIR      cartella del Caddyfile (predefinita: /etc/caddy)
#   --omi-service NOME    servizio OMIMyMCP da cui dipende Caddy (predefinito: omi-mcp)
#   --port PORTA          porta di omi-mcp su 127.0.0.1 (predefinita: 8765)
#   --template FILE       Caddyfile di partenza (predefinito: deploy/caddy/Caddyfile.internet.example)
#   --no-firewall         non apre le porte 80 e 443 in ufw/firewalld
#   --dry-run             mostra le azioni senza eseguirle
#   -h, --help            mostra questo aiuto
#
# Non scarica nulla. OMIMyMCP va installato con --bind 127.0.0.1 --trust-proxy e senza aprire la
# porta 8765 nel firewall, ad esempio con "install.sh --source-dir ... --trust-proxy --log".
# Lo script è idempotente: un Caddyfile esistente viene salvato come Caddyfile.bak.

set -euo pipefail

DOMAIN=""
EMAIL=""
CADDY_BIN=""
CONFIG_DIR="/etc/caddy"
OMI_SERVICE="omi-mcp"
PORT=8765
TEMPLATE=""
FIREWALL=1
DRY_RUN=0

uso() { sed -n '4,26p' "$0" | sed 's/^# \{0,1\}//'; }
errore() { echo "Errore: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --domain)      DOMAIN="${2:?valore mancante per $1}"; shift 2 ;;
        --email)       EMAIL="${2:?valore mancante per $1}"; shift 2 ;;
        --caddy-bin)   CADDY_BIN="${2:?valore mancante per $1}"; shift 2 ;;
        --config-dir)  CONFIG_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --omi-service) OMI_SERVICE="${2:?valore mancante per $1}"; shift 2 ;;
        --port)        PORT="${2:?valore mancante per $1}"; shift 2 ;;
        --template)    TEMPLATE="${2:?valore mancante per $1}"; shift 2 ;;
        --no-firewall) FIREWALL=0; shift ;;
        --dry-run)     DRY_RUN=1; shift ;;
        -h|--help)     uso; exit 0 ;;
        *)             errore "opzione sconosciuta: $1 (vedere --help)" ;;
    esac
done

[ -n "$DOMAIN" ] || errore "indicare --domain (vedere --help)."
[[ "$DOMAIN" =~ ^[A-Za-z0-9]([A-Za-z0-9.-]*[A-Za-z0-9])?$ ]] || errore "dominio non valido: $DOMAIN"
if [ -n "$EMAIL" ]; then
    [[ "$EMAIL" =~ ^[^[:space:]{}\"\&\|\\]+@[^[:space:]{}\"\&\|\\]+$ ]] || errore "email non valida: $EMAIL"
fi
[[ "$PORT" =~ ^[0-9]+$ ]] && [ "$PORT" -ge 1 ] && [ "$PORT" -le 65535 ] || errore "porta non valida: $PORT"

run() {
    echo "> $*"
    if [ "$DRY_RUN" -eq 0 ]; then "$@"; fi
}

if [ "$DRY_RUN" -eq 0 ] && [ "$(id -u)" -ne 0 ]; then
    errore "servono i privilegi di root: eseguire con sudo."
fi
command -v systemctl >/dev/null 2>&1 || errore "systemctl non trovato: questo script richiede systemd."

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ -z "$TEMPLATE" ]; then
    for cand in "$SCRIPT_DIR/Caddyfile.internet.example" "$SCRIPT_DIR/../caddy/Caddyfile.internet.example"; do
        if [ -f "$cand" ]; then TEMPLATE="$cand"; break; fi
    done
fi
[ -n "$TEMPLATE" ] && [ -f "$TEMPLATE" ] || errore "Caddyfile.internet.example non trovato: indicarlo con --template."

# 0. Binario di Caddy.
if [ -n "$CADDY_BIN" ]; then
    [ -f "$CADDY_BIN" ] || errore "caddy non trovato: $CADDY_BIN"
    echo "==> $CADDY_BIN"
    echo "    SHA-256: $(sha256sum "$CADDY_BIN" | awk '{print $1}')"
    echo "    Confrontarlo con l'hash pubblicato per questa versione (pagina di download o release di Caddy)."
    CADDY="/usr/local/bin/caddy"
else
    CADDY="$(command -v caddy || true)"
    [ -n "$CADDY" ] || errore "caddy non installato: installarlo con il gestore di pacchetti (vedi https://caddyserver.com/docs/install) oppure indicare --caddy-bin."
fi

# Porte 80 e 443: se le usa un altro programma Caddy non parte.
if command -v ss >/dev/null 2>&1; then
    for porta in 80 443; do
        occupante="$(ss -H -ltnp "sport = :$porta" 2>/dev/null | grep -v '"caddy"' | head -1 || true)"
        if [ -n "$occupante" ]; then
            echo "Avviso: la porta TCP $porta è già in uso: $occupante. Caddy non riuscirà ad avviarsi." >&2
        fi
    done
fi

# 1. Utente, cartelle e binario (solo con --caddy-bin).
DATA_DIR="/var/lib/caddy"
LOG_DIR="/var/log/caddy"
if [ -n "$CADDY_BIN" ]; then
    echo "==> Installazione di caddy in /usr/local/bin e utente di sistema"
    getent group caddy >/dev/null 2>&1 || run groupadd --system caddy
    id -u caddy >/dev/null 2>&1 || run useradd --system --gid caddy --create-home --home-dir "$DATA_DIR" \
        --shell /usr/sbin/nologin --comment "Caddy web server" caddy
    run install -m 755 "$CADDY_BIN" "$CADDY"
fi
echo "==> Cartelle $CONFIG_DIR, $DATA_DIR/data e $LOG_DIR"
run install -d -m 755 "$CONFIG_DIR"
if id -u caddy >/dev/null 2>&1 || [ "$DRY_RUN" -eq 1 ]; then
    run install -d -m 700 -o caddy -g caddy "$DATA_DIR" "$DATA_DIR/data"
    run install -d -m 755 -o caddy -g caddy "$LOG_DIR"
else
    errore "l'utente caddy non esiste: il pacchetto di Caddy non l'ha creato; usare --caddy-bin."
fi

# 2. Caddyfile dall'esempio.
CADDYFILE="$CONFIG_DIR/Caddyfile"
testo="$(cat "$TEMPLATE")"
testo="${testo//\{\$OMI_DOMINIO\}/$DOMAIN}"
if [ -n "$EMAIL" ]; then
    testo="${testo//\{\$OMI_ACME_EMAIL\}/$EMAIL}"
else
    # "email" senza argomento non è valido: senza --email la riga si toglie.
    testo="$(printf '%s\n' "$testo" | grep -v 'email {\$OMI_ACME_EMAIL}')"
    echo "Avviso: nessuna --email: Let's Encrypt non potrà avvisare della scadenza dei certificati." >&2
fi
testo="${testo//C:\/ProgramData\/Caddy\/data/$DATA_DIR/data}"
testo="${testo//C:\/ProgramData\/Caddy\/logs/$LOG_DIR}"
testo="${testo//127.0.0.1:8765/127.0.0.1:$PORT}"
echo "==> Caddyfile $CADDYFILE"
if [ "$DRY_RUN" -eq 0 ]; then
    if [ -f "$CADDYFILE" ] && ! cmp -s <(printf '%s\n' "$testo") "$CADDYFILE"; then
        cp -p "$CADDYFILE" "$CADDYFILE.bak"
        echo "    Caddyfile precedente salvato in $CADDYFILE.bak"
    fi
    printf '%s\n' "$testo" >"$CADDYFILE"
    chmod 644 "$CADDYFILE"
else
    echo "> (scrive $CADDYFILE)"
fi

# 3. Validazione: se fallisce non si tocca il servizio.
echo "==> Validazione"
if [ "$DRY_RUN" -eq 0 ]; then
    "$CADDY" validate --config "$CADDYFILE" --adapter caddyfile
else
    echo "> $CADDY validate --config $CADDYFILE --adapter caddyfile"
fi

# 4. Unità systemd (solo con --caddy-bin; il pacchetto della distribuzione ha già la sua).
if [ -n "$CADDY_BIN" ]; then
    UNIT="/etc/systemd/system/caddy.service"
    echo "==> Unità systemd $UNIT"
    UNIT_TESTO="# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
# Generato da deploy/linux/install-caddy.sh per OMIMyMCP (unità basata su quella della documentazione di Caddy).

[Unit]
Description=Caddy - reverse proxy HTTPS per OMIMyMCP
Documentation=https://caddyserver.com/docs/
After=network.target network-online.target $OMI_SERVICE.service
Wants=network-online.target

[Service]
Type=notify
User=caddy
Group=caddy
ExecStart=$CADDY run --environ --config $CADDYFILE --adapter caddyfile
ExecReload=$CADDY reload --config $CADDYFILE --adapter caddyfile --force
TimeoutStopSec=5s
LimitNOFILE=1048576
PrivateTmp=true
ProtectHome=true
ProtectSystem=full
AmbientCapabilities=CAP_NET_ADMIN CAP_NET_BIND_SERVICE
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
"
    if [ "$DRY_RUN" -eq 0 ]; then
        printf '%s' "$UNIT_TESTO" >"$UNIT"
        chmod 644 "$UNIT"
    else
        echo "> (scrive $UNIT)"
    fi
    run systemctl daemon-reload
fi

# 5. Firewall: 80 per il challenge HTTP-01 e il reindirizzamento a HTTPS, 443 per HTTPS.
if [ "$FIREWALL" -eq 1 ]; then
    echo "==> Firewall (TCP 80 e 443 in ingresso)"
    if command -v ufw >/dev/null 2>&1 && ufw status 2>/dev/null | grep -q "^Status: active"; then
        run ufw allow 80/tcp
        run ufw allow 443/tcp
    elif command -v firewall-cmd >/dev/null 2>&1 && firewall-cmd --state >/dev/null 2>&1; then
        run firewall-cmd --permanent --add-service=http
        run firewall-cmd --permanent --add-service=https
        run firewall-cmd --reload
    else
        echo "    Nessun firewall attivo rilevato (ufw/firewalld): aprire a mano TCP 80 e 443 se serve."
    fi
fi

# 6. OMIMyMCP deve ascoltare solo in locale e leggere l'IP originale dal proxy.
OMI_UNIT="/etc/systemd/system/${OMI_SERVICE}.service"
if [ -f "$OMI_UNIT" ]; then
    grep -q -- '--trust-proxy' "$OMI_UNIT" || echo "Avviso: $OMI_SERVICE non usa --trust-proxy: nel log del server compare solo 127.0.0.1. Rilanciare install.sh con --trust-proxy." >&2
    grep -Eq -- '--bind (127\.0\.0\.1|::1)( |$)' "$OMI_UNIT" || echo "Avviso: $OMI_SERVICE non ascolta solo su 127.0.0.1: la porta $PORT potrebbe essere raggiungibile senza passare da Caddy." >&2
else
    echo "Avviso: l'unità $OMI_SERVICE non esiste: installarla con install.sh --trust-proxy." >&2
fi
echo "Promemoria: $OMI_SERVICE va installato con --bind 127.0.0.1 --trust-proxy e senza aprire la porta $PORT."

# 7. Avvio e verifica.
echo "==> Avvio di Caddy"
run systemctl enable caddy
run systemctl restart caddy
if [ "$DRY_RUN" -eq 0 ] && command -v curl >/dev/null 2>&1; then
    URL="https://$DOMAIN/health"
    echo "==> Verifica di $URL (il primo certificato può richiedere un minuto)"
    ok=0
    for _ in $(seq 1 18); do
        if [ "$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "$URL" || true)" = "200" ]; then ok=1; break; fi
        sleep 5
    done
    if [ "$ok" -eq 1 ]; then
        echo "    $URL risponde 200."
    else
        echo "Avviso: $URL non risponde. Controllare: record DNS di $DOMAIN, inoltro delle porte 80 e 443, servizio $OMI_SERVICE in esecuzione, journalctl -u caddy -n 50, log in $LOG_DIR." >&2
        exit 1
    fi
fi
echo "Installazione di Caddy completata. URL del connettore: https://$DOMAIN/mcp"
