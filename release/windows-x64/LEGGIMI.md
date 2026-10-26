<!--
OMIMyMCP-rs - Copyright 2026 Explobot
SPDX-License-Identifier: Apache-2.0
-->

# OMIMyMCP-rs: pacchetto compilato per Windows (x64)

Questo pacchetto contiene i programmi **già compilati** e gli script per installarli: non servono Rust né
compilatori. Progetto originale: <https://github.com/Explo-bot/OMIMyMCP> · licenza Apache-2.0.

| Contenuto | Scopo |
|---|---|
| `omi-import.exe` | Carica in `omi.db` i file `QI*.zip` / `QIP*.zip` scaricati dal sito dell'Agenzia delle Entrate |
| `omi-mcp.exe` | Server MCP (HTTP su `/mcp`), da console o come servizio Windows |
| `deploy\windows\install.ps1` | Installa `omi-mcp` come servizio Windows |
| `deploy\windows\uninstall.ps1` | Disinstalla servizio, programmi e (a richiesta) dati |
| `deploy\windows\aggiorna-dati.ps1` | Carica nuovi semestri nel database del servizio |
| `deploy\windows\install-caddy.ps1` | Installa Caddy come proxy HTTPS (facoltativo, per i connettori di claude.ai) |
| `deploy\windows\test-servizio.ps1` | Prova di installazione con un servizio di prova |
| `deploy\windows\verifica-residui.ps1` | Controlla che dopo la disinstallazione non restino residui |
| `deploy\caddy\` | Caddyfile di esempio e guida a Caddy |
| `MANUALE_TECNICO.md` | Manuale completo |
| `LICENSE`, `NOTICE` | Licenza Apache-2.0 e avvisi (da conservare se si ridistribuisce) |
| `SHA256SUMS.txt` | Hash SHA-256 dei due eseguibili |

Requisiti: Windows 10/11 o Server 2016+ a 64 bit. Nessun altro software (gli eseguibili includono SQLite e il runtime C).

> **Avvertenze.** Software fornito "così com'è", senza garanzie. Le quotazioni OMI hanno valore indicativo e non
> sostituiscono la stima di un professionista. Non esporre il server a Internet senza protezioni. I dati OMI **non**
> sono inclusi: si scaricano dal sito dell'Agenzia delle Entrate e vanno citati come **"Agenzia delle Entrate - OMI"**.
> Gli eseguibili **non sono firmati**: SmartScreen può mostrare un avviso ("Maggiori informazioni" → "Esegui comunque").

## 1. Verifica dei file

```powershell
Get-FileHash .\omi-import.exe, .\omi-mcp.exe -Algorithm SHA256
Get-Content .\SHA256SUMS.txt
```

I valori devono coincidere. Se i file arrivano da internet, sbloccarli: `Get-ChildItem -Recurse . | Unblock-File`.

## 2. Caricare i dati

Scaricare gli ZIP dal servizio **Forniture dati OMI** dell'Agenzia delle Entrate (vedi il `README.md` del progetto, oppure il
capitolo 4 del manuale), salvarli senza rinominarli in `C:\DatiOMI` e caricarli:

```powershell
New-Item -ItemType Directory -Force C:\ProgramData\OMIMyMCP | Out-Null
.\omi-import.exe 'C:\DatiOMI\QI*.zip' 'C:\ProgramData\OMIMyMCP\omi.db'
```

Un semestre nazionale con perimetri richiede circa 5-10 secondi. Per aggiungere un semestre si rilancia il comando con il
nuovo ZIP e lo stesso database. `.\omi-import.exe --help` mostra tutte le opzioni.

## 3. Prima prova da console

```powershell
.\omi-mcp.exe C:\ProgramData\OMIMyMCP\omi.db 8765 --bind 127.0.0.1
```

Il server ascolta su `http://127.0.0.1:8765/mcp` (Ctrl+C per fermarlo). In un altro terminale:

```powershell
Invoke-WebRequest http://127.0.0.1:8765/health      # 200 ok
claude mcp add --transport http omi http://127.0.0.1:8765/mcp
claude mcp list
```

Con `-l` (o `/l`) il server registra richieste e chiamate in `omi.log`, accanto al database.

## 4. Installare come servizio Windows

Da **PowerShell aperto come amministratore**, nella cartella del pacchetto estratto. Il database deve già esistere in
`C:\ProgramData\OMIMyMCP\omi.db` (punto 2).

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\deploy\windows\install.ps1 -SourceDir .\ -Log -AddToPath
```

Il servizio `OMIMyMCP` parte con Windows, ascolta solo su `127.0.0.1:8765` e viene riavviato in caso di errore. Programmi in
`C:\Program Files\OMIMyMCP`, dati e log in `C:\ProgramData\OMIMyMCP`. Opzioni principali di `install.ps1`:

| Opzione | Effetto |
|---|---|
| `-Port 9000` | Cambia la porta |
| `-WithToken` | Richiede `Authorization: Bearer <token>` su `/mcp` (token in `token.txt`) |
| `-Bind 0.0.0.0 -Firewall` | Rende il server raggiungibile dalla rete locale (usare anche `-WithToken`) |
| `-TrustProxy` | Per l'uso dietro Caddy |
| `-AddToPath` | Aggiunge la cartella dei programmi al `PATH` di sistema |
| `-WhatIf` | Mostra le azioni senza eseguirle |

Gestione del servizio:

```powershell
omi-mcp.exe service status
omi-mcp.exe service stop
omi-mcp.exe service start
```

Aggiornare i dati (ferma il servizio, importa, lo riavvia):

```powershell
.\deploy\windows\aggiorna-dati.ps1 -ZipDir C:\DatiOMI
```

Per aggiornare i programmi basta rilanciare `install.ps1` con la nuova versione del pacchetto: database, log e token restano.

## 5. Usarlo con Claude

| Client | Come collegarlo |
|---|---|
| Claude Code | `claude mcp add --transport http omi http://localhost:8765/mcp` |
| Claude Desktop | `claude_desktop_config.json` con `npx -y mcp-remote http://localhost:8765/mcp` (serve Node.js) |
| claude.ai / Cowork | Connettore personalizzato `https://<dominio>/mcp`: serve HTTPS pubblico, vedi il punto 6 |

Poi, ad esempio: *"Quali semestri OMI sono caricati?"* oppure *"Elenca le zone OMI di Bergamo con le quotazioni delle abitazioni
civili"*. Dettagli e risoluzione dei problemi: capitolo 9 del manuale.

## 6. HTTPS per claude.ai (facoltativo)

Servono un nome DNS pubblico, le porte 80 e 443 raggiungibili e `caddy.exe` scaricato da <https://caddyserver.com/download>:

```powershell
.\deploy\windows\install.ps1 -SourceDir .\ -TrustProxy -Log
.\deploy\windows\install-caddy.ps1 -CaddyExe C:\Download\caddy.exe -Dominio omi.esempio.it -Email nome@esempio.it
```

URL del connettore: `https://omi.esempio.it/mcp`. Guida completa: `deploy\caddy\LEGGIMI.md` e capitolo 8 del manuale.

## 7. Disinstallare

```powershell
.\deploy\windows\uninstall.ps1                          # servizio, firewall, programmi, PATH; i dati restano
.\deploy\windows\uninstall.ps1 -RemoveCaddy             # anche Caddy
.\deploy\windows\uninstall.ps1 -RemoveCaddy -RemoveData # anche database, log, token (con conferma)
.\deploy\windows\verifica-residui.ps1                   # controllo finale
```

## 8. Problemi comuni

| Sintomo | Soluzione |
|---|---|
| "L'esecuzione di script è disabilitata" | `Set-ExecutionPolicy -Scope Process Bypass` nella stessa finestra |
| "Servono i privilegi di amministratore" | Aprire PowerShell con **Esegui come amministratore** |
| Il servizio parte e si ferma | Leggere `C:\ProgramData\OMIMyMCP\omi.log`; controllare che `omi.db` esista e che la porta sia libera |
| `Database non trovato` | Caricare prima gli ZIP con `omi-import.exe` (punto 2) |
| `/l` non riconosciuto in Git Bash | Usare `-l` |

Altre cause e soluzioni: capitolo 14 del manuale.
