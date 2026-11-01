use crate::convert;
use crate::error::Error;
use crate::formats;
use crate::osnet::{Conn, Listener};
use crate::pathutil::stem;
use crate::zipio::ZipWriter;

const INDEX: &str = include_str!("../assets/index.html");
const CSS: &str = include_str!("../assets/app.css");
const JS: &str = include_str!("../assets/app.js");
const FAVICON: &[u8] = include_bytes!("../assets/favicon.svg");
const FONT_SANS: &[u8] = include_bytes!("../assets/fonts/SchibstedGrotesk.ttf");
const FONT_SANS_I: &[u8] = include_bytes!("../assets/fonts/SchibstedGrotesk-Italic.ttf");
const FONT_SERIF: &[u8] = include_bytes!("../assets/fonts/Newsreader.ttf");
const FONT_SERIF_I: &[u8] = include_bytes!("../assets/fonts/Newsreader-Italic.ttf");

const MAX_BODY: usize = 80 * 1024 * 1024;

pub fn serve(port: u16) -> Result<(), String> {
    let listener = Listener::bind(port)?;
    eprintln!("ConverteTudo em http://0.0.0.0:{port}");
    loop {
        match listener.accept() {
            Ok(mut conn) => {
                if let Err(e) = handle(&mut conn) {
                    let _ = write_response(&mut conn, 500, "application/json; charset=utf-8", &json_err(&e), &[]);
                }
                conn.close();
            }
            Err(e) => eprintln!("conexão: {e}"),
        }
    }
}

fn handle(conn: &mut Conn) -> Result<(), String> {
    let req = read_request(conn)?;
    let path = req.path.split('?').next().unwrap_or("/");
    if req.method == "GET" {
        return match path {
            "/" | "/index.html" => write_response(conn, 200, "text/html; charset=utf-8", INDEX.as_bytes(), &[("Cache-Control", "no-cache")]),
            "/app.css" => write_response(conn, 200, "text/css; charset=utf-8", CSS.as_bytes(), &[("Cache-Control", "no-cache")]),
            "/app.js" => write_response(conn, 200, "text/javascript; charset=utf-8", JS.as_bytes(), &[("Cache-Control", "no-cache")]),
            "/favicon.svg" => write_response(conn, 200, "image/svg+xml", FAVICON, &[]),
            "/assets/SchibstedGrotesk.ttf" => font(conn, FONT_SANS),
            "/assets/SchibstedGrotesk-Italic.ttf" => font(conn, FONT_SANS_I),
            "/assets/Newsreader.ttf" => font(conn, FONT_SERIF),
            "/assets/Newsreader-Italic.ttf" => font(conn, FONT_SERIF_I),
            "/api/formats" => write_response(conn, 200, "application/json; charset=utf-8", formats::formats_json().as_bytes(), &[]),
            "/api/health" => write_response(conn, 200, "application/json", b"{\"ok\":true}", &[]),
            _ => write_response(conn, 404, "text/plain; charset=utf-8", "não encontrado\n".as_bytes(), &[]),
        };
    }
    if req.method != "POST" {
        return write_response(conn, 405, "text/plain; charset=utf-8", "método não aceito\n".as_bytes(), &[]);
    }
    let ctype = req.header("content-type").unwrap_or("");
    let boundary = boundary_of(ctype).ok_or_else(|| "o envio precisa ser multipart/form-data".to_string())?;
    let parts = parse_multipart(&req.body, &boundary)?;
    match path {
        "/api/preview" => preview(conn, &parts),
        "/api/convert" => convert_one(conn, &parts),
        "/api/zip" => zip_ready(conn, &parts),
        "/api/batch" => batch(conn, &parts),
        _ => write_response(conn, 404, "application/json", &json_err("rota inexistente"), &[]),
    }
}

fn font(conn: &mut Conn, bytes: &[u8]) -> Result<(), String> {
    write_response(conn, 200, "font/ttf", bytes, &[("Cache-Control", "public, max-age=86400")])
}

fn require_file<'a>(conn: &mut Conn, parts: &'a [Part]) -> Result<Option<&'a Part>, String> {
    let Some(file) = parts.iter().find(|p| p.name == "file") else {
        write_response(conn, 422, "application/json; charset=utf-8", &json_err("falta o arquivo"), &[])?;
        return Ok(None);
    };
    if file.data.is_empty() {
        write_response(conn, 422, "application/json; charset=utf-8", &json_err("o arquivo está vazio"), &[])?;
        return Ok(None);
    }
    Ok(Some(file))
}

fn preview(conn: &mut Conn, parts: &[Part]) -> Result<(), String> {
    let Some(file) = require_file(conn, parts)? else {
        return Ok(());
    };
    let to = field(parts, "to").unwrap_or("txt");
    match convert::convert(&file.filename, &file.data, to) {
        Ok(out) => {
            let html = crate::write_text::write_html_fragment(&out.doc);
            let text = crate::write_text::write_txt(&out.doc);
            let mut warns = String::from("[");
            for (i, w) in out.warnings.iter().enumerate() {
                if i > 0 { warns.push(','); }
                warns.push_str(&json_string(w));
            }
            warns.push(']');
            let body = format!(
                "{{\"from\":\"{}\",\"to\":\"{}\",\"title\":{},\"html\":{},\"text\":{},\"warnings\":{}}}",
                json_escape(&out.doc.meta.source_format),
                json_escape(to),
                json_string(&out.doc.meta.title),
                json_string(&html),
                json_string(&text),
                warns
            );
            write_response(conn, 200, "application/json; charset=utf-8", body.as_bytes(), &[])
        }
        Err(e) => write_response(conn, 422, "application/json; charset=utf-8", &json_err(&e.msg), &[]),
    }
}

fn convert_one(conn: &mut Conn, parts: &[Part]) -> Result<(), String> {
    let Some(file) = require_file(conn, parts)? else {
        return Ok(());
    };
    let to = field(parts, "to").unwrap_or("txt");
    match convert::convert(&file.filename, &file.data, to) {
        Ok(out) => {
            let disp = content_disposition(&out.filename);
            let warn = urlencoding_min(&out.warnings.join("\n"));
            write_response(
                conn,
                200,
                &out.mime,
                &out.bytes,
                &[
                    ("Content-Disposition", &disp),
                    ("X-Convert-Warn", &warn),
                    ("Cache-Control", "no-store"),
                ],
            )
        }
        Err(e) => write_response(conn, 422, "application/json; charset=utf-8", &json_err(&e.msg), &[]),
    }
}

fn batch(conn: &mut Conn, parts: &[Part]) -> Result<(), String> {
    let to = field(parts, "to").unwrap_or("txt");
    let files: Vec<&Part> = parts.iter().filter(|p| p.name == "file" && !p.data.is_empty()).collect();
    if files.is_empty() {
        return write_response(conn, 422, "application/json; charset=utf-8", &json_err("nenhum arquivo"), &[]);
    }
    let mut z = ZipWriter::new();
    let mut errors = Vec::new();
    for f in &files {
        match convert::convert(&f.filename, &f.data, to) {
            Ok(out) => z.add(&out.filename, &out.bytes, true),
            Err(e) => errors.push(format!("{}: {}", f.filename, e.msg)),
        }
    }
    if errors.len() == files.len() {
        return write_response(conn, 422, "application/json; charset=utf-8", &json_err(&errors.join(" ")), &[]);
    }
    let bytes = z.finish();
    write_response(conn, 200, "application/zip", &bytes, &[("Content-Disposition", "attachment; filename=\"convertetudo.zip\"")])
}

fn zip_ready(conn: &mut Conn, parts: &[Part]) -> Result<(), String> {
    let files: Vec<&Part> = parts.iter().filter(|p| p.name == "file" && !p.data.is_empty()).collect();
    if files.is_empty() {
        return write_response(conn, 422, "application/json; charset=utf-8", &json_err("nada para zipar"), &[]);
    }
    let mut z = ZipWriter::new();
    let mut used = std::collections::BTreeMap::<String, usize>::new();
    for f in files {
        let original = if f.filename.is_empty() { "arquivo".to_string() } else { f.filename.clone() };
        let n = used.get(&original).copied().unwrap_or(0);
        let name = if n == 0 {
            original.clone()
        } else {
            let s = stem(&original);
            let ext = crate::pathutil::extension(&original);
            if ext.is_empty() { format!("{s}-{n}") } else { format!("{s}-{n}.{ext}") }
        };
        used.insert(original, n + 1);
        z.add(&name, &f.data, true);
    }
    let bytes = z.finish();
    write_response(conn, 200, "application/zip", &bytes, &[("Content-Disposition", "attachment; filename=\"convertetudo.zip\"")])
}

fn field<'a>(parts: &'a [Part], name: &str) -> Option<&'a str> {
    parts.iter().find(|p| p.name == name).and_then(|p| std::str::from_utf8(&p.data).ok())
}

struct Request {
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

fn read_request(conn: &mut Conn) -> Result<Request, String> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let header_end;
    loop {
        let n = conn.read(&mut tmp)?;
        if n == 0 {
            return Err("conexão fechou antes do pedido".into());
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(i) = find_header_end(&buf) {
            header_end = i;
            break;
        }
        if buf.len() > 1024 * 256 {
            return Err("cabeçalho grande demais".into());
        }
    }
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let start = lines.next().unwrap_or("");
    let mut sp = start.split_whitespace();
    let method = sp.next().unwrap_or("").to_string();
    let path = sp.next().unwrap_or("/").to_string();
    let mut headers = Vec::new();
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let len = headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("content-length")).and_then(|(_, v)| v.parse::<usize>().ok()).unwrap_or(0);
    if len > MAX_BODY {
        return Err("arquivo maior que 80 MB".into());
    }
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < len {
        let n = conn.read(&mut tmp)?;
        if n == 0 {
            return Err("corpo do pedido incompleto".into());
        }
        body.extend_from_slice(&tmp[..n]);
    }
    body.truncate(len);
    Ok(Request { method, path, headers, body })
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

struct Part {
    name: String,
    filename: String,
    data: Vec<u8>,
}

fn boundary_of(ctype: &str) -> Option<String> {
    for piece in ctype.split(';') {
        let piece = piece.trim();
        if let Some(v) = piece.strip_prefix("boundary=") {
            return Some(v.trim_matches('"').to_string());
        }
    }
    None
}

fn parse_multipart(body: &[u8], boundary: &str) -> Result<Vec<Part>, String> {
    let marker = format!("--{boundary}");
    let marker_b = marker.as_bytes();
    let mut parts = Vec::new();
    let mut i = 0;
    while i + marker_b.len() <= body.len() {
        if &body[i..i + marker_b.len()] != marker_b {
            i += 1;
            continue;
        }
        i += marker_b.len();
        if i + 1 < body.len() && body[i] == b'-' && body[i + 1] == b'-' {
            break;
        }
        if i + 1 < body.len() && body[i] == b'\r' {
            i += 2;
        }
        let rest = &body[i..];
        let head_end = rest.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| "multipart sem cabeçalho".to_string())?;
        let head = String::from_utf8_lossy(&rest[..head_end]).to_ascii_lowercase();
        let head_raw = String::from_utf8_lossy(&rest[..head_end]).into_owned();
        i += head_end + 4;
        let next = body[i..].windows(marker_b.len()).position(|w| w == marker_b).ok_or_else(|| "multipart sem fim".to_string())?;
        let mut end = i + next;
        if end >= 2 && body[end - 2] == b'\r' && body[end - 1] == b'\n' {
            end -= 2;
        }
        let data = body[i..end].to_vec();
        let mut name = String::new();
        let mut filename = String::new();
        if let Some(cd) = head_raw.split("\r\n").find(|l| l.to_ascii_lowercase().starts_with("content-disposition")) {
            name = disp_param(cd, "name");
            filename = disp_param(cd, "filename");
        }
        let _ = head;
        parts.push(Part { name, filename, data });
        i = i + next;
    }
    Ok(parts)
}

fn disp_param(line: &str, key: &str) -> String {
    let star = format!("{key}*=");
    if let Some(i) = line.to_ascii_lowercase().find(&star.to_ascii_lowercase()) {
        let v = &line[i + star.len()..];
        let v = v.split(';').next().unwrap_or(v).trim().trim_matches('"');
        if let Some(rest) = v.split("''").nth(1) {
            return urlencoding_decode(rest);
        }
    }
    let pat = format!("{key}=");
    if let Some(i) = line.to_ascii_lowercase().find(&pat.to_ascii_lowercase()) {
        let v = &line[i + pat.len()..];
        let v = v.trim();
        if let Some(stripped) = v.strip_prefix('"') {
            return stripped.split('"').next().unwrap_or("").to_string();
        }
        return v.split(';').next().unwrap_or("").trim().to_string();
    }
    String::new()
}

fn write_response(conn: &mut Conn, status: u16, ctype: &str, body: &[u8], extra: &[(&str, &str)]) -> Result<(), String> {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        422 => "Unprocessable Entity",
        _ => "Error",
    };
    let mut head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n",
        body.len()
    );
    for (k, v) in extra {
        head.push_str(k);
        head.push_str(": ");
        head.push_str(v);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    conn.write_all(head.as_bytes())?;
    conn.write_all(body)?;
    Ok(())
}

fn json_err(msg: &str) -> Vec<u8> {
    format!("{{\"error\":{}}}", json_string(msg)).into_bytes()
}

fn json_string(s: &str) -> String {
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

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn content_disposition(name: &str) -> String {
    let ascii: String = name.chars().map(|c| if c.is_ascii() && c != '"' && c != '\\' { c } else { '_' }).collect();
    format!("attachment; filename=\"{ascii}\"; filename*=UTF-8''{}", urlencoding_min(name))
}

fn urlencoding_min(s: &str) -> String {
    let mut out = String::new();
    for b in s.as_bytes() {
        if b.is_ascii_alphanumeric() || matches!(*b, b'-' | b'_' | b'.' | b'~') {
            out.push(*b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn urlencoding_decode(s: &str) -> String {
    crate::pathutil::percent_decode(s)
}

pub fn cli_convert(inputs: &[String], to: &str, out_dir: Option<&str>, out_file: Option<&str>) -> Result<(), Error> {
    if inputs.is_empty() {
        return Err(Error::new("informe pelo menos um arquivo"));
    }
    if inputs.len() == 1 {
        let data = std::fs::read(&inputs[0]).map_err(|e| Error::new(format!("não li {}: {e}", inputs[0])))?;
        let out = convert::convert(&inputs[0], &data, to)?;
        let dest = if let Some(f) = out_file {
            f.to_string()
        } else if let Some(dir) = out_dir {
            format!("{}/{}", dir.trim_end_matches('/'), out.filename)
        } else {
            out.filename.clone()
        };
        if let Some(parent) = std::path::Path::new(&dest).parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        std::fs::write(&dest, &out.bytes)?;
        println!("{dest}");
        for w in &out.warnings {
            eprintln!("aviso: {w}");
        }
        return Ok(());
    }
    let dir = out_dir.unwrap_or(".");
    std::fs::create_dir_all(dir)?;
    for input in inputs {
        let data = std::fs::read(input).map_err(|e| Error::new(format!("não li {input}: {e}")))?;
        let out = convert::convert(input, &data, to)?;
        let dest = format!("{}/{}", dir.trim_end_matches('/'), out.filename);
        std::fs::write(&dest, &out.bytes)?;
        println!("{dest}");
        for w in &out.warnings {
            eprintln!("aviso: {w}");
        }
    }
    Ok(())
}
