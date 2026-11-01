use crate::entities::{html_escape, xml_escape};
use crate::model::*;

pub fn write_txt(doc: &Document) -> String {
    let mut out = String::new();
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        out.push_str(&doc.meta.title);
        out.push_str("\n\n");
    }
    if !doc.meta.authors.is_empty() {
        out.push_str(&doc.meta.authors.join(", "));
        out.push_str("\n\n");
    }
    let mut first_h1 = true;
    write_blocks_txt(&doc.blocks, &mut out, 0, &mut first_h1);
    while out.ends_with('\n') {
        out.pop();
    }
    out.push('\n');
    out
}

fn write_blocks_txt(blocks: &[Block], out: &mut String, depth: usize, first_h1: &mut bool) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => {
                let t = plain(inlines).trim().to_string();
                if t.is_empty() {
                    continue;
                }
                if *level == 1 && !*first_h1 {
                    out.push_str("\n————\n\n");
                }
                if *level == 1 {
                    *first_h1 = false;
                }
                out.push_str(&t);
                out.push_str("\n\n");
            }
            Block::Paragraph { inlines } => {
                let t = inline_txt(inlines);
                if t.is_empty() {
                    continue;
                }
                indent(out, depth);
                out.push_str(&t);
                out.push_str("\n\n");
            }
            Block::List { ordered, items } => {
                for (i, item) in items.iter().enumerate() {
                    indent(out, depth);
                    if *ordered {
                        out.push_str(&format!("{}. ", i + 1));
                    } else {
                        out.push_str("• ");
                    }
                    let mut first = true;
                    for block in item {
                        match block {
                            Block::Paragraph { inlines } => {
                                if !first {
                                    out.push('\n');
                                    indent(out, depth + 1);
                                }
                                out.push_str(&inline_txt(inlines));
                                first = false;
                            }
                            Block::List { .. } => {
                                out.push('\n');
                                write_blocks_txt(std::slice::from_ref(block), out, depth + 1, first_h1);
                            }
                            other => {
                                if !first {
                                    out.push('\n');
                                }
                                write_blocks_txt(std::slice::from_ref(other), out, depth + 1, first_h1);
                                first = false;
                            }
                        }
                    }
                    if !out.ends_with('\n') {
                        out.push('\n');
                    }
                }
                out.push('\n');
            }
            Block::Quote { blocks } => {
                let mut inner = String::new();
                let mut fh = true;
                write_blocks_txt(blocks, &mut inner, 0, &mut fh);
                for line in inner.lines() {
                    if line.trim().is_empty() {
                        out.push('\n');
                    } else {
                        indent(out, depth);
                        out.push_str("    ");
                        out.push_str(line.trim());
                        out.push('\n');
                    }
                }
                out.push('\n');
            }
            Block::Code { text } => {
                for line in text.lines() {
                    indent(out, depth);
                    out.push_str("    ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::Table { rows } => {
                let widths = col_widths(rows);
                for row in rows {
                    indent(out, depth);
                    for (i, cell) in row.iter().enumerate() {
                        if i > 0 {
                            out.push_str("  ");
                        }
                        let t = plain(&cell.inlines);
                        out.push_str(&t);
                        let w = widths.get(i).copied().unwrap_or(0);
                        let pad = w.saturating_sub(t.chars().count());
                        for _ in 0..pad {
                            out.push(' ');
                        }
                    }
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::Rule => out.push_str("———\n\n"),
            Block::Image { alt } => {
                if !alt.trim().is_empty() {
                    indent(out, depth);
                    out.push_str("[Imagem: ");
                    out.push_str(alt.trim());
                    out.push_str("]\n\n");
                }
            }
        }
    }
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn inline_txt(inlines: &[Inline]) -> String {
    let mut s = String::new();
    fn rec(i: &[Inline], s: &mut String) {
        for n in i {
            match n {
                Inline::Text(t) | Inline::Code(t) => s.push_str(t),
                Inline::Strong(c) | Inline::Emph(c) | Inline::Strike(c) | Inline::Sup(c) | Inline::Sub(c) => rec(c, s),
                Inline::Link { href, children } => {
                    rec(children, s);
                    if !href.is_empty() && !href.starts_with('#') {
                        s.push_str(" (");
                        s.push_str(href);
                        s.push(')');
                    }
                }
                Inline::Break => s.push('\n'),
            }
        }
    }
    rec(inlines, &mut s);
    tidy_text(&s)
}

fn col_widths(rows: &[Vec<Cell>]) -> Vec<usize> {
    let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut w = vec![0; cols];
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            w[i] = w[i].max(plain(&c.inlines).chars().count());
        }
    }
    w
}

pub fn write_md(doc: &Document) -> String {
    let mut out = String::new();
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        out.push_str("# ");
        out.push_str(&doc.meta.title);
        out.push_str("\n\n");
    }
    if !doc.meta.authors.is_empty() {
        out.push_str(&doc.meta.authors.join(", "));
        out.push_str("\n\n");
    }
    write_md_blocks(&doc.blocks, &mut out, 0);
    out
}

fn write_md_blocks(blocks: &[Block], out: &mut String, depth: usize) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => {
                let lv = (*level).clamp(1, 6) as usize;
                for _ in 0..lv {
                    out.push('#');
                }
                out.push(' ');
                out.push_str(&inline_md(inlines));
                out.push_str("\n\n");
            }
            Block::Paragraph { inlines } => {
                out.push_str(&inline_md(inlines));
                out.push_str("\n\n");
            }
            Block::List { ordered, items } => {
                for (i, item) in items.iter().enumerate() {
                    for _ in 0..depth {
                        out.push_str("  ");
                    }
                    if *ordered {
                        out.push_str(&format!("{}. ", i + 1));
                    } else {
                        out.push_str("- ");
                    }
                    let mut first = true;
                    for block in item {
                        if let Block::Paragraph { inlines } = block {
                            if !first {
                                out.push('\n');
                            }
                            out.push_str(&inline_md(inlines));
                            first = false;
                        } else {
                            out.push('\n');
                            write_md_blocks(std::slice::from_ref(block), out, depth + 1);
                        }
                    }
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::Quote { blocks } => {
                let mut inner = String::new();
                write_md_blocks(blocks, &mut inner, 0);
                for line in inner.lines() {
                    out.push_str("> ");
                    out.push_str(line);
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::Code { text } => {
                out.push_str("```\n");
                out.push_str(text);
                if !text.ends_with('\n') {
                    out.push('\n');
                }
                out.push_str("```\n\n");
            }
            Block::Table { rows } => {
                if rows.is_empty() {
                    continue;
                }
                let cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
                let cell = |r: &Vec<Cell>, i: usize| {
                    rows_cell(r, i)
                };
                out.push('|');
                for i in 0..cols {
                    out.push(' ');
                    out.push_str(&cell(&rows[0], i));
                    out.push_str(" |");
                }
                out.push('\n');
                out.push('|');
                for _ in 0..cols {
                    out.push_str(" --- |");
                }
                out.push('\n');
                for row in rows.iter().skip(1) {
                    out.push('|');
                    for i in 0..cols {
                        out.push(' ');
                        out.push_str(&cell(row, i));
                        out.push_str(" |");
                    }
                    out.push('\n');
                }
                out.push('\n');
            }
            Block::Rule => out.push_str("---\n\n"),
            Block::Image { alt } => {
                out.push_str("![");
                out.push_str(alt);
                out.push_str("]()\n\n");
            }
        }
    }
}

fn rows_cell(row: &[Cell], i: usize) -> String {
    row.get(i).map(|c| inline_md(&c.inlines).replace('|', "\\|")).unwrap_or_default()
}

fn inline_md(inlines: &[Inline]) -> String {
    let mut s = String::new();
    fn rec(i: &[Inline], s: &mut String) {
        for n in i {
            match n {
                Inline::Text(t) => s.push_str(&escape_md_text(t)),
                Inline::Code(t) => {
                    s.push('`');
                    s.push_str(t);
                    s.push('`');
                }
                Inline::Strong(c) => {
                    s.push_str("**");
                    rec(c, s);
                    s.push_str("**");
                }
                Inline::Emph(c) => {
                    s.push('*');
                    rec(c, s);
                    s.push('*');
                }
                Inline::Strike(c) => {
                    s.push_str("~~");
                    rec(c, s);
                    s.push_str("~~");
                }
                Inline::Sup(c) | Inline::Sub(c) => rec(c, s),
                Inline::Link { href, children } => {
                    s.push('[');
                    rec(children, s);
                    s.push_str("](");
                    s.push_str(href);
                    s.push(')');
                }
                Inline::Break => s.push_str("  \n"),
            }
        }
    }
    rec(inlines, &mut s);
    s
}

fn escape_md_text(s: &str) -> String {
    s.replace('\\', "\\\\")
}

pub fn write_html_fragment(doc: &Document) -> String {
    let mut out = String::new();
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        out.push_str("<h1>");
        out.push_str(&html_escape(&doc.meta.title));
        out.push_str("</h1>");
    }
    if !doc.meta.authors.is_empty() {
        out.push_str("<p class=\"by\">");
        out.push_str(&html_escape(&doc.meta.authors.join(", ")));
        out.push_str("</p>");
    }
    write_html_blocks(&doc.blocks, &mut out);
    out
}

pub fn write_html_document(doc: &Document) -> String {
    let title = if doc.meta.title.is_empty() {
        "Documento".to_string()
    } else {
        doc.meta.title.clone()
    };
    format!(
        "<!DOCTYPE html>\n<html lang=\"{}\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n<style>{}</style>\n</head>\n<body>\n<article>\n{}\n</article>\n</body>\n</html>\n",
        html_escape(if doc.meta.language.is_empty() { "pt" } else { &doc.meta.language }),
        html_escape(&title),
        PAPER_CSS,
        write_html_fragment(doc)
    )
}

const PAPER_CSS: &str = r#"
:root { color-scheme: light; }
html { background: #e7e1d6; }
body { margin: 0; font-family: "Newsreader", "Iowan Old Style", Palatino, "Palatino Linotype", serif; color: #1c1915; }
article { max-width: 68ch; margin: 0 auto; padding: 56px 28px 80px; background: #f7f3ea; min-height: 100vh; }
h1, h2, h3, h4 { font-weight: 560; letter-spacing: -0.02em; line-height: 1.15; }
h1 { font-size: 2rem; margin: 0 0 0.6rem; }
h2 { font-size: 1.45rem; margin: 2rem 0 0.6rem; }
h3 { font-size: 1.2rem; margin: 1.6rem 0 0.4rem; }
p { font-size: 1.08rem; line-height: 1.55; margin: 0 0 0.9rem; }
.by { color: #5c564c; font-style: italic; }
ul, ol { padding-left: 1.2rem; }
li { margin: 0.25rem 0; line-height: 1.45; }
blockquote { margin: 1rem 0; padding-left: 1rem; border-left: 1px solid #c4b8a4; color: #3a342c; }
pre { font-family: "DejaVu Sans Mono", ui-monospace, monospace; font-size: 0.92rem; background: #efeae0; padding: 12px 14px; overflow: auto; }
table { border-collapse: collapse; width: 100%; margin: 1rem 0 1.4rem; font-size: 0.98rem; }
th, td { border-bottom: 1px solid #d9d0c2; text-align: left; padding: 6px 8px; vertical-align: top; }
th { font-weight: 600; }
a { color: #8d2e22; }
hr { border: 0; border-top: 1px solid #d9d0c2; margin: 1.6rem 0; }
"#;

fn write_html_blocks(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => {
                let lv = (*level).clamp(1, 6);
                out.push_str(&format!("<h{lv}>"));
                write_html_inlines(inlines, out);
                out.push_str(&format!("</h{lv}>"));
            }
            Block::Paragraph { inlines } => {
                out.push_str("<p>");
                write_html_inlines(inlines, out);
                out.push_str("</p>");
            }
            Block::List { ordered, items } => {
                out.push_str(if *ordered { "<ol>" } else { "<ul>" });
                for item in items {
                    out.push_str("<li>");
                    if item.len() == 1 {
                        if let Block::Paragraph { inlines } = &item[0] {
                            write_html_inlines(inlines, out);
                        } else {
                            write_html_blocks(item, out);
                        }
                    } else {
                        write_html_blocks(item, out);
                    }
                    out.push_str("</li>");
                }
                out.push_str(if *ordered { "</ol>" } else { "</ul>" });
            }
            Block::Quote { blocks } => {
                out.push_str("<blockquote>");
                write_html_blocks(blocks, out);
                out.push_str("</blockquote>");
            }
            Block::Code { text } => {
                out.push_str("<pre>");
                out.push_str(&html_escape(text));
                out.push_str("</pre>");
            }
            Block::Table { rows } => {
                out.push_str("<table>");
                for (i, row) in rows.iter().enumerate() {
                    out.push_str("<tr>");
                    for cell in row {
                        let tag = if cell.header || i == 0 { "th" } else { "td" };
                        out.push_str(&format!("<{tag}>"));
                        write_html_inlines(&cell.inlines, out);
                        out.push_str(&format!("</{tag}>"));
                    }
                    out.push_str("</tr>");
                }
                out.push_str("</table>");
            }
            Block::Rule => out.push_str("<hr>"),
            Block::Image { alt } => {
                out.push_str("<p>[");
                out.push_str(&html_escape(alt));
                out.push_str("]</p>");
            }
        }
    }
}

fn write_html_inlines(inlines: &[Inline], out: &mut String) {
    for n in inlines {
        match n {
            Inline::Text(t) => out.push_str(&html_escape(t)),
            Inline::Code(t) => {
                out.push_str("<code>");
                out.push_str(&html_escape(t));
                out.push_str("</code>");
            }
            Inline::Strong(c) => {
                out.push_str("<strong>");
                write_html_inlines(c, out);
                out.push_str("</strong>");
            }
            Inline::Emph(c) => {
                out.push_str("<em>");
                write_html_inlines(c, out);
                out.push_str("</em>");
            }
            Inline::Strike(c) => {
                out.push_str("<s>");
                write_html_inlines(c, out);
                out.push_str("</s>");
            }
            Inline::Sup(c) => {
                out.push_str("<sup>");
                write_html_inlines(c, out);
                out.push_str("</sup>");
            }
            Inline::Sub(c) => {
                out.push_str("<sub>");
                write_html_inlines(c, out);
                out.push_str("</sub>");
            }
            Inline::Link { href, children } => {
                let safe = if href.to_ascii_lowercase().starts_with("javascript:") {
                    String::new()
                } else {
                    href.clone()
                };
                out.push_str("<a href=\"");
                out.push_str(&html_escape(&safe));
                out.push_str("\">");
                write_html_inlines(children, out);
                out.push_str("</a>");
            }
            Inline::Break => out.push_str("<br>"),
        }
    }
}

pub fn write_rtf(doc: &Document) -> Vec<u8> {
    let mut s = String::from("{\\rtf1\\ansi\\deff0{\\fonttbl{\\f0 Times New Roman;}}\n\\fs24\n");
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        s.push_str("{\\b\\fs36 ");
        s.push_str(&rtf_escape(&doc.meta.title));
        s.push_str("}\\par\\par\n");
    }
    write_rtf_blocks(&doc.blocks, &mut s);
    s.push('}');
    s.into_bytes()
}

fn write_rtf_blocks(blocks: &[Block], s: &mut String) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => {
                let fs = match level {
                    1 => 36,
                    2 => 30,
                    3 => 26,
                    _ => 24,
                };
                s.push_str(&format!("{{\\b\\fs{fs} "));
                write_rtf_inlines(inlines, s);
                s.push_str("}\\par\\par\n");
            }
            Block::Paragraph { inlines } => {
                write_rtf_inlines(inlines, s);
                s.push_str("\\par\\par\n");
            }
            Block::List { ordered, items } => {
                for (i, item) in items.iter().enumerate() {
                    if *ordered {
                        s.push_str(&format!("{}. ", i + 1));
                    } else {
                        s.push_str("\\u8226? ");
                    }
                    for block in item {
                        if let Block::Paragraph { inlines } = block {
                            write_rtf_inlines(inlines, s);
                        }
                    }
                    s.push_str("\\par\n");
                }
                s.push_str("\\par\n");
            }
            Block::Quote { blocks } => {
                s.push_str("\\li360 ");
                write_rtf_blocks(blocks, s);
                s.push_str("\\li0 ");
            }
            Block::Code { text } => {
                s.push_str("{\\f0\\fs20 ");
                s.push_str(&rtf_escape(text));
                s.push_str("}\\par\\par\n");
            }
            Block::Table { rows } => {
                for row in rows {
                    for (i, c) in row.iter().enumerate() {
                        if i > 0 {
                            s.push_str("\\tab ");
                        }
                        write_rtf_inlines(&c.inlines, s);
                    }
                    s.push_str("\\par\n");
                }
                s.push_str("\\par\n");
            }
            Block::Rule => s.push_str("\\par ——— \\par\n"),
            Block::Image { alt } => {
                s.push_str("[");
                s.push_str(&rtf_escape(alt));
                s.push_str("]\\par\n");
            }
        }
    }
}

fn write_rtf_inlines(inlines: &[Inline], s: &mut String) {
    for n in inlines {
        match n {
            Inline::Text(t) | Inline::Code(t) => s.push_str(&rtf_escape(t)),
            Inline::Strong(c) => {
                s.push_str("{\\b ");
                write_rtf_inlines(c, s);
                s.push('}');
            }
            Inline::Emph(c) => {
                s.push_str("{\\i ");
                write_rtf_inlines(c, s);
                s.push('}');
            }
            Inline::Strike(c) | Inline::Sup(c) | Inline::Sub(c) => write_rtf_inlines(c, s),
            Inline::Link { children, .. } => write_rtf_inlines(children, s),
            Inline::Break => s.push_str("\\line "),
        }
    }
}

fn rtf_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\line "),
            c if (c as u32) < 128 => out.push(c),
            c => {
                let mut n = c as i32;
                if n > 32767 {
                    n -= 65536;
                }
                out.push_str(&format!("\\u{n}?"));
            }
        }
    }
    out
}

pub fn write_latex(doc: &Document) -> String {
    let mut s = String::from("\\documentclass[11pt]{article}\n\\usepackage[utf8]{inputenc}\n\\usepackage[T1]{fontenc}\n\\usepackage[brazilian]{babel}\n");
    if !doc.meta.title.is_empty() {
        s.push_str("\\title{");
        s.push_str(&latex_escape(&doc.meta.title));
        s.push_str("}\n");
    }
    if !doc.meta.authors.is_empty() {
        s.push_str("\\author{");
        s.push_str(&latex_escape(&doc.meta.authors.join(", ")));
        s.push_str("}\n");
    }
    s.push_str("\\begin{document}\n");
    if !doc.meta.title.is_empty() && !body_opens_with_title(doc) {
        s.push_str("\\maketitle\n");
    }
    write_latex_blocks(&doc.blocks, &mut s);
    s.push_str("\\end{document}\n");
    s
}

fn write_latex_blocks(blocks: &[Block], s: &mut String) {
    for b in blocks {
        match b {
            Block::Heading { level, inlines } => {
                let cmd = match level {
                    1 => "section",
                    2 => "subsection",
                    3 => "subsubsection",
                    _ => "paragraph",
                };
                s.push('\\');
                s.push_str(cmd);
                s.push('{');
                s.push_str(&latex_escape(&plain(inlines)));
                s.push_str("}\n");
            }
            Block::Paragraph { inlines } => {
                s.push_str(&latex_inlines(inlines));
                s.push_str("\n\n");
            }
            Block::List { ordered, items } => {
                s.push_str(if *ordered { "\\begin{enumerate}\n" } else { "\\begin{itemize}\n" });
                for item in items {
                    s.push_str("\\item ");
                    for block in item {
                        if let Block::Paragraph { inlines } = block {
                            s.push_str(&latex_inlines(inlines));
                        }
                    }
                    s.push('\n');
                }
                s.push_str(if *ordered { "\\end{enumerate}\n" } else { "\\end{itemize}\n" });
            }
            Block::Quote { blocks } => {
                s.push_str("\\begin{quote}\n");
                write_latex_blocks(blocks, s);
                s.push_str("\\end{quote}\n");
            }
            Block::Code { text } => {
                s.push_str("\\begin{verbatim}\n");
                s.push_str(text);
                s.push_str("\n\\end{verbatim}\n");
            }
            Block::Table { rows } => {
                for row in rows {
                    for (i, c) in row.iter().enumerate() {
                        if i > 0 {
                            s.push_str(" & ");
                        }
                        s.push_str(&latex_escape(&plain(&c.inlines)));
                    }
                    s.push_str(" \\\\\n");
                }
                s.push('\n');
            }
            Block::Rule => s.push_str("\\hrulefill\n"),
            Block::Image { alt } => {
                s.push_str("% imagem: ");
                s.push_str(&latex_escape(alt));
                s.push('\n');
            }
        }
    }
}

fn latex_inlines(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for n in inlines {
        match n {
            Inline::Text(t) | Inline::Code(t) => s.push_str(&latex_escape(t)),
            Inline::Strong(c) => {
                s.push_str("\\textbf{");
                s.push_str(&latex_inlines(c));
                s.push('}');
            }
            Inline::Emph(c) => {
                s.push_str("\\emph{");
                s.push_str(&latex_inlines(c));
                s.push('}');
            }
            Inline::Link { href, children } => {
                s.push_str(&latex_inlines(children));
                s.push_str(" (\\url{");
                s.push_str(&latex_escape(href));
                s.push_str("})");
            }
            Inline::Strike(c) | Inline::Sup(c) | Inline::Sub(c) => s.push_str(&latex_inlines(c)),
            Inline::Break => s.push_str("\\\\\n"),
        }
    }
    s
}

fn latex_escape(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '$' => out.push_str("\\$"),
            '&' => out.push_str("\\&"),
            '%' => out.push_str("\\%"),
            '#' => out.push_str("\\#"),
            '_' => out.push_str("\\_"),
            '~' => out.push_str("\\textasciitilde{}"),
            '^' => out.push_str("\\textasciicircum{}"),
            c => out.push(c),
        }
    }
    out
}

pub fn write_json(doc: &Document) -> String {
    let mut s = String::from("{\n  \"convertetudo\": 1,\n  \"meta\": {\n");
    s.push_str(&format!("    \"title\": {},\n", json_str(&doc.meta.title)));
    s.push_str("    \"authors\": [");
    for (i, a) in doc.meta.authors.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&json_str(a));
    }
    s.push_str("],\n");
    s.push_str(&format!("    \"language\": {},\n", json_str(&doc.meta.language)));
    s.push_str(&format!("    \"source\": {}\n", json_str(&doc.meta.source_format)));
    s.push_str("  },\n  \"text\": ");
    s.push_str(&json_str(&write_txt(doc)));
    s.push_str(",\n  \"warnings\": [");
    for (i, w) in doc.warnings.iter().enumerate() {
        if i > 0 {
            s.push_str(", ");
        }
        s.push_str(&json_str(w));
    }
    s.push_str("]\n}\n");
    s
}

fn json_str(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 32 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn write_csv(doc: &Document) -> String {
    let mut rows: Vec<Vec<String>> = Vec::new();
    fn grab(blocks: &[Block], rows: &mut Vec<Vec<String>>) {
        for b in blocks {
            match b {
                Block::Table { rows: table } => {
                    for r in table {
                        rows.push(r.iter().map(|c| plain(&c.inlines)).collect());
                    }
                }
                Block::List { items, .. } => {
                    for it in items {
                        grab(it, rows);
                    }
                }
                Block::Quote { blocks } => grab(blocks, rows),
                Block::Paragraph { inlines } | Block::Heading { inlines, .. } => {
                    rows.push(vec![plain(inlines)]);
                }
                Block::Code { text } => rows.push(vec![text.clone()]),
                _ => {}
            }
        }
    }
    grab(&doc.blocks, &mut rows);
    let mut out = String::new();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if c.contains([',', '"', '\n']) {
                out.push('"');
                out.push_str(&c.replace('"', "\"\""));
                out.push('"');
            } else {
                out.push_str(c);
            }
        }
        out.push('\n');
    }
    out
}

pub fn xml_escape_pub(s: &str) -> String {
    xml_escape(s)
}
