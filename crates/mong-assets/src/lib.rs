//! mong-assets — định dạng gói phân phối `.mongpack` v0.
//!
//! Bố cục nhị phân (mọi số đều little-endian):
//! ```text
//! magic "MONGPACK" (8B) | format_version u32 | codec u8 | entry_count u32
//! sau đó với mỗi entry:
//!   name_len u16 | name (UTF-8) | kind u8 | raw_len u64 | comp_len u64
//!   | crc32(raw) u32 | dữ liệu đã nén (comp_len byte)
//! ```
//! Codec v0 = DEFLATE (thuần Rust, chạy được cả trên WASM). Trường `codec`
//! trong header cho phép thêm zstd làm codec 2 sau này mà không phá định dạng.

pub mod manifest;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use flate2::Compression;
pub use manifest::{Asset, AssetKind, Character, Layer, LayerKind, Manifest, Scene};
use std::fmt;
use std::io::{self, Read, Write};

pub const MAGIC: &[u8; 8] = b"MONGPACK";
pub const FORMAT_VERSION: u32 = 0;
pub const CODEC_DEFLATE: u8 = 1;

/// Loại nội dung của một entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Meta,
    StoryIr,
    Strings,
    Image,
    Audio,
    Plugin,
    Font,
    /// Kind chưa biết — gói được tạo bởi engine mới hơn. Đọc được (khung
    /// entry tự mô tả kích thước), tầng trên bỏ qua (mongpack-entries §5.2).
    /// `to_u8(Unknown(x))` trả nguyên `x` để công cụ đọc-sửa-ghi giữ được
    /// entry lạ; đừng tự tay dựng `Unknown` với byte của kind đã biết.
    Unknown(u8),
}

impl EntryKind {
    fn to_u8(self) -> u8 {
        match self {
            EntryKind::Meta => 0,
            EntryKind::StoryIr => 1,
            EntryKind::Strings => 2,
            EntryKind::Image => 3,
            EntryKind::Audio => 4,
            EntryKind::Plugin => 5,
            EntryKind::Font => 6,
            EntryKind::Unknown(v) => v,
        }
    }
    fn from_u8(v: u8) -> Self {
        match v {
            0 => EntryKind::Meta,
            1 => EntryKind::StoryIr,
            2 => EntryKind::Strings,
            3 => EntryKind::Image,
            4 => EntryKind::Audio,
            5 => EntryKind::Plugin,
            6 => EntryKind::Font,
            other => EntryKind::Unknown(other),
        }
    }
}

/// Một entry trong gói.
#[derive(Debug, Clone, PartialEq)]
pub struct PackEntry {
    pub name: String,
    pub kind: EntryKind,
    pub data: Vec<u8>,
}

/// Lỗi đọc/ghi gói.
#[derive(Debug)]
pub enum PackError {
    Io(io::Error),
    BadMagic,
    BadVersion(u32),
    BadCodec(u8),
    Corrupt(String),
}

impl fmt::Display for PackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PackError::Io(e) => write!(f, "loi io: {e}"),
            PackError::BadMagic => write!(f, "khong phai file .mongpack"),
            PackError::BadVersion(v) => write!(f, "format_version {v} khong ho tro"),
            PackError::BadCodec(c) => write!(f, "codec {c} khong ho tro"),
            PackError::Corrupt(m) => write!(f, "goi hong: {m}"),
        }
    }
}
impl std::error::Error for PackError {}
impl From<io::Error> for PackError {
    fn from(e: io::Error) -> Self {
        PackError::Io(e)
    }
}

fn w_u16<W: Write>(w: &mut W, v: u16) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn w_u32<W: Write>(w: &mut W, v: u32) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn w_u64<W: Write>(w: &mut W, v: u64) -> io::Result<()> {
    w.write_all(&v.to_le_bytes())
}
fn r_u16<R: Read>(r: &mut R) -> io::Result<u16> {
    let mut b = [0u8; 2];
    r.read_exact(&mut b)?;
    Ok(u16::from_le_bytes(b))
}
fn r_u32<R: Read>(r: &mut R) -> io::Result<u32> {
    let mut b = [0u8; 4];
    r.read_exact(&mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn r_u64<R: Read>(r: &mut R) -> io::Result<u64> {
    let mut b = [0u8; 8];
    r.read_exact(&mut b)?;
    Ok(u64::from_le_bytes(b))
}

/// Ghi một gói .mongpack.
pub fn write_pack<W: Write>(w: &mut W, entries: &[PackEntry]) -> Result<(), PackError> {
    w.write_all(MAGIC)?;
    w_u32(w, FORMAT_VERSION)?;
    w.write_all(&[CODEC_DEFLATE])?;
    w_u32(w, entries.len() as u32)?;
    for e in entries {
        let name = e.name.as_bytes();
        if name.len() > u16::MAX as usize {
            return Err(PackError::Corrupt(format!("ten entry qua dai: {}", e.name)));
        }
        let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
        enc.write_all(&e.data)?;
        let comp = enc.finish()?;
        let crc = crc32fast::hash(&e.data);

        w_u16(w, name.len() as u16)?;
        w.write_all(name)?;
        w.write_all(&[e.kind.to_u8()])?;
        w_u64(w, e.data.len() as u64)?;
        w_u64(w, comp.len() as u64)?;
        w_u32(w, crc)?;
        w.write_all(&comp)?;
    }
    Ok(())
}

/// Đọc toàn bộ gói, xác thực magic/version/codec/crc từng entry.
pub fn read_pack<R: Read>(r: &mut R) -> Result<Vec<PackEntry>, PackError> {
    let mut magic = [0u8; 8];
    r.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(PackError::BadMagic);
    }
    let ver = r_u32(r)?;
    // mongpack-entries §5.1: đọc được mọi version cũ hơn trong cùng major;
    // mới hơn thì từ chối. FORMAT_VERSION = 0 nên nhánh migrate còn rỗng —
    // khi tăng version, thêm `match ver` chuyển đổi bố cục ngay tại đây.
    if ver > FORMAT_VERSION {
        return Err(PackError::BadVersion(ver));
    }
    let mut codec = [0u8; 1];
    r.read_exact(&mut codec)?;
    if codec[0] != CODEC_DEFLATE {
        return Err(PackError::BadCodec(codec[0]));
    }
    let count = r_u32(r)?;
    let mut out = Vec::new();
    for _ in 0..count {
        let name_len = r_u16(r)? as usize;
        let mut name_buf = vec![0u8; name_len];
        r.read_exact(&mut name_buf)?;
        let name = String::from_utf8(name_buf)
            .map_err(|_| PackError::Corrupt("ten entry khong phai UTF-8".into()))?;
        let mut kind_b = [0u8; 1];
        r.read_exact(&mut kind_b)?;
        // Kind lạ không phải gói hỏng — là gói mới hơn runtime (§5.2).
        let kind = EntryKind::from_u8(kind_b[0]);
        let raw_len = r_u64(r)?;
        let comp_len = r_u64(r)?;
        let crc = r_u32(r)?;
        // Doc co gioi han: khong bao gio cap phat theo con so trong header,
        // nen header hong (hoac ac y) chi dan den loi Corrupt, khong the
        // lam tran bo nho hay zip-bomb.
        let mut comp = Vec::new();
        let mut limited = r.by_ref().take(comp_len);
        let mut buf = [0u8; 8192];
        loop {
            match limited.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => comp.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(PackError::Io(e)),
            }
        }
        if comp.len() as u64 != comp_len {
            return Err(PackError::Corrupt(format!(
                "entry '{name}': thieu du lieu nen ({} / {comp_len} byte)",
                comp.len()
            )));
        }
        let mut data = Vec::new();
        {
            let mut decoder = DeflateDecoder::new(&comp[..]).take(raw_len.saturating_add(1));
            let mut buf = [0u8; 8192];
            loop {
                match decoder.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => data.extend_from_slice(&buf[..n]),
                    Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => return Err(PackError::Io(e)),
                }
            }
        }
        if data.len() as u64 != raw_len {
            return Err(PackError::Corrupt(format!(
                "entry '{name}': kich thuoc giai nen {} != khai bao {raw_len}",
                data.len()
            )));
        }
        if crc32fast::hash(&data) != crc {
            return Err(PackError::Corrupt(format!("entry '{name}': sai crc32")));
        }
        out.push(PackEntry { name, kind, data });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Vec<PackEntry> {
        vec![
            PackEntry {
                name: "meta.json".into(),
                kind: EntryKind::Meta,
                data: b"{}".to_vec(),
            },
            PackEntry {
                name: "assets/nhac.ogg".into(),
                kind: EntryKind::Audio,
                data: (0..2048u32).flat_map(|i| i.to_le_bytes()).collect(),
            },
            PackEntry {
                name: "rong".into(),
                kind: EntryKind::Image,
                data: vec![],
            },
        ]
    }

    /// §5.2: kind lạ đọc được và round-trip giữ nguyên — gói mới hơn không
    /// giết runtime cũ.
    #[test]
    fn kind_la_khong_giet_goi() {
        let entries = vec![
            PackEntry {
                name: "tuong_lai.bin".into(),
                kind: EntryKind::Unknown(200),
                data: vec![1, 2, 3],
            },
            PackEntry {
                name: "story.ir".into(),
                kind: EntryKind::StoryIr,
                data: b"{}".to_vec(),
            },
        ];
        let mut buf = Vec::new();
        write_pack(&mut buf, &entries).unwrap();
        let back = read_pack(&mut &buf[..]).unwrap();
        assert_eq!(entries, back);
    }

    /// §5.1: version mới hơn runtime → từ chối rõ ràng, không đoán mò.
    #[test]
    fn version_moi_hon_bi_tu_choi() {
        let mut buf = Vec::new();
        write_pack(&mut buf, &sample()).unwrap();
        // Header: magic 8B, rồi format_version u32 LE tại offset 8.
        buf[8..12].copy_from_slice(&(FORMAT_VERSION + 1).to_le_bytes());
        assert!(matches!(
            read_pack(&mut &buf[..]),
            Err(PackError::BadVersion(v)) if v == FORMAT_VERSION + 1
        ));
    }

    #[test]
    fn round_trip() {
        let entries = sample();
        let mut buf = Vec::new();
        write_pack(&mut buf, &entries).unwrap();
        let back = read_pack(&mut &buf[..]).unwrap();
        assert_eq!(entries, back);
    }

    #[test]
    fn phat_hien_hong_du_lieu() {
        let mut buf = Vec::new();
        write_pack(&mut buf, &sample()).unwrap();
        // Lật một bit ở giữa phần dữ liệu nén.
        let mid = buf.len() - 10;
        buf[mid] ^= 0xFF;
        assert!(read_pack(&mut &buf[..]).is_err());
    }

    #[test]
    fn tu_choi_magic_la() {
        let buf = b"KHONGPACKxxxxxxxxxxx".to_vec();
        assert!(matches!(read_pack(&mut &buf[..]), Err(PackError::BadMagic)));
    }
}
