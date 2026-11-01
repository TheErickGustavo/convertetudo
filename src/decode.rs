use crate::error::{Error, Result};

pub fn decode_text(bytes: &[u8]) -> Result<String> {
    if bytes.starts_with(&[0xef, 0xbb, 0xbf]) {
        return Ok(String::from_utf8_lossy(&bytes[3..]).into_owned());
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return Ok(utf16(bytes, false));
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return Ok(utf16(bytes, true));
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok(s.to_string());
    }
    // Declared encoding in the first 512 bytes?
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(400)]);
    let lower = head.to_ascii_lowercase();
    if lower.contains("iso-8859-1") || lower.contains("latin-1") || lower.contains("windows-1252") || lower.contains("cp1252") {
        return Ok(decode_1252(bytes));
    }
    // If the invalid sequences are rare, lossy UTF-8 is kinder than 1252.
    let lossy = String::from_utf8_lossy(bytes);
    if lossy.contains('\u{fffd}') {
        let n = lossy.chars().filter(|c| *c == '\u{fffd}').count();
        if n > 8 {
            return Ok(decode_1252(bytes));
        }
    }
    Ok(lossy.into_owned())
}

fn utf16(bytes: &[u8], be: bool) -> String {
    let rest = if bytes.len() >= 2 { &bytes[2..] } else { bytes };
    let mut units = Vec::new();
    let mut i = 0;
    while i + 1 < rest.len() {
        let u = if be {
            u16::from_be_bytes([rest[i], rest[i + 1]])
        } else {
            u16::from_le_bytes([rest[i], rest[i + 1]])
        };
        units.push(u);
        i += 2;
    }
    String::from_utf16_lossy(&units)
}

fn decode_1252(bytes: &[u8]) -> String {
    const EXTRA: [char; 32] = [
        '\u{20ac}', '\u{0081}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}',
        '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008d}', '\u{017d}', '\u{008f}',
        '\u{0090}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}',
        '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}', '\u{0153}', '\u{009d}', '\u{017e}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|&b| {
            if b < 128 {
                b as char
            } else if (128..160).contains(&b) {
                EXTRA[(b - 128) as usize]
            } else {
                b as char
            }
        })
        .collect()
}

pub fn ensure_nonempty(label: &str, s: &str) -> Result<()> {
    if s.trim().is_empty() {
        Err(Error::new(format!("{label} não tem texto para converter")))
    } else {
        Ok(())
    }
}
