use crate::dom::{self, Elem, Node};
use crate::error::{Error, Result};
use crate::model::*;
use crate::pathutil::{parent_dir, resolve_zip_path};
use crate::zipio::unzip;

pub fn read_epub(data: &[u8]) -> Result<Document> {
    let zip = unzip(data)?;
    let container = zip.get_ci("META-INF/container.xml").ok_or_else(|| {
        Error::new("este EPUB não tem META-INF/container.xml. O arquivo pode estar corrompido ou não ser um EPUB.")
    })?;
    let container_xml = std::str::from_utf8(container)
        .map_err(|_| Error::new("container.xml do EPUB não está em UTF-8"))?;
    let container_dom = dom::parse_xml(container_xml)?;
    let mut rootfiles = Vec::new();
    dom::descendants_named(&container_dom, "rootfile", &mut rootfiles);
    let opf_path = rootfiles
        .iter()
        .find_map(|e| e.attr("full-path"))
        .ok_or_else(|| Error::new("o container.xml do EPUB não aponta para um arquivo OPF"))?
        .to_string();
    let opf_bytes = zip.get(&opf_path).or_else(|| zip.get_ci(&opf_path)).ok_or_else(|| {
        Error::new(format!("o OPF “{opf_path}” citado no container não está no EPUB"))
    })?;
    let opf_str = decode_xml(opf_bytes)?;
    let opf = dom::parse_xml(&opf_str)?;
    let mut doc = Document::new("epub");
    read_meta(&opf, &mut doc);

    let mut manifest = Vec::new();
    dom::descendants_named(&opf, "item", &mut manifest);
    let mut items: Vec<ManItem> = Vec::new();
    for it in &manifest {
        let id = it.attr("id").unwrap_or("").to_string();
        let href = it.attr("href").unwrap_or("").to_string();
        let media = it.attr("media-type").unwrap_or("").to_ascii_lowercase();
        let props = it.attr("properties").unwrap_or("").to_ascii_lowercase();
        if id.is_empty() || href.is_empty() {
            continue;
        }
        items.push(ManItem { id, href, media, props });
    }

    let opf_dir = parent_dir(&opf_path);
    let nav_titles = nav_map(&zip, &items, &opf_dir);

    let mut spine = Vec::new();
    dom::descendants_named(&opf, "itemref", &mut spine);
    let mut refs: Vec<(String, bool)> = Vec::new();
    for r in &spine {
        let idref = r.attr("idref").unwrap_or("");
        let linear = r.attr("linear").unwrap_or("yes") != "no";
        if !idref.is_empty() {
            refs.push((idref.to_string(), linear));
        }
    }
    if refs.iter().all(|(_, lin)| !lin) {
        for r in &mut refs {
            r.1 = true;
        }
    }

    let nav_hrefs: Vec<String> = items
        .iter()
        .filter(|it| it.props.split_whitespace().any(|p| p == "nav") || it.media.contains("dtbncx"))
        .map(|it| resolve_zip_path(&opf_dir, &it.href))
        .collect();

    let mut chapters = 0usize;
    for (idref, linear) in &refs {
        if !linear {
            continue;
        }
        let Some(item) = items.iter().find(|it| it.id == *idref) else {
            doc.warn(format!("o spine cita “{idref}”, que não está no manifesto"));
            continue;
        };
        if item.media.contains("dtbncx") || item.props.split_whitespace().any(|p| p == "nav") {
            continue;
        }
        if !(item.media.contains("html") || item.media.contains("xml") || item.href.ends_with(".xhtml") || item.href.ends_with(".html") || item.href.ends_with(".htm")) {
            continue;
        }
        let path = resolve_zip_path(&opf_dir, &item.href);
        if nav_hrefs.iter().any(|n| n == &path) {
            continue;
        }
        let Some(bytes) = zip.get(&path).or_else(|| zip.get_ci(&path)) else {
            doc.warn(format!("capítulo ausente no EPUB: {path}"));
            continue;
        };
        let xml = match decode_xml(bytes) {
            Ok(s) => s,
            Err(e) => {
                doc.warn(e.msg);
                continue;
            }
        };
        let dom = if xml.contains("<html") || xml.contains("<HTML") {
            dom::parse_html(&xml).or_else(|_| dom::parse_xml(&xml))?
        } else {
            dom::parse_xml(&xml)?
        };
        let mut blocks = dom::elem_to_blocks(&dom);
        if blocks.is_empty() {
            continue;
        }
        let key = path.clone();
        let title = nav_titles
            .iter()
            .find(|(h, _)| same_href(h, &key) || same_href(h, &item.href))
            .map(|(_, t)| t.clone());
        let has_heading = blocks.iter().any(|b| matches!(b, Block::Heading { .. }));
        if let Some(title) = title {
            if !has_heading && !title.trim().is_empty() && title.trim().len() < 180 {
                blocks.insert(0, heading(1, title));
            }
        }
        doc.blocks.extend(blocks);
        chapters += 1;
    }

    if chapters == 0 {
        // last resort: every html in the zip, in archive order
        for f in &zip.files {
            let n = f.name.to_ascii_lowercase();
            if n.ends_with(".xhtml") || n.ends_with(".html") || n.ends_with(".htm") {
                if n.contains("nav") || n.contains("toc") {
                    continue;
                }
                if let Ok(xml) = decode_xml(&f.data) {
                    if let Ok(dom) = dom::parse_html(&xml).or_else(|_| dom::parse_xml(&xml)) {
                        let blocks = dom::elem_to_blocks(&dom);
                        if !blocks.is_empty() {
                            doc.blocks.extend(blocks);
                            chapters += 1;
                        }
                    }
                }
            }
        }
    }

    if chapters == 0 || doc.blocks.is_empty() {
        return Err(Error::new(
            "o EPUB abriu, mas não encontrei texto nos capítulos. Pode ser um livro só de imagens.",
        ));
    }
    doc.blocks = clean_blocks(doc.blocks);
    Ok(doc)
}

struct ManItem {
    id: String,
    href: String,
    media: String,
    props: String,
}

fn read_meta(opf: &Elem, doc: &mut Document) {
    let mut titles = Vec::new();
    let mut nodes = Vec::new();
    dom::descendants_named(opf, "title", &mut nodes);
    for n in &nodes {
        let t = dom::first_text(n);
        if !t.is_empty() {
            titles.push(t);
        }
    }
    if let Some(t) = titles.first() {
        doc.meta.title = t.clone();
    }
    let mut creators = Vec::new();
    dom::descendants_named(opf, "creator", &mut creators);
    for n in &creators {
        let t = dom::first_text(n);
        if !t.is_empty() {
            doc.meta.authors.push(t);
        }
    }
    grab(opf, "language", &mut doc.meta.language);
    grab(opf, "identifier", &mut doc.meta.identifier);
    grab(opf, "description", &mut doc.meta.description);
    grab(opf, "date", &mut doc.meta.date);
    grab(opf, "publisher", &mut doc.meta.publisher);
    let mut subjects = Vec::new();
    dom::descendants_named(opf, "subject", &mut subjects);
    for n in &subjects {
        let t = dom::first_text(n);
        if !t.is_empty() {
            doc.meta.subjects.push(t);
        }
    }
}

fn grab(root: &Elem, name: &str, dest: &mut String) {
    if !dest.is_empty() && name != "language" {
        return;
    }
    let mut nodes = Vec::new();
    dom::descendants_named(root, name, &mut nodes);
    if let Some(n) = nodes.first() {
        let t = dom::first_text(n);
        if !t.is_empty() {
            *dest = t;
        }
    }
}

fn nav_map(zip: &crate::zipio::ZipArchive, items: &[ManItem], opf_dir: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Some(nav) = items.iter().find(|it| it.props.split_whitespace().any(|p| p == "nav")) {
        let path = resolve_zip_path(opf_dir, &nav.href);
        if let Some(bytes) = zip.get(&path).or_else(|| zip.get_ci(&path)) {
            if let Ok(xml) = decode_xml(bytes) {
                if let Ok(dom) = dom::parse_html(&xml).or_else(|_| dom::parse_xml(&xml)) {
                    collect_anchors(&dom, &mut out);
                }
            }
        }
    }
    if out.is_empty() {
        if let Some(ncx) = items.iter().find(|it| it.media.contains("dtbncx") || it.href.to_ascii_lowercase().ends_with(".ncx")) {
            let path = resolve_zip_path(opf_dir, &ncx.href);
            if let Some(bytes) = zip.get(&path).or_else(|| zip.get_ci(&path)) {
                if let Ok(xml) = decode_xml(bytes) {
                    if let Ok(dom) = dom::parse_xml(&xml) {
                        collect_ncx(&dom, &mut out);
                    }
                }
            }
        }
    }
    out
}

fn collect_anchors(e: &Elem, out: &mut Vec<(String, String)>) {
    if e.name == "a" {
        if let Some(href) = e.attr("href") {
            let text = dom::first_text(e);
            if !text.is_empty() {
                out.push((href.split('#').next().unwrap_or(href).to_string(), text));
            }
        }
    }
    for n in &e.children {
        if let Node::Elem(c) = n {
            collect_anchors(c, out);
        }
    }
}

fn collect_ncx(e: &Elem, out: &mut Vec<(String, String)>) {
    if e.name == "navpoint" {
        let mut label = String::new();
        let mut src = String::new();
        let mut texts = Vec::new();
        dom::descendants_named(e, "text", &mut texts);
        if let Some(t) = texts.first() {
            label = dom::first_text(t);
        }
        let mut contents = Vec::new();
        dom::descendants_named(e, "content", &mut contents);
        if let Some(c) = contents.first() {
            src = c.attr("src").unwrap_or("").to_string();
        }
        if !label.is_empty() && !src.is_empty() {
            let src = src.split('#').next().unwrap_or(&src).to_string();
            out.push((src, label));
        }
    }
    for n in &e.children {
        if let Node::Elem(c) = n {
            collect_ncx(c, out);
        }
    }
}

fn same_href(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.split('#').next().unwrap_or(s).trim_start_matches("./").replace('\\', "/").to_ascii_lowercase()
    };
    let a = norm(a);
    let b = norm(b);
    a == b || a.ends_with(&format!("/{b}")) || b.ends_with(&format!("/{a}"))
}

pub fn decode_xml(bytes: &[u8]) -> Result<String> {
    crate::decode::decode_text(bytes)
}
