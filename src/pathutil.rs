pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        if b[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn resolve_zip_path(base_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or(href);
    let href = percent_decode(href.trim());
    if href.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = if href.starts_with('/') {
        Vec::new()
    } else {
        base_dir
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    };
    for p in href.trim_start_matches('/').split('/') {
        if p == ".." {
            parts.pop();
        } else if p != "." && !p.is_empty() {
            parts.push(p.to_string());
        }
    }
    parts.join("/")
}

pub fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(i) => path[..i].to_string(),
        None => String::new(),
    }
}

pub fn extension(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match base.rfind('.') {
        Some(i) if i > 0 && i + 1 < base.len() => base[i + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

pub fn stem(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
    match base.rfind('.') {
        Some(i) if i > 0 => base[..i].to_string(),
        _ => base.to_string(),
    }
}

pub fn file_name(path: &str) -> String {
    path.rsplit(['/', '\\']).next().unwrap_or(path).to_string()
}
