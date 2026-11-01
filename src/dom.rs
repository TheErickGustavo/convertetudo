use crate::entities::decode_entities;
use crate::error::{Error, Result};
use crate::model::*;

#[derive(Clone, Debug)]
pub struct Elem {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Clone, Debug)]
pub enum Node {
    Elem(Elem),
    Text(String),
}

impl Elem {
    pub fn attr(&self, key: &str) -> Option<&str> {
        let key_l = key.to_ascii_lowercase();
        self.attrs.iter().find_map(|(k, v)| {
            let kl = k.to_ascii_lowercase();
            if kl == key_l || kl.ends_with(&format!(":{key_l}")) {
                Some(v.as_str())
            } else {
                None
            }
        })
    }

    pub fn text(&self) -> String {
        let mut s = String::new();
        fn rec(n: &Node, s: &mut String) {
            match n {
                Node::Text(t) => s.push_str(t),
                Node::Elem(e) => {
                    if e.name == "script" || e.name == "style" {
                        return;
                    }
                    for c in &e.children {
                        rec(c, s);
                    }
                }
            }
        }
        for c in &self.children {
            rec(c, &mut s);
        }
        s
    }
}

pub fn parse_xml(input: &str) -> Result<Elem> {
    let s = strip_bom(input);
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut stack: Vec<Elem> = vec![Elem {
        name: "#root".into(),
        attrs: Vec::new(),
        children: Vec::new(),
    }];
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'<' {
                i += 1;
            }
            let text = decode_entities(&s[start..i]);
            if !text.is_empty() {
                stack.last_mut().unwrap().children.push(Node::Text(text));
            }
            continue;
        }
        if s[i..].starts_with("<!--") {
            if let Some(end) = s[i + 4..].find("-->") {
                i += 4 + end + 3;
                continue;
            }
            break;
        }
        if s[i..].starts_with("<![CDATA[") {
            if let Some(end) = s[i + 9..].find("]]>") {
                let text = s[i + 9..i + 9 + end].to_string();
                stack.last_mut().unwrap().children.push(Node::Text(text));
                i += 9 + end + 3;
                continue;
            }
            break;
        }
        if s[i..].starts_with("<?") {
            if let Some(end) = s[i + 2..].find("?>") {
                i += 2 + end + 2;
                continue;
            }
            break;
        }
        if s[i..].starts_with("<!") {
            if let Some(end) = s[i + 2..].find('>') {
                i += 2 + end + 1;
                continue;
            }
            break;
        }
        let closing = s[i + 1..].starts_with('/');
        let mut j = i + 1 + if closing { 1 } else { 0 };
        if j >= bytes.len() {
            break;
        }
        let name_start = j;
        while j < bytes.len() && is_name(bytes[j]) {
            j += 1;
        }
        if j == name_start {
            i += 1;
            continue;
        }
        let name = local_name(&s[name_start..j]);
        if closing {
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            if j < bytes.len() {
                j += 1;
            }
            close_tag(&mut stack, &name);
            i = j;
            continue;
        }
        let (attrs, self_close, next) = parse_attrs(s, j)?;
        let elem = Elem {
            name,
            attrs,
            children: Vec::new(),
        };
        if self_close || is_void(&elem.name) {
            stack.last_mut().unwrap().children.push(Node::Elem(elem));
        } else if stack.len() < 256 {
            stack.push(elem);
        } else {
            return Err(Error::new("XML aninhado demais"));
        }
        i = next;
    }
    while stack.len() > 1 {
        let e = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(Node::Elem(e));
    }
    Ok(stack.pop().unwrap())
}

fn close_tag(stack: &mut Vec<Elem>, name: &str) {
    if stack.len() <= 1 {
        return;
    }
    let mut idx = None;
    for (n, e) in stack.iter().enumerate().rev() {
        if n == 0 {
            break;
        }
        if e.name == name {
            idx = Some(n);
            break;
        }
    }
    if let Some(idx) = idx {
        while stack.len() > idx {
            let e = stack.pop().unwrap();
            stack.last_mut().unwrap().children.push(Node::Elem(e));
        }
    }
}

fn parse_attrs(s: &str, mut j: usize) -> Result<(Vec<(String, String)>, bool, usize)> {
    let bytes = s.as_bytes();
    let mut attrs = Vec::new();
    loop {
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= bytes.len() {
            return Ok((attrs, false, j));
        }
        if bytes[j] == b'/' {
            j += 1;
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            if j < bytes.len() {
                j += 1;
            }
            return Ok((attrs, true, j));
        }
        if bytes[j] == b'>' {
            return Ok((attrs, false, j + 1));
        }
        let ns = j;
        while j < bytes.len() && is_name(bytes[j]) {
            j += 1;
        }
        if j == ns {
            j += 1;
            continue;
        }
        let key = s[ns..j].to_string();
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'=' {
            j += 1;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            let val = if bytes[j] == b'"' || bytes[j] == b'\'' {
                let q = bytes[j];
                j += 1;
                let vs = j;
                while j < bytes.len() && bytes[j] != q {
                    j += 1;
                }
                let v = decode_entities(&s[vs..j]);
                if j < bytes.len() {
                    j += 1;
                }
                v
            } else {
                let vs = j;
                while j < bytes.len() && !bytes[j].is_ascii_whitespace() && bytes[j] != b'>' && bytes[j] != b'/'
                {
                    j += 1;
                }
                decode_entities(&s[vs..j])
            };
            attrs.push((key, val));
        } else {
            attrs.push((key, String::new()));
        }
    }
    Ok((attrs, false, j))
}

fn is_name(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b':' || b == b'_' || b == b'-' || b == b'.'
}

fn local_name(name: &str) -> String {
    let n = name.rsplit(':').next().unwrap_or(name);
    n.to_ascii_lowercase()
}

fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta"
            | "param" | "source" | "track" | "wbr"
    )
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

pub fn parse_html(input: &str) -> Result<Elem> {
    // HTML is not always well-formed. Reuse the XML scanner, which already
    // tolerates unclosed tags by leaving them on the stack, and treat script/style
    // as raw text so their contents don't become elements.
    let s = strip_bom(input);
    let bytes = s.as_bytes();
    let mut i = 0;
    let mut stack: Vec<Elem> = vec![Elem {
        name: "#root".into(),
        attrs: Vec::new(),
        children: Vec::new(),
    }];
    while i < bytes.len() {
        let raw = stack.last().map(|e| e.name == "script" || e.name == "style").unwrap_or(false);
        if raw {
            let name = stack.last().unwrap().name.clone();
            let end_tag = format!("</{name}");
            if let Some(rel) = s[i..].to_ascii_lowercase().find(&end_tag) {
                // Script and style are not document text. Consume the end tag
                // here; leaving i on `</` and continuing re-enters this branch
                // forever, because the element is still open.
                let mut j = i + rel + end_tag.len();
                while j < bytes.len() && bytes[j] != b'>' {
                    j += 1;
                }
                if j < bytes.len() {
                    j += 1;
                }
                close_tag(&mut stack, &name);
                i = j;
                continue;
            } else {
                break;
            }
        }
        if bytes[i] != b'<' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'<' {
                i += 1;
            }
            let text = decode_entities(&s[start..i]);
            if text.chars().any(|c| !c.is_whitespace()) || text.contains('\u{00a0}') {
                stack.last_mut().unwrap().children.push(Node::Text(text));
            } else if text.contains('\n') && stack.last().unwrap().name != "#root" {
                // keep a soft break signal only when the parent is inline-ish; skip pure gaps
            }
            continue;
        }
        if s[i..].starts_with("<!--") {
            if let Some(end) = s[i + 4..].find("-->") {
                i += 4 + end + 3;
                continue;
            }
            break;
        }
        if s[i..].starts_with("<!") || s[i..].starts_with("<?") {
            if let Some(end) = s[i..].find('>') {
                i += end + 1;
                continue;
            }
            break;
        }
        let closing = i + 1 < bytes.len() && bytes[i + 1] == b'/';
        let mut j = i + 1 + if closing { 1 } else { 0 };
        let name_start = j;
        while j < bytes.len() && is_name(bytes[j]) {
            j += 1;
        }
        if j == name_start {
            i += 1;
            continue;
        }
        let name = local_name(&s[name_start..j]);
        if closing {
            while j < bytes.len() && bytes[j] != b'>' {
                j += 1;
            }
            if j < bytes.len() {
                j += 1;
            }
            // HTML optional close: a new block implicitly closes <p>
            close_tag(&mut stack, &name);
            i = j;
            continue;
        }
        let (attrs, self_close, next) = parse_attrs(s, j)?;
        if matches!(name.as_str(), "p" | "li" | "dt" | "dd" | "tr" | "td" | "th" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
            if stack.last().map(|e| e.name == name).unwrap_or(false) {
                close_tag(&mut stack, &name);
            }
            if name != "li" && name != "td" && name != "th" && name != "tr" && name != "dt" && name != "dd" {
                if stack.last().map(|e| e.name == "p").unwrap_or(false) {
                    close_tag(&mut stack, "p");
                }
            }
        }
        let elem = Elem {
            name: name.clone(),
            attrs,
            children: Vec::new(),
        };
        if self_close || is_void(&name) {
            stack.last_mut().unwrap().children.push(Node::Elem(elem));
        } else if stack.len() < 256 {
            stack.push(elem);
        } else {
            return Err(Error::new("HTML aninhado demais"));
        }
        i = next;
    }
    while stack.len() > 1 {
        let e = stack.pop().unwrap();
        stack.last_mut().unwrap().children.push(Node::Elem(e));
    }
    Ok(stack.pop().unwrap())
}

pub fn find_child<'a>(e: &'a Elem, name: &str) -> Option<&'a Elem> {
    e.children.iter().find_map(|n| match n {
        Node::Elem(c) if c.name == name => Some(c),
        _ => None,
    })
}

pub fn children_named<'a>(e: &'a Elem, name: &str) -> Vec<&'a Elem> {
    e.children
        .iter()
        .filter_map(|n| match n {
            Node::Elem(c) if c.name == name => Some(c),
            _ => None,
        })
        .collect()
}

pub fn descendants_named<'a>(e: &'a Elem, name: &str, out: &mut Vec<&'a Elem>) {
    for n in &e.children {
        if let Node::Elem(c) = n {
            if c.name == name {
                out.push(c);
            }
            descendants_named(c, name, out);
        }
    }
}

const SKIP: &[&str] = &["script", "style", "head", "noscript", "template", "svg", "math"];

pub fn elem_to_blocks(root: &Elem) -> Vec<Block> {
    let body = find_child(root, "body")
        .or_else(|| find_child(root, "html").and_then(|h| find_child(h, "body")))
        .unwrap_or(root);
    let mut blocks = Vec::new();
    walk_blocks(body, &mut blocks);
    clean_blocks(blocks)
}

fn walk_blocks(e: &Elem, out: &mut Vec<Block>) {
    if should_skip(e) {
        return;
    }
    match e.name.as_str() {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
            let level = e.name.as_bytes()[1] - b'0';
            let inlines = inline_children(e);
            if !plain(&inlines).trim().is_empty() {
                out.push(Block::Heading { level, inlines });
            }
        }
        "p" | "subtitle" | "figcaption" => {
            let inlines = inline_children(e);
            if !plain(&inlines).trim().is_empty() {
                out.push(Block::Paragraph { inlines });
            }
        }
        "pre" => {
            let t = e.text();
            if !t.trim().is_empty() {
                out.push(Block::Code { text: t.trim_end().to_string() });
            }
        }
        "blockquote" | "cite" | "aside" => {
            let mut inner = Vec::new();
            walk_blocks(e, &mut inner);
            if inner.is_empty() {
                let inlines = inline_children(e);
                if !plain(&inlines).trim().is_empty() {
                    inner.push(Block::Paragraph { inlines });
                }
            }
            if !inner.is_empty() {
                out.push(Block::Quote { blocks: inner });
            }
        }
        "ul" | "ol" | "menu" => {
            let ordered = e.name == "ol";
            let mut items = Vec::new();
            for n in &e.children {
                if let Node::Elem(c) = n {
                    if c.name == "li" {
                        let mut item = Vec::new();
                        let mut acc = Vec::new();
                        let mut saw_block = false;
                        for ch in &c.children {
                            if let Node::Elem(g) = ch {
                                if is_block_tag(&g.name) {
                                    if !acc.is_empty() {
                                        item.push(Block::Paragraph {
                                            inlines: normalize_inlines(&acc),
                                        });
                                        acc.clear();
                                    }
                                    walk_blocks(g, &mut item);
                                    saw_block = true;
                                } else {
                                    acc.extend(inline_from_elem(g));
                                }
                            } else if let Node::Text(t) = ch {
                                let t = tidy_inline(t);
                                if !t.is_empty() {
                                    acc.push(Inline::Text(t));
                                }
                            }
                        }
                        if !acc.is_empty() {
                            item.push(Block::Paragraph {
                                inlines: normalize_inlines(&acc),
                            });
                        }
                        if item.is_empty() && !saw_block {
                            let inlines = inline_children(c);
                            if !plain(&inlines).trim().is_empty() {
                                item.push(Block::Paragraph { inlines });
                            }
                        }
                        if !item.is_empty() {
                            items.push(item);
                        }
                    }
                }
            }
            if !items.is_empty() {
                out.push(Block::List { ordered, items });
            }
        }
        "table" => {
            let mut rows = Vec::new();
            collect_rows(e, &mut rows);
            if !rows.is_empty() {
                out.push(Block::Table { rows });
            }
        }
        "hr" => out.push(Block::Rule),
        "img" => {
            let alt = e.attr("alt").unwrap_or("").trim().to_string();
            if !alt.is_empty() {
                out.push(Block::Image { alt });
            }
        }
        "br" => {}
        _ => {
            // container: if it only has inline content, emit a paragraph; else descend
            if only_inline(e) {
                let inlines = inline_children(e);
                if !plain(&inlines).trim().is_empty() {
                    out.push(Block::Paragraph { inlines });
                }
            } else {
                for n in &e.children {
                    match n {
                        Node::Elem(c) => walk_blocks(c, out),
                        Node::Text(t) => {
                            let t = tidy_text(t);
                            if !t.is_empty() {
                                out.push(paragraph(t));
                            }
                        }
                    }
                }
            }
        }
    }
}

fn collect_rows(e: &Elem, rows: &mut Vec<Vec<Cell>>) {
    for n in &e.children {
        if let Node::Elem(c) = n {
            if c.name == "tr" {
                let mut row = Vec::new();
                for celln in &c.children {
                    if let Node::Elem(cell) = celln {
                        if cell.name == "td" || cell.name == "th" {
                            row.push(Cell {
                                header: cell.name == "th",
                                inlines: inline_children(cell),
                            });
                        }
                    }
                }
                if !row.is_empty() {
                    rows.push(row);
                }
            } else {
                collect_rows(c, rows);
            }
        }
    }
}

fn should_skip(e: &Elem) -> bool {
    if SKIP.contains(&e.name.as_str()) {
        return true;
    }
    if e.attr("hidden").is_some() {
        return true;
    }
    if e.attr("aria-hidden").map(|v| v == "true").unwrap_or(false) && e.attr("epub:type").is_none() {
        // keep aria-hidden only when it is clearly decorative and has no text of substance? 
        // many EPUBs hide page numbers this way. Skip if the text is short.
        let t = e.text();
        if t.trim().len() < 12 {
            return true;
        }
    }
    let epub_type = e.attr("epub:type").unwrap_or("").to_ascii_lowercase();
    let role = e.attr("role").unwrap_or("");
    if epub_type.split_whitespace().any(|t| t == "pagebreak") || role == "doc-pagebreak" {
        return true;
    }
    if let Some(style) = e.attr("style") {
        let s = style.replace(' ', "").to_ascii_lowercase();
        if s.contains("display:none") || s.contains("visibility:hidden") {
            return true;
        }
    }
    false
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "table"
            | "blockquote"
            | "pre"
            | "hr"
            | "figure"
            | "aside"
            | "dl"
            | "tr"
    )
}

fn only_inline(e: &Elem) -> bool {
    e.children.iter().all(|n| match n {
        Node::Text(_) => true,
        Node::Elem(c) => !is_block_tag(&c.name) && c.name != "ul" && c.name != "ol" && c.name != "table",
    })
}

fn inline_children(e: &Elem) -> Vec<Inline> {
    let mut out = Vec::new();
    for n in &e.children {
        match n {
            Node::Text(t) => {
                let t = tidy_inline(t);
                if !t.is_empty() {
                    out.push(Inline::Text(t));
                }
            }
            Node::Elem(c) => out.extend(inline_from_elem(c)),
        }
    }
    normalize_inlines(&out)
}

fn inline_from_elem(e: &Elem) -> Vec<Inline> {
    if should_skip(e) {
        return Vec::new();
    }
    match e.name.as_str() {
        "br" => vec![Inline::Break],
        "img" => {
            let alt = e.attr("alt").unwrap_or("").trim().to_string();
            if alt.is_empty() {
                Vec::new()
            } else {
                text_inline(format!("[{alt}]"))
            }
        }
        "strong" | "b" => vec![Inline::Strong(inline_children(e))],
        "em" | "i" | "cite" | "dfn" => vec![Inline::Emph(inline_children(e))],
        "s" | "strike" | "del" => vec![Inline::Strike(inline_children(e))],
        "code" | "kbd" | "samp" => vec![Inline::Code(e.text())],
        "sup" => vec![Inline::Sup(inline_children(e))],
        "sub" => vec![Inline::Sub(inline_children(e))],
        "a" => {
            let href = e.attr("href").unwrap_or("").to_string();
            let children = inline_children(e);
            if href.is_empty() || href.starts_with('#') {
                children
            } else if href.to_ascii_lowercase().starts_with("javascript:") {
                children
            } else {
                vec![Inline::Link { href, children }]
            }
        }
        _ => inline_children(e),
    }
}

pub fn first_text(e: &Elem) -> String {
    tidy_text(&e.text())
}
