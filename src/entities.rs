pub fn decode_entities(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'&' {
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        if let Some((ch, next)) = parse_entity(s, i) {
            out.push(ch);
            i = next;
        } else {
            out.push('&');
            i += 1;
        }
    }
    out
}

fn parse_entity(s: &str, start: usize) -> Option<(char, usize)> {
    let rest = &s[start + 1..];
    let end = rest.find(';')?;
    if end == 0 || end > 32 {
        return None;
    }
    let body = &rest[..end];
    let next = start + 1 + end + 1;
    if let Some(hex) = body.strip_prefix('#') {
        let (num, ok) = if let Some(h) = hex.strip_prefix('x').or_else(|| hex.strip_prefix('X')) {
            (u32::from_str_radix(h, 16).ok()?, true)
        } else {
            (hex.parse::<u32>().ok()?, true)
        };
        if !ok {
            return None;
        }
        return Some((char_from_code(num)?, next));
    }
    named(body).map(|c| (c, next))
}

fn char_from_code(n: u32) -> Option<char> {
    if n == 0 || (0xD800..=0xDFFF).contains(&n) {
        return None;
    }
    char::from_u32(n)
}

fn named(name: &str) -> Option<char> {
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00a0}',
        "shy" => '\u{00ad}',
        "ensp" | "emsp" | "thinsp" => ' ',
        "ndash" => '\u{2013}',
        "mdash" => '\u{2014}',
        "lsquo" => '\u{2018}',
        "rsquo" => '\u{2019}',
        "ldquo" => '\u{201c}',
        "rdquo" => '\u{201d}',
        "hellip" => '\u{2026}',
        "bull" => '\u{2022}',
        "middot" => '\u{00b7}',
        "copy" => '\u{00a9}',
        "reg" => '\u{00ae}',
        "trade" => '\u{2122}',
        "deg" => '\u{00b0}',
        "plusmn" => '\u{00b1}',
        "times" => '\u{00d7}',
        "divide" => '\u{00f7}',
        "frac12" => '\u{00bd}',
        "frac14" => '\u{00bc}',
        "frac34" => '\u{00be}',
        "euro" => '\u{20ac}',
        "pound" => '\u{00a3}',
        "yen" => '\u{00a5}',
        "cent" => '\u{00a2}',
        "sect" => '\u{00a7}',
        "para" => '\u{00b6}',
        "laquo" => '\u{00ab}',
        "raquo" => '\u{00bb}',
        "iexcl" => '\u{00a1}',
        "iquest" => '\u{00bf}',
        "ordf" => '\u{00aa}',
        "ordm" => '\u{00ba}',
        "not" => '\u{00ac}',
        "macr" => '\u{00af}',
        "acute" => '\u{00b4}',
        "cedil" => '\u{00b8}',
        "uml" => '\u{00a8}',
        "brvbar" => '\u{00a6}',
        "micro" => '\u{00b5}',
        "aacute" => 'á',
        "Aacute" => 'Á',
        "acirc" => 'â',
        "Acirc" => 'Â',
        "agrave" => 'à',
        "Agrave" => 'À',
        "atilde" => 'ã',
        "Atilde" => 'Ã',
        "auml" => 'ä',
        "Auml" => 'Ä',
        "aring" => 'å',
        "Aring" => 'Å',
        "aelig" => 'æ',
        "AElig" => 'Æ',
        "ccedil" => 'ç',
        "Ccedil" => 'Ç',
        "eacute" => 'é',
        "Eacute" => 'É',
        "ecirc" => 'ê',
        "Ecirc" => 'Ê',
        "egrave" => 'è',
        "Egrave" => 'È',
        "euml" => 'ë',
        "Euml" => 'Ë',
        "iacute" => 'í',
        "Iacute" => 'Í',
        "icirc" => 'î',
        "Icirc" => 'Î',
        "igrave" => 'ì',
        "Igrave" => 'Ì',
        "iuml" => 'ï',
        "Iuml" => 'Ï',
        "ntilde" => 'ñ',
        "Ntilde" => 'Ñ',
        "oacute" => 'ó',
        "Oacute" => 'Ó',
        "ocirc" => 'ô',
        "Ocirc" => 'Ô',
        "ograve" => 'ò',
        "Ograve" => 'Ò',
        "otilde" => 'õ',
        "Otilde" => 'Õ',
        "ouml" => 'ö',
        "Ouml" => 'Ö',
        "uacute" => 'ú',
        "Uacute" => 'Ú',
        "ucirc" => 'û',
        "Ucirc" => 'Û',
        "ugrave" => 'ù',
        "Ugrave" => 'Ù',
        "uuml" => 'ü',
        "Uuml" => 'Ü',
        "yacute" => 'ý',
        "Yacute" => 'Ý',
        "yuml" => 'ÿ',
        "szlig" => 'ß',
        "oslash" => 'ø',
        "Oslash" => 'Ø',
        "thorn" => 'þ',
        "ETH" => 'Ð',
        "eth" => 'ð',
        "zcaron" => 'ž',
        "Zcaron" => 'Ž',
        "scaron" => 'š',
        "Scaron" => 'Š',
        _ => return None,
    })
}

pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\u{00a0}' => out.push_str("&#160;"),
            c if (c as u32) < 32 && c != '\n' && c != '\t' && c != '\r' => {}
            c => out.push(c),
        }
    }
    out
}

pub fn html_escape(s: &str) -> String {
    xml_escape(s)
}
