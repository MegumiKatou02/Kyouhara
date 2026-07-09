//! mong-cli — công cụ dòng lệnh của Mộng Engine.
//!
//! M1: `run` (text-mode runner) và `lint`. M2: nhận `.mongscript`, thêm
//! `fmt` (chuẩn hoá + sinh key + xuất sidecar chuỗi), bảng chuỗi đi qua
//! mong-i18n (locale + fallback) thay cờ `--strings` cũ.
//! `new` / `pack` / `export` thêm ở các mốc sau.

use mong_assets::{read_pack, EntryKind};
use mong_core::{Story, Vm, VmEvent, VmStatus};
use mong_i18n::Catalog;
use mong_script::dsl;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

const USAGE: &str = "mong-cli — Mong Engine CLI

CACH DUNG:
  mong-cli run <file> [--locale <loc>]   choi truyen trong terminal
  mong-cli lint <file>                   kiem tra cot truyen
  mong-cli fmt <file> [--check]          chuan hoa file .mongscript

<file> la .mongscript, JSON du an, hoac goi .mongpack (nhan qua magic bytes).
Bang chuoi: van ban defaultLocale nam ngay trong .mongscript; cac locale khac
doc tu sidecar <ten>.strings.<loc>.json canh file (JSON/mongpack doc sidecar
cho ca defaultLocale). `fmt` ghi key #~ con thieu vao file va xuat sidecar
defaultLocale.
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
            let locale = match args.get(2).map(String::as_str) {
                Some("--locale") => Some(args.get(3).ok_or(USAGE)?.clone()),
                Some(_) => return Err(USAGE.into()),
                None => None,
            };
            cmd_run(path, locale.as_deref())?;
            Ok(ExitCode::SUCCESS)
        }
        Some("lint") => cmd_lint(args.get(1).ok_or(USAGE)?),
        Some("fmt") => {
            let path = args.get(1).ok_or(USAGE)?;
            let check = match args.get(2).map(String::as_str) {
                Some("--check") => true,
                Some(_) => return Err(USAGE.into()),
                None => false,
            };
            cmd_fmt(path, check)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Cốt truyện đã nạp, kèm những gì frontend DSL biết thêm.
struct Loaded {
    story: Story,
    /// Bảng chuỗi defaultLocale lấy thẳng từ văn bản DSL (rỗng với JSON/pack).
    default_strings: BTreeMap<String, String>,
    /// Số key DSL vừa sinh trong bộ nhớ — >0 nghĩa là file nguồn chưa có
    /// key bền vững, cần chạy `fmt`.
    generated_keys: usize,
}

fn load_input(path: &str) -> Result<Loaded, Box<dyn Error>> {
    if path.ends_with(".mongscript") {
        let src = fs::read_to_string(path)?;
        let out = dsl::load_story_dsl(&src).map_err(|e| format!("{path}: {e}"))?;
        return Ok(Loaded {
            story: out.story,
            default_strings: out.strings,
            generated_keys: out.generated_keys,
        });
    }
    Ok(Loaded {
        story: parse_story(&fs::read(path)?)?,
        default_strings: BTreeMap::new(),
        generated_keys: 0,
    })
}

/// Nhận cả hai định dạng nhị phân/văn bản: .mongpack (nhận qua magic)
/// hoặc JSON dự án. (.mongscript đi đường riêng vì cần cả bảng chuỗi.)
fn parse_story(bytes: &[u8]) -> Result<Story, Box<dyn Error>> {
    if bytes.starts_with(mong_assets::MAGIC) {
        let entries = read_pack(&mut &bytes[..])?;
        let e = entries
            .into_iter()
            .find(|e| e.kind == EntryKind::StoryIr)
            .ok_or("mongpack khong co entry story.ir")?;
        Ok(serde_json::from_slice(&e.data)?)
    } else {
        Ok(mong_script::load_story_json(std::str::from_utf8(bytes)?)?)
    }
}

/// Đường sidecar: `mot/duong/ten.mongscript` → `mot/duong/ten.strings.vi.json`.
fn sidecar_path(input: &str, locale: &str) -> PathBuf {
    let p = Path::new(input);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(input);
    p.with_file_name(format!("{stem}.strings.{locale}.json"))
}

/// Dựng Catalog: defaultLocale từ DSL (nếu có), mọi locale khai báo trong
/// Story đọc thêm từ sidecar. Sidecar vắng mặt không phải lỗi — fallback lo.
fn build_catalog(path: &str, loaded: &Loaded) -> Result<Catalog, Box<dyn Error>> {
    let story = &loaded.story;
    let mut cat = Catalog::new(story.default_locale.clone());
    if !loaded.default_strings.is_empty() {
        cat.set_table(story.default_locale.clone(), loaded.default_strings.clone());
    }
    let all = std::iter::once(&story.default_locale).chain(story.locales.iter());
    for loc in all {
        if cat.has_locale(loc) {
            continue; // defaultLocale tu DSL uu tien hon sidecar
        }
        let p = sidecar_path(path, loc);
        if p.exists() {
            let table: BTreeMap<String, String> = serde_json::from_str(&fs::read_to_string(&p)?)
                .map_err(|e| format!("{}: {e}", p.display()))?;
            cat.set_table(loc.clone(), table);
        }
    }
    Ok(cat)
}

fn nhac_fmt_neu_thieu_key(path: &str, generated: usize) {
    if generated > 0 {
        eprintln!(
            "canh bao: {generated} dong chua co key #~ (dang dung key tam trong bo nho).\n\
             Chay `mong-cli fmt {path}` de ghi key ben vung vao file."
        );
    }
}

fn cmd_lint(path: &str) -> Result<ExitCode, Box<dyn Error>> {
    let loaded = load_input(path)?;
    nhac_fmt_neu_thieu_key(path, loaded.generated_keys);
    let issues = mong_script::validate(&loaded.story);
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

fn cmd_fmt(path: &str, check: bool) -> Result<ExitCode, Box<dyn Error>> {
    if !path.ends_with(".mongscript") {
        return Err("`fmt` chi nhan file .mongscript".into());
    }
    let src = fs::read_to_string(path)?;
    let out = dsl::format_dsl(&src).map_err(|e| format!("{path}: {e}"))?;

    if check {
        return Ok(if out.text == src {
            println!("{path}: da o dang chuan.");
            ExitCode::SUCCESS
        } else {
            println!("{path}: can format (chay `mong-cli fmt {path}`).");
            ExitCode::FAILURE
        });
    }

    if out.text != src {
        fs::write(path, &out.text)?;
        println!(
            "{path}: da format{}.",
            if out.generated_keys > 0 {
                format!(", sinh {} key moi", out.generated_keys)
            } else {
                String::new()
            }
        );
    } else {
        println!("{path}: da o dang chuan.");
    }

    // Xuất sidecar chuỗi defaultLocale (spec-mongscript mục 7/11) — cần file
    // đủ ngữ nghĩa để compile; chưa đủ (vd thiếu @locale) thì bỏ qua có báo.
    match dsl::load_story_dsl(&out.text) {
        Ok(c) => {
            let p = sidecar_path(path, &c.story.default_locale);
            let json = serde_json::to_string_pretty(&c.strings)? + "\n";
            if fs::read_to_string(&p).ok().as_deref() != Some(json.as_str()) {
                fs::write(&p, json)?;
                println!(
                    "{}: da xuat bang chuoi {}.",
                    p.display(),
                    c.story.default_locale
                );
            }
        }
        Err(e) => eprintln!("chua xuat sidecar chuoi ({e})"),
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_run(path: &str, locale: Option<&str>) -> Result<(), Box<dyn Error>> {
    let loaded = load_input(path)?;
    nhac_fmt_neu_thieu_key(path, loaded.generated_keys);
    let issues = mong_script::validate(&loaded.story);
    if issues
        .iter()
        .any(|i| i.severity == mong_script::Severity::Error)
    {
        return Err("cot truyen co loi lint — chay `mong-cli lint` de xem chi tiet".into());
    }

    let catalog = build_catalog(path, &loaded)?;
    let story = loaded.story;
    let locale = match locale {
        Some(l) => {
            let known = l == story.default_locale || story.locales.iter().any(|x| x == l);
            if !known {
                let mut co = story.default_locale.clone();
                for x in &story.locales {
                    co.push_str(", ");
                    co.push_str(x);
                }
                return Err(format!("locale '{l}' khong co trong truyen (co: {co})").into());
            }
            l.to_string()
        }
        None => story.default_locale.clone(),
    };
    if !story.title.is_empty() {
        println!("=== {} ===", story.title);
    }

    let mut vm = Vm::new(story)?;
    let mut events = vm.start()?;
    let stdin = io::stdin();
    loop {
        for e in &events {
            render(e, &catalog, &locale);
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

fn render(ev: &VmEvent, c: &Catalog, loc: &str) {
    let t = |key: &str| c.text_or_key(loc, key).to_string();
    match ev {
        VmEvent::Say { speaker, text, .. } => match speaker {
            Some(sp) => println!("{sp}: {}", t(text)),
            None => println!("* {}", t(text)),
        },
        VmEvent::Choices { arms } => {
            println!();
            for a in arms {
                println!("  [{}] {}", a.index + 1, t(&a.text));
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

    #[test]
    fn sidecar_path_dung_quy_uoc() {
        assert_eq!(
            sidecar_path("a/b/demo-story.mongscript", "en"),
            PathBuf::from("a/b/demo-story.strings.en.json")
        );
        assert_eq!(
            sidecar_path("demo.mongscript", "vi"),
            PathBuf::from("demo.strings.vi.json")
        );
    }
}
