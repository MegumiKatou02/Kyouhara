//! Trạng thái sân khấu — thứ renderer đọc mỗi frame. Thuần dữ liệu,
//! không wgpu, để test được trong terminal.

use mong_assets::Manifest;
use mong_core::StagePos;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionKind {
    Cut,
    Fade,
}

impl TransitionKind {
    /// Tên trong IR (`scene san_thuong fade`); tên lạ coi như `Cut` và
    /// runtime ghi log — mongpack dùng transition tương lai vẫn chạy được.
    fn from_ir(name: Option<&str>) -> Self {
        match name {
            Some("fade") => TransitionKind::Fade,
            _ => TransitionKind::Cut,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    /// Nền cũ, vẽ mờ dần bên dưới nền mới.
    pub from_bg: Option<String>,
    pub elapsed: f32,
    pub duration: f32,
}

impl Transition {
    pub fn progress(&self) -> f32 {
        if self.duration <= 0.0 {
            1.0
        } else {
            (self.elapsed / self.duration).clamp(0.0, 1.0)
        }
    }
    pub fn done(&self) -> bool {
        self.progress() >= 1.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StageChar {
    pub id: String,
    pub pose: Option<String>,
    pub pos: StagePos,
    /// Nhân vật không nói được làm tối (hành vi đã kiểm chứng ở prototype).
    pub dim: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Stage {
    pub scene: Option<String>,
    /// Asset id của nền hiện tại (đã tra manifest).
    pub bg: Option<String>,
    pub chars: Vec<StageChar>,
    pub transition: Option<Transition>,
}

/// Thời lượng fade mặc định; sẽ đưa vào manifest ở mốc sau.
const FADE_SECS: f32 = 0.4;

impl Stage {
    /// `scene` dọn sạch sân khấu (spec-ir, mục "quy ước cho runtime").
    pub fn enter_scene(&mut self, scene: &str, transition: Option<&str>, man: &Manifest) {
        let kind = TransitionKind::from_ir(transition);
        let from_bg = self.bg.take();
        self.chars.clear();
        self.scene = Some(scene.to_string());
        self.bg = man.scenes.get(scene).map(|s| s.bg.clone());
        self.transition = match kind {
            TransitionKind::Cut => None,
            TransitionKind::Fade => Some(Transition {
                kind,
                from_bg,
                elapsed: 0.0,
                duration: FADE_SECS,
            }),
        };
    }

    pub fn show(&mut self, id: &str, pose: Option<String>, pos: StagePos) {
        match self.chars.iter_mut().find(|c| c.id == id) {
            Some(c) => {
                if pose.is_some() {
                    c.pose = pose;
                }
                c.pos = pos;
            }
            None => self.chars.push(StageChar {
                id: id.to_string(),
                pose,
                pos,
                dim: false,
            }),
        }
    }

    pub fn hide(&mut self, id: &str) {
        self.chars.retain(|c| c.id != id);
    }

    /// Chỉ người đang nói sáng. `None` (dẫn truyện) → không ai bị làm tối.
    pub fn focus(&mut self, speaker: Option<&str>) {
        for c in &mut self.chars {
            c.dim = speaker.is_some_and(|s| s != c.id);
        }
    }

    pub fn tick(&mut self, dt: f32) {
        if let Some(t) = &mut self.transition {
            t.elapsed += dt;
            if t.done() {
                self.transition = None;
            } // Trạng thái sân khấu — thứ renderer đọc mỗi frame. Thuần dữ liệu,
              // không wgpu, để test được trong terminal.

            use mong_assets::Manifest;
            use mong_core::StagePos;

            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            pub enum TransitionKind {
                Cut,
                Fade,
            }

            impl TransitionKind {
                /// Tên trong IR (`scene san_thuong fade`); tên lạ coi như `Cut` và
                /// runtime ghi log — mongpack dùng transition tương lai vẫn chạy được.
                fn from_ir(name: Option<&str>) -> Self {
                    match name {
                        Some("fade") => TransitionKind::Fade,
                        _ => TransitionKind::Cut,
                    }
                }
            }

            #[derive(Debug, Clone, PartialEq)]
            pub struct Transition {
                pub kind: TransitionKind,
                /// Nền cũ, vẽ mờ dần bên dưới nền mới.
                pub from_bg: Option<String>,
                pub elapsed: f32,
                pub duration: f32,
            }

            impl Transition {
                pub fn progress(&self) -> f32 {
                    if self.duration <= 0.0 {
                        1.0
                    } else {
                        (self.elapsed / self.duration).clamp(0.0, 1.0)
                    }
                }
                pub fn done(&self) -> bool {
                    self.progress() >= 1.0
                }
            }

            #[derive(Debug, Clone, PartialEq)]
            pub struct StageChar {
                pub id: String,
                pub pose: Option<String>,
                pub pos: StagePos,
                /// Nhân vật không nói được làm tối (hành vi đã kiểm chứng ở prototype).
                pub dim: bool,
            }

            #[derive(Debug, Clone, Default, PartialEq)]
            pub struct Stage {
                pub scene: Option<String>,
                /// Asset id của nền hiện tại (đã tra manifest).
                pub bg: Option<String>,
                pub chars: Vec<StageChar>,
                pub transition: Option<Transition>,
            }

            /// Thời lượng fade mặc định; sẽ đưa vào manifest ở mốc sau.
            const FADE_SECS: f32 = 0.4;

            impl Stage {
                /// `scene` dọn sạch sân khấu (spec-ir, mục "quy ước cho runtime").
                pub fn enter_scene(
                    &mut self,
                    scene: &str,
                    transition: Option<&str>,
                    man: &Manifest,
                ) {
                    let kind = TransitionKind::from_ir(transition);
                    let from_bg = self.bg.take();
                    self.chars.clear();
                    self.scene = Some(scene.to_string());
                    self.bg = man.scenes.get(scene).map(|s| s.bg.clone());
                    self.transition = match kind {
                        TransitionKind::Cut => None,
                        TransitionKind::Fade => Some(Transition {
                            kind,
                            from_bg,
                            elapsed: 0.0,
                            duration: FADE_SECS,
                        }),
                    };
                }

                pub fn show(&mut self, id: &str, pose: Option<String>, pos: StagePos) {
                    match self.chars.iter_mut().find(|c| c.id == id) {
                        Some(c) => {
                            if pose.is_some() {
                                c.pose = pose;
                            }
                            c.pos = pos;
                        }
                        None => self.chars.push(StageChar {
                            id: id.to_string(),
                            pose,
                            pos,
                            dim: false,
                        }),
                    }
                }

                pub fn hide(&mut self, id: &str) {
                    self.chars.retain(|c| c.id != id);
                }

                /// Chỉ người đang nói sáng. `None` (dẫn truyện) → không ai bị làm tối.
                pub fn focus(&mut self, speaker: Option<&str>) {
                    for c in &mut self.chars {
                        c.dim = speaker.is_some_and(|s| s != c.id);
                    }
                }

                pub fn tick(&mut self, dt: f32) {
                    if let Some(t) = &mut self.transition {
                        t.elapsed += dt;
                        if t.done() {
                            self.transition = None;
                        }
                    }
                }
            }
        }
    }
}
