//! mong-runtime — ghép mong-core (logic) với tầng trình diễn.
//!
//! Core phát `VmEvent`; runtime dịch thành trạng thái sân khấu + hàng đợi
//! lệnh âm thanh, và lo *thời gian* (typewriter, `wait`, transition) mà core
//! cố ý không biết. Core vẫn xác định: runtime chỉ gọi `advance`/`choose`.
//!
//! Runtime không đụng wgpu/kira — shell rút `stage()`, `line()`, `choices()`,
//! `take_audio()` rồi tự vẽ và tự phát.

mod stage;
mod text;

use mong_assets::Manifest;
use mong_core::{PresentedChoice, SayOpts, Story, Vm, VmError, VmEvent, VmStatus};
use mong_i18n::Catalog;

pub use stage::{Stage, StageChar, Transition, TransitionKind};
pub use text::Typewriter;

/// Lệnh gửi xuống mong-audio. Runtime không biết kira là gì.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioCmd {
    /// `None` = tắt nhạc (spec-ir, `bgm{asset: None}`).
    Bgm(Option<String>),
    Sfx(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Advance,
    Choose(usize),
    Rollback,
}

/// Dòng thoại đang hiển thị, văn bản đã tra bảng chuỗi theo locale.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub speaker: Option<String>,
    pub text: String,
    pub tw: Typewriter,
    /// `opts.exit`: giấu người nói khi dòng này bị bỏ qua.
    exit: bool,
}

impl Line {
    pub fn visible(&self) -> &str {
        self.tw.visible(&self.text)
    }
}

/// Tốc độ gõ chữ mặc định (grapheme/giây); cấu hình ở mốc cài đặt người chơi.
const DEFAULT_CPS: f32 = 45.0;

pub struct Runtime {
    vm: Vm,
    catalog: Catalog,
    manifest: Manifest,
    locale: String,
    stage: Stage,
    /// Gương của ring snapshot trong VM: core không lưu sân khấu (xem
    /// ghi chú M3 trong docs), nên runtime tự đẩy/rút 1:1 mỗi lần VM dừng.
    stage_history: Vec<Stage>,
    line: Option<Line>,
    choices: Vec<PresentedChoice>,
    audio: Vec<AudioCmd>,
    wait_left: Option<f32>,
    cps: f32,
}

impl Runtime {
    pub fn new(
        story: Story,
        catalog: Catalog,
        manifest: Manifest,
        locale: impl Into<String>,
    ) -> Result<Self, VmError> {
        Ok(Runtime {
            vm: Vm::new(story)?,
            catalog,
            manifest,
            locale: locale.into(),
            stage: Stage::default(),
            stage_history: Vec::new(),
            line: None,
            choices: Vec::new(),
            audio: Vec::new(),
            wait_left: None,
            cps: DEFAULT_CPS,
        })
    }

    pub fn set_cps(&mut self, cps: f32) {
        self.cps = cps;
    }

    pub fn stage(&self) -> &Stage {
        &self.stage
    }
    pub fn line(&self) -> Option<&Line> {
        self.line.as_ref()
    }
    pub fn choices(&self) -> &[PresentedChoice] {
        &self.choices
    }
    pub fn status(&self) -> VmStatus {
        self.vm.status()
    }
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Shell rút hàng đợi mỗi frame rồi đẩy xuống mong-audio.
    pub fn take_audio(&mut self) -> Vec<AudioCmd> {
        std::mem::take(&mut self.audio)
    }

    pub fn start(&mut self) -> Result<(), VmError> {
        let evs = self.vm.start()?;
        self.apply(evs);
        Ok(())
    }

    /// `dt` do shell đo — core không bao giờ thấy nó.
    pub fn tick(&mut self, dt: f32) -> Result<(), VmError> {
        self.stage.tick(dt);
        if let Some(l) = &mut self.line {
            l.tw.tick(dt, self.cps);
        }
        if let Some(left) = &mut self.wait_left {
            *left -= dt;
            if *left <= 0.0 {
                self.wait_left = None;
                self.step_vm()?; // `wait` hết giờ = advance (spec-ir)
            }
        }
        Ok(())
    }

    pub fn input(&mut self, input: Input) -> Result<(), VmError> {
        match input {
            // Đang gõ chữ: click đầu hiện hết dòng, click sau mới sang dòng mới.
            Input::Advance if self.line.as_ref().is_some_and(|l| !l.tw.done()) => {
                if let Some(l) = &mut self.line {
                    l.tw.reveal_all();
                }
                Ok(())
            }
            Input::Advance => {
                if self.wait_left.is_some() {
                    self.wait_left = None; // bấm để bỏ qua `wait`
                }
                self.step_vm()
            }
            Input::Choose(i) => {
                if self.vm.status() != VmStatus::AwaitChoice {
                    return Err(VmError::NotAwaitingChoice);
                }
                self.stage_history.push(self.stage.clone());
                self.choices.clear();
                let evs = self.vm.choose(i)?;
                self.apply(evs);
                Ok(())
            }
            Input::Rollback => {
                if let Some(evs) = self.vm.rollback() {
                    self.stage = self.stage_history.pop().unwrap_or_default();
                    self.line = None;
                    self.choices.clear();
                    self.wait_left = None;
                    self.apply(evs);
                }
                Ok(())
            }
        }
    }

    fn step_vm(&mut self) -> Result<(), VmError> {
        if self.vm.status() != VmStatus::AwaitAdvance {
            return Err(VmError::NotAwaitingAdvance);
        }
        // Chụp trước khi áp `exit`: snapshot của VM ứng với lúc dòng thoại
        // còn hiển thị, sân khấu phải khớp đúng thời điểm đó.
        self.stage_history.push(self.stage.clone());
        if let Some(l) = &self.line {
            if l.exit {
                if let Some(s) = &l.speaker {
                    let s = s.clone();
                    self.stage.hide(&s);
                }
            }
        }
        self.line = None;
        let evs = self.vm.advance()?;
        self.apply(evs);
        Ok(())
    }

    fn apply(&mut self, evs: Vec<VmEvent>) {
        for e in evs {
            match e {
                VmEvent::SceneChanged { scene, transition } => {
                    self.stage
                        .enter_scene(&scene, transition.as_deref(), &self.manifest);
                    // BGM khai báo của cảnh; lệnh `bgm` sau đó ghi đè.
                    if let Some(b) = self.manifest.scenes.get(&scene).and_then(|s| s.bgm.clone()) {
                        self.audio.push(AudioCmd::Bgm(Some(b)));
                    }
                }
                VmEvent::Show {
                    character,
                    pose,
                    pos,
                } => self.stage.show(&character, pose, pos),
                VmEvent::Hide { character } => self.stage.hide(&character),
                VmEvent::Say {
                    speaker,
                    text,
                    opts,
                } => self.begin_line(speaker, &text, opts),
                VmEvent::Choices { arms } => self.choices = arms,
                VmEvent::Wait { ms } => self.wait_left = Some(ms as f32 / 1000.0),
                VmEvent::Sfx { asset } => self.audio.push(AudioCmd::Sfx(asset)),
                VmEvent::Bgm { asset } => self.audio.push(AudioCmd::Bgm(asset)),
                VmEvent::Ext { command, .. } => {
                    // Không ai xử lý = bỏ qua, không phải lỗi cứng (spec-ir).
                    eprintln!("ext '{command}': khong co plugin dang ky, bo qua");
                }
                VmEvent::NodeEntered { .. } | VmEvent::Ended => {}
            }
        }
    }

    /// `say` mang cả dữ liệu sân khấu: pose/pos đưa người nói lên nếu chưa có.
    fn begin_line(&mut self, speaker: Option<String>, key: &str, opts: SayOpts) {
        if let Some(id) = &speaker {
            if opts.pose.is_some() || opts.pos.is_some() {
                let pos = opts.pos.unwrap_or(mong_core::StagePos::Center);
                self.stage.show(id, opts.pose.clone(), pos);
            }
        }
        self.stage.focus(speaker.as_deref());
        if let Some(sfx) = opts.sfx {
            self.audio.push(AudioCmd::Sfx(sfx));
        }
        let text = self.catalog.text_or_key(&self.locale, key).to_string();
        let tw = Typewriter::new(&text);
        self.line = Some(Line {
            speaker,
            tw,
            text,
            exit: opts.exit,
        });
    }
}
