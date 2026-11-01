use crate::error::{Error, Result};
use crate::inflate::{inflate_raw, inflate_zlib};
use crate::model::*;

pub fn read_pdf(data: &[u8]) -> Result<Document> {
    if !data.starts_with(b"%PDF") {
        return Err(Error::new("isto não é um PDF (o arquivo não começa com %PDF)."));
    }
    if data.windows(8).any(|w| w == b"/Encrypt") {
        return Err(Error::new(
            "este PDF está criptografado. Abra-o sem senha e exporte de novo — não quebro senha.",
        ));
    }
    let mut doc = Document::new("pdf");
    let streams = collect_streams(data);
    let mut pages = 0usize;
    for stream in &streams {
        if !looks_like_content(stream) {
            continue;
        }
        let text = extract_content(stream);
        let text = tidy_pdf_text(&text);
        if text.trim().is_empty() {
            continue;
        }
        pages += 1;
        for para in text.split("\n\n") {
            let p = para.trim();
            if p.is_empty() {
                continue;
            }
            if p.chars().count() < 80 && !p.contains('.') && pages == 1 && doc.meta.title.is_empty() {
                doc.meta.title = p.to_string();
                doc.blocks.push(heading(1, p));
            } else {
                doc.blocks.push(paragraph(p.replace('\n', " ")));
            }
        }
        doc.blocks.push(Block::Rule);
    }
    if doc.blocks.last().map(|b| matches!(b, Block::Rule)).unwrap_or(false) {
        doc.blocks.pop();
    }
    doc.blocks = clean_blocks(doc.blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new(
            "este PDF não tem camada de texto. Se for uma página escaneada, OCR ainda não entra — o arquivo precisa ter texto selecionável.",
        ));
    }
    if pages > 0 {
        doc.warn(format!("texto extraído de {pages} fluxo(s) de página. O PDF não guarda parágrafos; a divisão é uma reconstrução."));
    }
    Ok(doc)
}

fn collect_streams(data: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + 6 < data.len() {
        if &data[i..i + 6] == b"stream" {
            let mut start = i + 6;
            if start < data.len() && data[start] == b'\r' {
                start += 1;
            }
            if start < data.len() && data[start] == b'\n' {
                start += 1;
            }
            if let Some(rel) = find_endstream(&data[start..]) {
                let raw = &data[start..start + rel];
                let dict = lookback_dict(data, i);
                let decoded = decode_stream(raw, &dict);
                if decoded.len() > 8 {
                    out.push(decoded);
                }
                i = start + rel;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn find_endstream(data: &[u8]) -> Option<usize> {
    let needle = b"endstream";
    let mut i = 0;
    while i + needle.len() <= data.len() {
        if &data[i..i + needle.len()] == needle {
            let mut end = i;
            if end > 0 && data[end - 1] == b'\n' {
                end -= 1;
            }
            if end > 0 && data[end - 1] == b'\r' {
                end -= 1;
            }
            return Some(end);
        }
        i += 1;
    }
    None
}

fn lookback_dict(data: &[u8], stream_at: usize) -> String {
    let from = stream_at.saturating_sub(800);
    String::from_utf8_lossy(&data[from..stream_at]).into_owned()
}

fn decode_stream(raw: &[u8], dict: &str) -> Vec<u8> {
    let d = dict.to_ascii_lowercase();
    let mut cur = raw.to_vec();
    if d.contains("flatedecode") || d.contains("/fl") {
        if let Ok(v) = inflate_zlib(&cur).or_else(|_| inflate_raw(&cur)) {
            cur = v;
        } else if cur.len() > 2 {
            if let Ok(v) = inflate_zlib(&cur[2..]).or_else(|_| inflate_raw(&cur)) {
                cur = v;
            }
        }
    }
    if d.contains("ascii85decode") {
        if let Some(v) = ascii85(&cur) {
            cur = v;
        }
    }
    if d.contains("asciihexdecode") {
        if let Some(v) = ascii_hex(&cur) {
            cur = v;
        }
    }
    cur
}

fn looks_like_content(data: &[u8]) -> bool {
    let head = &data[..data.len().min(4000)];
    head.windows(2).any(|w| w == b"BT" || w == b"Tj") || head.windows(2).any(|w| w == b"TJ")
}

fn extract_content(data: &[u8]) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut last_was_text = false;
    while i < data.len() {
        if data[i] == b'(' {
            if let Some((s, next)) = read_literal(data, i) {
                if last_was_text && !out.ends_with(' ') && !out.ends_with('\n') && !s.starts_with(' ') {
                    out.push(' ');
                }
                out.push_str(&s);
                last_was_text = true;
                i = next;
                continue;
            }
        }
        if data[i] == b'<' && i + 1 < data.len() && data[i + 1] != b'<' {
            if let Some((s, next)) = read_hex_string(data, i) {
                if last_was_text && !out.ends_with(' ') && !s.is_empty() && !s.starts_with(' ') {
                    out.push(' ');
                }
                out.push_str(&s);
                last_was_text = !s.is_empty();
                i = next;
                continue;
            }
        }
        // operators that imply a line break
        if data[i] == b'\n' || data[i] == b'T' && i + 1 < data.len() && matches!(data[i + 1], b'*' | b'd' | b'D' | b'm') {
            if data[i] == b'T' {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                last_was_text = false;
            }
        }
        if data[i] == b'\'' || data[i] == b'"' {
            if !out.ends_with('\n') {
                out.push('\n');
            }
        }
        i += 1;
    }
    out
}

fn read_literal(data: &[u8], start: usize) -> Option<(String, usize)> {
    let mut i = start + 1;
    let mut raw = Vec::new();
    let mut depth = 1i32;
    while i < data.len() {
        let c = data[i];
        if c == b'\\' && i + 1 < data.len() {
            let n = data[i + 1];
            match n {
                b'n' => raw.push(b'\n'),
                b'r' => raw.push(b'\r'),
                b't' => raw.push(b'\t'),
                b'b' => raw.push(8),
                b'f' => raw.push(12),
                b'(' => raw.push(b'('),
                b')' => raw.push(b')'),
                b'\\' => raw.push(b'\\'),
                b'\n' => {}
                b'\r' => {
                    if i + 2 < data.len() && data[i + 2] == b'\n' {
                        i += 1;
                    }
                }
                d if (b'0'..=b'7').contains(&d) => {
                    let mut val = (d - b'0') as u16;
                    let mut k = 1;
                    while k < 3 && i + 1 + k < data.len() && (b'0'..=b'7').contains(&data[i + 1 + k]) {
                        val = val * 8 + (data[i + 1 + k] - b'0') as u16;
                        k += 1;
                    }
                    raw.push((val & 0xff) as u8);
                    i += 1 + k;
                    continue;
                }
                other => raw.push(other),
            }
            i += 2;
            continue;
        }
        if c == b'(' {
            depth += 1;
            raw.push(c);
            i += 1;
            continue;
        }
        if c == b')' {
            depth -= 1;
            if depth == 0 {
                return Some((decode_pdf_string(&raw), i + 1));
            }
            raw.push(c);
            i += 1;
            continue;
        }
        raw.push(c);
        i += 1;
    }
    None
}

fn read_hex_string(data: &[u8], start: usize) -> Option<(String, usize)> {
    let mut i = start + 1;
    let mut hex = String::new();
    while i < data.len() && data[i] != b'>' {
        if data[i].is_ascii_hexdigit() {
            hex.push(data[i] as char);
        }
        i += 1;
    }
    if i >= data.len() {
        return None;
    }
    if hex.len() % 2 == 1 {
        hex.push('0');
    }
    let mut raw = Vec::new();
    let bytes = hex.as_bytes();
    let mut k = 0;
    while k + 1 < bytes.len() {
        let v = u8::from_str_radix(std::str::from_utf8(&bytes[k..k + 2]).ok()?, 16).ok()?;
        raw.push(v);
        k += 2;
    }
    Some((decode_pdf_string(&raw), i + 1))
}

fn decode_pdf_string(raw: &[u8]) -> String {
    if raw.starts_with(&[0xfe, 0xff]) && raw.len() >= 2 {
        let mut u = Vec::new();
        let mut i = 2;
        while i + 1 < raw.len() {
            u.push(u16::from_be_bytes([raw[i], raw[i + 1]]));
            i += 2;
        }
        return String::from_utf16_lossy(&u);
    }
    if raw.starts_with(&[0xff, 0xfe]) {
        let mut u = Vec::new();
        let mut i = 2;
        while i + 1 < raw.len() {
            u.push(u16::from_le_bytes([raw[i], raw[i + 1]]));
            i += 2;
        }
        return String::from_utf16_lossy(&u);
    }
    // UTF-8 if it validates and contains multibyte
    if raw.iter().any(|b| *b >= 128) {
        if let Ok(s) = std::str::from_utf8(raw) {
            if s.chars().count() < raw.len() {
                return s.to_string();
            }
        }
    }
    raw.iter().map(|&b| cp1252(b)).collect()
}

fn cp1252(b: u8) -> char {
    if b < 128 || b >= 160 {
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

fn strip_page_tail(s: &str) -> String {
    let t = s.trim();
    if let Some(i) = t.rfind(' ') {
        if is_page_label(t[i + 1..].trim()) {
            return t[..i].trim().to_string();
        }
    }
    t.to_string()
}

fn is_page_label(s: &str) -> bool {
    let b = s.as_bytes();
    let Some(slash) = b.iter().position(|c| *c == b'/') else {
        return false;
    };
    slash > 0
        && slash + 1 < b.len()
        && b[..slash].iter().all(|c| c.is_ascii_digit())
        && b[slash + 1..].iter().all(|c| c.is_ascii_digit())
}

fn tidy_pdf_text(s: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in s.split(['\n', '\r']) {
        let t = strip_page_tail(&tidy_text(line));
        if t.is_empty() || is_page_label(&t) {
            if lines.last().map(|l| !l.is_empty()).unwrap_or(false) {
                lines.push(String::new());
            }
            continue;
        }
        // join hyphenation across lines
        if let Some(prev) = lines.last_mut() {
            if prev.ends_with('-') && t.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false) {
                prev.pop();
                prev.push_str(&t);
                continue;
            }
        }
        lines.push(t);
    }
    // collapse single newlines inside paragraphs: a short gap stays a space,
    // blank line stays a paragraph break.
    let mut paras = Vec::new();
    let mut cur = String::new();
    for line in lines {
        if line.is_empty() {
            if !cur.is_empty() {
                paras.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(&line);
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    paras.join("\n\n")
}

fn ascii85(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut vals = [0u8; 5];
    let mut n = 0;
    for &b in data {
        if b.is_ascii_whitespace() {
            continue;
        }
        if b == b'~' {
            break;
        }
        if b == b'z' && n == 0 {
            out.extend_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        if !(33..=117).contains(&b) {
            continue;
        }
        vals[n] = b - 33;
        n += 1;
        if n == 5 {
            let v = ((vals[0] as u32) * 85u32.pow(4))
                + ((vals[1] as u32) * 85u32.pow(3))
                + ((vals[2] as u32) * 85 * 85)
                + ((vals[3] as u32) * 85)
                + vals[4] as u32;
            out.extend_from_slice(&v.to_be_bytes());
            n = 0;
        }
    }
    if n > 1 {
        for k in n..5 {
            vals[k] = 84;
        }
        let v = ((vals[0] as u32) * 85u32.pow(4))
            + ((vals[1] as u32) * 85u32.pow(3))
            + ((vals[2] as u32) * 85 * 85)
            + ((vals[3] as u32) * 85)
            + vals[4] as u32;
        let bytes = v.to_be_bytes();
        out.extend_from_slice(&bytes[..n - 1]);
    }
    Some(out)
}

fn ascii_hex(data: &[u8]) -> Option<Vec<u8>> {
    let mut hex = String::new();
    for &b in data {
        if b.is_ascii_hexdigit() {
            hex.push(b as char);
        }
        if b == b'>' {
            break;
        }
    }
    if hex.len() % 2 == 1 {
        hex.push('0');
    }
    let mut out = Vec::new();
    let b = hex.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        out.push(u8::from_str_radix(std::str::from_utf8(&b[i..i + 2]).ok()?, 16).ok()?);
        i += 2;
    }
    Some(out)
}
