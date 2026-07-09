//! Nạp thư mục dự án. Trùng logic sidecar của mong-cli — nợ kỹ thuật đã ghi:
//! M6 tách thành crate `mong-project` dùng chung cho CLI, shell, editor.
//!
//! Bố cục mong đợi:
//!   <dir>/story.mongscript
//!   <dir>/story.strings.<locale>.json   (locale khác defaultLocale)
//!   <dir>/manifest.json
//!   <dir>/assets/...

use mong_assets::{AssetKind, Manifest};
use mong_core::Story;
use mong_i18n::Catalog;
use std::collections::BTreeMap;
use std::error::Error;
use std::path::{Path, PathBuf};

pub struct Loaded {
    pub story: Story,
    pub catalog: Catalog,
    pub manifest: Manifest,
    pub locale: String,
    root: PathBuf,
}

impl Loaded {
    fn by_kind(&self, kind: AssetKind) -> Vec<(String, PathBuf)> {
        self.manifest
            .assets
            .iter()
            .filter(|(_, a)| a.kind == kind)
            .map(|(id, a)| (id.clone(), self.root.join("assets").join(&a.path)))
            .collect()
    }

    pub fn images(&self) -> Vec<(String, PathBuf)> {
        self.by_kind(AssetKind::Image)
    }
    pub fn sounds(&self) -> Vec<(String, PathBuf)> {
        self.by_kind(AssetKind::Audio)
    }

    /// Chuỗi font fallback của locale hiện tại, rơi về defaultLocale.
    pub fn fonts(&self) -> Vec<PathBuf> {
        let chain = self
            .manifest
            .fonts
            .get(&self.locale)
            .or_else(|| self.manifest.fonts.get(self.catalog.default_locale()));
        chain
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.manifest.asset_path(id))
                    .map(|p| self.root.join("assets").join(p))
                    .collect()
            })
            .unwrap_or_default()
    }
}

pub fn load(dir: &str, locale: Option<&str>) -> Result<Loaded, Box<dyn Error>> {
    let root = PathBuf::from(dir);
    let script = root.join("story.mongscript");
    let src = std::fs::read_to_string(&script).map_err(|e| format!("{}: {e}", script.display()))?;
    let out =
        mong_script::dsl::load_story_dsl(&src).map_err(|e| format!("story.mongscript: {e}"))?;

    let issues = mong_script::validate(&out.story);
    if let Some(err) = issues
        .iter()
        .find(|i| i.severity == mong_script::Severity::Error)
    {
        return Err(format!("cot truyen co loi: {}", err.message).into());
    }

    let manifest = Manifest::parse(&std::fs::read_to_string(root.join("manifest.json"))?)?;
    for msg in manifest.validate() {
        eprintln!("manifest: {msg}");
    }

    let mut catalog = Catalog::new(out.story.default_locale.clone());
    catalog.set_table(out.story.default_locale.clone(), out.strings);
    for loc in &out.story.locales {
        let p = root.join(format!("story.strings.{loc}.json"));
        if p.exists() {
            let table: BTreeMap<String, String> =
                serde_json::from_str(&std::fs::read_to_string(&p)?)?;
            catalog.set_table(loc.clone(), table);
        }
    }

    // Bảng chuỗi metadata (manifest v2): bù vào, không đè lên bảng nội dung.
    for (loc, table) in &manifest.strings {
        catalog.merge_table(loc.clone(), table.clone());
    }

    let locale = locale.unwrap_or(&out.story.default_locale).to_string();
    Ok(Loaded {
        story: out.story,
        catalog,
        manifest,
        locale,
        root,
    })
}

/// PNG → RGBA8. `png` thay `image` để giữ dependency mỏng (web build < 5 MB).
pub fn load_png(path: &Path) -> Result<(Vec<u8>, u32, u32), Box<dyn Error>> {
    let decoder = png::Decoder::new(std::fs::File::open(path)?);
    let mut reader = decoder.read_info()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf)?;
    buf.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buf,
        png::ColorType::Rgb => buf
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        other => return Err(format!("{path:?}: color type {other:?} chua ho tro").into()),
    };
    Ok((rgba, info.width, info.height))
}
