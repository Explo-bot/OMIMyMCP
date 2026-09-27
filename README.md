# OMIMyMCP

Importa le quotazioni immobiliari dell'**Osservatorio del Mercato Immobiliare (OMI)** dell'Agenzia delle Entrate
in un database SQLite e le rende interrogabili come **server MCP** da Claude (claude.ai, Claude Desktop,
Cowork, Claude Code) o da qualunque altro client MCP.

🇮🇹 Italiano · 🇬🇧 [English](README.en.md)

Autore: **Explobot** – progetto originale: <https://github.com/Explo-bot/OMIMyMCP> – licenza [Apache-2.0](LICENSE)

> ## ⚠️ AVVISO IMPORTANTE – LEGGERE PRIMA DELL'USO
>
> **Questo software è stato sviluppato interamente con Claude Code e il modello Claude Opus 5.5.**
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
- [Scaricare i dati dal sito dell'Agenzia delle Entrate](#scaricare-i-dati-dal-sito-dellagenzia-delle-entrate)
- [Scaricare molti file in modo massivo con Claude Chat](#scaricare-molti-file-in-modo-massivo-con-claude-chat)
- [Caricare i dati con OMIMyMCP](#caricare-i-dati-con-OMIMyMCP)
- [Uso del programma](#uso-del-programma)
- [Collegamento a Claude come server MCP](#collegamento-a-claude-come-server-mcp)
- [Uso combinato con MCP Cruscotto Italia](#uso-combinato-con-mcp-cruscotto-italia)
- [Esempi di utilizzo con Claude Cowork](#esempi-di-utilizzo-con-claude-cowork)
- [Strumenti MCP](#strumenti-mcp)
- [Struttura del database](#struttura-del-database)
- [Licenza e obblighi](#licenza-e-obblighi)

## Come funziona

Il progetto **non contiene né distribuisce dati OMI**. Ogni utente scarica gratuitamente dal sito
dell'Agenzia delle Entrate le forniture dei semestri che gli interessano e le carica con OMIMyMCP nel proprio
database SQLite locale:

```
Agenzia delle Entrate            OMIMyMCP                     Claude / client MCP
Forniture dati OMI  ──ZIP──►  caricamento  ──►  omi.db  ──►  server MCP (/mcp)
(QI*.zip, QIP*.zip)
```

Si può caricare un solo semestre o l'intera serie storica, l'intero territorio nazionale o solo alcune
regioni, province o comuni. I nuovi semestri si aggiungono man mano che l'Agenzia delle Entrate li pubblica.

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
> fatto la richiesta**. OMIMyMCP lo rimuove dai nomi registrati nel database, ma gli ZIP originali non vanno
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

## Caricare i dati con OMIMyMCP

### Primo caricamento

Aprire un terminale nella cartella con gli ZIP scaricati e lanciare OMIMyMCP (vedi [Compilazione](#compilazione))
**senza parametri**:

```
cd C:\DatiOMI
C:\percorso\OMIMyMCP.exe
```

Vengono caricati in `omi.db`, nella stessa cartella, tutti i file `QI*.zip` e `QIP*.zip`, in ordine di
nome. Un semestre nazionale con perimetri richiede circa 10 secondi, uno con le sole quotazioni circa
2 secondi: l'intera serie 2004–2025 si carica in 4–5 minuti. Un file danneggiato viene segnalato e
saltato senza interrompere gli altri.

### Aggiungere un nuovo semestre

Scaricare la fornitura del nuovo semestre e caricarla nel database esistente:

```
OMIMyMCP QIP<n. richiesta>_<codice fiscale>.zip omi.db
```

Si possono indicare più file nello stesso comando.

### Ricaricare o correggere dati

Il caricamento **sostituisce**, per il semestre del file, i dati dei comuni che il file contiene:

- una fornitura **nazionale** sostituisce l'intero semestre (utile se l'Agenzia delle Entrate ripubblica dati corretti);
- forniture di **regioni, province o comuni** diversi dello stesso semestre si sommano;
- ricaricare lo stesso file non crea duplicati;
- i CSV sostituiscono zone e quotazioni, i KML i perimetri: un file `QI` senza perimetri non cancella i
  perimetri già caricati per quel semestre.

I comuni presenti nel database ma assenti dalla nuova fornitura (per esempio soppressi) non vengono
cancellati. Per ripartire da zero basta eliminare `omi.db` e ricaricare gli ZIP.

### Verifica

Avviando il server (`OMIMyMCP omi.db 8765`) lo strumento `elenco_semestri` mostra i semestri caricati con il
numero di zone, quotazioni e perimetri. In alternativa, con `sqlite3`:

```
sqlite3 omi.db "SELECT semestre, n_zone, n_valori, n_geometrie FROM semestre ORDER BY semestre;"
```

## Uso del programma

```
OMIMyMCP [QI*.zip ...] [database.db] [porta] [/l]
```

Tutti i parametri sono facoltativi, possono essere indicati in qualunque ordine e vengono riconosciuti
dal tipo:

| Parametro | Significato |
|---|---|
| `QI*.zip` / `QIP*.zip` | Fornitura OMI da caricare (vedi [Ricaricare o correggere dati](#ricaricare-o-correggere-dati)). Si possono indicare più file. |
| `*.db` | Database SQLite (default `omi.db` nella cartella corrente). Se non esiste viene creato. |
| `0..65535` | Porta del server MCP. Al termine del caricamento il programma resta in ascolto su `http://host:porta/mcp` (HTTP *streamable*). Con `0` la porta viene scelta dal sistema e stampata a video. |
| `/l` | Abilita il [log del server](#log-del-server). Equivalenti: `-l`, `--log`. Ha effetto solo se è indicata una porta. |

Senza alcun parametro carica tutti i `QI*.zip` / `QIP*.zip` della cartella corrente. Se si indicano solo
database, porta e/o `/l` non viene caricato nulla.

```
OMIMyMCP                                              # carica tutti gli zip della cartella in omi.db
OMIMyMCP QIP1438117_XXXXXXXXXXXXXXXX.zip              # (ri)carica un semestre
OMIMyMCP QIP1438200_XXXXXXXXXXXXXXXX.zip omi.db 8765  # carica un semestre e avvia il server
OMIMyMCP omi.db 8765                                  # solo server MCP sulla porta 8765
OMIMyMCP omi.db 8765 /l                               # server MCP con log attivo
```

Il server accetta connessioni su tutte le interfacce di rete (su Windows il firewall può chiedere
l'autorizzazione). Non prevede autenticazione: espone solo strumenti di sola lettura.

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
ai fini della privacy.

> In **Git Bash** gli argomenti che iniziano con `/` vengono convertiti in percorsi Windows: usare `-l`
> al posto di `/l`. Nel Prompt dei comandi e in PowerShell `/l` funziona normalmente.

### Compilazione

Richiede [.NET 10 SDK](https://dotnet.microsoft.com/download).

```
cd OMIMyMCP
dotnet build -c Release
```

L'eseguibile è in `bin/Release/net10.0/OMIMyMCP.exe`. Per un eseguibile unico senza runtime installato:

```
dotnet publish -c Release -r win-x64 --self-contained -p:PublishSingleFile=true
```

(`linux-x64`, `osx-arm64`, … per altri sistemi).

## Collegamento a Claude come server MCP

Il server espone il protocollo MCP su HTTP all'indirizzo `http://<host>:<porta>/mcp`.

### claude.ai (web, app desktop e mobile, Cowork)

I *connettori personalizzati* di claude.ai vengono contattati dall'infrastruttura di Anthropic, quindi il
server deve essere raggiungibile **da Internet tramite HTTPS**; `localhost` non funziona.

1. **Rendere il server raggiungibile via HTTPS**, ad esempio:
   - su un server/VPS con un reverse proxy (Caddy, nginx, IIS) che termina TLS e inoltra a `http://localhost:8765`;
   - per prove, con un tunnel temporaneo:
     ```
     OMIMyMCP omi.db 8765
     cloudflared tunnel --url http://localhost:8765
     ```
     che restituisce un indirizzo del tipo `https://nome-casuale.trycloudflare.com`.
2. In claude.ai aprire **Impostazioni → Connettori → Aggiungi connettore personalizzato**
   (nei piani Team/Enterprise lo aggiunge un amministratore dalle impostazioni dell'organizzazione).
3. Indicare un nome (es. `Quotazioni OMI`) e l'URL completo dell'endpoint, ad esempio
   `https://nome-casuale.trycloudflare.com/mcp`. Non è richiesta autenticazione OAuth.
4. In una conversazione attivare il connettore dal menu degli strumenti (pulsante **+** / *Connettori*).

I connettori aggiunti al proprio account sono disponibili anche nell'app desktop e in **Cowork**.

> Il server non ha autenticazione: chiunque conosca l'URL pubblico può interrogarlo. I dati esposti sono
> di sola lettura, ma è comunque opportuno non lasciare tunnel aperti inutilmente e, per un'installazione
> stabile, proteggere l'accesso (es. autenticazione sul reverse proxy o restrizione degli IP).

### Claude Desktop con server locale

In alternativa, senza esporre il server su Internet, Claude Desktop può usare il server locale tramite
il bridge [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) (richiede Node.js). In
**Impostazioni → Sviluppatore → Modifica configurazione** (`claude_desktop_config.json`):

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

Riavviare Claude Desktop con OMIMyMCP già in esecuzione (`OMIMyMCP omi.db 8765`).

### Claude Code

```
claude mcp add --transport http omi http://localhost:8765/mcp
```

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
  (**Impostazioni → Connettori → Aggiungi connettore personalizzato**) con l'URL dell'endpoint.
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

Il semestre si indica come `AAAA/S` (es. `2025/2`); se omesso si usa l'ultimo caricato.
Per i semestri senza perimetri (2004/1 – 2010/1) la ricerca per coordinate usa i perimetri del semestre
più vicino, collegati per codice catastale + codice zona; se nel frattempo la zona è stata rinominata la
risposta lo segnala.

## Struttura del database

### Contenuto degli ZIP originali

* `..._AAAAS_ZONE.csv` – una riga per zona OMI: comune (codice catastale/Belfiore, ISTAT), fascia, codice e
  descrizione zona, `LinkZona`, tipologia e stato prevalenti, microzona.
* `..._AAAAS_VALORI.csv` – quotazioni per zona (`LinkZona`), tipologia e stato conservativo:
  compravendita €/m² min/max, locazione €/m²·mese min/max, superficie lorda/netta.
* `*.kml` (solo nei `QIP*`) – un file per comune con i poligoni delle zone.

### Tabelle

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

La geometria è in `geometria.dati`: varint zigzag delle differenze delle coordinate in microgradi WGS84
(vedi `Geometry.Encode/Decode` in `Geo.cs`).

Esempio di query diretta:

```sql
SELECT z.comune_descr, z.zona, z.zona_descr, v.compr_min, v.compr_max, v.loc_min, v.loc_max
FROM zona z JOIN valore v ON v.semestre = z.semestre AND v.link_zona = z.link_zona
WHERE z.semestre = 20252 AND z.comune_amm = 'F205' AND v.cod_tip = 20 AND v.stato_prev = 'P'
ORDER BY v.compr_max DESC;
```

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
(`serverInfo`, istruzioni e risposta di `elenco_semestri`).

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

