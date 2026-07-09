//! In `.mongscript`: formatter chuẩn hoá (spec-mongscript mục 8) và chiều
//! ngược IR + bảng chuỗi → text (mục 7).
//!
//! Một máy in duy nhất trên AST; hai đường vào:
//! - [`format_dsl`]: text → parse → sinh key thiếu → in lại (giữ comment,
//!   dòng trống — bất biến 2 và 5).
//! - [`print_story`]: `Story` + bảng chuỗi → dựng AST → in (bất biến 1).
//!
//! DSL không phân biệt được vài dạng IR tương đương ngữ nghĩa (xem
//! [`canonicalize_story`]); `print_story` in ra dạng chuẩn tắc, nên
//! `parse(print(ir)) == canonicalize(ir)` — với IR chuẩn tắc là đẳng thức.

use super::ast::*;
use super::lower::generate_keys;
use super::parse::{parse_dsl, DslError};
use mong_core::{
    BinOp, ChoiceArm, Cond, Effect, Expr, Instr, Node, SayOpts, SetOp, StagePos, Story, Value,
};
use std::collections::BTreeMap;

/// Lỗi khi in: IR/bảng chuỗi chứa thứ DSL không biểu diễn được.
#[derive(Debug, Clone, PartialEq)]
pub struct PrintError {
    pub message: String,
}

impl std::fmt::Display for PrintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "khong in duoc DSL: {}", self.message)
    }
}
impl std::error::Error for PrintError {}

fn perr(message: impl Into<String>) -> PrintError {
    PrintError {
        message: message.into(),
    }
}

/// Kết quả format một file nguồn.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatOutput {
    pub text: String,
    /// Số key `#~` vừa sinh (đã nằm trong `text`).
    pub generated_keys: usize,
}

/// Format một file `.mongscript`: chuẩn hoá trình bày + sinh key cho dòng
/// thiếu `#~`. Không đòi hỏi file đầy đủ ngữ nghĩa (thiếu @locale vẫn
/// format được — kiểm tra ngữ nghĩa là việc của `compile`/lint).
pub fn format_dsl(src: &str) -> Result<FormatOutput, DslError> {
    let mut file = parse_dsl(src)?;
    let generated_keys = generate_keys(&mut file);
    let text = print_script(&file).map_err(|e| DslError {
        pos: Pos { line: 1, col: 1 },
        message: e.message,
    })?;
    Ok(FormatOutput {
        text,
        generated_keys,
    })
}

/// In `Story` + bảng chuỗi defaultLocale thành text chuẩn hoá (spec mục 7).
/// Key trong IR không có trong bảng chuỗi → lỗi. Văn bản được trim hai đầu
/// (DSL chỉ chở văn bản một dòng, không giữ khoảng trắng biên).
pub fn print_story(
    story: &Story,
    strings: &BTreeMap<String, String>,
) -> Result<String, PrintError> {
    print_script(&story_to_ast(story, strings)?)
}

/// Đưa IR về dạng chuẩn tắc mà DSL biểu diễn được 1:1 — các biến đổi đều
/// giữ nguyên ngữ nghĩa runtime:
/// - `set_expr {var, Lit(v)}` → `set {assign v}`;
/// - `Effect{toggle, value}` → value chuẩn hoá `Bool(true)` (toggle không
///   đọc value);
/// - title của Story/Node trim hai đầu.
pub fn canonicalize_story(story: &Story) -> Story {
    let mut s = story.clone();
    s.title = s.title.trim().to_string();
    for n in &mut s.nodes {
        n.title = n.title.trim().to_string();
        canon_body(&mut n.body);
    }
    s
}

fn canon_body(body: &mut [Instr]) {
    for i in body.iter_mut() {
        match i {
            Instr::SetExpr {
                var,
                expr: Expr::Lit(v),
            } => {
                *i = Instr::Set {
                    effect: Effect {
                        var: std::mem::take(var),
                        op: SetOp::Assign,
                        value: v.clone(),
                    },
                };
            }
            Instr::Set { effect } if effect.op == SetOp::Toggle => {
                effect.value = Value::Bool(true);
            }
            Instr::Choice { arms } => {
                for a in arms {
                    for e in &mut a.effects {
                        if e.op == SetOp::Toggle {
                            e.value = Value::Bool(true);
                        }
                    }
                }
            }
            Instr::If {
                then_branch,
                else_branch,
                ..
            } => {
                canon_body(then_branch);
                canon_body(else_branch);
            }
            _ => {}
        }
    }
}

// ---- kiểm tra token in được ----

fn is_ident(s: &str) -> bool {
    let mut ch = s.chars();
    matches!(ch.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && ch.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn is_key(s: &str) -> bool {
    let mut ch = s.chars();
    matches!(ch.next(), Some(c) if c.is_ascii_alphanumeric() || c == '_')
        && ch.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn is_locale(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn ck_ident<'a>(s: &'a str, what: &str) -> Result<&'a str, PrintError> {
    if is_ident(s) {
        Ok(s)
    } else {
        Err(perr(format!("{what} '{s}' khong phai dinh danh hop le")))
    }
}

/// Văn bản một dòng (thoại/nhãn/tiêu đề): cấm xuống dòng, trim, cấm rỗng.
fn ck_text<'a>(s: &'a str, what: &str) -> Result<&'a str, PrintError> {
    if s.contains('\n') || s.contains('\r') {
        return Err(perr(format!(
            "{what} chua xuong dong — DSL chi cho van ban mot dong"
        )));
    }
    let t = s.trim();
    if t.is_empty() {
        return Err(perr(format!("{what} rong")));
    }
    Ok(t)
}

// ---- in từ AST ----

/// In một [`ScriptFile`] thành text chuẩn hoá theo spec mục 8.
pub fn print_script(file: &ScriptFile) -> Result<String, PrintError> {
    let mut out = String::new();

    // Trivia đầu file: dồn dòng trống, bỏ trống ở biên.
    let leading = tidy_trivia(&file.leading);
    for s in &leading {
        print_stmt(&mut out, s, 0)?;
    }
    if !leading.is_empty() {
        out.push('\n');
    }

    // Directive cấp file, thứ tự cố định.
    let mut has_dir = false;
    if let Some(t) = &file.story_title {
        if !t.trim().is_empty() {
            out.push_str(&format!("@story {}\n", esc_say(ck_text(t, "@story")?)));
            has_dir = true;
        }
    }
    if !file.locales.is_empty() {
        for l in &file.locales {
            if !is_locale(l) {
                return Err(perr(format!("locale '{l}' khong hop le")));
            }
        }
        out.push_str(&format!("@locale {}\n", file.locales.join(" ")));
        has_dir = true;
    }
    for (name, val) in &file.vars {
        out.push_str(&format!(
            "@var {} = {}\n",
            ck_ident(name, "bien")?,
            fmt_value(val)?
        ));
        has_dir = true;
    }
    if let Some(start) = &file.start {
        out.push_str(&format!("@start {}\n", ck_ident(start, "@start")?));
        has_dir = true;
    }
    if has_dir {
        out.push('\n');
    }

    for (i, n) in file.nodes.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        print_node(&mut out, n)?;
    }

    // Kết thúc đúng một LF; file rỗng hoàn toàn thì thôi.
    while out.ends_with("\n\n") {
        out.pop();
    }
    Ok(out)
}

fn print_node(out: &mut String, n: &NodeAst) -> Result<(), PrintError> {
    out.push_str(&format!("@node {}\n", ck_ident(&n.id, "node")?));
    if let Some(t) = &n.title {
        out.push_str(&format!("@title {}\n", esc_say(ck_text(t, "@title")?)));
    }
    if let Some(s) = &n.scene {
        out.push_str(&format!("@scene {}\n", ck_ident(s, "scene")?));
    }
    let body = tidy_trivia(&n.body);
    if !body.is_empty() {
        out.push('\n');
    }
    for s in &body {
        print_stmt(out, s, 1)?;
    }
    Ok(())
}

/// Dồn chuỗi dòng trống liên tiếp còn một, bỏ dòng trống ở hai biên
/// (quy tắc 8.3 — dòng trống giữa các khối là việc của cấu trúc in).
fn tidy_trivia(body: &[StmtAst]) -> Vec<&StmtAst> {
    let mut out: Vec<&StmtAst> = Vec::with_capacity(body.len());
    for s in body {
        if matches!(s.kind, StmtKind::Blank) {
            let sau_blank_hoac_dau = match out.last() {
                None => true,
                Some(x) => matches!(x.kind, StmtKind::Blank),
            };
            if sau_blank_hoac_dau {
                continue;
            }
        }
        out.push(s);
    }
    while matches!(out.last().map(|s| &s.kind), Some(StmtKind::Blank)) {
        out.pop();
    }
    out
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn print_stmt(out: &mut String, s: &StmtAst, depth: usize) -> Result<(), PrintError> {
    let pad = indent(depth);
    let suffix = stmt_suffix(s)?;
    match &s.kind {
        StmtKind::Blank => out.push('\n'),
        StmtKind::Comment(c) => {
            let c = c.trim_end();
            if c.is_empty() {
                out.push_str(&format!("{pad}#\n"));
            } else {
                out.push_str(&format!("{pad}# {c}\n"));
            }
        }
        StmtKind::Say {
            speaker,
            opts,
            text,
        } => {
            let opts_s = fmt_say_opts(opts)?;
            match speaker {
                None => {
                    // Văn bản mở đầu bằng `(` phải escape để không bị đọc
                    // nhầm thành opts của dòng `*`.
                    let mut text = esc_say(ck_text(text, "dan truyen")?);
                    if text.starts_with('(') {
                        text.insert(0, '\\');
                    }
                    out.push_str(&format!("{pad}*{opts_s} {text}{suffix}\n"));
                }
                Some(sp) => out.push_str(&format!(
                    "{pad}{}{opts_s}: {}{suffix}\n",
                    ck_ident(sp, "nhan vat")?,
                    esc_say(ck_text(text, "thoai")?)
                )),
            }
        }
        StmtKind::ChoiceArm {
            text,
            target,
            cond,
            effects,
        } => {
            let mut line = format!("{pad}> {}", esc_arm(ck_text(text, "nhan lua chon")?));
            if let Some(c) = cond {
                line.push_str(&format!(" [ {} ]", fmt_cond(c)?));
            }
            if let Some(t) = target {
                line.push_str(&format!(" -> {}", ck_ident(t, "dich lua chon")?));
            }
            if !effects.is_empty() {
                let fx: Vec<String> = effects.iter().map(fmt_effect).collect::<Result<_, _>>()?;
                line.push_str(&format!(" {{ {} }}", fx.join("; ")));
            }
            out.push_str(&line);
            out.push_str(&suffix);
            out.push('\n');
        }
        StmtKind::SetToggle { var } => {
            out.push_str(&format!("{pad}~ !{}{suffix}\n", ck_ident(var, "bien")?))
        }
        StmtKind::SetAssign { var, op, rhs } => {
            let var = ck_ident(var, "bien")?;
            let op = match op {
                AssignOp::Assign => "=",
                AssignOp::Add => "+=",
                AssignOp::Sub => "-=",
            };
            out.push_str(&format!("{pad}~ {var} {op} {}{suffix}\n", fmt_expr(rhs)?));
        }
        StmtKind::If {
            cond,
            then_branch,
            else_branch,
        } => {
            out.push_str(&format!("{pad}? {} {{\n", fmt_cond(cond)?));
            for st in tidy_trivia(then_branch) {
                print_stmt(out, st, depth + 1)?;
            }
            let else_body = tidy_trivia(else_branch);
            if else_body.is_empty() {
                out.push_str(&format!("{pad}}}{suffix}\n"));
            } else {
                out.push_str(&format!("{pad}}} : {{\n"));
                for st in else_body {
                    print_stmt(out, st, depth + 1)?;
                }
                out.push_str(&format!("{pad}}}{suffix}\n"));
            }
        }
        StmtKind::Jump { target } => out.push_str(&format!(
            "{pad}jump {}{suffix}\n",
            ck_ident(target, "node")?
        )),
        StmtKind::Call { target } => out.push_str(&format!(
            "{pad}call {}{suffix}\n",
            ck_ident(target, "node")?
        )),
        StmtKind::Return => out.push_str(&format!("{pad}return{suffix}\n")),
        StmtKind::Label { name } => {
            out.push_str(&format!("{pad}label {}{suffix}\n", ck_ident(name, "nhan")?))
        }
        StmtKind::Goto { label } => {
            out.push_str(&format!("{pad}goto {}{suffix}\n", ck_ident(label, "nhan")?))
        }
        StmtKind::End => out.push_str(&format!("{pad}end{suffix}\n")),
        StmtKind::Scene { scene, transition } => {
            let mut line = format!("{pad}scene {}", ck_ident(scene, "scene")?);
            if let Some(t) = transition {
                line.push_str(&format!(" {}", ck_ident(t, "transition")?));
            }
            out.push_str(&line);
            out.push_str(&suffix);
            out.push('\n');
        }
        StmtKind::Show {
            character,
            pose,
            pos,
        } => {
            let mut line = format!("{pad}show {}", ck_ident(character, "nhan vat")?);
            if let Some(p) = pose {
                line.push_str(&format!(" {}", ck_ident(p, "pose")?));
            }
            line.push_str(&format!(" {}", stage_pos_str(*pos)));
            out.push_str(&line);
            out.push_str(&suffix);
            out.push('\n');
        }
        StmtKind::Hide { character } => out.push_str(&format!(
            "{pad}hide {}{suffix}\n",
            ck_ident(character, "nhan vat")?
        )),
        StmtKind::Wait { ms } => out.push_str(&format!("{pad}wait {ms}{suffix}\n")),
        StmtKind::Sfx { asset } => {
            out.push_str(&format!("{pad}sfx {}{suffix}\n", ck_ident(asset, "asset")?))
        }
        StmtKind::Bgm { asset } => match asset {
            Some(a) => out.push_str(&format!("{pad}bgm {}{suffix}\n", ck_ident(a, "asset")?)),
            None => out.push_str(&format!("{pad}bgm{suffix}\n")),
        },
        StmtKind::Rand { var, min, max } => out.push_str(&format!(
            "{pad}rand {} {min} {max}{suffix}\n",
            ck_ident(var, "bien")?
        )),
        StmtKind::Ext { command, args } => {
            // Dòng ext không nhận key/comment (grammar) — suffix chắc chắn rỗng.
            let cmd = ck_ident(command, "lenh ext")?;
            if args.is_null() {
                out.push_str(&format!("{pad}ext {cmd}\n"));
            } else {
                let json = serde_json::to_string(args)
                    .map_err(|e| perr(format!("ext '{cmd}': args khong serialize duoc: {e}")))?;
                out.push_str(&format!("{pad}ext {cmd} {json}\n"));
            }
        }
    }
    Ok(())
}

/// Đuôi dòng: `  #~ key` rồi `  # comment` (mỗi phần cách 2 space — 8.2).
fn stmt_suffix(s: &StmtAst) -> Result<String, PrintError> {
    let mut suf = String::new();
    if let Some(k) = &s.key {
        if !s.kind.carries_text() {
            return Err(perr(format!("key '{k}' gan tren dong khong mang van ban")));
        }
        if !is_key(k) {
            return Err(perr(format!("key '{k}' khong hop le")));
        }
        suf.push_str(&format!("  #~ {k}"));
    }
    if let Some(c) = &s.comment {
        if matches!(s.kind, StmtKind::Ext { .. }) {
            return Err(perr("dong ext khong nhan comment duoi dong".to_string()));
        }
        let c = c.trim_end();
        if c.contains('\n') || c.contains('\r') {
            return Err(perr("comment chua xuong dong".to_string()));
        }
        if c.is_empty() {
            suf.push_str("  #");
        } else {
            suf.push_str(&format!("  # {c}"));
        }
    }
    Ok(suf)
}

fn stage_pos_str(p: StagePos) -> &'static str {
    match p {
        StagePos::Left => "left",
        StagePos::Center => "center",
        StagePos::Right => "right",
    }
}

fn fmt_say_opts(o: &SayOpts) -> Result<String, PrintError> {
    let mut items: Vec<String> = Vec::new();
    if let Some(p) = &o.pose {
        // `(left)` một mục sẽ parse thành pos (spec 3.1) — pose trùng từ khoá
        // vị trí chỉ biểu diễn được khi có pos đi kèm để giữ đúng vị trí mục.
        if o.pos.is_none() && stage_pos_of(p).is_some() {
            return Err(perr(format!(
                "pose '{p}' trung tu khoa vi tri ma khong co pos — khong bieu dien duoc"
            )));
        }
        items.push(ck_ident(p, "pose")?.to_string());
    }
    if let Some(p) = o.pos {
        items.push(stage_pos_str(p).to_string());
    }
    if let Some(sfx) = &o.sfx {
        items.push(format!("sfx={}", ck_ident(sfx, "sfx")?));
    }
    if o.exit {
        items.push("exit".to_string());
    }
    if items.is_empty() {
        Ok(String::new())
    } else {
        Ok(format!(" ({})", items.join(", ")))
    }
}

fn stage_pos_of(s: &str) -> Option<StagePos> {
    match s {
        "left" => Some(StagePos::Left),
        "center" => Some(StagePos::Center),
        "right" => Some(StagePos::Right),
        _ => None,
    }
}

fn fmt_cond(c: &Cond) -> Result<String, PrintError> {
    let op = match c.op {
        mong_core::CondOp::Ge => ">=",
        mong_core::CondOp::Le => "<=",
        mong_core::CondOp::Eq => "==",
        mong_core::CondOp::Ne => "!=",
    };
    Ok(format!(
        "{} {op} {}",
        ck_ident(&c.var, "bien")?,
        fmt_value(&c.value)?
    ))
}

fn fmt_effect(e: &Effect) -> Result<String, PrintError> {
    let var = ck_ident(&e.var, "bien")?;
    Ok(match e.op {
        SetOp::Assign => format!("{var} = {}", fmt_value(&e.value)?),
        SetOp::Add | SetOp::Sub => {
            let Value::Int(i) = e.value else {
                return Err(perr(format!("effect +=/-= tren '{var}' phai la so nguyen")));
            };
            let op = if e.op == SetOp::Add { "+=" } else { "-=" };
            format!("{var} {op} {i}")
        }
        SetOp::Toggle => format!("!{var}"),
    })
}

fn fmt_value(v: &Value) -> Result<String, PrintError> {
    Ok(match v {
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Str(s) => {
            if s.contains('\r') {
                return Err(perr("chuoi chua \\r — DSL chua ho tro".to_string()));
            }
            let mut out = String::with_capacity(s.len() + 2);
            out.push('"');
            for c in s.chars() {
                match c {
                    '\\' => out.push_str("\\\\"),
                    '"' => out.push_str("\\\""),
                    '\n' => out.push_str("\\n"),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
    })
}

/// In biểu thức với ngoặc tối thiểu, đúng ưu tiên spec mục 4.
/// `Neg` luôn in `-( … )` — quy ước ngược với "`-5` là literal âm".
fn fmt_expr(e: &Expr) -> Result<String, PrintError> {
    fmt_expr_prec(e, 1)
}

fn prec(op: BinOp) -> u8 {
    match op {
        BinOp::Add | BinOp::Sub => 1,
        BinOp::Mul | BinOp::Div | BinOp::Rem => 2,
    }
}

fn fmt_expr_prec(e: &Expr, min: u8) -> Result<String, PrintError> {
    Ok(match e {
        Expr::Lit(v) => fmt_value(v)?,
        Expr::Var(v) => ck_ident(v, "bien")?.to_string(),
        Expr::Neg(inner) => format!("-({})", fmt_expr_prec(inner, 1)?),
        Expr::Bin { op, lhs, rhs } => {
            let p = prec(*op);
            let sym = match op {
                BinOp::Add => "+",
                BinOp::Sub => "-",
                BinOp::Mul => "*",
                BinOp::Div => "/",
                BinOp::Rem => "%",
            };
            // Kết hợp trái: vế phải cùng độ ưu tiên phải bọc ngoặc.
            let s = format!(
                "{} {sym} {}",
                fmt_expr_prec(lhs, p)?,
                fmt_expr_prec(rhs, p + 1)?
            );
            if p < min {
                format!("({s})")
            } else {
                s
            }
        }
    })
}

// ---- escape văn bản (nghịch đảo của unescape trong parse.rs) ----

fn esc_say(s: &str) -> String {
    s.replace('#', "\\#")
}

fn esc_arm(s: &str) -> String {
    s.replace('#', "\\#")
        .replace('[', "\\[")
        .replace('{', "\\{")
        .replace("->", "-\\>")
}

// ---- dựng AST từ IR (chiều print_story) ----

fn story_to_ast(
    story: &Story,
    strings: &BTreeMap<String, String>,
) -> Result<ScriptFile, PrintError> {
    let mut file = ScriptFile {
        story_title: Some(story.title.clone()).filter(|t| !t.trim().is_empty()),
        locales: std::iter::once(story.default_locale.clone())
            .chain(story.locales.iter().cloned())
            .collect(),
        vars: story
            .variables
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
        // @start bỏ được khi trùng node đầu (parse sẽ tự điền lại đúng thế).
        start: Some(story.start.clone()).filter(|s| story.nodes.first().map(|n| &n.id) != Some(s)),
        nodes: Vec::with_capacity(story.nodes.len()),
        leading: Vec::new(),
    };
    for n in &story.nodes {
        file.nodes.push(node_to_ast(n, strings)?);
    }
    Ok(file)
}

fn node_to_ast(n: &Node, strings: &BTreeMap<String, String>) -> Result<NodeAst, PrintError> {
    Ok(NodeAst {
        id: n.id.clone(),
        title: Some(n.title.trim().to_string()).filter(|t| !t.is_empty()),
        scene: n.scene.clone(),
        body: body_to_ast(&n.body, strings)?,
        pos: Pos::default(),
    })
}

fn body_to_ast(
    body: &[Instr],
    strings: &BTreeMap<String, String>,
) -> Result<Vec<StmtAst>, PrintError> {
    let mut out = Vec::new();
    for (idx, i) in body.iter().enumerate() {
        // Hai choice liền nhau: chèn dòng trống để parse không gộp nhóm.
        if matches!(i, Instr::Choice { .. })
            && idx > 0
            && matches!(body[idx - 1], Instr::Choice { .. })
        {
            out.push(bare(StmtKind::Blank));
        }
        instr_to_ast(i, strings, &mut out)?;
    }
    Ok(out)
}

fn bare(kind: StmtKind) -> StmtAst {
    StmtAst {
        kind,
        key: None,
        comment: None,
        pos: Pos::default(),
    }
}

fn keyed(kind: StmtKind, key: &str) -> StmtAst {
    StmtAst {
        key: Some(key.to_string()),
        ..bare(kind)
    }
}

fn lookup<'a>(strings: &'a BTreeMap<String, String>, key: &str) -> Result<&'a str, PrintError> {
    strings
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| perr(format!("key '{key}' khong co trong bang chuoi")))
}

fn instr_to_ast(
    i: &Instr,
    strings: &BTreeMap<String, String>,
    out: &mut Vec<StmtAst>,
) -> Result<(), PrintError> {
    match i {
        Instr::Say {
            speaker,
            text,
            opts,
        } => out.push(keyed(
            StmtKind::Say {
                speaker: speaker.clone(),
                opts: opts.clone(),
                text: lookup(strings, text)?.to_string(),
            },
            text,
        )),
        Instr::Choice { arms } => {
            for a in arms {
                out.push(arm_to_ast(a, strings)?);
            }
        }
        Instr::Set { effect } => out.push(bare(set_to_ast(effect))),
        Instr::SetExpr { var, expr } => out.push(bare(set_expr_to_ast(var, expr))),
        Instr::If {
            cond,
            then_branch,
            else_branch,
        } => out.push(bare(StmtKind::If {
            cond: cond.clone(),
            then_branch: body_to_ast(then_branch, strings)?,
            else_branch: body_to_ast(else_branch, strings)?,
        })),
        Instr::Jump { target } => out.push(bare(StmtKind::Jump {
            target: target.clone(),
        })),
        Instr::Call { target } => out.push(bare(StmtKind::Call {
            target: target.clone(),
        })),
        Instr::Return => out.push(bare(StmtKind::Return)),
        Instr::Label { name } => out.push(bare(StmtKind::Label { name: name.clone() })),
        Instr::Goto { label } => out.push(bare(StmtKind::Goto {
            label: label.clone(),
        })),
        Instr::End => out.push(bare(StmtKind::End)),
        Instr::Scene { scene, transition } => out.push(bare(StmtKind::Scene {
            scene: scene.clone(),
            transition: transition.clone(),
        })),
        Instr::Show {
            character,
            pose,
            pos,
        } => out.push(bare(StmtKind::Show {
            character: character.clone(),
            pose: pose.clone(),
            pos: *pos,
        })),
        Instr::Hide { character } => out.push(bare(StmtKind::Hide {
            character: character.clone(),
        })),
        Instr::Wait { ms } => out.push(bare(StmtKind::Wait { ms: *ms })),
        Instr::Sfx { asset } => out.push(bare(StmtKind::Sfx {
            asset: asset.clone(),
        })),
        Instr::Bgm { asset } => out.push(bare(StmtKind::Bgm {
            asset: asset.clone(),
        })),
        Instr::Rand { var, min, max } => out.push(bare(StmtKind::Rand {
            var: var.clone(),
            min: *min,
            max: *max,
        })),
        Instr::Ext { command, args } => out.push(bare(StmtKind::Ext {
            command: command.clone(),
            args: args.clone(),
        })),
    }
    Ok(())
}

fn arm_to_ast(a: &ChoiceArm, strings: &BTreeMap<String, String>) -> Result<StmtAst, PrintError> {
    let mut effects = a.effects.clone();
    for e in &mut effects {
        if e.op == SetOp::Toggle {
            e.value = Value::Bool(true); // chuẩn tắc — toggle không đọc value
        }
    }
    Ok(keyed(
        StmtKind::ChoiceArm {
            text: lookup(strings, &a.text)?.to_string(),
            target: a.target.clone(),
            cond: a.cond.clone(),
            effects,
        },
        &a.text,
    ))
}

fn set_to_ast(e: &Effect) -> StmtKind {
    match e.op {
        SetOp::Toggle => StmtKind::SetToggle { var: e.var.clone() },
        SetOp::Assign => StmtKind::SetAssign {
            var: e.var.clone(),
            op: AssignOp::Assign,
            rhs: Expr::Lit(e.value.clone()),
        },
        SetOp::Add => StmtKind::SetAssign {
            var: e.var.clone(),
            op: AssignOp::Add,
            rhs: Expr::Lit(e.value.clone()),
        },
        SetOp::Sub => StmtKind::SetAssign {
            var: e.var.clone(),
            op: AssignOp::Sub,
            rhs: Expr::Lit(e.value.clone()),
        },
    }
}

/// Nghịch đảo quy tắc 3.3: `set_expr {v, Bin(Add|Sub, Var(v), rhs)}` in dạng
/// ngắn `+=`/`-=` khi rhs không phải Lit(Int) (nếu là Lit(Int), dạng ngắn sẽ
/// parse ra `set` — phải in dạng dài `v = v + n`). `set_expr {v, Lit}` là
/// dạng thoái hoá: in như `set assign` (xem [`canonicalize_story`]).
fn set_expr_to_ast(var: &str, expr: &Expr) -> StmtKind {
    if let Expr::Lit(v) = expr {
        return StmtKind::SetAssign {
            var: var.to_string(),
            op: AssignOp::Assign,
            rhs: Expr::Lit(v.clone()),
        };
    }
    if let Expr::Bin { op, lhs, rhs } = expr {
        let shorthand = match op {
            BinOp::Add => Some(AssignOp::Add),
            BinOp::Sub => Some(AssignOp::Sub),
            _ => None,
        };
        if let (Some(aop), Expr::Var(v)) = (shorthand, lhs.as_ref()) {
            if v == var && !matches!(rhs.as_ref(), Expr::Lit(Value::Int(_))) {
                return StmtKind::SetAssign {
                    var: var.to_string(),
                    op: aop,
                    rhs: rhs.as_ref().clone(),
                };
            }
        }
    }
    StmtKind::SetAssign {
        var: var.to_string(),
        op: AssignOp::Assign,
        rhs: expr.clone(),
    }
}
