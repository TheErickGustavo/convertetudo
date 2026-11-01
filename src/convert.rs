use crate::error::{Error, Result};
use crate::formats::{self, alias};
use crate::model::Document;
use crate::pathutil::extension;

pub struct Output {
    pub bytes: Vec<u8>,
    pub filename: String,
    pub mime: String,
    pub warnings: Vec<String>,
    pub doc: Document,
}

pub fn convert(name: &str, data: &[u8], to: &str) -> Result<Output> {
    let to = alias(to.trim().trim_start_matches('.'));
    if !formats::can_write(to) {
        return Err(Error::new(format!(
            "não sei escrever “.{to}”. Escolha TXT, HTML, Markdown, RTF, DOCX, ODT, PDF, EPUB, FB2, CSV, XLSX, LaTeX ou JSON."
        )));
    }
    let from = detect(name, data);
    if !formats::can_read(&from) && from != "html" {
        return Err(Error::new(format!(
            "não sei ler “.{from}”. Documentos nesta versão: EPUB, HTML, DOCX, ODT, RTF, PDF, TXT, Markdown, FB2, MOBI, CSV, XLSX, ODS, XPS, ABW e LaTeX."
        )));
    }
    let doc = read_any(&from, name, data)?;
    let bytes = write_any(to, &doc)?;
    let stem = crate::pathutil::stem(name);
    let stem = if stem.is_empty() { "documento".to_string() } else { stem };
    let filename = format!("{stem}.{}", formats::extension_of(to));
    Ok(Output {
        bytes,
        filename,
        mime: formats::mime_of(to).to_string(),
        warnings: doc.warnings.clone(),
        doc,
    })
}

pub fn detect(name: &str, data: &[u8]) -> String {
    let ext = extension(name);
    if data.starts_with(b"%PDF") {
        return "pdf".into();
    }
    if data.starts_with(b"{\\rtf") {
        return "rtf".into();
    }
    if data.len() > 68 && (&data[60..68] == b"BOOKMOBI" || &data[60..68] == b"TEXtREAd") {
        return "mobi".into();
    }
    if data.starts_with(b"PK") {
        if let Ok(zip) = crate::zipio::unzip(data) {
            if zip.get_ci("mimetype").map(|m| m.starts_with(b"application/epub+zip")).unwrap_or(false)
                || zip.get_ci("META-INF/container.xml").is_some()
            {
                return "epub".into();
            }
            if zip.get_ci("word/document.xml").is_some() {
                return "docx".into();
            }
            if zip.get_ci("xl/workbook.xml").is_some() {
                return "xlsx".into();
            }
            if zip.get_ci("content.xml").is_some() {
                if let Some(mt) = zip.get_ci("mimetype") {
                    let s = String::from_utf8_lossy(mt);
                    if s.contains("spreadsheet") {
                        return "ods".into();
                    }
                    if s.contains("text") {
                        return "odt".into();
                    }
                }
                return if ext == "ods" { "ods".into() } else { "odt".into() };
            }
            let names = zip.names();
            if names.iter().any(|n| n.to_ascii_lowercase().ends_with(".fpage")) {
                return "xps".into();
            }
        }
        return if ext.is_empty() { "zip".into() } else { alias(&ext).to_string() };
    }
    let head = String::from_utf8_lossy(&data[..data.len().min(800)]).to_ascii_lowercase();
    if head.contains("<fictionbook") {
        return "fb2".into();
    }
    if head.contains("<abiword") {
        return "abw".into();
    }
    if head.contains("<!doctype html") || head.contains("<html") || head.contains("<body") {
        return "html".into();
    }
    if head.contains("<book") && head.contains("docbook") || head.contains("<article") && head.contains("docbook") {
        return "html".into();
    }
    if ext == "md" || ext == "markdown" {
        return "md".into();
    }
    if ext == "tex" || ext == "latex" || head.contains("\\documentclass") || head.contains("\\begin{document}") {
        return "tex".into();
    }
    if ext == "csv" || ext == "tsv" {
        return "csv".into();
    }
    if ext == "rtf" {
        return "rtf".into();
    }
    if !ext.is_empty() {
        return alias(&ext).to_string();
    }
    "txt".into()
}

fn read_any(from: &str, name: &str, data: &[u8]) -> Result<Document> {
    match alias(from) {
        "epub" => crate::read_epub::read_epub(data),
        "html" => {
            let head = String::from_utf8_lossy(&data[..data.len().min(500)]).to_ascii_lowercase();
            if head.contains("docbook") || head.contains("<book") && head.contains("<chapter") {
                crate::read_text::read_docbook(data)
            } else {
                crate::read_text::read_html(data, "html")
            }
        }
        "docx" => crate::read_office::read_docx(data),
        "odt" => crate::read_office::read_odt(data),
        "xlsx" => crate::read_office::read_xlsx(data),
        "ods" => crate::read_office::read_ods(data),
        "xps" => crate::read_office::read_xps(data),
        "pdf" => crate::read_pdf::read_pdf(data),
        "rtf" => crate::read_text::read_rtf(data),
        "md" => crate::read_text::read_markdown(data),
        "txt" => crate::read_text::read_plain(data),
        "fb2" => crate::read_text::read_fb2(data),
        "mobi" => crate::read_mobi::read_mobi(data),
        "csv" => {
            let delim = if extension(name) == "tsv" { b'\t' } else { b',' };
            crate::read_text::read_csv(data, delim)
        }
        "abw" => crate::read_text::read_abw(data),
        "tex" => crate::read_text::read_latex(data),
        other => Err(Error::new(format!(
            "leitura de .{other} não está disponível. Converta para DOCX, EPUB ou TXT primeiro."
        ))),
    }
}

fn write_any(to: &str, doc: &Document) -> Result<Vec<u8>> {
    Ok(match alias(to) {
        "txt" => crate::write_text::write_txt(doc).into_bytes(),
        "md" => crate::write_text::write_md(doc).into_bytes(),
        "html" => crate::write_text::write_html_document(doc).into_bytes(),
        "rtf" => crate::write_text::write_rtf(doc),
        "docx" => crate::write_office::write_docx(doc),
        "odt" => crate::write_office::write_odt(doc),
        "pdf" => crate::write_office::write_pdf(doc),
        "epub" => crate::write_office::write_epub(doc),
        "fb2" => crate::write_office::write_fb2(doc),
        "csv" => crate::write_text::write_csv(doc).into_bytes(),
        "xlsx" => crate::write_office::write_xlsx(doc),
        "json" => crate::write_text::write_json(doc).into_bytes(),
        "tex" => crate::write_text::write_latex(doc).into_bytes(),
        other => return Err(Error::new(format!("escrita de .{other} não está disponível"))),
    })
}
