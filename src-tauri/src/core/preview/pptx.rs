//! PPTX preview — text-only outline. Reading visual layout would require
//! either an external converter (LibreOffice headless) or a full DrawingML
//! renderer; both blow past v1's "lokal-zuerst, schlank" budget. Instead
//! we walk the slide XML and collect:
//!
//!   - the title (any shape whose placeholder type is `title` / `ctrTitle`)
//!   - body paragraphs (every other text-bearing shape on the slide)
//!   - speaker notes (paragraphs from the matching `notesSlide{N}.xml`,
//!     minus the auto-generated slide-number placeholder)
//!
//! The result is enough for the user to understand "what's on this deck"
//! in the preview pane without opening PowerPoint.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::Serialize;

use crate::core::error::{CoreError, CoreResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlidePreview {
    /// 1-based slide number.
    pub index: usize,
    pub title: Option<String>,
    pub body: Vec<String>,
    pub notes: Vec<String>,
}

pub fn pptx_preview(path: &Path) -> CoreResult<Vec<SlidePreview>> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| CoreError::Llm(format!("PPTX kein gültiges ZIP: {e}")))?;

    // Find slide and notes XMLs by numeric suffix so we keep deck order.
    // BTreeMap on the parsed index gives us 1, 2, 3, … 10 (not 1, 10, 2).
    let mut slide_files: BTreeMap<usize, String> = BTreeMap::new();
    let mut notes_files: BTreeMap<usize, String> = BTreeMap::new();
    for i in 0..zip.len() {
        let name = zip.by_index(i).map_err(zip_err)?.name().to_string();
        if let Some(idx) = parse_index(&name, "ppt/slides/slide", ".xml") {
            slide_files.insert(idx, name);
        } else if let Some(idx) = parse_index(&name, "ppt/notesSlides/notesSlide", ".xml") {
            notes_files.insert(idx, name);
        }
    }

    let mut out: Vec<SlidePreview> = Vec::with_capacity(slide_files.len());
    for (idx, slide_name) in &slide_files {
        let xml = read_entry(&mut zip, slide_name)?;
        let (title, body) = parse_slide(&xml, false)?;

        let notes = match notes_files.get(idx) {
            Some(notes_name) => {
                let nxml = read_entry(&mut zip, notes_name)?;
                let (_, paragraphs) = parse_slide(&nxml, true)?;
                paragraphs
            }
            None => Vec::new(),
        };

        out.push(SlidePreview {
            index: *idx,
            title,
            body,
            notes,
        });
    }
    Ok(out)
}

fn zip_err(e: zip::result::ZipError) -> CoreError {
    CoreError::Llm(format!("PPTX Zip-Fehler: {e}"))
}

fn read_entry(zip: &mut zip::ZipArchive<std::fs::File>, name: &str) -> CoreResult<String> {
    let mut entry = zip.by_name(name).map_err(zip_err)?;
    let mut s = String::new();
    entry.read_to_string(&mut s)?;
    Ok(s)
}

/// Extract `(title, body_paragraphs)` from a slide- or notes-XML.
///
/// `is_notes`: notes slides include an auto-generated slide-number text
/// box (placeholder type `sldNum`); we drop it so the user only sees what
/// they actually wrote in the notes pane.
fn parse_slide(xml: &str, is_notes: bool) -> CoreResult<(Option<String>, Vec<String>)> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();

    let mut title: Option<String> = None;
    let mut body: Vec<String> = Vec::new();

    // Per-shape state.
    let mut in_shape = false;
    let mut shape_ph_type: Option<String> = None;
    let mut shape_paragraphs: Vec<String> = Vec::new();

    // Per-paragraph state (within a shape's txBody).
    let mut in_text_body = false;
    let mut current_para = String::new();
    let mut in_text = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let qname = e.name();
                match local_name(qname.as_ref()) {
                    b"sp" => {
                        in_shape = true;
                        shape_ph_type = None;
                        shape_paragraphs.clear();
                    }
                    b"txBody" if in_shape => in_text_body = true,
                    b"p" if in_text_body => current_para.clear(),
                    b"t" if in_text_body => in_text = true,
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let qname = e.name();
                match local_name(qname.as_ref()) {
                    b"ph" if in_shape => {
                        shape_ph_type = attr_val(e, b"type");
                    }
                    b"br" if in_text_body => current_para.push('\n'),
                    _ => {}
                }
            }
            Ok(Event::Text(t)) if in_text => {
                let raw = t.unescape().unwrap_or_default();
                current_para.push_str(&raw);
            }
            Ok(Event::End(ref e)) => {
                let qname = e.name();
                match local_name(qname.as_ref()) {
                    b"t" => in_text = false,
                    b"p" if in_text_body => {
                        let trimmed = current_para.trim();
                        if !trimmed.is_empty() {
                            shape_paragraphs.push(trimmed.to_string());
                        }
                        current_para.clear();
                    }
                    b"txBody" => in_text_body = false,
                    b"sp" => {
                        if !shape_paragraphs.is_empty() {
                            let ph = shape_ph_type.as_deref();
                            let is_title = matches!(ph, Some("title") | Some("ctrTitle"));
                            let is_slide_num = ph == Some("sldNum");
                            if is_title && title.is_none() {
                                title = Some(shape_paragraphs.join(" "));
                            } else if is_notes && is_slide_num {
                                // skip auto slide-number placeholder
                            } else {
                                body.append(&mut shape_paragraphs);
                            }
                        }
                        in_shape = false;
                        shape_ph_type = None;
                        shape_paragraphs.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(CoreError::Llm(format!("PPTX-XML-Parse-Fehler: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    Ok((title, body))
}

fn parse_index(name: &str, prefix: &str, suffix: &str) -> Option<usize> {
    let rest = name.strip_prefix(prefix)?.strip_suffix(suffix)?;
    rest.parse().ok()
}

fn attr_val(e: &BytesStart, name: &[u8]) -> Option<String> {
    e.attributes()
        .filter_map(|a| a.ok())
        .find(|a| a.key.as_ref() == name)
        .map(|a| String::from_utf8_lossy(a.value.as_ref()).into_owned())
}

fn local_name(qname: &[u8]) -> &[u8] {
    qname
        .iter()
        .position(|&b| b == b':')
        .map(|i| &qname[i + 1..])
        .unwrap_or(qname)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_title_and_bullets() {
        let xml = r#"<?xml version="1.0"?>
<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
       xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree>
    <p:sp>
      <p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
      <p:txBody>
        <a:p><a:r><a:t>Quartalsbericht</a:t></a:r></a:p>
      </p:txBody>
    </p:sp>
    <p:sp>
      <p:nvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
      <p:txBody>
        <a:p><a:r><a:t>Punkt eins</a:t></a:r></a:p>
        <a:p><a:r><a:t>Punkt zwei</a:t></a:r></a:p>
      </p:txBody>
    </p:sp>
  </p:spTree></p:cSld>
</p:sld>"#;
        let (title, body) = parse_slide(xml, false).unwrap();
        assert_eq!(title.as_deref(), Some("Quartalsbericht"));
        assert_eq!(body, vec!["Punkt eins", "Punkt zwei"]);
    }

    #[test]
    fn drops_slide_number_in_notes() {
        let xml = r#"<?xml version="1.0"?>
<p:notes xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"
         xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:cSld><p:spTree>
    <p:sp>
      <p:nvSpPr><p:nvPr><p:ph type="sldNum" idx="2"/></p:nvPr></p:nvSpPr>
      <p:txBody><a:p><a:r><a:t>3</a:t></a:r></a:p></p:txBody>
    </p:sp>
    <p:sp>
      <p:nvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr>
      <p:txBody><a:p><a:r><a:t>Notiz</a:t></a:r></a:p></p:txBody>
    </p:sp>
  </p:spTree></p:cSld>
</p:notes>"#;
        let (_, body) = parse_slide(xml, true).unwrap();
        assert_eq!(body, vec!["Notiz"]);
    }

    #[test]
    fn slide_index_parsing_keeps_numeric_order() {
        assert_eq!(
            parse_index("ppt/slides/slide10.xml", "ppt/slides/slide", ".xml"),
            Some(10)
        );
        assert_eq!(
            parse_index("ppt/slides/slide1.xml", "ppt/slides/slide", ".xml"),
            Some(1)
        );
        assert_eq!(
            parse_index("ppt/slides/_rels/slide1.xml.rels", "ppt/slides/slide", ".xml"),
            None
        );
    }
}
