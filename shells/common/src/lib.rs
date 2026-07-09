//! mong-shell — vòng lặp, cửa sổ, input, và việc dịch `Stage`/`Line` của
//! runtime sang `Sprite` của renderer. Dùng chung desktop và web (RFC-002).
//!
//! Khác biệt giữa hai nền tảng chỉ nằm sau `cfg` trong crate này: đồng hồ,
//! backend audio, cách dựng cửa sổ, và cách chờ `request_adapter`. Không rò
//! ra ngoài — hai shell đầu cuối đều dưới 30 dòng.

mod app;
mod state;

pub use app::run;

use mong_audio::AudioSink;

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn audio_moi() -> Box<dyn AudioSink> {
    Box::new(mong_audio::KiraAudio::new().expect("khong mo duoc thiet bi am thanh"))
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn audio_moi() -> Box<dyn AudioSink> {
    Box::new(mong_audio::WebAudio::new().expect("WebAudio::new khong bao gio fail"))
}
