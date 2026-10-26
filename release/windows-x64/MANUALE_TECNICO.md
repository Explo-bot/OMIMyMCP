<!--
OMIMyMCP-rs - Copyright 2026 Explobot
SPDX-License-Identifier: Apache-2.0
-->

# OMIMyMCP-rs: manuale tecnico

Versione 1.0.0 · aggiornato al 25/10/2026

Questo manuale descrive il programma così com'è, per **Windows** e per **Linux**: i comandi e le opzioni citati sono
controllati contro l'help dei due eseguibili e gli script in `deploy/`. Dove una prova richiede privilegi di
amministratore (root), un dominio pubblico o un account Claude, il testo lo dice.

## Indice

1. [Introduzione](#1-introduzione)
2. [Requisiti](#2-requisiti)
3. [Compilazione](#3-compilazione)
4. [Ottenere i dati OMI](#4-ottenere-i-dati-omi)
5. [Caricamento con omi-import](#5-caricamento-con-omi-import)
6. [Esecuzione di omi-mcp da riga di comando](#6-esecuzione-di-omi-mcp-da-riga-di-comando)
7. [Installazione come servizio (Windows e Linux)](#7-installazione-come-servizio-windows-e-linux)
8. [Esposizione HTTPS con Caddy](#8-esposizione-https-con-caddy)
9. [Uso del server MCP da Claude](#9-uso-del-server-mcp-da-claude)
10. [Strumenti MCP](#10-strumenti-mcp)
11. [Struttura del database](#11-struttura-del-database)
12. [Sicurezza](#12-sicurezza)
13. [Disinstallazione](#13-disinstallazione)
14. [Risoluzione dei problemi](#14-risoluzione-dei-problemi)
15. [Scelte di progetto](#15-scelte-di-progetto)
16. [Licenza](#16-licenza)
17. [Appendice A: riferimento della riga di comando](#appendice-a-riferimento-della-riga-di-comando)
18. [Appendice B: cartelle installate](#appendice-b-cartelle-installate)
19. [Appendice C: file prodotti](#appendice-c-file-prodotti)

Percorsi usati negli esempi **Windows**: programmi in `C:\Program Files\OMIMyMCP`, dati in `C:\ProgramData\OMIMyMCP`
(database `C:\ProgramData\OMIMyMCP\omi.db`), porta TCP 8765, ZIP scaricati in `C:\DatiOMI`; i comandi sono per PowerShell.
Percorsi usati negli esempi **Linux**: programmi in `/opt/omimymcp`, dati in `/var/lib/omimymcp` (database
`/var/lib/omimymcp/omi.db`), ZIP scaricati in `~/DatiOMI`; i comandi sono per bash.

---

## 1. Introduzione

Il sistema porta le quotazioni immobiliari dell'Osservatorio del Mercato Immobiliare (OMI) dell'Agenzia delle
Entrate dentro un database SQLite e le rende interrogabili da Claude, o da qualunque altro client, tramite il
*Model Context Protocol* (MCP). È formato da due programmi che condividono la stessa libreria:

| Programma | Che cosa fa |
|---|---|
| `omi-import` | Riga di comando. Legge i file `QI*.zip` (quotazioni) e `QIP*.zip` (quotazioni e perimetri delle zone) scaricati dal sito dell'Agenzia e li carica in `omi.db`. |
| `omi-mcp` | Server MCP (HTTP *streamable* su `/mcp`). Apre `omi.db` **in sola lettura** ed espone 9 strumenti: elenco dei semestri e delle tipologie, ricerca dei comuni, zone di un comune, dettaglio di una zona, zona da coordinate, zone vicine, serie storica, statistiche. Si avvia da console o si installa come servizio (Windows) o come unità systemd (Linux). |

```
 QI*.zip / QIP*.zip              omi.db                      HTTP :8765            HTTPS :443
 (Agenzia delle Entrate)   (SQLite, WAL, R*Tree)
        |                          |                            |                     |
        v                          v                            v                     v
   +-----------+   scrive    +-----------+   legge     +-----------+   (facoltativo)  +--------+     +--------------+
   | omi-import| ----------> |  omi.db   | <---------- |  omi-mcp  | <--------------- | Caddy  | <-- | client MCP   |
   +-----------+             +-----------+  sola       +-----------+   reverse proxy  +--------+     | Claude, ...  |
                                            lettura          ^                                       +--------------+
                                                             +--- connessione diretta (stesso PC o LAN) ---> client MCP
```

In breve: ZIP → `omi-import` → `omi.db` → `omi-mcp` → [Caddy] → client MCP.

Caddy è un programma esterno e facoltativo: serve solo quando il server deve essere raggiunto da Internet in
HTTPS, per esempio dal connettore personalizzato di claude.ai (capitolo 8).

### Avvertenze

- Il software è fornito **"così com'è"**, senza garanzie; l'uso è a rischio dell'utente.
- Le quotazioni OMI hanno valore indicativo e **non sostituiscono la stima di un professionista**.
- Non esporre il server a Internet senza protezioni adeguate (capitolo 12).
- Ogni risultato basato sui dati va accompagnato dalla fonte **"Agenzia delle Entrate - OMI"**.

I dati OMI non sono inclusi nel progetto e restano soggetti alle condizioni d'uso dell'Agenzia (capitolo 16).

---

## 2. Requisiti

### Per l'esecuzione

| Voce | Requisito |
|---|---|
| Sistema operativo | **Windows** 10/11 o Windows Server 2016 e successivi, a 64 bit; **Linux** a 64 bit con systemd (per l'installazione come servizio) |
| Software da installare | Nessuno: i binari Windows sono compilati con il runtime C statico (`crt-static`); entrambi contengono SQLite. Su Linux il binario usa la libreria C di sistema (glibc) |
| Spazio su disco | Circa 2 GB per l'intera serie storica 2004–2025; tre semestri (2005/2, 2011/1, 2025/1) occupano 159 MB. Per caricare servono anche gli ZIP (circa 3,5 GB per l'intera serie) |
| Rete | Una porta TCP libera (predefinita 8765) |
| Memoria | Il server usa poche decine di MB (23 MB al massimo nelle misure con dati reali) |

Il servizio Windows (`omi-mcp service ...`) e gli script PowerShell sono solo per Windows; su Linux si usano gli script
bash in `deploy/linux/` e un'unità systemd.

### Per la compilazione

| Voce | Requisito |
|---|---|
| Rust | `rustup` con la toolchain stabile (`x86_64-pc-windows-msvc` su Windows, `x86_64-unknown-linux-gnu` su Linux); versione minima **1.99** (`rust-version` del workspace) |
| Compilatore C | **Windows:** Visual Studio Build Tools con il carico di lavoro "Sviluppo di applicazioni desktop con C++" (serve a compilare SQLite e a collegare con MSVC). **Linux:** `gcc` (o `clang`) e `make`, ad esempio `build-essential` su Debian/Ubuntu |
| Git | Per ottenere i sorgenti |

---

## 3. Compilazione

### 3.1 Installare gli strumenti

**Windows**

1. Installare i **Build Tools per Visual Studio** (<https://visualstudio.microsoft.com/downloads/>, sezione
   "Strumenti per Visual Studio"), scegliendo il carico di lavoro C++.
2. Installare **rustup** da <https://rustup.rs> e accettare la toolchain predefinita (`stable`, target MSVC).
3. Installare Git da <https://git-scm.com/download/win>.
4. Aprire un nuovo terminale e verificare:

```powershell
rustc --version      # 1.99.0 o successiva
cargo --version
git --version
rustup target list --installed    # deve comprendere x86_64-pc-windows-msvc
```

**Linux** (Debian/Ubuntu; su altre distribuzioni usare i pacchetti equivalenti):

```bash
sudo apt install build-essential git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh     # accettare la toolchain predefinita (stable)
. "$HOME/.cargo/env"
rustc --version      # 1.99.0 o successiva
cargo --version
```

### 3.2 Compilare

Dalla cartella del progetto (stesso comando su Windows e Linux):

```powershell
cargo build --release --workspace
```

Su **Windows** gli eseguibili sono in `target\release\`: `omi-import.exe` e `omi-mcp.exe` (rispettivamente circa 4,4 MB e 7,5 MB).
Verifica:

```powershell
target\release\omi-import.exe --version
target\release\omi-mcp.exe --version
target\release\omi-import.exe --help
target\release\omi-mcp.exe --help
```

Se `link.exe` non viene trovato o compaiono errori del compilatore C, mancano i Build Tools con il carico di lavoro
C++: aggiungerli, oppure compilare da "Developer PowerShell for VS".

Su **Linux** gli eseguibili sono in `target/release/` (`omi-import` e `omi-mcp`, senza estensione):

```bash
cargo build --release --workspace
target/release/omi-import --version
target/release/omi-mcp --version
```

### 3.3 Test e controlli di qualità

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

La suite comprende test unitari, di integrazione (import, strumenti, server HTTP con un client rmcp reale), controlli
trasversali sul repository (intestazioni di licenza, arrotondamenti, assenza di codici fiscali) e 40 risultati di
riferimento ("golden", in `crates/omi-core/tests/golden/`). Gira allo stesso modo su Windows e su Linux. Alcuni test
sono marcati `#[ignore]` perché richiedono dati reali o misurano le prestazioni:

```bash
# Import di ZIP reali messi in dati-test/ (non versionata) o in OMI_DATI_TEST, con controlli di integrità
cargo test --release -p omi-core --test dati_reali -- --ignored --nocapture
# Prestazioni del server su un database reale
OMI_PROVA_DB=/percorso/omi.db cargo test --release -p omi-mcp --test prestazioni -- --ignored --nocapture
```

### 3.4 Compilazione per Linux

Il codice del servizio Windows è compilato solo con `#[cfg(windows)]`, quindi tutto il resto si compila e passa i test su
Linux senza modifiche:

```bash
cargo build --release --workspace
cargo test --workspace
```

Su Linux `omi-mcp service ...` risponde "Disponibile solo su Windows": l'avvio automatico si ottiene con un'unità systemd
(§7.8) creata da `deploy/linux/install.sh`. Su Linux `omi-mcp` si ferma in modo ordinato con `SIGINT` (Ctrl+C) e con `SIGTERM`
(quello inviato da `systemctl stop`). Per compilare per un'altra architettura si usa `--target` (ad esempio
`aarch64-unknown-linux-gnu`, con il relativo linker).

### 3.5 Pacchetti di rilascio

**Windows.** Lo script `deploy\windows\build-release.ps1` esegue i test, compila in release e produce lo ZIP da distribuire in
`dist\`:

```powershell
.\deploy\windows\build-release.ps1
```

Il risultato è `dist\OMIMyMCP-rs-<versione>-win-x64.zip`, con il file `SHA256SUMS.txt` accanto. Contenuto: gli
eseguibili, `LICENSE`, `NOTICE`, questo manuale, `LEGGIMI.md` (istruzioni d'uso del pacchetto), gli script `deploy\windows\*.ps1` e gli
esempi di Caddyfile (con il `LEGGIMI.md` di Caddy). Gli eseguibili del pacchetto sono quelli compilati con
`--target x86_64-pc-windows-msvc`, in `target\x86_64-pc-windows-msvc\release\`. Parametri facoltativi: `-SkipTests` (salta `cargo test`),
`-OutDir` (cartella di destinazione) e `-CertThumbprint`, che firma gli eseguibili con `signtool` se si dispone di un certificato di firma
del codice (con `-TimestampUrl` per il server di marcatura temporale). Lo script supporta `-WhatIf`. Per verificare lo ZIP:
`Get-FileHash dist\OMIMyMCP-rs-1.0.0-win-x64.zip` deve coincidere con `dist\SHA256SUMS.txt`.

**Linux.** Lo script `deploy/linux/build-release.sh` fa lo stesso e produce un archivio `.tar.gz`:

```bash
./deploy/linux/build-release.sh               # test, compilazione, pacchetto in dist/
./deploy/linux/build-release.sh --skip-tests --out-dir /tmp/rilascio
```

Il risultato è `dist/OMIMyMCP-rs-<versione>-linux-x64.tar.gz` con `SHA256SUMS-linux.txt` accanto (`sha256sum -c`). Contiene i
due eseguibili, `LICENSE`, `NOTICE`, `README.md`, questo manuale, gli script `deploy/linux/*.sh`, l'unità `omi-mcp.service` e gli
esempi di Caddyfile.

**Eseguibili Windows già compilati nel repository.** La cartella `release/windows-x64/` contiene il pacchetto Windows già pronto,
con il suo `LEGGIMI.md`: non serve compilare per usarlo.

---

## 4. Ottenere i dati OMI

L'Agenzia delle Entrate distribuisce i dati OMI con il servizio **Forniture dati OMI**, riservato agli utenti
autenticati (<https://www.agenziaentrate.gov.it/portale/it/web/guest/schede/fabbricatiterreni/omi/forniture-dati-omi>).

| Fornitura | Disponibile da | File ottenuto | Contenuto |
|---|---|---|---|
| Quotazioni immobiliari | 1° semestre 2004 | `QI<n. richiesta>_<codice fiscale>.zip` (circa 2 MB per l'Italia) | `…_AAAAS_ZONE.csv`, `…_AAAAS_VALORI.csv` |
| Perimetri delle zone OMI | 2° semestre 2010 | `QIP<n. richiesta>_<codice fiscale>.zip` (40–130 MB per l'Italia) | gli stessi due CSV **più** un file `.kml` per comune |

- Ogni file riguarda **un solo semestre**. L'ambito può essere nazionale, regionale, provinciale o comunale;
  forniture parziali dello stesso semestre si sommano.
- Dal 2° semestre 2010 basta la fornitura dei perimetri (`QIP`), che contiene anche le quotazioni. Per i semestri dal
  2004/1 al 2010/1 esistono solo le quotazioni (`QI`).
- L'intera serie 2004–2025 occupa circa 3,5 GB di ZIP e produce un database di circa 2 GB.

### Procedura di download

1. Aprire la pagina del servizio indicata sopra e premere **Accedi al servizio** (oppure
   <https://telematici.agenziaentrate.gov.it/Main/index.jsp>).
2. Autenticarsi con SPID, CIE, CNS o credenziali Fisconline/Entratel.
3. Aprire **Forniture dati OMI** (se non compare nel menu, usare la ricerca dell'area riservata).
4. Scegliere la fornitura, l'ambito territoriale e il semestre, poi inviare la richiesta.
5. Le richieste sono elaborate in modo asincrono: il file compare nell'elenco delle forniture richieste, di solito
   dopo pochi minuti.
6. Salvare tutti gli ZIP in una stessa cartella (per esempio `C:\DatiOMI`), **senza rinominarli né estrarli**.

### Privacy dei nomi dei file

Il nome del file contiene il **codice fiscale di chi ha fatto la richiesta**. Gli ZIP non vanno condivisi. Nel
database si registra solo il nome ripulito (`QIP1438117_RSSMRA80A01H501U.zip` diventa `QIP1438117.zip`; il codice
fiscale è qui un esempio sintetico) e `omi-import` stampa sempre il nome ripulito. Anche il log del server e gli script non
mostrano mai il nome originale.

---

## 5. Caricamento con omi-import

### 5.1 Sintassi

```
omi-import [QI*.zip ...] [database.db]
omi-import --help | -h | /?
omi-import --version
```

| Parametro | Significato |
|---|---|
| `QI*.zip`, `QIP*.zip` | Forniture scaricate, senza rinominarle. Si possono indicare più file e usare i caratteri jolly `*` e `?` (li espande `omi-import`, quindi funzionano anche in PowerShell e nel Prompt dei comandi). Il nome deve iniziare con `QI` (anche `QIP`), senza distinzione di maiuscole |
| `database.db` | Database da aggiornare. Se non è indicato vale `omi.db` nella cartella corrente. Se non esiste viene creato |
| `--help`, `-h`, `/?` | Mostra l'aiuto |
| `--version` | Mostra nome, versione e autore |

Gli argomenti possono essere dati **in qualunque ordine**: l'argomento che non è uno ZIP `QI*` è considerato il
database (se ce ne sono più d'uno vale l'ultimo). **Senza argomenti** carica tutti i file `QI*.zip` e `QIP*.zip` della
cartella corrente in `omi.db`.

### 5.2 Primo caricamento

```powershell
New-Item -ItemType Directory -Force C:\ProgramData\OMIMyMCP | Out-Null
& 'C:\Program Files\OMIMyMCP\omi-import.exe' 'C:\DatiOMI\QI*.zip' 'C:\ProgramData\OMIMyMCP\omi.db'
```

Esempio di output (con forniture sintetiche di prova). Su Linux il comando equivalente è
`omi-import ~/DatiOMI/QI*.zip /var/lib/omimymcp/omi.db` (eseguito come utente `omi`, vedi §7.8):

```
OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)
Fonte dei dati: Agenzia Entrate - OMI
Database: C:\ProgramData\OMIMyMCP\omi.db
Import di QI1438200.zip (0.0 MB)...
  Semestre 2025/1: 6 zone, 9 valori
  completato in 0.0 s
Import di QIP1438117.zip (0.0 MB)...
  Semestre 2024/2: 6 zone, 9 valori
  Semestre 2024/2: 3 perimetri di zona da 2 file KML
  completato in 0.0 s
Aggiornamento anagrafiche e indici...
Caricati 2/2 file.
Contenuto del database:
  Ultimo aggiornamento:     25/10/2026 22:34
  Ultimo semestre caricato: 2025/1 (elaborazione Agenzia del 12/03/2026)
  Semestri disponibili:     2 (2024/2 - 2025/1), 1 con perimetri
  Comuni censiti:           5 nell'ultimo semestre (6 zone), 5 in totale nella serie storica
```

Con le forniture reali si aggiungono le righe di avanzamento `1000/7887 file KML...` ogni 1000 file. Il pattern
`QI*.zip` comprende anche i file `QIP*.zip`.

### 5.3 Aggiungere un semestre

Basta lanciare di nuovo il comando con il nuovo ZIP e lo stesso database:

```powershell
& 'C:\Program Files\OMIMyMCP\omi-import.exe' 'C:\DatiOMI\QIP1500001_*.zip' 'C:\ProgramData\OMIMyMCP\omi.db'
```

I semestri già presenti non vengono toccati.

### 5.4 Ricaricamento e correzione

| Situazione | Che cosa succede |
|---|---|
| Fornitura **nazionale** di un semestre già caricato | Sostituisce l'intero semestre |
| Forniture di regioni, province o comuni **diversi** dello stesso semestre | Si sommano |
| Stesso file ricaricato | Non crea duplicati: il database resta identico (verificato su dati reali) |
| `QI` (senza perimetri) su un semestre che ha già i perimetri | I CSV sostituiscono zone e quotazioni, ma i **perimetri già caricati restano** |
| Comune presente nel database ma assente dalla nuova fornitura | Non viene cancellato |
| Ripartire da zero | Eliminare `omi.db` (con `omi.db-wal` e `omi.db-shm`, se presenti) e ricaricare gli ZIP |

Per correggere un errore basta quindi ricaricare lo ZIP giusto. Ogni file è elaborato in **una sola transazione**:
se fallisce, il database resta com'era prima di quel file, e gli altri file vengono caricati lo stesso.

### 5.5 Codici d'uscita

| Codice | Significato |
|---|---|
| 0 | Caricamento riuscito (uno ZIP senza CSV né KML OMI viene ignorato e non è un errore) |
| 1 | Parametri non validi o errore grave (database non apribile, file inesistente, nessun file corrispondente) |
| 2 | Almeno un file non è stato caricato (ZIP danneggiato, semestre non determinabile, ...): gli altri sono stati caricati |

Esempio con uno ZIP danneggiato accanto a uno valido:

```
Import di QI9999999_X.zip (0.0 MB)...
  ERRORE su QI9999999_X.zip: errore ZIP: invalid Zip archive: Could not find EOCD
Import di QI1438200.zip (0.0 MB)...
  Semestre 2025/1: 6 zone, 9 valori
  completato in 0.0 s
Aggiornamento anagrafiche e indici...
Caricati 1/2 file.
```

Nei semestri 2011/1 il file `C302.kml` (Castiglione Chiavarese) è malformato: l'importatore stampa
`Attenzione: C302.kml malformato (...); letti 3 placemark, scartati 1` e carica le zone leggibili (la B1 manca).
È un caso noto, non un errore.

### 5.6 Verifica del database

Con lo strumento `sqlite3` (<https://sqlite.org/download.html>, "sqlite-tools") oppure un qualunque client SQLite:

```powershell
# Linux: sqlite3 -readonly /var/lib/omimymcp/omi.db "..."
sqlite3 -readonly C:\ProgramData\OMIMyMCP\omi.db "SELECT semestre, n_zone, n_valori, n_geometrie FROM semestre ORDER BY semestre;"
```

`n_geometrie` è vuoto (NULL) per i semestri caricati solo con `QI`. Per una query sulle quotazioni (abitazioni civili,
stato conservativo prevalente, comune di Milano, 2° semestre 2025):

```sql
SELECT z.comune_descr, z.zona, z.zona_descr, v.compr_min, v.compr_max, v.loc_min, v.loc_max
FROM zona z JOIN valore v ON v.semestre = z.semestre AND v.link_zona = z.link_zona
WHERE z.semestre = 20252 AND z.comune_amm = 'F205' AND v.cod_tip = 20 AND v.stato_prev = 'P'
ORDER BY v.compr_max DESC;
```

`omi-import` stampa lo stesso riepilogo a fine caricamento, e `omi-mcp` lo stampa all'avvio.

### 5.7 Tempi indicativi

Misurati in release su Windows 11 (Intel Core i5-11400F, 32 GB) con forniture reali:

| Fornitura | Dimensione | Tempo |
|---|---|---|
| QI nazionale 2005/2 | 2,2 MB | 0,85 s |
| QIP nazionale 2011/1 (7399 KML) | 44,3 MB | 2,6 s |
| QIP nazionale 2025/1 (7887 KML) | 107,6 MB | 4,6 s |
| Aggiornamento finale delle anagrafiche e degli indici | | 0,8 s |

### 5.8 Caricare con il server attivo

Il server legge il database in sola lettura e il database usa il journal WAL, quindi un import può girare mentre il
server è in funzione: le interrogazioni continuano a rispondere e vedono i dati nuovi dopo il commit di ogni file.
Per i caricamenti nazionali grandi è comunque più prudente fermare il servizio durante l'import: lo fanno gli script
`aggiorna-dati.ps1` (§7.6) e `aggiorna-dati.sh` (§7.8). Le interrogazioni che si scontrano con una scrittura attendono fino a 5 secondi
(`busy_timeout`).

---

## 6. Esecuzione di omi-mcp da riga di comando

### 6.1 Sintassi

```
omi-mcp [database.db] [porta] [/l] [--bind INDIRIZZO] [--trust-proxy]
        [--token SEGRETO | --token-file FILE] [--allowed-host HOST ...]
```

| Opzione | Significato |
|---|---|
| `database.db` | Database creato con `omi-import`; predefinito `omi.db`. Viene aperto **in sola lettura** e non viene mai creato: se manca il programma esce con `Database non trovato: … Crearlo con omi-import.` |
| `porta` | Porta TCP; predefinita 8765; `0` = scelta dal sistema |
| `/l`, `-l`, `--log` | Registra le richieste HTTP e le chiamate ai tool nel file `<database>.log` (per `omi.db` è `omi.log`). In Git Bash usare `-l`, perché `/l` viene convertito in un percorso |
| `--bind INDIRIZZO` | Indirizzo di ascolto. Predefinito: tutte le interfacce (IPv4 e IPv6). Dietro Caddy usare `--bind 127.0.0.1` |
| `--trust-proxy` | Nel log usa l'IP del client indicato da `X-Forwarded-For`. Da usare solo dietro un proxy affidabile |
| `--token SEGRETO` | Richiede `Authorization: Bearer SEGRETO` su `/mcp` (`/` e `/health` restano liberi) |
| `--token-file FILE` | Come `--token`, con il segreto nella prima riga del file. Preferibile a `--token`, che resta visibile nell'elenco dei processi |
| `OMI_MCP_TOKEN` | Variabile d'ambiente alternativa alle due opzioni (l'opzione esplicita ha la precedenza; `--token` e `--token-file` insieme sono un errore) |
| `--allowed-host HOST` | Accetta su `/mcp` solo le richieste con questo `Host` (oltre a `localhost`, `127.0.0.1` e `::1`); ripetibile. Senza l'opzione l'`Host` non viene controllato |
| `--help`, `-h`, `/?`, `--version` | Aiuto e versione |

Endpoint: `/mcp` (MCP su HTTP streamable, senza sessioni: `GET` e `DELETE` rispondono 405), `/` (testo informativo),
`/health` (200 `ok` se il database risponde, altrimenti 503).

### 6.2 Esempi

Solo sul PC locale (Claude Code, Claude Desktop con `mcp-remote`):

```powershell
omi-mcp.exe C:\ProgramData\OMIMyMCP\omi.db 8765 --bind 127.0.0.1
```

Su Linux (stessi argomenti, senza `.exe`):

```bash
omi-mcp /var/lib/omimymcp/omi.db 8765 --bind 127.0.0.1
```

Sulla rete locale, con token e log (a questo indirizzo il server è raggiungibile da altri PC: aprire la porta nel
firewall e usare il token):

```powershell
Set-Content C:\ProgramData\OMIMyMCP\token.txt 'una-stringa-lunga-e-casuale'
omi-mcp.exe C:\ProgramData\OMIMyMCP\omi.db 8765 /l --token-file C:\ProgramData\OMIMyMCP\token.txt
```

Dietro Caddy (ascolta solo sul loopback e registra l'IP originale dei client):

```powershell
omi-mcp.exe C:\ProgramData\OMIMyMCP\omi.db 8765 /l --bind 127.0.0.1 --trust-proxy
```

Con un nome di dominio specifico accettato su `/mcp`:

```powershell
omi-mcp.exe C:\ProgramData\OMIMyMCP\omi.db 8765 --bind 127.0.0.1 --trust-proxy --allowed-host omi.esempio.it
```

All'avvio il programma stampa il riepilogo del database e l'indirizzo, poi attende:

```
OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)
Fonte dei dati: Agenzia Entrate - OMI
Database: C:\ProgramData\OMIMyMCP\omi.db
Contenuto del database:
  Ultimo aggiornamento:     25/10/2026 22:34
  Ultimo semestre caricato: 2025/1 (elaborazione Agenzia del 12/03/2026)
  Semestri disponibili:     2 (2024/2 - 2025/1), 1 con perimetri
  Comuni censiti:           5 nell'ultimo semestre (6 zone), 5 in totale nella serie storica
Server MCP in ascolto su http://127.0.0.1:8765/mcp
2026-10-25 22:34:51.055 Log attivo, file: C:\ProgramData\OMIMyMCP\omi.log
Ctrl+C per terminare.
```

Con un bind su tutte le interfacce l'indirizzo mostrato è `http://localhost:8765/mcp`.

### 6.3 Formato del log

Il file `<database>.log` è in *append* (mai svuotato). Ogni riga comincia con data e ora locali:

```
2026-10-25 22:34:53.103 HTTP 127.0.0.1 GET /health -> 200 (0 ms)
2026-10-25 22:34:53.108 HTTP 127.0.0.1 POST /mcp -> 401 (0 ms)
2026-10-25 22:34:53.165 TOOL elenco_semestri {} -> ok, 510 caratteri (0 ms)
2026-10-25 22:34:53.177 TOOL dettaglio_zona {"comune":"samone","zona":"R1"} -> ERRORE: 'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754] (0 ms)
```

- `HTTP <ip> <metodo> <percorso> -> <stato> (<ms> ms)`: una riga per richiesta (senza query string).
- `TOOL <nome> <argomenti> -> ok, <n> caratteri (<ms> ms)` oppure `-> ERRORE: <messaggio> (<ms> ms)`.
- Gli errori interni (database, panic) vanno in una riga a parte `ERRORE INTERNO in '<tool>': <dettaglio>`; al client arriva solo
  `Errore interno durante l'esecuzione di '<tool>'.`
- Un errore di scrittura sul log non blocca mai il server.
- Il log contiene IP e testi delle richieste: va trattato come dato personale (capitolo 12). Il token non compare mai.

### 6.4 Arresto

`Ctrl+C` nella finestra della console (su Linux anche `SIGTERM`, cioè `kill <pid>` o `systemctl stop`). Il server termina dopo aver concluso le richieste in corso.

---

## 7. Installazione come servizio (Windows e Linux)

I paragrafi 7.1–7.7 riguardano **Windows**; l'installazione su **Linux** (systemd) è al §7.8.

Il servizio Windows si chiama `OMIMyMCP` e parte automaticamente all'avvio del PC. Tutti i comandi di questo capitolo
vanno eseguiti da una **PowerShell aperta come amministratore**. Senza privilegi `omi-mcp service install` risponde:
`Servono i privilegi di amministratore: aprire il Prompt dei comandi o PowerShell con "Esegui come amministratore".`

### 7.1 Installazione rapida

Prerequisito: il database deve esistere già in `C:\ProgramData\OMIMyMCP\omi.db` (§5.2). Dal pacchetto di rilascio
estratto (o dalla cartella del progetto, indicando `target\release` come `-SourceDir`):

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\deploy\windows\install.ps1 -SourceDir .\ -Log -WithToken -AddToPath
```

(`-Firewall` serve solo se il server deve essere raggiungibile da altri PC, cioè con `-Bind 0.0.0.0` o l'IP della scheda: vedi gli esempi più sotto.)

Lo script:

1. se il servizio esiste già, lo disinstalla (lasciando regola del firewall, database, log e token);
2. copia `omi-mcp.exe`, `omi-import.exe`, `LICENSE`, `NOTICE` e `MANUALE_TECNICO.md` in `C:\Program Files\OMIMyMCP`;
3. crea `C:\ProgramData\OMIMyMCP` e, con `-WithToken`, genera un token casuale in `token.txt` (se manca), leggibile solo
   da Administrators e dall'account del servizio;
4. si ferma con un avviso se `omi.db` non esiste (dopo aver copiato i programmi): caricare il database e rilanciare lo script;
5. installa e avvia il servizio (`omi-mcp service install ... --now`), verificando `/health`;
6. con `-AddToPath` aggiunge `C:\Program Files\OMIMyMCP` al `PATH` di sistema (vale per le nuove finestre).

| Parametro di `install.ps1` | Significato (predefinito) |
|---|---|
| `-SourceDir` (obbligatorio) | Cartella con `omi-mcp.exe` e `omi-import.exe` |
| `-InstallDir` | Cartella dei programmi (`C:\Program Files\OMIMyMCP`) |
| `-DataDir` | Cartella dei dati: `omi.db`, `omi.log`, `token.txt` (`C:\ProgramData\OMIMyMCP`) |
| `-Port` | Porta TCP (8765) |
| `-Bind` | Indirizzo di ascolto (`127.0.0.1`, cioè solo locale) |
| `-ServiceName` | Nome del servizio (`OMIMyMCP`) |
| `-Log` | Registra anche richieste HTTP e chiamate ai tool in `omi.log` |
| `-Firewall` | Aggiunge la regola in ingresso nel firewall di Windows |
| `-WithToken` | Richiede il token Bearer su `/mcp` |
| `-TrustProxy` | Aggiunge `--trust-proxy`; da usare dietro Caddy, con `-Bind 127.0.0.1` e senza `-Firewall` |
| `-AddToPath` | Aggiunge `InstallDir` al `PATH` di sistema |

Lo script supporta `-WhatIf`, che mostra le azioni senza eseguirle.

Esempi:

```powershell
# Solo sul PC locale (Claude Code, Claude Desktop)
.\deploy\windows\install.ps1 -SourceDir .\

# Sulla rete locale, con token e firewall
.\deploy\windows\install.ps1 -SourceDir .\ -Bind 0.0.0.0 -WithToken -Firewall -Log

# Dietro Caddy
.\deploy\windows\install.ps1 -SourceDir .\ -TrustProxy -Log
```

Se si usa il token, il client deve inviare `Authorization: Bearer <contenuto di token.txt>`.

### 7.2 Installazione manuale

Equivale ai passi 2–5 dello script:

```powershell
New-Item -ItemType Directory -Force 'C:\Program Files\OMIMyMCP', 'C:\ProgramData\OMIMyMCP' | Out-Null
Copy-Item .\omi-mcp.exe, .\omi-import.exe 'C:\Program Files\OMIMyMCP'
& 'C:\Program Files\OMIMyMCP\omi-mcp.exe' service install `
    --db C:\ProgramData\OMIMyMCP\omi.db --port 8765 --bind 127.0.0.1 --log --now
```

Opzioni di `omi-mcp service install`:

| Opzione | Significato |
|---|---|
| `--db FILE` | Database (obbligatorio; il percorso viene reso assoluto) |
| `--name NOME` | Nome del servizio (predefinito `OMIMyMCP`; vale anche per `uninstall`, `start`, `stop` e `status`). Non può contenere `"`, `/` o `\` |
| `--display-name TESTO` | Nome visualizzato (predefinito "OMIMyMCP - Server MCP quotazioni OMI") |
| `--port PORTA` | Porta TCP (predefinita 8765; 0 non è ammessa) |
| `--bind INDIRIZZO` | Indirizzo di ascolto (predefinito `127.0.0.1`, sempre scritto negli argomenti del servizio) |
| `--log` | Registra anche richieste HTTP e chiamate ai tool in `<database>.log` |
| `--trust-proxy` | Usa `X-Forwarded-For` nel log |
| `--token-file FILE` | File con il token Bearer, leggibile dall'account del servizio |
| `--start auto\|delayed\|manual` | Avvio automatico (predefinito), automatico ritardato o manuale |
| `--account localservice\|networkservice\|localsystem` | Account del servizio (predefinito `localservice`, quello con meno privilegi) |
| `--firewall` | Aggiunge una regola in ingresso nel firewall di Windows, con lo stesso nome del servizio |
| `--no-acl` | Non concede all'account il permesso di modifica sulla cartella del database |
| `--now` | Avvia subito il servizio e verifica `/health` (esce con 1 se non risponde 200) |

Il servizio, in caso di errore, viene riavviato da Windows dopo 5, 30 e 60 secondi. Un servizio con lo stesso nome già
presente è un errore (`install.ps1` lo disinstalla prima).

### 7.3 Account del servizio e permessi sulla cartella dei dati

Il servizio gira per impostazione predefinita come **LocalService**. Il server apre il database in sola lettura, ma il
database è in modalità **WAL**: per leggerlo SQLite deve poter creare o aggiornare i file `omi.db-wal` e `omi.db-shm`
nella stessa cartella. Se l'account non può scrivere nella cartella del database, il servizio parte e si ferma subito
oppure risponde "database is locked" (vedi §14).

Per questo `service install` concede all'account del servizio il permesso di **modifica** su `C:\ProgramData\OMIMyMCP`
(con `icacls`), a meno che non si passi `--no-acl`. Per lo stesso motivo la cartella dei dati sta in
`C:\ProgramData` e non nel profilo di un utente, che LocalService non può leggere. Il file del token ha un'ACL
ancora più stretta: lettura per l'account del servizio e controllo completo per gli Administrators.

### 7.4 Firewall

- Con `--firewall` (o `-Firewall`) `service install` crea la regola in ingresso `OMIMyMCP` per il programma
  `omi-mcp.exe`; `service uninstall` la rimuove (a meno di `--keep-firewall`). Le regole omonime di altri programmi non
  vengono toccate: la cancellazione filtra anche per programma.
- Con il bind predefinito `127.0.0.1` la regola non serve: il server non è raggiungibile da altri PC.
- Dietro Caddy **non** aprire la porta 8765: la raggiunge solo il proxy, sul loopback (§8).

Verifica delle regole:

```powershell
netsh advfirewall firewall show rule name="OMIMyMCP"
```

### 7.5 Gestione del servizio

```powershell
omi-mcp.exe service status              # stato, tipo di avvio, account, percorso e argomenti
omi-mcp.exe service stop
omi-mcp.exe service start
sc.exe query OMIMyMCP                   # stato secondo Windows
services.msc                            # interfaccia grafica: cercare "OMIMyMCP"
```

Esempio di `service status` (qui su un servizio qualsiasi di Windows; per `OMIMyMCP` mostra anche gli argomenti e la
verifica di `/health`):

```
Servizio:      Spooler (Spooler di stampa)
Stato:         in esecuzione
Tipo di avvio: automatico
Account:       LocalSystem
Percorso:      C:\WINDOWS\System32\spoolsv.exe
Argomenti:     (nessuno)
PID:           4620
```

`status` funziona anche senza privilegi di amministratore. Se il servizio non esiste: `Il servizio OMIMyMCP non è installato.`
(codice d'uscita 1).

**Dove trovare i log.** Il servizio scrive sempre il suo ciclo di vita (avvio con l'attribuzione, configurazione con il token
sostituito da `********`, indirizzo di ascolto, arresto, errori) in `C:\ProgramData\OMIMyMCP\omi.log`. Con `--log` (o
`-Log`) lo stesso file contiene anche le righe `HTTP` e `TOOL`. Il Registro eventi di Windows non è usato. Se il
servizio si ferma da solo, `sc.exe query OMIMyMCP` mostra il codice specifico (`SERVICE_EXIT_CODE`): 1 database
mancante, 2 porta non disponibile, 3 altri errori.

### 7.6 Aggiornamento dei dati

Per caricare nuove forniture nel database del servizio:

```powershell
.\deploy\windows\aggiorna-dati.ps1 -ZipDir C:\DatiOMI
```

Lo script ferma il servizio, esegue `omi-import` sui file `QI*.zip` della cartella (`QIP*.zip` compresi), riavvia il servizio anche
se l'import non riesce e mostra il riepilogo. Non stampa i nomi dei file, perché contengono il codice fiscale. Il codice d'uscita è quello
di `omi-import` (0 tutto caricato, 2 almeno un file con errori). Parametri facoltativi: `-DataDir`, `-InstallDir`, `-ServiceName`;
supporta `-WhatIf`.

### 7.7 Aggiornamento dei binari

1. Fermare il servizio: `omi-mcp.exe service stop`.
2. Sostituire `omi-mcp.exe` e `omi-import.exe` in `C:\Program Files\OMIMyMCP` (con il servizio in esecuzione Windows non
   permette di sovrascrivere l'eseguibile).
3. Riavviare: `omi-mcp.exe service start`.

Per **cambiare le opzioni** del servizio (porta, bind, token, account) occorre reinstallarlo. Il modo più semplice è
rilanciare `install.ps1` con la nuova versione e le nuove opzioni: lo script disinstalla il servizio esistente e lo
reinstalla, conservando database, log, token e regola del firewall. A mano:

```powershell
omi-mcp.exe service uninstall --keep-firewall
omi-mcp.exe service install --db C:\ProgramData\OMIMyMCP\omi.db --port 8765 --log --now
```

### 7.8 Installazione come servizio Linux (systemd)

Gli script in `deploy/linux/` sono l'equivalente dei PowerShell di `deploy\windows`. Richiedono systemd e `sudo`.
Prerequisito: i binari (compilati come al §3.4, o dal pacchetto `.tar.gz`) e un database già caricato.

**Primo caricamento del database.** L'utente di sistema `omi` e la cartella dei dati vengono creati da `install.sh`; per avere il
database prima dell'installazione del servizio si può caricare in una cartella qualunque e poi copiarlo, oppure lanciare
`install.sh` una prima volta (si ferma con un avviso dopo aver creato utente, cartelle e programmi) e caricare così:

```bash
sudo ./deploy/linux/install.sh --source-dir target/release        # crea utente omi, cartelle e copia i programmi; poi avvisa che manca il database
sudo -u omi /opt/omimymcp/omi-import /var/lib/omimymcp/omi.db ~/DatiOMI/QI*.zip
sudo ./deploy/linux/install.sh --source-dir target/release --log --with-token --add-to-path
```

(`sudo -u omi` richiede che l'utente `omi` possa leggere gli ZIP; se sono nella home di un altro utente, copiarli in una cartella
leggibile, ad esempio `/tmp/dati-omi`.)

`install.sh`:

1. se il servizio esiste già, lo ferma (database, log e token restano);
2. crea l'utente di sistema `omi` (senza shell né login) e le cartelle `/opt/omimymcp` (programmi, 755) e `/var/lib/omimymcp` (dati, 750, di `omi`);
3. copia `omi-mcp`, `omi-import`, `LICENSE`, `NOTICE` e `MANUALE_TECNICO.md` in `/opt/omimymcp`;
4. con `--with-token` genera un token casuale in `/var/lib/omimymcp/token.txt` (se manca), di `omi` con permessi 600;
5. si ferma con un avviso se `omi.db` non esiste;
6. scrive `/etc/systemd/system/omi-mcp.service`, esegue `daemon-reload`, `enable` e `restart`;
7. verifica `/health` e, con `--add-to-path`, crea i collegamenti `/usr/local/bin/omi-mcp` e `omi-import`.

| Opzione di `install.sh` | Significato (predefinito) |
|---|---|
| `--source-dir` (obbligatoria) | Cartella con `omi-mcp` e `omi-import` |
| `--install-dir` | Cartella dei programmi (`/opt/omimymcp`) |
| `--data-dir` | Cartella dei dati: `omi.db`, `omi.log`, `token.txt` (`/var/lib/omimymcp`) |
| `--port` | Porta TCP (8765) |
| `--bind` | Indirizzo di ascolto (`127.0.0.1`, cioè solo locale) |
| `--service-name` | Nome dell'unità (`omi-mcp`) |
| `--user` | Utente di sistema del servizio (`omi`) |
| `--log` | Registra anche richieste HTTP e chiamate ai tool in `omi.log` |
| `--with-token` | Richiede il token Bearer su `/mcp` |
| `--trust-proxy` | Usa `X-Forwarded-For` nel log; solo dietro Caddy, con `--bind 127.0.0.1` |
| `--allowed-host HOST` | Accetta su `/mcp` solo questo `Host` (ripetibile) |
| `--add-to-path` | Collegamenti in `/usr/local/bin` |
| `--dry-run` | Mostra le azioni senza eseguirle |

Esempi:

```bash
# Solo sul PC locale (Claude Code, Claude Desktop)
sudo ./deploy/linux/install.sh --source-dir target/release

# Sulla rete locale, con token (aprire a mano la porta nel firewall, ad esempio: sudo ufw allow from 192.168.1.0/24 to any port 8765 proto tcp)
sudo ./deploy/linux/install.sh --source-dir target/release --bind 0.0.0.0 --with-token --log

# Dietro Caddy
sudo ./deploy/linux/install.sh --source-dir target/release --trust-proxy --log
```

L'unità generata (`ProtectSystem=strict` con la sola cartella dei dati scrivibile, `NoNewPrivileges`, `PrivateTmp`, `ProtectHome`,
riavvio dopo 5 secondi in caso di errore) è simile a `deploy/linux/omi-mcp.service`, che resta come modello per chi preferisce
installare a mano:

```bash
sudo useradd --system --home-dir /var/lib/omimymcp --no-create-home --shell /usr/sbin/nologin omi
sudo install -d -o omi -g omi -m 750 /var/lib/omimymcp
sudo install -D -m 755 target/release/omi-mcp target/release/omi-import -t /opt/omimymcp/
sudo cp deploy/linux/omi-mcp.service /etc/systemd/system/ && sudo systemctl daemon-reload
sudo systemctl enable --now omi-mcp
```

**Gestione.**

```bash
systemctl status omi-mcp
sudo systemctl stop omi-mcp          # SIGTERM: arresto ordinato
sudo systemctl start omi-mcp
sudo systemctl restart omi-mcp
journalctl -u omi-mcp -n 50 --no-pager   # avvio, arresto, errori
tail -f /var/lib/omimymcp/omi.log        # con --log anche le righe HTTP e TOOL
curl http://127.0.0.1:8765/health        # ok
```

**Account e permessi.** Come su Windows, il database è in modalità WAL: l'utente del servizio deve poter creare `omi.db-wal` e
`omi.db-shm` nella cartella del database, quindi la cartella dei dati appartiene a `omi` (e `ReadWritePaths` la lascia scrivibile
nonostante `ProtectSystem=strict`). Non tenere il database in una home protetta (`ProtectHome=true`) né su una condivisione di rete.

**Aggiornamento dei dati.**

```bash
sudo ./deploy/linux/aggiorna-dati.sh --zip-dir ~/DatiOMI
```

Ferma il servizio, esegue `omi-import` come utente `omi` sui `QI*.zip` della cartella, riavvia il servizio anche se l'import non
riesce e esce con il codice di `omi-import` (0 tutto caricato, 2 almeno un file con errori). Non stampa i nomi dei file. Opzioni:
`--data-dir`, `--install-dir`, `--service-name`, `--user`, `--dry-run`. (`omi` deve poter leggere la cartella degli ZIP.)

**Aggiornamento dei binari o delle opzioni.** Rilanciare `install.sh` con i nuovi binari e le nuove opzioni: ferma il servizio,
sostituisce i file, riscrive l'unità e riavvia, conservando database, log e token.

**Prova dell'installazione.** `sudo ./deploy/linux/test-servizio.sh --db /percorso/omi.db` installa un servizio di prova
(`omi-mcp-test`, porta 18765, utente `omitest`, con token), verifica `/health`, il 401 senza token, `tools/list` (9 strumenti),
arresto e riavvio, il log delle richieste senza il token e la disinstallazione, poi elimina tutto. Esce con 0 solo se ogni verifica è riuscita.

---

## 8. Esposizione HTTPS con Caddy

### 8.1 Quando serve

| Scenario | Serve Caddy? |
|---|---|
| Claude Code o Claude Desktop (tramite `mcp-remote`) sullo stesso PC | No: `http://localhost:8765/mcp` |
| Altri PC della LAN | No, oppure sì con `tls internal` se si vuole HTTPS (`Caddyfile.lan.example`) |
| **Connettore personalizzato di claude.ai / Cowork / app Claude** | **Sì**: il server dev'essere raggiungibile da Internet in HTTPS con un certificato valido (`Caddyfile.internet.example`) |
| Prove veloci da Internet | In alternativa a Caddy, `cloudflared tunnel --url http://localhost:8765` |

I connettori personalizzati sono contattati dall'infrastruttura di Anthropic, anche quando si usa l'app desktop o
Cowork: `localhost` e le reti private non funzionano.

### 8.2 Prerequisiti di rete

- Un **nome DNS pubblico** (record A/AAAA) che punti all'IP pubblico della macchina, oppure un servizio DNS dinamico.
- **Inoltro delle porte 80 e 443** sul router o NAT verso la macchina. Alcuni provider bloccano la porta 80 sulle
  linee domestiche: poiché il challenge HTTP-01 di Let's Encrypt la usa, in quel caso serve il challenge DNS (modulo DNS
  del provider, build personalizzata di Caddy) oppure il tunnel `cloudflared`.
- Firewall: TCP 80 e 443 in ingresso (su Windows le crea `install-caddy.ps1`; su Linux `install-caddy.sh` le apre in ufw o firewalld, se attivi); TCP 8765 **non** esposta.
- Conservare `C:\ProgramData\Caddy\data`: contiene certificati e chiavi; una nuova emissione ripetuta può urtare i limiti
  di Let's Encrypt.

### 8.3 Installazione

**Windows.**

1. Scaricare `caddy.exe` da <https://caddyserver.com/download>: lo script non scarica nulla. Confrontare l'hash SHA-256
   che lo script stampa con quello pubblicato.
2. Installare OMIMyMCP in modo che accetti solo il proxy, con l'IP originale nel log e **senza** regola di firewall propria:

   ```powershell
   .\deploy\windows\install.ps1 -SourceDir .\ -TrustProxy -Log
   ```

3. Installare Caddy come servizio (PowerShell come amministratore):

   ```powershell
   .\deploy\windows\install-caddy.ps1 -CaddyExe C:\Download\caddy.exe -Dominio omi.esempio.it -Email nome@esempio.it
   ```

| Parametro di `install-caddy.ps1` | Significato (predefinito) |
|---|---|
| `-CaddyExe` (obbligatorio) | Percorso di `caddy.exe` scaricato |
| `-Dominio` (obbligatorio) | Nome DNS pubblico |
| `-Email` | Contatto per Let's Encrypt (se omesso, la riga `email` viene tolta dal Caddyfile) |
| `-InstallDir` | Cartella di `caddy.exe` (`C:\Program Files\Caddy`) |
| `-ConfigDir` | Cartella di Caddyfile, certificati e log (`C:\ProgramData\Caddy`) |
| `-OmiServiceName` | Servizio da cui dipende Caddy (`OMIMyMCP`) |
| `-Port` | Porta di `omi-mcp` (8765) |
| `-Template` | Caddyfile alternativo da cui partire |

Lo script valida il `Caddyfile`, lo scrive in `C:\ProgramData\Caddy\Caddyfile` (cartella accessibile solo ad Administrators e
SYSTEM), crea il servizio `caddy` (avvio automatico, dipendente da `OMIMyMCP`, riavvio dopo i guasti), apre TCP 80 e 443
(regole `OMIMyMCP-Caddy-80` e `OMIMyMCP-Caddy-443`) e verifica `https://<dominio>/health`. Avvisa se `OMIMyMCP` non usa
`--trust-proxy` o non ascolta solo sul loopback. Supporta `-WhatIf`.

> La creazione reale del servizio `caddy` e l'emissione di un certificato Let's Encrypt con un dominio vero vanno provate
> sul proprio sistema: lo script supporta `-WhatIf` (Windows) e `--dry-run` (Linux) per vedere le azioni senza eseguirle.

### 8.4 Il Caddyfile

`deploy\caddy\Caddyfile.internet.example` (per un dominio pubblico):

```caddy
{
	# certificati e stato in una cartella fissa (il servizio gira come LocalSystem)
	storage file_system C:/ProgramData/Caddy/data
	email {$OMI_ACME_EMAIL}
}

{$OMI_DOMINIO} {
	encode zstd gzip

	# facoltativo: limitare l'accesso agli IP noti. Il traffico dei connettori di claude.ai parte da
	# 160.79.104.0/21 (verificato il 25/10/2026, vedi LEGGIMI.md): aggiungere anche i propri IP per le prove.
	# @esterni not remote_ip 160.79.104.0/21 203.0.113.7/32
	# respond @esterni 403

	request_body {
		max_size 1MB
	}

	handle /mcp* {
		reverse_proxy 127.0.0.1:8765 {
			flush_interval -1 # niente buffering: necessario per le risposte in streaming (SSE)
			transport http {
				read_timeout 5m
			}
		}
	}
	handle /health {
		reverse_proxy 127.0.0.1:8765
	}
	handle {
		respond "OMIMyMCP - endpoint /mcp" 200
	}

	log {
		output file C:/ProgramData/Caddy/logs/omi-access.log {
			roll_size 10MiB
			roll_keep 10
		}
		format json
	}
}
```

Nel file installato lo script scrive il dominio e l'email al posto delle variabili `{$OMI_DOMINIO}` e
`{$OMI_ACME_EMAIL}` (una variabile vuota in `email` non è valida per Caddy). Punti chiave: `storage` fissa i certificati in
una cartella nota; `request_body` limita le richieste a 1 MB; `flush_interval -1` disattiva il buffering; il blocco
`log` scrive il log di accesso con rotazione. I percorsi `C:/ProgramData/Caddy/...` sono quelli di Windows: `install-caddy.sh` li sostituisce con quelli di Linux. Per la rete locale c'è `Caddyfile.lan.example`, con `tls internal`
(certificato della CA locale di Caddy, che i client devono considerare attendibile: sulla macchina di Caddy `caddy trust`,
sugli altri PC si importa `pki\authorities\local\root.crt`). Un connettore di claude.ai **non** funziona con quel certificato.

### 8.5 Verifica

```powershell
Invoke-WebRequest https://omi.esempio.it/health          # 200 ok
Invoke-WebRequest https://omi.esempio.it/                # "OMIMyMCP - endpoint /mcp"
Get-Service caddy, OMIMyMCP
Get-Content C:\ProgramData\OMIMyMCP\omi.log -Tail 5     # con -Log: l'IP del client compare come "<ip> (via 127.0.0.1)"
```

Su Linux: `curl https://omi.esempio.it/health`, `systemctl status caddy omi-mcp`, `tail -5 /var/lib/omimymcp/omi.log`.

### 8.6 Rinnovo dei certificati

Caddy ottiene e rinnova da solo i certificati Let's Encrypt. Perché il rinnovo funzioni il servizio `caddy` deve restare
attivo, il nome DNS deve continuare a puntare alla macchina, le porte 80/443 devono restare raggiungibili e la cartella
`C:\ProgramData\Caddy\data` non va cancellata. In caso di problemi di emissione, `caddy.exe run --config
C:\ProgramData\Caddy\Caddyfile` (a servizio fermo) mostra i messaggi di Caddy in console (su Linux: `journalctl -u caddy`).

### 8.7 Alternativa: tunnel cloudflared

Per prove veloci, senza dominio né inoltro delle porte (con `omi-mcp` in esecuzione su `localhost:8765`):

```powershell
cloudflared tunnel --url http://localhost:8765
```

Il comando stampa un indirizzo `https://….trycloudflare.com` da usare come URL del connettore (`https://….trycloudflare.com/mcp`).
L'indirizzo cambia a ogni avvio e il tunnel dura finché `cloudflared` è in esecuzione: non è adatto a un uso stabile. Con il tunnel
`omi-mcp` si lascia in ascolto sul loopback (`--bind 127.0.0.1`, già predefinito nel servizio) e senza regola di firewall.

### 8.8 Protezioni compatibili con claude.ai

| Protezione | Compatibile con claude.ai? | Note |
|---|---|---|
| **Restrizione per IP in Caddy** | **Sì** | Il traffico dei connettori parte da `160.79.104.0/21` (elenco ufficiale: <https://platform.claude.com/docs/en/api/ip-addresses>). Decommentare la regola `@esterni` nel Caddyfile, aggiungendo i propri IP per le prove |
| **Token Bearer** (`--token`/`--token-file`) | **In parte** | Il connettore accetta una credenziale fissa nelle intestazioni di richiesta ("Request headers"), ma la funzione è in beta per un insieme limitato di organizzazioni (verifica del 25/10/2026). Funziona sempre con Claude Code (`--header`) e con `mcp-remote` |
| **OAuth 2.0** | Sì in teoria | Non è implementato da `omi-mcp` |
| **Autenticazione sul proxy** (Basic, `forward_auth`) | **No** | Il connettore non sa presentarla |
| **Nessuna** | Sì | Chiunque conosca l'URL può interrogare il server: sconsigliato senza restrizione per IP |

Scelta consigliata per claude.ai: **restrizione per IP** sempre, più il **token Bearer** se l'organizzazione ha le intestazioni
di richiesta. Con entrambi, anche un indirizzo dell'intervallo di Anthropic che non presenta il token riceve 401.


### 8.9 Caddy su Linux

Su Linux Caddy si installa con il gestore di pacchetti (vedi <https://caddyserver.com/docs/install>; per Debian/Ubuntu c'è il
repository ufficiale, che crea l'utente `caddy` e l'unità systemd) oppure scaricando il binario da <https://caddyserver.com/download>.

```bash
# 1. OMIMyMCP accetta solo il proxy, con l'IP originale nel log e senza aprire la 8765
sudo ./deploy/linux/install.sh --source-dir target/release --trust-proxy --log

# 2a. Caddy dal pacchetto della distribuzione
sudo ./deploy/linux/install-caddy.sh --domain omi.esempio.it --email nome@esempio.it

# 2b. oppure Caddy scaricato a mano (lo script lo copia in /usr/local/bin e crea utente e unità)
sudo ./deploy/linux/install-caddy.sh --caddy-bin ~/Download/caddy --domain omi.esempio.it --email nome@esempio.it
```

| Opzione di `install-caddy.sh` | Significato (predefinito) |
|---|---|
| `--domain` (obbligatoria) | Nome DNS pubblico |
| `--email` | Contatto per Let's Encrypt (se omessa, la riga `email` viene tolta dal Caddyfile) |
| `--caddy-bin` | `caddy` scaricato; se omessa si usa quello installato dal gestore di pacchetti |
| `--config-dir` | Cartella del Caddyfile (`/etc/caddy`) |
| `--omi-service` | Unità da cui dipende Caddy (`omi-mcp`) |
| `--port` | Porta di `omi-mcp` (8765) |
| `--template` | Caddyfile alternativo da cui partire |
| `--no-firewall` | Non apre le porte 80 e 443 |
| `--dry-run` | Mostra le azioni senza eseguirle |

Lo script genera `/etc/caddy/Caddyfile` da `Caddyfile.internet.example` (certificati in `/var/lib/caddy/data`, log di accesso in
`/var/log/caddy/omi-access.log`; un Caddyfile precedente è salvato come `Caddyfile.bak`), esegue `caddy validate`, apre TCP 80 e 443 in
ufw o firewalld se attivi, abilita e riavvia `caddy` e verifica `https://<dominio>/health`. Avvisa se `omi-mcp` non usa `--trust-proxy` o
non ascolta solo sul loopback. Verifica:

```bash
curl https://omi.esempio.it/health        # ok
systemctl status caddy omi-mcp
journalctl -u caddy -n 50 --no-pager
```

Per la rete locale, copiare `Caddyfile.lan.example` in `/etc/caddy/Caddyfile`, sostituire i percorsi `C:/ProgramData/Caddy/...` con
`/var/lib/caddy/data` e `/var/log/caddy/omi-access.log`, eseguire `sudo caddy trust` sulla macchina del proxy e importare
`pki/authorities/local/root.crt` sugli altri client.

---

## 9. Uso del server MCP da Claude

L'endpoint del server è `http://<host>:<porta>/mcp` (HTTP *streamable*, senza sessioni). Questo capitolo spiega come
collegarlo a ciascun client Claude, come verificarlo e come farsi dare risposte affidabili.

> **Verifica sulla documentazione Anthropic: 25/10/2026.** Nomi dei menu, comandi e opzioni di autenticazione sono
> stati controllati su <https://support.claude.com/en/articles/11175166-get-started-with-custom-connectors-using-remote-mcp>,
> <https://code.claude.com/docs/en/mcp>, <https://code.claude.com/docs/en/permissions> e
> <https://modelcontextprotocol.io/docs/develop/connect-local-servers>. Le interfacce cambiano spesso: se un menu
> non corrisponde, cercare la voce "connettori personalizzati" (*custom connectors*) nella guida in linea di Claude.
> Il percorso del menu è oggi **Customize > Connectors**, non "Impostazioni → Connettori" come in versioni precedenti dell'interfaccia.

### 9.0 Quale client, quale collegamento

| Client | Da dove parte la connessione | Collegamento |
|---|---|---|
| **claude.ai** (web, app desktop, app mobili) | Dal cloud di Anthropic | Connettore personalizzato; il server dev'essere raggiungibile da Internet in HTTPS (§8) |
| **Cowork** | Dal cloud di Anthropic | Gli stessi connettori dell'account |
| **Claude Desktop con server locale** | Dal PC, tramite `mcp-remote` | `claude_desktop_config.json` (Windows e macOS); funziona con `http://localhost:8765/mcp` |
| **Claude Code** | Dal PC | `claude mcp add --transport http`; funziona con `http://localhost:8765/mcp` |

Anche se Claude Desktop e Cowork girano sul vostro PC, i **connettori** (quelli configurati nell'account) sono
contattati dai server di Anthropic, non dal PC: per quelli `localhost` non funziona. I server locali configurati in
`claude_desktop_config.json` sono un meccanismo separato, che usa la rete del PC ma non è disponibile in claude.ai né in Cowork.

### 9.1 claude.ai (web, app desktop e mobili) e Cowork

**Prerequisiti**

1. Il server è in esecuzione (servizio `OMIMyMCP` attivo, `Get-Service OMIMyMCP`).
2. L'URL pubblico risponde in HTTPS con un certificato valido: `https://omi.esempio.it/health` restituisce `ok` (§8). Per
   una prova veloce va bene un tunnel `cloudflared` (§8.7).
3. Se il server richiede il **token** Bearer: serve la sezione "Request headers" nella finestra del connettore (funzione
   in beta per un insieme limitato di organizzazioni, verifica del 25/10/2026). Se manca, non usare il token e proteggere
   il server con la restrizione per IP di Caddy (§8.8).

**Aggiungere il connettore (piani Pro e Max)**

1. Aprire **Customize > Connectors** (<https://claude.ai/customize/connectors>).
2. Premere **+ Add**, poi **Add custom connector**.
3. Inserire un nome (per esempio `Quotazioni OMI`) e l'URL completo del server, per esempio `https://omi.esempio.it/mcp`, poi **Continue**.
4. Controllare le impostazioni di autenticazione rilevate. Per un server **senza token** scegliere **Authentication → No sign in**.
   `omi-mcp` non implementa OAuth, quindi "Sign in now" e "Sign in when needed" non sono utilizzabili.
5. Con il token, se compare **Request headers**, aggiungere l'intestazione `Authorization` con valore `Bearer <token>`
   (il contenuto di `C:\ProgramData\OMIMyMCP\token.txt`).
6. Premere **Add**.

**Piani Team ed Enterprise.** Il connettore lo aggiunge un Owner (o un Primary Owner; per Enterprise anche un ruolo
personalizzato con *Manage access to Libraries*) da **Organization settings > Connectors** (<https://claude.ai/admin-settings/connectors>):
**Add**, passare su **Custom**, scegliere **Web**, poi gli stessi passi 3–6. Poi ogni membro apre **Customize > Connectors**,
trova il connettore e lo connette. Con una credenziale fissa nelle intestazioni tutti i membri usano la stessa credenziale.

**Attivarlo in una conversazione.** Nella conversazione, premere il pulsante **+** in basso a sinistra, poi
**Connectors**, e attivare l'interruttore del connettore. Dal menu **Search and tools** si possono disattivare i singoli strumenti.

**Verifica.** Scrivere: *"Quali semestri OMI sono caricati?"* Claude deve chiamare `elenco_semestri` e riportare semestri,
numero di zone e fonte dei dati. Seconda prova: *"Cerca il comune di Asti nei dati OMI"* (chiama `cerca_comuni`). Se Claude
risponde senza chiamare gli strumenti: verificare che il connettore sia attivo nella conversazione, oppure scrivere
*"usa il server OMI"*.

**Permessi di chiamata.** Alla prima chiamata di uno strumento Claude chiede l'approvazione. Gli strumenti di OMIMyMCP
sono tutti di **sola lettura** (`readOnlyHint`), quindi si può scegliere **Allow always** per il connettore. Attenzione
alla funzione Research: durante una ricerca approfondita Claude può chiamare gli strumenti dei connettori senza ulteriore approvazione.

**Rimuoverlo.** **Customize > Connectors**, premere **Remove** (o i tre puntini accanto al connettore) e confermare. Per
modificare un connettore bisogna rimuoverlo e aggiungerlo di nuovo. Gli Owner di Team ed Enterprise possono farlo
da **Organization settings > Connectors**.

**Cowork, app desktop e app mobili.** Usano i connettori dell'account: dopo averlo aggiunto da claude.ai (o dalle impostazioni
dell'app) compare anche lì. Va attivato nella conversazione come sopra.

**Problemi tipici**

| Sintomo | Causa probabile | Soluzione |
|---|---|---|
| Il connettore non si collega / non si aggiunge | Server non raggiungibile da Internet, certificato non valido (per esempio quello `tls internal` di Caddy), DNS errato, porta 80 bloccata (Let's Encrypt), server su rete privata o VPN | Provare `https://<dominio>/health` da un altro dispositivo fuori dalla rete; vedere §8 e §14 |
| Il connettore si collega ma le risposte arrivano a pezzi o si bloccano | Un proxy mette in buffer le risposte | Nel Caddyfile `flush_interval -1`, come nell'esempio |
| Errore 401 / autenticazione | Il server ha il token ma il connettore non lo invia, oppure il token è sbagliato | Aggiungere l'intestazione `Authorization: Bearer <token>` se disponibile; altrimenti avviare il server senza token e usare la restrizione per IP |
| Errore 403 | La restrizione per IP di Caddy blocca l'indirizzo di Anthropic | Verificare che `160.79.104.0/21` sia ammesso (§8.8) |
| Il connettore non compare nell'elenco della conversazione | Non è attivo, oppure (Team/Enterprise) l'Owner non l'ha aggiunto o il membro non l'ha connesso | **+ → Connectors** e attivarlo; vedere sopra |
| Claude non usa gli strumenti | Il connettore non è attivo in quella conversazione | Attivarlo, o scrivere "usa il server OMI" |

### 9.2 Claude Desktop con server locale (`mcp-remote`)

Serve per usare un server **sul proprio PC** (`localhost`) senza HTTPS. Richiede **Node.js** (`node --version`, `npx --version`).
`mcp-remote` è un ponte che trasforma il server HTTP in un server locale `stdio` per Claude Desktop.

**Prerequisiti:** server in esecuzione su `http://localhost:8765/mcp` (servizio o console), Node.js installato.

**Aggiungere il server.** In Claude Desktop: **Settings > Developer > Edit Config** (impostazioni → sviluppatore → modifica
configurazione). Si apre il file `%APPDATA%\Claude\claude_desktop_config.json`. Inserire:

```json
{
  "mcpServers": {
    "omi": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://localhost:8765/mcp"]
    }
  }
}
```

Con il **token**:

```json
{
  "mcpServers": {
    "omi": {
      "command": "npx",
      "args": [
        "-y", "mcp-remote", "http://localhost:8765/mcp",
        "--header", "Authorization: Bearer INCOLLARE_QUI_IL_TOKEN"
      ]
    }
  }
}
```

Se il file contiene già altri server, aggiungere solo la voce `"omi"` dentro `mcpServers`. Salvare e **riavviare Claude
Desktop** (chiudere anche dall'icona nell'area di notifica). Provato con `mcp-remote` 0.15.2: il ponte si collega a
`omi-mcp`, con e senza token, e usa il trasporto *Streamable HTTP*; per un indirizzo `http://` diverso da `localhost` (per esempio
un altro PC della LAN) `mcp-remote` richiede l'opzione `--allow-http` negli argomenti.

**Attivarlo e verificare.** Nella conversazione il server compare tra gli strumenti disponibili. Domanda di prova:
*"Quali semestri OMI sono caricati?"*.

**Permessi.** Claude Desktop chiede l'approvazione alla prima chiamata di ogni strumento; gli strumenti sono di sola lettura e
si può consentirli in modo permanente.

**Rimuoverlo.** Cancellare la voce `"omi"` dal file di configurazione, salvare e riavviare Claude Desktop.

**Problemi tipici**

| Sintomo | Soluzione |
|---|---|
| Il server non è elencato | Controllare la sintassi JSON (virgole, parentesi), salvare e riavviare davvero Claude Desktop |
| Errore all'avvio del server, `npx` non trovato | Installare Node.js; riaprire Claude Desktop |
| Errore di connessione | Verificare che `http://localhost:8765/health` risponda; il servizio sia attivo e la porta sia quella giusta |
| 401 | Aggiungere la coppia `"--header", "Authorization: Bearer …"` con il token del server |

### 9.3 Claude Code

**Prerequisiti:** server in esecuzione su `http://localhost:8765/mcp` (o su un URL HTTPS); `claude --version`.

**Aggiungere il server**

```powershell
claude mcp add --transport http omi http://localhost:8765/mcp
```

Con il token:

```powershell
claude mcp add --transport http omi http://localhost:8765/mcp --header "Authorization: Bearer <token>"
```

Ambito (opzione `--scope`): `local` (predefinito: solo il progetto corrente, privato), `project` (salvato in `.mcp.json` nella radice
del progetto e condiviso con chi usa lo stesso repository: **non metterci il token**), `user` (disponibile in tutti i progetti):

```powershell
claude mcp add --transport http omi http://localhost:8765/mcp --scope user
```

`claude mcp add` salva la configurazione senza verificare le credenziali: per controllare la connessione si usano i comandi seguenti.

**Attivarlo e verificare**

```powershell
claude mcp list          # elenco dei server e stato della connessione; "omi" deve risultare connesso
claude mcp get omi       # dettagli di un server
```

In una sessione di Claude Code, `/mcp` mostra lo stato di ogni server (un segno di spunta se connesso, ✘ se non lo è, con
il dettaglio dell'errore, per esempio un 401) e l'elenco degli strumenti. Poi la domanda di prova: *"Quali semestri OMI sono caricati?"*.

**Permessi di chiamata.** Alla prima chiamata di ogni strumento Claude Code chiede il permesso. Poiché gli strumenti sono di
sola lettura si possono consentire in modo permanente con una regola nei permessi (file `.claude/settings.json` del progetto o
`~/.claude/settings.json`):

```json
{
  "permissions": {
    "allow": ["mcp__omi__*"]
  }
}
```

`mcp__omi__*` consente tutti gli strumenti del server `omi`; per uno solo si scrive il nome completo, per esempio
`mcp__omi__elenco_semestri`. Il nome nella regola è quello scelto in `claude mcp add` (qui `omi`).

**Rimuoverlo**

```powershell
claude mcp remove omi
```

**Problemi tipici**

| Sintomo | Soluzione |
|---|---|
| `claude mcp list` mostra il server non connesso | Verificare `http://localhost:8765/health`; se il server ha il token, aggiungere `--header "Authorization: Bearer <token>"` (rimuovere e aggiungere di nuovo) |
| `/mcp` mostra ✘ con 401 | Token mancante o sbagliato |
| Il server non compare in un altro progetto | Era stato aggiunto con ambito `local`: riaggiungerlo con `--scope user` |
| Server `project` non caricato | Claude Code chiede l'approvazione dei server di `.mcp.json` la prima volta; `claude mcp reset-project-choices` riazzera le scelte |
| Il comando `claude` non viene riconosciuto | Riaprire il terminale; `claude doctor` |

### 9.4 Come formulare le domande

- Indicare il **comune con la provincia** quando il nome è ambiguo ("Samone (TO)"), oppure il **codice catastale** (`H501`).
  Un nome ambiguo restituisce l'errore con l'elenco dei candidati.
- Indicare il **semestre** (`2025/2`) quando conta. Se manca si usa l'ultimo caricato. Il formato accettato è `AAAA/S`
  (anche `2025-2`, `20252`).
- Per un indirizzo fornire le **coordinate** (latitudine e longitudine WGS84): il server OMI non converte gli indirizzi in
  coordinate. Si possono ricavare con MCP Cruscotto Italia (§9.6).
- Per le **serie storiche** chiedere di segnalare i cambi di descrizione della zona: le revisioni della zonizzazione possono aver
  ridefinito i codici zona e provocato salti di quotazione.
- Chiedere sempre che i documenti prodotti **citino la fonte "Agenzia delle Entrate - OMI"**.
- Ricordare che i valori sono indicativi: sono forbici di quotazione per zona e stato conservativo, non il prezzo di un immobile.
- Se la risposta è vuota o sorprendente, chiedere di verificare con `elenco_semestri` (semestri disponibili) e `cerca_comuni`
  (comune e codice).

### 9.5 Percorso guidato: sei domande d'esempio

| # | Domanda | Strumento che Claude userà | Che cosa aspettarsi |
|---|---|---|---|
| 1 | *"Quali semestri OMI sono caricati?"* | `elenco_semestri` | L'elenco dei semestri con zone, quotazioni e perimetri, la data di elaborazione, la fonte |
| 2 | *"Elenca le zone OMI di Bergamo con le quotazioni delle abitazioni civili in stato normale, dalla più cara alla più economica."* | `cerca_comuni`/`zone_comune` (con `includi_valori`) | Una tabella con zona, fascia, descrizione e forbice €/m² per compravendita e locazione, ordinata da Claude per il massimo; sono le quotazioni dell'ultimo semestre |
| 3 | *"Ho un appartamento di 95 m² in buono stato in via Nizza 150 a Torino. In che zona OMI si trova e qual è l'intervallo di valore di compravendita e di affitto mensile secondo l'ultimo semestre?"* | Cruscotto Italia (coordinate del civico), poi `zona_da_coordinate` | La zona che contiene il punto, le forbici €/m² e un calcolo (95 m² × minimo/massimo). Claude dovrebbe ricordare che "buono stato" corrisponde allo stato NORMALE/OTTIMO e che la superficie OMI può essere lorda o netta |
| 4 | *"Quali zone OMI si trovano entro 1,5 km da Piazza del Campo a Siena?"* | `zone_vicine` (con `raggio_m` = 1500) | Le zone ordinate per distanza, con `distanza_m` e le quotazioni delle abitazioni civili. Il raggio massimo è 20 000 m |
| 5 | *"Per le zone centrali (fascia B) di Bologna calcola il rendimento lordo annuo da affitto delle abitazioni civili."* | `zone_comune` con `includi_valori` (o `statistiche_comune`) | Per ogni zona B: affitto annuo = locazione €/m² mese × 12, diviso il prezzo €/m²; Claude dovrebbe usare i punti medi delle forbici e dichiararlo |
| 6 | *"Mostrami l'andamento dal 2004 a oggi delle quotazioni delle abitazioni civili nella zona B12 di Milano, con un grafico o un foglio di calcolo."* | `storico_zona` (con `cod_tipologia` = 20) | Una serie per tipologia e stato conservativo, semestre per semestre, e le eventuali **descrizioni di zona cambiate**; poi un grafico o un file di calcolo prodotto da Claude, se il client lo permette |

Dalla 3 in avanti la risposta dipende dai semestri caricati: la serie del punto 6 parte dal 2004 solo se sono stati
caricati i `QI` dal 2004/1.

### 9.6 Uso combinato con MCP Cruscotto Italia

Il server MCP pubblico di AgID **Cruscotto Italia** (<https://www.dati.gov.it/sviluppatori/mcp-cruscotto-italia>, endpoint
`https://cruscotto-italia-mcp.agid.workers.dev/mcp`) espone dati pubblici per ogni comune italiano (ISTAT, MEF, ISPRA, PNRR e
altre fonti). Insieme a OMIMyMCP permette di ricavare le coordinate di un indirizzo dallo stradario ANNCSU e passarle a
`zona_da_coordinate`, oppure di confrontare prezzi e redditi IRPEF. In Claude Code:

```powershell
claude mcp add --transport http cruscotto-italia https://cruscotto-italia-mcp.agid.workers.dev/mcp
```

In claude.ai si aggiunge come connettore personalizzato con lo stesso URL (§9.1, "No sign in"). Poi si possono fare domande
come: *"Trova le coordinate di via Nizza 150 a Torino con Cruscotto Italia e usale per cercare la zona OMI."* I dati di
Cruscotto Italia hanno licenze proprie (prevalentemente CC-BY 4.0), da citare insieme alla fonte OMI.

### 9.7 Prova degli strumenti senza Claude: MCP Inspector

L'Inspector è lo strumento di prova ufficiale di MCP (richiede Node.js).

Interfaccia grafica:

```powershell
npx @modelcontextprotocol/inspector
```

Nel browser scegliere il trasporto **Streamable HTTP**, inserire l'URL `http://localhost:8765/mcp` (con l'intestazione `Authorization:
Bearer <token>` se il token è attivo), connettersi e provare gli strumenti dalla scheda *Tools*.

Riga di comando (provata con il server di prova, con token):

```powershell
npx @modelcontextprotocol/inspector --cli http://localhost:8765/mcp --method tools/list --header "Authorization: Bearer <token>"
npx @modelcontextprotocol/inspector --cli http://localhost:8765/mcp --method tools/call --tool-name cerca_comuni --tool-arg testo=Roma --header "Authorization: Bearer <token>"
```

Il secondo comando restituisce:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\"trovati\":1,\"comuni\":[{\"codice_catastale\":\"H501\",\"codice_istat\":\"1058091\",\"nome\":\"ROMA\",\"provincia\":\"RM\",\"regione\":\"LAZIO\",\"area_territoriale\":\"CENTRO\",\"primo_semestre\":\"2024/2\",\"ultimo_semestre\":\"2025/1\"}]}"
    }
  ],
  "isError": false
}
```

Senza il token l'Inspector (non interattivo) termina con `auth_required` e codice 3: il server ha risposto 401.

---

## 10. Strumenti MCP

Il server espone **9 strumenti**, tutti con le annotazioni `readOnlyHint: true` e `idempotentHint: true`. Ogni risultato è **un solo
blocco di testo** con un JSON compatto (nell'esempio sopra il JSON è dentro il campo `text`). Gli errori di validazione sono risultati
con `isError: true` e il messaggio come testo.

### 10.1 Elenco e parametri

`*` = obbligatorio. Numeri e booleani passati come stringa (`"20"`, `"true"`) sono accettati.

| Strumento | Descrizione | Parametri |
|---|---|---|
| `elenco_semestri` | Elenca i semestri OMI caricati nel database con numero di zone, quotazioni e perimetri disponibili. | nessuno |
| `elenco_tipologie` | Elenca le tipologie edilizie OMI (codice e descrizione), es. 20 = Abitazioni civili, 13 = Box. | nessuno |
| `cerca_comuni` | Cerca comuni per nome (anche parziale, senza accenti), codice catastale (es. H501) o codice ISTAT. Restituisce codici, provincia, regione e semestri disponibili. | `testo`*, `provincia` |
| `zone_comune` | Elenca le zone OMI di un comune in un semestre: codice zona, LinkZona, fascia, descrizione, microzona, tipologia e stato prevalenti. Facoltativamente include quotazioni e dati del perimetro (centro, bbox, area). | `comune`*, `provincia`, `semestre`, `includi_valori` (false), `includi_perimetri` (false) |
| `dettaglio_zona` | Tutti i dati di una zona OMI in un semestre: anagrafica zona e comune, quotazioni di compravendita (€/m²) e locazione (€/m² mese) per tipologia e stato conservativo, dati del perimetro e, se richiesto, la geometria GeoJSON. | `comune`*, `zona`* (codice o LinkZona), `provincia`, `semestre`, `includi_geometria` (false) |
| `zona_da_coordinate` | Individua la zona OMI che contiene un punto (latitudine/longitudine WGS84) e restituisce comune, zona, quotazioni e perimetro. Se il punto non ricade in alcuna zona restituisce la più vicina entro 2 km. | `latitudine`*, `longitudine`*, `semestre`, `includi_geometria` (false) |
| `zone_vicine` | Elenca le zone OMI entro un raggio (metri) da un punto, ordinate per distanza, con le quotazioni delle abitazioni civili. | `latitudine`*, `longitudine`*, `raggio_m` (1000, massimo 20 000), `semestre` |
| `storico_zona` | Serie storica delle quotazioni di una zona OMI su tutti i semestri caricati, per tipologia e stato conservativo. | `comune`*, `zona`*, `provincia`, `cod_tipologia`, `stato` (OTTIMO, NORMALE o SCADENTE) |
| `statistiche_comune` | Riepilogo delle quotazioni di un comune in un semestre: minimo, massimo e medio per tipologia e medie delle abitazioni civili per fascia (centrale, semicentrale, periferica, ...). | `comune`*, `provincia`, `semestre` |

`comune` è il nome del comune, il codice catastale (Belfiore, per esempio `H501` = Roma) o il codice ISTAT; `provincia` è la sigla
(per esempio `RM`), utile quando il nome è ambiguo; `semestre` è `AAAA/S`, e se omesso si usa l'ultimo disponibile.

### 10.2 Formato del semestre

I semestri si scrivono `AAAA/S` (per esempio `2025/2`); sono accettati anche `2025-2`, `20252`, `2025 2` e `2025S2`: si tengono solo le cifre,
che devono essere cinque e l'ultima `1` o `2`. Un valore diverso dà l'errore
`Semestre non valido: '2025/3'. Usare il formato AAAA/S, es. 2025/1.`; un semestre non caricato dà
`Semestre 2020/1 non presente nel database. Usare elenco_semestri.`

### 10.3 Semestri senza perimetri

I perimetri (KML) esistono solo dal 2° semestre 2010 e solo per i semestri caricati con una fornitura `QIP`. `zona_da_coordinate` e
`zone_vicine` usano i perimetri del **semestre con perimetri più vicino** a quello richiesto, collegando le zone per codice, e lo dicono nel
risultato (`semestre_perimetri` e una `nota`). Se non esiste alcun semestre con perimetri: `Nel database non sono presenti perimetri di zona (file KML).`
Se il punto non ricade in nessuna zona, lo strumento restituisce la più vicina entro 2 km (`approssimato: true`); oltre, `"trovato": false`.

### 10.4 Esempi di chiamata e risposta

Gli esempi sono quelli dei test di riferimento ("golden", `crates\omi-core\tests\golden\`), calcolati su un database **sintetico**: i
numeri non sono quotazioni reali. Le risposte sono abbreviate con `…`.

**`elenco_semestri`**: nessun argomento →

```json
{"semestri":[{"semestre":"2025/1","n_zone":6,"n_valori":9,"elaborazione":"12/03/2026","file_csv":"QI1438200.zip"},{"semestre":"2024/2","n_zone":6,"n_valori":9,"n_perimetri":3,"elaborazione":"12/03/2026","file_csv":"QIP1438117.zip","file_kml":"QIP1438117.zip"}],"nota":"I perimetri (KML) sono disponibili solo per alcuni semestri; per gli altri la ricerca per coordinate usa i perimetri del semestre più vicino.","fonte_dati":"Agenzia Entrate - OMI","software":"OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)"}
```

**`elenco_tipologie`**: nessun argomento →

```json
[{"codice":13,"descrizione":"Box"},{"codice":20,"descrizione":"Abitazioni civili"}]
```

**`cerca_comuni`**: `{"testo": "Roma"}` →

```json
{"trovati":1,"comuni":[{"codice_catastale":"H501","codice_istat":"1058091","nome":"ROMA","provincia":"RM","regione":"LAZIO","area_territoriale":"CENTRO","primo_semestre":"2024/2","ultimo_semestre":"2025/1"}]}
```

**`zone_comune`**: `{"comune": "roma"}` →

```json
{"comune":{"codice_catastale":"H501","nome":"ROMA",…},"semestre":"2025/1","n_zone":2,"zone":[{"info":{"zona":"B1","link_zona":"RM00000001","fascia":"B","fascia_descr":"Centrale","descrizione":"ZONA B1 DI ROMA","microzona":1,"tipologia_prevalente":{"codice":20,"descrizione":"Abitazioni civili"},"stato_prevalente":"NORMALE","comune_catastale":"H501"}},{"info":{"zona":"D5",…}}]}
```

**`dettaglio_zona`**: `{"comune": "roma", "zona": "b1"}` →

```json
{"comune":{…},"semestre":"2025/1","zone":[{"info":{"zona":"B1","link_zona":"RM00000001",…},"valori":[…,{"cod_tipologia":20,"tipologia":"Abitazioni civili","stato":"NORMALE","stato_prevalente":true,"compravendita_eur_mq":{"min":3600,"max":5300,"superficie":"lorda"},"locazione_eur_mq_mese":{"min":12.8,"max":18.6,"superficie":"lorda"}},…],"perimetro":{"semestre_perimetro":"2024/2","centro":{"lat":…,"lon":…},"bbox":{…},"area_km2":…}}]}
```

Con un nome ambiguo (`{"comune": "samone", "zona": "R1"}`) il risultato è un errore con l'elenco dei candidati: `'samone' è ambiguo, specificare la
provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754]`.

**`zona_da_coordinate`**: `{"latitudine": 41.905, "longitudine": 12.495, "semestre": "2024/2"}` →

```json
{"trovato":true,"lat":41.905,"lon":12.495,"semestre":"2024/2","semestre_perimetri":"2024/2","risultati":[{"comune":{"codice_catastale":"H501",…},"zona":"B1","dati":[{"info":{…},"valori":[…],"perimetro":{…}}]}]}
```

**`zone_vicine`**: `{"latitudine": 41.905, "longitudine": 12.495, "raggio_m": 10000}` →

```json
{"lat":41.905,"lon":12.495,"raggio_m":10000,"semestre":"2025/1","semestre_perimetri":"2024/2","n":2,"zone":[{"comune":"ROMA (RM)","codice_catastale":"H501","zona":"B1","link_zona":"RM00000001","fascia":"B","descrizione":"ZONA B1 DI ROMA","distanza_m":0,"abitazioni_civili":{"stato":"NORMALE","compr_min":3600,"compr_max":5300,"loc_min":12.8,"loc_max":18.6}},{"comune":"ROMA (RM)","codice_catastale":"H501","zona":"C2","link_zona":"RM00000002","distanza_m":7132}]}
```

**`storico_zona`**: `{"comune": "roma", "zona": "b1", "cod_tipologia": "13"}` →

```json
{"comune":{…},"zona":"B1","descrizioni_zona":[{"descrizione":"ZONA B1 DI ROMA","dal":"2024/2","al":"2025/1"}],"n_serie":2,"serie":[{"cod_tipologia":13,"tipologia":"Box","stato":"NORMALE","valori":[{"semestre":"2024/2","compr_min":1800,"compr_max":2500,"loc_min":8,"loc_max":11,"prevalente":true},{"semestre":"2025/1","compr_min":1850,"compr_max":2550,"loc_min":8.2,"loc_max":11.2,"prevalente":true}]},{…"stato":"SCADENTE",…}],"nota":"I codici zona OMI possono essere stati ridefiniti nelle revisioni della zonizzazione: confrontare le descrizioni."}
```

**`statistiche_comune`**: `{"comune": "roma", "semestre": "2024-2"}` →

```json
{"comune":{…},"semestre":"2024/2","nota":"Valori €/m² (locazione €/m² mese) calcolati sullo stato conservativo prevalente di ciascuna zona.","per_tipologia":[{"cod_tipologia":13,"tipologia":"Box","n_zone":1,"compravendita":{"min":1800,"max":2500,"medio":2150},"locazione":{"min":8,"max":11,"medio":9.5}},{"cod_tipologia":20,…}],"per_fascia":[{"fascia":"B","fascia_descr":"Centrale","n_zone":1,"abitazioni_civili_compr_medio":4350.3,"abitazioni_civili_loc_medio":15.4},{"fascia":"C",…}]}
```

### 10.5 Come leggere i risultati

- **Fasce:** B centrale, C semicentrale, D periferica, E suburbana, R extraurbana/rurale.
- **Stato conservativo:** OTTIMO, NORMALE, SCADENTE. `stato_prevalente: true` indica quello prevalente nella zona; le medie di
  `statistiche_comune` usano solo lo stato prevalente.
- **Superficie:** `lorda` o `netta`, per compravendita e locazione separatamente.
- **Unità:** compravendita in €/m²; locazione in €/m² **al mese**.
- Le proprietà senza valore sono omesse (non compaiono `null`), tranne la `descrizione` delle tipologie.
- I numeri reali interi sono scritti senza `.0` (`1500`); `distanza_m` è arrotondata all'intero.
- Il comune e il semestre sono sempre ripetuti nel risultato, così Claude può citarli.
- Ogni risposta basata su questi dati va accompagnata dalla fonte **"Agenzia delle Entrate - OMI"** (le `instructions` del server
  lo chiedono a Claude).


---

## 11. Struttura del database

`omi.db` è un database SQLite (modalità WAL, con il modulo R\*Tree). Lo schema (§11.4) può essere letto e
scritto da qualunque strumento che lo rispetti. `omi-import` esegue il DDL a ogni apertura in scrittura; `omi-mcp` apre il file
in sola lettura (`query_only=1`) e non lo crea mai.

### 11.1 Tabelle

| Tabella | Contenuto |
|---|---|
| `semestre` | Semestri caricati, con data di elaborazione dell'Agenzia, file sorgente (nome ripulito) e conteggi (`n_zone`, `n_valori`, `n_geometrie`) |
| `zona` | Una riga per zona OMI e semestre, dal file `*_ZONE.csv`; chiave `(semestre, link_zona)` |
| `valore` | Quotazioni dal file `*_VALORI.csv`; chiave `(semestre, link_zona, cod_tip, stato)` |
| `tipologia` | Codice e descrizione delle tipologie edilizie (es. 20 = Abitazioni civili, 13 = Box) |
| `geometria` | Poligoni delle zone, deduplicati tra semestri tramite `hash`, con bbox, centro, area e numero di punti |
| `geometria_rt` | Indice spaziale R\*Tree sui bbox delle geometrie |
| `zona_geo` | Collegamento semestre + comune + zona → geometria (dai KML) |
| `comune` | Anagrafica dei comuni (dati dell'ultimo semestre) con primo e ultimo semestre disponibile |
| `comune_nome` | Nomi dei comuni, anche storici, normalizzati per la ricerca |
| `file_caricato` | Registro dei file importati, **senza** il codice fiscale nel nome |

Convenzioni: il **semestre** è un intero `AAAAS` (`20252` = 2° semestre 2025; etichetta `2025/2`); il **codice comune** è il codice
catastale (Belfiore, `H501` = Roma); gli importi sono `REAL` (compravendita in €/m², locazione in €/m² al mese); le date di
caricamento sono testo `yyyy-MM-dd HH:mm:ss` in ora locale; le coordinate sono WGS84. Il DDL completo è al §11.4 e in `crates/omi-core/src/db.rs`.

### 11.2 Codifica delle geometrie

Una geometria è un **multipoligono**: lista di poligoni, ognuno lista di anelli (il primo è il bordo esterno, gli altri i buchi), con
coordinate in **microgradi** interi (`x = round(lon × 10^6)`, arrotondando all'intero pari). La colonna `geometria.dati` è un BLOB:

```
varint n_poligoni
per ogni poligono: varint n_anelli
  per ogni anello: varint n_punti
    per ogni punto: zigzag(x - px), zigzag(y - py)     (px, py partono da 0 e non si azzerano tra anelli e poligoni)
```

Gli interi senza segno sono *varint* LEB128; quelli con segno sono *zigzag*. Un poligono con un anello dei punti (1,2), (3,5), (0,0)
diventa `01 01 03 02 04 04 06 05 09`. `geometria.hash` è lo SHA-256 del BLOB troncato ai primi **16 byte**: serve a non
duplicare i perimetri uguali tra semestri. Le colonne `min_lon`, `min_lat`, `max_lon`, `max_lat`, `centro_lon`, `centro_lat`, `area_km2`
sono già calcolate (centro a 6 decimali, area a 4); `geometria_rt` ha le stesse bbox nell'ordine `id, min_lon, max_lon, min_lat, max_lat`.
Le formule (centroide, area, punto nel poligono, distanza dal bordo) sono in `crates/omi-core/src/geometry.rs`.

### 11.3 Query di esempio

```sql
-- semestri caricati e loro conteggi
SELECT semestre, n_zone, n_valori, n_geometrie FROM semestre ORDER BY semestre;

-- file caricati (nomi ripuliti)
SELECT file, semestre, contenuto, caricato FROM file_caricato ORDER BY caricato;

-- comuni censiti
SELECT comune_amm, descrizione, prov, primo_semestre, ultimo_semestre FROM comune ORDER BY descrizione;

-- serie storica dell'abitazione civile in stato NORMALE per una zona (link_zona)
SELECT semestre, compr_min, compr_max, loc_min, loc_max FROM valore
WHERE link_zona = 'RM00000001' AND cod_tip = 20 AND stato = 'NORMALE' ORDER BY semestre;

-- zone i cui bbox contengono un punto (candidati; la verifica puntuale richiede decodificare il poligono)
SELECT zg.comune_amm, zg.zona FROM geometria_rt rt
JOIN zona_geo zg ON zg.geometria_id = rt.id AND zg.semestre = 20242
WHERE rt.min_lon <= 12.495 AND rt.max_lon >= 12.495 AND rt.min_lat <= 41.905 AND rt.max_lat >= 41.905;
```

Le query sono state eseguite sul database sintetico di prova. Per le quotazioni delle abitazioni civili di un comune vedi anche la
query del §5.6. Il database non va modificato a mano mentre il servizio è in esecuzione: usare `omi-import`.

### 11.4 DDL dello schema

Eseguito da `omi-import` a ogni apertura in scrittura (`CREATE ... IF NOT EXISTS`):

```sql
CREATE TABLE IF NOT EXISTS semestre (
    semestre            INTEGER PRIMARY KEY,   -- AAAAS
    anno                INTEGER NOT NULL,
    numero              INTEGER NOT NULL,      -- 1 o 2
    elaborazione_zone   TEXT,
    elaborazione_valori TEXT,
    file_csv            TEXT,
    caricato_csv        TEXT,
    file_kml            TEXT,
    caricato_kml        TEXT,
    n_zone              INTEGER,
    n_valori            INTEGER,
    n_geometrie         INTEGER
);

CREATE TABLE IF NOT EXISTS zona (
    semestre          INTEGER NOT NULL,
    link_zona         TEXT    NOT NULL,
    area_territoriale TEXT,
    regione           TEXT,
    prov              TEXT,
    comune_istat      TEXT,
    comune_cat        TEXT,
    sez               TEXT,
    comune_amm        TEXT    NOT NULL,   -- codice catastale (Belfiore) del comune
    comune_descr      TEXT,
    fascia            TEXT,               -- B centrale, C semicentrale, D periferica, E suburbana, R rurale
    zona              TEXT    NOT NULL,
    zona_descr        TEXT,
    cod_tip_prev      INTEGER,
    descr_tip_prev    TEXT,
    stato_prev        TEXT,
    microzona         INTEGER,
    PRIMARY KEY (semestre, link_zona)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS ix_zona_comune ON zona(comune_amm, semestre, zona);
CREATE INDEX IF NOT EXISTS ix_zona_link ON zona(link_zona, semestre);

CREATE TABLE IF NOT EXISTS valore (
    semestre     INTEGER NOT NULL,
    link_zona    TEXT    NOT NULL,
    cod_tip      INTEGER NOT NULL,
    stato        TEXT    NOT NULL,   -- OTTIMO / NORMALE / SCADENTE (può essere vuoto)
    stato_prev   TEXT,               -- 'P' = stato conservativo prevalente
    compr_min    REAL,               -- €/m² compravendita
    compr_max    REAL,
    sup_nl_compr TEXT,               -- L lorda / N netta
    loc_min      REAL,               -- €/m² x mese locazione
    loc_max      REAL,
    sup_nl_loc   TEXT,
    PRIMARY KEY (semestre, link_zona, cod_tip, stato)
) WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS tipologia (
    cod_tip     INTEGER PRIMARY KEY,
    descrizione TEXT
);

CREATE TABLE IF NOT EXISTS geometria (
    id        INTEGER PRIMARY KEY,
    hash      BLOB NOT NULL UNIQUE,
    min_lon   REAL, min_lat REAL, max_lon REAL, max_lat REAL,
    centro_lon REAL, centro_lat REAL,
    area_km2  REAL,
    n_poligoni INTEGER,
    n_punti   INTEGER,
    dati      BLOB NOT NULL        -- codifica descritta nel manuale tecnico (§11.2)
);
CREATE VIRTUAL TABLE IF NOT EXISTS geometria_rt USING rtree(id, min_lon, max_lon, min_lat, max_lat);

CREATE TABLE IF NOT EXISTS zona_geo (
    id           INTEGER PRIMARY KEY,
    semestre     INTEGER NOT NULL,
    comune_amm   TEXT    NOT NULL,
    zona         TEXT    NOT NULL,
    link_zona    TEXT,
    nome         TEXT,
    colore       TEXT,             -- colore del poligono nel KML (AABBGGRR)
    geometria_id INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS ix_zona_geo_zona ON zona_geo(comune_amm, zona, semestre);
CREATE INDEX IF NOT EXISTS ix_zona_geo_geom ON zona_geo(geometria_id, semestre);
CREATE INDEX IF NOT EXISTS ix_zona_geo_sem ON zona_geo(semestre);

CREATE TABLE IF NOT EXISTS comune (
    comune_amm        TEXT PRIMARY KEY,
    comune_istat      TEXT,
    descrizione       TEXT,
    prov              TEXT,
    regione           TEXT,
    area_territoriale TEXT,
    primo_semestre    INTEGER,
    ultimo_semestre   INTEGER
);
CREATE INDEX IF NOT EXISTS ix_comune_istat ON comune(comune_istat);

CREATE TABLE IF NOT EXISTS comune_nome (
    comune_amm TEXT NOT NULL,
    nome       TEXT NOT NULL,
    nome_norm  TEXT NOT NULL,
    prov       TEXT,
    PRIMARY KEY (comune_amm, nome, prov)
) WITHOUT ROWID;
CREATE INDEX IF NOT EXISTS ix_comune_nome_norm ON comune_nome(nome_norm);

CREATE TABLE IF NOT EXISTS file_caricato (
    file        TEXT PRIMARY KEY,
    dimensione  INTEGER,
    semestre    INTEGER,
    contenuto   TEXT,
    caricato    TEXT
);
```

---

## 12. Sicurezza

**Superficie esposta.** `omi-mcp` ascolta su una porta TCP HTTP e risponde a `/mcp` (MCP), `/` (testo informativo) e `/health`
(stato del database). Non esistono rotte di scrittura: il database è aperto in sola lettura (`query_only=1`) e nessuno strumento
modifica i dati. Il corpo delle richieste è limitato a 1 MiB (413 oltre) e ogni richiesta ha un timeout di 60 secondi.

**Chi può chiamarlo.** Per impostazione predefinita il servizio ascolta solo su `127.0.0.1`, quindi solo i programmi dello stesso PC (stesso default di `install.ps1` e `install.sh`). Ogni
passo oltre va deciso consapevolmente:

| Esposizione | Protezioni consigliate |
|---|---|
| Solo PC locale (`--bind 127.0.0.1`) | Nessuna aggiuntiva |
| Rete locale (`--bind 0.0.0.0` o l'IP della scheda) | Token (`--token-file`), regola del firewall limitata alla rete locale, eventualmente HTTPS con `tls internal` |
| Internet (connettore di claude.ai) | Dietro Caddy con HTTPS; `--bind 127.0.0.1` per `omi-mcp`; **restrizione per IP** di Caddy (`160.79.104.0/21` più gli IP di prova); token se la funzione è disponibile; nessuna regola di firewall sulla 8765 |

**Token.** `--token-file` (preferibile a `--token`, che si vede nell'elenco dei processi) o la variabile `OMI_MCP_TOKEN`. Il confronto è
a tempo costante sui digest SHA-256; il file del token creato da `install.ps1` è leggibile solo da Administrators e dall'account del
servizio (su Linux `install.sh` lo crea di proprietà dell'utente `omi`, permessi 600). Le rotte `/` e `/health` restano libere: non rivelano dati. Un token errato dà 401 con il corpo `Token mancante o non valido.`
Il token non viene mai scritto nel log (nel log di avvio del servizio compare `********`). Un token in chiaro, fisso e valido a tempo indeterminato non è
un controllo d'accesso forte: ruotarlo (rigenerare `token.txt` e reinstallare) se si sospetta una fuga.

**`--allowed-host`.** Se il server è raggiungibile con un nome di dominio, `--allowed-host omi.esempio.it` rifiuta (403) le richieste con un `Host`
diverso, contro gli attacchi di tipo *DNS rebinding* sulla porta locale.

**`--trust-proxy`.** Dà fiducia all'intestazione `X-Forwarded-For`, che chiunque può falsificare se raggiunge direttamente il server. Usarlo
**solo** dietro un proxy affidabile e con `--bind 127.0.0.1`.

**Account del servizio.** Su Windows `LocalService` (predefinito) ha pochi privilegi; su Linux l'utente di sistema `omi` non ha shell né login e l'unità limita le scritture alla sola cartella dei dati. Concede il permesso di modifica solo alla cartella dei
dati. Non usare `localsystem` senza un motivo.

**Privacy del log.** Il file `omi.log` registra l'**indirizzo IP dei client**, i parametri di ogni chiamata (comune, zona, coordinate, semestre) e le
durate. Sono dati personali nel senso del GDPR se i client sono persone: limitare l'accesso al file, non condividerlo, definire un tempo di
conservazione e ruotarlo o cancellarlo (con il servizio fermo). Il log non è attivo se non si passa `--log`, tranne le righe del ciclo di vita
del servizio e gli errori interni. Nel log non finiscono i nomi originali degli ZIP né i token. Anche il log di accesso di Caddy
(`C:\ProgramData\Caddy\logs\omi-access.log` su Windows, `/var/log/caddy/omi-access.log` su Linux) contiene gli IP.

**Condizioni d'uso dei dati OMI.** Esporre il server a terzi significa rendere loro accessibili i dati dell'Agenzia delle Entrate. Le condizioni
contrattuali non consentono di vendere, affittare, trasferire o cedere a terzi i contenuti della banca dati; il database prodotto è a uso di chi ha
scaricato i dati e **non va condiviso né pubblicato**. Valutare la compatibilità con quelle condizioni prima di aprire il server a persone esterne (§16).

**Altre precauzioni.** Tenere gli ZIP scaricati fuori dai repository e dalle cartelle condivise (il nome contiene un codice fiscale); `.gitignore`
esclude `*.zip`, `*.db`, `*.log`. Aggiornare i binari quando escono nuove versioni delle dipendenze (`cargo audit`, se installato). Il software è fornito
"così com'è", senza garanzie.

---

## 13. Disinstallazione

### 13.1 Con gli script

Da PowerShell come amministratore, dalla cartella degli script:

```powershell
.\deploy\windows\uninstall.ps1                         # servizio OMIMyMCP, regola del firewall, programmi, PATH; i dati restano
.\deploy\windows\uninstall.ps1 -RemoveCaddy            # anche il servizio caddy, le regole 80/443 e C:\Program Files\Caddy
.\deploy\windows\uninstall.ps1 -RemoveCaddy -RemoveData  # anche C:\ProgramData\OMIMyMCP e C:\ProgramData\Caddy (con conferma; -Force la salta)
```

| Parametro | Significato (predefinito) |
|---|---|
| `-InstallDir` | Cartella dei programmi (`C:\Program Files\OMIMyMCP`) |
| `-DataDir` | Cartella dei dati (`C:\ProgramData\OMIMyMCP`) |
| `-ServiceName` | Nome del servizio (`OMIMyMCP`) |
| `-RemoveData` | Elimina anche i dati (database, log, token) e, con `-RemoveCaddy`, `C:\ProgramData\Caddy` (certificati e Caddyfile) |
| `-RemoveCaddy` | Ferma ed elimina il servizio `caddy`, le regole del firewall `OMIMyMCP-Caddy-80` e `OMIMyMCP-Caddy-443` e `CaddyInstallDir` |
| `-CaddyInstallDir`, `-CaddyConfigDir` | Cartelle di Caddy (`C:\Program Files\Caddy`, `C:\ProgramData\Caddy`) |
| `-Force` | Con `-RemoveData` non chiede conferma |

Lo script supporta `-WhatIf` ed è idempotente. Senza `-RemoveData` i dati restano: **il database è il frutto di ore di download e non
si recupera**, conviene eliminarlo solo se si è sicuri.

### 13.2 A mano, passo per passo

1. **Servizio OMIMyMCP** (ferma, elimina il servizio e la sua regola del firewall):

   ```powershell
   & 'C:\Program Files\OMIMyMCP\omi-mcp.exe' service uninstall
   ```

   Se `omi-mcp.exe` non c'è più: `sc.exe stop OMIMyMCP` e `sc.exe delete OMIMyMCP`.

2. **Servizio Caddy** (solo se installato):

   ```powershell
   sc.exe stop caddy
   sc.exe delete caddy
   ```

3. **Regole del firewall.** `service uninstall` toglie già quella di `omi-mcp.exe`. Quelle di Caddy e un'eventuale regola
   rimasta si eliminano indicando anche il programma, per non toccare regole omonime di altri:

   ```powershell
   netsh advfirewall firewall delete rule name="OMIMyMCP" program="C:\Program Files\OMIMyMCP\omi-mcp.exe"
   netsh advfirewall firewall delete rule name="OMIMyMCP-Caddy-80"
   netsh advfirewall firewall delete rule name="OMIMyMCP-Caddy-443"
   ```

4. **Voci del `PATH`** (solo se si era usato `-AddToPath`):

   ```powershell
   $path = [Environment]::GetEnvironmentVariable('Path', 'Machine') -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ne 'C:\Program Files\OMIMyMCP' }
   [Environment]::SetEnvironmentVariable('Path', ($path -join ';'), 'Machine')
   ```

5. **Cartelle dei programmi:**

   ```powershell
   Remove-Item -Recurse -Force 'C:\Program Files\OMIMyMCP'
   Remove-Item -Recurse -Force 'C:\Program Files\Caddy'        # se Caddy era installato
   Remove-Item -Recurse -Force 'C:\ProgramData\Caddy'          # certificati e Caddyfile: solo se non servono più
   ```

6. **Dati** (facoltativo): `Remove-Item -Recurse -Force 'C:\ProgramData\OMIMyMCP'` elimina database, log e token. Per tenere il
   database, spostarlo prima.

7. **Client:** in claude.ai rimuovere il connettore (**Customize > Connectors**, **Remove**; per Team/Enterprise **Organization settings > Connectors**);
   in Claude Code `claude mcp remove omi` (e, se aggiunto, `claude mcp remove cruscotto-italia`); in Claude Desktop cancellare la voce
   `"omi"` da `claude_desktop_config.json`.

### 13.3 Controlli finali

```powershell
sc.exe query OMIMyMCP                        # deve rispondere che il servizio non esiste (errore 1060)
sc.exe query caddy                           # idem, se Caddy è stato rimosso
netsh advfirewall firewall show rule name="OMIMyMCP"        # nessuna regola per omi-mcp.exe (vedi nota)
netstat -ano | findstr :8765                 # nessun processo in ascolto
.\deploy\windows\verifica-residui.ps1        # sola lettura, anche senza privilegi: segnala servizi, regole, cartelle, PATH, porta residui
```

Lo script `verifica-residui.ps1` stampa `[OK]`, `[INFO]` o `[RESIDUO]` per ogni elemento ed esce con 1 se trova residui; con
`-DatiMantenuti` non considera residuo la cartella dei dati. Una regola del firewall chiamata `OMIMyMCP` ma relativa a **un altro
programma** (campo "Programma") non è di OMIMyMCP: lo script la segnala come `[INFO]` e non va eliminata.

### 13.4 Per lo sviluppo

```powershell
cargo clean                  # elimina la cartella target\
rustup self uninstall        # solo se non serve più Rust
```

Per liberare lo spazio dei pacchetti: eliminare la cartella `dist/` (non quella `release/`, che è versionata).

### 13.5 Disinstallazione su Linux

Con lo script (come root):

```bash
sudo ./deploy/linux/uninstall.sh                         # unità omi-mcp, collegamenti in /usr/local/bin e /opt/omimymcp; i dati restano
sudo ./deploy/linux/uninstall.sh --remove-caddy          # anche Caddy: ferma e disabilita il servizio, toglie il Caddyfile generato
sudo ./deploy/linux/uninstall.sh --remove-data           # anche /var/lib/omimymcp e l'utente omi (con conferma; --force la salta)
```

Opzioni: `--install-dir`, `--data-dir`, `--service-name`, `--user`, `--remove-data`, `--remove-caddy`, `--force`, `--dry-run`. Il pacchetto di
Caddy (o il binario in `/usr/local/bin/caddy`) non viene rimosso: si toglie con il gestore di pacchetti (`sudo apt remove caddy`).

A mano:

```bash
sudo systemctl disable --now omi-mcp
sudo rm -f /etc/systemd/system/omi-mcp.service && sudo systemctl daemon-reload
sudo rm -f /usr/local/bin/omi-mcp /usr/local/bin/omi-import
sudo rm -rf /opt/omimymcp
sudo rm -rf /var/lib/omimymcp && sudo userdel omi        # facoltativo: elimina database, log, token e utente
```

Controlli finali: `systemctl status omi-mcp` (unità non trovata), `ss -ltn | grep 8765` (nessun processo in ascolto).

---

## 14. Risoluzione dei problemi

| Sintomo | Causa probabile | Soluzione |
|---|---|---|
| `Errore: impossibile mettersi in ascolto sulla porta 8765: …` (codice 1; per il servizio, codice specifico 2) | La porta è già usata da un altro programma (o da un'altra istanza di `omi-mcp`) | `netstat -ano \| findstr :8765` per trovare il processo; fermarlo o scegliere un'altra porta (`omi-mcp.exe omi.db 8766`, `--port 8766`) |
| `Servono i privilegi di amministratore: aprire il Prompt dei comandi o PowerShell con "Esegui come amministratore".` | `service install/uninstall/start/stop` o gli script `deploy\windows` eseguiti senza elevazione | Aprire PowerShell con **Esegui come amministratore** |
| Gli script `.sh` rispondono "Permission denied" | Bit di esecuzione perso (copia da Windows o da uno ZIP) | `chmod +x deploy/linux/*.sh`; i file devono avere fine riga LF (`dos2unix` se serve) |
| Su Linux il servizio parte e si ferma subito | Database mancante o non leggibile da `omi`; porta occupata; cartella dei dati non scrivibile per `omi` (WAL) | `journalctl -u omi-mcp -n 50`; `ls -ld /var/lib/omimymcp` deve mostrare `omi omi`; `ss -ltnp \| grep 8765` |
| Gli script `.ps1` non si eseguono ("l'esecuzione di script è disabilitata") | Criterio di esecuzione di PowerShell o file scaricati bloccati | `Set-ExecutionPolicy -Scope Process Bypass` nella sessione; eventualmente `Get-ChildItem -Recurse . \| Unblock-File` |
| Il servizio parte e si ferma subito | (1) Database mancante o percorso sbagliato (codice d'uscita del servizio 1); (2) porta occupata (codice 2); (3) l'account del servizio non può scrivere nella cartella del database e SQLite non riesce a creare `omi.db-wal`/`omi.db-shm` | Leggere le ultime righe di `C:\ProgramData\OMIMyMCP\omi.log` e `sc.exe query OMIMyMCP`. Verificare che `omi.db` esista in quel percorso; per i permessi `icacls C:\ProgramData\OMIMyMCP` deve mostrare modifica per `LocalService` (reinstallare senza `--no-acl`); non tenere il database in una cartella del profilo utente o su una cartella di rete (il journal WAL non funziona su condivisioni di rete) |
| `database is locked` negli errori o nel log | Un import lungo sta scrivendo e la lettura ha atteso più di 5 secondi (`busy_timeout`); oppure i file `-wal`/`-shm` non sono accessibili all'account del servizio | Attendere la fine dell'import o fermare il servizio durante i caricamenti (`aggiorna-dati.ps1`); controllare i permessi come sopra; non aprire il database con altri programmi in scrittura |
| `Database non trovato: omi.db. Crearlo con omi-import.` | Il percorso è relativo alla cartella corrente, o il database non è mai stato creato | Indicare il percorso completo; caricare prima gli ZIP (§5) |
| Il connettore di claude.ai non si collega | HTTPS assente o con certificato non valido (anche `tls internal`); DNS che non punta alla macchina; porta 80 bloccata dal provider (il challenge di Let's Encrypt fallisce); NAT senza inoltro 80/443; restrizione per IP che esclude `160.79.104.0/21`; proxy che mette in buffer le risposte | Da un dispositivo **fuori** dalla rete aprire `https://<dominio>/health` (deve rispondere `ok`); controllare DNS, porte e certificato (§8); nel Caddyfile `flush_interval -1`; in alternativa usare il tunnel `cloudflared` (§8.7) |
| Errore 401 `Token mancante o non valido.` | Il server ha il token e il client non lo invia, o è diverso da `token.txt` | Aggiungere l'intestazione `Authorization: Bearer <token>` nel client (§9); verificare il contenuto di `token.txt` senza spazi finali |
| Errore 403 sul connettore | La restrizione per IP di Caddy blocca la richiesta, oppure `--allowed-host` non comprende il nome usato | Ammettere `160.79.104.0/21`; aggiungere `--allowed-host <dominio>` o toglierlo |
| `/l` non viene riconosciuto in Git Bash (il programma lo tratta come percorso) | Git Bash converte gli argomenti che iniziano con `/` | Usare `-l` (o `--log`) |
| Con i caratteri jolly: `Nessun file corrisponde a: QI*.zip` | La cartella indicata non contiene file con quel nome, o il percorso è sbagliato; in Git Bash la shell può espandere il jolly in modo diverso | Controllare la cartella; indicare il percorso completo tra apici (`'C:\DatiOMI\QI*.zip'`): `omi-import` espande i caratteri jolly da solo |
| `File non riconosciuto (atteso QI*.zip): …` oppure `Parametro non valido: …` | Un argomento non è né uno ZIP `QI*` né un database `.db` | Controllare gli argomenti; il nome degli ZIP deve iniziare con `QI` |
| Durante l'import: `ERRORE su …: errore ZIP: …` e codice d'uscita 2 | Lo ZIP è danneggiato o incompleto | Scaricarlo di nuovo; gli altri file sono stati caricati comunque |
| `Attenzione: C302.kml malformato (…); letti 3 placemark, scartati 1` | Un KML dell'Agenzia è malformato (caso noto del 2011/1) | Nessuna: l'importatore carica ciò che è leggibile; le zone senza perimetro restano senza |
| `Impossibile determinare il semestre dei CSV/KML` | ZIP non standard, senza il semestre né nel nome né nel titolo | Usare gli ZIP originali, senza rinominarli né estrarli |
| Risposte vuote o "Nessuna zona per il comune nel semestre indicato" | Semestre o comune sbagliati; il comune non è nel semestre; fornitura parziale | `elenco_semestri` per i semestri disponibili; `cerca_comuni` per il codice del comune; indicare provincia o codice catastale |
| `Semestre 2020/1 non presente nel database. Usare elenco_semestri.` | Il semestre richiesto non è stato caricato | Caricare la fornitura o scegliere un semestre disponibile |
| `Nel database non sono presenti perimetri di zona (file KML).` | Nessuna fornitura `QIP` caricata | Caricare almeno un `QIP*.zip` (dal 2010/2) |
| `'xxx' è ambiguo, specificare la provincia o il codice catastale. …` | Più comuni con lo stesso nome | Indicare la sigla della provincia o il codice catastale |
| `Usare --token oppure --token-file, non entrambi.` | Opzioni in conflitto | Usarne una sola |
| `Errore interno durante l'esecuzione di '<tool>'.` | Errore del database o bug | Cercare la riga `ERRORE INTERNO in '<tool>'` in `omi.log` |
| `il servizio OMIMyMCP è già installato: per reinstallarlo eseguire prima omi-mcp service uninstall --name OMIMyMCP` | `OMIMyMCP` è già installato | `omi-mcp.exe service uninstall --keep-firewall`, oppure rilanciare `install.ps1`, che lo fa da solo |

---

## 15. Scelte di progetto

- **Formati:** schema SQLite (§11.4), codifica delle geometrie (§11.2), hash SHA-256 troncato a 16 byte e convenzioni sui semestri
  sono stabili: il database può essere letto e scritto da qualunque strumento che li rispetti. Gli arrotondamenti sono sempre
  all'intero pari (`round_ties_even`), tranne il `round()` SQL di `statistiche_comune`.
- **Importatore:** i messaggi mostrano sempre il nome ripulito degli ZIP (mai il codice fiscale); `--help`, `-h`, `/?` e `--version` hanno la
  precedenza; gli errori sui parametri vanno su stderr con codice 1; uno ZIP "ignorato" non fa salire il codice d'uscita a 2; ogni file
  è caricato in una sola transazione.
- **Server:** trasporto senza stato (`NeverSessionManager`, risposte `application/json`, `GET`/`DELETE` su `/mcp` danno 405); pool di
  connessioni in sola lettura scritto a mano; deserializzatori tolleranti per numeri e booleani come stringa; l'errore interno ha un
  testo fisso e il dettaglio solo nel log; `--allowed-host` per il controllo dell'intestazione `Host`; su Unix arresto ordinato con
  `SIGINT` e `SIGTERM`.
- **Servizio Windows:** codice in `service_windows.rs` dietro `#[cfg(windows)]`; permesso di modifica sulla cartella dei dati per
  l'account del servizio (WAL); la regola del firewall si chiama come il servizio e viene cancellata filtrando anche per programma;
  codici d'uscita 1/2/3 e riavvio dopo 5, 30 e 60 secondi; il log del ciclo di vita è sempre su file.
- **Servizio Linux:** unità systemd generata da `install.sh`, con utente di sistema dedicato e filesystem in sola lettura tranne la
  cartella dei dati; il ciclo di vita va nel journal e, con `--log`, le richieste in `omi.log`.
- **Script:** i PowerShell sono salvati in UTF-8 con BOM (necessario per Windows PowerShell 5.1) e supportano `-WhatIf`; gli script bash
  supportano `--dry-run`; i token esistenti non vengono rigenerati; gli script di aggiornamento non stampano i nomi dei file;
  `verifica-residui.ps1` è di sola lettura.
- **Caddy:** `email {$OMI_ACME_EMAIL}` con variabile vuota non è valido, quindi gli script di installazione scrivono i valori nel
  Caddyfile; `deploy/caddy/LEGGIMI.md` riporta requisiti di rete e compatibilità delle protezioni con i connettori di claude.ai.
- **Privacy:** il nome originale degli ZIP (con il codice fiscale) non finisce mai nel database, nel log, nei test o in git.
- **Test:** i valori dei risultati di riferimento ("golden") sono scritti a mano e non derivati dall'output del codice.

---

## 16. Licenza

### Codice

Il codice è distribuito con licenza **Apache License 2.0** (file `LICENSE`, testo integrale; ogni file `.rs` ha l'intestazione
`SPDX-License-Identifier: Apache-2.0`). Il file `NOTICE` va conservato da chi ridistribuisce il software o opere derivate (sezione 4(d)
della licenza). Copyright 2026 Explobot.

### Crate di terze parti

Gli eseguibili per Windows includono **146 crate** Rust (dipendenze normali, ricavate con `cargo tree --workspace -e normal`, che riporta la
licenza dichiarata da ciascuna), più SQLite compilato da `rusqlite` (`bundled`, pubblico dominio, <https://sqlite.org>). `cargo about` e `cargo license`
non erano installati; l'elenco completo con versione e licenza è nel file `NOTICE`. Riepilogo per licenza:

| Licenza dichiarata | Crate |
|---|---|
| `MIT OR Apache-2.0` (anche scritto `Apache-2.0 OR MIT` o `MIT/Apache-2.0`) | 95 |
| `MIT` (di cui una `MIT AND BSD-3-Clause`) | 34 |
| `Apache-2.0` | 5 |
| `BSD-3-Clause` | 3 |
| `Zlib` (`foldhash`, `zlib-rs`) | 2 |
| `Unlicense OR MIT` | 2 |
| `CC0-1.0 OR MIT-0` (una anche `OR Apache-2.0`) | 2 |
| `Apache-2.0 OR BSL-1.0` | 1 |
| `bzip2-1.0.6` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |

Nessuna crate è soggetta a licenze copyleft. Le licenze permissive richiedono di conservare gli avvisi di copyright: il `NOTICE` e l'elenco in esso
contenuto servono a questo.

### Dati OMI

Il progetto **non contiene né distribuisce dati OMI**. I dati restano soggetti alle condizioni contrattuali e d'uso dell'Agenzia delle Entrate
(<https://www.agenziaentrate.gov.it/portale/schede/fabbricatiterreni/omi/banche-dati/quotazioni-immobiliari/condizioni-contrattuali-qi>), che non
associa una licenza aperta standard. In sintesi:

- le informazioni sono di proprietà esclusiva dell'Agenzia delle Entrate;
- è obbligatorio citare la fonte **"Agenzia delle Entrate - OMI"**, anche per dati elaborati;
- l'elaborazione per uso proprio è consentita;
- non è consentito vendere, affittare, trasferire o cedere a terzi i contenuti della banca dati: il database prodotto è a uso di chi ha scaricato i dati e
  **non va condiviso né pubblicato**;
- l'Agenzia avverte che revisioni delle zone e dei metodi hanno prodotto salti di quotazione tra semestri: le serie storiche vanno confrontate con cautela;
- esporre il server a terzi significa rendere loro accessibili i dati: va valutato alla luce di queste condizioni.

---

## Appendice A: riferimento della riga di comando

### `omi-import --help`

```text
OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)
Carica nel database SQLite le quotazioni immobiliari OMI dell'Agenzia delle Entrate.

Uso:
  omi-import [QI*.zip ...] [database.db]
  omi-import --help | -h | /?
  omi-import --version

Argomenti, in qualunque ordine:
  QI*.zip, QIP*.zip  forniture scaricate dal sito dell'Agenzia delle Entrate, senza
                     rinominarle; sono ammessi i caratteri jolly * e ?
  database.db        database da aggiornare (predefinito: omi.db nella cartella corrente);
                     se non esiste viene creato

Senza argomenti carica tutti i file QI*.zip e QIP*.zip della cartella corrente.

Codici d'uscita: 0 caricamento riuscito, 1 parametri non validi o errore grave,
2 almeno un file non caricato.
```

### `omi-mcp --help`

`omi-mcp service --help` mostra lo stesso testo.

```text
OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)
Server MCP sulle quotazioni immobiliari OMI (Agenzia delle Entrate).

Uso:
  omi-mcp [database.db] [porta] [/l] [--bind INDIRIZZO] [--trust-proxy]
          [--token SEGRETO | --token-file FILE] [--allowed-host HOST ...]
  omi-mcp service install --db FILE [opzioni del servizio]   (solo Windows, come amministratore)
  omi-mcp service uninstall [--name NOME] [--keep-firewall]
  omi-mcp service start|stop|status [--name NOME]
  omi-mcp --help | -h | /?
  omi-mcp --version

  database.db          database creato con omi-import (predefinito: omi.db), aperto in sola lettura
  porta                porta TCP (predefinita: 8765; 0 = scelta dal sistema)
  /l, -l, --log        registra richieste HTTP e chiamate ai tool nel file <database>.log
                       (in Git Bash usare -l)
  --bind INDIRIZZO     indirizzo di ascolto (predefinito: tutte le interfacce, IPv4 e IPv6);
                       dietro un reverse proxy come Caddy: --bind 127.0.0.1
  --trust-proxy        nel log usa l'IP del client indicato da X-Forwarded-For
  --token SEGRETO      richiede "Authorization: Bearer SEGRETO" su /mcp (/ e /health restano liberi)
  --token-file FILE    come --token, con il segreto nella prima riga del file
                       (in alternativa: variabile d'ambiente OMI_MCP_TOKEN)
  --allowed-host HOST  accetta su /mcp solo le richieste con questo Host (oltre a localhost,
                       127.0.0.1 e ::1); ripetibile. Senza l'opzione l'Host non viene controllato.

Endpoint: /mcp (MCP su HTTP streamable), / (informazioni), /health (stato del database).

Opzioni del servizio Windows (service install):
  --db FILE            database (obbligatorio; percorso reso assoluto)
  --name NOME          nome del servizio (predefinito: OMIMyMCP; vale anche per gli altri comandi)
  --display-name TESTO nome visualizzato (predefinito: "OMIMyMCP - Server MCP quotazioni OMI")
  --port PORTA         porta TCP (predefinita: 8765)
  --bind INDIRIZZO     indirizzo di ascolto (predefinito: 127.0.0.1)
  --log, --trust-proxy come sopra
  --token-file FILE    file con il token Bearer, leggibile dall'account del servizio
  --start auto|delayed|manual
                       avvio automatico (predefinito), automatico ritardato o manuale
  --account localservice|networkservice|localsystem
                       account del servizio (predefinito: localservice)
  --firewall           aggiunge una regola in ingresso nel firewall di Windows
  --no-acl             non concede all'account il permesso di modifica sulla cartella del database
  --now                avvia subito il servizio e verifica /health
  --keep-firewall      (uninstall) lascia la regola del firewall
Il file di log <database>.log registra sempre avvio, arresto ed errori del servizio.
```

Comandi di `omi-mcp service`: `install` (richiede `--db`; opzioni elencate nel §7.2), `uninstall [--name NOME] [--keep-firewall]`,
`start|stop|status [--name NOME]`. Sono disponibili solo su Windows. I comandi `install`, `uninstall`, `start` e `stop` richiedono i privilegi
di amministratore; `status` no.

### Script PowerShell (`deploy\windows`)

| Script | Parametri | Scopo |
|---|---|---|
| `install.ps1` | `-SourceDir` (obbligatorio), `-InstallDir`, `-DataDir`, `-Port`, `-Bind`, `-ServiceName`, `-Log`, `-Firewall`, `-WithToken`, `-TrustProxy`, `-AddToPath` | Installa il servizio (§7.1) |
| `uninstall.ps1` | `-InstallDir`, `-DataDir`, `-ServiceName`, `-CaddyInstallDir`, `-CaddyConfigDir`, `-RemoveData`, `-RemoveCaddy`, `-Force` | Disinstalla (§13.1) |
| `aggiorna-dati.ps1` | `-ZipDir` (obbligatorio), `-DataDir`, `-InstallDir`, `-ServiceName` | Carica nuove forniture (§7.6) |
| `install-caddy.ps1` | `-CaddyExe`, `-Dominio` (obbligatori), `-Email`, `-InstallDir`, `-ConfigDir`, `-OmiServiceName`, `-Port`, `-Template` | Installa Caddy (§8.3) |
| `build-release.ps1` | `-CertThumbprint`, `-TimestampUrl`, `-SkipTests`, `-OutDir` | Crea il pacchetto di rilascio (§3.5) |
| `test-servizio.ps1` | `-SourceDir`, `-Db`, `-ServiceName` (`OMIMyMCP-Test`), `-Port` (18765), `-KeepFiles` | Prova di installazione completa con un servizio di prova |
| `verifica-residui.ps1` | `-ServiceName`, `-InstallDir`, `-DataDir`, `-Port`, `-DatiMantenuti` | Controlla che non restino residui (§13.3) |

Tutti gli script di installazione, disinstallazione, aggiornamento e di Caddy supportano `-WhatIf`.

### Script bash (`deploy/linux`)

| Script | Opzioni | Scopo |
|---|---|---|
| `install.sh` | `--source-dir` (obbligatoria), `--install-dir`, `--data-dir`, `--port`, `--bind`, `--service-name`, `--user`, `--log`, `--with-token`, `--trust-proxy`, `--allowed-host`, `--add-to-path`, `--dry-run` | Installa il servizio systemd (§7.8) |
| `uninstall.sh` | `--install-dir`, `--data-dir`, `--service-name`, `--user`, `--remove-data`, `--remove-caddy`, `--force`, `--dry-run` | Disinstalla (§13.5) |
| `aggiorna-dati.sh` | `--zip-dir` (obbligatoria), `--data-dir`, `--install-dir`, `--service-name`, `--user`, `--dry-run` | Carica nuove forniture (§7.8) |
| `install-caddy.sh` | `--domain` (obbligatoria), `--email`, `--caddy-bin`, `--config-dir`, `--omi-service`, `--port`, `--template`, `--no-firewall`, `--dry-run` | Configura Caddy (§8.9) |
| `build-release.sh` | `--skip-tests`, `--out-dir`, `--target` | Crea il pacchetto `.tar.gz` (§3.5) |
| `test-servizio.sh` | `--db` (obbligatoria), `--source-dir`, `--port`, `--keep-files` | Prova di installazione con un servizio di prova |
| `omi-mcp.service` | | Unità systemd di esempio, per l'installazione manuale |

Tutti gli script mostrano l'aiuto con `--help`.

## Appendice B: cartelle installate

```
C:\Program Files\OMIMyMCP\           programmi (install.ps1)
├─ omi-mcp.exe
├─ omi-import.exe
├─ LICENSE
├─ NOTICE
└─ MANUALE_TECNICO.md

C:\ProgramData\OMIMyMCP\             dati (account del servizio con permesso di modifica)
├─ omi.db                            database
├─ omi.db-wal, omi.db-shm            file del journal WAL (compaiono con il database in uso)
├─ omi.log                           log del servizio e, con --log, delle richieste
└─ token.txt                         token Bearer (solo con -WithToken)

C:\Program Files\Caddy\caddy.exe     solo con install-caddy.ps1
C:\ProgramData\Caddy\                solo con install-caddy.ps1
├─ Caddyfile
├─ data\                             certificati e stato (da conservare)
└─ logs\omi-access.log               log di accesso di Caddy
```

Linux (`install.sh` con i valori predefiniti):

```
/opt/omimymcp/                       programmi
├─ omi-mcp
├─ omi-import
├─ LICENSE, NOTICE
└─ MANUALE_TECNICO.md
/usr/local/bin/omi-mcp, omi-import   collegamenti (solo con --add-to-path)
/etc/systemd/system/omi-mcp.service  unità systemd
/var/lib/omimymcp/                   dati (proprietario: utente omi, 750)
├─ omi.db, omi.db-wal, omi.db-shm
├─ omi.log
└─ token.txt                         solo con --with-token (600)
/etc/caddy/Caddyfile                 solo con install-caddy.sh
/var/lib/caddy/data/                 certificati e stato di Caddy (da conservare)
/var/log/caddy/omi-access.log        log di accesso di Caddy
```

Pacchetto di rilascio Windows (`release\windows-x64\` nel repository, oppure `dist\OMIMyMCP-rs-1.0.0-win-x64.zip` se ricreato con `build-release.ps1`):

```
omi-import.exe, omi-mcp.exe, LICENSE, NOTICE, MANUALE_TECNICO.md, LEGGIMI.md
deploy\windows\*.ps1
deploy\caddy\*.example, deploy\caddy\LEGGIMI.md
```

Pacchetto di rilascio Linux (`dist/OMIMyMCP-rs-1.0.0-linux-x64.tar.gz`):

```
omi-import, omi-mcp, LICENSE, NOTICE, README.md, MANUALE_TECNICO.md
deploy/linux/*.sh, deploy/linux/omi-mcp.service
deploy/caddy/*.example, deploy/caddy/LEGGIMI.md
```

Gli script si lanciano dalla radice del pacchetto estratto (`.\deploy\windows\install.ps1 -SourceDir .\` su Windows, `sudo ./deploy/linux/install.sh --source-dir .` su Linux).

## Appendice C: file prodotti

| File | Prodotto da | Contenuto |
|---|---|---|
| `omi.db` | `omi-import` | Database SQLite (§11) |
| `omi.db-wal` | SQLite | Scritture non ancora riportate nel file principale. Non cancellarlo mentre il database è in uso: contiene dati committati. `omi-import` esegue un *checkpoint* `TRUNCATE` alla fine del caricamento |
| `omi.db-shm` | SQLite | Memoria condivisa dell'indice del WAL; si ricrea da solo |
| `omi.log` (cioè `<database>.log`) | `omi-mcp` | Log in *append* (§6.3); mai svuotato dal programma |
| `token.txt` | `install.ps1 -WithToken`, `install.sh --with-token` | Token Bearer (un solo rigo); non va condiviso |
| `SHA256SUMS.txt`, `SHA256SUMS-linux.txt` | `build-release.ps1`, `build-release.sh` | Hash SHA-256 dello ZIP / dell'archivio di rilascio |

Per copiare o archiviare il database: fermare il servizio (o attendere che nessuno scriva) e copiare `omi.db`; dopo un caricamento completato da `omi-import` il
file principale è completo anche senza `-wal`.
