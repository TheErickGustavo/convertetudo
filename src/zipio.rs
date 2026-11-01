use crate::crc32::crc32;
use crate::error::{Error, Result};
use crate::inflate::inflate_raw;

#[derive(Clone, Debug)]
pub struct ZipFile {
    pub name: String,
    pub data: Vec<u8>,
}

pub struct ZipArchive {
    pub files: Vec<ZipFile>,
}

impl ZipArchive {
    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|f| f.name == name || f.name.replace('\\', "/") == name)
            .map(|f| f.data.as_slice())
    }

    pub fn get_ci(&self, name: &str) -> Option<&[u8]> {
        let want = name.to_ascii_lowercase();
        self.files
            .iter()
            .find(|f| f.name.replace('\\', "/").to_ascii_lowercase() == want)
            .map(|f| f.data.as_slice())
    }

    pub fn names(&self) -> Vec<&str> {
        self.files.iter().map(|f| f.name.as_str()).collect()
    }
}

fn u16(d: &[u8], i: usize) -> Result<u16> {
    if i + 2 > d.len() {
        return Err(Error::new("ZIP truncado"));
    }
    Ok(u16::from_le_bytes([d[i], d[i + 1]]))
}
fn u32(d: &[u8], i: usize) -> Result<u32> {
    if i + 4 > d.len() {
        return Err(Error::new("ZIP truncado"));
    }
    Ok(u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]]))
}
fn u64(d: &[u8], i: usize) -> Result<u64> {
    if i + 8 > d.len() {
        return Err(Error::new("ZIP truncado"));
    }
    Ok(u64::from_le_bytes(d[i..i + 8].try_into().unwrap()))
}

pub fn unzip(data: &[u8]) -> Result<ZipArchive> {
    if data.len() < 22 || &data[..2] != b"PK" {
        return Err(Error::new("não é um arquivo ZIP válido"));
    }
    let eocd = find_eocd(data)?;
    let disk_entries = u16(data, eocd + 8)?;
    let total_entries = u16(data, eocd + 10)? as usize;
    let cd_size = u32(data, eocd + 12)? as usize;
    let cd_off = u32(data, eocd + 16)? as usize;
    if disk_entries == 0xffff || total_entries == 0xffff || cd_off == 0xffff_ffff {
        return unzip_zip64(data, eocd);
    }
    let _ = cd_size;
    read_cd(data, cd_off, total_entries)
}

fn find_eocd(data: &[u8]) -> Result<usize> {
    let start = data.len().saturating_sub(22 + 65535);
    let mut i = data.len() - 22;
    loop {
        if data[i] == 0x50 && data[i + 1] == 0x4b && data[i + 2] == 0x05 && data[i + 3] == 0x06 {
            let comment = u16(data, i + 20)? as usize;
            if i + 22 + comment == data.len() || i + 22 + comment <= data.len() {
                return Ok(i);
            }
        }
        if i == start {
            break;
        }
        i -= 1;
    }
    Err(Error::new(
        "o ZIP não tem diretório central — o arquivo pode estar incompleto",
    ))
}

fn read_cd(data: &[u8], mut off: usize, entries: usize) -> Result<ZipArchive> {
    let mut files = Vec::with_capacity(entries);
    for _ in 0..entries {
        if off + 46 > data.len() {
            return Err(Error::new("diretório central do ZIP truncado"));
        }
        let sig = u32(data, off)?;
        if sig != 0x02014b50 {
            return Err(Error::new("entrada do diretório central inválida"));
        }
        let flags = u16(data, off + 8)?;
        let method = u16(data, off + 10)?;
        let crc = u32(data, off + 16)?;
        let comp_size = u32(data, off + 20)? as u64;
        let uncomp_size = u32(data, off + 24)? as u64;
        let name_len = u16(data, off + 28)? as usize;
        let extra_len = u16(data, off + 30)? as usize;
        let comment_len = u16(data, off + 32)? as usize;
        let local_off = u32(data, off + 42)? as u64;
        if off + 46 + name_len > data.len() {
            return Err(Error::new("nome de arquivo ZIP truncado"));
        }
        let name = decode_name(&data[off + 46..off + 46 + name_len], flags)?;
        let extra = &data[off + 46 + name_len..off + 46 + name_len + extra_len.min(data.len() - (off + 46 + name_len))];
        let (comp_size, uncomp_size, local_off) =
            zip64_overrides(extra, comp_size, uncomp_size, local_off)?;
        if flags & 1 != 0 {
            return Err(Error::new(format!(
                "“{name}” está criptografado. Este conversor não abre ZIP com senha."
            )));
        }
        if !name.ends_with('/') {
            let bytes = read_local(data, local_off as usize, method, comp_size, uncomp_size, crc)?;
            files.push(ZipFile { name, data: bytes });
        }
        off += 46 + name_len + extra_len + comment_len;
    }
    Ok(ZipArchive { files })
}

fn zip64_overrides(extra: &[u8], mut comp: u64, mut uncomp: u64, mut local: u64) -> Result<(u64, u64, u64)> {
    let mut i = 0;
    while i + 4 <= extra.len() {
        let id = u16(extra, i)?;
        let sz = u16(extra, i + 2)? as usize;
        i += 4;
        if i + sz > extra.len() {
            break;
        }
        if id == 0x0001 {
            let mut p = i;
            if uncomp == 0xffff_ffff {
                uncomp = u64(extra, p)?;
                p += 8;
            }
            if comp == 0xffff_ffff {
                comp = u64(extra, p)?;
                p += 8;
            }
            if local == 0xffff_ffff {
                local = u64(extra, p)?;
            }
        }
        i += sz;
    }
    Ok((comp, uncomp, local))
}

fn unzip_zip64(data: &[u8], eocd: usize) -> Result<ZipArchive> {
    // ZIP64 EOCD locator sits 20 bytes before EOCD when present.
    if eocd < 20 {
        return Err(Error::new("ZIP64 incompleto"));
    }
    let loc = eocd - 20;
    if u32(data, loc)? != 0x07064b50 {
        return Err(Error::new(
            "este ZIP usa ZIP64 de um jeito que não consegui ler. Tente recompactar o arquivo.",
        ));
    }
    let z64_off = u64(data, loc + 8)? as usize;
    if z64_off + 56 > data.len() || u32(data, z64_off)? != 0x06064b50 {
        return Err(Error::new("diretório ZIP64 ausente"));
    }
    let entries = u64(data, z64_off + 32)? as usize;
    let cd_off = u64(data, z64_off + 48)? as usize;
    read_cd(data, cd_off, entries)
}

fn decode_name(raw: &[u8], flags: u16) -> Result<String> {
    if flags & (1 << 11) != 0 {
        return Ok(String::from_utf8_lossy(raw).replace('\\', "/"));
    }
    // ZIP historically uses CP437. UTF-8 is common anyway; prefer UTF-8 when valid.
    if let Ok(s) = std::str::from_utf8(raw) {
        return Ok(s.replace('\\', "/"));
    }
    Ok(cp437(raw).replace('\\', "/"))
}

fn read_local(
    data: &[u8],
    off: usize,
    method: u16,
    comp_size: u64,
    uncomp_size: u64,
    crc: u32,
) -> Result<Vec<u8>> {
    if off + 30 > data.len() || u32(data, off)? != 0x04034b50 {
        return Err(Error::new("cabeçalho local do ZIP inválido"));
    }
    let name_len = u16(data, off + 26)? as usize;
    let extra_len = u16(data, off + 28)? as usize;
    let start = off + 30 + name_len + extra_len;
    let comp_size = comp_size as usize;
    if start + comp_size > data.len() {
        return Err(Error::new("dados compactados do ZIP truncados"));
    }
    let comp = &data[start..start + comp_size];
    let plain = match method {
        0 => comp.to_vec(),
        8 => inflate_raw(comp).map_err(|e| {
            Error::new(format!("falha ao descompactar (deflate): {}", e.msg))
        })?,
        other => {
            return Err(Error::new(format!(
                "método de compactação ZIP {other} não é suportado (uso store e deflate)"
            )));
        }
    };
    if uncomp_size != 0 && plain.len() as u64 != uncomp_size && uncomp_size != 0xffff_ffff {
        // Some writers lie about size. Keep the bytes if CRC matches, else warn via error only if CRC fails.
    }
    if crc != 0 && crc32(&plain) != crc && uncomp_size != 0 {
        return Err(Error::new(
            "CRC do ZIP não confere — o arquivo pode estar corrompido",
        ));
    }
    Ok(plain)
}

pub struct ZipWriter {
    files: Vec<(String, Vec<u8>, bool)>,
}

impl ZipWriter {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// `stored` forces method 0. EPUB requires the first entry `mimetype` stored.
    pub fn add(&mut self, name: &str, data: &[u8], stored: bool) {
        self.files.push((name.replace('\\', "/"), data.to_vec(), stored));
    }

    pub fn finish(self) -> Vec<u8> {
        // Deflate-compatible stored blocks would need a compressor. Method 0 is
        // valid for DOCX, ODT, EPUB (except readers that insist on deflate — all
        // major ones accept store) and keeps the binary free of a compressor.
        let mut out = Vec::new();
        let mut central = Vec::new();
        let mut entries = 0u16;
        for (name, data, _stored) in &self.files {
            let local_off = out.len() as u32;
            let name_b = name.as_bytes();
            let crc = crc32(data);
            out.extend_from_slice(&0x04034b50u32.to_le_bytes());
            out.extend_from_slice(&20u16.to_le_bytes()); // version needed
            out.extend_from_slice(&0x0800u16.to_le_bytes()); // utf-8
            out.extend_from_slice(&0u16.to_le_bytes()); // method store
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name_b.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name_b);
            out.extend_from_slice(data);

            central.extend_from_slice(&0x02014b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0x0800u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name_b.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&local_off.to_le_bytes());
            central.extend_from_slice(name_b);
            entries += 1;
        }
        let cd_off = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x06054b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&entries.to_le_bytes());
        out.extend_from_slice(&entries.to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_off.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }
}

fn cp437(raw: &[u8]) -> String {
    const MAP: [char; 128] = [
        '\u{00c7}', '\u{00fc}', '\u{00e9}', '\u{00e2}', '\u{00e4}', '\u{00e0}', '\u{00e5}', '\u{00e7}',
        '\u{00ea}', '\u{00eb}', '\u{00e8}', '\u{00ef}', '\u{00ee}', '\u{00ec}', '\u{00c4}', '\u{00c5}',
        '\u{00c9}', '\u{00e6}', '\u{00c6}', '\u{00f4}', '\u{00f6}', '\u{00f2}', '\u{00fb}', '\u{00f9}',
        '\u{00ff}', '\u{00d6}', '\u{00dc}', '\u{00a2}', '\u{00a3}', '\u{00a5}', '\u{20a7}', '\u{0192}',
        '\u{00e1}', '\u{00ed}', '\u{00f3}', '\u{00fa}', '\u{00f1}', '\u{00d1}', '\u{00aa}', '\u{00ba}',
        '\u{00bf}', '\u{2310}', '\u{00ac}', '\u{00bd}', '\u{00bc}', '\u{00a1}', '\u{00ab}', '\u{00bb}',
        '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}', '\u{2556}',
        '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255d}', '\u{255c}', '\u{255b}', '\u{2510}',
        '\u{2514}', '\u{2534}', '\u{252c}', '\u{251c}', '\u{2500}', '\u{253c}', '\u{255e}', '\u{255f}',
        '\u{255a}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256c}', '\u{2567}',
        '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}', '\u{256b}',
        '\u{256a}', '\u{2518}', '\u{250c}', '\u{2588}', '\u{2584}', '\u{258c}', '\u{2590}', '\u{2580}',
        '\u{03b1}', '\u{00df}', '\u{0393}', '\u{03c0}', '\u{03a3}', '\u{03c3}', '\u{00b5}', '\u{03c4}',
        '\u{03a6}', '\u{0398}', '\u{03a9}', '\u{03b4}', '\u{221e}', '\u{03c6}', '\u{03b5}', '\u{2229}',
        '\u{2261}', '\u{00b1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00f7}', '\u{2248}',
        '\u{00b0}', '\u{2219}', '\u{00b7}', '\u{221a}', '\u{207f}', '\u{00b2}', '\u{25a0}', '\u{00a0}',
    ];
    raw.iter()
        .map(|&b| {
            if b < 128 {
                b as char
            } else {
                MAP[(b - 128) as usize]
            }
        })
        .collect()
}
