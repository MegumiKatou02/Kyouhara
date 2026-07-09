//! Hạ [`ScriptFile`] xuống [`Story`] + bảng chuỗi defaultLocale, kèm sinh key
//! cho dòng chưa có `#~` (spec-mongscript mục 6–7).
//!
//! Nhận `&mut ScriptFile` có chủ đích: key mới sinh được ghi ngược vào AST để
//! formatter (bước 3) in ra file — "bước duy nhất ghi ngược vào nguồn".

use super::ast::*;
use super::parse::DslError;
use mong_core::{BinOp, ChoiceArm, Effect, Expr, Instr, Node, SetOp, Story, Value, FORMAT_VERSION};
use std::collections::{BTreeMap, BTreeSet};

/// Kết quả biên dịch một file DSL.
#[derive(Debug, Clone, PartialEq)]
pub struct CompileOutput {
    pub story: Story,
    /// Bảng chuỗi của defaultLocale: key → văn bản trong file.
    pub strings: BTreeMap<String, String>,
    /// Số key vừa sinh mới (đã ghi ngược vào AST) — >0 nghĩa là file nguồn
    /// cần được format lại để lưu key bền vững.
    pub generated_keys: usize,
}

fn err_at(pos: Pos, message: impl Into<String>) -> DslError {
    DslError {
        pos,
        message: message.into(),
    }
}

/// Biên dịch AST → Story. Tiện ích một phát: `parse_dsl` rồi `compile`.
pub fn load_story_dsl(src: &str) -> Result<CompileOutput, DslError> {
    let mut file = super::parse::parse_dsl(src)?;
    compile(&mut file)
}

pub fn compile(file: &mut ScriptFile) -> Result<CompileOutput, DslError> {
    if file.nodes.is_empty() {
        return Err(err_at(Pos { line: 1, col: 1 }, "file khong co @node nao"));
    }
    if file.locales.is_empty() {
        return Err(err_at(
            Pos { line: 1, col: 1 },
            "thieu @locale (phan tu dau la defaultLocale)",
        ));
    }

    let generated_keys = generate_keys(file);

    let mut strings = BTreeMap::new();
    let mut nodes = Vec::with_capacity(file.nodes.len());
    for n in &file.nodes {
        nodes.push(Node {
            id: n.id.clone(),
            title: n.title.clone().unwrap_or_default(),
            scene: n.scene.clone(),
            body: lower_body(&n.body, &mut strings)?,
        });
    }

    let mut variables = BTreeMap::new();
    for (name, val) in &file.vars {
        variables.insert(name.clone(), val.clone());
    }

    Ok(CompileOutput {
        story: Story {
            format_version: FORMAT_VERSION,
            title: file.story_title.clone().unwrap_or_default(),
            default_locale: file.locales[0].clone(),
            locales: file.locales[1..].to_vec(),
            variables,
            start: file
                .start
                .clone()
                .unwrap_or_else(|| file.nodes[0].id.clone()),
            nodes,
        },
        strings,
        generated_keys,
    })
}

// ---- sinh key (spec mục 6) ----

/// Sinh key cho mọi dòng dịch được còn thiếu `#~`, ghi ngược vào AST.
/// Trả về số key vừa sinh. Tách khỏi `compile` để formatter dùng được
/// trên file chưa đầy đủ ngữ nghĩa (spec mục 7: key-gen đi cùng format).
pub fn generate_keys(file: &mut ScriptFile) -> usize {
    KeyGen::collect(file).fill_missing(file)
}

/// Sinh key tự động dạng `<node_id>.l<n>` / `<node_id>.c<n>`.
/// `n` đếm tiếp từ số lớn nhất đã thấy trong file cho (node, loại) đó —
/// không tái dùng số của dòng đã xoá trong phiên; key là định danh mờ,
/// dòng chuyển sang node khác vẫn giữ key cũ.
struct KeyGen {
    taken: BTreeSet<String>,
    /// (node_id, 'l'|'c') → counter lớn nhất đã thấy.
    max_seen: BTreeMap<(String, char), u64>,
}

impl KeyGen {
    fn collect(file: &ScriptFile) -> Self {
        let mut kg = KeyGen {
            taken: BTreeSet::new(),
            max_seen: BTreeMap::new(),
        };
        for n in &file.nodes {
            kg.scan_body(&n.body);
        }
        kg
    }

    fn scan_body(&mut self, body: &[StmtAst]) {
        for s in body {
            if let Some(k) = &s.key {
                self.taken.insert(k.clone());
                if let Some((node, kind, num)) = split_auto_key(k) {
                    let e = self.max_seen.entry((node.to_string(), kind)).or_insert(0);
                    *e = (*e).max(num);
                }
            }
            if let StmtKind::If {
                then_branch,
                else_branch,
                ..
            } = &s.kind
            {
                self.scan_body(then_branch);
                self.scan_body(else_branch);
            }
        }
    }

    fn fill_missing(mut self, file: &mut ScriptFile) -> usize {
        let mut count = 0;
        let ids: Vec<String> = file.nodes.iter().map(|n| n.id.clone()).collect();
        for (n, id) in file.nodes.iter_mut().zip(ids) {
            count += self.fill_body(&id, &mut n.body);
        }
        count
    }

    fn fill_body(&mut self, node_id: &str, body: &mut [StmtAst]) -> usize {
        let mut count = 0;
        for s in body.iter_mut() {
            match &mut s.kind {
                StmtKind::Say { .. } if s.key.is_none() => {
                    s.key = Some(self.next(node_id, 'l'));
                    count += 1;
                }
                StmtKind::ChoiceArm { .. } if s.key.is_none() => {
                    s.key = Some(self.next(node_id, 'c'));
                    count += 1;
                }
                StmtKind::If {
                    then_branch,
                    else_branch,
                    ..
                } => {
                    count += self.fill_body(node_id, then_branch);
                    count += self.fill_body(node_id, else_branch);
                }
                _ => {}
            }
        }
        count
    }

    fn next(&mut self, node_id: &str, kind: char) -> String {
        let counter = self
            .max_seen
            .entry((node_id.to_string(), kind))
            .or_insert(0);
        loop {
            *counter += 1;
            let k = format!("{node_id}.{kind}{counter}");
            if self.taken.insert(k.clone()) {
                return k;
            }
        }
    }
}

/// Tách key dạng tự sinh `node.l3` → (node, 'l', 3); dạng khác → None.
fn split_auto_key(k: &str) -> Option<(&str, char, u64)> {
    let (node, rest) = k.rsplit_once('.')?;
    let kind = rest.chars().next()?;
    if kind != 'l' && kind != 'c' {
        return None;
    }
    let num: u64 = rest[1..].parse().ok()?;
    Some((node, kind, num))
}

// ---- hạ lệnh ----

fn lower_body(
    body: &[StmtAst],
    strings: &mut BTreeMap<String, String>,
) -> Result<Vec<Instr>, DslError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < body.len() {
        let s = &body[i];
        match &s.kind {
            // Trivia: không vào IR. Dòng trống vẫn có ngữ nghĩa duy nhất là
            // cắt nhóm `>` — được xử lý tự nhiên vì nhóm bên dưới chỉ gom
            // các arm liền kề.
            StmtKind::Blank | StmtKind::Comment(_) => i += 1,
            StmtKind::ChoiceArm { .. } => {
                let mut arms = Vec::new();
                // Gom arm liên tiếp; comment nguyên dòng xen giữa không cắt
                // nhóm, mọi thứ khác (kể cả dòng trống) thì cắt (spec 3.2).
                while i < body.len() {
                    match &body[i].kind {
                        StmtKind::ChoiceArm {
                            text,
                            target,
                            cond,
                            effects,
                        } => {
                            arms.push(ChoiceArm {
                                text: take_key(&body[i], text, strings)?,
                                target: target.clone(),
                                cond: cond.clone(),
                                effects: effects.clone(),
                            });
                            i += 1;
                        }
                        StmtKind::Comment(_) => i += 1,
                        _ => break,
                    }
                }
                out.push(Instr::Choice { arms });
            }
            StmtKind::Say {
                speaker,
                opts,
                text,
            } => {
                out.push(Instr::Say {
                    speaker: speaker.clone(),
                    text: take_key(s, text, strings)?,
                    opts: opts.clone(),
                });
                i += 1;
            }
            StmtKind::SetToggle { var } => {
                out.push(Instr::Set {
                    effect: Effect {
                        var: var.clone(),
                        op: SetOp::Toggle,
                        value: Value::Bool(true),
                    },
                });
                i += 1;
            }
            StmtKind::SetAssign { var, op, rhs } => {
                out.push(lower_assign(var, *op, rhs, s.pos)?);
                i += 1;
            }
            StmtKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                out.push(Instr::If {
                    cond: cond.clone(),
                    then_branch: lower_body(then_branch, strings)?,
                    else_branch: lower_body(else_branch, strings)?,
                });
                i += 1;
            }
            StmtKind::Jump { target } => {
                out.push(Instr::Jump {
                    target: target.clone(),
                });
                i += 1;
            }
            StmtKind::Call { target } => {
                out.push(Instr::Call {
                    target: target.clone(),
                });
                i += 1;
            }
            StmtKind::Return => {
                out.push(Instr::Return);
                i += 1;
            }
            StmtKind::Label { name } => {
                out.push(Instr::Label { name: name.clone() });
                i += 1;
            }
            StmtKind::Goto { label } => {
                out.push(Instr::Goto {
                    label: label.clone(),
                });
                i += 1;
            }
            StmtKind::End => {
                out.push(Instr::End);
                i += 1;
            }
            StmtKind::Scene { scene, transition } => {
                out.push(Instr::Scene {
                    scene: scene.clone(),
                    transition: transition.clone(),
                });
                i += 1;
            }
            StmtKind::Show {
                character,
                pose,
                pos,
            } => {
                out.push(Instr::Show {
                    character: character.clone(),
                    pose: pose.clone(),
                    pos: *pos,
                });
                i += 1;
            }
            StmtKind::Hide { character } => {
                out.push(Instr::Hide {
                    character: character.clone(),
                });
                i += 1;
            }
            StmtKind::Wait { ms } => {
                out.push(Instr::Wait { ms: *ms });
                i += 1;
            }
            StmtKind::Sfx { asset } => {
                out.push(Instr::Sfx {
                    asset: asset.clone(),
                });
                i += 1;
            }
            StmtKind::Bgm { asset } => {
                out.push(Instr::Bgm {
                    asset: asset.clone(),
                });
                i += 1;
            }
            StmtKind::Rand { var, min, max } => {
                out.push(Instr::Rand {
                    var: var.clone(),
                    min: *min,
                    max: *max,
                });
                i += 1;
            }
            StmtKind::Ext { command, args } => {
                out.push(Instr::Ext {
                    command: command.clone(),
                    args: args.clone(),
                });
                i += 1;
            }
        }
    }
    Ok(out)
}

/// Lấy key (chắc chắn đã được sinh) và ghi văn bản vào bảng chuỗi.
fn take_key(
    s: &StmtAst,
    text: &str,
    strings: &mut BTreeMap<String, String>,
) -> Result<String, DslError> {
    let key = s.key.clone().expect("key da duoc sinh o buoc fill_missing");
    if strings.insert(key.clone(), text.to_string()).is_some() {
        return Err(err_at(s.pos, format!("key '{key}' trung trong file")));
    }
    Ok(key)
}

/// Quy tắc chọn set/set_expr — spec 3.3, xác định để round-trip ổn định.
fn lower_assign(var: &str, op: AssignOp, rhs: &Expr, pos: Pos) -> Result<Instr, DslError> {
    match (op, rhs) {
        (AssignOp::Assign, Expr::Lit(v)) => Ok(Instr::Set {
            effect: Effect {
                var: var.into(),
                op: SetOp::Assign,
                value: v.clone(),
            },
        }),
        (AssignOp::Add, Expr::Lit(Value::Int(i))) => Ok(Instr::Set {
            effect: Effect {
                var: var.into(),
                op: SetOp::Add,
                value: Value::Int(*i),
            },
        }),
        (AssignOp::Sub, Expr::Lit(Value::Int(i))) => Ok(Instr::Set {
            effect: Effect {
                var: var.into(),
                op: SetOp::Sub,
                value: Value::Int(*i),
            },
        }),
        (AssignOp::Add | AssignOp::Sub, Expr::Lit(_)) => {
            Err(err_at(pos, "~ var +=/-= chi nhan so nguyen hoac bieu thuc"))
        }
        (AssignOp::Assign, e) => Ok(Instr::SetExpr {
            var: var.into(),
            expr: e.clone(),
        }),
        (aop, e) => Ok(Instr::SetExpr {
            var: var.into(),
            expr: Expr::Bin {
                op: if aop == AssignOp::Add {
                    BinOp::Add
                } else {
                    BinOp::Sub
                },
                lhs: Box::new(Expr::Var(var.into())),
                rhs: Box::new(e.clone()),
            },
        }),
    }
}
