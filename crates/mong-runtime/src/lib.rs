//! mong-runtime — ghép mong-core (logic) với tầng trình diễn.
//!
//! Core phát `VmEvent`; runtime dịch thành trạng thái sân khấu + hàng đợi
//! lệnh âm thanh, và lo *thời gian* (typewriter, `wait`, transition) mà core
//! cố ý không biết. Core vẫn xác định: runtime chỉ gọi `advance`/`choose`.
//!
//! Runtime không đụng wgpu/kira — shell rút `stage()`, `line()`, `choices()`,
//! `take_audio()` rồi tự vẽ và tự phát.

mod draw;
mod stage;
mod text;
pub mod ui;
pub use draw::{DrawItem, Fit, VIRTUAL_H, VIRTUAL_W};

use mong_assets::Manifest;
use mong_core::{PresentedChoice, SayOpts, Story, Value, VarStore, Vm, VmError, VmEvent, VmStatus};
use mong_i18n::Catalog;
use mong_plugin::{Action, Hook, Host};
use serde_json::json;
use std::collections::BTreeMap;

pub use stage::{Stage, StageChar, Transition, TransitionKind};
pub use text::Typewriter;

/// Hiệu ứng rung do plugin yêu cầu — trình diễn thuần, không vào snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Shake {
    amp: f32,
    total: f32,
    left: f32,
}

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
    /// Số grapheme đã bắn `on_type`. Skip (reveal_all) đặt = total: người
    /// chơi bấm bỏ qua thì không dội một tràng sfx gõ chữ.
    typed_fired: usize,
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

    host: Option<Host>,
    shake: Option<Shake>,
    /// Ngân sách goto dây chuyền trong một lượt input/tick — chặn plugin
    /// on_node_enter goto lẫn nhau vô hạn. Reset ở mỗi entry công khai.
    goto_left: u8,
    jump_gen: u64,
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
            host: None,
            shake: None,
            goto_left: 8,
            jump_gen: 0,
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

    /// Tên hiển thị của nhân vật, đã tra bảng chuỗi. Chưa khai báo → chính id.
    /// `'a` chung cho `self` và `id`: kết quả mượn từ một trong hai tuỳ nhánh.
    pub fn speaker_name<'a>(&'a self, id: &'a str) -> &'a str {
        match self.manifest.characters.get(id) {
            Some(c) if !c.name.is_empty() => self.catalog.text_or_key(&self.locale, &c.name),
            _ => id,
        }
    }

    /// Nhãn của một lựa chọn, đã tra bảng chuỗi.
    pub fn choice_text<'a>(&'a self, arm: &'a PresentedChoice) -> &'a str {
        self.catalog.text_or_key(&self.locale, &arm.text)
    }

    /// Shell rút hàng đợi mỗi frame rồi đẩy xuống mong-audio.
    pub fn take_audio(&mut self) -> Vec<AudioCmd> {
        std::mem::take(&mut self.audio)
    }

    /// Nạp plugin từ Loaded.plugins. Gọi TRƯỚC `start()` — on_game_start
    /// bắn trong start. Lỗi biên dịch từng plugin chỉ vô hiệu plugin đó.
    pub fn set_plugins(&mut self, sources: &BTreeMap<String, String>) {
        if sources.is_empty() {
            self.host = None;
            return;
        }
        self.host = Some(Host::new(sources));
        self.drain_log();
    }

    /// Offset sân khấu do hiệu ứng rung — shell cộng vào toạ độ khi vẽ.
    pub fn shake_offset(&self) -> (f32, f32) {
        let Some(s) = &self.shake else {
            return (0.0, 0.0);
        };
        let t = s.total - s.left;
        let k = s.amp * (s.left / s.total).max(0.0); // decay tuyến tính
        (k * (t * 55.0).sin(), k * (t * 47.0).cos())
    }

    pub fn vars(&self) -> &VarStore {
        self.vm.vars()
    }

    pub fn start(&mut self) -> Result<(), VmError> {
        self.goto_left = 8;
        let evs = self.vm.start()?;
        let gen = self.jump_gen;
        self.fire(Hook::GameStart, json!({}));
        if self.jump_gen == gen {
            self.apply(evs, true);
        }
        Ok(())
    }

    pub fn tick(&mut self, dt: f32) -> Result<(), VmError> {
        self.goto_left = 8;
        self.stage.tick(dt);
        if let Some(s) = &mut self.shake {
            s.left -= dt;
            if s.left <= 0.0 {
                self.shake = None;
            }
        }
        // Gom grapheme mới lộ rồi mới bắn hook — né mượn self hai lần.
        let mut typed: Vec<(usize, String)> = Vec::new();
        let mut total = 0;
        if let Some(l) = &mut self.line {
            let before = l.typed_fired;
            l.tw.tick(dt, self.cps);
            let now = l.tw.shown();
            if now > before && self.host.is_some() {
                use unicode_segmentation::UnicodeSegmentation;
                for (i, g) in l
                    .text
                    .graphemes(true)
                    .enumerate()
                    .skip(before)
                    .take(now - before)
                {
                    typed.push((i, g.to_string()));
                }
            }
            l.typed_fired = now;
            total = l.tw.total();
        }
        for (i, g) in typed {
            self.fire(
                Hook::Type,
                json!({"grapheme": g, "index": i, "total": total}),
            );
        }
        if let Some(left) = &mut self.wait_left {
            *left -= dt;
            if *left <= 0.0 {
                self.wait_left = None;
                self.step_vm()?;
            }
        }
        Ok(())
    }

    pub fn input(&mut self, input: Input) -> Result<(), VmError> {
        self.goto_left = 8;
        match input {
            // Đang gõ chữ: click đầu hiện hết dòng, click sau mới sang dòng mới.
            Input::Advance if self.line.as_ref().is_some_and(|l| !l.tw.done()) => {
                if let Some(l) = &mut self.line {
                    l.tw.reveal_all();
                    l.typed_fired = l.tw.total(); // skip: không bắn on_type dồn
                }
                Ok(())
            }
            Input::Advance => {
                // Bấm tiếp khi đang chờ chọn, hoặc sau khi hết truyện: bình
                // thường, không phải lỗi. `NotAwaitingAdvance` chỉ dành cho
                // lỗi lập trình thật (shell gọi sai lúc VM đang Running).
                if matches!(self.vm.status(), VmStatus::AwaitChoice | VmStatus::Ended) {
                    return Ok(());
                }
                if self.wait_left.is_some() {
                    self.wait_left = None;
                }
                self.step_vm()
            }
            Input::Choose(i) => {
                if self.vm.status() != VmStatus::AwaitChoice {
                    return Err(VmError::NotAwaitingChoice);
                }
                let key = self.choices.get(i).map(|c| c.text.clone());
                self.stage_history.push(self.stage.clone());
                self.choices.clear();
                let evs = self.vm.choose(i)?;
                let gen = self.jump_gen;
                self.fire(Hook::ChoicePicked, json!({"index": i, "key": key}));
                if self.jump_gen == gen {
                    self.apply(evs, true);
                }
                Ok(())
            }
            Input::Rollback => {
                if let Some(evs) = self.vm.rollback() {
                    self.stage = self.stage_history.pop().unwrap_or_default();
                    self.line = None;
                    self.choices.clear();
                    self.wait_left = None;
                    self.apply(evs, false); // replay: hook không bắn (spec-plugin 5.2)
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
        self.apply(evs, true);
        Ok(())
    }

    fn apply(&mut self, evs: Vec<VmEvent>, fresh: bool) {
        let gen = self.jump_gen;
        for e in evs {
            // Hook vừa bắn có thể đã goto — phần còn lại của batch này
            // thuộc về node cũ, áp tiếp là desync trình diễn với VM.
            if self.jump_gen != gen {
                break;
            }
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
                } => self.begin_line(speaker, &text, opts, fresh),
                VmEvent::Choices { arms } => self.choices = arms,
                VmEvent::Wait { ms } => self.wait_left = Some(ms as f32 / 1000.0),
                VmEvent::Sfx { asset } => self.audio.push(AudioCmd::Sfx(asset)),
                VmEvent::Bgm { asset } => self.audio.push(AudioCmd::Bgm(asset)),
                VmEvent::Ext { command, args } => {
                    if !fresh {
                        // Replay: hậu quả lần đầu đã nằm trong state, im lặng.
                    } else if self.host.is_some() {
                        let vars = vars_json(self.vm.vars());
                        let r = self.host.as_mut().unwrap().ext(&command, &args, &vars);
                        self.drain_log();
                        match r {
                            Some(acts) => self.run_actions(acts),
                            None => {
                                eprintln!("ext '{command}': khong co plugin dang ky, bo qua")
                            }
                        }
                    } else {
                        eprintln!("ext '{command}': khong co plugin dang ky, bo qua");
                    }
                }
                VmEvent::NodeEntered { node } => {
                    if fresh {
                        self.fire(Hook::NodeEnter, json!({"node": node}));
                    }
                }
                VmEvent::Ended => {
                    if fresh {
                        self.fire(Hook::GameEnd, json!({}));
                    }
                }
            }
        }
    }

    /// `say` mang cả dữ liệu sân khấu: pose/pos đưa người nói lên nếu chưa có.
    fn begin_line(&mut self, speaker: Option<String>, key: &str, opts: SayOpts, fresh: bool) {
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
        let looked = self.catalog.text_or_key(&self.locale, key).to_string();
        // Filter chạy cả khi replay — nó là một phần của "dựng dòng thoại",
        // và phải thuần túy nên tái lập đúng (spec-plugin mục 2, 5.3).
        let text = if self.host.is_some() {
            let vars = vars_json(self.vm.vars());
            let t =
                self.host
                    .as_mut()
                    .unwrap()
                    .filter_text(speaker.as_deref(), key, &looked, &vars);
            self.drain_log();
            t
        } else {
            looked
        };
        let mut tw = Typewriter::new(&text);
        if self.cps <= 0.0 {
            tw.reveal_all(); // cps ≤ 0 nghĩa là "hiện tức thì"
        }
        let typed_fired = tw.shown();
        self.line = Some(Line {
            speaker: speaker.clone(),
            tw,
            text: text.clone(),
            exit: opts.exit,
            typed_fired,
        });
        if fresh {
            self.fire(
                Hook::LineShow,
                json!({"speaker": speaker, "key": key, "text": text}),
            );
        }
    }

    fn fire(&mut self, hook: Hook, payload: serde_json::Value) {
        if self.host.is_none() {
            return;
        }
        let vars = vars_json(self.vm.vars());
        let acts = self.host.as_mut().unwrap().fire(hook, &payload, &vars);
        self.drain_log();
        self.run_actions(acts);
    }

    fn drain_log(&mut self) {
        if let Some(h) = &mut self.host {
            for l in h.take_log() {
                eprintln!("{l}");
            }
        }
    }

    /// Áp action plugin. `goto`: cái cuối trong batch thắng, áp ngay (VM
    /// đang ở điểm dừng), có ngân sách chống dây chuyền vô hạn.
    fn run_actions(&mut self, acts: Vec<Action>) {
        let mut goto: Option<String> = None;
        for a in acts {
            match a {
                Action::SetVar { name, value } => match from_json(&value) {
                    Some(v) => {
                        if let Err(e) = self.vm.set_var(&name, v) {
                            eprintln!("plugin set_var '{name}': {e}");
                        }
                    }
                    None => {
                        eprintln!("plugin set_var '{name}': kieu khong ho tro (chi Int/Bool/Str)")
                    }
                },
                Action::Goto { node } => goto = Some(node),
                Action::PlaySfx { asset } => self.audio.push(AudioCmd::Sfx(asset)),
                Action::Shake { amplitude, ms } => {
                    let total = ms as f32 / 1000.0;
                    if total > 0.0 && amplitude > 0.0 {
                        self.shake = Some(Shake {
                            amp: amplitude,
                            total,
                            left: total,
                        });
                    }
                }
                Action::SetCps { cps } => self.cps = cps,
            }
        }
        if let Some(node) = goto {
            if self.goto_left == 0 {
                eprintln!("plugin goto '{node}': qua nhieu goto lien tiep, bo qua");
                return;
            }
            self.goto_left -= 1;
            match self.vm.jump_to(&node) {
                Ok(evs) => {
                    self.jump_gen += 1;
                    self.stage_history.push(self.stage.clone());
                    self.line = None;
                    self.choices.clear();
                    self.wait_left = None;
                    self.apply(evs, true);
                }
                Err(e) => eprintln!("plugin goto '{node}': {e} — bo qua"),
            }
        }
    }
}

fn to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::Int(i) => json!(i),
        Value::Bool(b) => json!(b),
        Value::Str(s) => json!(s),
    }
}

fn from_json(v: &serde_json::Value) -> Option<Value> {
    match v {
        serde_json::Value::Number(n) => n.as_i64().map(Value::Int),
        serde_json::Value::Bool(b) => Some(Value::Bool(*b)),
        serde_json::Value::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

fn vars_json(vars: &VarStore) -> BTreeMap<String, serde_json::Value> {
    vars.iter()
        .map(|(k, v)| (k.to_string(), to_json(v)))
        .collect()
}
