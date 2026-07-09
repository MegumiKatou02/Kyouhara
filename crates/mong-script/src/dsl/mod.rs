//! Frontend DSL MộngScript (docs/spec-mongscript.md).
//!
//! Đường đi: text → [`parse_dsl`] → [`ScriptFile`] (AST giữ comment/key/vị
//! trí) → [`compile`] → `Story` + bảng chuỗi defaultLocale. Cùng một IR với
//! đường JSON — hai frontend, một máy ảo. Chiều ngược: [`print_story`]
//! (IR → text) và [`format_dsl`] (chuẩn hoá file nguồn, sinh key thiếu).

mod ast;
mod lower;
mod parse;
mod print;

pub use ast::{AssignOp, NodeAst, Pos, ScriptFile, StmtAst, StmtKind};
pub use lower::{compile, generate_keys, load_story_dsl, CompileOutput};
pub use parse::{parse_dsl, DslError};
pub use print::{
    canonicalize_story, format_dsl, print_script, print_story, FormatOutput, PrintError,
};
