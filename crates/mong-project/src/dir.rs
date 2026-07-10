//! Đường dev: nạp thư mục dự án. Chỉ tồn tại ở feature `fs`.
//!
//!   <dir>/story.mongscript
//!   <dir>/story.strings.<locale>.json   (locale khác defaultLocale)
//!   <dir>/manifest.json
//!   <dir>/assets/...

use crate::{pick_locale, Loaded, ProjectError};
use mong_assets::Manifest;
use mong_i18n::Table;
use std::collections::BTreeMap;
use std::path::Path;

fn read_text(p: &Path) -> Result<String, ProjectError> {
    std::fs::read_to_string(p).map_err(|source| ProjectError::Io {
        path: p.display().to_string(),
        source,
    })
}

fn read_bytes(p: &Path) -> Result<Vec<u8>, ProjectError> {
    std::fs::read(p).map_err(|source| ProjectError::Io {
        path: p.display().to_string(),
        source,
    })
}

pub fn load_dir(dir: impl AsRef<Path>, locale: Option<&str>) -> Result<Loaded, ProjectError> {
    let root = dir.as_ref();
    let src = read_text(&root.join("story.mongscript"))?;
    let out = mong_script::dsl::load_story_dsl(&src)
        .map_err(|e| ProjectError::Story(format!("story.mongscript: {e}")))?;

    if let Some(err) = mong_script::validate(&out.story)
        .into_iter()
        .find(|i| i.severity == mong_script::Severity::Error)
    {
        return Err(ProjectError::Story(err.message));
    }

    let manifest = Manifest::parse(&read_text(&root.join("manifest.json"))?)?;
    for msg in manifest.validate() {
        eprintln!("manifest: {msg}");
    }

    let mut strings = BTreeMap::new();
    strings.insert(out.story.default_locale.clone(), out.strings);
    for loc in &out.story.locales {
        let p = root.join(format!("story.strings.{loc}.json"));
        if p.exists() {
            let t: Table = serde_json::from_str(&read_text(&p)?)
                .map_err(|e| ProjectError::Json(format!("{}: {e}", p.display())))?;
            strings.insert(loc.clone(), t);
        }
    }

    // File asset thiếu chỉ cảnh báo: chạy câm vẫn hơn không chạy (demo chưa
    // có .ogg). Chỗ từ chối là `to_pack` — gói hỏng phải lộ lúc build.
    let mut assets = BTreeMap::new();
    for (id, a) in &manifest.assets {
        let p = root.join("assets").join(&a.path);
        match read_bytes(&p) {
            Ok(b) => {
                assets.insert(id.clone(), b);
            }
            Err(e) => eprintln!("canh bao: asset '{id}': {e}"),
        }
    }

    let locale = pick_locale(&out.story, locale)?;
    Ok(Loaded {
        story: out.story,
        strings,
        manifest,
        locale,
        assets,
    })
}
