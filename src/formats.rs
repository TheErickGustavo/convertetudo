#[derive(Clone, Copy, Debug)]
pub struct Format {
    pub id: &'static str,
    pub label: &'static str,
    pub family: &'static str,
    pub read: bool,
    pub write: bool,
    pub note: &'static str,
    pub mime: &'static str,
}

pub const FORMATS: &[Format] = &[
    Format { id: "txt", label: "TXT", family: "Texto", read: true, write: true, note: "Texto puro, com capítulos e listas.", mime: "text/plain; charset=utf-8" },
    Format { id: "md", label: "Markdown", family: "Texto", read: true, write: true, note: "Títulos, listas, ênfase e tabelas.", mime: "text/markdown; charset=utf-8" },
    Format { id: "html", label: "HTML", family: "Texto", read: true, write: true, note: "Documento HTML autônomo, pronto para ler.", mime: "text/html; charset=utf-8" },
    Format { id: "rtf", label: "RTF", family: "Escritório", read: true, write: true, note: "Abre no Word, LibreOffice e Pages.", mime: "application/rtf" },
    Format { id: "docx", label: "DOCX", family: "Escritório", read: true, write: true, note: "Word moderno. Títulos, negrito, itálico e tabelas.", mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" },
    Format { id: "odt", label: "ODT", family: "Escritório", read: true, write: true, note: "OpenDocument, o formato do LibreOffice.", mime: "application/vnd.oasis.opendocument.text" },
    Format { id: "pdf", label: "PDF", family: "Escritório", read: true, write: true, note: "Leitura extrai a camada de texto. Escrita pagina em A4. Sem OCR.", mime: "application/pdf" },
    Format { id: "epub", label: "EPUB", family: "Ebook", read: true, write: true, note: "Capítulos na ordem do sumário, acentos preservados.", mime: "application/epub+zip" },
    Format { id: "fb2", label: "FB2", family: "Ebook", read: true, write: true, note: "FictionBook, comum em bibliotecas russas e europeias.", mime: "application/xml" },
    Format { id: "mobi", label: "MOBI", family: "Ebook", read: true, write: false, note: "Leitura de PalmDOC e HTML interno. Escrita não entra nesta versão.", mime: "application/x-mobipocket-ebook" },
    Format { id: "csv", label: "CSV", family: "Dados", read: true, write: true, note: "Tabelas. Um documento vira uma coluna se não houver tabela.", mime: "text/csv; charset=utf-8" },
    Format { id: "xlsx", label: "XLSX", family: "Dados", read: true, write: true, note: "Planilha. Cada aba vira uma seção.", mime: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" },
    Format { id: "ods", label: "ODS", family: "Dados", read: true, write: false, note: "Planilha OpenDocument, lida como tabelas.", mime: "application/vnd.oasis.opendocument.spreadsheet" },
    Format { id: "tex", label: "LaTeX", family: "Texto", read: true, write: true, note: "Texto visível. Macros complexas não são reproduzidas.", mime: "application/x-tex" },
    Format { id: "json", label: "JSON", family: "Dados", read: false, write: true, note: "O texto extraído, com metadados, para outro programa.", mime: "application/json" },
    Format { id: "abw", label: "ABW", family: "Escritório", read: true, write: false, note: "AbiWord, só leitura.", mime: "application/x-abiword" },
    Format { id: "xps", label: "XPS", family: "Escritório", read: true, write: false, note: "Texto dos glifos. Sem layout de página.", mime: "application/vnd.ms-xpsdocument" },
];

pub fn by_id(id: &str) -> Option<&'static Format> {
    let id = id.trim().trim_start_matches('.').to_ascii_lowercase();
    let id = alias(&id);
    FORMATS.iter().find(|f| f.id == id)
}

pub fn alias(id: &str) -> &str {
    match id {
        "text" | "textfile" => "txt",
        "markdown" | "mdown" => "md",
        "htm" | "xhtml" => "html",
        "docm" | "dotx" | "dotm" => "docx",
        "xlsm" => "xlsx",
        "tsv" => "csv",
        "azw" | "azw3" | "prc" => "mobi",
        "latex" => "tex",
        "oxps" => "xps",
        "sxw" => "odt",
        "dbk" | "xml" => "html",
        other => other,
    }
}

pub fn can_read(id: &str) -> bool {
    by_id(id).map(|f| f.read).unwrap_or(false)
}

pub fn can_write(id: &str) -> bool {
    by_id(id).map(|f| f.write).unwrap_or(false)
}

pub fn mime_of(id: &str) -> &'static str {
    by_id(id).map(|f| f.mime).unwrap_or("application/octet-stream")
}

pub fn extension_of(id: &str) -> String {
    let id = alias(id);
    if id == "tex" {
        "tex".into()
    } else {
        id.to_string()
    }
}

pub fn formats_json() -> String {
    let mut s = String::from("{\"formats\":[");
    for (i, f) in FORMATS.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!(
            "{{\"id\":\"{}\",\"label\":\"{}\",\"family\":\"{}\",\"read\":{},\"write\":{},\"note\":\"{}\"}}",
            f.id,
            f.label,
            f.family,
            f.read,
            f.write,
            json_escape(f.note)
        ));
    }
    s.push_str("]}");
    s
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
