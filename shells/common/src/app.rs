//! Vòng lặp winit. Desktop chờ `State::new` bằng `block_on`; web không thể —
//! `request_adapter` là Promise thật, nên State ra đời sau vài frame và sự
//! kiện tới sớm bị bỏ qua.

use crate::state::State;
use mong_project::Loaded;
use mong_runtime::Input;
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

struct App {
    loaded: Option<Loaded>,
    state: Option<State>,
    #[cfg(target_arch = "wasm32")]
    pending: std::rc::Rc<std::cell::RefCell<Option<State>>>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.state.is_some() {
            return; // Android gọi lại resumed; desktop thì không.
        }
        #[cfg(target_arch = "wasm32")]
        if self.pending.borrow().is_some() || self.loaded.is_none() {
            return;
        }

        let window = Arc::new(
            el.create_window(window_attrs())
                .expect("khong tao duoc cua so"),
        );
        let loaded = self.loaded.take().expect("chi khoi tao mot lan");

        #[cfg(not(target_arch = "wasm32"))]
        {
            self.state = Some(pollster::block_on(State::new(window, loaded)));
        }
        #[cfg(target_arch = "wasm32")]
        {
            let slot = self.pending.clone();
            // spawn_local, không spawn: `Arc<Window>` trên web không Send.
            wasm_bindgen_futures::spawn_local(async move {
                *slot.borrow_mut() = Some(State::new(window, loaded).await);
            });
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        #[cfg(target_arch = "wasm32")]
        if self.state.is_none() {
            self.state = self.pending.borrow_mut().take();
        }
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

#[cfg(not(target_arch = "wasm32"))]
fn window_attrs() -> winit::window::WindowAttributes {
    Window::default_attributes()
        .with_title("Mộng Engine")
        .with_inner_size(winit::dpi::LogicalSize::new(1280, 720))
}

/// Gắn vào `<canvas id="mong">` có sẵn. Kích thước canvas do trang lo (CSS +
/// devicePixelRatio); winit quan sát và phát `Resized`.
#[cfg(target_arch = "wasm32")]
fn window_attrs() -> winit::window::WindowAttributes {
    use wasm_bindgen::JsCast;
    use winit::platform::web::WindowAttributesExtWebSys;
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("mong"))
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .expect("trang phai co <canvas id=\"mong\">");
    Window::default_attributes().with_canvas(Some(canvas))
}

pub fn run(loaded: Loaded) {
    let event_loop = EventLoop::new().expect("khong tao duoc event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let app = App {
        loaded: Some(loaded),
        state: None,
        #[cfg(target_arch = "wasm32")]
        pending: Default::default(),
    };

    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut app = app;
        event_loop.run_app(&mut app).expect("event loop hong");
    }
    // spawn_app không trả về: nhường quyền cho vòng lặp sự kiện của trang.
    #[cfg(target_arch = "wasm32")]
    {
        use winit::platform::web::EventLoopExtWebSys;
        event_loop.spawn_app(app);
    }
}
