//! mong-script — nạp cốt truyện và kiểm tra tính toàn vẹn (lint).
//!
//! M0: nạp JSON (định dạng dự án `.mong`) thành [`Story`] + `validate`.
//! M2 sẽ thêm parser DSL MộngScript; cả hai đường đều ra cùng một IR.

use mong_core::{Instr, Story};
use std::collections::BTreeSet;

/// Mức độ của một phát hiện lint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// Một phát hiện của bộ lint.
#[derive(Debug, Clone)]
pub struct Issue {
    pub severity: Severity,
    pub node: Option<String>,
    pub message: String,
}

/// Nạp cốt truyện từ JSON của dự án.
pub fn load_story_json(json: &str) -> Result<Story, serde_json::Error> {
    serde_json::from_str(json)
}

fn err(node: Option<&str>, msg: String) -> Issue {
    Issue { severity: Severity::Error, node: node.map(String::from), message: msg }
}
fn warn(node: Option<&str>, msg: String) -> Issue {
    Issue { severity: Severity::Warning, node: node.map(String::from), message: msg }
}

/// Gom mọi đích nhảy (jump/call/choice) trong một block, đệ quy qua `if`.
fn collect_targets<'a>(body: &'a [Instr], out: &mut Vec<&'a str>) {
    for i in body {
        match i {
            Instr::Jump { target } | Instr::Call { target } => out.push(target),
            Instr::Choice { arms } => {
                for a in arms {
                    if let Some(t) = &a.target {
                        out.push(t);
                    }
                }
            }
            Instr::If { then_branch, else_branch, .. } => {
                collect_targets(then_branch, out);
                collect_targets(else_branch, out);
            }
            _ => {}
        }
    }
}

/// Gom mọi biến được dùng (cond + effect), đệ quy qua `if`.
fn collect_vars<'a>(body: &'a [Instr], out: &mut Vec<&'a str>) {
    for i in body {
        match i {
            Instr::Set { effect } => out.push(&effect.var),
            Instr::Choice { arms } => {
                for a in arms {
                    if let Some(c) = &a.cond {
                        out.push(&c.var);
                    }
                    for e in &a.effects {
                        out.push(&e.var);
                    }
                }
            }
            Instr::If { cond, then_branch, else_branch } => {
                out.push(&cond.var);
                collect_vars(then_branch, out);
                collect_vars(else_branch, out);
            }
            _ => {}
        }
    }
}

/// Bộ lint cốt truyện — chuyển thẳng các quy tắc đã kiểm chứng ở prototype v4
/// sang chạy trên IR (chính xác tuyệt đối, dùng được trong CI).
pub fn validate(story: &Story) -> Vec<Issue> {
    let mut issues = Vec::new();
    let ids: BTreeSet<&str> = story.nodes.iter().map(|n| n.id.as_str()).collect();

    if ids.len() != story.nodes.len() {
        issues.push(err(None, "co node trung id".into()));
    }
    if !ids.contains(story.start.as_str()) {
        issues.push(err(None, format!("diem bat dau '{}' khong ton tai", story.start)));
        return issues;
    }

    // Dich den phai ton tai + bien phai duoc khai bao.
    for n in &story.nodes {
        let mut targets = Vec::new();
        collect_targets(&n.body, &mut targets);
        for t in targets {
            if !ids.contains(t) {
                issues.push(err(Some(&n.id), format!("tro toi node '{t}' khong ton tai")));
            }
        }
        let mut vars = Vec::new();
        collect_vars(&n.body, &mut vars);
        for v in vars {
            if !story.variables.contains_key(v) {
                issues.push(err(Some(&n.id), format!("dung bien '{v}' chua khai bao")));
            }
        }
        // Soft-lock: choice ma moi arm deu co dieu kien.
        for i in &n.body {
            if let Instr::Choice { arms } = i {
                if !arms.is_empty() && arms.iter().all(|a| a.cond.is_some()) {
                    issues.push(warn(
                        Some(&n.id),
                        "moi lua chon deu co dieu kien — co the ket thuc dot ngot".into(),
                    ));
                }
                if arms.is_empty() {
                    issues.push(err(Some(&n.id), "lenh choice khong co lua chon nao".into()));
                }
            }
        }
    }

    // Kha nang tiep can tu diem bat dau (BFS).
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut queue: Vec<&str> = vec![story.start.as_str()];
    while let Some(id) = queue.pop() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(n) = story.nodes.iter().find(|n| n.id == id) {
            let mut targets = Vec::new();
            collect_targets(&n.body, &mut targets);
            for t in targets {
                if ids.contains(t) {
                    queue.push(t);
                }
            }
        }
    }
    for n in &story.nodes {
        if !seen.contains(n.id.as_str()) {
            issues.push(err(Some(&n.id), "nhanh mo coi: khong den duoc tu diem bat dau".into()));
        }
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use mong_core::{ChoiceArm, Cond, CondOp, Node, Value};
    use std::collections::BTreeMap;

    fn story_with(nodes: Vec<Node>, vars: &[(&str, i64)]) -> Story {
        let mut m = BTreeMap::new();
        for (k, v) in vars {
            m.insert(k.to_string(), Value::Int(*v));
        }
        Story {
            format_version: 0,
            title: String::new(),
            default_locale: "vi".into(),
            locales: vec![],
            variables: m,
            start: "a".into(),
            nodes,
        }
    }

    #[test]
    fn bat_dich_den_khong_ton_tai() {
        let s = story_with(
            vec![Node {
                id: "a".into(),
                title: String::new(),
                scene: None,
                body: vec![Instr::Jump { target: "ma".into() }],
            }],
            &[],
        );
        let iss = validate(&s);
        assert!(iss.iter().any(|i| i.severity == Severity::Error && i.message.contains("'ma'")));
    }

    #[test]
    fn bat_nhanh_mo_coi_va_bien_chua_khai_bao() {
        let s = story_with(
            vec![
                Node { id: "a".into(), title: String::new(), scene: None, body: vec![Instr::End] },
                Node {
                    id: "orphan".into(),
                    title: String::new(),
                    scene: None,
                    body: vec![Instr::If {
                        cond: Cond { var: "x".into(), op: CondOp::Ge, value: Value::Int(1) },
                        then_branch: vec![],
                        else_branch: vec![],
                    }],
                },
            ],
            &[],
        );
        let iss = validate(&s);
        assert!(iss.iter().any(|i| i.message.contains("mo coi")));
        assert!(iss.iter().any(|i| i.message.contains("'x'")));
    }

    #[test]
    fn canh_bao_soft_lock() {
        let s = story_with(
            vec![Node {
                id: "a".into(),
                title: String::new(),
                scene: None,
                body: vec![Instr::Choice {
                    arms: vec![ChoiceArm {
                        text: "t".into(),
                        target: None,
                        cond: Some(Cond { var: "tc".into(), op: CondOp::Ge, value: Value::Int(1) }),
                        effects: vec![],
                    }],
                }],
            }],
            &[("tc", 0)],
        );
        let iss = validate(&s);
        assert!(iss.iter().any(|i| i.severity == Severity::Warning));
    }
}
