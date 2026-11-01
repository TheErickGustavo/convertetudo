use crate::error::{Error, Result};

const MAX_OUT: usize = 128 * 1024 * 1024;

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn bit(&mut self) -> Result<u32> {
        let byte = self.pos / 8;
        if byte >= self.data.len() {
            return Err(Error::new("fluxo deflate truncado"));
        }
        let b = ((self.data[byte] >> (self.pos % 8)) & 1) as u32;
        self.pos += 1;
        Ok(b)
    }

    fn bits(&mut self, n: u32) -> Result<u32> {
        let mut v = 0u32;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Ok(v)
    }

    fn align(&mut self) {
        if self.pos % 8 != 0 {
            self.pos = (self.pos + 7) & !7;
        }
    }

    fn byte(&mut self) -> Result<u8> {
        self.align();
        let byte = self.pos / 8;
        if byte >= self.data.len() {
            return Err(Error::new("fluxo deflate truncado"));
        }
        self.pos += 8;
        Ok(self.data[byte])
    }
}

struct Huff {
    /// counts[len] = number of codes of that length
    counts: [u16; 16],
    symbols: Vec<u16>,
    min_len: u8,
    max_len: u8,
}

impl Huff {
    fn from_lengths(lengths: &[u8]) -> Result<Self> {
        let mut counts = [0u16; 16];
        let mut max_len = 0u8;
        let mut min_len = 16u8;
        for &l in lengths {
            if l > 15 {
                return Err(Error::new("comprimento Huffman inválido"));
            }
            if l > 0 {
                counts[l as usize] += 1;
                max_len = max_len.max(l);
                min_len = min_len.min(l);
            }
        }
        if max_len == 0 {
            return Ok(Self {
                counts,
                symbols: Vec::new(),
                min_len: 0,
                max_len: 0,
            });
        }
        let mut offs = [0u16; 16];
        let mut acc = 0u16;
        for len in 1..=15 {
            offs[len] = acc;
            acc += counts[len];
        }
        let mut symbols = vec![0u16; acc as usize];
        let mut next = offs;
        for (sym, &l) in lengths.iter().enumerate() {
            if l > 0 {
                let i = next[l as usize] as usize;
                symbols[i] = sym as u16;
                next[l as usize] += 1;
            }
        }
        Ok(Self {
            counts,
            symbols,
            min_len,
            max_len,
        })
    }

    fn decode(&self, bits: &mut Bits) -> Result<u16> {
        if self.max_len == 0 {
            return Err(Error::new("tabela Huffman vazia"));
        }
        let mut code = 0u32;
        let mut first = 0u32;
        let mut index = 0u32;
        for len in 1..=self.max_len {
            code = (code << 1) | bits.bit()?;
            let count = self.counts[len as usize] as u32;
            if code < first + count {
                let i = index + (code - first);
                return Ok(self.symbols[i as usize]);
            }
            index += count;
            first = (first + count) << 1;
        }
        Err(Error::new("código Huffman inválido"))
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115,
    131, 163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

fn fixed_lit() -> Huff {
    let mut lengths = [0u8; 288];
    for i in 0..=143 {
        lengths[i] = 8;
    }
    for i in 144..=255 {
        lengths[i] = 9;
    }
    for i in 256..=279 {
        lengths[i] = 7;
    }
    for i in 280..=287 {
        lengths[i] = 8;
    }
    Huff::from_lengths(&lengths).unwrap()
}

fn fixed_dist() -> Huff {
    Huff::from_lengths(&[5u8; 32]).unwrap()
}

fn inflate_block(bits: &mut Bits, out: &mut Vec<u8>, lit: &Huff, dist: &Huff) -> Result<()> {
    loop {
        let sym = lit.decode(bits)?;
        if sym < 256 {
            push(out, sym as u8)?;
        } else if sym == 256 {
            return Ok(());
        } else if sym <= 285 {
            let idx = (sym - 257) as usize;
            if idx >= LEN_BASE.len() {
                return Err(Error::new("código de comprimento inválido"));
            }
            let extra = bits.bits(LEN_EXTRA[idx] as u32)?;
            let length = LEN_BASE[idx] as usize + extra as usize;
            let dsym = dist.decode(bits)? as usize;
            if dsym >= DIST_BASE.len() {
                return Err(Error::new("código de distância inválido"));
            }
            let dextra = bits.bits(DIST_EXTRA[dsym] as u32)?;
            let distance = DIST_BASE[dsym] as usize + dextra as usize;
            if distance == 0 || distance > out.len() {
                return Err(Error::new("referência deflate fora do buffer"));
            }
            for _ in 0..length {
                let b = out[out.len() - distance];
                push(out, b)?;
            }
        } else {
            return Err(Error::new("símbolo deflate inválido"));
        }
    }
}

fn push(out: &mut Vec<u8>, b: u8) -> Result<()> {
    if out.len() >= MAX_OUT {
        return Err(Error::new(
            "o arquivo descompactado passa de 128 MB — recusei para não estourar a memória",
        ));
    }
    out.push(b);
    Ok(())
}

const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

fn dynamic(bits: &mut Bits) -> Result<(Huff, Huff)> {
    let hlit = bits.bits(5)? as usize + 257;
    let hdist = bits.bits(5)? as usize + 1;
    let hclen = bits.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 32 {
        return Err(Error::new("cabeçalho Huffman dinâmico inválido"));
    }
    let mut cl_lens = [0u8; 19];
    for i in 0..hclen {
        cl_lens[CL_ORDER[i]] = bits.bits(3)? as u8;
    }
    let cl = Huff::from_lengths(&cl_lens)?;
    let total = hlit + hdist;
    let mut lens = vec![0u8; total];
    let mut i = 0;
    while i < total {
        let sym = cl.decode(bits)?;
        if sym < 16 {
            lens[i] = sym as u8;
            i += 1;
        } else if sym == 16 {
            if i == 0 {
                return Err(Error::new("repetição Huffman sem símbolo anterior"));
            }
            let n = bits.bits(2)? as usize + 3;
            let prev = lens[i - 1];
            for _ in 0..n {
                if i >= total {
                    return Err(Error::new("repetição Huffman estoura a tabela"));
                }
                lens[i] = prev;
                i += 1;
            }
        } else if sym == 17 {
            let n = bits.bits(3)? as usize + 3;
            i += n;
        } else if sym == 18 {
            let n = bits.bits(7)? as usize + 11;
            i += n;
        } else {
            return Err(Error::new("código de comprimento inválido"));
        }
    }
    if i != total {
        return Err(Error::new("tabela Huffman incompleta"));
    }
    let lit = Huff::from_lengths(&lens[..hlit])?;
    let dist = Huff::from_lengths(&lens[hlit..])?;
    Ok((lit, dist))
}

pub fn inflate_raw(data: &[u8]) -> Result<Vec<u8>> {
    let mut bits = Bits { data, pos: 0 };
    let mut out = Vec::new();
    let fixed_l = fixed_lit();
    let fixed_d = fixed_dist();
    loop {
        let bfinal = bits.bit()?;
        let btype = bits.bits(2)?;
        match btype {
            0 => {
                bits.align();
                let len = bits.byte()? as usize | ((bits.byte()? as usize) << 8);
                let nlen = bits.byte()? as usize | ((bits.byte()? as usize) << 8);
                if len != (!nlen & 0xffff) {
                    return Err(Error::new("bloco stored do deflate com tamanho inválido"));
                }
                for _ in 0..len {
                    push(&mut out, bits.byte()?)?;
                }
            }
            1 => inflate_block(&mut bits, &mut out, &fixed_l, &fixed_d)?,
            2 => {
                let (l, d) = dynamic(&mut bits)?;
                inflate_block(&mut bits, &mut out, &l, &d)?;
            }
            _ => return Err(Error::new("tipo de bloco deflate inválido")),
        }
        if bfinal == 1 {
            break;
        }
    }
    Ok(out)
}

/// zlib wrapper (RFC 1950), used by PDF FlateDecode.
pub fn inflate_zlib(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < 2 {
        return Err(Error::new("fluxo zlib curto demais"));
    }
    let cmf = data[0];
    let flg = data[1];
    if cmf & 0x0f != 8 {
        return Err(Error::new("zlib com método que não é deflate"));
    }
    if ((cmf as u16) * 256 + flg as u16) % 31 != 0 {
        return Err(Error::new("cabeçalho zlib com checksum inválido"));
    }
    if flg & 0x20 != 0 {
        return Err(Error::new("zlib com dicionário predefinido não é suportado"));
    }
    inflate_raw(&data[2..])
}

pub fn inflate_auto(data: &[u8]) -> Result<Vec<u8>> {
    if data.len() >= 2 && data[0] == 0x78 && (data[1] == 0x01 || data[1] == 0x5e || data[1] == 0x9c || data[1] == 0xda) {
        if let Ok(v) = inflate_zlib(data) {
            return Ok(v);
        }
    }
    inflate_raw(data)
}
