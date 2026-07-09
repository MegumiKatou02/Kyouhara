//! Cây cú pháp MộngScript — giữ đủ thông tin bề mặt (comment, dòng trống,
//! vị trí) để formatter (bước 3 của M2) in lại được; IR thì không cần chúng.

use mong_core::{Cond, Effect, Expr, SayOpts, StagePos, Value};

/// Vị trí 1-based trong file nguồn, cho thông báo lỗi.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub line: usize,
    pub col: usize,
}

/// Một file `.mongscript` đã parse. Directive cấp file theo spec mục 11.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScriptFile {
    /// `@story` — title của Story.
    pub story_title: Option<String>,
    /// `@locale a b c` — phần tử đầu là defaultLocale.
    pub locales: Vec<String>,
    /// `@var x = <literal>` theo thứ tự khai báo.
    pub vars: Vec<(String, Value)>,
    /// `@start` — vắng thì lấy node đầu tiên.
    pub start: Option<String>,
    pub nodes: Vec<NodeAst>,
    /// Comment/dòng trống trước node đầu tiên — giữ cho formatter.
    pub leading: Vec<StmtAst>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeAst {
    pub id: String,
    pub title: Option<String>,
    pub scene: Option<String>,
    pub body: Vec<StmtAst>,
    pub pos: Pos,
}

/// Một dòng trong thân node, kèm phần đuôi (key `#~`, comment `#`).
#[derive(Debug, Clone, PartialEq)]
pub struct StmtAst {
    pub kind: StmtKind,
    /// Key bảng chuỗi — chỉ có nghĩa với `Say`/`ChoiceArm`. `None` trên
    /// dòng dịch được nghĩa là "chưa sinh"; `compile` sẽ điền vào đây.
    pub key: Option<String>,
    /// Comment đuôi dòng, không gồm dấu `#`.
    pub comment: Option<String>,
    pub pos: Pos,
}

/// Toán tử của `~ var <op> expr` (toggle tách riêng vì không có vế phải).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    Add,
    Sub,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StmtKind {
    /// Comment nguyên dòng (không gồm `#`).
    Comment(String),
    /// Dòng trống — có ngữ nghĩa: cắt nhóm `>` (spec 3.2).
    Blank,
    Say {
        /// `None` = dẫn truyện (`*`).
        speaker: Option<String>,
        opts: SayOpts,
        /// Văn bản đã unescape — sẽ vào bảng chuỗi, không vào IR.
        text: String,
    },
    ChoiceArm {
        text: String,
        /// `None` = kết thúc truyện (spec 3.2, quyết định 4).
        target: Option<String>,
        cond: Option<Cond>,
        effects: Vec<Effect>,
    },
    SetToggle {
        var: String,
    },
    SetAssign {
        var: String,
        op: AssignOp,
        rhs: Expr,
    },
    If {
        cond: Cond,
        then_branch: Vec<StmtAst>,
        else_branch: Vec<StmtAst>,
    },
    Jump {
        target: String,
    },
    Call {
        target: String,
    },
    Return,
    Label {
        name: String,
    },
    Goto {
        label: String,
    },
    End,
    Scene {
        scene: String,
        transition: Option<String>,
    },
    Show {
        character: String,
        pose: Option<String>,
        pos: StagePos,
    },
    Hide {
        character: String,
    },
    Wait {
        ms: u32,
    },
    Sfx {
        asset: String,
    },
    Bgm {
        asset: Option<String>,
    },
    Rand {
        var: String,
        min: i64,
        max: i64,
    },
    Ext {
        command: String,
        args: serde_json::Value,
    },
}

impl StmtKind {
    /// Dòng này có mang văn bản dịch được (tức được phép/cần key) không.
    pub fn carries_text(&self) -> bool {
        matches!(self, StmtKind::Say { .. } | StmtKind::ChoiceArm { .. })
    }
}
