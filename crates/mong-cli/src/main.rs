//! mong-cli — công cụ dòng lệnh của Mộng Engine.
//!
//! M1: `run` (text-mode runner — DoD M1) và `lint`.
//! `new` / `pack` / `export` thêm ở các mốc sau.
//! Bảng chuỗi `--strings` là map phẳng tạm thời; mong-i18n thay thế ở M2.

use mong_assets::{read_pack, EntryKind};
use mong_core::{Story, Vm, VmEvent, VmStatus};
use mong_script::dsl::load_story_dsl;
use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::process::ExitCode;

type Strings = HashMap<String, String>;

const USAGE: &str = "mong-cli — Mong Engine CLI

CACH DUNG:
  mong-cli run <file> [--strings <file.json>]   choi truyen trong terminal
  mong-cli lint <file>                          kiem tra cot truyen

<file> la JSON du an hoac goi .mongpack (tu nhan qua magic bytes).
Trong luc choi: Enter = tiep | so = chon | z = lui mot buoc | q = thoat.";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run_cli(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("loi: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run_cli(args: &[String]) -> Result<ExitCode, Box<dyn Error>> {
    match args.first().map(String::as_str) {
        Some("run") => {
            let path = args.get(1).ok_or(USAGE)?;
            let strings = match args.get(2).map(String::as_str) {
                Some("--strings") => load_strings(args.get(3).ok_or(USAGE)?)?,
                Some(_) => return Err(USAGE.into()),
                None => Strings::new(),
            };
            cmd_run(path, &strings)?;
            Ok(ExitCode::SUCCESS)
        }
        Some("lint") => cmd_lint(args.get(1).ok_or(USAGE)?),
        _ => {
            eprintln!("{USAGE}");
            Ok(ExitCode::FAILURE)
        }
    }
}

struct LoadedStory {
    story: Story,
    strings: Strings,
}

fn load_story(path: &str) -> Result<LoadedStory, Box<dyn Error>> {
    let p = Path::new(path);
    match p.extension().and_then(|e| e.to_str()) {
        Some("mongscript") => {
            let src = fs::read_to_string(path)?;
            let out = load_story_dsl(&src)
                .map_err(|e| format!("{}", e))?;
            Ok(LoadedStory {
                strings: out.strings.into_iter().collect(),
                story: out.story,
            })
        }
        _ => load_story_inner(&fs::read(path)?),
    }
}

/// Nhận bytes → Story, dùng trong test.
fn parse_story(bytes: &[u8]) -> Result<Story, Box<dyn Error>> {
    Ok(load_story_inner(bytes)?.story)
}

/// Nhận bytes → Story + strings.
fn load_story_inner(bytes: &[u8]) -> Result<LoadedStory, Box<dyn Error>> {
    if bytes.starts_with(mong_assets::MAGIC) {
        let entries = read_pack(&mut &bytes[..])?;
        let e = entries
            .into_iter()
            .find(|e| e.kind == EntryKind::StoryIr)
            .ok_or("mongpack khong co entry story.ir")?;
        Ok(LoadedStory {
            story: serde_json::from_slice(&e.data)?,
            strings: Strings::new(),
        })
    } else {
        Ok(LoadedStory {
            story: mong_script::load_story_json(std::str::from_utf8(bytes)?)?,
            strings: Strings::new(),
        })
    }
}

fn load_strings(path: &str) -> Result<Strings, Box<dyn Error>> {
    Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
}

fn cmd_lint(path: &str) -> Result<ExitCode, Box<dyn Error>> {
    let story = load_story(path)?.story;
    let issues = mong_script::validate(&story);
    let mut has_error = false;
    for i in &issues {
        let tag = match i.severity {
            mong_script::Severity::Error => {
                has_error = true;
                "LOI"
            }
            mong_script::Severity::Warning => "CANH BAO",
        };
        match &i.node {
            Some(n) => println!("[{tag}] ({n}) {}", i.message),
            None => println!("[{tag}] {}", i.message),
        }
    }
    if issues.is_empty() {
        println!("khong phat hien van de nao.");
    }
    Ok(if has_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

fn cmd_run(path: &str, extra_strings: &Strings) -> Result<(), Box<dyn Error>> {
    let loaded = load_story(path)?;
    let story = loaded.story;
    let mut strings = loaded.strings;
    strings.extend(extra_strings.iter().map(|(k, v)| (k.clone(), v.clone())));
    let issues = mong_script::validate(&story);
    if issues
        .iter()
        .any(|i| i.severity == mong_script::Severity::Error)
    {
        return Err("cot truyen co loi lint — chay `mong-cli lint` de xem chi tiet".into());
    }
    if !story.title.is_empty() {
        println!("=== {} ===", story.title);
    }

    let mut vm = Vm::new(story)?;
    let mut events = vm.start()?;
    let stdin = io::stdin();
    loop {
        for e in &events {
            render(e, &strings);
        }
        // Core không tự hẹn giờ (spec-ir.md): text runner advance ngay sau Wait.
        let auto_wait = matches!(events.last(), Some(VmEvent::Wait { .. }));
        match vm.status() {
            VmStatus::Ended => return Ok(()),
            VmStatus::AwaitAdvance if auto_wait => events = vm.advance()?,
            VmStatus::AwaitAdvance => match prompt_advance(&stdin, &mut vm)? {
                Some(ev) => events = ev,
                None => return Ok(()),
            },
            VmStatus::AwaitChoice => {
                let n = events
                    .iter()
                    .rev()
                    .find_map(|e| match e {
                        VmEvent::Choices { arms } => Some(arms.len()),
                        _ => None,
                    })
                    .unwrap_or(0);
                match prompt_choice(&stdin, &mut vm, n)? {
                    Some(ev) => events = ev,
                    None => return Ok(()),
                }
            }
            VmStatus::Idle | VmStatus::Running => unreachable!("vm phai dung o trang thai cho"),
        }
    }
}

/// Đọc một dòng đã trim; `None` khi EOF.
fn read_line(stdin: &io::Stdin) -> io::Result<Option<String>> {
    print!("> ");
    io::stdout().flush()?;
    let mut s = String::new();
    if stdin.lock().read_line(&mut s)? == 0 {
        return Ok(None);
    }
    Ok(Some(s.trim().to_string()))
}

fn prompt_advance(stdin: &io::Stdin, vm: &mut Vm) -> Result<Option<Vec<VmEvent>>, Box<dyn Error>> {
    loop {
        let Some(line) = read_line(stdin)? else {
            return Ok(None);
        };
        match line.as_str() {
            "" => return Ok(Some(vm.advance()?)),
            "z" => match vm.rollback() {
                Some(replay) => return Ok(Some(replay)),
                None => println!("(khong con gi de lui)"),
            },
            "q" => return Ok(None),
            _ => println!("(Enter = tiep, z = lui, q = thoat)"),
        }
    }
}

fn prompt_choice(
    stdin: &io::Stdin,
    vm: &mut Vm,
    n: usize,
) -> Result<Option<Vec<VmEvent>>, Box<dyn Error>> {
    loop {
        let Some(line) = read_line(stdin)? else {
            return Ok(None);
        };
        match line.as_str() {
            "z" => match vm.rollback() {
                Some(replay) => return Ok(Some(replay)),
                None => println!("(khong con gi de lui)"),
            },
            "q" => return Ok(None),
            s => match s.parse::<usize>() {
                Ok(i) if (1..=n).contains(&i) => return Ok(Some(vm.choose(i - 1)?)),
                _ => println!("(nhap so 1..{n}, z = lui, q = thoat)"),
            },
        }
    }
}

fn t<'a>(strings: &'a Strings, key: &'a str) -> &'a str {
    strings.get(key).map(String::as_str).unwrap_or(key)
}

fn render(ev: &VmEvent, s: &Strings) {
    match ev {
        VmEvent::Say { speaker, text, .. } => match speaker {
            Some(sp) => println!("{sp}: {}", t(s, text)),
            None => println!("* {}", t(s, text)),
        },
        VmEvent::Choices { arms } => {
            println!();
            for a in arms {
                println!("  [{}] {}", a.index + 1, t(s, &a.text));
            }
        }
        VmEvent::SceneChanged { scene, transition } => match transition {
            Some(tr) => println!("\n--- canh: {scene} ({tr}) ---"),
            None => println!("\n--- canh: {scene} ---"),
        },
        VmEvent::Show {
            character,
            pose,
            pos,
        } => {
            println!(
                "[hien {character} ({}) @ {pos:?}]",
                pose.as_deref().unwrap_or("mac dinh")
            );
        }
        VmEvent::Hide { character } => println!("[an {character}]"),
        VmEvent::Sfx { asset } => println!("[sfx: {asset}]"),
        VmEvent::Bgm { asset } => match asset {
            Some(a) => println!("[bgm: {a}]"),
            None => println!("[bgm: tat]"),
        },
        VmEvent::Wait { ms } => println!("[cho {ms}ms — text-mode advance ngay]"),
        VmEvent::Ext { command, .. } => println!("[ext '{command}' — khong ai xu ly, bo qua]"),
        VmEvent::NodeEntered { .. } => {}
        VmEvent::Ended => println!("\n=== HET ==="),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mong_assets::{write_pack, PackEntry};

    const DEMO: &str = include_str!("../../../crates/mong-script/tests/data/demo-story.json");

    #[test]
    fn parse_story_nhan_ca_json_lan_mongpack() {
        let s1 = parse_story(DEMO.as_bytes()).unwrap();
        let ir = serde_json::to_vec(&s1).unwrap();
        let entries = vec![PackEntry {
            name: "story.ir".into(),
            kind: EntryKind::StoryIr,
            data: ir,
        }];
        let mut buf = Vec::new();
        write_pack(&mut buf, &entries).unwrap();
        let s2 = parse_story(&buf).unwrap();
        assert_eq!(s1, s2);
    }
}
