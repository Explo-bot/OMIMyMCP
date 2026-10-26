# OMIMyMCP

Importa le quotazioni immobiliari dell'**Osservatorio del Mercato Immobiliare (OMI)** dell'Agenzia delle Entrate
in un database SQLite e le rende interrogabili come **server MCP** da Claude (claude.ai, Claude Desktop,
Cowork, Claude Code) o da qualunque altro client MCP.

Implementazione in **Rust** per **Windows** e **Linux**: due programmi (`omi-import` e `omi-mcp`) che condividono una libreria.

Autore: **Explobot** – progetto originale: <https://github.com/Explo-bot/OMIMyMCP> – licenza [Apache-2.0](LICENSE)

> ## ⚠️ AVVISO IMPORTANTE – LEGGERE PRIMA DELL'USO
>
> **Questo software è stato sviluppato con Claude Code**, a partire dalla skill e dagli script del repository
> <https://github.com/Explo-bot/OMIMyMCPScript>.
> Il codice **non è ancora stato sottoposto a revisione** da parte di sviluppatori umani, né a una verifica
> sistematica di correttezza, sicurezza e affidabilità: è **in attesa di revisione**.
>
> Il software è fornito **"così com'è" (as is)**, senza garanzie di alcun tipo, espresse o implicite,
> incluse a titolo esemplificativo la commerciabilità, l'idoneità a uno scopo particolare e l'assenza di
> errori. **L'utilizzo è a esclusivo rischio e pericolo dell'utente.** L'autore non risponde di alcun danno,
> perdita di dati o conseguenza, diretta o indiretta, derivante dall'uso del software o dall'affidamento
> sui risultati prodotti.
>
> Le quotazioni OMI hanno valore puramente indicativo e **non sostituiscono la stima di un professionista**:
> ogni decisione economica va verificata su fonti ufficiali. Verificare sempre i risultati prima di
> utilizzarli in contesti professionali, e non esporre il server a Internet senza le opportune protezioni.

> **Fonte dei dati: Agenzia delle Entrate - OMI.**
> Questo progetto non è affiliato né approvato dall'Agenzia delle Entrate. Leggere la sezione
> [Licenza e obblighi](#licenza-e-obblighi) prima di usare o ridistribuire i dati.
> Descrizione ufficiale del servizio:
> [Forniture dati OMI](https://www.agenziaentrate.gov.it/portale/it/web/guest/schede/fabbricatiterreni/omi/forniture-dati-omi).

## Indice

- [Come funziona](#come-funziona)
- [Avvio rapido](#avvio-rapido)
- [Contenuto del repository](#contenuto-del-repository)
- [Scaricare i dati dal sito dell'Agenzia delle Entrate](#scaricare-i-dati-dal-sito-dellagenzia-delle-entrate)
- [Scaricare molti file in modo massivo con Claude Chat](#scaricare-molti-file-in-modo-massivo-con-claude-chat)
- [Compilazione](#compilazione)
- [Versione compilata per Windows](#versione-compilata-per-windows)
- [Caricare i dati con omi-import](#caricare-i-dati-con-omi-import)
- [Uso del server omi-mcp](#uso-del-server-omi-mcp)
- [Installazione come servizio](#installazione-come-servizio)
- [HTTPS con Caddy](#https-con-caddy)
- [Collegamento a Claude come server MCP](#collegamento-a-claude-come-server-mcp)
- [Uso combinato con MCP Cruscotto Italia](#uso-combinato-con-mcp-cruscotto-italia)
- [Esempi di utilizzo con Claude Cowork](#esempi-di-utilizzo-con-claude-cowork)
- [Strumenti MCP](#strumenti-mcp)
- [Struttura del database](#struttura-del-database)
- [Sicurezza](#sicurezza)
- [Risoluzione dei problemi](#risoluzione-dei-problemi)
- [Licenza e obblighi](#licenza-e-obblighi)

## Come funziona

Il progetto **non contiene né distribuisce dati OMI**. Ogni utente scarica gratuitamente dal sito
dell'Agenzia delle Entrate le forniture dei semestri che gli interessano e le carica con `omi-import` nel proprio
database SQLite locale; `omi-mcp` lo apre in sola lettura ed espone 9 strumenti MCP:

```
Agenzia delle Entrate         omi-import                    omi-mcp                     Claude / client MCP
Forniture dati OMI  ──ZIP──►  caricamento  ──►  omi.db  ──►  server MCP (/mcp)  ──►  (HTTP, o HTTPS via Caddy)
(QI*.zip, QIP*.zip)
```

| Programma | Ruolo |
|---|---|
| `omi-import` | Riga di comando: carica in `omi.db` i file `QI*.zip` / `QIP*.zip` scaricati dal sito dell'Agenzia delle Entrate |
| `omi-mcp` | Server MCP (Axum + rmcp, HTTP *streamable* su `/mcp`) con 9 strumenti, in sola lettura sul database. Da console, come servizio Windows o come unità systemd su Linux. Per l'HTTPS si usa Caddy come proxy esterno e facoltativo |

Si può caricare un solo semestre o l'intera serie storica, l'intero territorio nazionale o solo alcune
regioni, province o comuni. I nuovi semestri si aggiungono man mano che l'Agenzia delle Entrate li pubblica.

## Avvio rapido

Dopo aver scaricato gli ZIP dall'Agenzia delle Entrate (vedi [sotto](#scaricare-i-dati-dal-sito-dellagenzia-delle-entrate)).

**Windows** (PowerShell, con la [versione compilata](#versione-compilata-per-windows) estratta o dopo la compilazione; gli ZIP sono in `C:\DatiOMI`):

```powershell
cd release\windows-x64            # oppure target\release dopo "cargo build --release --workspace"
.\omi-import.exe C:\DatiOMI\QI*.zip C:\DatiOMI\omi.db
.\omi-mcp.exe C:\DatiOMI\omi.db 8765
claude mcp add --transport http omi http://localhost:8765/mcp
claude mcp list
```

**Linux** (bash, dopo `cargo build --release --workspace`; gli ZIP sono in `~/DatiOMI`):

```bash
target/release/omi-import ~/DatiOMI/QI*.zip ~/DatiOMI/omi.db
target/release/omi-mcp ~/DatiOMI/omi.db 8765
claude mcp add --transport http omi http://localhost:8765/mcp
claude mcp list
```

Per l'avvio automatico come servizio vedere [Installazione come servizio](#installazione-come-servizio).

## Contenuto del repository

```
Cargo.toml, Cargo.lock, .cargo/config.toml    workspace Rust (3 crate)
crates/
  omi-core/        libreria: database, geometria, testo, import, logica degli strumenti MCP; test e risultati di riferimento (tests/golden)
  omi-import/      programma omi-import
  omi-mcp/         programma omi-mcp (server e servizio Windows)
deploy/
  windows/         script PowerShell: install, uninstall, aggiorna-dati, install-caddy, build-release, test-servizio, verifica-residui; LEGGIMI.md
  linux/           script bash: install, uninstall, aggiorna-dati, install-caddy, build-release, test-servizio; unità systemd di esempio
  caddy/           Caddyfile di esempio (Internet e rete locale) e guida
docs/
  MANUALE_TECNICO.md   manuale completo (16 capitoli e 3 appendici)
release/
  windows-x64/     pacchetto Windows già compilato (omi-import.exe, omi-mcp.exe, SHA256SUMS.txt), con script e istruzioni (LEGGIMI.md)
LICENSE, NOTICE    licenza Apache-2.0 e avvisi (da conservare se si ridistribuisce)
```

## Scaricare i dati dal sito dell'Agenzia delle Entrate

I dati si scaricano gratuitamente dal servizio **Forniture dati OMI**, riservato agli utenti
autenticati.

### Periodi disponibili

| Dato | Disponibile da | Fornitura da richiedere | File ottenuto |
|---|---|---|---|
| Quotazioni (zone e valori, CSV) | 1° semestre 2004 | **Quotazioni immobiliari** | `QI<n. richiesta>_<codice fiscale>.zip` |
| Perimetri delle zone (KML) **più** le quotazioni dello stesso semestre | 2° semestre 2010 | **Perimetri delle zone OMI** | `QIP<n. richiesta>_<codice fiscale>.zip` |

L'Agenzia delle Entrate pubblica i dati di un semestre di norma alcuni mesi dopo la sua chiusura (ad esempio quelli
del 2° semestre 2025 sono stati generati a marzo 2026). Ogni file riguarda un **solo semestre**: per
più periodi si fanno più richieste.

### Quali file scaricare

| Periodo desiderato | Cosa richiedere per ogni semestre |
|---|---|
| dal 2° semestre 2010 in poi | solo la fornitura **Perimetri delle zone OMI**: contiene sia i perimetri sia le quotazioni |
| dal 1° semestre 2004 al 1° semestre 2010 | la fornitura **Quotazioni immobiliari** (per questi semestri i perimetri non esistono) |

Senza perimetri OMIMyMCP funziona comunque, ma la ricerca per coordinate (`zona_da_coordinate`,
`zone_vicine`) ha bisogno di almeno un semestre con perimetri: per i semestri che ne sono privi usa
quelli del semestre più vicino.

Dimensioni indicative per l'intero territorio nazionale: circa 2 MB per un file `QI` e 40–130 MB per
un file `QIP`. L'intera serie 2004–2025 occupa circa 3,5 GB di ZIP e produce un database di circa 2 GB.

### Procedura di download

1. Aprire la pagina di spiegazione del servizio:
   <https://www.agenziaentrate.gov.it/portale/it/web/guest/schede/fabbricatiterreni/omi/forniture-dati-omi>
2. Premere **Accedi al servizio** (oppure andare direttamente su
   <https://telematici.agenziaentrate.gov.it/Main/index.jsp>).
3. Autenticarsi con **SPID**, **CIE**, **CNS** oppure con credenziali **Fisconline/Entratel**.
4. Nell'area riservata aprire il servizio **Forniture dati OMI**. Se non compare nel menu dei servizi
   usare la funzione di ricerca dell'area riservata: la posizione nei menu può cambiare nel tempo.
5. Scegliere la fornitura (**Perimetri delle zone OMI** o **Quotazioni immobiliari**, vedi tabella sopra).
6. Selezionare l'**ambito territoriale**: *intero territorio nazionale* oppure una regione, una provincia
   o un comune. OMIMyMCP accetta qualunque ambito e le forniture parziali si sommano tra loro.
7. Selezionare il **semestre** e inviare la richiesta.
8. Le richieste vengono elaborate in modo asincrono: il file compare nell'elenco delle forniture
   richieste del servizio e si scarica quando è pronto (di solito pochi minuti per i file nazionali).
9. Ripetere per ogni semestre desiderato e salvare tutti gli ZIP in una stessa cartella, **senza
   rinominarli né estrarli**.

> **Attenzione alla privacy.** Il nome dei file forniti dall'Agenzia contiene il **codice fiscale di chi ha
> fatto la richiesta**. OMIMyMCP lo rimuove dai nomi registrati nel database e non lo scrive mai nei log, ma gli ZIP originali non vanno
> pubblicati né condivisi: il `.gitignore` del progetto esclude `*.zip` e i file `.db`.

### Scaricare molti file in modo massivo con Claude Chat

Richiedere e scaricare a mano decine di semestri è ripetitivo. I file possono essere scaricati **in modo
massivo e senza fatica con Claude Chat**, lasciando che Claude ripeta al posto dell'utente la procedura sul
sito dell'Agenzia delle Entrate (richiesta, attesa dell'elaborazione, download di ogni ZIP). Serve una modalità di
Claude in grado di usare il browser (per esempio Claude in Chrome).

**Esempio di prompt:**

> Mi scarichi da https://fornituredatiomi.agenziaentrate.gov.it/dati-omi-fe/RF
> le quotazioni immobiliari complete di perimetri di zona per ciascun semestre
> disponibile a livello nazionale.

> **Autenticazione.** L'accesso al sito dell'Agenzia delle Entrate (SPID, CIE, CNS o Fisconline/Entratel)
> viene richiesto **in modo interattivo**: Claude si ferma e attende che sia l'utente ad autenticarsi nel
> browser. Le credenziali non vanno mai scritte nel prompt né comunicate a Claude.
> Restano valide le note su [download e privacy](#procedura-di-download): gli ZIP contengono il codice
> fiscale di chi ha fatto la richiesta e non vanno condivisi.

## Compilazione

Il workspace compila e supera i test sia su Windows sia su Linux. Il codice del servizio Windows è compilato solo
su Windows (`#[cfg(windows)]`). Versione minima di Rust: **1.99**.

### Windows

1. Installare i **Build Tools per Visual Studio** con il carico di lavoro *Sviluppo di applicazioni desktop con C++*
   (`winget install Microsoft.VisualStudio.2022.BuildTools --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"`).
2. Installare **Rust** da <https://rustup.rs> (`winget install Rustlang.Rustup`) con la toolchain `stable-x86_64-pc-windows-msvc`, e Git (`winget install Git.Git`).
3. In un nuovo terminale PowerShell:

```powershell
git clone https://github.com/Explo-bot/OMIMyMCP.git
cd OMIMyMCP
cargo build --release --workspace
target\release\omi-import.exe --version
target\release\omi-mcp.exe --version
```

Gli eseguibili sono `target\release\omi-import.exe` (circa 4,4 MB) e `target\release\omi-mcp.exe` (circa 7,5 MB): sono
autonomi (runtime C statico, SQLite incluso) e non richiedono altro software sul PC di destinazione.
Se `link.exe` non viene trovato mancano i Build Tools con il carico di lavoro C++: aggiungerli, oppure compilare da "Developer PowerShell for VS".

### Linux

Esempio per Debian/Ubuntu (su altre distribuzioni servono gli equivalenti di `gcc`, `make` e `git`):

```bash
sudo apt install build-essential git curl
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh      # toolchain stable
. "$HOME/.cargo/env"
git clone https://github.com/Explo-bot/OMIMyMCP.git
cd OMIMyMCP
cargo build --release --workspace
target/release/omi-import --version
target/release/omi-mcp --version
```

Gli eseguibili sono `target/release/omi-import` e `target/release/omi-mcp`. Il database SQLite è incluso (feature `bundled`), quindi non serve
`libsqlite3`; il collegamento è dinamico solo con la libreria C di sistema (glibc). Per altre architetture: `cargo build --release --workspace --target aarch64-unknown-linux-gnu` (con il relativo linker).

### Test e controlli di qualità

```
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

La suite (identica su Windows e Linux) comprende test unitari, di integrazione (import su forniture sintetiche, strumenti, server HTTP con
un client rmcp reale, riga di comando dei binari), 40 risultati di riferimento ("golden") scritti a mano e controlli sul repository
(intestazioni di licenza, arrotondamenti, assenza di codici fiscali). I test marcati `#[ignore]` richiedono dati reali o misurano le prestazioni:

```
cargo test --release -p omi-core --test dati_reali -- --ignored --nocapture     # ZIP reali in dati-test/ (o in OMI_DATI_TEST)
OMI_PROVA_DB=percorso/omi.db cargo test --release -p omi-mcp --test prestazioni -- --ignored --nocapture
```

### Pacchetti di rilascio

| Sistema | Comando | Risultato |
|---|---|---|
| Windows | `.\deploy\windows\build-release.ps1` | `dist\OMIMyMCP-rs-<versione>-win-x64.zip` + `dist\SHA256SUMS.txt` (test, compilazione con target MSVC, pacchetto; `-SkipTests`, `-OutDir`, `-CertThumbprint` per la firma, `-WhatIf`) |
| Linux | `./deploy/linux/build-release.sh` | `dist/OMIMyMCP-rs-<versione>-linux-<arch>.tar.gz` (`<arch>` = `x64` o `arm64`, secondo la macchina o il `--target`) + `dist/SHA256SUMS-linux.txt` (`--skip-tests`, `--out-dir`, `--target`) |

## Versione compilata per Windows

La cartella [`release/windows-x64/`](release/windows-x64) contiene il pacchetto Windows x64 **già compilato**, pronto all'uso senza
installare Rust né Visual Studio:

| File | Contenuto |
|---|---|
| `omi-import.exe`, `omi-mcp.exe` | I due programmi |
| `deploy\windows\*.ps1` | Script di installazione, disinstallazione, aggiornamento dei dati, Caddy, prova e verifica residui |
| `deploy\caddy\` | Caddyfile di esempio e guida |
| `LEGGIMI.md` | Istruzioni passo passo del pacchetto |
| `MANUALE_TECNICO.md`, `LICENSE`, `NOTICE` | Manuale e avvisi di licenza |
| `SHA256SUMS.txt` | Hash SHA-256 degli eseguibili |

Uso in breve (PowerShell; l'installazione del servizio richiede PowerShell **come amministratore**):

```powershell
cd release\windows-x64
Get-FileHash .\omi-import.exe, .\omi-mcp.exe          # confrontare con SHA256SUMS.txt
.\omi-import.exe 'C:\DatiOMI\QI*.zip' 'C:\ProgramData\OMIMyMCP\omi.db'
.\omi-mcp.exe 'C:\ProgramData\OMIMyMCP\omi.db' 8765   # prova da console
Set-ExecutionPolicy -Scope Process Bypass
.\deploy\windows\install.ps1 -SourceDir .\ -Log -AddToPath    # servizio Windows OMIMyMCP
```

Gli eseguibili non sono firmati: SmartScreen può mostrare un avviso al primo avvio. Le istruzioni complete sono in
[`release/windows-x64/LEGGIMI.md`](release/windows-x64/LEGGIMI.md).

## Caricare i dati con omi-import

### Sintassi

```
omi-import [QI*.zip ...] [database.db]
omi-import --help | -h | /?
omi-import --version
```

Tutti i parametri sono facoltativi e possono essere indicati in qualunque ordine: ciò che non è uno ZIP `QI*` è il database.

| Parametro | Significato |
|---|---|
| `QI*.zip` / `QIP*.zip` | Fornitura OMI da caricare. Si possono indicare più file e usare i caratteri jolly `*` e `?` (li espande `omi-import`, quindi funzionano anche in PowerShell e nel Prompt dei comandi). Il nome deve iniziare con `QI` |
| `*.db` | Database SQLite (default `omi.db` nella cartella corrente). Se non esiste viene creato |
| `--version` | Mostra nome, versione e autore |

Senza alcun parametro carica tutti i `QI*.zip` / `QIP*.zip` della cartella corrente in `omi.db`.

| Codice d'uscita | Significato |
|---|---|
| 0 | Caricamento riuscito |
| 1 | Parametri non validi o errore grave (database non apribile, nessun file corrispondente) |
| 2 | Almeno un file non è stato caricato (ZIP danneggiato, semestre non determinabile, ...): gli altri sono stati caricati |

### Primo caricamento

Aprire un terminale nella cartella con gli ZIP scaricati e lanciare `omi-import` **senza parametri**:

```
cd C:\DatiOMI
C:\percorso\omi-import.exe                  # Windows
```
```
cd ~/DatiOMI
/percorso/omi-import                        # Linux
```

Vengono caricati in `omi.db`, nella stessa cartella, tutti i file `QI*.zip` e `QIP*.zip`, in ordine di
nome. Un semestre nazionale con perimetri richiede circa 5 secondi, uno con le sole quotazioni meno di 1 secondo: l'intera serie
2004–2025 si carica in pochi minuti. Un file danneggiato viene segnalato e saltato senza interrompere gli altri.
Esempio di output:

```
OMIMyMCP 1.0.0 di Explobot (licenza Apache-2.0)
Fonte dei dati: Agenzia Entrate - OMI
Database: C:\DatiOMI\omi.db
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
(Esempio con forniture sintetiche di prova; con le forniture reali compaiono anche le righe di avanzamento `1000/7887 file KML...` ogni 1000 file.)

### Aggiungere un nuovo semestre

Scaricare la fornitura del nuovo semestre e caricarla nel database esistente:

```
omi-import QIP<n. richiesta>_<codice fiscale>.zip omi.db
```

Si possono indicare più file nello stesso comando.

### Ricaricare o correggere dati

Il caricamento **sostituisce**, per il semestre del file, i dati dei comuni che il file contiene:

- una fornitura **nazionale** sostituisce l'intero semestre (utile se l'Agenzia delle Entrate ripubblica dati corretti);
- forniture di **regioni, province o comuni** diversi dello stesso semestre si sommano;
- ricaricare lo stesso file non crea duplicati;
- i CSV sostituiscono zone e quotazioni, i KML i perimetri: un file `QI` senza perimetri non cancella i
  perimetri già caricati per quel semestre;
- ogni file è caricato in una sola transazione: se fallisce, il database resta com'era prima di quel file.

I comuni presenti nel database ma assenti dalla nuova fornitura (per esempio soppressi) non vengono
cancellati. Per ripartire da zero basta eliminare `omi.db` (con `omi.db-wal` e `omi.db-shm`, se presenti) e ricaricare gli ZIP.
Il server può restare attivo durante un import (il database usa il journal WAL), ma per i caricamenti nazionali è più prudente fermarlo:
lo fanno `aggiorna-dati.ps1` e `aggiorna-dati.sh`.

### Verifica

Avviando il server (`omi-mcp omi.db 8765`) lo strumento `elenco_semestri` mostra i semestri caricati con il
numero di zone, quotazioni e perimetri. In alternativa, con `sqlite3`:

```
sqlite3 -readonly omi.db "SELECT semestre, n_zone, n_valori, n_geometrie FROM semestre ORDER BY semestre;"
```

`omi-import` stampa lo stesso riepilogo del contenuto a fine caricamento, e `omi-mcp` lo stampa all'avvio.

## Uso del server omi-mcp

```
omi-mcp [database.db] [porta] [/l] [--bind INDIRIZZO] [--trust-proxy]
        [--token SEGRETO | --token-file FILE] [--allowed-host HOST ...]
```

| Opzione | Significato |
|---|---|
| `database.db` | Database creato con `omi-import` (default `omi.db`). Aperto **in sola lettura**, mai creato |
| `porta` | Porta TCP (default 8765; `0` = scelta dal sistema e stampata a video) |
| `/l`, `-l`, `--log` | Abilita il [log del server](#log-del-server) |
| `--bind INDIRIZZO` | Indirizzo di ascolto. Default: tutte le interfacce (IPv4 e IPv6). Dietro Caddy: `--bind 127.0.0.1` |
| `--trust-proxy` | Nel log usa l'IP indicato da `X-Forwarded-For` (solo dietro un proxy affidabile) |
| `--token SEGRETO`, `--token-file FILE` | Richiede `Authorization: Bearer <segreto>` su `/mcp`. Preferibile il file; in alternativa la variabile `OMI_MCP_TOKEN` |
| `--allowed-host HOST` | Accetta su `/mcp` solo le richieste con questo `Host` (ripetibile) |

Endpoint: `/mcp` (MCP su HTTP *streamable*), `/` (informazioni), `/health` (`ok` se il database risponde). Ctrl+C (o `SIGTERM` su Linux) lo ferma.

```
omi-mcp                                    # omi.db nella cartella corrente, porta 8765
omi-mcp omi.db 8765                        # solo server MCP sulla porta 8765
omi-mcp omi.db 8765 /l                     # con log attivo
omi-mcp omi.db 8765 --bind 127.0.0.1       # solo accessi dalla stessa macchina
omi-mcp omi.db 8765 --token-file token.txt # con token Bearer
```

Senza `--bind` il server accetta connessioni su tutte le interfacce di rete (su Windows il firewall può chiedere
l'autorizzazione). Senza `--token` non prevede autenticazione: espone solo strumenti di sola lettura.
Su Windows i programmi si chiamano `omi-import.exe` e `omi-mcp.exe`.

### Log del server

Con `/l` il server registra ogni richiesta ricevuta. Le righe vengono mostrate sulla console e aggiunte al
file di log, che ha lo stesso nome e la stessa cartella del database con estensione `.log` (ad esempio
`omi.db` → `omi.log`). Il file non viene mai svuotato: per ricominciare basta cancellarlo.

Per ogni richiesta vengono scritte due righe:

- **`HTTP`**: indirizzo IP del client, metodo, percorso, codice di risposta e durata;
- **`TOOL`**: per ogni chiamata a uno strumento MCP, nome dello strumento, argomenti ricevuti, esito
  (`ok` con la dimensione della risposta in caratteri, oppure `ERRORE` con il messaggio restituito) e durata.

```
2026-09-27 09:20:53.747 TOOL cerca_comuni {"testo":"Asti"} -> ok, 215 caratteri (39 ms)
2026-09-27 09:20:53.766 HTTP ::1 POST /mcp -> 200 (185 ms)
2026-09-27 09:20:53.811 TOOL zona_da_coordinate {"latitudine":44.9,"longitudine":8.2} -> ok, 3076 caratteri (31 ms)
2026-09-27 09:20:53.822 TOOL dettaglio_zona {"zona":"R1","comune":"samone"} -> ERRORE: 'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754] (4 ms)
```

Il log è utile per vedere quali strumenti usa Claude e con quali argomenti, per capire perché una
risposta è vuota o sbagliata e per controllare chi accede a un server esposto su Internet. Registra gli
indirizzi IP dei client e il testo delle richieste: se il server è usato da altre persone, tenerne conto
ai fini della privacy. Il token e i nomi originali degli ZIP non compaiono mai.

> In **Git Bash** gli argomenti che iniziano con `/` vengono convertiti in percorsi Windows: usare `-l`
> al posto di `/l`. Nel Prompt dei comandi e in PowerShell `/l` funziona normalmente.

## Installazione come servizio

Per avere il server sempre attivo, avviato con il sistema. In entrambi i casi il database deve esistere già
(vedi [Caricare i dati](#caricare-i-dati-con-omi-import)), il server ascolta per default solo su `127.0.0.1` e tutti gli script
hanno una modalità di prova (`-WhatIf` su Windows, `--dry-run` su Linux) che mostra le azioni senza eseguirle.

### Windows (servizio Windows)

Da **PowerShell aperto come amministratore**, dalla radice del progetto o del [pacchetto compilato](#versione-compilata-per-windows)
(`-SourceDir` è la cartella con `omi-mcp.exe` e `omi-import.exe`: `.\` nel pacchetto, `target\release` dopo la compilazione):

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\deploy\windows\install.ps1 -SourceDir .\ -Log -AddToPath
```

Lo script copia i programmi in `C:\Program Files\OMIMyMCP`, usa i dati in `C:\ProgramData\OMIMyMCP` (database `omi.db`, log `omi.log`, token),
installa il servizio `OMIMyMCP` (avvio automatico, account LocalService, riavvio dopo 5, 30 e 60 secondi in caso di errore, permesso di modifica
sulla cartella dei dati per il journal WAL) e verifica `/health`. È idempotente: rilanciato con nuovi binari li aggiorna conservando dati e token.

| Opzione di `install.ps1` | Significato (predefinito) |
|---|---|
| `-SourceDir` (obbligatorio) | Cartella con `omi-mcp.exe` e `omi-import.exe` |
| `-InstallDir`, `-DataDir` | `C:\Program Files\OMIMyMCP`, `C:\ProgramData\OMIMyMCP` |
| `-Port`, `-Bind` | 8765, `127.0.0.1` |
| `-ServiceName` | `OMIMyMCP` |
| `-Log` | Registra richieste HTTP e chiamate ai tool in `omi.log` |
| `-WithToken` | Genera `token.txt` (leggibile solo da Administrators e dal servizio) e richiede il token Bearer |
| `-Firewall` | Aggiunge la regola in ingresso nel firewall di Windows |
| `-TrustProxy` | Per l'uso dietro Caddy (con `-Bind 127.0.0.1` e senza `-Firewall`) |
| `-AddToPath` | Aggiunge la cartella dei programmi al `PATH` di sistema |

```powershell
.\deploy\windows\install.ps1 -SourceDir .\                                       # solo PC locale
.\deploy\windows\install.ps1 -SourceDir .\ -Bind 0.0.0.0 -WithToken -Firewall -Log   # rete locale, con token
.\deploy\windows\install.ps1 -SourceDir .\ -TrustProxy -Log                      # dietro Caddy
```

Gestione, aggiornamento dei dati e disinstallazione:

```powershell
omi-mcp.exe service status | stop | start      # anche: sc.exe query OMIMyMCP, services.msc
.\deploy\windows\aggiorna-dati.ps1 -ZipDir C:\DatiOMI    # ferma il servizio, importa i QI*.zip, riavvia
.\deploy\windows\uninstall.ps1                           # servizio, firewall, programmi, PATH (i dati restano)
.\deploy\windows\uninstall.ps1 -RemoveCaddy -RemoveData  # anche Caddy e i dati (con conferma; -Force la salta)
.\deploy\windows\verifica-residui.ps1                    # controllo finale di sola lettura
.\deploy\windows\test-servizio.ps1                       # prova di installazione con un servizio di prova
```

Il servizio scrive sempre il suo ciclo di vita in `C:\ProgramData\OMIMyMCP\omi.log`; il codice d'uscita del servizio indica la causa
(1 database mancante, 2 porta non disponibile, 3 altri errori). Senza script: `omi-mcp.exe service install --db ... --port 8765 --log --now`
(opzioni: `--name`, `--bind`, `--token-file`, `--start auto|delayed|manual`, `--account localservice|networkservice|localsystem`, `--firewall`, `--no-acl`, `--trust-proxy`).

### Linux (systemd)

Come root (`sudo`), dalla radice del progetto o del pacchetto `.tar.gz`. Su Linux `omi-mcp service ...` non esiste: l'unità
systemd è creata da `install.sh`.

```bash
# 1. Crea utente di sistema "omi", cartelle e copia i programmi; si ferma avvisando che manca il database
sudo ./deploy/linux/install.sh --source-dir target/release
# 2. Carica il database come utente del servizio (gli ZIP devono essere leggibili da "omi")
sudo -u omi /opt/omimymcp/omi-import /var/lib/omimymcp/omi.db /tmp/dati-omi/QI*.zip
# 3. Installa e avvia il servizio
sudo ./deploy/linux/install.sh --source-dir target/release --log --with-token --add-to-path
```

Lo script crea l'utente `omi` (senza shell), `/opt/omimymcp` (programmi), `/var/lib/omimymcp` (dati: `omi.db`, `omi.log`, `token.txt`, di proprietà di `omi`),
scrive `/etc/systemd/system/omi-mcp.service` (`ProtectSystem=strict` con la sola cartella dei dati scrivibile, `NoNewPrivileges`, `PrivateTmp`, `ProtectHome`,
`Restart=on-failure`), esegue `enable` e `restart` e verifica `/health`. È idempotente: rilanciato con nuovi binari o nuove opzioni li aggiorna conservando dati e token.

| Opzione di `install.sh` | Significato (predefinito) |
|---|---|
| `--source-dir` (obbligatoria) | Cartella con `omi-mcp` e `omi-import` |
| `--install-dir`, `--data-dir` | `/opt/omimymcp`, `/var/lib/omimymcp` |
| `--port`, `--bind` | 8765, `127.0.0.1` |
| `--service-name`, `--user` | `omi-mcp`, `omi` |
| `--log` | Registra richieste HTTP e chiamate ai tool in `omi.log` |
| `--with-token` | Genera `token.txt` (proprietario `omi`, permessi 600) e richiede il token Bearer |
| `--trust-proxy` | Per l'uso dietro Caddy (con `--bind 127.0.0.1`) |
| `--allowed-host HOST` | Accetta solo questo `Host` su `/mcp` (ripetibile) |
| `--add-to-path` | Collegamenti `omi-mcp` e `omi-import` in `/usr/local/bin` |
| `--dry-run` | Mostra le azioni senza eseguirle |

```bash
sudo ./deploy/linux/install.sh --source-dir target/release                                   # solo macchina locale
sudo ./deploy/linux/install.sh --source-dir target/release --bind 0.0.0.0 --with-token --log  # rete locale (aprire la porta nel firewall)
sudo ./deploy/linux/install.sh --source-dir target/release --trust-proxy --log                # dietro Caddy

systemctl status omi-mcp
sudo systemctl stop omi-mcp                # SIGTERM: arresto ordinato
sudo systemctl start omi-mcp
journalctl -u omi-mcp -n 50 --no-pager     # ciclo di vita ed errori
tail -f /var/lib/omimymcp/omi.log          # con --log: righe HTTP e TOOL
curl http://127.0.0.1:8765/health

sudo ./deploy/linux/aggiorna-dati.sh --zip-dir ~/DatiOMI      # ferma il servizio, importa come "omi", riavvia
sudo ./deploy/linux/uninstall.sh                              # unità, collegamenti, programmi (i dati restano)
sudo ./deploy/linux/uninstall.sh --remove-caddy --remove-data # anche Caddy e dati/utente (con conferma; --force la salta)
sudo ./deploy/linux/test-servizio.sh --db /percorso/omi.db    # prova di installazione con un servizio di prova
```

Installazione a mano, senza script: `deploy/linux/omi-mcp.service` è un'unità di esempio con i comandi di installazione nell'intestazione.
Gli script `.sh` richiedono il bit di esecuzione e fine riga LF (`chmod +x deploy/linux/*.sh` se si perde dopo una copia).

## HTTPS con Caddy

Caddy è **facoltativo** ed esterno al programma: è necessario solo quando il server deve essere raggiungibile da Internet in HTTPS, ad esempio dal
connettore personalizzato di claude.ai. Accetta HTTPS sulla 443 e inoltra a `omi-mcp` su `127.0.0.1:8765`.

Prerequisiti: un nome DNS pubblico che punta alla macchina, porte TCP 80 e 443 raggiungibili (la 80 serve al challenge HTTP-01 di Let's Encrypt) e **nessuna** regola
di firewall per la 8765. Il servizio OMIMyMCP va installato con `--bind 127.0.0.1 --trust-proxy` (default di bind già locale). I due script non scaricano nulla.

**Windows** (PowerShell come amministratore; `caddy.exe` scaricato da <https://caddyserver.com/download>):

```powershell
.\deploy\windows\install.ps1 -SourceDir .\ -TrustProxy -Log
.\deploy\windows\install-caddy.ps1 -CaddyExe C:\Download\caddy.exe -Dominio omi.esempio.it -Email nome@esempio.it
```

Crea il servizio `caddy` (dipendente da `OMIMyMCP`), scrive `C:\ProgramData\Caddy\Caddyfile`, apre TCP 80 e 443 e verifica `https://<dominio>/health`.
Rimozione: `.\deploy\windows\uninstall.ps1 -RemoveCaddy [-RemoveData]`.

**Linux** (Caddy dal gestore di pacchetti, <https://caddyserver.com/docs/install>, oppure il binario scaricato con `--caddy-bin`):

```bash
sudo ./deploy/linux/install.sh --source-dir target/release --trust-proxy --log
sudo ./deploy/linux/install-caddy.sh --domain omi.esempio.it --email nome@esempio.it
# oppure con il binario scaricato:
sudo ./deploy/linux/install-caddy.sh --caddy-bin ~/Download/caddy --domain omi.esempio.it --email nome@esempio.it
```

Scrive `/etc/caddy/Caddyfile` (certificati in `/var/lib/caddy/data`, log di accesso in `/var/log/caddy`), valida la configurazione, apre 80 e 443 in
ufw/firewalld se attivi, abilita `caddy` e verifica `https://<dominio>/health`. Rimozione: `sudo ./deploy/linux/uninstall.sh --remove-caddy`.

I Caddyfile di esempio sono in [`deploy/caddy/`](deploy/caddy): `Caddyfile.internet.example` (dominio pubblico, Let's Encrypt, richieste limitate a 1 MB,
`flush_interval -1` per lo streaming, log di accesso con rotazione, restrizione facoltativa per IP) e `Caddyfile.lan.example` (rete locale con `tls internal`;
non funziona con claude.ai). La guida [`deploy/caddy/LEGGIMI.md`](deploy/caddy/LEGGIMI.md) e i capitoli 8 e 12 del [manuale tecnico](docs/MANUALE_TECNICO.md)
descrivono requisiti di rete, rinnovo dei certificati, il tunnel `cloudflared` per le prove (`cloudflared tunnel --url http://localhost:8765`) e le protezioni
compatibili con claude.ai (restrizione per IP `160.79.104.0/21` e, dove disponibile, token Bearer nelle intestazioni).

## Collegamento a Claude come server MCP

Il server espone il protocollo MCP su HTTP all'indirizzo `http://<host>:<porta>/mcp`.

### claude.ai (web, app desktop e mobile, Cowork)

I *connettori personalizzati* di claude.ai vengono contattati dall'infrastruttura di Anthropic, quindi il
server deve essere raggiungibile **da Internet tramite HTTPS** con un certificato valido; `localhost` non funziona.

1. **Rendere il server raggiungibile via HTTPS**, ad esempio:
   - con Caddy (sezione [HTTPS con Caddy](#https-con-caddy)) su un server/VPS o su una macchina con porte 80/443 raggiungibili;
   - per prove, con un tunnel temporaneo:
     ```
     omi-mcp omi.db 8765
     cloudflared tunnel --url http://localhost:8765
     ```
     che restituisce un indirizzo del tipo `https://nome-casuale.trycloudflare.com`.
2. In claude.ai aprire **Customize → Connectors** (<https://claude.ai/customize/connectors>), premere **+ Add** e **Add custom connector**
   (nei piani Team/Enterprise lo aggiunge un Owner da **Organization settings → Connectors**). I nomi dei menu cambiano: cercare "connettori personalizzati".
3. Indicare un nome (es. `Quotazioni OMI`) e l'URL completo dell'endpoint, ad esempio
   `https://omi.esempio.it/mcp` o `https://nome-casuale.trycloudflare.com/mcp`. Il server non implementa OAuth: come autenticazione scegliere **No sign in**.
   Con il token Bearer, se la finestra offre la sezione *Request headers*, aggiungere `Authorization` = `Bearer <token>`.
4. In una conversazione attivare il connettore dal menu degli strumenti (pulsante **+** → *Connectors*).

I connettori aggiunti al proprio account sono disponibili anche nell'app desktop e in **Cowork**.

> Senza token chiunque conosca l'URL pubblico può interrogare il server. I dati esposti sono
> di sola lettura, ma è comunque opportuno non lasciare tunnel aperti inutilmente e, per un'installazione
> stabile, proteggere l'accesso (restrizione degli IP sul reverse proxy, token).

### Claude Desktop con server locale

In alternativa, senza esporre il server su Internet, Claude Desktop può usare il server locale tramite
il bridge [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) (richiede Node.js). In
**Settings → Developer → Edit Config** (`claude_desktop_config.json`):

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

Con il token aggiungere agli argomenti `"--header", "Authorization: Bearer <token>"`. Riavviare Claude Desktop con il server già in
esecuzione (da console o come servizio).

### Claude Code

```
claude mcp add --transport http omi http://localhost:8765/mcp
claude mcp add --transport http omi http://localhost:8765/mcp --header "Authorization: Bearer <token>"   # con il token
claude mcp list          # "omi" deve risultare connesso
```

Per consentire in modo permanente gli strumenti (sono tutti di sola lettura) aggiungere ai permessi `"mcp__omi__*"`.

Una guida più dettagliata (permessi, problemi tipici, MCP Inspector per provare gli strumenti senza Claude) è nel capitolo 9 del [manuale tecnico](docs/MANUALE_TECNICO.md).

## Uso combinato con MCP Cruscotto Italia

OMIMyMCP diventa molto più utile se usato **insieme al server MCP ufficiale
[MCP Cruscotto Italia](https://www.dati.gov.it/sviluppatori/mcp-cruscotto-italia)** di AgID.
[Cruscotto Italia](https://cruscotto-italia.dati.gov.it/) raccoglie per ogni Comune italiano i dati
pubblici di oltre 20 fonti istituzionali (ISTAT, MEF, ANAC, ISPRA, Italia Domani/PNRR, Ministero della
Salute, Agenzia delle Entrate e altre) e li espone tramite un endpoint MCP pubblico e gratuito.

Con entrambi i connettori attivi Claude può incrociare le quotazioni immobiliari OMI con il contesto del
comune, ad esempio:

- **indirizzi → coordinate**: lo stradario georeferenziato ANNCSU (numeri civici con coordinate) consente
  di ricavare con precisione latitudine e longitudine di un indirizzo, da passare poi a `zona_da_coordinate`;
- **demografia e censimento**: popolazione, età, famiglie, abitazioni occupate e vuote;
- **redditi IRPEF** su base comunale, per confrontare prezzi e affitti con la capacità di spesa;
- **territorio e rischi**: consumo di suolo, rischio idrogeologico, classificazione sismica, qualità dell'aria;
- **servizi e investimenti**: scuole, farmacie, ospedali, colonnine di ricarica, banda ultralarga, beni
  culturali, progetti PNRR e opere pubbliche.

### Collegamento

L'endpoint è `https://cruscotto-italia-mcp.agid.workers.dev/mcp` e non richiede un server locale.

- **claude.ai / Claude Desktop / Cowork**: aggiungere un secondo connettore personalizzato
  (**Customize → Connectors → Add custom connector**) con l'URL dell'endpoint.
- **Claude Code**:
  ```
  claude mcp add --transport http cruscotto-italia https://cruscotto-italia-mcp.agid.workers.dev/mcp
  ```

Le istruzioni ufficiali sono nella pagina
[Accesso MCP di Cruscotto Italia](https://cruscotto-italia.dati.gov.it/about.html#accesso-mcp). Per risposte
più mirate AgID mette a disposizione anche una skill opzionale per Claude, scaricabile dalla
[pagina di presentazione](https://www.dati.gov.it/sviluppatori/mcp-cruscotto-italia).

### Esempi di domande combinate

> Qual è la zona OMI di via Nizza 150 a Torino? Usa lo stradario ANNCSU per le coordinate e dammi le
> quotazioni delle abitazioni civili.

> Confronta i comuni della provincia di Monza e Brianza: valore medio delle abitazioni civili
> nell'ultimo semestre, reddito IRPEF medio e popolazione. Quali hanno il rapporto prezzo/reddito più alto?

> Per i capoluoghi dell'Emilia-Romagna metti in relazione l'andamento dei prezzi delle abitazioni dal
> 2016 a oggi con la variazione della popolazione e il rischio idrogeologico.

I dati di Cruscotto Italia hanno licenze proprie (in prevalenza CC-BY 4.0, indicate per ciascuna fonte):
nei risultati vanno citate sia le fonti di Cruscotto Italia sia **"Agenzia delle Entrate - OMI"**.

## Esempi di utilizzo con Claude Cowork

Con il connettore attivo si può chiedere a Claude, in linguaggio naturale, di interrogare i dati e
di produrre documenti, fogli di calcolo o grafici. Il server non converte gli indirizzi in coordinate:
quando si indica una via, Claude ne ricava latitudine e longitudine e usa `zona_da_coordinate`
(per indirizzi precisi conviene fornire direttamente le coordinate o verificare la zona restituita, oppure
attivare anche [MCP Cruscotto Italia](#uso-combinato-con-mcp-cruscotto-italia), che dispone dello
stradario georeferenziato ANNCSU).

**Valutazione di un immobile**
> Ho un appartamento di 95 m² in buono stato in via Nizza 150 a Torino. In che zona OMI si trova e
> qual è l'intervallo di valore di compravendita e di affitto mensile secondo l'ultimo semestre?

Artefatto ottenuto: https://claude.ai/artifact/44hMgy8mT8SxEGdvYEp7je

**Confronto tra zone di un comune**
> Elenca le zone OMI di Bergamo con le quotazioni delle abitazioni civili in stato normale e prepara
> un foglio Excel ordinato dalla più cara alla più economica, con una colonna per il valore medio.

**Serie storica e grafico**
> Mostrami l'andamento dal 2004 a oggi delle quotazioni delle abitazioni civili nella zona B12 di Milano,
> con un grafico del valore medio di compravendita e di locazione. Tieni conto che la zonizzazione è cambiata
> nel tempo e segnalami i semestri in cui cambia la descrizione della zona.

Artefatto ottenuto: https://claude.ai/artifact/6dU1DpCpQuSCt7ZWAcikPb

**Andamento degli immobili commerciali**
> Mi indichi l'andamento degli ultimi 10 anni dei prezzi di acquisto e di affitto degli immobili
> commerciali del centro di Monza?

Artefatto ottenuto: https://claude.ai/artifact/7GQT5u1kKHuHSh8LmY5d2w

**Analisi territoriale**
> Quali zone OMI si trovano entro 1,5 km da Piazza del Campo a Siena? Per ognuna dammi fascia,
> descrizione e quotazioni di negozi e abitazioni civili.

**Rendimento da locazione**
> Per le zone centrali (fascia B) di Bologna calcola il rendimento lordo annuo da affitto delle
> abitazioni civili (canone medio × 12 / valore medio di compravendita) e crea un breve report in Word.

**Simulazione di un investimento**
> Se avessi investito 100.000 euro nel 2014 in immobili da destinare all'affitto, non considerando tasse e
> costi connessi con l'acquisto, tasse sul reddito, tasse sugli immobili e spese straordinarie, mi mostri
> i grafici di quanto avrei guadagnato per ciascuna zona di Roma, distinguendo l'incasso per gli affitti e
> l'aumento del valore dell'immobile?
> Al termine mi aggiungi considerazioni sugli eventuali costi come tasse di acquisto, sul reddito e
> sull'immobile, spese straordinarie e simili?

Artefatto ottenuto: https://claude.ai/artifact/45mznWXmn6k4CH2uUFPsxo

**Confronto tra città**
> Confronta Firenze, Pisa e Livorno nel 2° semestre 2025: valore medio al m² per fascia e per tipologia
> residenziale. Riassumi le differenze in una tabella.

**Mappa**
> Recupera il perimetro in GeoJSON della zona C3 di Padova e di quelle confinanti e prepara una pagina
> HTML con una mappa Leaflet colorata in base al valore delle abitazioni civili.

Nei risultati prodotti con questi dati va sempre citata la fonte **"Agenzia delle Entrate - OMI"** (vedi sotto).

## Strumenti MCP

| Tool | Descrizione |
|---|---|
| `elenco_semestri` | semestri disponibili con numero di zone, quotazioni e perimetri |
| `elenco_tipologie` | tipologie edilizie (codice e descrizione) |
| `cerca_comuni` | ricerca per nome (anche parziale, accenti ignorati), codice catastale o ISTAT |
| `zone_comune` | zone di un comune in un semestre, opzionalmente con quotazioni e perimetri |
| `dettaglio_zona` | anagrafica, quotazioni e perimetro (anche GeoJSON) di una zona |
| `zona_da_coordinate` | zona che contiene un punto lat/lon con tutti i dati (o la più vicina entro 2 km) |
| `zone_vicine` | zone entro un raggio da un punto, con le quotazioni delle abitazioni civili |
| `storico_zona` | serie storica delle quotazioni di una zona |
| `statistiche_comune` | minimi, massimi e medi per tipologia e per fascia di un comune |

Tutti gli strumenti sono di sola lettura (`readOnlyHint`). Il semestre si indica come `AAAA/S` (es. `2025/2`; accettati anche `2025-2` e `20252`); se omesso si usa l'ultimo caricato.
Per i semestri senza perimetri (2004/1 – 2010/1) la ricerca per coordinate usa i perimetri del semestre
più vicino, collegati per codice catastale + codice zona; se nel frattempo la zona è stata rinominata la
risposta lo segnala. Parametri, esempi di chiamata e risposta sono nel capitolo 10 del [manuale tecnico](docs/MANUALE_TECNICO.md).

## Struttura del database

### Contenuto degli ZIP originali

* `..._AAAAS_ZONE.csv` – una riga per zona OMI: comune (codice catastale/Belfiore, ISTAT), fascia, codice e
  descrizione zona, `LinkZona`, tipologia e stato prevalenti, microzona.
* `..._AAAAS_VALORI.csv` – quotazioni per zona (`LinkZona`), tipologia e stato conservativo:
  compravendita €/m² min/max, locazione €/m²·mese min/max, superficie lorda/netta.
* `*.kml` (solo nei `QIP*`) – un file per comune con i poligoni delle zone.

### Tabelle

`omi.db` è un database SQLite in modalità WAL con il modulo R\*Tree. `omi-import` lo crea e lo scrive; `omi-mcp` lo apre in sola lettura.

| Tabella | Contenuto |
|---|---|
| `semestre` | semestri caricati (codice `AAAAS`, es. 20252), data di elaborazione, file sorgente, conteggi |
| `zona` | tutte le colonne di `ZONE.csv`, chiave `(semestre, link_zona)` |
| `valore` | tutte le colonne di `VALORI.csv`, chiave `(semestre, link_zona, cod_tip, stato)`; importi numerici |
| `tipologia` | codice e descrizione delle tipologie edilizie |
| `geometria` | poligoni delle zone (codifica binaria compatta), bbox, centroide, area; deduplicati tra semestri |
| `geometria_rt` | indice spaziale R*Tree sui bbox |
| `zona_geo` | semestre + comune + zona → geometria (con LinkZona e colore del KML) |
| `comune` | anagrafica comuni (dati dell'ultimo semestre) con primo/ultimo semestre disponibile |
| `comune_nome` | nomi comune (anche storici) normalizzati per la ricerca |
| `file_caricato` | registro dei file importati (senza codice fiscale) |

La geometria è in `geometria.dati`: varint zigzag delle differenze delle coordinate in microgradi WGS84, con hash SHA-256 troncato a 16 byte per la
deduplicazione (vedi `crates/omi-core/src/geometry.rs` e il capitolo 11 del manuale tecnico, che riporta anche il DDL completo).

Esempio di query diretta:

```sql
SELECT z.comune_descr, z.zona, z.zona_descr, v.compr_min, v.compr_max, v.loc_min, v.loc_max
FROM zona z JOIN valore v ON v.semestre = z.semestre AND v.link_zona = z.link_zona
WHERE z.semestre = 20252 AND z.comune_amm = 'F205' AND v.cod_tip = 20 AND v.stato_prev = 'P'
ORDER BY v.compr_max DESC;
```

## Sicurezza

- `omi-mcp` apre il database in sola lettura (`query_only=1`) e non espone rotte di scrittura; le richieste sono limitate a 1 MiB e hanno un timeout di 60 secondi.
- Per default il servizio ascolta solo su `127.0.0.1`. Esposizioni più ampie vanno decise consapevolmente:

| Esposizione | Protezioni consigliate |
|---|---|
| Solo macchina locale (`--bind 127.0.0.1`) | Nessuna aggiuntiva |
| Rete locale (`--bind 0.0.0.0`) | Token (`--token-file`), regola di firewall limitata alla rete locale, eventualmente HTTPS con `tls internal` |
| Internet (connettore di claude.ai) | Dietro Caddy con HTTPS; `omi-mcp` su `127.0.0.1`; restrizione per IP (`160.79.104.0/21`); token se disponibile; nessuna regola di firewall sulla 8765 |

- Il token è confrontato a tempo costante, non compare mai nei log e va tenuto in un file con permessi ristretti (`--token-file`, non `--token`).
- `--allowed-host` protegge dagli attacchi di tipo *DNS rebinding*; `--trust-proxy` va usato solo dietro un proxy affidabile con `--bind 127.0.0.1`.
- Il log registra IP e parametri delle richieste: trattarlo come dato personale.
- Tenere gli ZIP scaricati e i database fuori dai repository e dalle cartelle condivise (il nome degli ZIP contiene un codice fiscale).

Dettagli nel capitolo 12 del [manuale tecnico](docs/MANUALE_TECNICO.md).

## Risoluzione dei problemi

| Sintomo | Soluzione |
|---|---|
| `Database non trovato: omi.db. Crearlo con omi-import.` | Indicare il percorso completo del database o caricare prima gli ZIP |
| `impossibile mettersi in ascolto sulla porta 8765` | Porta già in uso: scegliere un'altra porta (`omi-mcp omi.db 8766`) o fermare l'altro programma |
| Il servizio parte e si ferma subito | Leggere `omi.log` (Windows: `C:\ProgramData\OMIMyMCP`; Linux: `journalctl -u omi-mcp` e `/var/lib/omimymcp/omi.log`): database mancante, porta occupata o cartella dei dati non scrivibile dall'account del servizio (il WAL richiede `omi.db-wal` e `omi.db-shm`) |
| Script `.ps1` bloccati | `Set-ExecutionPolicy -Scope Process Bypass` nella finestra |
| Script `.sh` "Permission denied" | `chmod +x deploy/linux/*.sh` |
| Il connettore di claude.ai non si collega | HTTPS non valido (anche `tls internal`), DNS o porte 80/443 non raggiungibili, restrizione per IP troppo stretta: provare `https://<dominio>/health` da fuori rete |
| Errore 401 `Token mancante o non valido.` | Inviare `Authorization: Bearer <token>` (contenuto di `token.txt`, senza spazi finali) |
| `/l` non riconosciuto in Git Bash | Usare `-l` |
| Durante l'import: `ERRORE su …: errore ZIP` (codice 2) | ZIP danneggiato: scaricarlo di nuovo; gli altri file sono stati caricati |
| `Attenzione: C302.kml malformato …` | Caso noto del 1° semestre 2011: si caricano le zone leggibili, nessuna azione |

La tabella completa è nel capitolo 14 del [manuale tecnico](docs/MANUALE_TECNICO.md).

## Licenza e obblighi

### Codice sorgente

Copyright 2026 **Explobot** – <https://github.com/Explo-bot/OMIMyMCP>

Il codice di OMIMyMCP è rilasciato con licenza **[Apache License 2.0](LICENSE)**. È possibile usarlo,
modificarlo e ridistribuirlo liberamente, anche per scopi commerciali, alle seguenti condizioni:

- **Attribuzione obbligatoria**: chi ridistribuisce il software, in forma originale o modificata, deve
  includere il file [`NOTICE`](NOTICE) e riportarne il contenuto, cioè il **nome dell'autore originale**
  (Explobot) e il **link al progetto originale** (https://github.com/Explo-bot/OMIMyMCP), dove vengono normalmente mostrate le
  attribuzioni (documentazione, schermata "informazioni", file distribuiti);
- va inclusa una copia della licenza;
- nei file modificati va indicato chiaramente che sono stati modificati;
- le intestazioni di copyright presenti nei sorgenti vanno conservate.

Il server MCP riporta autore e link al progetto nelle informazioni che fornisce ai client
(`serverInfo`, istruzioni e risposta di `elenco_semestri`). L'elenco delle crate Rust di terze parti incluse negli eseguibili, con le relative licenze, è in [`NOTICE`](NOTICE).

Il progetto è stato sviluppato con Claude Code a partire dalla skill e dagli script del repository
<https://github.com/Explo-bot/OMIMyMCPScript>.

### Dati OMI

Il progetto **non include e non ridistribuisce dati OMI**: ogni utente li scarica direttamente
dall'Agenzia delle Entrate con le proprie credenziali. I dati (CSV e KML originali e il database che
OMIMyMCP ne ricava) **non** sono coperti dalla licenza del codice e restano soggetti alle
[Condizioni contrattuali e d'uso](https://www.agenziaentrate.gov.it/portale/schede/fabbricatiterreni/omi/banche-dati/quotazioni-immobiliari/condizioni-contrattuali-qi)
dell'Agenzia delle Entrate e alle indicazioni della pagina
[Forniture dati OMI](https://www.agenziaentrate.gov.it/portale/schede/fabbricatiterreni/omi/forniture-dati-omi).
L'Agenzia delle Entrate non associa ai dati una licenza aperta standard (es. Creative Commons). In sintesi:

- **Proprietà**: le informazioni sono di proprietà esclusiva dell'Agenzia delle Entrate.
- **Citazione obbligatoria**: chi usa o pubblica i dati, anche elaborati, deve citare la fonte
  **"Agenzia delle Entrate - OMI"**.
- **Elaborazione consentita**: i dati possono essere utilizzati anche per elaborazioni proprie,
  citando la fonte in caso di pubblicazione.
- **Divieti**: le condizioni non consentono di vendere, affittare, noleggiare, trasferire o cedere
  a terzi i contenuti della banca dati, né di assumere obbligazioni verso terzi su di essi.
  Per questo **il database prodotto da OMIMyMCP è a uso di chi ha scaricato i dati e non va condiviso
  né pubblicato**.
- **Nessuna garanzia**: l'Agenzia delle Entrate non risponde di danni derivanti dall'uso dei dati. Le quotazioni
  indicano valori di larga massima riferiti all'ordinarietà degli immobili e **non sostituiscono la
  stima** di un tecnico professionista.
- **Serie storiche**: l'Agenzia delle Entrate avverte che revisioni delle zone e miglioramenti dei metodi hanno
  prodotto salti di quotazione tra semestri; confrontare le serie storiche con cautela.

Esporre il server MCP a terzi (per esempio tramite un connettore pubblico) significa rendere loro
accessibili i dati: valutarlo alla luce delle condizioni dell'Agenzia delle Entrate.

### Trasformazioni applicate durante il caricamento

Il database conserva tutte le informazioni dei file originali, con queste trasformazioni:

- importi convertiti da testo con virgola decimale a numeri; apici attorno alle descrizioni rimossi;
- perimetri convertiti dal formato KML a una codifica binaria, con deduplicazione dei poligoni identici
  tra semestri e calcolo di bbox, centroide e area (le descrizioni HTML dei placemark non sono conservate);
- per i KML che dichiarano UTF-8 ma contengono caratteri Latin-1 viene usata la decodifica Latin-1;
- i placemark KML malformati vengono scartati e segnalati a video (caso noto: 1° semestre 2011,
  file `C302.kml`, Castiglione Chiavarese, zona B1 priva di geometria);
- dai nomi dei file sorgente registrati viene rimosso il codice fiscale del richiedente.

Fonte dei dati: **Agenzia delle Entrate - OMI**.
