use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentFileInfo {
    pub index: usize, // 1-based index for aria2 --select-file
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TorrentInspectResult {
    pub name: String,
    pub total_size: u64,
    pub info_hash: String,
    pub piece_length: u64,
    pub piece_count: usize,
    pub comment: Option<String>,
    pub created_by: Option<String>,
    pub creation_date: Option<i64>,
    pub files: Vec<TorrentFileInfo>,
}

#[derive(Debug, Clone, PartialEq)]
enum BVal<'a> {
    Int(i64),
    Bytes(&'a [u8]),
    List(Vec<BVal<'a>>),
    Dict(Vec<(&'a [u8], BVal<'a>)>),
}

struct BencodeParser<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> BencodeParser<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn peek(&self) -> Option<u8> {
        self.data.get(self.pos).copied()
    }

    fn next_byte(&mut self) -> Option<u8> {
        if self.pos < self.data.len() {
            let b = self.data[self.pos];
            self.pos += 1;
            Some(b)
        } else {
            None
        }
    }

    fn parse_value(&mut self) -> Result<BVal<'a>> {
        match self.peek().ok_or_else(|| anyhow!("Unexpected EOF"))? {
            b'i' => self.parse_int(),
            b'l' => self.parse_list(),
            b'd' => self.parse_dict(),
            b'0'..=b'9' => self.parse_bytes(),
            other => Err(anyhow!("Invalid bencode byte: {}", other as char)),
        }
    }

    fn parse_int(&mut self) -> Result<BVal<'a>> {
        self.next_byte(); // skip 'i'
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b'e' {
                break;
            }
            self.pos += 1;
        }
        let end = self.pos;
        self.next_byte(); // skip 'e'
        let s = std::str::from_utf8(&self.data[start..end])?;
        let n: i64 = s.parse()?;
        Ok(BVal::Int(n))
    }

    fn parse_bytes(&mut self) -> Result<BVal<'a>> {
        let start = self.pos;
        while let Some(b) = self.peek() {
            if b == b':' {
                break;
            }
            self.pos += 1;
        }
        let end = self.pos;
        self.next_byte(); // skip ':'
        let len_str = std::str::from_utf8(&self.data[start..end])?;
        let len: usize = len_str.parse()?;
        if self.pos + len > self.data.len() {
            return Err(anyhow!("Bencode byte string out of bounds"));
        }
        let bytes = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(BVal::Bytes(bytes))
    }

    fn parse_list(&mut self) -> Result<BVal<'a>> {
        self.next_byte(); // skip 'l'
        let mut list = Vec::new();
        while let Some(b) = self.peek() {
            if b == b'e' {
                self.next_byte();
                return Ok(BVal::List(list));
            }
            list.push(self.parse_value()?);
        }
        Err(anyhow!("Unterminated bencode list"))
    }

    fn parse_dict(&mut self) -> Result<BVal<'a>> {
        self.next_byte(); // skip 'd'
        let mut dict = Vec::new();
        while let Some(b) = self.peek() {
            if b == b'e' {
                self.next_byte();
                return Ok(BVal::Dict(dict));
            }
            let key = match self.parse_value()? {
                BVal::Bytes(k) => k,
                _ => return Err(anyhow!("Dictionary key must be byte string")),
            };
            let val = self.parse_value()?;
            dict.push((key, val));
        }
        Err(anyhow!("Unterminated bencode dict"))
    }
}

/// Standard SHA-1 computation for info_hash
fn sha1_digest(data: &[u8]) -> [u8; 20] {
    let mut h0: u32 = 0x67452301;
    let mut h1: u32 = 0xEFCDAB89;
    let mut h2: u32 = 0x98BADCFE;
    let mut h3: u32 = 0x10325476;
    let mut h4: u32 = 0xC3D2E1F0;

    let msg_len = data.len();
    let bit_len = (msg_len as u64) * 8;

    let mut padded = data.to_vec();
    padded.push(0x80);
    while (padded.len() % 64) != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in padded.chunks_exact(64) {
        let mut w = [0u32; 80];
        for (i, c) in chunk.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([c[0], c[1], c[2], c[3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }

        let mut a = h0;
        let mut b = h1;
        let mut c = h2;
        let mut d = h3;
        let mut e = h4;

        for (i, &w_val) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((b & c) | ((!b) & d), 0x5A827999),
                20..=39 => (b ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
                _ => (b ^ c ^ d, 0xCA62C1D6),
            };

            let temp = a
                .rotate_left(5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w_val);
            e = d;
            d = c;
            c = b.rotate_left(30);
            b = a;
            a = temp;
        }

        h0 = h0.wrapping_add(a);
        h1 = h1.wrapping_add(b);
        h2 = h2.wrapping_add(c);
        h3 = h3.wrapping_add(d);
        h4 = h4.wrapping_add(e);
    }

    let mut result = [0u8; 20];
    result[0..4].copy_from_slice(&h0.to_be_bytes());
    result[4..8].copy_from_slice(&h1.to_be_bytes());
    result[8..12].copy_from_slice(&h2.to_be_bytes());
    result[12..16].copy_from_slice(&h3.to_be_bytes());
    result[16..20].copy_from_slice(&h4.to_be_bytes());
    result
}

/// Find raw byte slice range of info dict inside raw torrent data
fn find_raw_info_dict(data: &[u8]) -> Option<&[u8]> {
    let mut pos = 0;
    while pos < data.len() {
        if data[pos..].starts_with(b"4:info") {
            let start = pos + 6;
            // Now parse until the matching 'e' for this dict
            if start < data.len() && data[start] == b'd' {
                let mut parser = BencodeParser::new(&data[start..]);
                if parser.parse_dict().is_ok() {
                    let end = start + parser.pos;
                    return Some(&data[start..end]);
                }
            }
        }
        pos += 1;
    }
    None
}

/// Inspect a local .torrent file and extract file hierarchy, sizes, info hash, and metadata
pub fn inspect_torrent_bytes(data: &[u8]) -> Result<TorrentInspectResult> {
    let mut parser = BencodeParser::new(data);
    let root = parser.parse_value().map_err(|e| anyhow!("Failed to parse bencode: {}", e))?;

    let root_dict = match root {
        BVal::Dict(d) => d,
        _ => return Err(anyhow!("Root bencode is not a dictionary")),
    };

    let mut comment = None;
    let mut created_by = None;
    let mut creation_date = None;
    let mut info_val = None;

    for (k, v) in root_dict {
        match k {
            b"comment" => {
                if let BVal::Bytes(b) = v {
                    comment = String::from_utf8(b.to_vec()).ok();
                }
            }
            b"created by" => {
                if let BVal::Bytes(b) = v {
                    created_by = String::from_utf8(b.to_vec()).ok();
                }
            }
            b"creation date" => {
                if let BVal::Int(ts) = v {
                    creation_date = Some(ts);
                }
            }
            b"info" => {
                info_val = Some(v);
            }
            _ => {}
        }
    }

    let info_dict = match info_val {
        Some(BVal::Dict(d)) => d,
        _ => return Err(anyhow!("Missing info dictionary in torrent")),
    };

    // Compute Info Hash v1 (SHA1 of the raw info dict bytes)
    let info_bytes = find_raw_info_dict(data).ok_or_else(|| anyhow!("Failed to isolate raw info dict"))?;
    let hash_bytes = sha1_digest(info_bytes);
    let mut info_hash = String::with_capacity(40);
    for byte in hash_bytes {
        use std::fmt::Write;
        let _ = write!(&mut info_hash, "{:02x}", byte);
    }

    let mut name = String::new();
    let mut piece_length: u64 = 0;
    let mut pieces_bytes = Vec::new();
    let mut files_val = None;
    let mut single_length: Option<u64> = None;

    for (k, v) in info_dict {
        match k {
            b"name" => {
                if let BVal::Bytes(b) = v {
                    name = String::from_utf8_lossy(b).to_string();
                }
            }
            b"piece length" => {
                if let BVal::Int(pl) = v {
                    piece_length = pl as u64;
                }
            }
            b"pieces" => {
                if let BVal::Bytes(b) = v {
                    pieces_bytes = b.to_vec();
                }
            }
            b"files" => {
                files_val = Some(v);
            }
            b"length" => {
                if let BVal::Int(l) = v {
                    single_length = Some(l as u64);
                }
            }
            _ => {}
        }
    }

    let piece_count = pieces_bytes.len() / 20;
    let mut files = Vec::new();
    let mut total_size: u64 = 0;

    if let Some(BVal::List(file_list)) = files_val {
        for (i, f_val) in file_list.into_iter().enumerate() {
            if let BVal::Dict(f_dict) = f_val {
                let mut f_len: u64 = 0;
                let mut path_parts = Vec::new();

                for (fk, fv) in f_dict {
                    if fk == b"length" {
                        if let BVal::Int(l) = fv {
                            f_len = l as u64;
                        }
                    } else if fk == b"path" {
                        if let BVal::List(p_list) = fv {
                            for p in p_list {
                                if let BVal::Bytes(pb) = p {
                                    path_parts.push(String::from_utf8_lossy(pb).to_string());
                                }
                            }
                        }
                    }
                }

                let subpath = path_parts.join("/");
                let full_path = if !name.is_empty() {
                    format!("{}/{}", name, subpath)
                } else {
                    subpath
                };

                total_size += f_len;
                files.push(TorrentFileInfo {
                    index: i + 1, // 1-based index
                    path: full_path,
                    size: f_len,
                });
            }
        }
    } else if let Some(len) = single_length {
        total_size = len;
        files.push(TorrentFileInfo {
            index: 1,
            path: name.clone(),
            size: len,
        });
    }

    Ok(TorrentInspectResult {
        name,
        total_size,
        info_hash,
        piece_length,
        piece_count,
        comment,
        created_by,
        creation_date,
        files,
    })
}

pub fn inspect_torrent_file(path: impl AsRef<Path>) -> Result<TorrentInspectResult> {
    let bytes = std::fs::read(path)?;
    inspect_torrent_bytes(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bencode_int() {
        let mut parser = BencodeParser::new(b"i42e");
        let val = parser.parse_value().unwrap();
        assert_eq!(val, BVal::Int(42));

        let mut neg_parser = BencodeParser::new(b"i-100e");
        let neg_val = neg_parser.parse_value().unwrap();
        assert_eq!(neg_val, BVal::Int(-100));
    }

    #[test]
    fn test_bencode_bytes() {
        let mut parser = BencodeParser::new(b"4:spam");
        let val = parser.parse_value().unwrap();
        assert_eq!(val, BVal::Bytes(b"spam"));
    }

    #[test]
    fn test_bencode_list() {
        let mut parser = BencodeParser::new(b"l4:spami42ee");
        let val = parser.parse_value().unwrap();
        assert_eq!(val, BVal::List(vec![BVal::Bytes(b"spam"), BVal::Int(42)]));
    }

    #[test]
    fn test_bencode_dict_and_torrent_inspection() {
        // Construct a synthetic minimal torrent bencode
        // d8:announce12:udp://trk:804:infod6:lengthi1024e4:name8:test.bin12:piece lengthi512e6:pieces20:12345678901234567890ee
        let torrent_data = b"d8:announce12:udp://trk:804:infod6:lengthi1024e4:name8:test.bin12:piece lengthi512e6:pieces20:12345678901234567890ee";
        let res = inspect_torrent_bytes(torrent_data).expect("Should parse torrent bytes");
        assert_eq!(res.name, "test.bin");
        assert_eq!(res.total_size, 1024);
        assert_eq!(res.piece_length, 512);
        assert_eq!(res.piece_count, 1);
        assert_eq!(res.files.len(), 1);
        assert_eq!(res.files[0].index, 1);
        assert_eq!(res.files[0].path, "test.bin");
        assert_eq!(res.files[0].size, 1024);
        assert_eq!(res.info_hash.len(), 40);
    }
}

