use crate::dom::{self, Elem, Node};
use crate::error::{Error, Result};
use crate::model::*;
use crate::zipio::unzip;

pub fn read_docx(data: &[u8]) -> Result<Document> {
    let zip = unzip(data)?;
    let xml = zip.get_ci("word/document.xml").ok_or_else(|| {
        Error::new("este arquivo não tem word/document.xml. Não parece um DOCX válido.")
    })?;
    let s = crate::decode::decode_text(xml)?;
    let dom = dom::parse_xml(&s)?;
    let mut rels = Vec::new();
    if let Some(r) = zip.get_ci("word/_rels/document.xml.rels") {
        if let Ok(rs) = crate::decode::decode_text(r) {
            if let Ok(rd) = dom::parse_xml(&rs) {
                let mut items = Vec::new();
                dom::descendants_named(&rd, "relationship", &mut items);
                for it in items {
                    if let (Some(id), Some(target)) = (it.attr("id"), it.attr("target")) {
                        rels.push((id.to_string(), target.to_string()));
                    }
                }
            }
        }
    }
    let mut doc = Document::new("docx");
    if let Some(core) = zip.get_ci("docProps/core.xml") {
        if let Ok(cs) = crate::decode::decode_text(core) {
            if let Ok(cd) = dom::parse_xml(&cs) {
                grab_first_str(&cd, "title", &mut doc.meta.title);
                grab_first(&cd, "creator", &mut doc.meta.authors);
                grab_first_str(&cd, "description", &mut doc.meta.description);
            }
        }
    }
    let mut body = Vec::new();
    dom::descendants_named(&dom, "body", &mut body);
    let body = body.first().copied().unwrap_or(&dom);
    let mut blocks = Vec::new();
    walk_docx(body, &rels, &mut blocks, false);
    doc.blocks = clean_blocks(blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new("o DOCX abriu, mas não tem parágrafos de texto."));
    }
    Ok(doc)
}

fn grab_first(e: &Elem, name: &str, authors: &mut Vec<String>) {
    let mut nodes = Vec::new();
    dom::descendants_named(e, name, &mut nodes);
    if let Some(n) = nodes.first() {
        let t = dom::first_text(n);
        if !t.is_empty() {
            authors.push(t);
        }
    }
}

fn grab_first_str(e: &Elem, name: &str, dest: &mut String) {
    let mut nodes = Vec::new();
    dom::descendants_named(e, name, &mut nodes);
    if let Some(n) = nodes.first() {
        let t = dom::first_text(n);
        if !t.is_empty() {
            *dest = t;
        }
    }
}

fn walk_docx(e: &Elem, rels: &[(String, String)], out: &mut Vec<Block>, in_table: bool) {
    for n in &e.children {
        let Node::Elem(c) = n else { continue };
        match c.name.as_str() {
            "p" => {
                if let Some(b) = docx_paragraph(c, rels) {
                    out.push(b);
                }
            }
            "tbl" if !in_table => {
                let mut rows = Vec::new();
                collect_docx_rows(c, rels, &mut rows);
                if !rows.is_empty() {
                    out.push(Block::Table { rows });
                }
            }
            "sdt" | "sdtcontent" | "txbxcontent" | "hyperlink" => walk_docx(c, rels, out, in_table),
            _ => {
                if c.name != "sectpr" {
                    // drawings and text boxes live inside paragraphs; also walk unknown wrappers
                    if c.children.iter().any(|n| matches!(n, Node::Elem(e) if e.name == "p" || e.name == "tbl")) {
                        walk_docx(c, rels, out, in_table);
                    }
                }
            }
        }
    }
}

fn docx_paragraph(p: &Elem, rels: &[(String, String)]) -> Option<Block> {
    let mut style = String::new();
    let mut list = false;
    let mut inlines = Vec::new();
    fn walk(e: &Elem, rels: &[(String, String)], inlines: &mut Vec<Inline>, style: &mut String, list: &mut bool, bold: bool, ital: bool) {
        for n in &e.children {
            match n {
                Node::Elem(c) => match c.name.as_str() {
                    "pstyle" => {
                        if let Some(v) = c.attr("val") {
                            *style = v.to_string();
                        }
                    }
                    "numpr" => *list = true,
                    "r" => {
                        let (b, i) = run_flags(c, bold, ital);
                        walk(c, rels, inlines, style, list, b, i);
                    }
                    "t" => {
                        let t = c.text();
                        if !t.is_empty() {
                            push_styled(inlines, t, bold, ital);
                        }
                    }
                    "tab" => inlines.push(Inline::Text(" ".into())),
                    "br" | "cr" => inlines.push(Inline::Break),
                    "hyperlink" => {
                        let href = c
                            .attr("id")
                            .and_then(|id| rels.iter().find(|(k, _)| k == id).map(|(_, t)| t.clone()))
                            .unwrap_or_default();
                        let mut inner = Vec::new();
                        walk(c, rels, &mut inner, style, list, bold, ital);
                        if !href.is_empty() {
                            inlines.push(Inline::Link {
                                href,
                                children: inner,
                            });
                        } else {
                            inlines.extend(inner);
                        }
                    }
                    "instrtext" | "deltext" => {}
                    _ => walk(c, rels, inlines, style, list, bold, ital),
                },
                Node::Text(t) => {
                    if e.name == "t" && !t.is_empty() {
                        push_styled(inlines, t.clone(), bold, ital);
                    }
                }
            }
        }
    }
    walk(p, rels, &mut inlines, &mut style, &mut list, false, false);
    let inlines = normalize_inlines(&inlines);
    if plain(&inlines).trim().is_empty() {
        return None;
    }
    let style_l = style.to_ascii_lowercase();
    if let Some(level) = heading_level(&style_l) {
        return Some(Block::Heading { level, inlines });
    }
    if style_l.contains("title") {
        return Some(Block::Heading { level: 1, inlines });
    }
    if style_l.contains("subtitle") {
        return Some(Block::Heading { level: 2, inlines });
    }
    if list {
        return Some(Block::List {
            ordered: false,
            items: vec![vec![Block::Paragraph { inlines }]],
        });
    }
    if style_l.contains("quote") {
        return Some(Block::Quote {
            blocks: vec![Block::Paragraph { inlines }],
        });
    }
    Some(Block::Paragraph { inlines })
}

fn push_styled(inlines: &mut Vec<Inline>, t: String, bold: bool, ital: bool) {
    let mut node = vec![Inline::Text(t)];
    if ital {
        node = vec![Inline::Emph(node)];
    }
    if bold {
        node = vec![Inline::Strong(node)];
    }
    inlines.extend(node);
}

fn run_flags(r: &Elem, bold: bool, ital: bool) -> (bool, bool) {
    let mut b = bold;
    let mut i = ital;
    for n in &r.children {
        if let Node::Elem(c) = n {
            if c.name == "rpr" {
                for p in &c.children {
                    if let Node::Elem(pr) = p {
                        if pr.name == "b" && pr.attr("val").unwrap_or("true") != "false" && pr.attr("val").unwrap_or("1") != "0" {
                            b = true;
                        }
                        if pr.name == "i" && pr.attr("val").unwrap_or("true") != "false" && pr.attr("val").unwrap_or("1") != "0" {
                            i = true;
                        }
                    }
                }
            }
        }
    }
    (b, i)
}

fn heading_level(style: &str) -> Option<u8> {
    let s = style.replace(' ', "");
    for n in 1..=6 {
        if s == format!("heading{n}") || s == format!("ttulo{n}") || s == format!("titulo{n}") || s.ends_with(&format!("heading{n}")) {
            return Some(n);
        }
    }
    None
}

fn collect_docx_rows(tbl: &Elem, rels: &[(String, String)], rows: &mut Vec<Vec<Cell>>) {
    for n in &tbl.children {
        if let Node::Elem(c) = n {
            if c.name == "tr" {
                let mut row = Vec::new();
                for celln in &c.children {
                    if let Node::Elem(cell) = celln {
                        if cell.name == "tc" {
                            let mut blocks = Vec::new();
                            walk_docx(cell, rels, &mut blocks, true);
                            let mut inlines = Vec::new();
                            for b in blocks {
                                if !inlines.is_empty() {
                                    inlines.push(Inline::Text(" ".into()));
                                }
                                match b {
                                    Block::Paragraph { inlines: i } | Block::Heading { inlines: i, .. } => inlines.extend(i),
                                    _ => inlines.extend(text_inline(block_plain(&b))),
                                }
                            }
                            row.push(Cell {
                                header: false,
                                inlines: normalize_inlines(&inlines),
                            });
                        }
                    }
                }
                if !row.is_empty() {
                    rows.push(row);
                }
            }
        }
    }
}

fn block_plain(b: &Block) -> String {
    match b {
        Block::Paragraph { inlines } | Block::Heading { inlines, .. } => plain(inlines),
        Block::Code { text } => text.clone(),
        _ => String::new(),
    }
}

pub fn read_odt(data: &[u8]) -> Result<Document> {
    let zip = unzip(data)?;
    let xml = zip.get_ci("content.xml").ok_or_else(|| {
        Error::new("este arquivo não tem content.xml. Não parece um ODT válido.")
    })?;
    let s = crate::decode::decode_text(xml)?;
    let dom = dom::parse_xml(&s)?;
    let mut doc = Document::new("odt");
    if let Some(meta) = zip.get_ci("meta.xml") {
        if let Ok(ms) = crate::decode::decode_text(meta) {
            if let Ok(md) = dom::parse_xml(&ms) {
                grab_first_str(&md, "title", &mut doc.meta.title);
                let mut creators = Vec::new();
                dom::descendants_named(&md, "creator", &mut creators);
                for c in creators {
                    let t = dom::first_text(c);
                    if !t.is_empty() {
                        doc.meta.authors.push(t);
                    }
                }
            }
        }
    }
    let mut office = Vec::new();
    dom::descendants_named(&dom, "text", &mut office);
    // body is text element inside body
    let root = office.first().copied().unwrap_or(&dom);
    let mut blocks = Vec::new();
    walk_odt(root, &mut blocks);
    doc.blocks = clean_blocks(coalesce_lists(blocks));
    if doc.blocks.is_empty() {
        return Err(Error::new("o ODT abriu, mas não tem texto."));
    }
    Ok(doc)
}

fn walk_odt(e: &Elem, out: &mut Vec<Block>) {
    for n in &e.children {
        let Node::Elem(c) = n else { continue };
        match c.name.as_str() {
            "h" => {
                let level = c.attr("outline-level").and_then(|v| v.parse().ok()).unwrap_or(1);
                let inlines = odt_inlines(c);
                if !plain(&inlines).trim().is_empty() {
                    out.push(Block::Heading {
                        level: level.clamp(1, 6),
                        inlines,
                    });
                }
            }
            "p" => {
                let inlines = odt_inlines(c);
                if !plain(&inlines).trim().is_empty() {
                    out.push(Block::Paragraph { inlines });
                }
            }
            "list" => {
                let mut items = Vec::new();
                for ch in &c.children {
                    if let Node::Elem(li) = ch {
                        if li.name == "list-item" {
                            let mut item = Vec::new();
                            walk_odt(li, &mut item);
                            if !item.is_empty() {
                                items.push(item);
                            }
                        }
                    }
                }
                if !items.is_empty() {
                    out.push(Block::List { ordered: false, items });
                }
            }
            "table" => {
                let mut rows = Vec::new();
                collect_odt_rows(c, &mut rows);
                if !rows.is_empty() {
                    out.push(Block::Table { rows });
                }
            }
            "span" | "s" | "tab" | "line-break" => {}
            _ => walk_odt(c, out),
        }
    }
}

fn odt_inlines(e: &Elem) -> Vec<Inline> {
    let mut out = Vec::new();
    fn rec(e: &Elem, out: &mut Vec<Inline>, bold: bool, ital: bool) {
        for n in &e.children {
            match n {
                Node::Text(t) => {
                    let t = tidy_text(t);
                    if !t.is_empty() {
                        let mut node = vec![Inline::Text(t)];
                        if ital {
                            node = vec![Inline::Emph(node)];
                        }
                        if bold {
                            node = vec![Inline::Strong(node)];
                        }
                        out.extend(node);
                    }
                }
                Node::Elem(c) => match c.name.as_str() {
                    "s" => out.push(Inline::Text(" ".into())),
                    "tab" => out.push(Inline::Text("\t".into())),
                    "line-break" => out.push(Inline::Break),
                    "span" => {
                        let style = c.attr("style-name").unwrap_or("").to_ascii_lowercase();
                        let b = bold || style.contains("bold") || style.contains("strong");
                        let i = ital || style.contains("italic") || style.contains("emphasis");
                        rec(c, out, b, i);
                    }
                    "a" => {
                        let href = c.attr("href").unwrap_or("").to_string();
                        let mut inner = Vec::new();
                        rec(c, &mut inner, bold, ital);
                        if href.is_empty() {
                            out.extend(inner);
                        } else {
                            out.push(Inline::Link { href, children: inner });
                        }
                    }
                    _ => rec(c, out, bold, ital),
                },
            }
        }
    }
    rec(e, &mut out, false, false);
    normalize_inlines(&out)
}

fn collect_odt_rows(table: &Elem, rows: &mut Vec<Vec<Cell>>) {
    for n in &table.children {
        if let Node::Elem(c) = n {
            if c.name == "table-row" {
                let mut row = Vec::new();
                for celln in &c.children {
                    if let Node::Elem(cell) = celln {
                        if cell.name == "table-cell" || cell.name == "covered-table-cell" {
                            let inlines = odt_inlines(cell);
                            row.push(Cell {
                                header: false,
                                inlines,
                            });
                        }
                    }
                }
                if !row.is_empty() {
                    rows.push(row);
                }
            } else {
                collect_odt_rows(c, rows);
            }
        }
    }
}

fn coalesce_lists(blocks: Vec<Block>) -> Vec<Block> {
    let mut out = Vec::new();
    for b in blocks {
        if let Block::List { ordered, items } = &b {
            if let Some(Block::List { ordered: o2, items: prev }) = out.last_mut() {
                if *o2 == *ordered && items.len() == 1 {
                    prev.extend(items.clone());
                    continue;
                }
            }
        }
        out.push(b);
    }
    out
}

pub fn read_xlsx(data: &[u8]) -> Result<Document> {
    let zip = unzip(data)?;
    let mut strings = Vec::new();
    if let Some(ss) = zip.get_ci("xl/sharedStrings.xml") {
        let s = crate::decode::decode_text(ss)?;
        let dom = dom::parse_xml(&s)?;
        let mut sis = Vec::new();
        dom::descendants_named(&dom, "si", &mut sis);
        for si in sis {
            strings.push(tidy_text(&si.text()));
        }
    }
    let mut doc = Document::new("xlsx");
    let mut sheets: Vec<String> = zip
        .names()
        .into_iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.starts_with("xl/worksheets/sheet") && l.ends_with(".xml")
        })
        .map(|s| s.to_string())
        .collect();
    sheets.sort();
    if sheets.is_empty() {
        return Err(Error::new("a planilha não tem worksheets. Não parece um XLSX válido."));
    }
    for (idx, path) in sheets.iter().enumerate() {
        let Some(bytes) = zip.get(path).or_else(|| zip.get_ci(path)) else { continue };
        let s = crate::decode::decode_text(bytes)?;
        let dom = dom::parse_xml(&s)?;
        let mut rows_e = Vec::new();
        dom::descendants_named(&dom, "row", &mut rows_e);
        let mut rows = Vec::new();
        for row in rows_e {
            let mut cells_e = Vec::new();
            for n in &row.children {
                if let Node::Elem(c) = n {
                    if c.name == "c" {
                        cells_e.push(c);
                    }
                }
            }
            if cells_e.is_empty() {
                continue;
            }
            let mut cells = Vec::new();
            let mut col = 0usize;
            for c in cells_e {
                let want = c.attr("r").and_then(col_index).unwrap_or(col);
                while cells.len() < want {
                    cells.push(Cell { header: false, inlines: Vec::new() });
                }
                let t = c.attr("t").unwrap_or("");
                let mut v = String::new();
                if t == "s" {
                    let mut vs = Vec::new();
                    dom::descendants_named(c, "v", &mut vs);
                    if let Some(vn) = vs.first() {
                        if let Ok(i) = dom::first_text(vn).parse::<usize>() {
                            v = strings.get(i).cloned().unwrap_or_default();
                        }
                    }
                } else if t == "inlineStr" {
                    v = tidy_text(&c.text());
                } else {
                    let mut vs = Vec::new();
                    dom::descendants_named(c, "v", &mut vs);
                    if let Some(vn) = vs.first() {
                        v = dom::first_text(vn);
                    } else {
                        v = tidy_text(&c.text());
                    }
                }
                cells.push(Cell {
                    header: false,
                    inlines: text_inline(v),
                });
                col = cells.len();
            }
            if cells.iter().any(|c| !plain(&c.inlines).is_empty()) {
                rows.push(cells);
            }
        }
        if !rows.is_empty() {
            if sheets.len() > 1 {
                doc.blocks.push(heading(2, format!("Planilha {}", idx + 1)));
            }
            doc.blocks.push(Block::Table { rows });
        }
    }
    if doc.blocks.is_empty() {
        return Err(Error::new("a planilha abriu, mas as células estão vazias."));
    }
    Ok(doc)
}

fn col_index(r: &str) -> Option<usize> {
    let letters: String = r.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    if letters.is_empty() {
        return None;
    }
    let mut n = 0usize;
    for c in letters.chars() {
        n = n * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
    }
    Some(n - 1)
}

pub fn read_ods(data: &[u8]) -> Result<Document> {
    let mut doc = read_odt(data)?;
    doc.meta.source_format = "ods".into();
    Ok(doc)
}

pub fn read_xps(data: &[u8]) -> Result<Document> {
    let zip = unzip(data)?;
    let mut doc = Document::new("xps");
    let mut pages: Vec<String> = zip
        .names()
        .into_iter()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.ends_with(".fpage") || (l.contains("/documents/") && l.ends_with(".xml"))
        })
        .map(|s| s.to_string())
        .collect();
    pages.sort();
    for (i, p) in pages.iter().enumerate() {
        let Some(bytes) = zip.get(p) else { continue };
        let s = crate::decode::decode_text(bytes)?;
        let dom = match dom::parse_xml(&s) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let mut glyphs = Vec::new();
        dom::descendants_named(&dom, "glyphs", &mut glyphs);
        let mut lines: Vec<(i32, i32, String)> = Vec::new();
        for g in glyphs {
            let text = g.attr("unicodestring").unwrap_or("").to_string();
            if text.trim().is_empty() {
                continue;
            }
            let origin = g.attr("originx").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            let originy = g.attr("originy").and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            lines.push((originy as i32, origin as i32, text));
        }
        if lines.is_empty() {
            continue;
        }
        lines.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut buf = String::new();
        let mut last_y = lines[0].0;
        for (y, _, t) in lines {
            if (y - last_y).abs() > 8 && !buf.is_empty() {
                doc.blocks.push(paragraph(buf.trim()));
                buf.clear();
            }
            if !buf.is_empty() && !buf.ends_with(' ') && !t.starts_with(' ') {
                buf.push(' ');
            }
            buf.push_str(&t);
            last_y = y;
        }
        if !buf.trim().is_empty() {
            doc.blocks.push(paragraph(buf.trim()));
        }
        if pages.len() > 1 && i + 1 != pages.len() {
            doc.blocks.push(Block::Rule);
        }
    }
    doc.blocks = clean_blocks(doc.blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new("o XPS abriu, mas não achei texto nos glifos."));
    }
    Ok(doc)
}
