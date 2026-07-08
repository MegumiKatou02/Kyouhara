//! Tập lệnh trung gian (IR) — hợp đồng chung giữa editor, DSL và runtime.
//! Xem docs/spec-ir.md cho ngữ nghĩa chi tiết từng lệnh.

use serde::{Deserialize, Serialize};

/// Khoá trỏ vào bảng chuỗi theo locale (mong-i18n giải quyết ở tầng trên).
pub type StringKey = String;
/// Định danh phân đoạn.
pub type NodeId = String;

/// Giá trị biến của cốt truyện.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Value {
    Int(i64),
    Bool(bool),
    Str(String),
}

/// Toán tử so sánh trong điều kiện.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CondOp {
    Ge,
    Le,
    Eq,
    Ne,
}

/// Điều kiện `var <op> value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cond {
    pub var: String,
    pub op: CondOp,
    pub value: Value,
}

/// Toán tử ghi biến.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SetOp {
    Assign,
    Add,
    Sub,
    Toggle,
}

/// Phép ghi biến `var <op>= value`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub var: String,
    pub op: SetOp,
    pub value: Value,
}

/// Vị trí trên sân khấu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StagePos {
    Left,
    Center,
    Right,
}

/// Tuỳ chọn trình diễn kèm một dòng thoại.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SayOpts {
    #[serde(default)]
    pub pose: Option<String>,
    #[serde(default)]
    pub pos: Option<StagePos>,
    #[serde(default)]
    pub sfx: Option<String>,
    #[serde(default)]
    pub exit: bool,
}

/// Một lựa chọn trong lệnh `choice`. `target = None` nghĩa là kết thúc truyện.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChoiceArm {
    pub text: StringKey,
    #[serde(default)]
    pub target: Option<NodeId>,
    #[serde(default)]
    pub cond: Option<Cond>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

/// Phiên bản IR hiện hành. VM/loader nhận mọi `format_version <= FORMAT_VERSION`
/// trong cùng major; v1 là superset thuần của v0 (migration 0→1 là no-op).
pub const FORMAT_VERSION: u32 = 1;

/// Toán tử hai ngôi trong biểu thức (v1). Số học Int-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

/// Biểu thức của `set_expr` (v1) — AST có cấu trúc ngay trong IR, không phải
/// chuỗi text: cú pháp text là việc của DSL (M2), core không parse gì.
/// Externally-tagged có chủ đích: untagged sẽ không phân biệt được
/// `"abc"` là Lit(Str) hay Var.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Expr {
    Lit(Value),
    Var(String),
    Neg(Box<Expr>),
    Bin {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
}

/// Tập lệnh IR v0. Thêm lệnh mới = thêm variant + tăng formatVersion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Instr {
    Say {
        #[serde(default)]
        speaker: Option<String>,
        text: StringKey,
        #[serde(default)]
        opts: SayOpts,
    },
    Show {
        character: String,
        #[serde(default)]
        pose: Option<String>,
        pos: StagePos,
    },
    Hide {
        character: String,
    },
    Scene {
        scene: String,
        #[serde(default)]
        transition: Option<String>,
    },
    Choice {
        arms: Vec<ChoiceArm>,
    },
    Jump {
        target: NodeId,
    },
    Call {
        target: NodeId,
    },
    Return,
    Set {
        effect: Effect,
    },
    If {
        cond: Cond,
        then_branch: Vec<Instr>,
        #[serde(default)]
        else_branch: Vec<Instr>,
    },
    Wait {
        ms: u32,
    },
    Sfx {
        asset: String,
    },
    Bgm {
        #[serde(default)]
        asset: Option<String>,
    },
    Ext {
        command: String,
        #[serde(default)]
        args: serde_json::Value,
    },
    Rand {
        var: String,
        min: i64,
        max: i64,
    },
    Label {
        name: String,
    },
    Goto {
        label: String,
    },
    SetExpr {
        var: String,
        expr: Expr,
    },
    End,
}
