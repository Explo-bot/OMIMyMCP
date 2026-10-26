# Risultati di riferimento ("golden")

Il test `crates/omi-core/tests/golden.rs` chiama gli strumenti di `omi_core::service` sul database sintetico (`support::scenario_server`: QIP del 2024/2 con perimetri e QI del 2025/1 senza perimetri) e
confronta il **testo** prodotto con i file di questa cartella:

- `{caso}.json`: JSON compatto atteso (una riga);
- `{caso}.errore.txt`: messaggio d'errore di validazione atteso (risultato MCP con `isError: true`).

I file sono stati **scritti a mano** a partire dalle regole di serializzazione (ordine fisso delle chiavi, omissione dei
`null`, numeri senza `.0`) e dai dati del generatore `tests/support/mod.rs`, non dall'output del codice. Se un
caso non coincide si corregge il codice; si cambia il file solo se risulta sbagliato
il file stesso.

Gli argomenti di ogni caso sono nella tabella `casi()` di `golden.rs`. Alcuni sono passati di proposito come
stringhe (`"true"`, `"13"`, `"50000"`, `"41.895"`), per provare anche i deserializzatori tolleranti.

## Dati dello scenario usati

| Comune | Codice | ISTAT | Prov | Zone 2024/2 | Zone 2025/1 |
|---|---|---|---|---|---|
| ROMA | H501 | 1058091 | RM | B1 `RM00000001`, C2 `RM00000002` | B1, D5 `RM00000005` |
| MILANO | F205 | 1015146 | MI | B1 `MI00000001` | B1 |
| SAMONE | H753 | 1001235 | TO | B1 `TO00000001` | B1 |
| SAMONE | H754 | 4022165 | TN | R1 `TN00000001` | R1 |
| BORGO VECCHIO → BORGO NUOVO | B999 | 1001999 | TO | B1 `TO00000099` | B1 |

Perimetri (solo 2024/2): Roma B1 = rettangolo 12,48–12,50 × 41,89–41,91 con il buco 12,485–12,49 × 41,895–41,90,
più il rettangolo 12,51–12,52 × 41,89–41,90 (stile `7f00ff00`); Roma C2 = 12,40–12,45 × 41,80–41,85 (senza
stile); Milano B1 = 9,18–9,20 × 45,46–45,47.

## Valori calcolati

Le misure geometriche seguono le formule del modulo `geometry`, rifatte da zero con una calcolatrice
(Python come semplice calcolatrice, fuori dal progetto), con arrotondamento all'intero pari:

| Grandezza | Calcolo | Valore |
|---|---|---|
| Area di Roma B1 in gradi² | 0,0004 − 0,000025 + 0,0001 | 0,000475 |
| Centro di Roma B1 | (12,49·0,0004 − 12,4875·0,000025 + 12,515·0,0001) / 0,000475 = 12,49539474; analogo in latitudine = 41,89907895 | lon 12.495395, lat 41.899079 |
| `area_km2` di Roma B1 | 0,000475 · 111,32 · 111,32 · cos(41,89907895°) = 4,38128 | 4.3813 |
| `area_km2` di Roma C2 | 0,0025 · 111,32² · cos(41,825°) = 23,08610 | 23.0861 |
| `n_punti` di Roma B1 | 3 anelli chiusi da 5 punti | 15 |
| Distanza di (41,895; 12,503) da Roma B1 | 0,003° · 111 320 · cos(41,895°) = 248,59 m (lato x = 12,50) | 249 |
| Distanza di (41,905; 12,495) da Roma C2 | spigolo (12,45; 41,85): √((0,045·111 320·cos 41,905°)² + (0,055·110 540)²) = 7131,81 m | 7132 |
| Margine di `zone_vicine` a 10 km | 10 000 / 111 000 / cos(41,905°) = 0,121° | Roma C2 è candidata, Milano no |
| Raggio di 50 000 m | `clamp(50000, 0, 20000)` | 20000 |

Le medie di `statistiche_comune` vengono dall'SQL degli strumenti, eseguito da SQLite. `round()` di SQLite
arrotonda la rappresentazione decimale del `double` e, sulle metà esatte, **lontano dallo zero**:

| Semestre | Grandezza | Calcolo | Valore |
|---|---|---|---|
| 2025/1 | abitazioni civili, compravendita | media di (3600+5300)/2 e (2000+2800)/2 | 3425 |
| 2025/1 | abitazioni civili, locazione | media di (12,8+18,6)/2 e (7,5+10)/2 = 12,225000000000001 | 12.23 |
| 2025/1 | box, locazione | (8,2+11,2)/2 | 9.7 |
| 2024/2 | abitazioni civili, compravendita | media di 4350,25 e 2950 = 3650,125 | 3650.1 |
| 2024/2 | abitazioni civili, locazione | media di 15,4 e 10,55 = 12,975000000000001 | 12.98 |
| 2024/2 | fascia B, compravendita | (3500+5200,5)/2 = 4350,25 (metà esatta) | 4350.3 |

Il valore 4350.3 è quello di SQLite: è il `round()` della query degli strumenti, non un
arrotondamento fatto dal codice Rust, al quale si applica invece l'arrotondamento all'intero pari.
