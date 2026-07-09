//! Shell desktop: winit + wgpu + kira. Mỏng theo thiết kế — nó chỉ tạo cửa
//! sổ, đo thời gian, chuyển input, và dịch giữa các kiểu của runtime/render.
//!
//! Chạy: `cargo run -p mong-desktop -- <thu_muc_du_an> [locale]`
//! Trong lúc chơi: click / Space / Enter = tiếp | 1-9 = chọn | Z = lùi.

mod project;
mod ui;

use mong_assets::Manifest;
use mong_audio::{AudioSink, KiraAudio};
use mong_core::VmStatus;
use mong_render::text::{GlyphAtlas, LineSpec, ShapedLine, Shaper};
use mong_render::{letterbox, Fit, Renderer, Sprite, TextureId};
use mong_runtime::{AudioCmd, Input, Runtime, VIRTUAL_H, VIRTUAL_W};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

fn main() {
    let mut args = std::env::args().skip(1);
    let Some(dir) = args.next() else {
        eprintln!("cach dung: mong-desktop <thu_muc_du_an> [locale]");
        std::process::exit(2);
    };
    let locale = args.next();

    let loaded = match project::load(&dir, locale.as_deref()) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("loi: {e}");
            std::process::exit(1);
        }
    };

    let event_loop = EventLoop::new().expect("khong tao duoc event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        loaded: Some(loaded),
        state: None,
    };
    event_loop.run_app(&mut app).expect("event loop hong");
}

struct App {
    loaded: Option<project::Loaded>,
    state: Option<State>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // Android gọi lại resumed; desktop thì không.
        }
        let attrs = Window::default_attributes()
            .with_title("Mộng Engine")
            .with_inner_size(winit::dpi::LogicalSize::new(1280, 720));
        let window = Arc::new(el.create_window(attrs).expect("khong tao duoc cua so"));
        let loaded = self.loaded.take().expect("chi khoi tao mot lan");
        self.state = Some(pollster::block_on(State::new(window, loaded)));
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(st) = &mut self.state else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => st.resize(size.width, size.height),
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => st.input(Input::Advance),
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                match event.logical_key.as_ref() {
                    Key::Named(NamedKey::Space) | Key::Named(NamedKey::Enter) => {
                        st.input(Input::Advance)
                    }
                    Key::Named(NamedKey::Escape) => el.exit(),
                    Key::Character("z") | Key::Character("Z") => st.input(Input::Rollback),
                    Key::Character(c) => {
                        if let Some(n) = c.chars().next().and_then(|c| c.to_digit(10)) {
                            if n >= 1 {
                                st.input(Input::Choose(n as usize - 1));
                            }
                        }
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                st.frame();
                st.window.request_redraw();
            }
            _ => {}
        }
    }
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    renderer: Renderer,
    shaper: Shaper,
    atlas: GlyphAtlas,
    rt: Runtime,
    audio: KiraAudio,
    /// asset id → texture đã nạp.
    textures: HashMap<String, TextureId>,
    /// 1×1 trắng: hộp thoại là một quad tô màu bằng tint.
    white: TextureId,
    family: String,
    /// Shape một lần mỗi chuỗi. Shape lại mỗi frame làm chữ nhảy chỗ khi
    /// xuống dòng (xem ghi chú ở mong-render::text::shape).
    shaped: HashMap<String, ShapedLine>,
    last: Instant,

    missing: HashSet<String>,
}

impl State {
    async fn new(window: Arc<Window>, loaded: project::Loaded) -> Self {
        let instance = wgpu::Instance::default();
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
            .expect("khong tim thay GPU");

        let caps = surface.get_capabilities(&adapter);
        // sRGB bắt buộc: tint tính trong không gian tuyến tính.
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

        // Ảnh: nạp hết lúc khởi động. Demo vài MB; streaming là việc của M4+.
        let mut textures = HashMap::new();
        for (id, path) in loaded.images() {
            match project::load_png(&path) {
                Ok((rgba, w, h)) => match renderer.upload(&rgba, w, h) {
                    Ok(t) => {
                        textures.insert(id, t);
                    }
                    Err(e) => eprintln!("{id}: {e}"),
                },
                Err(e) => eprintln!("{id}: {e}"),
            }
        }
        let white = renderer
            .upload(&[255, 255, 255, 255], 1, 1)
            .expect("1x1 luon nap duoc");

        let mut shaper = Shaper::new();
        let family = loaded
            .fonts()
            .into_iter()
            .filter_map(|p| std::fs::read(p).ok())
            .flat_map(|b| shaper.add_font(b))
            .next()
            .expect("du an phai khai bao it nhat mot font");
        let atlas = renderer.create_glyph_atlas();

        let mut audio = KiraAudio::new().expect("khong mo duoc thiet bi am thanh");
        for (id, path) in loaded.sounds() {
            match std::fs::read(&path) {
                Ok(b) => {
                    if let Err(e) = audio.register(&id, b) {
                        eprintln!("{e}");
                    }
                }
                Err(e) => eprintln!("{id}: {e}"),
            }
        }
        audio.unlock(); // desktop không cần chờ cử chỉ người dùng

        let mut rt = Runtime::new(
            loaded.story,
            loaded.catalog,
            loaded.manifest,
            loaded.locale.clone(),
        )
        .expect("cot truyen hong");
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
            textures,
            white,
            family,
            shaped: HashMap::new(),
            last: Instant::now(),
            missing: HashSet::new(),
        }
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(self.renderer.device(), &self.config);
    }

    fn input(&mut self, input: Input) {
        // Chọn sai chỉ số / bấm khi không chờ = bỏ qua, không sập game.
        if let Err(e) = self.rt.input(input) {
            eprintln!("input bo qua: {e}");
        }
    }

    fn spec(&self, size: f32, max_w: f32) -> LineSpec {
        LineSpec {
            font_size: size,
            line_height: ui::LINE_H,
            max_width: max_w,
            family: self.family.clone(),
        }
    }

    /// Shape một lần rồi giữ. Khoá là chính văn bản — số dòng thoại hữu hạn.
    fn shape(&mut self, text: &str, spec: &LineSpec) -> &ShapedLine {
        // Khoá gộp cả cỡ chữ: cùng chuỗi ở hai cỡ là hai kết quả.
        let key = format!("{}|{}|{}", spec.font_size, spec.max_width, text);
        if !self.shaped.contains_key(&key) {
            let line = self.shaper.shape(text, spec);
            self.shaped.insert(key.clone(), line);
        }
        &self.shaped[&key]
    }

    fn frame(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f32().min(0.1); // kẹp khi cửa sổ bị treo
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
        self.renderer.draw(&view, vp, &sprites);
        frame.present();

        if self.rt.status() == VmStatus::Ended {
            self.window.set_title("Mộng Engine — hết truyện");
        }
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
