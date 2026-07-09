//! Harness M4.2 — chứng minh `mong-audio` sống trong trình duyệt trước khi
//! đầu tư vào renderer. Nếu kira chết ở đây thì ta mất một buổi, không phải
//! một tuần. Xoá file này khi shell thật thành hình (M4.3).

use mong_audio::{AudioSink, Bus, WebAudio};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub struct Harness {
    audio: WebAudio,
}

#[wasm_bindgen]
impl Harness {
    /// Gọi lúc nạp trang. **Không** `unlock()` ở đây: WebAudio đòi cử chỉ
    /// người dùng, và `AudioSink::unlock` tồn tại chính vì lý do đó.
    #[wasm_bindgen(constructor)]
    pub fn new(sfx: Vec<u8>, bgm: Vec<u8>) -> Result<Harness, JsError> {
        console_error_panic_hook::set_once();
        let mut audio = WebAudio::new().map_err(|e| JsError::new(&e.to_string()))?;
        audio.register("chuong", sfx).map_err(js)?;
        audio.register("nhac", bgm).map_err(js)?;
        Ok(Harness { audio })
    }

    /// Gọi trong handler `pointerdown`, không sớm hơn. Xả hàng đợi rồi phát.
    pub fn unlock(&mut self) {
        self.audio.unlock();
    }

    pub fn sfx(&mut self) {
        self.audio.sfx("chuong");
    }

    /// Xếp hàng *trước* unlock: kiểm chứng luôn hành vi hàng đợi của
    /// `KiraAudio` (chỉ giữ BGM cuối, sfx phát tuần tự).
    pub fn bgm(&mut self, on: bool) {
        self.audio.bgm(on.then_some("nhac"));
    }

    pub fn volume(&mut self, v: f32) {
        self.audio.set_bus_volume(Bus::Bgm, v);
    }
}

fn js(e: mong_audio::AudioError) -> JsError {
    JsError::new(&e.to_string())
}
