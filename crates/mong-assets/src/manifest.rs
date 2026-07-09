//! manifest.json — metadata trình diễn: scene, nhân vật, asset, font.
//!
//! Cố ý KHÔNG nằm trong `Story`: mong-core không được biết bg/sprite là gì
//! (ranh giới mục 3 tài liệu thiết kế). Runtime tra manifest khi nhận
//! `VmEvent::SceneChanged` / `Show`. Version độc lập với IR format_version.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// Phiên bản schema manifest. Đổi cấu trúc = tăng số này + viết migration.
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Image,
    Audio,
    Font,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    /// Đường tương đối trong `assets/`.
    pub path: String,
    pub kind: AssetKind,
    #[serde(default)]
    pub hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Key bảng chuỗi (tên hiển thị), rỗng nếu không cần.
    #[serde(default)]
    pub name: String,
    /// Asset id của ảnh nền.
    pub bg: String,
    /// BGM mặc định của cảnh: `scene` phát nó, lệnh `bgm` sau đó ghi đè.
    #[serde(default)]
    pub bgm: Option<String>,
    #[serde(default)]
    pub ambience: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerKind {
    Base,
    Face,
    Outfit,
}

/// Một lớp sprite ghép chồng. `pose` của IR chọn variant của lớp `Face`;
/// các lớp khác dùng `default` cho tới khi có lệnh riêng (mốc sau).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    pub kind: LayerKind,
    /// tên variant → asset id.
    pub variants: BTreeMap<String, String>,
    pub default: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Character {
    #[serde(default)]
    pub name: String,
    /// Màu tên hiển thị, dạng `#rrggbb`.
    #[serde(default)]
    pub color: Option<String>,
    /// Thứ tự vẽ từ dưới lên trên.
    pub layers: Vec<Layer>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub format_version: u32,
    #[serde(default)]
    pub scenes: BTreeMap<String, Scene>,
    #[serde(default)]
    pub characters: BTreeMap<String, Character>,
    #[serde(default)]
    pub assets: BTreeMap<String, Asset>,
    /// locale → chuỗi font fallback (asset id), thử theo thứ tự.
    #[serde(default)]
    pub fonts: BTreeMap<String, Vec<String>>,
}

#[derive(Debug)]
pub enum ManifestError {
    Json(serde_json::Error),
    UnsupportedVersion(u32),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ManifestError::Json(e) => write!(f, "manifest khong doc duoc: {e}"),
            ManifestError::UnsupportedVersion(v) => {
                write!(f, "manifest format_version {v} khong ho tro")
            }
        }
    }
}
impl std::error::Error for ManifestError {}

impl Manifest {
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        let m: Manifest = serde_json::from_str(json).map_err(ManifestError::Json)?;
        if m.format_version > MANIFEST_VERSION {
            return Err(ManifestError::UnsupportedVersion(m.format_version));
        }
        Ok(m)
    }

    pub fn asset_path(&self, id: &str) -> Option<&str> {
        self.assets.get(id).map(|a| a.path.as_str())
    }

    /// Chồng sprite cần vẽ cho nhân vật ở `pose`: các lớp theo thứ tự khai
    /// báo; lớp `Face` lấy variant = pose nếu có, không thì `default`.
    /// Nhân vật lạ → rỗng (runtime bỏ qua, không phải lỗi cứng).
    pub fn sprite_stack(&self, character: &str, pose: Option<&str>) -> Vec<&str> {
        let Some(c) = self.characters.get(character) else {
            return Vec::new();
        };
        c.layers
            .iter()
            .map(|l| {
                let want = match (l.kind, pose) {
                    (LayerKind::Face, Some(p)) => p,
                    _ => l.default.as_str(),
                };
                l.variants
                    .get(want)
                    .or_else(|| l.variants.get(&l.default))
                    .map(String::as_str)
                    .unwrap_or("")
            })
            .filter(|id| !id.is_empty())
            .collect()
    }

    /// Kiểm tra tham chiếu treo — `mong-cli lint`/`pack` gọi trước khi đóng gói.
    pub fn validate(&self) -> Vec<String> {
        let mut out = Vec::new();
        let need = |id: &str, ctx: &str, out: &mut Vec<String>| {
            if !id.is_empty() && !self.assets.contains_key(id) {
                out.push(format!("{ctx}: asset '{id}' khong khai bao"));
            }
        };
        for (sid, s) in &self.scenes {
            need(&s.bg, &format!("scene '{sid}'.bg"), &mut out);
            for (f, a) in [("bgm", &s.bgm), ("ambience", &s.ambience)] {
                if let Some(a) = a {
                    need(a, &format!("scene '{sid}'.{f}"), &mut out);
                }
            }
        }
        for (cid, c) in &self.characters {
            for (i, l) in c.layers.iter().enumerate() {
                if !l.variants.contains_key(&l.default) {
                    out.push(format!(
                        "character '{cid}' layer {i}: default '{}' khong co trong variants",
                        l.default
                    ));
                }
                for (v, a) in &l.variants {
                    need(
                        a,
                        &format!("character '{cid}' layer {i} variant '{v}'"),
                        &mut out,
                    );
                }
            }
        }
        for (loc, chain) in &self.fonts {
            for a in chain {
                need(a, &format!("fonts['{loc}']"), &mut out);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m() -> Manifest {
        Manifest::parse(include_str!("../tests/data/demo-manifest.json")).unwrap()
    }

    #[test]
    fn chong_sprite_theo_pose() {
        assert_eq!(
            m().sprite_stack("lan", Some("vui")),
            vec!["lan_than", "lan_mat_vui", "lan_ao_dai"]
        );
    }

    #[test]
    fn pose_la_roi_ve_default() {
        assert_eq!(
            m().sprite_stack("lan", Some("khong_ton_tai"))[1],
            "lan_mat_thuong"
        );
    }

    #[test]
    fn nhan_vat_la_khong_phai_loi_cung() {
        assert!(m().sprite_stack("ai_do", None).is_empty());
    }

    #[test]
    fn validate_bat_asset_treo() {
        let mut x = m();
        x.scenes.get_mut("quan_ca_phe").unwrap().bg = "khong_co".into();
        assert_eq!(x.validate().len(), 1);
    }

    #[test]
    fn tu_choi_version_moi_hon() {
        let j = r#"{"format_version": 99}"#;
        assert!(matches!(
            Manifest::parse(j),
            Err(ManifestError::UnsupportedVersion(99))
        ));
    }
}
