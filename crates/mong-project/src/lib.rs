//! mong-project — nạp dự án thành [`Loaded`], từ thư mục (dev) hoặc từ gói
//! `.mongpack` (phân phối). Nơi duy nhất biết bố cục dự án; CLI, shell
//! desktop, shell web và editor dùng chung.
//!
//! Ranh giới quan trọng: [`load_pack`] không phụ thuộc `mong-script`. Web chỉ
//! bật đường pack, nhờ vậy parser không vào bundle wasm.
//!
//! Bố cục entry trong gói (tên là hợp đồng, đổi = tăng FORMAT_VERSION):
//! ```text
//! manifest.json          Meta
//! story.ir               StoryIr    (JSON của Story)
//! strings/<locale>.json  Strings    (miền nội dung, KHÔNG gồm manifest.strings)
//! assets/<asset_id>      Image|Audio|Font   (tên = id, không phải path)
//! ```

#[cfg(feature = "fs")]
mod dir;
#[cfg(feature = "fs")]
pub use dir::load_dir;

use mong_assets::manifest::ManifestError;
use mong_assets::{read_pack, write_pack, AssetKind, EntryKind, Manifest, PackEntry, PackError};
use mong_core::Story;
use mong_i18n::{Catalog, Table};
use std::collections::BTreeMap;
use std::fmt;

pub const ENTRY_MANIFEST: &str = "manifest.json";
pub const ENTRY_STORY: &str = "story.ir";
pub const PREFIX_STRINGS: &str = "strings/";
pub const PREFIX_ASSETS: &str = "assets/";

#[derive(Debug)]
pub enum ProjectError {
    /// Luôn kèm đường dẫn — `io::Error` trần không cho biết file nào.
    Io {
        path: String,
        source: std::io::Error,
    },
    Pack(PackError),
    Manifest(ManifestError),
    Json(String),
    /// Cú pháp hoặc lint cốt truyện.
    Story(String),
    MissingEntry(&'static str),
    UnknownLocale(String),
    /// Manifest khai báo asset mà không có dữ liệu — chặn lúc `pack`.
    MissingAsset(String),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::Io { path, source } => write!(f, "{path}: {source}"),
            ProjectError::Pack(e) => write!(f, "{e}"),
            ProjectError::Manifest(e) => write!(f, "{e}"),
            ProjectError::Json(m) => write!(f, "json hong: {m}"),
            ProjectError::Story(m) => write!(f, "cot truyen co loi: {m}"),
            ProjectError::MissingEntry(n) => write!(f, "goi thieu entry '{n}'"),
            ProjectError::UnknownLocale(l) => write!(f, "locale '{l}' khong co trong truyen"),
            ProjectError::MissingAsset(id) => {
                write!(
                    f,
                    "asset '{id}' khai bao trong manifest nhung khong co du lieu"
                )
            }
        }
    }
}
impl std::error::Error for ProjectError {}

impl From<PackError> for ProjectError {
    fn from(e: PackError) -> Self {
        ProjectError::Pack(e)
    }
}
impl From<ManifestError> for ProjectError {
    fn from(e: ManifestError) -> Self {
        ProjectError::Manifest(e)
    }
}

/// Dự án đã nạp đầy đủ vào bộ nhớ. Không giữ đường dẫn: web không có fs.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub story: Story,
    /// locale → bảng chuỗi **nội dung**. Miền key của `manifest.strings` tách
    /// riêng, chỉ hợp nhất trong [`Loaded::catalog`] (spec mục 4).
    pub strings: BTreeMap<String, Table>,
    pub manifest: Manifest,
    pub locale: String,
    /// asset id → bytes thô (PNG/OGG/TTF chưa giải mã).
    pub assets: BTreeMap<String, Vec<u8>>,
}

impl Loaded {
    /// Nội dung trước, metadata bù vào chỗ trống (key nội dung thắng).
    pub fn catalog(&self) -> Catalog {
        let mut c = Catalog::new(self.story.default_locale.clone());
        for (loc, t) in &self.strings {
            c.set_table(loc.clone(), t.clone());
        }
        for (loc, t) in &self.manifest.strings {
            c.merge_table(loc.clone(), t.clone());
        }
        c
    }

    pub fn bytes(&self, id: &str) -> Option<&[u8]> {
        self.assets.get(id).map(Vec::as_slice)
    }

    fn by_kind(&self, kind: AssetKind) -> Vec<(&str, &[u8])> {
        self.manifest
            .assets
            .iter()
            .filter(|(_, a)| a.kind == kind)
            .filter_map(|(id, _)| Some((id.as_str(), self.bytes(id)?)))
            .collect()
    }

    pub fn images(&self) -> Vec<(&str, &[u8])> {
        self.by_kind(AssetKind::Image)
    }
    pub fn sounds(&self) -> Vec<(&str, &[u8])> {
        self.by_kind(AssetKind::Audio)
    }

    /// Chuỗi font fallback của locale hiện tại, rơi về defaultLocale.
    pub fn fonts(&self) -> Vec<&[u8]> {
        self.manifest
            .fonts
            .get(&self.locale)
            .or_else(|| self.manifest.fonts.get(&self.story.default_locale))
            .map(|ids| ids.iter().filter_map(|id| self.bytes(id)).collect())
            .unwrap_or_default()
    }

    /// Asset có trong manifest mà thiếu bytes. `load_dir` chỉ cảnh báo;
    /// `to_pack` từ chối.
    pub fn missing_assets(&self) -> Vec<&str> {
        self.manifest
            .assets
            .keys()
            .filter(|id| !self.assets.contains_key(*id))
            .map(String::as_str)
            .collect()
    }
}

pub(crate) fn pick_locale(story: &Story, want: Option<&str>) -> Result<String, ProjectError> {
    match want {
        None => Ok(story.default_locale.clone()),
        Some(l) if l == story.default_locale || story.locales.iter().any(|x| x == l) => {
            Ok(l.to_string())
        }
        Some(l) => Err(ProjectError::UnknownLocale(l.to_string())),
    }
}

fn json<T: serde::Serialize>(what: &str, v: &T) -> Result<Vec<u8>, ProjectError> {
    serde_json::to_vec(v).map_err(|e| ProjectError::Json(format!("{what}: {e}")))
}

pub fn load_pack(bytes: &[u8], locale: Option<&str>) -> Result<Loaded, ProjectError> {
    let mut story: Option<Story> = None;
    let mut manifest: Option<Manifest> = None;
    let mut strings = BTreeMap::new();
    let mut assets = BTreeMap::new();

    for e in read_pack(&mut &bytes[..])? {
        match e.kind {
            EntryKind::StoryIr => {
                story = Some(
                    serde_json::from_slice(&e.data)
                        .map_err(|x| ProjectError::Json(format!("{}: {x}", e.name)))?,
                );
            }
            EntryKind::Meta if e.name == ENTRY_MANIFEST => {
                let s = std::str::from_utf8(&e.data)
                    .map_err(|x| ProjectError::Json(format!("{}: {x}", e.name)))?;
                manifest = Some(Manifest::parse(s)?);
            }
            EntryKind::Strings => {
                let loc = e
                    .name
                    .strip_prefix(PREFIX_STRINGS)
                    .and_then(|n| n.strip_suffix(".json"))
                    .ok_or_else(|| ProjectError::Json(format!("ten entry la: {}", e.name)))?
                    .to_string();
                let t: Table = serde_json::from_slice(&e.data)
                    .map_err(|x| ProjectError::Json(format!("{}: {x}", e.name)))?;
                strings.insert(loc, t);
            }
            EntryKind::Image | EntryKind::Audio | EntryKind::Font => {
                let id = e
                    .name
                    .strip_prefix(PREFIX_ASSETS)
                    .unwrap_or(&e.name)
                    .to_string();
                assets.insert(id, e.data);
            }
            // Meta lạ và Plugin (M5): bỏ qua, gói mới vẫn chạy trên runtime cũ.
            EntryKind::Meta | EntryKind::Plugin => {}
        }
    }

    let story = story.ok_or(ProjectError::MissingEntry(ENTRY_STORY))?;
    let manifest = manifest.ok_or(ProjectError::MissingEntry(ENTRY_MANIFEST))?;
    let locale = pick_locale(&story, locale)?;
    Ok(Loaded {
        story,
        strings,
        manifest,
        locale,
        assets,
    })
}

/// Thứ tự entry cố định (manifest → story → strings → assets, mỗi nhóm theo
/// thứ tự `BTreeMap`) ⇒ cùng dự án cho ra cùng byte. Điều kiện để CI so hash
/// và để `pack` idempotent.
pub fn to_pack(l: &Loaded) -> Result<Vec<PackEntry>, ProjectError> {
    if let Some(id) = l.missing_assets().first() {
        return Err(ProjectError::MissingAsset((*id).to_string()));
    }
    let mut out = vec![
        PackEntry {
            name: ENTRY_MANIFEST.into(),
            kind: EntryKind::Meta,
            data: json("manifest", &l.manifest)?,
        },
        PackEntry {
            name: ENTRY_STORY.into(),
            kind: EntryKind::StoryIr,
            data: json("story", &l.story)?,
        },
    ];
    for (loc, t) in &l.strings {
        out.push(PackEntry {
            name: format!("{PREFIX_STRINGS}{loc}.json"),
            kind: EntryKind::Strings,
            data: json(loc, t)?,
        });
    }
    for (id, a) in &l.manifest.assets {
        let kind = match a.kind {
            AssetKind::Image => EntryKind::Image,
            AssetKind::Audio => EntryKind::Audio,
            AssetKind::Font => EntryKind::Font,
        };
        out.push(PackEntry {
            name: format!("{PREFIX_ASSETS}{id}"),
            kind,
            data: l.assets[id].clone(),
        });
    }
    Ok(out)
}

pub fn to_pack_bytes(l: &Loaded) -> Result<Vec<u8>, ProjectError> {
    let mut buf = Vec::new();
    write_pack(&mut buf, &to_pack(l)?)?;
    Ok(buf)
}
