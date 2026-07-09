//! Backend WebAudio thuần `web-sys`. Không kira, không cpal, không symphonia.
//!
//! Vì sao không dùng kira trên web (bằng chứng M4.2): cpal-wasm không dựng
//! `AudioContext` nào cả — `AudioManager::new()` trả Ok rồi câm lặng. Ngoài
//! ra `decodeAudioData` của trình duyệt giải mã native, miễn phí, và nuốt
//! mọi định dạng; symphonia trong wasm thì tốn CPU lẫn vài trăm KB bundle.
//!
//! Bất biến: `AudioContext` **chỉ** ra đời trong [`AudioSink::unlock`], tức
//! trong ngăn xếp của cử chỉ người dùng. Dựng nó sớm hơn thì trình duyệt cho
//! trạng thái `suspended` và mọi thứ im lặng.

use crate::{AudioError, AudioSink, Bus, CROSSFADE_SECS};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{AudioBuffer, AudioBufferSourceNode, AudioContext, GainNode};

#[cfg(not(target_arch = "wasm32"))]
compile_error!(
    "feature `web-backend` chi danh cho wasm32. Tren host, dung `kira-backend`.\n\
     Neu ban vua chay `cargo test --all-features`: xem [package.metadata] trong Cargo.toml."
);

fn canh_bao(m: &str) {
    web_sys::console::warn_1(&JsValue::from_str(m));
}

fn loi(e: JsValue) -> AudioError {
    AudioError::Backend(format!("{e:?}"))
}

#[derive(Default)]
struct Inner {
    /// `None` cho tới khi `unlock()`.
    ctx: Option<AudioContext>,
    buses: HashMap<Bus, GainNode>,
    /// Volume đặt trước `unlock` phải nhớ, không thì bị nuốt.
    volumes: HashMap<Bus, f32>,
    /// Bytes chưa giải mã. `register` chạy trước `unlock`, mà giải mã cần ctx.
    raw: HashMap<String, Vec<u8>>,
    buffers: HashMap<String, AudioBuffer>,
    /// Ý muốn mới nhất: `None` = tắt nhạc. Áp lại mỗi khi có buffer mới.
    want_bgm: Option<String>,
    /// Nguồn đang phát + gain riêng của nó (để crossfade).
    current: Option<(String, AudioBufferSourceNode, GainNode)>,
    /// Sfx yêu cầu khi buffer chưa giải mã xong.
    pending_sfx: Vec<String>,
}

pub struct WebAudio {
    inner: Rc<RefCell<Inner>>,
}

impl WebAudio {
    pub fn new() -> Result<Self, AudioError> {
        Ok(WebAudio {
            inner: Rc::new(RefCell::new(Inner::default())),
        })
    }
}

impl Default for WebAudio {
    fn default() -> Self {
        Self::new().expect("WebAudio::new khong bao gio fail")
    }
}

impl Inner {
    fn dung_thiet_bi(&mut self) -> Result<(), AudioError> {
        let ctx = AudioContext::new().map_err(loi)?;
        let dest = ctx.destination();
        for bus in Bus::ALL {
            let g = ctx.create_gain().map_err(loi)?;
            g.gain()
                .set_value(self.volumes.get(&bus).copied().unwrap_or(1.0));
            g.connect_with_audio_node(&dest).map_err(loi)?;
            self.buses.insert(bus, g);
        }
        self.ctx = Some(ctx);
        Ok(())
    }

    /// Đưa trạng thái thật về khớp `want_bgm`, rồi xả sfx đang chờ buffer.
    /// Gọi lại sau mỗi lần một buffer giải mã xong.
    fn dong_bo(&mut self) {
        let Some(ctx) = self.ctx.clone() else { return };

        let dang_dung = self.current.as_ref().map(|(id, ..)| id.clone());
        if dang_dung != self.want_bgm {
            match &self.want_bgm {
                // Buffer chưa xong: để nguyên, lần giải mã sau sẽ gọi lại.
                Some(id) if !self.buffers.contains_key(id) => {}
                Some(id) => {
                    let id = id.clone();
                    self.fade_out();
                    if let Err(e) = self.phat_bgm(&ctx, &id) {
                        canh_bao(&e.to_string());
                    }
                }
                None => self.fade_out(),
            }
        }

        let cho = std::mem::take(&mut self.pending_sfx);
        for id in cho {
            if self.buffers.contains_key(&id) {
                if let Err(e) = self.phat_sfx(&ctx, &id) {
                    canh_bao(&e.to_string());
                }
            } else {
                self.pending_sfx.push(id);
            }
        }
    }

    fn phat_bgm(&mut self, ctx: &AudioContext, id: &str) -> Result<(), AudioError> {
        let buf = &self.buffers[id];
        let src = ctx.create_buffer_source().map_err(loi)?;
        src.set_buffer(Some(buf));
        src.set_loop(true);

        let g = ctx.create_gain().map_err(loi)?;
        g.gain().set_value(0.0);
        g.gain()
            .linear_ramp_to_value_at_time(1.0, ctx.current_time() + CROSSFADE_SECS)
            .map_err(loi)?;

        src.connect_with_audio_node(&g).map_err(loi)?;
        g.connect_with_audio_node(&self.buses[&Bus::Bgm])
            .map_err(loi)?;
        src.start().map_err(loi)?;

        self.current = Some((id.to_string(), src, g));
        Ok(())
    }

    fn fade_out(&mut self) {
        let (Some(ctx), Some((_, src, g))) = (self.ctx.as_ref(), self.current.take()) else {
            return;
        };
        let het = ctx.current_time() + CROSSFADE_SECS;
        // `cancel_scheduled_values` + neo giá trị hiện tại: nếu bài này còn
        // đang fade-in dở, ramp mới phải bắt đầu từ chỗ nó đang đứng.
        let _ = g.gain().cancel_scheduled_values(ctx.current_time());
        let _ = g
            .gain()
            .set_value_at_time(g.gain().value(), ctx.current_time());
        if let Err(e) = g.gain().linear_ramp_to_value_at_time(0.0, het) {
            canh_bao(&format!("{:?}", e));
        }
        if let Err(e) = src.stop_with_when(het) {
            canh_bao(&format!("{:?}", e));
        }
    }

    fn phat_sfx(&self, ctx: &AudioContext, id: &str) -> Result<(), AudioError> {
        let src = ctx.create_buffer_source().map_err(loi)?;
        src.set_buffer(Some(&self.buffers[id]));
        src.connect_with_audio_node(&self.buses[&Bus::Sfx])
            .map_err(loi)?;
        src.start().map_err(loi)?;
        Ok(())
    }
}

/// `decodeAudioData` là async; `register` thì không. Nên: nhớ bytes lúc
/// register, giải mã hàng loạt lúc unlock, mỗi lần xong lại `dong_bo`.
fn giai_ma(inner: Rc<RefCell<Inner>>, id: String, bytes: Vec<u8>) {
    let ctx = match inner.borrow().ctx.clone() {
        Some(c) => c,
        None => return,
    };
    wasm_bindgen_futures::spawn_local(async move {
        // Sao chép sang ArrayBuffer của JS: decodeAudioData sẽ detach nó.
        let arr = js_sys::Uint8Array::from(&bytes[..]).buffer();
        let promise = match ctx.decode_audio_data(&arr) {
            Ok(p) => p,
            Err(e) => return canh_bao(&format!("{id}: {e:?}")),
        };
        match JsFuture::from(promise).await {
            Ok(v) => {
                let buf: AudioBuffer = v.unchecked_into();
                let mut i = inner.borrow_mut();
                i.buffers.insert(id, buf);
                i.dong_bo();
            }
            Err(e) => canh_bao(&AudioError::Decode(format!("{id}: {e:?}")).to_string()),
        }
    });
}

impl AudioSink for WebAudio {
    fn register(&mut self, id: &str, bytes: Vec<u8>) -> Result<(), AudioError> {
        let mut i = self.inner.borrow_mut();
        if i.ctx.is_some() {
            drop(i);
            giai_ma(self.inner.clone(), id.to_string(), bytes);
        } else {
            i.raw.insert(id.to_string(), bytes);
        }
        Ok(())
    }

    fn bgm(&mut self, id: Option<&str>) {
        let mut i = self.inner.borrow_mut();
        i.want_bgm = id.map(String::from);
        i.dong_bo();
    }

    fn sfx(&mut self, id: &str) {
        let mut i = self.inner.borrow_mut();
        if !i.raw.contains_key(id) && !i.buffers.contains_key(id) {
            canh_bao(&AudioError::UnknownSound(id.into()).to_string());
            return;
        }
        i.pending_sfx.push(id.to_string());
        i.dong_bo();
    }

    fn set_bus_volume(&mut self, bus: Bus, volume: f32) {
        let v = volume.clamp(0.0, 1.0);
        let mut i = self.inner.borrow_mut();
        i.volumes.insert(bus, v);
        if let Some(g) = i.buses.get(&bus) {
            g.gain().set_value(v);
        }
    }

    fn unlock(&mut self) {
        let raw = {
            let mut i = self.inner.borrow_mut();
            if i.ctx.is_some() {
                return;
            }
            if let Err(e) = i.dung_thiet_bi() {
                canh_bao(&e.to_string());
                return;
            }
            std::mem::take(&mut i.raw)
        };
        for (id, bytes) in raw {
            giai_ma(self.inner.clone(), id, bytes);
        }
    }
}
