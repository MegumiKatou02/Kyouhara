//! Trạng thái một cửa sổ đang chạy: surface, renderer, atlas, runtime, audio.

/// Desktop: để wgpu tự chọn (Vulkan/Metal/DX12).
#[cfg(not(target_arch = "wasm32"))]
const BACKENDS: wgpu::Backends = wgpu::Backends::PRIMARY;

/// Web: **chỉ** WebGL2. Sàn bắt buộc theo mục 8 tài liệu thiết kế, và
/// `Renderer` đã ép `downlevel_webgl2_defaults()` từ M3 nên WebGPU không cho
/// thêm gì. Ngoài ra wgpu 22 gửi limit `maxInterStageShaderComponents` mà
/// WebGPU spec đã bỏ — Chrome ≥ M135 từ chối `requestDevice`. Mở lại đường
/// WebGPU khi nâng wgpu (nợ M4, mục 8).
#[cfg(target_arch = "wasm32")]
const BACKENDS: wgpu::Backends = wgpu::Backends::GL;

use crate::new_audio;
use mong_audio::AudioSink;
use mong_core::VmStatus;
use mong_project::Loaded;
use mong_render::text::{GlyphAtlas, LineSpec, ShapedLine, Shaper};
use mong_render::{decode_png, letterbox, Fit, Renderer, Sprite, TextureId};
use mong_runtime::{ui, AudioCmd, Input, Runtime, VIRTUAL_H, VIRTUAL_W};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use web_time::Instant; // ĐỔI 1: std::time::Instant panic trên wasm
use winit::window::Window;

pub(crate) struct State {
    pub(crate) window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    shaper: Shaper,
    atlas: GlyphAtlas,
    rt: Runtime,
    /// ĐỔI 2: backend do nền tảng chọn, shell không biết là kira hay WebAudio.
    audio: Box<dyn AudioSink>,
    /// ĐỔI 3: web đòi `unlock()` nằm trong ngăn xếp cử chỉ người dùng. Desktop
    /// thì input đầu tiên tới ngay, nên một đường đi cho cả hai.
    unlocked: bool,
    textures: HashMap<String, TextureId>,
    /// 1×1 trắng: hộp thoại là một quad tô màu bằng tint.
    white: TextureId,
    families: Vec<String>,
    /// Shape một lần mỗi chuỗi (xem ghi chú ở mong-render::text::shape).
    shaped: HashMap<String, ShapedLine>,
    last: Instant,
    missing: HashSet<String>,
    /// Web: đã báo `__mong_ready` cho CI chưa (đặt sau frame đầu tiên).
    #[cfg(target_arch = "wasm32")]
    ready_flagged: bool,
}

impl State {
    pub(crate) async fn new(window: Arc<Window>, loaded: Loaded) -> Self {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: BACKENDS,
            ..Default::default()
        });
        let surface = instance
            .create_surface(window.clone())
            .expect("khong tao duoc surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect(
                "khong tim thay GPU adapter — tren Safari/WebGL2 kiem tra \
                         canvas da co kich thuoc va context WebGL2 kha dung",
            );

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or_else(|| {
                eprintln!("canh bao: khong tim thay format sRGB, dung format mac dinh");
                caps.formats[0]
            });

        let mut renderer = Renderer::new(&adapter, format, (VIRTUAL_W, VIRTUAL_H))
            .await
            .expect("khong khoi tao duoc renderer");

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(renderer.device(), &config);

        let mut textures = HashMap::new();
        for (id, bytes) in loaded.images() {
            match decode_png(bytes).and_then(|(rgba, w, h)| renderer.upload(&rgba, w, h)) {
                Ok(t) => {
                    textures.insert(id.to_string(), t);
                }
                Err(e) => eprintln!("{id}: {e}"),
            }
        }
        let white = renderer
            .upload(&[255, 255, 255, 255], 1, 1)
            .expect("1x1 luon nap duoc");

        let mut shaper = Shaper::new();
        let families: Vec<String> = loaded
            .fonts()
            .into_iter()
            .filter_map(|b| shaper.add_font(b.to_vec()).into_iter().next())
            .collect();
        assert!(
            !families.is_empty(),
            "du an phai khai bao it nhat mot font trong manifest.fonts"
        );
        let atlas = renderer.create_glyph_atlas();

        // ĐỔI 4: không `unlock()` ở đây nữa — dời vào `input()`.
        let mut audio = new_audio();
        for (id, bytes) in loaded.sounds() {
            if let Err(e) = audio.register(id, bytes.to_vec()) {
                eprintln!("{e}");
            }
        }

        let catalog = loaded.catalog();
        let mut rt = Runtime::new(
            loaded.story,
            catalog,
            loaded.manifest,
            loaded.locale.clone(),
        )
        .expect("cot truyen hong");

        rt.set_plugins(&loaded.plugins);
        rt.start().expect("khong start duoc");

        State {
            window,
            surface,
            config,
            renderer,
            shaper,
            atlas,
            rt,
            audio,
            unlocked: false,
            textures,
            white,
            families,
            shaped: HashMap::new(),
            last: Instant::now(),
            missing: HashSet::new(),
            #[cfg(target_arch = "wasm32")]
            ready_flagged: false,
        }
    }

    pub(crate) fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(self.renderer.device(), &self.config);
    }

    pub(crate) fn input(&mut self, input: Input) {
        // Cử chỉ đầu tiên mở thiết bị âm thanh. Trên desktop `unlock` là
        // no-op ngoài việc xả hàng đợi; trên web nó dựng `AudioContext`.
        if !self.unlocked {
            self.audio.unlock();
            self.unlocked = true;
        }
        // Chọn sai chỉ số / bấm khi không chờ = bỏ qua, không sập game.
        if let Err(e) = self.rt.input(input) {
            eprintln!("input bo qua: {e}");
        }
    }

    pub(crate) fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1); // kẹp khi tab bị treo
        self.last = now;

        if let Err(e) = self.rt.tick(dt) {
            eprintln!("runtime dung: {e}");
        }
        for cmd in self.rt.take_audio() {
            match cmd {
                AudioCmd::Bgm(id) => self.audio.bgm(id.as_deref()),
                AudioCmd::Sfx(id) => self.audio.sfx(&id),
            }
        }

        let sprites = self.build_sprites();

        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(self.renderer.device(), &self.config);
                return;
            }
            Err(e) => {
                eprintln!("surface: {e}");
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let vp = letterbox(
            (self.config.width, self.config.height),
            (VIRTUAL_W, VIRTUAL_H),
        );
        self.renderer
            .draw(&view, vp, &sprites, self.rt.shake_offset());
        frame.present();

        // Nợ 0 (m4-ket-thuc): cờ CI phải chứng minh "renderer đã vẽ được một
        // frame thật", không phải "wasm nạp xong" — request_adapter fail trên
        // Safari xảy ra SAU khi start() trả về, JS đặt cờ sẽ báo xanh giả.
        #[cfg(target_arch = "wasm32")]
        if !self.ready_flagged {
            self.ready_flagged = true;
            if let Some(w) = web_sys::window() {
                let _ = js_sys::Reflect::set(&w, &"__mong_ready".into(), &true.into());
            }
        }

        if self.rt.status() == VmStatus::Ended {
            self.window.set_title("Mộng Engine — hết truyện");
        }
    }

    fn spec(&self, size: f32, max_w: f32) -> LineSpec {
        LineSpec {
            font_size: size,
            line_height: ui::LINE_H,
            max_width: max_w,
            families: self.families.clone(),
        }
    }

    fn shape(&mut self, text: &str, spec: &LineSpec) -> &ShapedLine {
        let key = format!("{}|{}|{}", spec.font_size, spec.max_width, text);
        if !self.shaped.contains_key(&key) {
            let line = self.shaper.shape(text, spec);
            self.shaped.insert(key.clone(), line);
        }
        &self.shaped[&key]
    }

    fn build_sprites(&mut self) -> Vec<Sprite> {
        let mut out = Vec::new();

        // 1. Sân khấu: nền + nhân vật, bố cục do runtime tính.
        for item in self.rt.stage().draw_list(self.rt.manifest()) {
            let Some(&tex) = self.textures.get(&item.asset) else {
                // Cảnh báo một lần mỗi asset: thiếu texture nghĩa là sân khấu
                // trống mà không ai biết vì sao — bug tệ nhất là bug im lặng.
                if self.missing.insert(item.asset.clone()) {
                    eprintln!("thieu texture '{}': sprite se khong hien", item.asset);
                }
                continue;
            };
            let fit = match item.fit {
                mong_runtime::Fit::Cover => Fit::Cover,
                mong_runtime::Fit::Anchor { x, y } => Fit::Anchor { x, y },
            };
            out.push(Sprite::image(tex, fit, item.tint));
        }

        // 2. Hộp thoại + chữ, chỉ khi có dòng đang hiện.
        if let Some(line) = self.rt.line() {
            let (speaker, text, vis) = (
                line.speaker.clone(),
                line.text.clone(),
                line.tw.visible_bytes(),
            );
            out.push(Sprite {
                texture: self.white,
                fit: Fit::Rect {
                    x: ui::BOX_X,
                    y: ui::BOX_Y,
                    w: ui::BOX_W,
                    h: ui::BOX_H,
                },
                tint: ui::BOX_TINT,
                uv: mong_render::FULL_UV,
                mask: false,
            });

            if let Some(id) = &speaker {
                let color = ui::parse_color(
                    self.rt
                        .manifest()
                        .characters
                        .get(id)
                        .and_then(|c| c.color.as_deref()),
                );
                // Tên nhân vật là key bảng chuỗi; chưa có thì hiện chính id.
                let name = self.rt.speaker_name(id).to_string();
                out.extend(self.text_quads(
                    &name,
                    ui::NAME_SIZE,
                    ui::TEXT_W,
                    ui::NAME_POS,
                    color,
                    usize::MAX,
                ));
            }
            out.extend(self.text_quads(
                &text,
                ui::TEXT_SIZE,
                ui::TEXT_W,
                ui::TEXT_POS,
                ui::TEXT_COLOR,
                vis,
            ));
        }

        // 3. Lựa chọn, đánh số 1..n khớp phím bấm.
        let arms: Vec<String> = self
            .rt
            .choices()
            .iter()
            .enumerate()
            .map(|(i, a)| format!("{}. {}", i + 1, self.rt.choice_text(a)))
            .collect();
        for (i, label) in arms.iter().enumerate() {
            let quads = self.text_quads(
                label,
                ui::TEXT_SIZE,
                ui::TEXT_W,
                ui::choice_pos(i),
                ui::CHOICE_COLOR,
                usize::MAX,
            );
            out.extend(quads);
        }
        out
    }

    /// `visible_bytes = usize::MAX` nghĩa là hiện hết (tên, lựa chọn).
    fn text_quads(
        &mut self,
        text: &str,
        size: f32,
        max_w: f32,
        origin: (f32, f32),
        color: [f32; 4],
        visible_bytes: usize,
    ) -> Vec<Sprite> {
        let spec = self.spec(size, max_w);
        let glyphs: Vec<_> = self
            .shape(text, &spec)
            .visible(visible_bytes)
            .copied()
            .collect();
        self.atlas.quads(
            &mut self.shaper,
            self.renderer.queue(),
            glyphs.into_iter(),
            origin,
            color,
        )
    }
}
