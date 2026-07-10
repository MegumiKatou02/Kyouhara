//! mong-cli — công cụ dòng lệnh của Mộng Engine.
//!
//! M1: `run` (text-mode runner) và `lint`. M2: nhận `.mongscript`, thêm
//! `fmt` (chuẩn hoá + sinh key + xuất sidecar chuỗi), bảng chuỗi đi qua
//! mong-i18n (locale + fallback) thay cờ `--strings` cũ.
//! `new` / `pack` / `export` thêm ở các mốc sau.

use mong_assets::{EntryKind, Manifest};
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
  mong-cli pack <thu_muc> [-o <file>]    dong goi du an -> .mongpack

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
        Some("pack") => {
            let dir = args.get(1).ok_or(USAGE)?;
            let out = match args.get(2).map(String::as_str) {
                Some("-o") => args.get(3).ok_or(USAGE)?.clone(),
                Some(_) => return Err(USAGE.into()),
                None => {
                    let name = Path::new(dir)
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("story");
                    format!("{name}.mongpack")
                }
            };
            cmd_pack(dir, &out)
        }
        _ => {
            eprintln!("{USAGE}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Thư mục dự án → một file .mongpack. Asset khai báo mà thiếu file thì từ
/// chối: gói hỏng phải lộ lúc build, không phải lúc người chơi mở.
fn cmd_pack(dir: &str, out: &str) -> Result<ExitCode, Box<dyn Error>> {
    let loaded = mong_project::load_dir(dir, None)?;
    let entries = mong_project::to_pack(&loaded)?;
    let bytes = {
        let mut b = Vec::new();
        mong_assets::write_pack(&mut b, &entries)?;
        b
    };
    fs::write(out, &bytes)?;

    // Tách "cốt truyện" khỏi "assets": ngân sách 5 MB gzip của DoD M4 tính
    // phần đầu, phần sau tải rời.
    let (mut logic, mut asset) = (0usize, 0usize);
    for e in &entries {
        match e.kind {
            EntryKind::Image | EntryKind::Audio | EntryKind::Font => asset += e.data.len(),
            _ => logic += e.data.len(),
        }
    }
    for e in &entries {
        println!("  {:<28} {:>9} B", e.name, e.data.len());
    }
    println!(
        "{out}: {} entry | logic {} B | assets {} B | goi {} B (nen {:.0}%)",
        entries.len(),
        logic,
        asset,
        bytes.len(),
        100.0 * bytes.len() as f64 / (logic + asset).max(1) as f64,
    );
    Ok(ExitCode::SUCCESS)
}

/// Một file script lẻ đã nạp — đường M1/M2, không manifest, không assets.
struct ScriptFile {
    story: Story,
    /// Bảng chuỗi defaultLocale lấy thẳng từ văn bản DSL (rỗng với JSON).
    default_strings: BTreeMap<String, String>,
    /// Số key DSL vừa sinh trong bộ nhớ — >0 nghĩa là file nguồn chưa có key
    /// bền vững, cần chạy `fmt`.
    generated_keys: usize,
}

fn load_script_file(path: &str) -> Result<ScriptFile, Box<dyn Error>> {
    if path.ends_with(".mongscript") {
        let src = fs::read_to_string(path)?;
        let out = dsl::load_story_dsl(&src).map_err(|e| format!("{path}: {e}"))?;
        return Ok(ScriptFile {
            story: out.story,
            default_strings: out.strings,
            generated_keys: out.generated_keys,
        });
    }
    // JSON dự án. `.mongpack` không tới đây nữa — nó là dự án, đi qua
    // `mong_project::load_pack` và mang theo cả manifest lẫn bảng chuỗi.
    Ok(ScriptFile {
        story: mong_script::load_story_json(&fs::read_to_string(path)?)?,
        default_strings: BTreeMap::new(),
        generated_keys: 0,
    })
}

/// Đường sidecar: `mot/duong/ten.mongscript` → `mot/duong/ten.strings.vi.json`.
fn sidecar_path(path: &str, locale: &str) -> PathBuf {
    let p = Path::new(path);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(path);
    p.with_file_name(format!("{stem}.strings.{locale}.json"))
}

/// Dựng Catalog: defaultLocale từ DSL (nếu có), mọi locale khai báo trong
/// Story đọc thêm từ sidecar. Sidecar vắng mặt không phải lỗi — fallback lo.
fn build_catalog(path: &str, file: &ScriptFile) -> Result<Catalog, Box<dyn Error>> {
    let story = &file.story;
    let mut cat = Catalog::new(story.default_locale.clone());
    if !file.default_strings.is_empty() {
        cat.set_table(story.default_locale.clone(), file.default_strings.clone());
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

fn warn_missing_keys(path: &str, generated: usize) {
    if generated > 0 {
        eprintln!(
            "canh bao: {generated} dong chua co key #~ (dang dung key tam trong bo nho).\n\
             Chay `mong-cli fmt {path}` de ghi key ben vung vao file."
        );
    }
}

fn cmd_lint(path: &str) -> Result<ExitCode, Box<dyn Error>> {
    let input = resolve_input(path)?;
    warn_missing_keys(path, input.generated_keys);
    let mut issues = mong_script::validate(&input.story);

    // Manifest chỉ có ở dự án. Đây là chỗ lẽ ra đã bắt được ba file .ogg
    // thiếu, thay vì để `pack` từ chối mãi về sau.
    if let Some(m) = &input.manifest {
        for msg in m.validate() {
            issues.push(mong_script::Issue {
                severity: mong_script::Severity::Warning,
                node: None,
                message: format!("manifest: {msg}"),
            });
        }
    }

    // Luật cần bảng chuỗi (docs/lint-rules.md L022–L024).
    let default_loc = &input.story.default_locale;
    if let Some(table) = input.content.table(default_loc) {
        issues.extend(mong_script::validate_strings(&input.story, table));
    }

    // L027 — locale khai báo nhưng thiếu bản dịch.
    for loc in &input.story.locales {
        let missing = input.content.missing_in(loc);
        if !missing.is_empty() {
            issues.push(mong_script::Issue {
                severity: mong_script::Severity::Warning,
                node: None,
                message: format!(
                    "locale '{loc}' thieu {} chuoi (vd: {})",
                    missing.len(),
                    missing
                        .iter()
                        .take(3)
                        .cloned()
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }
    // Sidecar defaultLocale lệch với văn bản trong .mongscript → đã cũ.
    if input.manifest.is_none() && path.ends_with(".mongscript") {
        let p = sidecar_path(path, default_loc);
        let on_disk: Option<BTreeMap<String, String>> = fs::read_to_string(&p)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok());
        if on_disk.is_some_and(|t| t != input.default_strings) {
            issues.push(mong_script::Issue {
                severity: mong_script::Severity::Warning,
                node: None,
                message: format!(
                    "{} da cu so voi file DSL — chay `mong-cli fmt {path}`",
                    p.display()
                ),
            });
        }
    }

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
    let input = resolve_input(path)?;
    warn_missing_keys(path, input.generated_keys);
    let issues = mong_script::validate(&input.story);
    if issues
        .iter()
        .any(|i| i.severity == mong_script::Severity::Error)
    {
        return Err("cot truyen co loi lint — chay `mong-cli lint` de xem chi tiet".into());
    }

    // `Vm::new` move `story`; tách trước để `catalog`/`manifest` sống tiếp.
    let Input {
        story,
        catalog,
        manifest,
        ..
    } = input;

    let locale = match locale {
        Some(l) => {
            let known = l == story.default_locale || story.locales.iter().any(|x| x == l);
            if !known {
                let mut available = story.default_locale.clone();
                for x in &story.locales {
                    available.push_str(", ");
                    available.push_str(x);
                }
                return Err(format!("locale '{l}' khong co trong truyen (co: {available})").into());
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
            render(e, manifest.as_ref(), &catalog, &locale);
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

fn render(ev: &VmEvent, manifest: Option<&Manifest>, c: &Catalog, loc: &str) {
    let t = |key: &str| c.text_or_key(loc, key).to_string();
    match ev {
        VmEvent::Say { speaker, text, .. } => match speaker {
            Some(sp) => println!("{}: {}", speaker_name(manifest, c, loc, sp), t(text)),
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

/// Đầu vào đã phân giải — cùng hình dạng cho cả dự án lẫn file lẻ.
struct Input {
    story: Story,
    /// Nội dung + metadata — dùng để **hiển thị**.
    catalog: Catalog,
    /// Chỉ miền nội dung — dùng để **lint**. Với file lẻ, hai cái trùng nhau.
    content: Catalog,
    /// `None` với file lẻ: không manifest thì tên nhân vật hiển thị là chính
    /// id, và các luật lint về manifest không chạy.
    manifest: Option<Manifest>,
    /// Chỉ có nghĩa với `.mongscript`: dùng để phát hiện sidecar đã cũ.
    default_strings: BTreeMap<String, String>,
    generated_keys: usize,
}

fn resolve_input(path: &str) -> Result<Input, Box<dyn Error>> {
    let from_project = |l: mong_project::Loaded| Input {
        catalog: l.catalog(),
        content: l.content_catalog(),
        story: l.story,
        manifest: Some(l.manifest),
        default_strings: BTreeMap::new(),
        generated_keys: 0,
    };
    if Path::new(path).is_dir() {
        return Ok(from_project(mong_project::load_dir(path, None)?));
    }
    if path.ends_with(".mongpack") {
        return Ok(from_project(mong_project::load_pack(
            &fs::read(path)?,
            None,
        )?));
    }
    let file = load_script_file(path)?;
    let catalog = build_catalog(path, &file)?;
    Ok(Input {
        story: file.story,
        content: catalog.clone(),
        catalog,
        manifest: None,
        default_strings: file.default_strings,
        generated_keys: file.generated_keys,
    })
}

/// Tên hiển thị của người nói. Không manifest, hoặc id lạ, thì trả chính id —
/// thà thấy `minh` còn hơn thấy khoảng trắng.
fn speaker_name<'a>(
    manifest: Option<&'a Manifest>,
    catalog: &'a Catalog,
    locale: &str,
    id: &'a str,
) -> &'a str {
    manifest
        .and_then(|m| m.characters.get(id))
        .map(|c| catalog.text_or_key(locale, &c.name))
        .unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Nợ M3 #5 / M4 #3: dự án thì hiện tên nhân vật, không hiện id.
    #[test]
    fn du_an_hien_ten_nhan_vat() {
        let input = resolve_input("../../examples/quan-ca-phe").unwrap();
        let m = input.manifest.as_ref();
        assert_eq!(speaker_name(m, &input.catalog, "vi", "minh"), "Minh");
        assert_eq!(
            speaker_name(m, &input.catalog, "vi", "khong_co"),
            "khong_co"
        );
    }

    /// File lẻ không có manifest — người nói hiện bằng id, đúng thiết kế.
    #[test]
    fn file_le_hien_id() {
        let p = "../mong-script/tests/data/demo-story.mongscript";
        let input = resolve_input(p).unwrap();
        assert!(input.manifest.is_none());
        assert_eq!(speaker_name(None, &input.catalog, "vi", "lan"), "lan");
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

    /// Key metadata (`char.*`, `scene.*`) không phải key nội dung. Lint đọc
    /// bảng đã hợp nhất thì báo mồ côi oan — hợp nhất là việc của lúc tra cứu.
    #[test]
    fn lint_du_an_khong_bao_key_metadata_mo_coi() {
        let input = resolve_input("../../examples/quan-ca-phe").unwrap();
        let table = input.content.table("vi").unwrap();
        assert!(!table.contains_key("char.lan"));
        assert!(input.catalog.table("vi").unwrap().contains_key("char.lan"));
    }
}
