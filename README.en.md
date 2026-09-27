# OMIMyMCP

Imports the real-estate price quotations of the **Osservatorio del Mercato Immobiliare (OMI)** of the Italian
Revenue Agency (*Agenzia delle Entrate*) into a SQLite database and makes them queryable as an **MCP server**
from Claude (claude.ai, Claude Desktop, Cowork, Claude Code) or any other MCP client.

🇮🇹 [Italiano](README.md) · 🇬🇧 English

Author: **Explobot** – original project: <https://github.com/Explo-bot/OMIMyMCP> – [Apache-2.0](LICENSE) license

> ## ⚠️ IMPORTANT NOTICE – PLEASE READ BEFORE USE
>
> **This software was developed entirely with Claude Code and the Claude Opus 5.5 model.**
> The code has **not yet been reviewed** by human developers, nor has it undergone systematic verification of
> correctness, security and reliability: it is **awaiting review**.
>
> The software is provided **"as is"**, without warranties of any kind, express or implied, including but not
> limited to merchantability, fitness for a particular purpose and freedom from errors. **Use is at the
> user's sole risk.** The author accepts no liability for any damage, data loss or consequence, direct or
> indirect, arising from the use of the software or from reliance on the results it produces.
>
> OMI quotations are indicative only and **do not replace a professional appraisal**: any financial decision
> must be checked against official sources. Always verify the results before using them in professional
> contexts, and do not expose the server to the Internet without appropriate protection.

> **Data source: Agenzia delle Entrate - OMI.**
> This project is not affiliated with or endorsed by the Agenzia delle Entrate. Read the
> [License and obligations](#license-and-obligations) section before using or redistributing the data.
> Official description of the service (in Italian):
> [Forniture dati OMI](https://www.agenziaentrate.gov.it/portale/it/web/guest/schede/fabbricatiterreni/omi/forniture-dati-omi).

## Contents

- [How it works](#how-it-works)
- [Downloading the data from the Agenzia delle Entrate website](#downloading-the-data-from-the-agenzia-delle-entrate-website)
- [Bulk downloading many files with Claude Chat](#bulk-downloading-many-files-with-claude-chat)
- [Loading the data with OMIMyMCP](#loading-the-data-with-omimymcp)
- [Using the program](#using-the-program)
- [Connecting to Claude as an MCP server](#connecting-to-claude-as-an-mcp-server)
- [Using it together with MCP Cruscotto Italia](#using-it-together-with-mcp-cruscotto-italia)
- [Usage examples with Claude Cowork](#usage-examples-with-claude-cowork)
- [MCP tools](#mcp-tools)
- [Database structure](#database-structure)
- [License and obligations](#license-and-obligations)

## How it works

The project **neither contains nor distributes OMI data**. Each user downloads, free of charge, the supplies
for the semesters they are interested in from the Agenzia delle Entrate website and loads them with OMIMyMCP
into their own local SQLite database:

```
Agenzia delle Entrate            OMIMyMCP                     Claude / MCP client
OMI data supplies   ──ZIP──►   loading    ──►  omi.db  ──►  MCP server (/mcp)
(QI*.zip, QIP*.zip)
```

You can load a single semester or the whole historical series, the entire country or only some regions,
provinces or municipalities. New semesters are added as the Agenzia delle Entrate publishes them.

## Downloading the data from the Agenzia delle Entrate website

The data is downloaded free of charge from the **Forniture dati OMI** (OMI data supplies) service, reserved for
authenticated users.

### Available periods

| Data | Available from | Supply to request | Resulting file |
|---|---|---|---|
| Quotations (zones and values, CSV) | 1st semester 2004 | **Quotazioni immobiliari** | `QI<request no.>_<tax code>.zip` |
| Zone boundaries (KML) **plus** the quotations of the same semester | 2nd semester 2010 | **Perimetri delle zone OMI** | `QIP<request no.>_<tax code>.zip` |

The Agenzia delle Entrate normally publishes the data of a semester a few months after it closes (for example, the data for
the 2nd semester 2025 was generated in March 2026). Each file covers **a single semester**: for several
periods, make several requests.

### Which files to download

| Desired period | What to request for each semester |
|---|---|
| from the 2nd semester 2010 onwards | only the **Perimetri delle zone OMI** supply: it contains both the boundaries and the quotations |
| from the 1st semester 2004 to the 1st semester 2010 | the **Quotazioni immobiliari** supply (boundaries do not exist for these semesters) |

OMIMyMCP also works without boundaries, but the coordinate search (`zona_da_coordinate`, `zone_vicine`) needs
at least one semester with boundaries: for semesters that lack them, it uses those of the nearest semester.

Approximate sizes for the entire country: about 2 MB for a `QI` file and 40–130 MB for a `QIP` file. The
whole 2004–2025 series takes about 3.5 GB of ZIPs and produces a database of about 2 GB.

### Download procedure

1. Open the service explanation page:
   <https://www.agenziaentrate.gov.it/portale/it/web/guest/schede/fabbricatiterreni/omi/forniture-dati-omi>
2. Click **Accedi al servizio** (access the service), or go directly to
   <https://telematici.agenziaentrate.gov.it/Main/index.jsp>.
3. Authenticate with **SPID**, **CIE**, **CNS** or **Fisconline/Entratel** credentials.
4. In the reserved area, open the **Forniture dati OMI** service. If it does not appear in the services menu,
   use the search function of the reserved area: its position in the menus may change over time.
5. Choose the supply (**Perimetri delle zone OMI** or **Quotazioni immobiliari**, see the table above).
6. Select the **territorial scope**: *entire national territory* or a region, province or municipality.
   OMIMyMCP accepts any scope and partial supplies add up.
7. Select the **semester** and submit the request.
8. Requests are processed asynchronously: the file appears in the service's list of requested supplies and can
   be downloaded when ready (usually a few minutes for national files).
9. Repeat for each desired semester and save all the ZIPs in the same folder, **without renaming or
   extracting them**.

> **Privacy warning.** The names of the files supplied by the Agency contain the **tax code of the person who
> made the request**. OMIMyMCP removes it from the names recorded in the database, but the original ZIPs must
> not be published or shared: the project's `.gitignore` excludes `*.zip` and `.db` files.

### Bulk downloading many files with Claude Chat

Requesting and downloading dozens of semesters by hand is repetitive. The files can be downloaded **in bulk and
with little effort using Claude Chat**, letting Claude repeat the procedure on the Agenzia delle Entrate website on the
user's behalf (request, waiting for processing, downloading each ZIP). This requires a Claude mode able to use
the browser (for example Claude in Chrome).

**Example prompt:**

> Download for me from https://fornituredatiomi.agenziaentrate.gov.it/dati-omi-fe/RF
> the real-estate quotations complete with zone boundaries for each semester
> available at national level.

> **Authentication.** Access to the Agenzia delle Entrate website (SPID, CIE, CNS or Fisconline/Entratel) is
> requested **interactively**: Claude stops and waits for the user to authenticate in the browser.
> Credentials must never be written in the prompt or communicated to Claude.
> The [download and privacy](#download-procedure) notes still apply: the ZIPs contain the tax code of the
> person who made the request and must not be shared.

## Loading the data with OMIMyMCP

### First load

Open a terminal in the folder containing the downloaded ZIPs and launch OMIMyMCP (see [Building](#building))
**without parameters**:

```
cd C:\DatiOMI
C:\path\to\OMIMyMCP.exe
```

All `QI*.zip` and `QIP*.zip` files are loaded, in name order, into `omi.db` in the same folder. A national
semester with boundaries takes about 10 seconds, one with quotations only about 2 seconds: the whole 2004–2025
series loads in 4–5 minutes. A corrupted file is reported and skipped without interrupting the others.

### Adding a new semester

Download the supply for the new semester and load it into the existing database:

```
OMIMyMCP QIP<request no.>_<tax code>.zip omi.db
```

Several files can be given in the same command.

### Reloading or correcting data

Loading **replaces**, for the file's semester, the data of the municipalities the file contains:

- a **national** supply replaces the whole semester (useful if the Agenzia delle Entrate republishes corrected data);
- supplies for different **regions, provinces or municipalities** of the same semester add up;
- reloading the same file does not create duplicates;
- CSVs replace zones and quotations, KMLs replace boundaries: a `QI` file without boundaries does not delete
  the boundaries already loaded for that semester.

Municipalities present in the database but absent from the new supply (for example, dissolved ones) are not
deleted. To start from scratch, delete `omi.db` and reload the ZIPs.

### Verification

When you start the server (`OMIMyMCP omi.db 8765`), the `elenco_semestri` tool shows the loaded semesters with
the number of zones, quotations and boundaries. Alternatively, with `sqlite3`:

```
sqlite3 omi.db "SELECT semestre, n_zone, n_valori, n_geometrie FROM semestre ORDER BY semestre;"
```

## Using the program

```
OMIMyMCP [QI*.zip ...] [database.db] [port] [/l]
```

All parameters are optional, can be given in any order and are recognized by their type:

| Parameter | Meaning |
|---|---|
| `QI*.zip` / `QIP*.zip` | OMI supply to load (see [Reloading or correcting data](#reloading-or-correcting-data)). Several files can be given. |
| `*.db` | SQLite database (default `omi.db` in the current folder). Created if it does not exist. |
| `0..65535` | MCP server port. After loading, the program keeps listening on `http://host:port/mcp` (*streamable* HTTP). With `0` the port is chosen by the system and printed on screen. |
| `/l` | Enables the [server log](#server-log). Equivalents: `-l`, `--log`. Only effective when a port is given. |

With no parameters, it loads all the `QI*.zip` / `QIP*.zip` files of the current folder. If only a database,
a port and/or `/l` are given, nothing is loaded.

```
OMIMyMCP                                              # loads all zips in the folder into omi.db
OMIMyMCP QIP1438117_XXXXXXXXXXXXXXXX.zip              # (re)loads one semester
OMIMyMCP QIP1438200_XXXXXXXXXXXXXXXX.zip omi.db 8765  # loads one semester and starts the server
OMIMyMCP omi.db 8765                                  # MCP server only, on port 8765
OMIMyMCP omi.db 8765 /l                               # MCP server with logging enabled
```

The server accepts connections on all network interfaces (on Windows the firewall may ask for permission).
It has no authentication: it only exposes read-only tools.

### Server log

With `/l` the server records every request it receives. Lines are shown on the console and appended to
the log file, which has the same name and folder as the database with a `.log` extension (for example
`omi.db` → `omi.log`). The file is never truncated: delete it to start over.

Each request produces two kinds of lines:

- **`HTTP`**: client IP address, method, path, response code and duration;
- **`TOOL`**: for every MCP tool call, the tool name, the arguments received, the outcome (`ok` with the
  response size in characters, or `ERRORE` with the returned message) and the duration.

```
2026-09-27 09:20:53.747 TOOL cerca_comuni {"testo":"Asti"} -> ok, 215 caratteri (39 ms)
2026-09-27 09:20:53.766 HTTP ::1 POST /mcp -> 200 (185 ms)
2026-09-27 09:20:53.811 TOOL zona_da_coordinate {"latitudine":44.9,"longitudine":8.2} -> ok, 3076 caratteri (31 ms)
2026-09-27 09:20:53.822 TOOL dettaglio_zona {"zona":"R1","comune":"samone"} -> ERRORE: 'samone' è ambiguo, specificare la provincia o il codice catastale. Candidati: SAMONE (TO) [H753], SAMONE (TN) [H754] (4 ms)
```

(Log messages are in Italian, like the rest of the program output.)

The log is useful to see which tools Claude uses and with which arguments, to understand why an answer
is empty or wrong, and to check who is accessing a server exposed on the Internet. It records client IP
addresses and the text of the requests: if other people use the server, take this into account for
privacy purposes.

> In **Git Bash**, arguments starting with `/` are converted into Windows paths: use `-l` instead of `/l`.
> In Command Prompt and PowerShell `/l` works normally.

### Building

Requires the [.NET 10 SDK](https://dotnet.microsoft.com/download).

```
cd OMIMyMCP
dotnet build -c Release
```

The executable is in `bin/Release/net10.0/OMIMyMCP.exe`. For a single executable that does not need the
runtime installed:

```
dotnet publish -c Release -r win-x64 --self-contained -p:PublishSingleFile=true
```

(`linux-x64`, `osx-arm64`, … for other systems).

## Connecting to Claude as an MCP server

The server exposes the MCP protocol over HTTP at `http://<host>:<port>/mcp`.

### claude.ai (web, desktop and mobile apps, Cowork)

claude.ai *custom connectors* are contacted by Anthropic's infrastructure, so the server must be reachable
**from the Internet over HTTPS**; `localhost` does not work.

1. **Make the server reachable over HTTPS**, for example:
   - on a server/VPS with a reverse proxy (Caddy, nginx, IIS) that terminates TLS and forwards to `http://localhost:8765`;
   - for testing, with a temporary tunnel:
     ```
     OMIMyMCP omi.db 8765
     cloudflared tunnel --url http://localhost:8765
     ```
     which returns an address such as `https://random-name.trycloudflare.com`.
2. In claude.ai open **Settings → Connectors → Add custom connector**
   (on Team/Enterprise plans an administrator adds it from the organization settings).
3. Enter a name (e.g. `OMI quotations`) and the full endpoint URL, for example
   `https://random-name.trycloudflare.com/mcp`. No OAuth authentication is required.
4. In a conversation, enable the connector from the tools menu (**+** button / *Connectors*).

Connectors added to your account are also available in the desktop app and in **Cowork**.

> The server has no authentication: anyone who knows the public URL can query it. The exposed data is
> read-only, but it is still advisable not to leave tunnels open unnecessarily and, for a stable installation,
> to protect access (e.g. authentication on the reverse proxy or IP restrictions).

### Claude Desktop with a local server

Alternatively, without exposing the server to the Internet, Claude Desktop can use the local server through
the [`mcp-remote`](https://www.npmjs.com/package/mcp-remote) bridge (requires Node.js). In
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

Restart Claude Desktop with OMIMyMCP already running (`OMIMyMCP omi.db 8765`).

### Claude Code

```
claude mcp add --transport http omi http://localhost:8765/mcp
```

## Using it together with MCP Cruscotto Italia

OMIMyMCP becomes much more useful when used **together with AgID's official MCP server
[MCP Cruscotto Italia](https://www.dati.gov.it/sviluppatori/mcp-cruscotto-italia)** (page in Italian).
[Cruscotto Italia](https://cruscotto-italia.dati.gov.it/) gathers, for every Italian municipality, public
data from more than 20 institutional sources (ISTAT, MEF, ANAC, ISPRA, Italia Domani/PNRR, Ministry of
Health, Agenzia delle Entrate and others) and exposes them through a free public MCP endpoint.

With both connectors enabled, Claude can cross-reference OMI property quotations with the context of the
municipality, for example:

- **addresses → coordinates**: the georeferenced ANNCSU street register (house numbers with coordinates)
  makes it possible to obtain the precise latitude and longitude of an address, to then pass to
  `zona_da_coordinate`;
- **demographics and census**: population, age, households, occupied and empty dwellings;
- **IRPEF income** by municipality, to compare prices and rents with spending capacity;
- **territory and risks**: land consumption, hydrogeological risk, seismic classification, air quality;
- **services and investments**: schools, pharmacies, hospitals, EV charging points, broadband, cultural
  heritage, PNRR projects and public works.

### Connection

The endpoint is `https://cruscotto-italia-mcp.agid.workers.dev/mcp` and does not require a local server.

- **claude.ai / Claude Desktop / Cowork**: add a second custom connector
  (**Settings → Connectors → Add custom connector**) with the endpoint URL.
- **Claude Code**:
  ```
  claude mcp add --transport http cruscotto-italia https://cruscotto-italia-mcp.agid.workers.dev/mcp
  ```

The official instructions are on the
[Cruscotto Italia MCP access page](https://cruscotto-italia.dati.gov.it/about.html#accesso-mcp). For more
targeted answers, AgID also provides an optional skill for Claude, downloadable from the
[presentation page](https://www.dati.gov.it/sviluppatori/mcp-cruscotto-italia).

### Examples of combined questions

> Which OMI zone is via Nizza 150 in Turin in? Use the ANNCSU street register for the coordinates and give
> me the quotations for residential dwellings.

> Compare the municipalities of the province of Monza and Brianza: average value of residential dwellings in
> the latest semester, average IRPEF income and population. Which have the highest price-to-income ratio?

> For the provincial capitals of Emilia-Romagna, relate the trend of residential prices from 2016 to today
> to the change in population and to hydrogeological risk.

Cruscotto Italia data have their own licences (mostly CC-BY 4.0, stated for each source): results must
cite both the Cruscotto Italia sources and **"Agenzia delle Entrate - OMI"**.

## Usage examples with Claude Cowork

With the connector enabled you can ask Claude, in natural language, to query the data and to produce
documents, spreadsheets or charts. The server does not convert addresses to coordinates: when a street is
given, Claude derives latitude and longitude and uses `zona_da_coordinate` (for precise addresses it is better
to provide the coordinates directly or to check the zone returned, or to also enable
[MCP Cruscotto Italia](#using-it-together-with-mcp-cruscotto-italia), which provides the georeferenced
ANNCSU street register).

**Valuing a property**
> I have a 95 m² apartment in good condition at via Nizza 150 in Turin. Which OMI zone is it in, and what
> are the sale value range and monthly rent range according to the latest semester?
Resulting artifact (in Italian): https://claude.ai/artifact/44hMgy8mT8SxEGdvYEp7je

**Comparing zones of a municipality**
> List the OMI zones of Bergamo with the quotations for normal-condition residential dwellings and prepare
> an Excel sheet sorted from most to least expensive, with a column for the average value.

**Historical series and chart**
> Show me the trend from 2004 to today of the quotations for residential dwellings in zone B12 of Milan,
> with a chart of the average sale and rental value. Take into account that the zoning has changed over time
> and flag the semesters in which the zone description changes.
Resulting artifact: https://claude.ai/artifact/6dU1DpCpQuSCt7ZWAcikPb

**Commercial property trends**
> Can you show me the trend over the last 10 years of purchase prices and rents for commercial properties
> in the centre of Monza?
Resulting artifact (in Italian): https://claude.ai/artifact/7GQT5u1kKHuHSh8LmY5d2w

**Territorial analysis**
> Which OMI zones are within 1.5 km of Piazza del Campo in Siena? For each, give the band, description and
> quotations for shops and residential dwellings.

**Rental yield**
> For the central zones (band B) of Bologna, calculate the gross annual rental yield of residential
> dwellings (average rent × 12 / average sale value) and create a short report in Word.

**Investment simulation**
> If I had invested 100,000 euros in 2014 in properties to rent out, ignoring purchase taxes and costs,
> income tax, property taxes and extraordinary expenses, can you show me charts of how much I would have
> earned in each zone of Rome, separating rental income from the increase in the property's value?
> At the end, can you add some considerations on the possible costs, such as purchase taxes, income and
> property taxes, extraordinary expenses and the like?
Resulting artifact (in Italian): https://claude.ai/artifact/45mznWXmn6k4CH2uUFPsxo

**Comparing cities**
> Compare Florence, Pisa and Livorno in the 2nd semester 2025: average value per m² by band and by
> residential type. Summarize the differences in a table.

**Map**
> Retrieve the GeoJSON boundary of zone C3 in Padua and its neighbouring zones and prepare an HTML page with
> a Leaflet map coloured by the value of residential dwellings.

In results produced with this data the source **"Agenzia delle Entrate - OMI"** must always be cited (see below).

## MCP tools

| Tool | Description |
|---|---|
| `elenco_semestri` | available semesters with number of zones, quotations and boundaries |
| `elenco_tipologie` | building types (code and description) |
| `cerca_comuni` | search by name (also partial, accents ignored), cadastral or ISTAT code |
| `zone_comune` | zones of a municipality in a semester, optionally with quotations and boundaries |
| `dettaglio_zona` | registry data, quotations and boundary (also as GeoJSON) of a zone |
| `zona_da_coordinate` | zone containing a lat/lon point with all its data (or the nearest within 2 km) |
| `zone_vicine` | zones within a radius of a point, with the quotations for residential dwellings |
| `storico_zona` | historical series of a zone's quotations |
| `statistiche_comune` | minimum, maximum and average values by type and by band for a municipality |

The semester is given as `YYYY/S` (e.g. `2025/2`); if omitted, the latest loaded one is used.
For semesters without boundaries (2004/1 – 2010/1) the coordinate search uses the boundaries of the nearest
semester, linked by cadastral code + zone code; if the zone has been renamed in the meantime, the response
says so.

## Database structure

### Contents of the original ZIPs

* `..._YYYYS_ZONE.csv` – one row per OMI zone: municipality (cadastral/Belfiore code, ISTAT), band, zone code
  and description, `LinkZona`, prevalent type and condition, microzone.
* `..._YYYYS_VALORI.csv` – quotations per zone (`LinkZona`), type and conservation status:
  sale €/m² min/max, rent €/m²·month min/max, gross/net area.
* `*.kml` (only in `QIP*`) – one file per municipality with the zone polygons.

### Tables

| Table | Contents |
|---|---|
| `semestre` | loaded semesters (code `YYYYS`, e.g. 20252), processing date, source file, counts |
| `zona` | all columns of `ZONE.csv`, key `(semestre, link_zona)` |
| `valore` | all columns of `VALORI.csv`, key `(semestre, link_zona, cod_tip, stato)`; numeric amounts |
| `tipologia` | code and description of building types |
| `geometria` | zone polygons (compact binary encoding), bbox, centroid, area; deduplicated across semesters |
| `geometria_rt` | R*Tree spatial index on the bboxes |
| `zona_geo` | semester + municipality + zone → geometry (with LinkZona and KML colour) |
| `comune` | municipality registry (latest semester data) with first/last available semester |
| `comune_nome` | municipality names (also historical) normalized for search |
| `file_caricato` | log of imported files (without tax code) |

The geometry is in `geometria.dati`: zigzag varint of the coordinate differences in WGS84 microdegrees
(see `Geometry.Encode/Decode` in `Geo.cs`).

Example of a direct query:

```sql
SELECT z.comune_descr, z.zona, z.zona_descr, v.compr_min, v.compr_max, v.loc_min, v.loc_max
FROM zona z JOIN valore v ON v.semestre = z.semestre AND v.link_zona = z.link_zona
WHERE z.semestre = 20252 AND z.comune_amm = 'F205' AND v.cod_tip = 20 AND v.stato_prev = 'P'
ORDER BY v.compr_max DESC;
```

## License and obligations

### Source code

Copyright 2026 **Explobot** – <https://github.com/Explo-bot/OMIMyMCP>

The OMIMyMCP code is released under the **[Apache License 2.0](LICENSE)**. You may use, modify and
redistribute it freely, including for commercial purposes, under the following conditions:

- **Mandatory attribution**: whoever redistributes the software, in original or modified form, must include
  the [`NOTICE`](NOTICE) file and reproduce its content, namely the **name of the original author** (Explobot)
  and the **link to the original project** (https://github.com/Explo-bot/OMIMyMCP), where attributions are
  normally shown (documentation, "about" screen, distributed files);
- a copy of the license must be included;
- modified files must clearly state that they have been modified;
- the copyright notices in the sources must be preserved.

The MCP server reports the author and project link in the information it provides to clients
(`serverInfo`, instructions and the `elenco_semestri` response).

### OMI data

The project **does not include or redistribute OMI data**: each user downloads it directly from the Agenzia
delle Entrate with their own credentials. The data (original CSV and KML files and the database OMIMyMCP
derives from them) is **not** covered by the code license and remains subject to the Agenzia delle Entrate's
[contractual and usage conditions](https://www.agenziaentrate.gov.it/portale/schede/fabbricatiterreni/omi/banche-dati/quotazioni-immobiliari/condizioni-contrattuali-qi)
and to the indications on the
[Forniture dati OMI](https://www.agenziaentrate.gov.it/portale/it/web/guest/schede/fabbricatiterreni/omi/forniture-dati-omi)
page. The Agenzia delle Entrate does not attach a standard open license (e.g. Creative Commons) to the data. In summary:

- **Ownership**: the information is the exclusive property of the Agenzia delle Entrate.
- **Mandatory citation**: whoever uses or publishes the data, even after processing, must cite the source
  **"Agenzia delle Entrate - OMI"**.
- **Processing allowed**: the data may also be used for one's own processing, citing the source if published.
- **Prohibitions**: the conditions do not allow selling, renting, leasing, transferring or assigning the
  database contents to third parties, nor assuming obligations towards third parties on them.
  For this reason **the database produced by OMIMyMCP is for the use of whoever downloaded the data and must
  not be shared or published**.
- **No warranty**: the Agenzia delle Entrate is not liable for damages arising from the use of the data. The quotations
  indicate broad values referring to the ordinary condition of properties and **do not replace the appraisal**
  of a professional.
- **Historical series**: the Agenzia delle Entrate warns that revisions of zones and improvements to methods have produced
  jumps in quotations between semesters; compare historical series with caution.

Exposing the MCP server to third parties (for example through a public connector) means making the data
accessible to them: evaluate this in light of the Agenzia delle Entrate's conditions.

### Transformations applied during loading

The database keeps all the information of the original files, with these transformations:

- amounts converted from text with a decimal comma to numbers; quotes around descriptions removed;
- boundaries converted from KML to a binary encoding, with deduplication of identical polygons across
  semesters and computation of bbox, centroid and area (the HTML descriptions of placemarks are not kept);
- for KMLs that declare UTF-8 but contain Latin-1 characters, Latin-1 decoding is used;
- malformed KML placemarks are discarded and reported on screen (known case: 1st semester 2011,
  file `C302.kml`, Castiglione Chiavarese, zone B1 without geometry);
- the requester's tax code is removed from the recorded source file names.

Data source: **Agenzia delle Entrate - OMI**.
