// OMIMyMCP-rs - Copyright 2026 Explobot
// SPDX-License-Identifier: Apache-2.0

//! Lettura dei KML dei perimetri: decodifica, macchina a stati, controlli di rigore e recupero dei
//! file malformati.

use std::collections::HashMap;
use std::sync::LazyLock;

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::{Reader, XmlVersion};
use regex::Regex;

use crate::error::{OmiError, Result};
use crate::geometry::{Geometry, Ring, S};

// Invariante: il pattern è costante e valido (verificato dai test di questo modulo).
static RE_SEMESTRE_DESCRIZIONE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(\d{4})\s*/\s*([12])").expect("espressione regolare costante valida")
});

/// Un placemark letto dal KML.
#[derive(Debug, Default)]
pub(super) struct Placemark {
    pub nome: Option<String>,
    pub descrizione: Option<String>,
    pub style_url: Option<String>,
    pub link_zona: Option<String>,
    pub cod_com: Option<String>,
    pub cod_zona: Option<String>,
    pub geometria: Geometry,
}

/// Contenuto di un KML.
#[derive(Debug, Default)]
pub(super) struct Kml {
    /// Colore del `PolyStyle` per id dello stile.
    pub stili: HashMap<String, String>,
    pub placemarks: Vec<Placemark>,
    /// Lettura completa fallita e recupero eseguito: messaggio d'errore e pezzi scartati.
    pub recupero: Option<(String, usize)>,
}

impl Kml {
    /// Semestre `AAAAS` dalla prima `description` che lo contiene.
    pub fn semestre(&self) -> Option<i64> {
        self.placemarks.iter().find_map(|p| {
            let c = RE_SEMESTRE_DESCRIZIONE.captures(p.descrizione.as_deref()?)?;
            let anno: i64 = c[1].parse().ok()?;
            let s: i64 = c[2].parse().ok()?;
            Some(anno * 10 + s)
        })
    }
}

/// UTF-8 stretto, altrimenti Latin-1 (ogni byte diventa U+00XX); la codifica dichiarata si ignora.
pub(super) fn decode(bytes: Vec<u8>) -> String {
    let mut s = String::from_utf8(bytes)
        .unwrap_or_else(|e| e.into_bytes().iter().map(|&b| char::from(b)).collect());
    if s.starts_with('\u{feff}') {
        s.drain(..'\u{feff}'.len_utf8());
    }
    s
}

/// Coordinate `lon,lat[,alt]` separate da spazi → anello in microgradi. I token non validi si scartano.
pub(super) fn parse_coords(text: &str) -> Ring {
    let mut ring = Vec::new();
    for token in text.split_whitespace() {
        let mut parts = token.split(',');
        let (Some(lon), Some(lat)) = (parts.next(), parts.next()) else {
            continue;
        };
        let (Ok(lon), Ok(lat)) = (lon.parse::<f64>(), lat.parse::<f64>()) else {
            continue;
        };
        if let (Some(x), Some(y)) = (micro(lon), micro(lat)) {
            ring.push(x);
            ring.push(y);
        }
    }
    ring
}

/// Gradi → microgradi arrotondati all'intero pari; `None` se il valore non è rappresentabile in `i32`.
fn micro(v: f64) -> Option<i32> {
    let m = (v * S).round_ties_even();
    (m >= f64::from(i32::MIN) && m <= f64::from(i32::MAX)).then_some(m as i32)
}

/// Legge un KML; se è malformato recupera i placemark leggibili (§6.3).
pub(super) fn read_kml(text: &str) -> Result<Kml> {
    let err = match parse(text) {
        Ok(kml) => return Ok(kml),
        Err(e) => e,
    };
    let Some(first) = text.find("<Placemark") else {
        return Err(err);
    };
    const FINE: &str = "</Placemark>";
    let header = &text[..first];
    let mut kml = Kml::default();
    let mut scartati = 0;
    let mut pos = first;
    while let Some(start) = text[pos..].find("<Placemark").map(|i| pos + i) {
        let Some(end) = text[start..].find(FINE).map(|i| start + i + FINE.len()) else {
            // Placemark senza chiusura: non si può leggere.
            scartati += 1;
            break;
        };
        let piece = format!("{header}{}</Document></kml>", &text[start..end]);
        match parse(&piece) {
            Ok(p) => {
                kml.stili.extend(p.stili);
                kml.placemarks.extend(p.placemarks);
            }
            Err(_) => scartati += 1,
        }
        pos = end;
    }
    kml.recupero = Some((err.to_string(), scartati));
    Ok(kml)
}

/// Elemento di cui serve il testo.
#[derive(Debug, Clone, Copy)]
enum Campo {
    Color,
    Name,
    Description,
    StyleUrl,
    Value,
    Coordinates,
}

impl Campo {
    fn from_name(name: &str) -> Option<Campo> {
        Some(match name {
            "color" => Campo::Color,
            "name" => Campo::Name,
            "description" => Campo::Description,
            "styleUrl" => Campo::StyleUrl,
            "value" => Campo::Value,
            "coordinates" => Campo::Coordinates,
            _ => return None,
        })
    }
}

/// Testo raccolto dall'apertura alla chiusura di un elemento, compresi gli elementi annidati.
struct Capture {
    depth: usize,
    campo: Campo,
    text: String,
}

#[derive(Default)]
struct State {
    kml: Kml,
    style_id: Option<String>,
    in_poly_style: bool,
    placemark: Option<Placemark>,
    polygon: Option<usize>,
    data_name: Option<String>,
    capture: Option<Capture>,
}

impl State {
    fn start(&mut self, e: &BytesStart, depth: usize) -> std::result::Result<(), String> {
        let local = e.local_name();
        let name = local.as_ref();
        // Tutti gli attributi vengono letti, così entità sconosciute e sintassi errata sono errori.
        let mut id = None;
        let mut data = None;
        for a in e.attributes() {
            let a = a.map_err(|e| e.to_string())?;
            let v = a
                .normalized_value(XmlVersion::Implicit1_0)
                .map_err(|e| e.to_string())?;
            match a.key.local_name().as_ref() {
                "id" => id = Some(v.into_owned()),
                "name" => data = Some(v.into_owned()),
                _ => {}
            }
        }
        match name {
            "Style" => self.style_id = id,
            // Gli elementi vuoti arrivano come apertura + chiusura: `<PolyStyle/>` torna subito a false.
            "PolyStyle" => self.in_poly_style = true,
            "Placemark" => {
                self.placemark = Some(Placemark::default());
                self.polygon = None;
            }
            "Data" if self.placemark.is_some() => {
                self.data_name = data.map(|d| d.to_uppercase());
            }
            "Polygon" => {
                if let Some(p) = &mut self.placemark {
                    p.geometria.polygons.push(Vec::new());
                    self.polygon = Some(p.geometria.polygons.len() - 1);
                }
            }
            _ => {}
        }
        if self.capture.is_none()
            && let Some(campo) = Campo::from_name(name)
        {
            self.capture = Some(Capture {
                depth,
                campo,
                text: String::new(),
            });
        }
        Ok(())
    }

    fn text(&mut self, s: &str) {
        if let Some(c) = &mut self.capture {
            c.text.push_str(s);
        }
    }

    fn end(&mut self, name: &str, depth: usize) {
        if self.capture.as_ref().is_some_and(|c| c.depth == depth)
            && let Some(c) = self.capture.take()
        {
            self.field(c.campo, c.text);
        }
        match name {
            "Placemark" => {
                if let Some(mut p) = self.placemark.take() {
                    p.geometria.polygons.retain(|rings| !rings.is_empty());
                    self.kml.placemarks.push(p);
                }
                self.polygon = None;
            }
            "Polygon" => self.polygon = None,
            "PolyStyle" => self.in_poly_style = false,
            "Style" => self.style_id = None,
            "Data" => self.data_name = None,
            _ => {}
        }
    }

    fn field(&mut self, campo: Campo, text: String) {
        match campo {
            Campo::Color => {
                if self.in_poly_style
                    && self.placemark.is_none()
                    && let Some(id) = &self.style_id
                {
                    self.kml.stili.insert(id.clone(), text.trim().to_string());
                }
            }
            Campo::Name => {
                if let Some(p) = &mut self.placemark
                    && p.nome.is_none()
                {
                    p.nome = Some(text.trim().to_string());
                }
            }
            Campo::Description => {
                if let Some(p) = &mut self.placemark {
                    p.descrizione = Some(text);
                }
            }
            Campo::StyleUrl => {
                if let Some(p) = &mut self.placemark {
                    p.style_url = Some(text.trim().to_string());
                }
            }
            Campo::Value => {
                if let (Some(p), Some(d)) = (&mut self.placemark, &self.data_name) {
                    let v = Some(text.trim().to_string());
                    match d.as_str() {
                        "LINKZONA" => p.link_zona = v,
                        "CODCOM" => p.cod_com = v,
                        "CODZONA" => p.cod_zona = v,
                        _ => {}
                    }
                }
            }
            Campo::Coordinates => {
                if let (Some(p), Some(i)) = (&mut self.placemark, self.polygon)
                    && let Some(polygon) = p.geometria.polygons.get_mut(i)
                {
                    let ring = parse_coords(&text);
                    if ring.len() >= 6 {
                        polygon.push(ring);
                    }
                }
            }
        }
    }
}

/// Carattere ammesso in XML 1.0.
fn is_xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
}

fn is_xml_space(s: &str) -> bool {
    s.chars().all(|c| matches!(c, ' ' | '\t' | '\r' | '\n'))
}

/// Riferimento `&…;`: entità predefinita o carattere; tutto il resto è un errore.
fn resolve_ref(r: &BytesRef) -> std::result::Result<String, String> {
    if let Some(c) = r.resolve_char_ref().map_err(|e| e.to_string())? {
        if !is_xml_char(c) {
            return Err(format!(
                "riferimento a un carattere non valido: &{};",
                r.as_ref()
            ));
        }
        return Ok(c.to_string());
    }
    quick_xml::escape::resolve_predefined_entity(r.as_ref())
        .map(str::to_string)
        .ok_or_else(|| format!("entità sconosciuta: &{};", r.as_ref()))
}

/// Lettura completa e rigorosa di un KML (§6.1–§6.2).
fn parse(text: &str) -> Result<Kml> {
    let formato =
        |pos: u64, msg: String| OmiError::Import(format!("XML non valido (byte {pos}): {msg}"));
    if let Some((pos, c)) = text.char_indices().find(|&(_, c)| !is_xml_char(c)) {
        return Err(formato(
            pos as u64,
            format!("carattere non ammesso U+{:04X}", u32::from(c)),
        ));
    }

    let mut reader = Reader::from_str(text);
    let config = reader.config_mut();
    config.check_end_names = true;
    config.expand_empty_elements = true;

    let mut st = State::default();
    let mut depth = 0usize;
    let mut root_closed = false;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| formato(reader.error_position(), e.to_string()))?;
        let pos = reader.buffer_position();
        let fuori_radice = || formato(pos, "contenuto dopo l'elemento radice".to_string());
        match event {
            Event::Start(e) => {
                if root_closed {
                    return Err(fuori_radice());
                }
                depth += 1;
                st.start(&e, depth).map_err(|m| formato(pos, m))?;
            }
            Event::Empty(e) => {
                // Non arriva con `expand_empty_elements`; trattato comunque come apertura + chiusura.
                if root_closed {
                    return Err(fuori_radice());
                }
                st.start(&e, depth + 1).map_err(|m| formato(pos, m))?;
                st.end(e.local_name().as_ref(), depth + 1);
                root_closed |= depth == 0;
            }
            Event::End(e) => {
                st.end(e.local_name().as_ref(), depth);
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| formato(pos, "tag di chiusura senza apertura".to_string()))?;
                root_closed |= depth == 0;
            }
            Event::Text(t) => {
                let s = t.xml10_content();
                if depth == 0 {
                    if !is_xml_space(&s) {
                        return Err(formato(pos, "testo fuori dall'elemento radice".to_string()));
                    }
                } else {
                    st.text(&s);
                }
            }
            Event::CData(c) => {
                if depth == 0 {
                    return Err(formato(pos, "CDATA fuori dall'elemento radice".to_string()));
                }
                st.text(&c.xml10_content());
            }
            Event::GeneralRef(r) => {
                let s = resolve_ref(&r).map_err(|m| formato(pos, m))?;
                if depth == 0 {
                    return Err(formato(pos, "testo fuori dall'elemento radice".to_string()));
                }
                st.text(&s);
            }
            Event::Eof => break,
            Event::Comment(_) | Event::Decl(_) | Event::PI(_) | Event::DocType(_) => {}
        }
    }
    if depth > 0 {
        return Err(formato(
            reader.buffer_position(),
            format!("{depth} elementi non chiusi a fine documento"),
        ));
    }
    Ok(st.kml)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTESTAZIONE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2"><Document>
  <Style id="s1"><LineStyle><color>ff000000</color></LineStyle><PolyStyle><color> 7f00ff00 </color></PolyStyle></Style>
  <Style id="vuoto"><PolyStyle/><color>ffffffff</color></Style>
"#;

    fn placemark(nome: &str, coords: &str) -> String {
        format!(
            "<Placemark><name>{nome}</name><Polygon><outerBoundaryIs><LinearRing><coordinates>{coords}</coordinates></LinearRing></outerBoundaryIs></Polygon></Placemark>\n"
        )
    }

    fn doc(body: &str) -> String {
        format!("{INTESTAZIONE}{body}</Document></kml>")
    }

    fn messaggio(r: Result<Kml>) -> String {
        match r {
            Err(OmiError::Import(m)) => m,
            other => panic!("atteso errore di formato, ottenuto {other:?}"),
        }
    }

    #[test]
    fn coordinate() {
        assert_eq!(
            parse_coords(" 12.48,41.89,0\n\t12.49,41.9 x 1,2,3,4 ,5 a,1 1,b"),
            vec![
                12_480_000, 41_890_000, 12_490_000, 41_900_000, 1_000_000, 2_000_000
            ]
        );
        // Metà esatta: arrotondamento all'intero pari.
        assert_eq!(parse_coords("0.0000025,0.0000035"), vec![2, 4]);
        assert!(parse_coords("1e10,1 NaN,1 inf,1").is_empty());
    }

    #[test]
    fn decodifica_latin1_di_ripiego() {
        assert_eq!(decode(b"Forl\xEC".to_vec()), "Forlì");
        assert_eq!(decode("Forlì".as_bytes().to_vec()), "Forlì");
        assert_eq!(decode(b"\xEF\xBB\xBF<kml/>".to_vec()), "<kml/>");
    }

    #[test]
    fn macchina_a_stati() {
        let text = doc(r##"
  <Placemark>
    <name> ROMA - Zona OMI B1 </name><name>ignorato</name>
    <description><![CDATA[<b>Semestre 2025/2</b>]]> &amp; altro</description>
    <styleUrl> #s1 </styleUrl>
    <Style id="interno"><PolyStyle><color>00000000</color></PolyStyle></Style>
    <ExtendedData>
      <Data name="linkzona"><value> RM00000001 </value></Data>
      <Data name="CODCOM"><value>h501</value></Data>
      <Data name="ALTRO"><value>x</value></Data>
      <Data name="CODZONA"><value>B1</value></Data>
    </ExtendedData>
    <MultiGeometry>
      <Polygon>
        <outerBoundaryIs><LinearRing><coordinates>0,0 4,0 4,4 0,4 0,0</coordinates></LinearRing></outerBoundaryIs>
        <innerBoundaryIs><LinearRing><coordinates>1,1 2,1 2,2 1,1</coordinates></LinearRing></innerBoundaryIs>
        <innerBoundaryIs><LinearRing><coordinates>3,3 3.5,3</coordinates></LinearRing></innerBoundaryIs>
      </Polygon>
      <Polygon><outerBoundaryIs><LinearRing><coordinates>1,1</coordinates></LinearRing></outerBoundaryIs></Polygon>
      <Polygon/>
      <Point><coordinates>9,9 9,9 9,9</coordinates></Point>
      <Polygon><outerBoundaryIs><LinearRing><coordinates>10,10 11,10 11,11</coordinates></LinearRing></outerBoundaryIs></Polygon>
    </MultiGeometry>
  </Placemark>
"##);
        let kml = parse(&text).unwrap();
        assert_eq!(
            kml.stili.len(),
            1,
            "solo i PolyStyle di Style fuori dai placemark"
        );
        assert_eq!(kml.stili["s1"], "7f00ff00");
        assert_eq!(kml.placemarks.len(), 1);
        let p = &kml.placemarks[0];
        assert_eq!(p.nome.as_deref(), Some("ROMA - Zona OMI B1"));
        assert_eq!(
            p.descrizione.as_deref(),
            Some("<b>Semestre 2025/2</b> & altro")
        );
        assert_eq!(p.style_url.as_deref(), Some("#s1"));
        assert_eq!(p.link_zona.as_deref(), Some("RM00000001"));
        assert_eq!(p.cod_com.as_deref(), Some("h501"));
        assert_eq!(p.cod_zona.as_deref(), Some("B1"));
        let m = |v: &[i32]| v.iter().map(|x| x * 1_000_000).collect::<Vec<_>>();
        assert_eq!(
            p.geometria.polygons,
            vec![
                vec![
                    m(&[0, 0, 4, 0, 4, 4, 0, 4, 0, 0]),
                    m(&[1, 1, 2, 1, 2, 2, 1, 1])
                ],
                vec![m(&[10, 10, 11, 10, 11, 11])],
            ],
            "anelli con meno di 3 punti scartati, poligoni senza anelli rimossi, Point ignorato"
        );
        assert_eq!(kml.semestre(), Some(20252));
    }

    #[test]
    fn placemark_senza_geometria_e_vuoto() {
        let kml = parse(&doc("<Placemark/><Placemark><name>N</name></Placemark>")).unwrap();
        assert_eq!(kml.placemarks.len(), 2);
        assert!(
            kml.placemarks
                .iter()
                .all(|p| p.geometria.polygons.is_empty())
        );
        assert_eq!(kml.semestre(), None);
    }

    #[test]
    fn errori_di_formato() {
        let casi = [
            (doc("<Placemark><name>A</nome></Placemark>"), "ill-formed"),
            (
                doc("<Placemark><name>&bogus;</name></Placemark>"),
                "entità sconosciuta: &bogus;",
            ),
            (
                doc("<Placemark><Data name=\"&bogus;\"/></Placemark>"),
                "bogus",
            ),
            (
                doc("<Placemark><name>&#1;</name></Placemark>"),
                "carattere non valido",
            ),
            (format!("{INTESTAZIONE}<Placemark>"), "elementi non chiusi"),
            (
                format!("{}<kml/>", doc("")),
                "contenuto dopo l'elemento radice",
            ),
            (
                format!("{} testo", doc("")),
                "testo fuori dall'elemento radice",
            ),
            (doc("<Placemark><name>a\u{1}b</name></Placemark>"), "U+0001"),
            (doc("</Folder>"), "ill-formed"),
        ];
        for (text, atteso) in casi {
            let m = messaggio(parse(&text));
            assert!(m.starts_with("XML non valido (byte "), "{m}");
            assert!(m.contains(atteso), "atteso «{atteso}» in «{m}»");
        }
        // Commenti, istruzioni e spazi dopo la radice sono ammessi; la DTD si ignora.
        let ok = format!("<!DOCTYPE kml>{}\n<!-- fine --><?pi x?>\n", doc(""));
        assert!(parse(&ok).is_ok());
    }

    #[test]
    fn recupero_dei_malformati() {
        let text = doc(&format!(
            "{}<Placemark><name>rotto</nome></Placemark>\n{}<Placemark><name>senza fine</name>",
            placemark("A - Zona OMI B1", "0,0 1,0 1,1 0,0"),
            placemark("B - Zona OMI B2", "2,2 3,2 3,3 2,2"),
        ));
        assert!(parse(&text).is_err());
        let kml = read_kml(&text).unwrap();
        let nomi: Vec<_> = kml
            .placemarks
            .iter()
            .filter_map(|p| p.nome.as_deref())
            .collect();
        assert_eq!(nomi, ["A - Zona OMI B1", "B - Zona OMI B2"]);
        assert_eq!(kml.stili["s1"], "7f00ff00", "stili ricostruiti dai pezzi");
        let (msg, scartati) = kml.recupero.unwrap();
        assert_eq!(scartati, 2, "un placemark rotto e uno senza chiusura");
        assert!(msg.starts_with("XML non valido"), "{msg}");
    }

    #[test]
    fn malformato_senza_placemark() {
        let text = doc("<Folder></Document></kml>");
        assert!(read_kml(&text).is_err());
        assert!(read_kml(&doc("")).unwrap().recupero.is_none());
    }
}
