//! Parse text MộngScript → [`ScriptFile`] (AST), theo docs/spec-mongscript.md.
//! Chỉ lo cú pháp; ánh xạ sang IR + sinh key nằm ở `lower.rs`.

use super::ast::*;
use mong_core::{BinOp, Cond, CondOp, Effect, Expr, SayOpts, SetOp, StagePos, Value};
use pest::iterators::Pair;
use pest::pratt_parser::{Assoc, Op, PrattParser};
use pest::Parser;
use std::sync::OnceLock;

#[derive(pest_derive::Parser)]
#[grammar = "mongscript.pest"]
struct MongScriptParser;

/// Lỗi cú pháp/ngữ nghĩa cục bộ của DSL, kèm vị trí 1-based.
#[derive(Debug, Clone, PartialEq)]
pub struct DslError {
    pub pos: Pos,
    pub message: String,
}

impl std::fmt::Display for DslError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "dong {}, cot {}: {}",
            self.pos.line, self.pos.col, self.message
        )
    }
}
impl std::error::Error for DslError {}

fn err_at(pos: Pos, message: impl Into<String>) -> DslError {
    DslError {
        pos,
        message: message.into(),
    }
}

fn pos_of(pair: &Pair<Rule>) -> Pos {
    let (line, col) = pair.line_col();
    Pos { line, col }
}

/// Parse một file `.mongscript`.
pub fn parse_dsl(src: &str) -> Result<ScriptFile, DslError> {
    let file = MongScriptParser::parse(Rule::file, src)
        .map_err(pest_to_dsl_error)?
        .next()
        .expect("rule file luon co dung mot pair");

    let mut out = ScriptFile::default();
    let mut seen_node = false;

    for item in file.into_inner() {
        let pos = pos_of(&item);
        match item.as_rule() {
            Rule::EOI => {}
            Rule::blank | Rule::comment_line if !seen_node => {
                out.leading.push(trivia_stmt(item));
            }
            // blank/comment giữa hai node bám vào node trước qua grammar,
            // nhánh này chỉ còn gặp khi file chưa có node nào.
            Rule::blank | Rule::comment_line => unreachable!("grammar gan trivia vao node"),
            Rule::story_dir | Rule::locale_dir | Rule::var_dir | Rule::start_dir => {
                if seen_node {
                    return Err(err_at(
                        pos,
                        "directive cap file phai dung truoc @node dau tien",
                    ));
                }
                file_directive(item, &mut out)?;
            }
            Rule::node => {
                seen_node = true;
                out.nodes.push(node_ast(item)?);
            }
            r => unreachable!("file_item bat ngo: {r:?}"),
        }
    }
    Ok(out)
}

fn pest_to_dsl_error(e: pest::error::Error<Rule>) -> DslError {
    let (line, col) = match e.line_col {
        pest::error::LineColLocation::Pos(p) => p,
        pest::error::LineColLocation::Span(p, _) => p,
    };
    DslError {
        pos: Pos { line, col },
        message: format!("cu phap khong hop le: {}", e.variant.message()),
    }
}

fn trivia_stmt(pair: Pair<Rule>) -> StmtAst {
    let pos = pos_of(&pair);
    let kind = match pair.as_rule() {
        Rule::blank => StmtKind::Blank,
        Rule::comment_line => {
            let c = pair.into_inner().next().expect("comment_line chua comment");
            StmtKind::Comment(comment_text(c.as_str()))
        }
        r => unreachable!("trivia bat ngo: {r:?}"),
    };
    StmtAst {
        kind,
        key: None,
        comment: None,
        pos,
    }
}

/// Cắt dấu `#` đầu và space đệm chuẩn của comment.
fn comment_text(raw: &str) -> String {
    raw.trim_start_matches('#')
        .strip_prefix(' ')
        .unwrap_or(raw.trim_start_matches('#'))
        .to_string()
}

fn file_directive(pair: Pair<Rule>, out: &mut ScriptFile) -> Result<(), DslError> {
    let pos = pos_of(&pair);
    let rule = pair.as_rule();
    let mut inner = pair.into_inner();
    match rule {
        Rule::story_dir => {
            let text = unescape_say(inner.next().expect("@story co text").as_str());
            if out.story_title.replace(text).is_some() {
                return Err(err_at(pos, "@story khai bao hai lan"));
            }
        }
        Rule::locale_dir => {
            if !out.locales.is_empty() {
                return Err(err_at(pos, "@locale khai bao hai lan"));
            }
            for p in inner {
                if p.as_rule() == Rule::loc_ident {
                    out.locales.push(p.as_str().to_string());
                }
            }
        }
        Rule::var_dir => {
            let name = inner.next().expect("@var co ten").as_str().to_string();
            let val = parse_value(inner.next().expect("@var co gia tri"))?;
            if out.vars.iter().any(|(n, _)| n == &name) {
                return Err(err_at(pos, format!("@var '{name}' khai bao hai lan")));
            }
            out.vars.push((name, val));
        }
        Rule::start_dir => {
            let id = inner.next().expect("@start co id").as_str().to_string();
            if out.start.replace(id).is_some() {
                return Err(err_at(pos, "@start khai bao hai lan"));
            }
        }
        r => unreachable!("directive bat ngo: {r:?}"),
    }
    Ok(())
}

fn node_ast(pair: Pair<Rule>) -> Result<NodeAst, DslError> {
    let pos = pos_of(&pair);
    let mut inner = pair.into_inner();
    let hdr = inner.next().expect("node bat dau bang node_hdr");
    let id = hdr
        .into_inner()
        .next()
        .expect("@node co id")
        .as_str()
        .to_string();

    let mut node = NodeAst {
        id,
        title: None,
        scene: None,
        body: Vec::new(),
        pos,
    };

    for item in inner {
        let ipos = pos_of(&item);
        match item.as_rule() {
            Rule::title_hdr => {
                if !node.body.iter().any(is_real_stmt) && node.title.is_none() {
                    let t = item.into_inner().next().expect("@title co text");
                    node.title = Some(unescape_say(t.as_str()));
                } else {
                    return Err(err_at(
                        ipos,
                        "@title phai dung ngay sau @node va chi mot lan",
                    ));
                }
            }
            Rule::scene_hdr => {
                if !node.body.iter().any(is_real_stmt) && node.scene.is_none() {
                    let s = item.into_inner().next().expect("@scene co id");
                    node.scene = Some(s.as_str().to_string());
                } else {
                    return Err(err_at(
                        ipos,
                        "@scene phai dung ngay sau @node va chi mot lan",
                    ));
                }
            }
            Rule::blank | Rule::comment_line => node.body.push(trivia_stmt(item)),
            _ => node.body.push(stmt_ast(item)?),
        }
    }
    Ok(node)
}

/// Comment/dòng trống không tính là "lệnh thật" khi xét vị trí @title/@scene.
fn is_real_stmt(s: &StmtAst) -> bool {
    !matches!(s.kind, StmtKind::Blank | StmtKind::Comment(_))
}

fn stmt_ast(pair: Pair<Rule>) -> Result<StmtAst, DslError> {
    let pos = pos_of(&pair);
    let rule = pair.as_rule();
    let mut key = None;
    let mut comment = None;
    let mut inner: Vec<Pair<Rule>> = Vec::new();

    // Tách phần đuôi (tail_text/tail_bare) khỏi phần thân cho mọi lệnh.
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::tail_text | Rule::tail_bare => {
                for t in p.into_inner() {
                    match t.as_rule() {
                        Rule::key_tag => {
                            let k = t.into_inner().next().expect("key_tag co key_ident");
                            key = Some(k.as_str().to_string());
                        }
                        Rule::comment => comment = Some(comment_text(t.as_str())),
                        r => unreachable!("tail bat ngo: {r:?}"),
                    }
                }
            }
            _ => inner.push(p),
        }
    }

    let kind = stmt_kind(rule, inner, pos)?;
    if key.is_some() && !kind.carries_text() {
        return Err(err_at(pos, "#~ chi duoc gan tren dong thoai hoac lua chon"));
    }
    Ok(StmtAst {
        kind,
        key,
        comment,
        pos,
    })
}

fn stmt_kind(rule: Rule, inner: Vec<Pair<Rule>>, pos: Pos) -> Result<StmtKind, DslError> {
    let mut it = inner.into_iter();
    Ok(match rule {
        Rule::narrate => {
            let mut opts = SayOpts::default();
            let mut text = None;
            for p in it {
                match p.as_rule() {
                    Rule::say_opts => opts = say_opts(p)?,
                    Rule::say_text => text = Some(unescape_say(p.as_str())),
                    r => unreachable!("narrate bat ngo: {r:?}"),
                }
            }
            StmtKind::Say {
                speaker: None,
                opts,
                text: text.expect("narrate co text"),
            }
        }
        Rule::dialogue => {
            let speaker = it.next().expect("dialogue co speaker").as_str().to_string();
            let mut opts = SayOpts::default();
            let mut text = None;
            for p in it {
                match p.as_rule() {
                    Rule::say_opts => opts = say_opts(p)?,
                    Rule::say_text => text = Some(unescape_say(p.as_str())),
                    r => unreachable!("dialogue bat ngo: {r:?}"),
                }
            }
            StmtKind::Say {
                speaker: Some(speaker),
                opts,
                text: text.expect("dialogue co text"),
            }
        }
        Rule::choice_arm => {
            let text = unescape_arm(it.next().expect("arm co text").as_str());
            let (mut target, mut cond, mut effects) = (None, None, Vec::new());
            for p in it {
                let ppos = pos_of(&p);
                match p.as_rule() {
                    Rule::arm_target => {
                        let t = p.into_inner().next().expect("-> co id");
                        if target.replace(t.as_str().to_string()).is_some() {
                            return Err(err_at(ppos, "lua chon co hai dich '->'"));
                        }
                    }
                    Rule::arm_cond => {
                        let c = parse_cond(p.into_inner().next().expect("[ ] chua cond"))?;
                        if cond.replace(c).is_some() {
                            return Err(err_at(ppos, "lua chon co hai dieu kien [ ]"));
                        }
                    }
                    Rule::arm_fx => {
                        if !effects.is_empty() {
                            return Err(err_at(ppos, "lua chon co hai khoi { }"));
                        }
                        for e in p.into_inner() {
                            effects.push(parse_effect(e)?);
                        }
                    }
                    r => unreachable!("arm bat ngo: {r:?}"),
                }
            }
            StmtKind::ChoiceArm {
                text,
                target,
                cond,
                effects,
            }
        }
        Rule::set_stmt => {
            let p = it.next().expect("set co than");
            match p.as_rule() {
                Rule::set_toggle => StmtKind::SetToggle {
                    var: p.into_inner().next().expect("!var").as_str().to_string(),
                },
                Rule::set_assign => {
                    let mut a = p.into_inner();
                    let var = a.next().expect("var").as_str().to_string();
                    let op = match a.next().expect("op").as_str() {
                        "=" => AssignOp::Assign,
                        "+=" => AssignOp::Add,
                        "-=" => AssignOp::Sub,
                        o => unreachable!("set_op bat ngo: {o}"),
                    };
                    let rhs = parse_expr(a.next().expect("expr"))?;
                    StmtKind::SetAssign { var, op, rhs }
                }
                r => unreachable!("set bat ngo: {r:?}"),
            }
        }
        Rule::if_stmt => {
            let cond = parse_cond(it.next().expect("? co cond"))?;
            let then_branch = parse_block(it.next().expect("? co block"))?;
            let else_branch = match it.next() {
                Some(ep) => parse_block(ep.into_inner().next().expect("else co block"))?,
                None => Vec::new(),
            };
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            }
        }
        Rule::jump_s => StmtKind::Jump {
            target: it.next().expect("jump co id").as_str().to_string(),
        },
        Rule::call_s => StmtKind::Call {
            target: it.next().expect("call co id").as_str().to_string(),
        },
        Rule::return_s => StmtKind::Return,
        Rule::label_s => StmtKind::Label {
            name: it.next().expect("label co ten").as_str().to_string(),
        },
        Rule::goto_s => StmtKind::Goto {
            label: it.next().expect("goto co ten").as_str().to_string(),
        },
        Rule::end_s => StmtKind::End,
        Rule::scene_s => StmtKind::Scene {
            scene: it.next().expect("scene co id").as_str().to_string(),
            transition: it.next().map(|p| p.as_str().to_string()),
        },
        Rule::show_s => {
            let character = it.next().expect("show co char").as_str().to_string();
            let idents: Vec<Pair<Rule>> = it.collect();
            let (pose, pos_pair) = match idents.len() {
                1 => (None, &idents[0]),
                2 => (Some(idents[0].as_str().to_string()), &idents[1]),
                n => unreachable!("show co {n} ident sau char"),
            };
            StmtKind::Show {
                character,
                pose,
                pos: parse_stage_pos(pos_pair)?,
            }
        }
        Rule::hide_s => StmtKind::Hide {
            character: it.next().expect("hide co char").as_str().to_string(),
        },
        Rule::wait_s => {
            let p = it.next().expect("wait co so ms");
            let ms = p
                .as_str()
                .parse::<u32>()
                .map_err(|_| err_at(pos_of(&p), "wait: so ms vuot pham vi u32"))?;
            StmtKind::Wait { ms }
        }
        Rule::sfx_s => StmtKind::Sfx {
            asset: it.next().expect("sfx co id").as_str().to_string(),
        },
        Rule::bgm_s => StmtKind::Bgm {
            asset: it.next().map(|p| p.as_str().to_string()),
        },
        Rule::rand_s => {
            let var = it.next().expect("rand co var").as_str().to_string();
            let min = parse_i64(it.next().expect("rand co min"))?;
            let max = parse_i64(it.next().expect("rand co max"))?;
            StmtKind::Rand { var, min, max }
        }
        Rule::ext_s => {
            let command = it.next().expect("ext co lenh").as_str().to_string();
            let args = match it.next() {
                Some(p) => serde_json::from_str(p.as_str()).map_err(|e| {
                    err_at(pos_of(&p), format!("ext: args khong phai JSON hop le: {e}"))
                })?,
                None => serde_json::Value::Null,
            };
            StmtKind::Ext { command, args }
        }
        r => Err(err_at(pos, format!("lenh khong ho tro: {r:?}")))?,
    })
}

fn parse_block(pair: Pair<Rule>) -> Result<Vec<StmtAst>, DslError> {
    debug_assert_eq!(pair.as_rule(), Rule::block);
    let mut out = Vec::new();
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::blank | Rule::comment_line => out.push(trivia_stmt(p)),
            _ => out.push(stmt_ast(p)?),
        }
    }
    Ok(out)
}

fn say_opts(pair: Pair<Rule>) -> Result<SayOpts, DslError> {
    let mut opts = SayOpts::default();
    let mut plains: Vec<Pair<Rule>> = Vec::new();
    for p in pair.into_inner() {
        match p.as_rule() {
            Rule::opt_sfx => {
                let id = p.into_inner().next().expect("sfx= co id");
                opts.sfx = Some(id.as_str().to_string());
            }
            Rule::opt_exit => opts.exit = true,
            Rule::opt_plain => plains.push(p),
            r => unreachable!("opt bat ngo: {r:?}"),
        }
    }
    // Positional: (pose, pos). Một mục duy nhất là left/center/right → pos,
    // còn lại → pose (spec 3.1).
    match plains.len() {
        0 => {}
        1 => {
            if let Ok(sp) = stage_pos(plains[0].as_str()) {
                opts.pos = Some(sp);
            } else {
                opts.pose = Some(plains[0].as_str().to_string());
            }
        }
        2 => {
            opts.pose = Some(plains[0].as_str().to_string());
            opts.pos = Some(parse_stage_pos(&plains[1])?);
        }
        _ => {
            return Err(err_at(
                pos_of(&plains[2]),
                "toi da hai muc positional trong ( ): pose, pos",
            ))
        }
    }
    Ok(opts)
}

fn stage_pos(s: &str) -> Result<StagePos, ()> {
    match s {
        "left" => Ok(StagePos::Left),
        "center" => Ok(StagePos::Center),
        "right" => Ok(StagePos::Right),
        _ => Err(()),
    }
}

fn parse_stage_pos(p: &Pair<Rule>) -> Result<StagePos, DslError> {
    stage_pos(p.as_str()).map_err(|_| {
        err_at(
            pos_of(p),
            format!("vi tri '{}' phai la left/center/right", p.as_str()),
        )
    })
}

fn parse_cond(pair: Pair<Rule>) -> Result<Cond, DslError> {
    debug_assert_eq!(pair.as_rule(), Rule::cond);
    let mut it = pair.into_inner();
    let var = it.next().expect("cond co var").as_str().to_string();
    let op = match it.next().expect("cond co op").as_str() {
        ">=" => CondOp::Ge,
        "<=" => CondOp::Le,
        "==" => CondOp::Eq,
        "!=" => CondOp::Ne,
        o => unreachable!("cond_op bat ngo: {o}"),
    };
    let value = parse_value(it.next().expect("cond co gia tri"))?;
    Ok(Cond { var, op, value })
}

fn parse_effect(pair: Pair<Rule>) -> Result<Effect, DslError> {
    debug_assert_eq!(pair.as_rule(), Rule::effect);
    let p = pair.into_inner().next().expect("effect co than");
    Ok(match p.as_rule() {
        Rule::toggle_fx => Effect {
            var: p.into_inner().next().expect("!var").as_str().to_string(),
            op: SetOp::Toggle,
            value: Value::Bool(true), // toggle không dùng value; giữ ổn định
        },
        Rule::assign_fx => {
            let mut it = p.into_inner();
            let var = it.next().expect("var").as_str().to_string();
            let op_pair = it.next().expect("op");
            let op = match op_pair.as_str() {
                "=" => SetOp::Assign,
                "+=" => SetOp::Add,
                "-=" => SetOp::Sub,
                o => unreachable!("eff_op bat ngo: {o}"),
            };
            let value = parse_value(it.next().expect("gia tri"))?;
            if matches!(op, SetOp::Add | SetOp::Sub) && !matches!(value, Value::Int(_)) {
                return Err(err_at(
                    pos_of(&op_pair),
                    "+=/-= trong { } chi nhan so nguyen",
                ));
            }
            Effect { var, op, value }
        }
        r => unreachable!("effect bat ngo: {r:?}"),
    })
}

fn parse_value(pair: Pair<Rule>) -> Result<Value, DslError> {
    debug_assert_eq!(pair.as_rule(), Rule::value);
    let p = pair.into_inner().next().expect("value co than");
    Ok(match p.as_rule() {
        Rule::int => Value::Int(parse_i64(p)?),
        Rule::bool_lit => Value::Bool(p.as_str() == "true"),
        Rule::str_lit => Value::Str(unescape_str_lit(p.as_str())),
        r => unreachable!("value bat ngo: {r:?}"),
    })
}

fn parse_i64(p: Pair<Rule>) -> Result<i64, DslError> {
    p.as_str()
        .parse::<i64>()
        .map_err(|_| err_at(pos_of(&p), "so nguyen vuot pham vi i64"))
}

fn pratt() -> &'static PrattParser<Rule> {
    static PRATT: OnceLock<PrattParser<Rule>> = OnceLock::new();
    PRATT.get_or_init(|| {
        // .op sau bind chặt hơn: + - < * / %  (spec mục 4).
        PrattParser::new()
            .op(Op::infix(Rule::op_add, Assoc::Left) | Op::infix(Rule::op_sub, Assoc::Left))
            .op(Op::infix(Rule::op_mul, Assoc::Left)
                | Op::infix(Rule::op_div, Assoc::Left)
                | Op::infix(Rule::op_rem, Assoc::Left))
    })
}

fn parse_expr(pair: Pair<Rule>) -> Result<Expr, DslError> {
    debug_assert_eq!(pair.as_rule(), Rule::expr);
    pratt()
        .map_primary(parse_atom)
        .map_infix(|lhs, op, rhs| {
            let op = match op.as_rule() {
                Rule::op_add => BinOp::Add,
                Rule::op_sub => BinOp::Sub,
                Rule::op_mul => BinOp::Mul,
                Rule::op_div => BinOp::Div,
                Rule::op_rem => BinOp::Rem,
                r => unreachable!("op bat ngo: {r:?}"),
            };
            Ok(Expr::Bin {
                op,
                lhs: Box::new(lhs?),
                rhs: Box::new(rhs?),
            })
        })
        .parse(pair.into_inner())
}

fn parse_atom(pair: Pair<Rule>) -> Result<Expr, DslError> {
    debug_assert_eq!(pair.as_rule(), Rule::atom);
    let p = pair.into_inner().next().expect("atom co than");
    Ok(match p.as_rule() {
        Rule::value => Expr::Lit(parse_value(p)?),
        Rule::var_ref => Expr::Var(p.into_inner().next().expect("ident").as_str().to_string()),
        Rule::paren => parse_expr(p.into_inner().next().expect("( ) chua expr"))?,
        Rule::neg => Expr::Neg(Box::new(parse_atom(
            p.into_inner().next().expect("- co atom"),
        )?)),
        r => unreachable!("atom bat ngo: {r:?}"),
    })
}

// ---- unescape văn bản ----

/// `\#` → `#` trong thoại/dẫn truyện/@title/@story; `\(` đầu chuỗi → `(`
/// (chỉ có nghĩa ở đầu — tránh nhầm với opts của dòng `*`); trim khoảng
/// trắng đuôi (khoảng trắng trước `#~`/cuối dòng là trình bày).
fn unescape_say(raw: &str) -> String {
    let raw = raw.strip_prefix("\\(").map_or_else(
        || raw.trim_end().to_string(),
        |rest| format!("({}", rest.trim_end()),
    );
    raw.replace("\\#", "#")
}

/// Nhãn lựa chọn: thêm `\[` `\{` `\>` (spec 3.2).
fn unescape_arm(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.trim_end().chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&n @ ('#' | '[' | '{' | '>')) => {
                    out.push(n);
                    chars.next();
                }
                _ => out.push(c),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Unescape literal chuỗi `"…"`: `\"` `\\` `\n`.
fn unescape_str_lit(raw: &str) -> String {
    let body = &raw[1..raw.len() - 1];
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some(o) => out.push(o),
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    out
}
