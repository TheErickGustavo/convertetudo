#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub title: String,
    pub authors: Vec<String>,
    pub language: String,
    pub identifier: String,
    pub description: String,
    pub date: String,
    pub publisher: String,
    pub subjects: Vec<String>,
    pub source_format: String,
}

#[derive(Clone, Debug)]
pub struct Document {
    pub meta: Meta,
    pub blocks: Vec<Block>,
    pub warnings: Vec<String>,
}

impl Document {
    pub fn new(source_format: &str) -> Self {
        let mut meta = Meta::default();
        meta.source_format = source_format.to_string();
        meta.language = "pt".to_string();
        Self {
            meta,
            blocks: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn warn(&mut self, msg: impl Into<String>) {
        self.warnings.push(msg.into());
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

#[derive(Clone, Debug)]
pub struct Cell {
    pub header: bool,
    pub inlines: Vec<Inline>,
}

#[derive(Clone, Debug)]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline> },
    Paragraph { inlines: Vec<Inline> },
    List { ordered: bool, items: Vec<Vec<Block>> },
    Quote { blocks: Vec<Block> },
    Code { text: String },
    Table { rows: Vec<Vec<Cell>> },
    Rule,
    Image { alt: String },
}

#[derive(Clone, Debug)]
pub enum Inline {
    Text(String),
    Strong(Vec<Inline>),
    Emph(Vec<Inline>),
    Strike(Vec<Inline>),
    Code(String),
    Link { href: String, children: Vec<Inline> },
    Sup(Vec<Inline>),
    Sub(Vec<Inline>),
    Break,
}

pub fn plain(inlines: &[Inline]) -> String {
    let mut s = String::new();
    push_plain(&mut s, inlines);
    s
}

fn push_plain(out: &mut String, inlines: &[Inline]) {
    for i in inlines {
        match i {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Strong(c)
            | Inline::Emph(c)
            | Inline::Strike(c)
            | Inline::Link { children: c, .. }
            | Inline::Sup(c)
            | Inline::Sub(c) => push_plain(out, c),
            Inline::Break => out.push(' '),
        }
    }
}

pub fn text_inline(s: impl Into<String>) -> Vec<Inline> {
    let s = s.into();
    if s.is_empty() {
        Vec::new()
    } else {
        vec![Inline::Text(s)]
    }
}

pub fn paragraph(s: impl Into<String>) -> Block {
    Block::Paragraph {
        inlines: text_inline(s),
    }
}

pub fn heading(level: u8, s: impl Into<String>) -> Block {
    Block::Heading {
        level: level.clamp(1, 6),
        inlines: text_inline(s),
    }
}

/// Collapse whitespace inside inline text but keep explicit breaks.
pub fn normalize_inlines(inlines: &[Inline]) -> Vec<Inline> {
    let mut out = Vec::new();
    fn rec(src: &[Inline], out: &mut Vec<Inline>) {
        for i in src {
            match i {
                Inline::Text(t) => {
                    let t = tidy_inline(t);
                    if !t.is_empty() {
                        out.push(Inline::Text(t));
                    }
                }
                Inline::Code(t) => out.push(Inline::Code(t.clone())),
                Inline::Break => out.push(Inline::Break),
                Inline::Strong(c) => {
                    let c = normalize_inlines(c);
                    if !c.is_empty() {
                        out.push(Inline::Strong(c));
                    }
                }
                Inline::Emph(c) => {
                    let c = normalize_inlines(c);
                    if !c.is_empty() {
                        out.push(Inline::Emph(c));
                    }
                }
                Inline::Strike(c) => {
                    let c = normalize_inlines(c);
                    if !c.is_empty() {
                        out.push(Inline::Strike(c));
                    }
                }
                Inline::Sup(c) => {
                    let c = normalize_inlines(c);
                    if !c.is_empty() {
                        out.push(Inline::Sup(c));
                    }
                }
                Inline::Sub(c) => {
                    let c = normalize_inlines(c);
                    if !c.is_empty() {
                        out.push(Inline::Sub(c));
                    }
                }
                Inline::Link { href, children } => {
                    let children = normalize_inlines(children);
                    if !children.is_empty() {
                        out.push(Inline::Link {
                            href: href.clone(),
                            children,
                        });
                    }
                }
            }
        }
    }
    rec(inlines, &mut out);
    merge_text(&mut out);
    out
}

fn merge_text(inlines: &mut Vec<Inline>) {
    let mut i = 0;
    while i + 1 < inlines.len() {
        if let (Inline::Text(a), Inline::Text(b)) = (&inlines[i], &inlines[i + 1]) {
            let mut joined = a.clone();
            joined.push_str(b);
            inlines[i] = Inline::Text(joined);
            inlines.remove(i + 1);
        } else {
            i += 1;
        }
    }
}

/// Like `tidy_text`, but a space at either edge of an inline fragment is kept.
/// HTML puts the space next to a tag in the adjacent text node; dropping it
/// glues “em <em>Nova</em> com” into “emNovacom”.
pub fn tidy_inline(s: &str) -> String {
    let lead = s.chars().next().is_some_and(|c| c.is_whitespace());
    let trail = s.chars().next_back().is_some_and(|c| c.is_whitespace());
    let mut core = tidy_text(s);
    if core.is_empty() {
        return if lead || trail { " ".into() } else { String::new() };
    }
    if lead {
        core.insert(0, ' ');
    }
    if trail {
        core.push(' ');
    }
    core
}

pub fn tidy_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for ch in s.chars() {
        if ch == '\u{00ad}' || ch == '\u{200b}' || ch == '\u{feff}' {
            continue;
        }
        if ch.is_whitespace() {
            space = true;
            continue;
        }
        if space && !out.is_empty() {
            out.push(' ');
        }
        space = false;
        out.push(ch);
    }
    out
}

/// True when the body already starts with a heading equal to the title.
/// Writers use this so “Recibo” is not printed twice.
pub fn body_opens_with_title(doc: &Document) -> bool {
    let title = doc.meta.title.trim();
    if title.is_empty() {
        return false;
    }
    for b in &doc.blocks {
        if block_is_empty(b) {
            continue;
        }
        return match b {
            Block::Heading { inlines, .. } => plain(inlines).trim() == title,
            _ => false,
        };
    }
    false
}

pub fn block_is_empty(b: &Block) -> bool {
    match b {
        Block::Heading { inlines, .. } | Block::Paragraph { inlines } => {
            plain(inlines).trim().is_empty()
        }
        Block::List { items, .. } => items.iter().all(|it| it.iter().all(block_is_empty)),
        Block::Quote { blocks } => blocks.iter().all(block_is_empty),
        Block::Code { text } => text.trim().is_empty(),
        Block::Table { rows } => rows.is_empty(),
        Block::Rule => false,
        Block::Image { alt } => alt.trim().is_empty(),
    }
}

pub fn clean_blocks(blocks: Vec<Block>) -> Vec<Block> {
    let mut out = Vec::new();
    for b in blocks {
        let b = clean_block(b);
        if !block_is_empty(&b) {
            out.push(b);
        }
    }
    out
}

fn clean_block(b: Block) -> Block {
    match b {
        Block::Heading { level, inlines } => Block::Heading {
            level,
            inlines: normalize_inlines(&inlines),
        },
        Block::Paragraph { inlines } => Block::Paragraph {
            inlines: normalize_inlines(&inlines),
        },
        Block::List { ordered, items } => Block::List {
            ordered,
            items: items
                .into_iter()
                .map(|it| clean_blocks(it))
                .filter(|it| !it.is_empty())
                .collect(),
        },
        Block::Quote { blocks } => Block::Quote {
            blocks: clean_blocks(blocks),
        },
        other => other,
    }
}
