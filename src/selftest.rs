use crate::convert;
use crate::error::{Error, Result};
use crate::model::*;
use crate::zipio::ZipWriter;

pub fn run() -> Result<()> {
    inflate_roundtrip()?;
    epub_to_txt()?;
    html_roundtrip()?;
    markdown_accents()?;
    docx_roundtrip()?;
    csv_roundtrip()?;
    rtf_roundtrip()?;
    pdf_roundtrip()?;
    fb2_roundtrip()?;
    println!("selftest ok");
    Ok(())
}

fn inflate_roundtrip() -> Result<()> {
    let plain = "Olá, capítulo — ação.".as_bytes();
    let stored = deflate_store(plain);
    let back = crate::inflate::inflate_raw(&stored)?;
    if back != plain {
        return Err(Error::new("inflate (bloco armazenado) não devolveu o texto original"));
    }
    // raw deflate of the same string, produced by zlib 1.3 (window -15, level 9)
    let huffman: &[u8] = &[
        243, 207, 57, 188, 80, 71, 33, 57, 177, 224, 240, 218, 146, 210, 156, 124, 133, 71, 13, 83,
        20, 18, 15, 47, 63, 188, 56, 95, 15, 0,
    ];
    let back = crate::inflate::inflate_raw(huffman)?;
    if back != plain {
        return Err(Error::new(format!(
            "inflate (huffman) devolveu {:?} em vez do texto",
            String::from_utf8_lossy(&back)
        )));
    }
    let zlib: &[u8] = &[
        120, 156, 243, 207, 57, 188, 80, 71, 33, 57, 177, 224, 240, 218, 146, 210, 156, 124, 133, 71,
        13, 83, 20, 18, 15, 47, 63, 188, 56, 95, 15, 0, 174, 2, 12, 216,
    ];
    let back = crate::inflate::inflate_zlib(zlib)?;
    if back != plain {
        return Err(Error::new("inflate zlib não devolveu o texto original"));
    }
    Ok(())
}

fn deflate_store(data: &[u8]) -> Vec<u8> {
    // A valid deflate stream made only of stored blocks.
    let mut out = Vec::new();
    let mut rest = data;
    if rest.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xff, 0xff]);
        return out;
    }
    while !rest.is_empty() {
        let n = rest.len().min(65535);
        let last = n == rest.len();
        out.push(if last { 1 } else { 0 });
        out.extend_from_slice(&(n as u16).to_le_bytes());
        out.extend_from_slice(&(!n as u16).to_le_bytes());
        out.extend_from_slice(&rest[..n]);
        rest = &rest[n..];
    }
    out
}

fn epub_to_txt() -> Result<()> {
    let ch1 = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Capítulo 1</title></head>
<body>
<h1>O começo</h1>
<p>Memórias de uma <em>tarde</em> em Nova Iguaçu. Ação, coração, não — e um &amp; comercial.</p>
<ul><li>Primeiro item</li><li>Segundo item com ã</li></ul>
<p epub:type="pagebreak" xmlns:epub="http://www.idpf.org/2007/ops">12</p>
</body></html>"#;
    let ch2 = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
<h1>O fim</h1>
<p>Último parágrafo, com ç e õ.</p>
</body></html>"#;
    let opf = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="uid" version="3.0">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
<dc:title>Memórias Póstumas</dc:title>
<dc:creator>Machado de Assis</dc:creator>
<dc:language>pt</dc:language>
<dc:identifier id="uid">urn:test</dc:identifier>
</metadata>
<manifest>
<item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
<item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
</manifest>
<spine><itemref idref="c1"/><itemref idref="c2"/></spine>
</package>"#;
    let nav = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body><nav><ol>
<li><a href="c1.xhtml">O começo</a></li>
<li><a href="c2.xhtml">O fim</a></li>
</ol></nav></body></html>"#;
    let container = br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;
    let mut z = ZipWriter::new();
    z.add("mimetype", b"application/epub+zip", true);
    z.add("META-INF/container.xml", container, true);
    z.add("OEBPS/content.opf", opf.as_bytes(), true);
    z.add("OEBPS/nav.xhtml", nav.as_bytes(), true);
    z.add("OEBPS/c1.xhtml", ch1.as_bytes(), true);
    z.add("OEBPS/c2.xhtml", ch2.as_bytes(), true);
    let bytes = z.finish();
    let out = convert::convert("memorias.epub", &bytes, "txt")?;
    let text = String::from_utf8_lossy(&out.bytes);
    for needle in ["Memórias Póstumas", "Machado de Assis", "O começo", "Nova Iguaçu", "tarde", "& comercial", "Primeiro item", "Segundo item com ã", "O fim", "ç e õ"] {
        if !text.contains(needle) {
            return Err(Error::new(format!("EPUB→TXT perdeu “{needle}”\n---\n{text}")));
        }
    }
    if text.contains('<') || text.contains("12") && text.contains("pagebreak") {
        return Err(Error::new("EPUB→TXT vazou marcação"));
    }
    if text.contains("pagebreak") {
        return Err(Error::new("EPUB→TXT incluiu pagebreak"));
    }
    // page number 12 should not appear as its own dumped token from the skipped element
    if text.lines().any(|l| l.trim() == "12") {
        return Err(Error::new("número de página vazou para o TXT"));
    }
    let pos1 = text.find("O começo").unwrap();
    let pos2 = text.find("O fim").unwrap();
    if pos1 > pos2 {
        return Err(Error::new("capítulos fora de ordem"));
    }
    Ok(())
}

fn html_roundtrip() -> Result<()> {
    let html = "<html><head><title>Contrato</title><script>alert(1)</script></head><body><h1>Cláusula</h1><p>O <strong>aluguel</strong> vence no dia 5.</p><style>p{color:red}</style></body></html>";
    let out = convert::convert("c.html", html.as_bytes(), "txt")?;
    let t = String::from_utf8_lossy(&out.bytes);
    if !t.contains("O aluguel vence") {
        return Err(Error::new(format!("HTML colou o texto em volta da ênfase: {t}")));
    }
    if !t.contains("Cláusula") || !t.contains("aluguel") || !t.contains("Contrato") {
        return Err(Error::new(format!("HTML→TXT incompleto: {t}")));
    }
    if t.contains("alert") || t.contains("color:red") {
        return Err(Error::new(format!("HTML vazou script ou estilo: {t}")));
    }
    let dup = "<html><head><title>Recibo</title></head><body><h1>Recibo</h1><p>Pago.</p></body></html>";
    let dup_out = convert::convert("r.html", dup.as_bytes(), "txt")?;
    let dup_txt = String::from_utf8_lossy(&dup_out.bytes);
    if dup_txt.matches("Recibo").count() != 1 {
        return Err(Error::new(format!("título repetido: {dup_txt}")));
    }
    Ok(())
}

fn markdown_accents() -> Result<()> {
    let md = "# Título\n\nUm parágrafo com ação e coração.\n\n- item ã\n- item ç\n";
    let out = convert::convert("nota.md", md.as_bytes(), "txt")?;
    let t = String::from_utf8_lossy(&out.bytes);
    if !t.contains("Título") || !t.contains("ação") || !t.contains("item ã") {
        return Err(Error::new(format!("MD→TXT perdeu acentos: {t}")));
    }
    Ok(())
}

fn docx_roundtrip() -> Result<()> {
    let doc = sample_doc();
    let bytes = crate::write_office::write_docx(&doc);
    let back = crate::read_office::read_docx(&bytes)?;
    let txt = crate::write_text::write_txt(&back);
    if !txt.contains("Capítulo Um") || !txt.contains("coração") {
        return Err(Error::new(format!("DOCX roundtrip falhou: {txt}")));
    }
    Ok(())
}

fn csv_roundtrip() -> Result<()> {
    let csv = "nome,cidade\nAna,Nova Iguaçu\n";
    let out = convert::convert("t.csv", csv.as_bytes(), "txt")?;
    let t = String::from_utf8_lossy(&out.bytes);
    if !t.contains("Ana") || !t.contains("Nova Iguaçu") {
        return Err(Error::new(format!("CSV falhou: {t}")));
    }
    Ok(())
}

fn rtf_roundtrip() -> Result<()> {
    let doc = sample_doc();
    let bytes = crate::write_text::write_rtf(&doc);
    let back = crate::read_text::read_rtf(&bytes)?;
    let txt = crate::write_text::write_txt(&back);
    if !txt.contains("coração") {
        return Err(Error::new(format!("RTF roundtrip falhou: {txt}")));
    }
    Ok(())
}

fn pdf_roundtrip() -> Result<()> {
    let doc = sample_doc();
    let bytes = crate::write_office::write_pdf(&doc);
    if !bytes.starts_with(b"%PDF") {
        return Err(Error::new("PDF não começa com %PDF"));
    }
    let back = crate::read_pdf::read_pdf(&bytes)?;
    let txt = crate::write_text::write_txt(&back);
    if !txt.contains("coração") && !txt.contains("coracao") {
        return Err(Error::new(format!("PDF roundtrip perdeu o texto: {txt}")));
    }
    if !txt.to_lowercase().contains("cap") {
        return Err(Error::new(format!("PDF roundtrip sem capítulo: {txt}")));
    }
    if txt.contains("1/1") {
        return Err(Error::new(format!("número de página vazou para o texto: {txt}")));
    }
    Ok(())
}

fn fb2_roundtrip() -> Result<()> {
    let xml = r#"<?xml version="1.0"?><FictionBook><description><title-info><book-title>Noite</book-title><author><first-name>Clara</first-name><last-name>Nunes</last-name></author><lang>pt</lang></title-info></description><body><section><title><p>Abertura</p></title><p>Texto com ã.</p></section></body></FictionBook>"#;
    let out = convert::convert("l.fb2", xml.as_bytes(), "txt")?;
    let t = String::from_utf8_lossy(&out.bytes);
    if !t.contains("Noite") || !t.contains("Abertura") || !t.contains("Texto com ã") {
        return Err(Error::new(format!("FB2 falhou: {t}")));
    }
    Ok(())
}

fn sample_doc() -> Document {
    let mut d = Document::new("test");
    d.meta.title = "Prova".into();
    d.meta.authors.push("Autor".into());
    d.blocks.push(heading(1, "Capítulo Um"));
    d.blocks.push(paragraph("Um coração em Nova Iguaçu."));
    d
}
