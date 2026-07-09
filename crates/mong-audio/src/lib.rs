//! mong-audio — ba bus `bgm / sfx / voice`, crossfade BGM, hàng đợi
//! chờ cử chỉ người dùng (web).
//!
//! Crate này KHÔNG biết `mong-runtime`: shell dịch `AudioCmd` thành lời gọi
//! [`AudioSink`]. Nhờ vậy mũi tên phụ thuộc vẫn một chiều.

#[cfg(feature = "kira-backend")]
mod kira_sink;
#[cfg(feature = "kira-backend")]
pub use kira_sink::KiraAudio;

#[cfg(all(feature = "web-backend", target_arch = "wasm32"))]
mod web_sink;
#[cfg(all(feature = "web-backend", target_arch = "wasm32"))]
pub use web_sink::WebAudio;

use std::fmt;

/// Thời lượng crossfade khi đổi BGM. Prototype cắt cứng; engine thì không.
pub const CROSSFADE_SECS: f64 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Bus {
    Bgm,
    Sfx,
    Voice,
}

impl Bus {
    pub const ALL: [Bus; 3] = [Bus::Bgm, Bus::Sfx, Bus::Voice];
}

#[derive(Debug)]
pub enum AudioError {
    /// Phát một id chưa `register` — lỗi dữ liệu, không phải lỗi cứng.
    UnknownSound(String),
    Decode(String),
    Backend(String),
}

impl fmt::Display for AudioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AudioError::UnknownSound(id) => write!(f, "khong co am thanh '{id}'"),
            AudioError::Decode(m) => write!(f, "khong giai ma duoc: {m}"),
            AudioError::Backend(m) => write!(f, "loi backend audio: {m}"),
        }
    }
}
impl std::error::Error for AudioError {}

/// Hợp đồng shell ↔ audio. Mọi lời gọi đều "không bao giờ panic": id lạ chỉ
/// ghi log rồi bỏ qua, đúng tinh thần `ext` của spec-ir.
pub trait AudioSink {
    /// Nạp trước dữ liệu đã giải nén từ mongpack.
    fn register(&mut self, id: &str, bytes: Vec<u8>) -> Result<(), AudioError>;

    /// `None` = tắt nhạc (spec-ir, `bgm{asset: None}`). Cùng id đang phát
    /// → no-op, không restart bài (đổi cảnh trong cùng BGM không giật).
    fn bgm(&mut self, id: Option<&str>);

    fn sfx(&mut self, id: &str);

    /// Biên độ tuyến tính 0.0..=1.0.
    fn set_bus_volume(&mut self, bus: Bus, volume: f32);

    /// handler của cử chỉ đó: backend dựng thiết bị ở đây (không phải ở
    /// `new`), rồi xả hàng đợi lệnh. Desktop gọi ngay lúc khởi tạo.
    /// Gọi nhiều lần là no-op.
    /// Web: audio chỉ khởi động sau cử chỉ đầu tiên. Trước đó lệnh phát bị
    /// xếp hàng; gọi cái này để xả hàng đợi. Desktop gọi ngay lúc khởi tạo.
    fn unlock(&mut self);
}

/// Backend rỗng: dùng cho test, text-mode runner, và CI không có thiết bị.
#[derive(Debug, Default)]
pub struct NullAudio {
    known: Vec<String>,
    /// Nhật ký lời gọi, dạng `"bgm:x"` / `"bgm:off"` / `"sfx:y"`.
    pub log: Vec<String>,
    unlocked: bool,
    pending: Vec<String>,
}

impl NullAudio {
    pub fn new() -> Self {
        Self::default()
    }

    fn emit(&mut self, entry: String) {
        if self.unlocked {
            self.log.push(entry);
        } else {
            self.pending.push(entry);
        }
    }
}

impl AudioSink for NullAudio {
    fn register(&mut self, id: &str, _bytes: Vec<u8>) -> Result<(), AudioError> {
        self.known.push(id.to_string());
        Ok(())
    }

    fn bgm(&mut self, id: Option<&str>) {
        let e = match id {
            Some(i) => format!("bgm:{i}"),
            None => "bgm:off".to_string(),
        };
        self.emit(e);
    }

    fn sfx(&mut self, id: &str) {
        self.emit(format!("sfx:{id}"));
    }

    fn set_bus_volume(&mut self, bus: Bus, volume: f32) {
        self.emit(format!("vol:{bus:?}:{volume}"));
    }

    fn unlock(&mut self) {
        self.unlocked = true;
        let queued = std::mem::take(&mut self.pending);
        self.log.extend(queued);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xep_hang_toi_khi_unlock() {
        let mut a = NullAudio::new();
        a.bgm(Some("x"));
        a.sfx("ding");
        assert!(a.log.is_empty(), "chua unlock thi chua phat gi");
        a.unlock();
        assert_eq!(a.log, vec!["bgm:x", "sfx:ding"]);
    }

    #[test]
    fn tat_nhac_la_bgm_none() {
        let mut a = NullAudio::new();
        a.unlock();
        a.bgm(None);
        assert_eq!(a.log, vec!["bgm:off"]);
    }
}
