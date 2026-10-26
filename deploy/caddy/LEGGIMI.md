<!--
OMIMyMCP-rs - Copyright 2026 Explobot
SPDX-License-Identifier: Apache-2.0
-->

# Caddy come reverse proxy HTTPS per OMIMyMCP

Caddy è **facoltativo** e non fa parte del binario: è un programma esterno, installato come servizio separato
(`caddy`, su Windows come servizio Windows, su Linux come unità systemd) che accetta HTTPS sulla porta 443 e inoltra a
`omi-mcp` in HTTP su `127.0.0.1:8765`.

```
Internet ──HTTPS:443──► Caddy (servizio "caddy") ──HTTP──► 127.0.0.1:8765 omi-mcp (servizio "OMIMyMCP" / unità "omi-mcp") ──► omi.db
```

## Quando serve

| Scenario | Serve Caddy? |
|---|---|
| Claude Code o Claude Desktop (tramite `mcp-remote`) sullo stesso PC | No: `http://localhost:8765/mcp` |
| Altri PC della LAN | No, oppure sì con `tls internal` se si vuole HTTPS (`Caddyfile.lan.example`) |
| **Connettore personalizzato di claude.ai / Cowork** | **Sì**: il server dev'essere raggiungibile da Internet in HTTPS con un certificato valido (`Caddyfile.internet.example`) |
| Prove veloci da Internet | In alternativa a Caddy, `cloudflared tunnel --url http://localhost:8765` |

## File

| File | Uso |
|---|---|
| `Caddyfile.internet.example` | Dominio pubblico, certificato Let's Encrypt automatico. Variabili `OMI_DOMINIO` e `OMI_ACME_EMAIL`. |
| `Caddyfile.lan.example` | Rete locale (`omi.lan`), certificato della CA locale di Caddy (`tls internal`). |
| `..\windows\install-caddy.ps1` | Windows: installa Caddy come servizio, genera il `Caddyfile`, apre le porte 80 e 443 nel firewall. |
| `../linux/install-caddy.sh` | Linux: genera `/etc/caddy/Caddyfile`, registra Caddy come unità systemd (se serve), apre 80 e 443 in ufw/firewalld. |

Le direttive dei due esempi sono state verificate con `caddy validate --adapter caddyfile` (Caddy 2.11.7).

## Installazione su Windows

1. Scaricare `caddy.exe` da <https://caddyserver.com/download> (o dalle release ufficiali del progetto Caddy):
   lo script non scarica nulla. Confrontare l'hash SHA-256 che lo script stampa con quello pubblicato.
2. Installare OMIMyMCP in modo che accetti solo il proxy, con l'IP originale nel log e **senza** regola di firewall
   propria (la porta 8765 non deve essere raggiungibile dall'esterno):
   ```powershell
   .\install.ps1 -SourceDir ..\..\target\release -TrustProxy
   # oppure: omi-mcp service install --db ... --bind 127.0.0.1 --trust-proxy --now
   ```
3. Da PowerShell come amministratore:
   ```powershell
   .\install-caddy.ps1 -CaddyExe C:\Download\caddy.exe -Dominio omi.esempio.it -Email nome@esempio.it
   ```
   Lo script valida il `Caddyfile`, crea il servizio `caddy` (avvio automatico, dipendente da `OMIMyMCP`,
   riavvio dopo i guasti), apre TCP 80 e 443 e verifica `https://<dominio>/health`.
4. Rimozione: `.\uninstall.ps1 -RemoveCaddy` (servizio, regole firewall 80/443 e cartella dei programmi);
   con `-RemoveData` anche `C:\ProgramData\Caddy` (certificati, stato, log di accesso).

Per la rete locale, copiare `Caddyfile.lan.example` in `C:\ProgramData\Caddy\Caddyfile` e usare `caddy trust`
(sulla macchina di Caddy) o importare `pki\authorities\local\root.crt` negli altri client.

## Installazione su Linux

1. Installare Caddy con il gestore di pacchetti (<https://caddyserver.com/docs/install>) oppure scaricare il binario da
   <https://caddyserver.com/download>: lo script non scarica nulla.
2. Installare OMIMyMCP in modo che accetti solo il proxy (come root):
   ```bash
   sudo ./install.sh --source-dir ../../target/release --trust-proxy --log
   ```
3. Configurare Caddy (con il pacchetto della distribuzione, oppure `--caddy-bin ~/Download/caddy` per il binario scaricato):
   ```bash
   sudo ./install-caddy.sh --domain omi.esempio.it --email nome@esempio.it
   ```
   Lo script valida il `Caddyfile` (scritto in `/etc/caddy`, con certificati in `/var/lib/caddy/data` e log di accesso in
   `/var/log/caddy`), abilita il servizio `caddy`, apre TCP 80 e 443 e verifica `https://<dominio>/health`. Con `--dry-run`
   mostra le azioni senza eseguirle.
4. Rimozione: `sudo ./uninstall.sh --remove-caddy` (disabilita Caddy e toglie il Caddyfile generato; il pacchetto si
   rimuove con il gestore di pacchetti).

Nei Caddyfile di esempio i percorsi `C:/ProgramData/Caddy/...` sono quelli di Windows: gli script per Linux li sostituiscono.

## Requisiti di rete

- Un **nome DNS pubblico** (record A/AAAA) che punti all'IP pubblico della macchina, oppure un servizio DNS
  dinamico.
- **Inoltro delle porte 80 e 443** sul router o NAT verso la macchina. Alcuni provider bloccano la porta 80 sulle
  linee domestiche: poiché il challenge HTTP-01 di Let's Encrypt la usa, in quel caso serve il challenge DNS
  (modulo DNS del provider, build personalizzata di Caddy) oppure il tunnel `cloudflared`.
- Firewall: TCP 80 e 443 in ingresso (le crea `install-caddy.ps1` su Windows; `install-caddy.sh` le apre in ufw/firewalld); TCP 8765 **non** esposta.
- Conservare `C:\ProgramData\Caddy\data` (Linux: `/var/lib/caddy/data`): contiene certificati e chiavi; un'emissione ripetuta può urtare i limiti
  di Let's Encrypt.

## Collegamento a claude.ai

**Impostazioni → Connettori → Aggiungi connettore personalizzato**, nome (es. `Quotazioni OMI`) e URL
`https://<dominio>/mcp`; poi attivare il connettore nella conversazione. Nei piani Team ed Enterprise lo aggiunge un
amministratore (Owner).

## Sicurezza

Il server è in sola lettura, ma espone dati soggetti alle condizioni dell'Agenzia delle Entrate (vedi il README
e il capitolo 16 del manuale tecnico): non lasciarlo aperto a Internet senza protezioni.

Opzioni e compatibilità con i connettori di claude.ai (documentazione Anthropic consultata il 25/10/2026,
<https://claude.com/docs/connectors/building/authentication>; i dettagli cambiano, ricontrollare prima di affidarsi):

| Protezione | Compatibile con claude.ai? | Note |
|---|---|---|
| **Restrizione per IP in Caddy** | **Sì** | Il traffico in uscita di Anthropic verso il server parte da `160.79.104.0/21` (elenco ufficiale: <https://platform.claude.com/docs/en/api/ip-addresses>). Va decommentata la regola `@esterni` nel `Caddyfile`, aggiungendo i propri IP per le prove. Il Caddyfile blocca tutto il resto, `/health` compreso. |
| **Token Bearer** (`--token`/`--token-file` di `omi-mcp`) | **Solo in parte** | Il connettore accetta una credenziale fissa nelle intestazioni (`static_headers`, es. `Authorization`), ma la funzione è in **beta per un insieme limitato di organizzazioni**: se nella finestra del connettore manca la sezione "Request headers", non è disponibile. Funziona invece con Claude Code (`--header "Authorization: Bearer <token>"`) e con `mcp-remote`. |
| **OAuth 2.0** (DCR o CIMD) | Sì, in teoria | Non è implementato da `omi-mcp`: richiederebbe un server di autorizzazione. Fuori dall'ambito del progetto. |
| **Autenticazione sul proxy** (Basic, `forward_auth`, ecc.) | **No** | Il connettore non sa presentare un'autenticazione interattiva o Basic al proxy. Utile solo per altri client. |
| **Nessuna** | Sì | Chiunque conosca l'URL può interrogare il server. Sconsigliato senza restrizione per IP. |

Scelta consigliata per un connettore di claude.ai: **restrizione per IP** (sempre), più il **token Bearer** se
l'organizzazione ha le intestazioni statiche. Con entrambi, anche un indirizzo dell'intervallo di Anthropic
senza il token riceve 401.

Nel log del server, con `--trust-proxy`, compare l'IP del client originale (da `X-Forwarded-For`) seguito da
`(via 127.0.0.1)`. Usare `--trust-proxy` solo dietro un proxy affidabile: l'intestazione si può falsificare se
il server è raggiungibile direttamente, ed è per questo che deve ascoltare solo su `127.0.0.1`.
