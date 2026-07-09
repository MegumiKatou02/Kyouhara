//! Shell web. JS fetch `.mongpack` rồi gọi `start(bytes)` — Rust không biết
//! `fetch` là gì, và bundle không mang theo hạ tầng HTTP.

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}

/// `locale = null` → defaultLocale của truyện. Không trả về: `spawn_app`
/// nhường quyền cho vòng lặp sự kiện của trang.
#[wasm_bindgen]
pub fn start(pack: Vec<u8>, locale: Option<String>) -> Result<(), JsError> {
    let loaded = mong_project::load_pack(&pack, locale.as_deref())
        .map_err(|e| JsError::new(&e.to_string()))?;
    mong_shell::run(loaded);
    Ok(())
}
