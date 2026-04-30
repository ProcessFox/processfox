//! DOCX → HTML conversion for the preview pane. Walks `word/document.xml`
//! event-stream and emits a sanitized subset of HTML covering the formatting
//! end-users actually expect to see in a preview: headings, paragraphs with
//! bold/italic/underline, bullet lists, and basic tables.
//!
//! This is intentionally a different pass from `core::tool::tools::read_docx`
//! (which extracts plain text for LLM consumption). Mixing the two would
//! couple two unrelated concerns; the walker logic differs enough that
//! sharing wouldn't actually save much code.
//!
//! HTML produced here is rendered inside an `<iframe sandbox>` so even if
//! we slip in an attacker-controlled string, scripts cannot execute. We
//! still escape user text defensively.

use std::io::Read;
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;

use crate::core::error::{CoreError, CoreResult};

/// Hard cap on emitted HTML. Realistic upper bound for a hundred-page
/// document is ~500 KB; 2 MB leaves room for outliers without freezing the
/// renderer on something pathological.
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

pub fn docx_to_html(path: &Path) -> CoreResult<String> {
    let xml = read_document_xml(path)?;
    let body = walk(&xml)?;
    Ok(body)
}

fn read_document_xml(path: &Path) -> CoreResult<String> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| CoreError::Llm(format!("DOCX kein gültiges ZIP: {e}")))?;
    let mut entry = zip
        .by_name("word/document.xml")
        .map_err(|e| CoreError::Llm(format!("word/document.xml nicht gefunden: {e}")))?;
    let mut xml = String::new();
    entry.read_to_string(&mut xml)?;
    Ok(xml)
}

#[derive(Default)]
struct State {
    /// Final HTML being assembled.
    out: String,

    /// Are we inside `<w:pPr>` (paragraph properties block)?
    in_para_props: bool,
    /// Are we inside `<w:rPr>` (run properties block)?
    in_run_props: bool,
    /// Are we inside `<w:t>` (text element of a run)?
    in_text: bool,

    /// Current paragraph's `pStyle` value if any (e.g. "Heading1", "Title").
    para_style: Option<String>,
    /// Does the current paragraph have a `<w:numPr>` (numbering/list)?
    para_is_list: bool,

    /// Are we currently inside a `<ul>` block? Used to fold consecutive
    /// list paragraphs into a single list and close it on transition.
    list_open: bool,

    /// Are we inside a paragraph that has been written into the current row's
    /// active cell? Tracked so the open-cell logic only runs once per `<w:tc>`.
    in_cell: bool,

    /// Current run's accumulated, escaped HTML text. Flushed when `</w:r>`
    /// is seen so we can wrap it in `<strong>`/`<em>`/`<u>` based on the run
    /// properties — those can appear in any order before the text element.
    run_html: String,
    run_bold: bool,
    run_italic: bool,
    run_underline: bool,

    /// Buffered HTML for the current paragraph. Holds runs until we know
    /// whether to wrap them in `<p>`, `<h1..6>` or `<li>`.
    para_html: String,
}

fn walk(xml: &str) -> CoreResult<String> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut s = State::default();

    loop {
        if s.out.len() > MAX_HTML_BYTES {
            s.out
                .push_str("\n<p><em>[Vorschau gekürzt — Dokument ist sehr groß.]</em></p>");
            break;
        }
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let qname = e.name();
                handle_start(&mut s, local_name(qname.as_ref()), e);
            }
            Ok(Event::Empty(ref e)) => {
                let qname = e.name();
                handle_empty(&mut s, local_name(qname.as_ref()), e);
            }
            Ok(Event::Text(e)) if s.in_text => {
                let raw = e.unescape().unwrap_or_default();
                s.run_html.push_str(&escape_html(&raw));
            }
            Ok(Event::End(ref e)) => {
                let qname = e.name();
                handle_end(&mut s, local_name(qname.as_ref()));
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(CoreError::Llm(format!("DOCX-XML-Parse-Fehler: {e}"))),
            _ => {}
        }
        buf.clear();
    }

    // Close any list still open at EOF.
    if s.list_open {
        s.out.push_str("</ul>");
    }
    Ok(s.out)
}

fn handle_start(s: &mut State, name: &[u8], _e: &BytesStart) {
    match name {
        b"p" => {
            // Reset per-paragraph state.
            s.para_style = None;
            s.para_is_list = false;
            s.para_html.clear();
        }
        b"pPr" => s.in_para_props = true,
        b"numPr" if s.in_para_props => {
            s.para_is_list = true;
        }
        b"r" => {
            s.run_bold = false;
            s.run_italic = false;
            s.run_underline = false;
            s.run_html.clear();
        }
        b"rPr" => s.in_run_props = true,
        b"t" => s.in_text = true,
        b"tbl" => {
            close_list_if_open(s);
            s.out.push_str("<table>");
        }
        b"tr" => s.out.push_str("<tr>"),
        b"tc" => {
            s.out.push_str("<td>");
            s.in_cell = true;
        }
        _ => {}
    }
}

fn handle_empty(s: &mut State, name: &[u8], e: &BytesStart) {
    match name {
        b"pStyle" if s.in_para_props => {
            s.para_style = attr_val(e, b"w:val").or_else(|| attr_val(e, b"val"));
        }
        b"numPr" if s.in_para_props => s.para_is_list = true,
        b"b" if s.in_run_props && !is_disabled_toggle(e) => {
            s.run_bold = true;
        }
        b"i" if s.in_run_props && !is_disabled_toggle(e) => {
            s.run_italic = true;
        }
        b"u" if s.in_run_props => {
            // `<w:u w:val="none">` disables underline; anything else (or no
            // attr) enables it.
            let val = attr_val(e, b"w:val")
                .or_else(|| attr_val(e, b"val"))
                .unwrap_or_default();
            s.run_underline = val != "none";
        }
        b"br" => s.run_html.push_str("<br>"),
        b"tab" => s.run_html.push_str("&nbsp;&nbsp;&nbsp;&nbsp;"),
        _ => {}
    }
}

fn handle_end(s: &mut State, name: &[u8]) {
    match name {
        b"t" => s.in_text = false,
        b"r" => flush_run(s),
        b"rPr" => s.in_run_props = false,
        b"pPr" => s.in_para_props = false,
        b"p" => flush_paragraph(s),
        b"tc" => {
            s.out.push_str("</td>");
            s.in_cell = false;
        }
        b"tr" => s.out.push_str("</tr>"),
        b"tbl" => s.out.push_str("</table>"),
        _ => {}
    }
}

fn flush_run(s: &mut State) {
    if s.run_html.is_empty() {
        return;
    }
    let mut wrapped = std::mem::take(&mut s.run_html);
    if s.run_underline {
        wrapped = format!("<u>{wrapped}</u>");
    }
    if s.run_italic {
        wrapped = format!("<em>{wrapped}</em>");
    }
    if s.run_bold {
        wrapped = format!("<strong>{wrapped}</strong>");
    }
    s.para_html.push_str(&wrapped);
}

fn flush_paragraph(s: &mut State) {
    let body = std::mem::take(&mut s.para_html);

    // List item: open the list if we're not in one yet.
    if s.para_is_list {
        if !s.list_open {
            s.out.push_str("<ul>");
            s.list_open = true;
        }
        s.out.push_str("<li>");
        s.out.push_str(&body);
        s.out.push_str("</li>");
        return;
    }

    // Non-list paragraph: close any open list first.
    close_list_if_open(s);

    let tag = heading_tag_for(s.para_style.as_deref());
    let body_or_break = if body.is_empty() {
        // Empty paragraph in a non-heading context is just visual spacing —
        // skip to keep the preview compact. Keep empty headings as-is in
        // case someone made an empty section title.
        if tag == "p" {
            return;
        }
        "&nbsp;".to_string()
    } else {
        body
    };
    s.out.push_str(&format!("<{tag}>{body_or_break}</{tag}>"));
}

fn close_list_if_open(s: &mut State) {
    if s.list_open {
        s.out.push_str("</ul>");
        s.list_open = false;
    }
}

/// Map a Word paragraph style name to an HTML heading tag, defaulting to
/// `p` for normal text. Matches the well-known names case-insensitively
/// and tolerates the spaced form ("Heading 1") seen in some templates.
fn heading_tag_for(style: Option<&str>) -> &'static str {
    let Some(raw) = style else {
        return "p";
    };
    let normalized: String = raw
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_lowercase())
        .collect();
    match normalized.as_str() {
        "title" | "heading1" | "heading01" => "h1",
        "subtitle" | "heading2" | "heading02" => "h2",
        "heading3" | "heading03" => "h3",
        "heading4" | "heading04" => "h4",
        "heading5" | "heading05" => "h5",
        "heading6" | "heading06" => "h6",
        _ => "p",
    }
}

fn is_disabled_toggle(e: &BytesStart) -> bool {
    let val = attr_val(e, b"w:val").or_else(|| attr_val(e, b"val"));
    matches!(val.as_deref(), Some("false") | Some("0"))
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

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_heading_styles() {
        assert_eq!(heading_tag_for(Some("Heading1")), "h1");
        assert_eq!(heading_tag_for(Some("Heading 1")), "h1");
        assert_eq!(heading_tag_for(Some("heading2")), "h2");
        assert_eq!(heading_tag_for(Some("Title")), "h1");
        assert_eq!(heading_tag_for(Some("Subtitle")), "h2");
        assert_eq!(heading_tag_for(None), "p");
        assert_eq!(heading_tag_for(Some("Normal")), "p");
    }

    #[test]
    fn escapes_html_chars() {
        assert_eq!(escape_html("a<b>&\""), "a&lt;b&gt;&amp;&quot;");
    }

    #[test]
    fn walks_simple_paragraph() {
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p>
      <w:pPr><w:pStyle w:val="Heading1"/></w:pPr>
      <w:r><w:t>Hello</w:t></w:r>
    </w:p>
    <w:p>
      <w:r><w:rPr><w:b/></w:rPr><w:t>bold</w:t></w:r>
      <w:r><w:t> </w:t></w:r>
      <w:r><w:rPr><w:i/></w:rPr><w:t>italic</w:t></w:r>
    </w:p>
  </w:body>
</w:document>"#;
        let html = walk(xml).unwrap();
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn folds_consecutive_list_items() {
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>One</w:t></w:r></w:p>
    <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Two</w:t></w:r></w:p>
    <w:p><w:r><w:t>After</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let html = walk(xml).unwrap();
        // Single <ul> wrapping both items, closed before the next paragraph.
        assert!(html.contains("<ul><li>One</li><li>Two</li></ul>"));
        assert!(html.contains("<p>After</p>"));
    }

    #[test]
    fn disabled_bold_is_not_emitted() {
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:rPr><w:b w:val="false"/></w:rPr><w:t>plain</w:t></w:r></w:p>
  </w:body>
</w:document>"#;
        let html = walk(xml).unwrap();
        assert!(!html.contains("<strong>"));
        assert!(html.contains(">plain<"));
    }
}
