//! mong-plugin — host plugin nội dung (tầng một, mục 7 tài liệu thiết kế).
//!
//! Hợp đồng ở `docs/spec-plugin.md`. Nguyên tắc: mọi payload/giá trị là
//! serde value; rhai là chi tiết hiện thực v1, không rò ra API công khai
//! (`Hook`, `Action`, chữ ký của `Host` không nhắc tới rhai).
//!
//! Crate này không phụ thuộc mong-runtime — runtime gọi xuống, đúng chiều
//! `shells → runtime → lõi`. Host không giữ VM: biến được đưa vào từng lần
//! gọi (mirror), action trả về để runtime áp — plugin không có state riêng
//! ngoài biến của VM (spec-plugin mục 5).

use rhai::serde::{from_dynamic, to_dynamic};
use rhai::{Dynamic, Engine, Scope, AST};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// Bảng hook (spec-plugin mục 2). Tên hàm rhai trùng tên hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    GameStart,
    NodeEnter,
    LineShow,
    Type,
    ChoicePicked,
    GameEnd,
}

impl Hook {
    pub fn fn_name(self) -> &'static str {
        match self {
            Hook::GameStart => "on_game_start",
            Hook::NodeEnter => "on_node_enter",
            Hook::LineShow => "on_line_show",
            Hook::Type => "on_type",
            Hook::ChoicePicked => "on_choice_picked",
            Hook::GameEnd => "on_game_end",
        }
    }

    fn class(self) -> Class {
        match self {
            // on_type bắn theo nhịp gõ chữ (phụ thuộc dt của shell) — cho nó
            // ghi biến/goto là phá tính xác định của core (spec-plugin mục 3).
            Hook::Type => Class::Presentation,
            _ => Class::Logic,
        }
    }
}

/// Action plugin yêu cầu; runtime áp. Giá trị là serde value — chuyển sang
/// kiểu của mong-core là việc của runtime.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    SetVar { name: String, value: Value },
    Goto { node: String },
    PlaySfx { asset: String },
    Shake { amplitude: f32, ms: u32 },
    SetCps { cps: f32 },
}

/// Lớp quyền của ngữ cảnh đang chạy. Thứ tự có nghĩa: action đòi lớp tối
/// thiểu, ngữ cảnh thấp hơn thì action bị bỏ qua kèm log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Filter,
    Presentation,
    Logic,
}

/// Trạng thái chia sẻ giữa host và các hàm ctx đã đăng ký vào engine.
struct Shared {
    /// Mirror biến của VM tại thời điểm gọi. `set_var` ghi cả vào đây để
    /// `get_var` ngay sau đó trong cùng hook thấy giá trị mới.
    vars: BTreeMap<String, Value>,
    actions: Vec<Action>,
    class: Class,
    /// Plugin id đang chạy — chỉ để log có địa chỉ.
    current: String,
    log: Vec<String>,
}

impl Shared {
    fn new() -> Self {
        Shared {
            vars: BTreeMap::new(),
            actions: Vec::new(),
            class: Class::Logic,
            current: String::new(),
            log: Vec::new(),
        }
    }

    fn log(&mut self, msg: String) {
        self.log.push(msg);
    }

    /// Kiểm quyền; không đủ thì log và trả false — action bị nuốt, không lỗi.
    fn allowed(&mut self, action: &str, need: Class) -> bool {
        if self.class >= need {
            return true;
        }
        let cur = self.current.clone();
        self.log(format!(
            "plugin '{cur}': {action} khong duoc phep trong ngu canh nay — bo qua"
        ));
        false
    }
}

struct Plugin {
    id: String,
    ast: AST,
    /// Tên hàm top-level → số tham số. Quét một lần lúc nạp; hook không khai
    /// báo thì không gọi (rẻ hơn bắt lỗi "không tìm thấy hàm" mỗi lần).
    fns: BTreeMap<String, usize>,
}

pub struct Host {
    engine: Engine,
    /// Thứ tự = thứ tự `BTreeMap` của id lúc nạp — thứ tự bắn hook và xâu
    /// chuỗi filter, một phần của tính xác định (spec-plugin mục 1).
    plugins: Vec<Plugin>,
    shared: Rc<RefCell<Shared>>,
}

impl Host {
    /// Nạp toàn bộ plugin. Lỗi biên dịch chỉ vô hiệu plugin đó (spec-plugin
    /// mục 6) — xem `take_log` để biết chuyện gì xảy ra.
    pub fn new(sources: &BTreeMap<String, String>) -> Self {
        let shared = Rc::new(RefCell::new(Shared::new()));
        let mut engine = Engine::new();

        // Sandbox (spec-plugin mục 7): không eval, không đồng hồ, ngân sách
        // phép tính chống vòng lặp vô hạn tác giả viết nhầm.
        engine.disable_symbol("eval");
        engine.disable_symbol("timestamp");
        engine.disable_symbol("sleep");
        engine.set_max_operations(100_000);
        engine.set_max_call_levels(32);
        engine.set_max_string_size(64 * 1024);
        engine.set_max_array_size(4096);
        engine.set_max_map_size(4096);

        register_ctx(&mut engine, &shared);

        let mut plugins = Vec::new();
        for (id, src) in sources {
            match engine.compile(src) {
                Ok(ast) => {
                    let fns = ast
                        .iter_functions()
                        .map(|f| (f.name.to_string(), f.params.len()))
                        .collect();
                    plugins.push(Plugin {
                        id: id.clone(),
                        ast,
                        fns,
                    });
                }
                Err(e) => shared
                    .borrow_mut()
                    .log(format!("plugin '{id}': loi bien dich, vo hieu: {e}")),
            }
        }
        Host {
            engine,
            plugins,
            shared,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    /// Bắn một hook cho mọi plugin có khai báo, trả về action đã gom.
    /// `vars` là biến VM tại thời điểm gọi (runtime chuyển đổi).
    pub fn fire(
        &mut self,
        hook: Hook,
        payload: &Value,
        vars: &BTreeMap<String, Value>,
    ) -> Vec<Action> {
        self.begin(hook.class(), vars);
        let name = hook.fn_name();
        let payload = self.value_to_dynamic(payload);
        for i in 0..self.plugins.len() {
            self.call(i, name, &payload);
        }
        self.take_actions()
    }

    /// Xâu chuỗi `filter_text` qua các plugin theo thứ tự id. Plugin lỗi thì
    /// giữ text của plugin liền trước. Filter phải thuần túy — nó chạy lại
    /// khi rollback dựng lại dòng (spec-plugin mục 5).
    pub fn filter_text(
        &mut self,
        speaker: Option<&str>,
        key: &str,
        text: &str,
        vars: &BTreeMap<String, Value>,
    ) -> String {
        self.begin(Class::Filter, vars);
        let mut cur = text.to_string();
        for i in 0..self.plugins.len() {
            let p = &self.plugins[i];
            let Some(&arity) = p.fns.get("filter_text") else {
                continue;
            };
            let (id, ast) = (p.id.clone(), p.ast.clone());
            if arity != 1 {
                self.shared.borrow_mut().log(format!(
                    "plugin '{id}': filter_text phai nhan dung 1 tham so"
                ));
                continue;
            }
            self.shared.borrow_mut().current = id.clone();
            let payload = self.value_to_dynamic(&serde_json::json!({
                "speaker": speaker, "key": key, "text": cur,
            }));
            match self
                .engine
                .call_fn::<String>(&mut Scope::new(), &ast, "filter_text", (payload,))
            {
                Ok(s) => cur = s,
                Err(e) => self.shared.borrow_mut().log(format!(
                    "plugin '{id}' filter_text: {e} — giu text truoc do"
                )),
            }
        }
        cur
    }

    /// Dispatch một lệnh `ext`. `None` = không plugin nào khai báo
    /// `ext_<command>` — runtime tự log "bỏ qua" theo spec-ir.
    pub fn ext(
        &mut self,
        command: &str,
        args: &Value,
        vars: &BTreeMap<String, Value>,
    ) -> Option<Vec<Action>> {
        let name = format!("ext_{command}");
        if !self.plugins.iter().any(|p| p.fns.contains_key(&name)) {
            return None;
        }
        self.begin(Class::Logic, vars);
        let args = self.value_to_dynamic(args);
        for i in 0..self.plugins.len() {
            self.call(i, &name, &args);
        }
        Some(self.take_actions())
    }

    /// Rút log (lỗi nạp, lỗi hook, action bị chặn). Runtime in ra; editor sau
    /// này hiển thị. Host không tự in — để test khẳng định được nội dung.
    pub fn take_log(&mut self) -> Vec<String> {
        std::mem::take(&mut self.shared.borrow_mut().log)
    }

    // ---- nội bộ ----

    fn begin(&mut self, class: Class, vars: &BTreeMap<String, Value>) {
        let mut sh = self.shared.borrow_mut();
        sh.class = class;
        sh.vars = vars.clone();
        sh.actions.clear();
    }

    fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.shared.borrow_mut().actions)
    }

    fn value_to_dynamic(&mut self, v: &Value) -> Dynamic {
        to_dynamic(v).unwrap_or_else(|e| {
            self.shared
                .borrow_mut()
                .log(format!("payload khong chuyen duoc sang script: {e}"));
            Dynamic::UNIT
        })
    }

    /// Gọi một hàm của plugin `i` nếu có khai báo. Chấp nhận arity 0 (bỏ
    /// payload) hoặc 1; khác đi là lỗi khai báo, log rồi bỏ.
    fn call(&mut self, i: usize, name: &str, payload: &Dynamic) {
        let p = &self.plugins[i];
        let Some(&arity) = p.fns.get(name) else {
            return;
        };
        let (id, ast) = (p.id.clone(), p.ast.clone());
        self.shared.borrow_mut().current = id.clone();
        let result = match arity {
            0 => self
                .engine
                .call_fn::<Dynamic>(&mut Scope::new(), &ast, name, ()),
            1 => self
                .engine
                .call_fn::<Dynamic>(&mut Scope::new(), &ast, name, (payload.clone(),)),
            n => {
                self.shared
                    .borrow_mut()
                    .log(format!("plugin '{id}': {name} nhan {n} tham so, toi da 1"));
                return;
            }
        };
        if let Err(e) = result {
            // Cô lập lỗi: log, bỏ kết quả hook này, game chạy tiếp.
            self.shared
                .borrow_mut()
                .log(format!("plugin '{id}' hook {name}: {e}"));
        }
    }
}

/// Đăng ký ctx API (spec-plugin mục 3) — hàm host thuần, không method, để
/// backend sau (Lua/WASM) map 1:1.
fn register_ctx(engine: &mut Engine, shared: &Rc<RefCell<Shared>>) {
    let sh = shared.clone();
    engine.register_fn("get_var", move |name: &str| -> Dynamic {
        let sh = sh.borrow();
        match sh.vars.get(name) {
            Some(v) => to_dynamic(v).unwrap_or(Dynamic::UNIT),
            None => Dynamic::UNIT, // biến chưa tồn tại: unit, plugin tự xử
        }
    });

    let sh = shared.clone();
    engine.register_fn("set_var", move |name: &str, value: Dynamic| {
        let mut s = sh.borrow_mut();
        if !s.allowed("set_var", Class::Logic) {
            return;
        }
        match from_dynamic::<Value>(&value) {
            Ok(v) => {
                s.vars.insert(name.to_string(), v.clone());
                s.actions.push(Action::SetVar {
                    name: name.to_string(),
                    value: v,
                });
            }
            Err(e) => {
                let cur = s.current.clone();
                s.log(format!("plugin '{cur}': set_var('{name}'): {e}"));
            }
        }
    });

    let sh = shared.clone();
    engine.register_fn("goto", move |node: &str| {
        let mut s = sh.borrow_mut();
        if s.allowed("goto", Class::Logic) {
            s.actions.push(Action::Goto {
                node: node.to_string(),
            });
        }
    });

    let sh = shared.clone();
    engine.register_fn("play_sfx", move |asset: &str| {
        let mut s = sh.borrow_mut();
        if s.allowed("play_sfx", Class::Presentation) {
            s.actions.push(Action::PlaySfx {
                asset: asset.to_string(),
            });
        }
    });

    // shake nhận cả int lẫn float cho biên độ — rhai literal `4` là i64.
    let sh = shared.clone();
    engine.register_fn("shake", move |amplitude: f64, ms: i64| {
        push_shake(&sh, amplitude, ms);
    });
    let sh = shared.clone();
    engine.register_fn("shake", move |amplitude: i64, ms: i64| {
        push_shake(&sh, amplitude as f64, ms);
    });

    let sh = shared.clone();
    engine.register_fn("set_cps", move |cps: f64| push_cps(&sh, cps));
    let sh = shared.clone();
    engine.register_fn("set_cps", move |cps: i64| push_cps(&sh, cps as f64));

    let sh = shared.clone();
    engine.register_fn("log", move |msg: &str| {
        let mut s = sh.borrow_mut();
        let cur = s.current.clone();
        s.log(format!("plugin '{cur}': {msg}"));
    });
}

fn push_shake(sh: &Rc<RefCell<Shared>>, amplitude: f64, ms: i64) {
    let mut s = sh.borrow_mut();
    if s.allowed("shake", Class::Presentation) {
        s.actions.push(Action::Shake {
            amplitude: amplitude.max(0.0) as f32,
            ms: ms.max(0) as u32,
        });
    }
}

fn push_cps(sh: &Rc<RefCell<Shared>>, cps: f64) {
    let mut s = sh.borrow_mut();
    if s.allowed("set_cps", Class::Presentation) {
        s.actions.push(Action::SetCps {
            cps: cps.max(0.0) as f32,
        });
    }
}
