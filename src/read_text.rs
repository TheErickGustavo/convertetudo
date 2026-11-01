use crate::dom;
use crate::error::{Error, Result};
use crate::model::*;

pub fn read_html(data: &[u8], format: &str) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let dom = dom::parse_html(&s).or_else(|_| dom::parse_xml(&s))?;
    let mut doc = Document::new(format);
    if let Some(title) = find_title(&dom) {
        doc.meta.title = title;
    }
    doc.blocks = dom::elem_to_blocks(&dom);
    if doc.blocks.is_empty() {
        return Err(Error::new("o HTML não tem texto no corpo."));
    }
    Ok(doc)
}

fn find_title(e: &dom::Elem) -> Option<String> {
    let mut titles = Vec::new();
    dom::descendants_named(e, "title", &mut titles);
    titles.first().map(|t| dom::first_text(t)).filter(|t| !t.is_empty())
}

pub fn read_plain(data: &[u8]) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let t = s.replace("\r\n", "\n").replace('\r', "\n");
    if looks_like_markdown(&t) {
        return read_markdown(data);
    }
    let mut doc = Document::new("txt");
    for para in t.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }
        let line = para.split('\n').map(str::trim).collect::<Vec<_>>().join(" ");
        doc.blocks.push(paragraph(line));
    }
    if doc.blocks.is_empty() {
        return Err(Error::new("o arquivo de texto está vazio."));
    }
    Ok(doc)
}

fn looks_like_markdown(s: &str) -> bool {
    let mut marks = 0;
    for line in s.lines().take(80) {
        let l = line.trim();
        if l.starts_with('#') || l.starts_with("```") || l.starts_with("- ") || l.starts_with("* ") {
            marks += 1;
        }
    }
    marks >= 2
}

pub fn read_markdown(data: &[u8]) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let text = s.replace("\r\n", "\n").replace('\r', "\n");
    let mut doc = Document::new("md");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let trim = line.trim();
        if trim.is_empty() {
            i += 1;
            continue;
        }
        if trim.starts_with("```") {
            i += 1;
            let mut buf = String::new();
            while i < lines.len() && !lines[i].trim().starts_with("```") {
                if !buf.is_empty() {
                    buf.push('\n');
                }
                buf.push_str(lines[i]);
                i += 1;
            }
            if i < lines.len() {
                i += 1;
            }
            doc.blocks.push(Block::Code { text: buf });
            continue;
        }
        if let Some(rest) = trim.strip_prefix('#') {
            let mut level = 1u8;
            let mut r = rest;
            while r.starts_with('#') && level < 6 {
                level += 1;
                r = &r[1..];
            }
            let title = r.trim();
            if !title.is_empty() {
                doc.blocks.push(heading(level, md_inline_plain(title)));
                i += 1;
                continue;
            }
        }
        if trim == "---" || trim == "***" || trim == "___" {
            doc.blocks.push(Block::Rule);
            i += 1;
            continue;
        }
        if trim.starts_with('>') {
            let mut q = Vec::new();
            while i < lines.len() && lines[i].trim().starts_with('>') {
                let l = lines[i].trim().trim_start_matches('>').trim();
                if !l.is_empty() {
                    q.push(l.to_string());
                }
                i += 1;
            }
            if !q.is_empty() {
                doc.blocks.push(Block::Quote {
                    blocks: vec![paragraph(q.join(" "))],
                });
            }
            continue;
        }
        if is_list_line(trim) {
            let ordered = trim.starts_with(|c: char| c.is_ascii_digit());
            let mut items = Vec::new();
            while i < lines.len() && is_list_line(lines[i].trim()) {
                let item = strip_list_marker(lines[i].trim());
                items.push(vec![Block::Paragraph {
                    inlines: md_inlines(item),
                }]);
                i += 1;
            }
            doc.blocks.push(Block::List { ordered, items });
            continue;
        }
        if trim.contains('|') && i + 1 < lines.len() && is_table_sep(lines[i + 1]) {
            let mut rows = Vec::new();
            rows.push(split_row(trim, true));
            i += 2;
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                rows.push(split_row(lines[i].trim(), false));
                i += 1;
            }
            doc.blocks.push(Block::Table { rows });
            continue;
        }
        let mut para = vec![trim.to_string()];
        i += 1;
        while i < lines.len() {
            let t = lines[i].trim();
            if t.is_empty() || t.starts_with('#') || t.starts_with("```") || is_list_line(t) || t.starts_with('>') {
                break;
            }
            para.push(t.to_string());
            i += 1;
        }
        doc.blocks.push(Block::Paragraph {
            inlines: md_inlines(&para.join(" ")),
        });
    }
    if doc.blocks.is_empty() {
        return Err(Error::new("o Markdown está vazio."));
    }
    Ok(doc)
}

fn is_list_line(t: &str) -> bool {
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return true;
    }
    let mut chars = t.chars();
    if chars.next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        let rest: String = chars.collect();
        return rest.starts_with(". ") || rest.starts_with(") ");
    }
    false
}

fn strip_list_marker(t: &str) -> &str {
    if let Some(r) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("+ ")) {
        return r;
    }
    if let Some(idx) = t.find(". ").or_else(|| t.find(") ")) {
        return t[idx + 2..].trim();
    }
    t
}

fn is_table_sep(line: &str) -> bool {
    let t = line.trim();
    t.contains('|') && t.contains('-') && t.chars().all(|c| "-:| ".contains(c))
}

fn split_row(line: &str, header: bool) -> Vec<Cell> {
    let t = line.trim().trim_matches('|');
    t.split('|')
        .map(|c| Cell {
            header,
            inlines: md_inlines(c.trim()),
        })
        .collect()
}

fn md_inline_plain(s: &str) -> String {
    plain(&md_inlines(s))
}

fn md_inlines(s: &str) -> Vec<Inline> {
    let b = s.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<Inline>| {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf)));
        }
    };
    while i < b.len() {
        if b[i] == b'`' {
            if let Some(end) = s[i + 1..].find('`') {
                flush(&mut buf, &mut out);
                out.push(Inline::Code(s[i + 1..i + 1 + end].to_string()));
                i += end + 2;
                continue;
            }
        }
        if s[i..].starts_with("**") || s[i..].starts_with("__") {
            let delim = &s[i..i + 2];
            if let Some(end) = s[i + 2..].find(delim) {
                flush(&mut buf, &mut out);
                out.push(Inline::Strong(md_inlines(&s[i + 2..i + 2 + end])));
                i += end + 4;
                continue;
            }
        }
        if b[i] == b'*' || b[i] == b'_' {
            let delim = b[i] as char;
            if let Some(end) = s[i + 1..].find(delim) {
                if end > 0 {
                    flush(&mut buf, &mut out);
                    out.push(Inline::Emph(md_inlines(&s[i + 1..i + 1 + end])));
                    i += end + 2;
                    continue;
                }
            }
        }
        if b[i] == b'[' {
            if let Some(close) = s[i + 1..].find(']') {
                let text = &s[i + 1..i + 1 + close];
                let after = i + 1 + close + 1;
                if after < b.len() && b[after] == b'(' {
                    if let Some(end) = s[after + 1..].find(')') {
                        flush(&mut buf, &mut out);
                        let href = s[after + 1..after + 1 + end].to_string();
                        out.push(Inline::Link {
                            href,
                            children: md_inlines(text),
                        });
                        i = after + 1 + end + 1;
                        continue;
                    }
                }
            }
        }
        let ch = s[i..].chars().next().unwrap_or('\u{FFFD}');
        buf.push(ch);
        i += ch.len_utf8();
    }
    flush(&mut buf, &mut out);
    normalize_inlines(&out)
}

pub fn read_csv(data: &[u8], delim: u8) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let rows_raw = parse_csv(&s, delim as char);
    if rows_raw.is_empty() {
        return Err(Error::new("a planilha CSV está vazia."));
    }
    let mut doc = Document::new(if delim == b'\t' { "tsv" } else { "csv" });
    let mut rows = Vec::new();
    for (i, r) in rows_raw.into_iter().enumerate() {
        rows.push(
            r.into_iter()
                .map(|c| Cell {
                    header: i == 0,
                    inlines: text_inline(c),
                })
                .collect(),
        );
    }
    doc.blocks.push(Block::Table { rows });
    Ok(doc)
}

fn parse_csv(s: &str, delim: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cell = String::new();
    let mut in_quotes = false;
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                if i + 1 < chars.len() && chars[i + 1] == '"' {
                    cell.push('"');
                    i += 2;
                    continue;
                }
                in_quotes = false;
                i += 1;
                continue;
            }
            cell.push(c);
            i += 1;
            continue;
        }
        if c == '"' && cell.is_empty() {
            in_quotes = true;
            i += 1;
            continue;
        }
        if c == delim {
            row.push(std::mem::take(&mut cell));
            i += 1;
            continue;
        }
        if c == '\n' || c == '\r' {
            if c == '\r' && i + 1 < chars.len() && chars[i + 1] == '\n' {
                i += 1;
            }
            row.push(std::mem::take(&mut cell));
            if row.iter().any(|c| !c.is_empty()) {
                rows.push(std::mem::take(&mut row));
            } else {
                row.clear();
            }
            i += 1;
            continue;
        }
        cell.push(c);
        i += 1;
    }
    if !cell.is_empty() || !row.is_empty() {
        row.push(cell);
        if row.iter().any(|c| !c.is_empty()) {
            rows.push(row);
        }
    }
    rows
}

pub fn read_fb2(data: &[u8]) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let dom = dom::parse_xml(&s)?;
    let mut doc = Document::new("fb2");
    let mut titles = Vec::new();
    dom::descendants_named(&dom, "book-title", &mut titles);
    if let Some(t) = titles.first() {
        doc.meta.title = dom::first_text(t);
    }
    let mut authors = Vec::new();
    dom::descendants_named(&dom, "author", &mut authors);
    for a in authors {
        let mut first = Vec::new();
        let mut last = Vec::new();
        dom::descendants_named(a, "first-name", &mut first);
        dom::descendants_named(a, "last-name", &mut last);
        let name = format!(
            "{} {}",
            first.first().map(|e| dom::first_text(e)).unwrap_or_default(),
            last.first().map(|e| dom::first_text(e)).unwrap_or_default()
        )
        .trim()
        .to_string();
        if !name.is_empty() {
            doc.meta.authors.push(name);
        }
    }
    let mut bodies = Vec::new();
    dom::descendants_named(&dom, "body", &mut bodies);
    for body in bodies {
        let name = body.attr("name").unwrap_or("");
        if name == "notes" || name == "comments" {
            doc.blocks.push(heading(1, "Notas"));
        }
        walk_fb2(body, &mut doc.blocks, 1);
    }
    doc.blocks = clean_blocks(doc.blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new("o FB2 não tem seções de texto."));
    }
    Ok(doc)
}

fn walk_fb2(e: &dom::Elem, out: &mut Vec<Block>, level: u8) {
    use crate::dom::Node;
    for n in &e.children {
        let Node::Elem(c) = n else { continue };
        match c.name.as_str() {
            "section" => walk_fb2(c, out, level),
            "title" => {
                let t = dom::first_text(c);
                if !t.is_empty() {
                    out.push(heading(level.min(6), t));
                }
            }
            "subtitle" => {
                let t = dom::first_text(c);
                if !t.is_empty() {
                    out.push(heading((level + 1).min(6), t));
                }
            }
            "p" | "v" => {
                let t = tidy_text(&c.text());
                if !t.is_empty() {
                    out.push(paragraph(t));
                }
            }
            "empty-line" => {}
            "cite" | "poem" | "epigraph" => {
                let mut inner = Vec::new();
                walk_fb2(c, &mut inner, level + 1);
                if !inner.is_empty() {
                    out.push(Block::Quote { blocks: inner });
                }
            }
            "image" => {
                let alt = c.attr("alt").unwrap_or("").to_string();
                if !alt.is_empty() {
                    out.push(Block::Image { alt });
                }
            }
            _ => walk_fb2(c, out, level),
        }
    }
}

pub fn read_abw(data: &[u8]) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let dom = dom::parse_xml(&s)?;
    let mut doc = Document::new("abw");
    walk_abw(&dom, &mut doc.blocks);
    doc.blocks = clean_blocks(doc.blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new("o AbiWord não tem parágrafos."));
    }
    Ok(doc)
}

fn walk_abw(e: &dom::Elem, out: &mut Vec<Block>) {
    use crate::dom::Node;
    for n in &e.children {
        let Node::Elem(c) = n else { continue };
        match c.name.as_str() {
            "p" | "c" => {
                let t = tidy_text(&c.text());
                if !t.is_empty() {
                    let style = c.attr("style").unwrap_or("");
                    if style.to_ascii_lowercase().contains("heading") {
                        out.push(heading(2, t));
                    } else {
                        out.push(paragraph(t));
                    }
                }
            }
            _ => walk_abw(c, out),
        }
    }
}

pub fn read_docbook(data: &[u8]) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let dom = dom::parse_xml(&s)?;
    let mut doc = Document::new("docbook");
    let mut titles = Vec::new();
    dom::descendants_named(&dom, "title", &mut titles);
    if let Some(t) = titles.first() {
        doc.meta.title = dom::first_text(t);
    }
    walk_docbook(&dom, &mut doc.blocks, 1);
    doc.blocks = clean_blocks(doc.blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new("o DocBook não tem texto."));
    }
    Ok(doc)
}

fn walk_docbook(e: &dom::Elem, out: &mut Vec<Block>, level: u8) {
    use crate::dom::Node;
    for n in &e.children {
        let Node::Elem(c) = n else { continue };
        match c.name.as_str() {
            "title" => {
                let t = dom::first_text(c);
                if !t.is_empty() {
                    out.push(heading(level.min(6), t));
                }
            }
            "para" | "simpara" => {
                let t = tidy_text(&c.text());
                if !t.is_empty() {
                    out.push(paragraph(t));
                }
            }
            "itemizedlist" | "orderedlist" => {
                let ordered = c.name == "orderedlist";
                let mut items = Vec::new();
                let mut lis = Vec::new();
                dom::descendants_named(c, "listitem", &mut lis);
                for li in lis {
                    let mut inner = Vec::new();
                    walk_docbook(li, &mut inner, level + 1);
                    if !inner.is_empty() {
                        items.push(inner);
                    }
                }
                if !items.is_empty() {
                    out.push(Block::List { ordered, items });
                }
            }
            "section" | "chapter" | "article" | "book" | "sect1" | "sect2" | "sect3" => {
                walk_docbook(c, out, if c.name == "title" { level } else { (level + 1).min(6) });
            }
            _ => walk_docbook(c, out, level),
        }
    }
}

pub fn read_latex(data: &[u8]) -> Result<Document> {
    let s = crate::decode::decode_text(data)?;
    let mut doc = Document::new("tex");
    doc.warn("LaTeX é convertido pelo texto visível. Macros, matemática e figuras não são reproduzidas.");
    if let Some(t) = command_arg(&s, "\\title") {
        doc.meta.title = t;
        doc.blocks.push(heading(1, doc.meta.title.clone()));
    }
    if let Some(a) = command_arg(&s, "\\author") {
        doc.meta.authors.push(a.clone());
        doc.blocks.push(paragraph(a));
    }
    let body = extract_document_env(&s).unwrap_or(s.as_str());
    let cleaned = strip_latex(body);
    for para in cleaned.split("\n\n") {
        let p = para.trim();
        if p.is_empty() {
            continue;
        }
        if let Some(rest) = p.strip_prefix("§H1 ") {
            doc.blocks.push(heading(1, rest));
        } else if let Some(rest) = p.strip_prefix("§H2 ") {
            doc.blocks.push(heading(2, rest));
        } else if let Some(rest) = p.strip_prefix("§H3 ") {
            doc.blocks.push(heading(3, rest));
        } else {
            doc.blocks.push(paragraph(p.replace('\n', " ")));
        }
    }
    if doc.blocks.is_empty() {
        return Err(Error::new("não achei texto visível neste LaTeX."));
    }
    Ok(doc)
}

fn command_arg(s: &str, cmd: &str) -> Option<String> {
    let i = s.find(cmd)?;
    let rest = s[i + cmd.len()..].trim_start();
    if !rest.starts_with('{') {
        return None;
    }
    let inner = take_braced(&rest[1..])?;
    Some(strip_latex(inner).trim().to_string())
}

fn take_braced(s: &str) -> Option<&str> {
    let mut depth = 1i32;
    for (i, c) in s.char_indices() {
        if c == '{' {
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                return Some(&s[..i]);
            }
        }
    }
    None
}

fn extract_document_env(s: &str) -> Option<&str> {
    let start = s.find("\\begin{document}")? + "\\begin{document}".len();
    let end = s[start..].find("\\end{document}").map(|i| start + i).unwrap_or(s.len());
    Some(&s[start..end])
}

fn strip_latex(s: &str) -> String {
    let mut out = String::new();
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i] == b'\\' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_alphabetic() {
                j += 1;
            }
            let cmd = &s[start..j];
            i = j;
            if cmd == "section" || cmd == "chapter" {
                if i < b.len() && b[i] == b'{' {
                    if let Some(inner) = take_braced(&s[i + 1..]) {
                        out.push_str("\n\n§H1 ");
                        out.push_str(&strip_latex(inner));
                        out.push_str("\n\n");
                        i += 1 + inner.len() + 1;
                    }
                }
                continue;
            }
            if cmd == "subsection" {
                if i < b.len() && b[i] == b'{' {
                    if let Some(inner) = take_braced(&s[i + 1..]) {
                        out.push_str("\n\n§H2 ");
                        out.push_str(&strip_latex(inner));
                        out.push_str("\n\n");
                        i += 1 + inner.len() + 1;
                    }
                }
                continue;
            }
            if cmd == "subsubsection" {
                if i < b.len() && b[i] == b'{' {
                    if let Some(inner) = take_braced(&s[i + 1..]) {
                        out.push_str("\n\n§H3 ");
                        out.push_str(&strip_latex(inner));
                        out.push_str("\n\n");
                        i += 1 + inner.len() + 1;
                    }
                }
                continue;
            }
            if matches!(cmd, "emph" | "textit" | "textbf" | "texttt" | "underline" | "mbox") {
                if i < b.len() && b[i] == b'{' {
                    if let Some(inner) = take_braced(&s[i + 1..]) {
                        out.push_str(&strip_latex(inner));
                        i += 1 + inner.len() + 1;
                    }
                }
                continue;
            }
            if cmd == "\\" || cmd.is_empty() {
                out.push('\n');
                continue;
            }
            if i < b.len() && b[i] == b'{' {
                if let Some(inner) = take_braced(&s[i + 1..]) {
                    // drop command, keep argument text for unknown commands
                    out.push_str(&strip_latex(inner));
                    i += 1 + inner.len() + 1;
                }
            }
            continue;
        }
        if b[i] == b'{' || b[i] == b'}' {
            i += 1;
            continue;
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

pub fn read_rtf(data: &[u8]) -> Result<Document> {
    let raw = if data.starts_with(b"{\\rtf") {
        data
    } else {
        return Err(Error::new("isto não é um RTF (falta {\\rtf)."));
    };
    let mut doc = Document::new("rtf");
    let mut out = String::new();
    let mut i = 0;
    let mut skip_group = 0i32;
    let mut uc = 1usize;
    let mut bold = false;
    let mut ital = false;
    let mut para_inlines: Vec<Inline> = Vec::new();
    let flush_text = |buf: &mut String, para: &mut Vec<Inline>, bold: bool, ital: bool| {
        if buf.is_empty() {
            return;
        }
        let t = std::mem::take(buf);
        let mut node = vec![Inline::Text(t)];
        if ital {
            node = vec![Inline::Emph(node)];
        }
        if bold {
            node = vec![Inline::Strong(node)];
        }
        para.extend(node);
    };
    let mut buf = String::new();
    while i < raw.len() {
        if skip_group > 0 {
            if raw[i] == b'{' {
                skip_group += 1;
            } else if raw[i] == b'}' {
                skip_group -= 1;
            } else if raw[i] == b'\\' && i + 1 < raw.len() {
                // still skip, but don't get stuck
            }
            i += 1;
            continue;
        }
        match raw[i] {
            b'{' => {
                // destination groups we skip: fonttbl, colortbl, stylesheet, info, pict, object, footer, header
                let rest = &raw[i + 1..];
                let dest = peek_dest(rest);
                if matches!(dest.as_deref(), Some("fonttbl") | Some("colortbl") | Some("stylesheet") | Some("info") | Some("pict") | Some("object") | Some("footer") | Some("header") | Some("footerf") | Some("headerf") | Some("*")) {
                    skip_group = 1;
                    i += 1;
                    continue;
                }
                i += 1;
            }
            b'}' => i += 1,
            b'\\' => {
                i += 1;
                if i >= raw.len() {
                    break;
                }
                let c = raw[i];
                if c == b'\\' || c == b'{' || c == b'}' {
                    buf.push(c as char);
                    i += 1;
                    continue;
                }
                if c == b'\'' && i + 2 < raw.len() {
                    flush_text(&mut buf, &mut para_inlines, bold, ital);
                    if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&raw[i + 1..i + 3]).unwrap_or(""), 16) {
                        buf.push(cp1252_char(v));
                    }
                    i += 3;
                    continue;
                }
                if c == b'u' && i + 1 < raw.len() && (raw[i + 1].is_ascii_digit() || raw[i + 1] == b'-') {
                    flush_text(&mut buf, &mut para_inlines, bold, ital);
                    i += 1;
                    let neg = raw[i] == b'-';
                    if neg {
                        i += 1;
                    }
                    let start = i;
                    while i < raw.len() && raw[i].is_ascii_digit() {
                        i += 1;
                    }
                    if let Ok(n) = std::str::from_utf8(&raw[start..i]).unwrap_or("").parse::<i32>() {
                        let mut cp = if neg { -n } else { n };
                        if cp < 0 {
                            cp += 65536;
                        }
                        if let Some(ch) = char::from_u32(cp as u32) {
                            buf.push(ch);
                        }
                    }
                    // skip uc fallback chars
                    let mut skipped = 0;
                    while skipped < uc && i < raw.len() {
                        if raw[i] == b'\\' {
                            break;
                        }
                        if raw[i] == b' ' {
                            i += 1;
                            break;
                        }
                        i += 1;
                        skipped += 1;
                    }
                    continue;
                }
                if c == b'~' {
                    buf.push('\u{00a0}');
                    i += 1;
                    continue;
                }
                if c == b'-' {
                    i += 1;
                    continue; // optional hyphen
                }
                if c == b'*' {
                    i += 1;
                    continue;
                }
                let start = i;
                while i < raw.len() && raw[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = std::str::from_utf8(&raw[start..i]).unwrap_or("");
                let mut num: Option<i32> = None;
                if i < raw.len() && (raw[i].is_ascii_digit() || raw[i] == b'-') {
                    let ns = i;
                    if raw[i] == b'-' {
                        i += 1;
                    }
                    while i < raw.len() && raw[i].is_ascii_digit() {
                        i += 1;
                    }
                    num = std::str::from_utf8(&raw[ns..i]).ok().and_then(|s| s.parse().ok());
                }
                if i < raw.len() && raw[i] == b' ' {
                    i += 1;
                }
                match word {
                    "par" | "line" => {
                        flush_text(&mut buf, &mut para_inlines, bold, ital);
                        let inlines = normalize_inlines(&para_inlines);
                        para_inlines.clear();
                        if !plain(&inlines).trim().is_empty() {
                            doc.blocks.push(Block::Paragraph { inlines });
                        }
                    }
                    "b" => {
                        flush_text(&mut buf, &mut para_inlines, bold, ital);
                        bold = num.unwrap_or(1) != 0;
                    }
                    "i" => {
                        flush_text(&mut buf, &mut para_inlines, bold, ital);
                        ital = num.unwrap_or(1) != 0;
                    }
                    "uc" => uc = num.unwrap_or(1).max(0) as usize,
                    "tab" => buf.push('\t'),
                    _ => {}
                }
            }
            b'\n' | b'\r' => i += 1,
            other => {
                buf.push(cp1252_char(other));
                i += 1;
            }
        }
        let _ = out;
    }
    flush_text(&mut buf, &mut para_inlines, bold, ital);
    if !plain(&para_inlines).trim().is_empty() {
        doc.blocks.push(Block::Paragraph {
            inlines: normalize_inlines(&para_inlines),
        });
    }
    doc.blocks = clean_blocks(doc.blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new("o RTF não tem texto visível."));
    }
    Ok(doc)
}

fn peek_dest(rest: &[u8]) -> Option<String> {
    let mut i = 0;
    if rest.first() == Some(&b'\\') {
        i = 1;
        if rest.get(i) == Some(&b'*') {
            return Some("*".into());
        }
        let start = i;
        while i < rest.len() && rest[i].is_ascii_alphabetic() {
            i += 1;
        }
        return Some(String::from_utf8_lossy(&rest[start..i]).into_owned());
    }
    None
}

fn cp1252_char(b: u8) -> char {
    if b < 128 {
        return b as char;
    }
    if b >= 160 {
        return b as char;
    }
    const EXTRA: [char; 32] = [
        '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
        '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}', '\u{017d}', '\u{008f}',
        '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
        '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
    ];
    EXTRA[(b - 128) as usize]
}
