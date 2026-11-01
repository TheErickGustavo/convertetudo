use crate::entities::xml_escape;
use crate::model::*;
use crate::write_text::{write_html_fragment, xml_escape_pub};
use crate::zipio::ZipWriter;

pub fn write_docx(doc: &Document) -> Vec<u8> {
    let mut body = String::new();
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        body.push_str(&docx_p(&text_inline(doc.meta.title.clone()), Some("Heading1")));
    }
    write_docx_blocks(&doc.blocks, &mut body);
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1134\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\"/></w:sectPr></w:body></w:document>"
    );
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/>
<Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/>
<Override PartName="/docProps/core.xml" ContentType="application/vnd.openxmlformats-package.core-properties+xml"/>
</Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/>
<Relationship Id="rId2" Type="http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties" Target="docProps/core.xml"/>
</Relationships>"#;
    let word_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>
</Relationships>"#;
    let styles = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:style w:type="paragraph" w:styleId="Normal"><w:name w:val="Normal"/><w:rPr><w:sz w:val="22"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="36"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="28"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:rPr><w:b/><w:sz w:val="24"/></w:rPr></w:style>
</w:styles>"#;
    let core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>",
        xml_escape(&doc.meta.title),
        xml_escape(doc.meta.authors.first().map(|s| s.as_str()).unwrap_or(""))
    );
    let mut z = ZipWriter::new();
    z.add("[Content_Types].xml", content_types.as_bytes(), true);
    z.add("_rels/.rels", rels.as_bytes(), true);
    z.add("word/document.xml", document.as_bytes(), true);
    z.add("word/_rels/document.xml.rels", word_rels.as_bytes(), true);
    z.add("word/styles.xml", styles.as_bytes(), true);
    z.add("docProps/core.xml", core.as_bytes(), true);
    z.finish()
}

fn write_docx_blocks(blocks: &[Block], body: &mut String) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => {
                let style = match level {
                    1 => "Heading1",
                    2 => "Heading2",
                    _ => "Heading3",
                };
                body.push_str(&docx_p(inlines, Some(style)));
            }
            Block::Paragraph { inlines } => body.push_str(&docx_p(inlines, None)),
            Block::List { ordered, items } => {
                for (i, item) in items.iter().enumerate() {
                    let mut line = if *ordered {
                        text_inline(format!("{}. ", i + 1))
                    } else {
                        text_inline("• ".to_string())
                    };
                    for block in item {
                        if let Block::Paragraph { inlines } = block {
                            line.extend(inlines.clone());
                        }
                    }
                    body.push_str(&docx_p(&line, None));
                }
            }
            Block::Quote { blocks } => write_docx_blocks(blocks, body),
            Block::Code { text } => body.push_str(&docx_p(&text_inline(text.clone()), None)),
            Block::Table { rows } => {
                body.push_str("<w:tbl><w:tblPr><w:tblW w:w=\"5000\" w:type=\"pct\"/></w:tblPr>");
                for row in rows {
                    body.push_str("<w:tr>");
                    for c in row {
                        body.push_str("<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr>");
                        body.push_str(&docx_p(&c.inlines, None));
                        body.push_str("</w:tc>");
                    }
                    body.push_str("</w:tr>");
                }
                body.push_str("</w:tbl>");
            }
            Block::Rule => body.push_str(&docx_p(&text_inline("———"), None)),
            Block::Image { alt } => body.push_str(&docx_p(&text_inline(format!("[{alt}]")), None)),
        }
    }
}

fn docx_p(inlines: &[Inline], style: Option<&str>) -> String {
    let mut s = String::from("<w:p>");
    if let Some(style) = style {
        s.push_str(&format!("<w:pPr><w:pStyle w:val=\"{style}\"/></w:pPr>"));
    }
    write_docx_runs(inlines, &mut s, false, false);
    s.push_str("</w:p>");
    s
}

fn write_docx_runs(inlines: &[Inline], s: &mut String, bold: bool, ital: bool) {
    for n in inlines {
        match n {
            Inline::Text(t) | Inline::Code(t) => {
                s.push_str("<w:r>");
                if bold || ital {
                    s.push_str("<w:rPr>");
                    if bold {
                        s.push_str("<w:b/>");
                    }
                    if ital {
                        s.push_str("<w:i/>");
                    }
                    s.push_str("</w:rPr>");
                }
                s.push_str("<w:t xml:space=\"preserve\">");
                s.push_str(&xml_escape(t));
                s.push_str("</w:t></w:r>");
            }
            Inline::Strong(c) => write_docx_runs(c, s, true, ital),
            Inline::Emph(c) => write_docx_runs(c, s, bold, true),
            Inline::Strike(c) | Inline::Sup(c) | Inline::Sub(c) | Inline::Link { children: c, .. } => {
                write_docx_runs(c, s, bold, ital)
            }
            Inline::Break => s.push_str("<w:r><w:br/></w:r>"),
        }
    }
}

pub fn write_odt(doc: &Document) -> Vec<u8> {
    let mut body = String::new();
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        body.push_str(&format!(
            "<text:h text:style-name=\"Heading_20_1\" text:outline-level=\"1\">{}</text:h>",
            xml_escape(&doc.meta.title)
        ));
    }
    write_odt_blocks(&doc.blocks, &mut body);
    let content = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<office:document-content xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:table=\"urn:oasis:names:tc:opendocument:xmlns:table:1.0\" office:version=\"1.2\">\
<office:body><office:text>{body}</office:text></office:body></office:document-content>"
    );
    let manifest = r#"<?xml version="1.0" encoding="UTF-8"?>
<manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0">
<manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.text"/>
<manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/>
<manifest:file-entry manifest:full-path="styles.xml" manifest:media-type="text/xml"/>
<manifest:file-entry manifest:full-path="meta.xml" manifest:media-type="text/xml"/>
</manifest:manifest>"#;
    let styles = r#"<?xml version="1.0" encoding="UTF-8"?>
<office:document-styles xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" office:version="1.2">
<office:styles>
<style:style style:name="Heading_20_1" style:family="paragraph"><style:text-properties fo:font-size="18pt" fo:font-weight="bold"/></style:style>
</office:styles>
</office:document-styles>"#;
    let meta = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
<office:document-meta xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" office:version=\"1.2\"><office:meta><dc:title>{}</dc:title></office:meta></office:document-meta>",
        xml_escape(&doc.meta.title)
    );
    let mimetype = b"application/vnd.oasis.opendocument.text";
    let mut z = ZipWriter::new();
    z.add("mimetype", mimetype, true);
    z.add("META-INF/manifest.xml", manifest.as_bytes(), true);
    z.add("content.xml", content.as_bytes(), true);
    z.add("styles.xml", styles.as_bytes(), true);
    z.add("meta.xml", meta.as_bytes(), true);
    z.finish()
}

fn write_odt_blocks(blocks: &[Block], body: &mut String) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => {
                let lv = (*level).clamp(1, 6);
                body.push_str(&format!(
                    "<text:h text:outline-level=\"{lv}\">{}</text:h>",
                    xml_escape(&plain(inlines))
                ));
            }
            Block::Paragraph { inlines } => {
                body.push_str("<text:p>");
                body.push_str(&odt_inlines(inlines));
                body.push_str("</text:p>");
            }
            Block::List { items, .. } => {
                body.push_str("<text:list>");
                for item in items {
                    body.push_str("<text:list-item>");
                    for block in item {
                        if let Block::Paragraph { inlines } = block {
                            body.push_str("<text:p>");
                            body.push_str(&odt_inlines(inlines));
                            body.push_str("</text:p>");
                        }
                    }
                    body.push_str("</text:list-item>");
                }
                body.push_str("</text:list>");
            }
            Block::Quote { blocks } => write_odt_blocks(blocks, body),
            Block::Code { text } => {
                body.push_str("<text:p>");
                body.push_str(&xml_escape(text));
                body.push_str("</text:p>");
            }
            Block::Table { rows } => {
                body.push_str("<table:table>");
                for row in rows {
                    body.push_str("<table:table-row>");
                    for c in row {
                        body.push_str("<table:table-cell><text:p>");
                        body.push_str(&xml_escape(&plain(&c.inlines)));
                        body.push_str("</text:p></table:table-cell>");
                    }
                    body.push_str("</table:table-row>");
                }
                body.push_str("</table:table>");
            }
            Block::Rule => body.push_str("<text:p>———</text:p>"),
            Block::Image { alt } => {
                body.push_str("<text:p>[");
                body.push_str(&xml_escape(alt));
                body.push_str("]</text:p>");
            }
        }
    }
}

fn odt_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for n in inlines {
        match n {
            Inline::Text(t) | Inline::Code(t) => s.push_str(&xml_escape(t)),
            Inline::Strong(c) => {
                s.push_str("<text:span text:style-name=\"Strong\">");
                s.push_str(&odt_inlines(c));
                s.push_str("</text:span>");
            }
            Inline::Emph(c) => {
                s.push_str("<text:span text:style-name=\"Emphasis\">");
                s.push_str(&odt_inlines(c));
                s.push_str("</text:span>");
            }
            Inline::Link { href, children } => {
                s.push_str("<text:a xlink:href=\"");
                s.push_str(&xml_escape(href));
                s.push_str("\">");
                s.push_str(&odt_inlines(children));
                s.push_str("</text:a>");
            }
            Inline::Strike(c) | Inline::Sup(c) | Inline::Sub(c) => s.push_str(&odt_inlines(c)),
            Inline::Break => s.push_str("<text:line-break/>"),
        }
    }
    s
}

pub fn write_epub(doc: &Document) -> Vec<u8> {
    let title = if doc.meta.title.is_empty() {
        "Documento".to_string()
    } else {
        doc.meta.title.clone()
    };
    let lang = if doc.meta.language.is_empty() {
        "pt".to_string()
    } else {
        doc.meta.language.clone()
    };
    let mut chapters: Vec<(String, String, String)> = Vec::new();
    let mut current_title = title.clone();
    let mut current: Vec<Block> = Vec::new();
    let flush = |chapters: &mut Vec<(String, String, String)>, title: &str, blocks: &mut Vec<Block>| {
        if blocks.is_empty() {
            return;
        }
        let n = chapters.len() + 1;
        let href = format!("chap_{n}.xhtml");
        let mut fake = Document::new("epub");
        fake.blocks = std::mem::take(blocks);
        let inner = write_html_fragment(&fake);
        let xhtml = format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"{lang}\"><head><title>{}</title><meta charset=\"utf-8\"/></head><body>{inner}</body></html>",
            xml_escape(title)
        );
        chapters.push((href, title.to_string(), xhtml));
    };
    for b in &doc.blocks {
        if let Block::Heading { level: 1, inlines } = b {
            flush(&mut chapters, &current_title, &mut current);
            current_title = plain(inlines);
            current.push(b.clone());
        } else {
            current.push(b.clone());
        }
    }
    flush(&mut chapters, &current_title, &mut current);
    if chapters.is_empty() {
        let mut fake = Document::new("epub");
        fake.meta.title = title.clone();
        fake.blocks.push(paragraph(""));
        chapters.push((
            "chap_1.xhtml".into(),
            title.clone(),
            format!(
                "<?xml version=\"1.0\" encoding=\"utf-8\"?><html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>{}</title></head><body><p></p></body></html>",
                xml_escape(&title)
            ),
        ));
    }
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut nav_items = String::new();
    for (i, (href, ct, _)) in chapters.iter().enumerate() {
        let id = format!("c{}", i + 1);
        manifest.push_str(&format!(
            "<item id=\"{id}\" href=\"{href}\" media-type=\"application/xhtml+xml\"/>"
        ));
        spine.push_str(&format!("<itemref idref=\"{id}\"/>"));
        nav_items.push_str(&format!(
            "<li><a href=\"{href}\">{}</a></li>",
            xml_escape(ct)
        ));
    }
    let authors = doc.meta.authors.join(", ");
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<package xmlns=\"http://www.idpf.org/2007/opf\" unique-identifier=\"uid\" version=\"3.0\">\
<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
<dc:identifier id=\"uid\">urn:uuid:convertetudo</dc:identifier>\
<dc:title>{}</dc:title>\
<dc:language>{}</dc:language>\
<dc:creator>{}</dc:creator>\
<meta property=\"dcterms:modified\">2026-01-01T00:00:00Z</meta>\
</metadata>\
<manifest>\
<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\
{manifest}\
</manifest>\
<spine>{spine}</spine>\
</package>",
        xml_escape(&title),
        xml_escape(&lang),
        xml_escape(&authors)
    );
    let nav = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\">\
<head><title>Sumário</title></head><body><nav epub:type=\"toc\"><h1>Sumário</h1><ol>{nav_items}</ol></nav></body></html>"
    );
    let mut z = ZipWriter::new();
    z.add("mimetype", b"application/epub+zip", true);
    z.add(
        "META-INF/container.xml",
        br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        true,
    );
    z.add("OEBPS/content.opf", opf.as_bytes(), true);
    z.add("OEBPS/nav.xhtml", nav.as_bytes(), true);
    for (href, _, xhtml) in &chapters {
        z.add(&format!("OEBPS/{href}"), xhtml.as_bytes(), true);
    }
    z.finish()
}

pub fn write_fb2(doc: &Document) -> Vec<u8> {
    let title = if doc.meta.title.is_empty() { "Documento" } else { &doc.meta.title };
    let mut body = String::new();
    body.push_str("<section>");
    for b in &doc.blocks {
        match b {
            Block::Heading { inlines, .. } => {
                body.push_str("<title><p>");
                body.push_str(&xml_escape(&plain(inlines)));
                body.push_str("</p></title>");
            }
            Block::Paragraph { inlines } => {
                body.push_str("<p>");
                body.push_str(&xml_escape(&plain(inlines)));
                body.push_str("</p>");
            }
            Block::List { items, .. } => {
                for item in items {
                    for block in item {
                        if let Block::Paragraph { inlines } = block {
                            body.push_str("<p>• ");
                            body.push_str(&xml_escape(&plain(inlines)));
                            body.push_str("</p>");
                        }
                    }
                }
            }
            Block::Quote { blocks } => {
                body.push_str("<cite>");
                for block in blocks {
                    if let Block::Paragraph { inlines } = block {
                        body.push_str("<p>");
                        body.push_str(&xml_escape(&plain(inlines)));
                        body.push_str("</p>");
                    }
                }
                body.push_str("</cite>");
            }
            Block::Code { text } => {
                body.push_str("<p>");
                body.push_str(&xml_escape(text));
                body.push_str("</p>");
            }
            _ => {}
        }
    }
    body.push_str("</section>");
    let author = doc.meta.authors.first().map(|s| s.as_str()).unwrap_or("");
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\
<FictionBook xmlns=\"http://www.gribuser.ru/xml/fictionbook/2.0\">\
<description><title-info><book-title>{}</book-title><author><first-name>{}</first-name></author><lang>{}</lang></title-info></description>\
<body>{body}</body></FictionBook>",
        xml_escape(title),
        xml_escape(author),
        xml_escape(if doc.meta.language.is_empty() { "pt" } else { &doc.meta.language })
    );
    xml.into_bytes()
}

pub fn write_xlsx(doc: &Document) -> Vec<u8> {
    let mut rows: Vec<Vec<String>> = Vec::new();
    fn grab(blocks: &[Block], rows: &mut Vec<Vec<String>>) {
        for b in blocks {
            match b {
                Block::Table { rows: table } => {
                    for r in table {
                        rows.push(r.iter().map(|c| plain(&c.inlines)).collect());
                    }
                }
                Block::Paragraph { inlines } | Block::Heading { inlines, .. } => {
                    rows.push(vec![plain(inlines)]);
                }
                Block::List { items, .. } => {
                    for it in items {
                        grab(it, rows);
                    }
                }
                Block::Quote { blocks } => grab(blocks, rows),
                Block::Code { text } => rows.push(vec![text.clone()]),
                _ => {}
            }
        }
    }
    grab(&doc.blocks, &mut rows);
    if rows.is_empty() {
        rows.push(vec![String::new()]);
    }
    let mut sheet = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    for (ri, row) in rows.iter().enumerate() {
        sheet.push_str(&format!("<row r=\"{}\">", ri + 1));
        for (ci, cell) in row.iter().enumerate() {
            let col = col_name(ci);
            sheet.push_str(&format!(
                "<c r=\"{col}{}\" t=\"inlineStr\"><is><t>{}</t></is></c>",
                ri + 1,
                xml_escape(cell)
            ));
        }
        sheet.push_str("</row>");
    }
    sheet.push_str("</sheetData></worksheet>");
    let content_types = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
<Default Extension="xml" ContentType="application/xml"/>
<Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>
<Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/>
</Types>"#;
    let rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/>
</Relationships>"#;
    let wb_rels = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
<Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/>
</Relationships>"#;
    let workbook = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Documento" sheetId="1" r:id="rId1"/></sheets></workbook>"#;
    let mut z = ZipWriter::new();
    z.add("[Content_Types].xml", content_types.as_bytes(), true);
    z.add("_rels/.rels", rels.as_bytes(), true);
    z.add("xl/workbook.xml", workbook.as_bytes(), true);
    z.add("xl/_rels/workbook.xml.rels", wb_rels.as_bytes(), true);
    z.add("xl/worksheets/sheet1.xml", sheet.as_bytes(), true);
    z.finish()
}

fn col_name(mut i: usize) -> String {
    let mut s = String::new();
    loop {
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    s
}

pub fn write_pdf(doc: &Document) -> Vec<u8> {
    let mut lines: Vec<(u8, String)> = Vec::new();
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        lines.push((1, doc.meta.title.clone()));
    }
    if !doc.meta.authors.is_empty() {
        lines.push((9, doc.meta.authors.join(", ")));
    }
    collect_pdf_lines(&doc.blocks, &mut lines);
    if lines.is_empty() {
        lines.push((0, String::new()));
    }
    let mut wrapped: Vec<(u8, String)> = Vec::new();
    for (kind, text) in lines {
        let width = match kind {
            1 => 42,
            2 => 52,
            9 => 70,
            _ => 88,
        };
        if text.is_empty() {
            wrapped.push((kind, String::new()));
            continue;
        }
        for line in wrap_width(&text, width) {
            wrapped.push((kind, line));
        }
    }
    let mut pages: Vec<Vec<(u8, String)>> = Vec::new();
    let mut page: Vec<(u8, String)> = Vec::new();
    let mut used = 0i32;
    let page_budget = 680i32;
    for item in wrapped {
        let h = match item.0 {
            1 => 28,
            2 => 22,
            _ => 15,
        };
        if used + h > page_budget && !page.is_empty() {
            pages.push(std::mem::take(&mut page));
            used = 0;
        }
        used += h;
        page.push(item);
    }
    if !page.is_empty() {
        pages.push(page);
    }
    if pages.is_empty() {
        pages.push(vec![(0, String::new())]);
    }

    let mut objects: Vec<Vec<u8>> = Vec::new();
    // 1 catalog, 2 pages, then each page + content, fonts
    // We'll assign ids after building content streams.
    let n_pages = pages.len();
    // ids: 1 catalog, 2 pages, 3 font regular, 4 font bold, then pairs of page/content
    let font_regular = 3;
    let font_bold = 4;
    let first_page_obj = 5;
    let mut kids = String::new();
    let mut content_objs = Vec::new();
    let mut page_objs = Vec::new();
    for (pi, page) in pages.iter().enumerate() {
        let page_id = first_page_obj + pi * 2;
        let content_id = page_id + 1;
        kids.push_str(&format!("{page_id} 0 R "));
        let mut stream = String::new();
        stream.push_str("BT\n");
        let mut y = 780i32;
        for (kind, text) in page {
            let (font, size, dy) = match kind {
                1 => (font_bold, 18, 28),
                2 => (font_bold, 14, 22),
                9 => (font_regular, 11, 16),
                _ => (font_regular, 11, 15),
            };
            if y < 56 {
                break;
            }
            let fname = if font == font_bold { "F2" } else { "F1" };
            stream.push_str(&format!("/{fname} {size} Tf\n1 0 0 1 56 {y} Tm\n"));
            stream.push('(');
            stream.push_str(&pdf_literal(text));
            stream.push_str(") Tj\n");
            y -= dy;
        }
        stream.push_str(&format!("/F1 9 Tf\n1 0 0 1 520 36 Tm\n({}/{}) Tj\n", pi + 1, n_pages));
        stream.push_str("ET\n");
        let bytes = stream.into_bytes();
        content_objs.push((content_id, bytes));
        let page_dict = format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents {content_id} 0 R /Resources << /Font << /F1 {font_regular} 0 R /F2 {font_bold} 0 R >> >> >>"
        );
        page_objs.push((page_id, page_dict));
    }
    // Build object list in id order. Missing ids filled.
    let max_id = first_page_obj + n_pages * 2 - 1;
    let mut raw_objs = vec![Vec::new(); max_id];
    raw_objs[0] = b"<< /Type /Catalog /Pages 2 0 R >>".to_vec();
    raw_objs[1] = format!("<< /Type /Pages /Count {n_pages} /Kids [{kids}] >>").into_bytes();
    raw_objs[2] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman /Encoding /WinAnsiEncoding >>".to_vec();
    raw_objs[3] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Times-Bold /Encoding /WinAnsiEncoding >>".to_vec();
    for (id, dict) in page_objs {
        raw_objs[id - 1] = dict.into_bytes();
    }
    for (id, bytes) in content_objs {
        let mut obj = format!("<< /Length {} >>\nstream\n", bytes.len()).into_bytes();
        obj.extend_from_slice(&bytes);
        obj.extend_from_slice(b"endstream");
        raw_objs[id - 1] = obj;
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::new();
    for (i, obj) in raw_objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(obj);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", raw_objs.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            raw_objs.len() + 1
        )
        .as_bytes(),
    );
    let _ = xml_escape_pub;
    out
}

fn collect_pdf_lines(blocks: &[Block], lines: &mut Vec<(u8, String)>) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => lines.push(((*level).min(2), plain(inlines))),
            Block::Paragraph { inlines } => lines.push((0, plain(inlines))),
            Block::List { ordered, items } => {
                for (i, item) in items.iter().enumerate() {
                    for block in item {
                        if let Block::Paragraph { inlines } = block {
                            let marker = if *ordered { format!("{}. ", i + 1) } else { "• ".into() };
                            lines.push((0, format!("{marker}{}", plain(inlines))));
                        } else {
                            collect_pdf_lines(std::slice::from_ref(block), lines);
                        }
                    }
                }
            }
            Block::Quote { blocks } => collect_pdf_lines(blocks, lines),
            Block::Code { text } => {
                for line in text.lines() {
                    lines.push((0, line.to_string()));
                }
            }
            Block::Table { rows } => {
                for row in rows {
                    let t = row.iter().map(|c| plain(&c.inlines)).collect::<Vec<_>>().join("  |  ");
                    lines.push((0, t));
                }
            }
            Block::Rule => lines.push((0, "———".into())),
            Block::Image { alt } => {
                if !alt.is_empty() {
                    lines.push((0, format!("[{alt}]")));
                }
            }
        }
    }
}

fn wrap_width(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if cur.is_empty() {
            cur.push_str(word);
        } else if cur.chars().count() + 1 + word.chars().count() <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            out.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

fn pdf_literal(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let b = to_winansi(c);
        match b {
            b'\\' => out.push_str("\\\\"),
            b'(' => out.push_str("\\("),
            b')' => out.push_str("\\)"),
            b'\n' => out.push_str("\\n"),
            b if b < 32 => out.push_str(&format!("\\{:03o}", b)),
            b if b > 127 => out.push_str(&format!("\\{:03o}", b)),
            b => out.push(b as char),
        }
    }
    out
}

fn to_winansi(c: char) -> u8 {
    let n = c as u32;
    if n < 128 {
        return n as u8;
    }
    const MAP: &[(char, u8)] = &[
        ('€', 128), ('‚', 130), ('ƒ', 131), ('„', 132), ('…', 133), ('†', 134), ('‡', 135),
        ('ˆ', 136), ('‰', 137), ('Š', 138), ('‹', 139), ('Œ', 140), ('Ž', 142), ('‘', 145),
        ('’', 146), ('“', 147), ('”', 148), ('•', 149), ('–', 150), ('—', 151), ('˜', 152),
        ('™', 153), ('š', 154), ('›', 155), ('œ', 156), ('ž', 158), ('Ÿ', 159), ('\u{00a0}', 160),
        ('¡', 161), ('¢', 162), ('£', 163), ('¤', 164), ('¥', 165), ('¦', 166), ('§', 167),
        ('¨', 168), ('©', 169), ('ª', 170), ('«', 171), ('¬', 172), ('®', 174), ('¯', 175),
        ('°', 176), ('±', 177), ('²', 178), ('³', 179), ('´', 180), ('µ', 181), ('¶', 182),
        ('·', 183), ('¸', 184), ('¹', 185), ('º', 186), ('»', 187), ('¼', 188), ('½', 189),
        ('¾', 190), ('¿', 191), ('À', 192), ('Á', 193), ('Â', 194), ('Ã', 195), ('Ä', 196),
        ('Å', 197), ('Æ', 198), ('Ç', 199), ('È', 200), ('É', 201), ('Ê', 202), ('Ë', 203),
        ('Ì', 204), ('Í', 205), ('Î', 206), ('Ï', 207), ('Ð', 208), ('Ñ', 209), ('Ò', 210),
        ('Ó', 211), ('Ô', 212), ('Õ', 213), ('Ö', 214), ('×', 215), ('Ø', 216), ('Ù', 217),
        ('Ú', 218), ('Û', 219), ('Ü', 220), ('Ý', 221), ('Þ', 222), ('ß', 223), ('à', 224),
        ('á', 225), ('â', 226), ('ã', 227), ('ä', 228), ('å', 229), ('æ', 230), ('ç', 231),
        ('è', 232), ('é', 233), ('ê', 234), ('ë', 235), ('ì', 236), ('í', 237), ('î', 238),
        ('ï', 239), ('ð', 240), ('ñ', 241), ('ò', 242), ('ó', 243), ('ô', 244), ('õ', 245),
        ('ö', 246), ('÷', 247), ('ø', 248), ('ù', 249), ('ú', 250), ('û', 251), ('ü', 252),
        ('ý', 253), ('þ', 254), ('ÿ', 255),
    ];
    MAP.iter().find(|(ch, _)| *ch == c).map(|(_, b)| *b).unwrap_or(b'?')
}
