#!/usr/bin/env bash
# OMIMyMCP-rs - Copyright 2026 Explobot
# SPDX-License-Identifier: Apache-2.0
#
# Carica nuove forniture OMI nel database del servizio. Da eseguire come root (sudo).
#
# Uso:
#   sudo ./aggiorna-dati.sh --zip-dir DIR [opzioni]
#
# Opzioni:
#   --zip-dir DIR         cartella con i file QI*.zip / QIP*.zip scaricati dal sito dell'Agenzia (obbligatoria)
#   --data-dir DIR        cartella dei dati (predefinita: /var/lib/omimymcp)
#   --install-dir DIR     cartella dei programmi (predefinita: /opt/omimymcp)
#   --service-name NOME   nome dell'unità systemd (predefinito: omi-mcp)
#   --user UTENTE         utente che esegue l'import (predefinito: omi)
#   --dry-run             mostra le azioni senza eseguirle
#   -h, --help            mostra questo aiuto
#
# Ferma il servizio, esegue omi-import come utente del servizio sui file QI*.zip di ZIP_DIR, riavvia il
# servizio (anche se l'import non riesce) e mostra il riepilogo. I nomi dei file non vengono stampati
# perché contengono il codice fiscale del richiedente.
# Codice d'uscita: quello di omi-import (0 tutto caricato, 2 almeno un file con errori).

set -euo pipefail

ZIP_DIR=""
DATA_DIR="/var/lib/omimymcp"
INSTALL_DIR="/opt/omimymcp"
SERVICE_NAME="omi-mcp"
SERVICE_USER="omi"
DRY_RUN=0

uso() { sed -n '4,24p' "$0" | sed 's/^# \{0,1\}//'; }
errore() { echo "Errore: $*" >&2; exit 1; }

while [ $# -gt 0 ]; do
    case "$1" in
        --zip-dir)      ZIP_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --data-dir)     DATA_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --install-dir)  INSTALL_DIR="${2:?valore mancante per $1}"; shift 2 ;;
        --service-name) SERVICE_NAME="${2:?valore mancante per $1}"; shift 2 ;;
        --user)         SERVICE_USER="${2:?valore mancante per $1}"; shift 2 ;;
        --dry-run)      DRY_RUN=1; shift ;;
        -h|--help)      uso; exit 0 ;;
        *)              errore "opzione sconosciuta: $1 (vedere --help)" ;;
    esac
done

[ -n "$ZIP_DIR" ] || errore "indicare --zip-dir (vedere --help)."
if [ "$DRY_RUN" -eq 0 ] && [ "$(id -u)" -ne 0 ]; then
    errore "servono i privilegi di root: eseguire con sudo."
fi

IMPORT="$INSTALL_DIR/omi-import"
DB="$DATA_DIR/omi.db"
[ -x "$IMPORT" ] || errore "omi-import non trovato in $INSTALL_DIR."
[ -d "$ZIP_DIR" ] || errore "cartella non trovata: $ZIP_DIR"
ZIP_DIR="$(cd "$ZIP_DIR" && pwd)"

shopt -s nullglob
zips=("$ZIP_DIR"/QI*.zip "$ZIP_DIR"/qi*.zip)
shopt -u nullglob
[ "${#zips[@]}" -gt 0 ] || errore "nessun file QI*.zip in $ZIP_DIR."
echo "==> ${#zips[@]} file QI*.zip da caricare in $DB"

servizio_attivo=0
if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet "$SERVICE_NAME" 2>/dev/null; then
    servizio_attivo=1
elif ! systemctl list-unit-files "${SERVICE_NAME}.service" 2>/dev/null | grep -q "^${SERVICE_NAME}.service"; then
    echo "Avviso: il servizio $SERVICE_NAME non è installato: carico solo il database." >&2
fi

codice=0
riavvia() {
    if [ "$servizio_attivo" -eq 1 ] && [ "$DRY_RUN" -eq 0 ]; then
        echo "==> Riavvio del servizio $SERVICE_NAME"
        systemctl start "$SERVICE_NAME"
        echo "    Servizio $SERVICE_NAME in esecuzione."
    elif [ "$servizio_attivo" -eq 1 ]; then
        echo "> systemctl start $SERVICE_NAME"
    fi
}
trap riavvia EXIT

if [ "$servizio_attivo" -eq 1 ]; then
    echo "==> Arresto del servizio $SERVICE_NAME"
    if [ "$DRY_RUN" -eq 0 ]; then systemctl stop "$SERVICE_NAME"; else echo "> systemctl stop $SERVICE_NAME"; fi
fi

echo "> $IMPORT $DB <${#zips[@]} file>"
if [ "$DRY_RUN" -eq 0 ]; then
    # Se esiste, l'import gira come utente del servizio: i file -wal/-shm restano suoi.
    if id -u "$SERVICE_USER" >/dev/null 2>&1; then
        set +e
        runuser -u "$SERVICE_USER" -- "$IMPORT" "$DB" "${zips[@]}"
        codice=$?
        set -e
    else
        set +e
        "$IMPORT" "$DB" "${zips[@]}"
        codice=$?
        set -e
    fi
    if [ "$codice" -eq 2 ]; then
        echo "Avviso: alcuni file non sono stati caricati: vedere i messaggi ERRORE qui sopra." >&2
    elif [ "$codice" -ne 0 ]; then
        echo "Avviso: omi-import è terminato con codice $codice." >&2
    fi
fi
exit "$codice"
