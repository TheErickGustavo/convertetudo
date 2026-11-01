use crate::error::{Error, Result};
use crate::model::*;

pub fn read_mobi(data: &[u8]) -> Result<Document> {
    if data.len() < 100 {
        return Err(Error::new("arquivo MOBI curto demais."));
    }
    let typ = &data[60..68];
    if typ != b"BOOKMOBI" && typ != b"TEXtREAd" {
        return Err(Error::new(
            "não reconheci um MOBI/AZW. O cabeçalho PalmDB não é BOOKMOBI.",
        ));
    }
    let nrecs = u16be(data, 76) as usize;
    if 78 + nrecs * 8 > data.len() || nrecs == 0 {
        return Err(Error::new("tabela de registros do MOBI está inválida."));
    }
    let mut offsets = Vec::with_capacity(nrecs);
    for i in 0..nrecs {
        offsets.push(u32be(data, 78 + i * 8) as usize);
    }
    offsets.push(data.len());
    let rec0 = &data[offsets[0]..offsets[1].min(data.len())];
    if rec0.len() < 16 {
        return Err(Error::new("registro 0 do MOBI está incompleto."));
    }
    let compression = u16be(rec0, 0);
    let text_length = u32be(rec0, 4) as usize;
    let rec_count = u16be(rec0, 8) as usize;
    let mut text = Vec::new();
    for i in 1..=rec_count {
        if i >= offsets.len() - 1 {
            break;
        }
        let rec = &data[offsets[i]..offsets[i + 1]];
        match compression {
            1 => text.extend_from_slice(rec),
            2 => palmdoc_decompress(rec, &mut text)?,
            17480 => {
                return Err(Error::new(
                    "este MOBI usa compressão Huff/CDIC. Converta antes para EPUB (Calibre) — esse codec não entra nesta versão.",
                ));
            }
            other => {
                return Err(Error::new(format!("compressão MOBI {other} não é suportada.")));
            }
        }
        if text.len() >= text_length && text_length > 0 {
            text.truncate(text_length);
            break;
        }
    }
    if text_length > 0 && text.len() > text_length {
        text.truncate(text_length);
    }
    let raw = String::from_utf8_lossy(&text).into_owned();
    let mut doc = Document::new("mobi");
    if raw.contains('<') && (raw.contains("<html") || raw.contains("<p") || raw.contains("<P") || raw.contains("<body")) {
        let parsed = crate::read_text::read_html(raw.as_bytes(), "mobi")?;
        doc.blocks = parsed.blocks;
        doc.meta = parsed.meta;
        doc.meta.source_format = "mobi".into();
        doc.warnings = parsed.warnings;
    } else {
        for para in raw.split("\n\n") {
            let p = tidy_text(para);
            if !p.is_empty() {
                doc.blocks.push(paragraph(p));
            }
        }
    }
    // title often sits in the MOBI header full name
    if doc.meta.title.is_empty() && rec0.len() > 0x70 {
        let name_off = u32be(rec0, 0x54) as usize;
        let name_len = u32be(rec0, 0x58) as usize;
        if name_off + name_len <= rec0.len() && name_len > 0 && name_len < 400 {
            let name = String::from_utf8_lossy(&rec0[name_off..name_off + name_len])
                .trim_end_matches('\0')
                .trim()
                .to_string();
            if !name.is_empty() {
                doc.meta.title = name;
            }
        }
    }
    doc.blocks = clean_blocks(doc.blocks);
    if doc.blocks.is_empty() {
        return Err(Error::new("o MOBI abriu, mas não achei texto."));
    }
    Ok(doc)
}

fn palmdoc_decompress(data: &[u8], out: &mut Vec<u8>) -> Result<()> {
    let mut i = 0;
    while i < data.len() {
        let b = data[i];
        i += 1;
        if b == 0 {
            out.push(0);
        } else if b <= 8 {
            let n = b as usize;
            if i + n > data.len() {
                return Err(Error::new("PalmDOC truncado"));
            }
            out.extend_from_slice(&data[i..i + n]);
            i += n;
        } else if b < 0x80 {
            out.push(b);
        } else if b < 0xc0 {
            if i >= data.len() {
                break;
            }
            let b2 = data[i];
            i += 1;
            let distance = ((((b as usize) << 8) | b2 as usize) >> 3) & 0x7ff;
            let length = (b2 as usize & 0x07) + 3;
            if distance == 0 || distance > out.len() {
                return Err(Error::new("referência PalmDOC inválida"));
            }
            for _ in 0..length {
                let byte = out[out.len() - distance];
                out.push(byte);
            }
        } else {
            out.push(b' ');
            out.push(b ^ 0x80);
        }
    }
    Ok(())
}

fn u16be(d: &[u8], i: usize) -> u16 {
    if i + 2 > d.len() {
        return 0;
    }
    u16::from_be_bytes([d[i], d[i + 1]])
}
fn u32be(d: &[u8], i: usize) -> u32 {
    if i + 4 > d.len() {
        return 0;
    }
    u32::from_be_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}
