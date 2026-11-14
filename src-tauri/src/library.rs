// 自作のライブラリの一覧と挿入（Phase 46。D-271）。
//
// - 一覧：アプリのフォルダの libraries/cpp/（ヘッダー）・libraries/ac-library/atcoder/（AC Library）・
//   libraries/rust/<クレート>/・libraries/python/ を読み、ファイル（Rust はクレート）ごとに、名前・説明（頭のコメントの 1 行目）・
//   中の名前（行の頭の書き方だけを見て拾う。文法は読まない）・入れる行を返す
// - 読むのは各フォルダの中だけ。外を指すシンボリックリンクはたどらない（docs/bug-review-2026-11-14.md の V-2 と同じ決め方）。
//   1 ファイル 1 MB・合わせて 1000 ファイル・深さ 8 まで
// - 入れる場所：`#include`・`import`・`use` の並びの最後の後ろ（なければ頭のコメントなどの後ろ）。同じ行がもうあれば入れない
// - Rust：問題の Cargo.toml の [dependencies] に `名前 = { path = "…" }` がなければ足す（画面が確かめてから）
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::language::Language;

// 1 ファイルの大きさの上限（これより大きいファイルは、一覧には出すが中を読まない）
const MAX_FILE_BYTES: u64 = 1024 * 1024;
// 合わせて読むファイルの数・フォルダの深さの上限
const MAX_FILES: usize = 1000;
const MAX_DEPTH: usize = 8;
// マウスを乗せたときに出す、頭の行の数
const HEAD_LINES: usize = 20;

const CPP_HEADERS: [&str; 4] = ["hpp", "h", "hh", "hxx"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Group {
    // 自作のライブラリ
    Own,
    // AC Library（C++）
    AcLibrary,
}

// 中の名前
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub name: String,
    // fn・struct・class・def など
    pub kind: String,
    // アプリのフォルダからの相対パス（区切りは `/`）と、行（1 始まり）
    pub path: String,
    pub line: usize,
    // この名前だけを入れる行（Rust の `use mylib::seg::SegTree;`）
    pub insert: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    pub language: Language,
    pub group: Group,
    // 見せる名前（C++ は libraries/cpp/ からの相対パス、Rust はクレートの名前、Python はモジュールの名前）
    pub name: String,
    // 開くファイル：アプリのフォルダからの相対パス（区切りは `/`）
    pub path: String,
    // AC Library は読み取り専用のタブで開くので、その file: の URI
    pub uri: Option<String>,
    pub description: Option<String>,
    pub items: Vec<Item>,
    // 入れる行（`#include "segtree.hpp"`・`import mylib`・`use mylib::*;`）
    pub insert: String,
    // Rust：Cargo.toml に足す依存の名前
    #[serde(rename = "crate")]
    pub krate: Option<String>,
    // 頭の数行（マウスを乗せたときに出す）
    pub head: String,
    // 大きすぎて中を読まなかった
    pub skipped: bool,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Libraries {
    pub libraries: Vec<Library>,
    // 上限に届いて、読まなかったものがある
    pub truncated: bool,
}

// ---- フォルダを辿る ----

struct Walker {
    count: usize,
    truncated: bool,
}

impl Walker {
    // root（canonicalize 済み）の下のファイルを、名前の順に返す。外を指すリンクと、隠しファイル・__pycache__・target は除く
    fn files(&mut self, root: &Path, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        if depth > MAX_DEPTH {
            return;
        }
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            if name.starts_with('.') || name == "__pycache__" || name == "target" {
                continue;
            }
            let Ok(real) = path.canonicalize() else {
                continue;
            };
            if !real.starts_with(root) {
                continue;
            }
            if real.is_dir() {
                self.files(root, &path, depth + 1, out);
            } else if real.is_file() {
                if self.count >= MAX_FILES {
                    self.truncated = true;
                    return;
                }
                self.count += 1;
                out.push(path);
            }
        }
    }
}

fn read_small(path: &Path) -> Option<String> {
    let size = fs::metadata(path).ok()?.len();
    if size > MAX_FILE_BYTES {
        return None;
    }
    String::from_utf8(fs::read(path).ok()?).ok()
}

fn head_of(text: &str) -> String {
    text.lines().take(HEAD_LINES).collect::<Vec<_>>().join("\n")
}

fn slash(path: &Path) -> String {
    path.components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/")
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_ident(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// 行の頭の名前（英数字と _）
fn leading_ident(text: &str) -> Option<String> {
    let name: String = text
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    is_ident(&name).then_some(name)
}

// ---- 説明（頭のコメントの 1 行目） ----

// C++・Rust：空行・`#pragma once`・`#![…]` を飛ばし、最初の `//`（`///`・`//!`）か `/* … */` の、文字のある最初の行
fn slash_comment_description(text: &str) -> Option<String> {
    let mut in_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if in_block {
            let body = trimmed
                .trim_end_matches("*/")
                .trim_start_matches('*')
                .trim();
            if !body.is_empty() {
                return Some(body.to_string());
            }
            if trimmed.contains("*/") {
                in_block = false;
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with("#pragma once") || trimmed.starts_with("#![") {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("//") {
            let body = rest.trim_start_matches(['/', '!']).trim();
            if body.is_empty() {
                continue;
            }
            return Some(body.to_string());
        }
        if let Some(rest) = trimmed.strip_prefix("/*") {
            let body = rest
                .trim_start_matches(['*', '!'])
                .trim_end_matches("*/")
                .trim();
            if !body.is_empty() {
                return Some(body.to_string());
            }
            in_block = !trimmed.contains("*/");
            continue;
        }
        return None;
    }
    None
}

// Python：`#!`・coding の行と空行を飛ばし、最初の `#` のコメントか、説明の文字列の、文字のある最初の行
fn python_description(text: &str) -> Option<String> {
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("#!")
            || trimmed.contains("coding:")
            || trimmed.contains("coding=")
        {
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix('#') {
            let body = rest.trim();
            if body.is_empty() {
                continue;
            }
            return Some(body.to_string());
        }
        let quote = ["\"\"\"", "'''", "r\"\"\"", "r'''"]
            .into_iter()
            .find(|q| trimmed.starts_with(q))?;
        let delimiter = &quote[quote.len() - 3..];
        let rest = trimmed[quote.len()..].trim();
        let first = rest.split(delimiter).next().unwrap_or("").trim();
        if !first.is_empty() {
            return Some(first.to_string());
        }
        if rest.contains(delimiter) {
            return None;
        }
        for line in lines.by_ref() {
            let body = line.trim();
            let text = body.split(delimiter).next().unwrap_or("").trim();
            if !text.is_empty() {
                return Some(text.to_string());
            }
            if body.contains(delimiter) {
                return None;
            }
        }
        return None;
    }
    None
}

// ---- 中の名前 ----

const CPP_NOT_FUNCTIONS: [&str; 8] = [
    "if", "for", "while", "switch", "return", "sizeof", "else", "do",
];

// C++：行の頭の `struct`・`class`（`template <…>` のあとも）と、行の頭の関数（`型 名前(`。`;` で終わる宣言も）
fn cpp_items(text: &str, path: &str) -> Vec<Item> {
    let mut items = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.starts_with([' ', '\t', '#', '/', '}', '{']) {
            continue;
        }
        let mut rest = line.trim_end();
        if let Some(after) = rest.strip_prefix("template") {
            // template <class T> struct X {
            match after.find('>') {
                Some(end) => rest = after[end + 1..].trim_start(),
                None => continue,
            }
        }
        for keyword in ["struct ", "class "] {
            if let Some(after) = rest.strip_prefix(keyword) {
                if let Some(name) = leading_ident(after.trim_start())
                    && !after.trim_end().ends_with(';')
                {
                    items.push(Item {
                        name,
                        kind: keyword.trim().to_string(),
                        path: path.to_string(),
                        line: index + 1,
                        insert: None,
                    });
                }
                rest = "";
            }
        }
        if rest.is_empty()
            || rest.starts_with("using ")
            || rest.starts_with("namespace ")
            || rest.starts_with("typedef ")
        {
            continue;
        }
        // 型 名前( … の形。名前は ( の直前の識別子
        let Some(open) = rest.find('(') else {
            continue;
        };
        let before = rest[..open].trim_end();
        if !before.contains([' ', '*', '&']) || before.contains('=') {
            continue;
        }
        let name: String = before
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        if is_ident(&name) && !CPP_NOT_FUNCTIONS.contains(&name.as_str()) && name != "main" {
            items.push(Item {
                name,
                kind: "関数".to_string(),
                path: path.to_string(),
                line: index + 1,
                insert: None,
            });
        }
    }
    items
}

// Rust：行の頭の `pub fn`・`pub struct`・`pub enum`・`pub trait`・`pub type`・`pub const`・`pub static`（`pub const fn` などは fn）と、
// `#[macro_export]` のついた `macro_rules!`（クレートの頭に出る）。module は `seg`・`a::b` の形（lib.rs なら空）
fn rust_items(text: &str, path: &str, krate: &str, module: &[String]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut previous = "";
    for (index, line) in text.lines().enumerate() {
        let mut found: Option<(String, String, bool)> = None;
        if let Some(mut rest) = line.strip_prefix("pub ") {
            loop {
                let trimmed = rest.trim_start();
                let next = ["unsafe ", "async ", "extern \"C\" "]
                    .into_iter()
                    .find_map(|q| trimmed.strip_prefix(q))
                    .or_else(|| {
                        trimmed.strip_prefix("const ").filter(|after| {
                            after.trim_start().starts_with("fn ")
                                || after.trim_start().starts_with("unsafe ")
                        })
                    });
                match next {
                    Some(after) => rest = after,
                    None => {
                        rest = trimmed;
                        break;
                    }
                }
            }
            for kind in [
                "fn", "struct", "enum", "trait", "type", "const", "static", "union",
            ] {
                if let Some(after) = rest.strip_prefix(kind).and_then(|a| a.strip_prefix(' '))
                    && let Some(name) = leading_ident(after.trim_start())
                {
                    found = Some((name, kind.to_string(), false));
                    break;
                }
            }
        } else if let Some(after) = line.strip_prefix("macro_rules!")
            && previous.trim() == "#[macro_export]"
            && let Some(name) = leading_ident(after.trim_start())
        {
            found = Some((name, "macro".to_string(), true));
        }
        if !line.trim().is_empty() {
            previous = line;
        }
        if let Some((name, kind, root)) = found {
            let mut segments = vec![krate.to_string()];
            if !root {
                segments.extend(module.iter().cloned());
            }
            segments.push(name.clone());
            items.push(Item {
                name,
                kind,
                path: path.to_string(),
                line: index + 1,
                insert: Some(format!("use {};", segments.join("::"))),
            });
        }
    }
    items
}

// Python：行の頭の `def`・`async def`・`class`（_ で始まるものは除く）
fn python_items(text: &str, path: &str) -> Vec<Item> {
    let mut items = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let found = [("def ", "def"), ("async def ", "def"), ("class ", "class")]
            .into_iter()
            .find_map(|(prefix, kind)| {
                Some((
                    leading_ident(line.strip_prefix(prefix)?.trim_start())?,
                    kind,
                ))
            });
        if let Some((name, kind)) = found
            && !name.starts_with('_')
        {
            items.push(Item {
                name,
                kind: kind.to_string(),
                path: path.to_string(),
                line: index + 1,
                insert: None,
            });
        }
    }
    items
}

// ---- 言語ごとの一覧 ----

fn canonical_dir(dir: &Path) -> Option<PathBuf> {
    dir.canonicalize().ok().filter(|d| d.is_dir())
}

fn cpp_libraries(app_dir: &Path, walker: &mut Walker) -> Vec<Library> {
    let base = app_dir.join(crate::cpp::LIBRARY_DIR);
    let Some(root) = canonical_dir(&base) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    walker.files(&root, &base, 0, &mut files);
    files
        .into_iter()
        .filter(|path| CPP_HEADERS.contains(&extension(path).as_str()))
        .filter_map(|path| {
            let relative = slash(path.strip_prefix(&base).ok()?);
            let app_path = format!("{}/{relative}", crate::cpp::LIBRARY_DIR);
            let text = read_small(&path);
            Some(Library {
                language: Language::Cpp,
                group: Group::Own,
                insert: format!("#include \"{relative}\""),
                name: relative,
                description: text.as_deref().and_then(slash_comment_description),
                items: text
                    .as_deref()
                    .map(|t| cpp_items(t, &app_path))
                    .unwrap_or_default(),
                head: text.as_deref().map(head_of).unwrap_or_default(),
                skipped: text.is_none(),
                path: app_path,
                uri: None,
                krate: None,
            })
        })
        .collect()
}

// AC Library：atcoder/ の直下の、拡張子のないヘッダー（`dsu` など。中の名前は dsu.hpp から読む）。なければ .hpp。internal_ は除く
fn ac_libraries(app_dir: &Path, walker: &mut Walker) -> Vec<Library> {
    let base = app_dir.join(crate::cpp::AC_LIBRARY_DIR).join("atcoder");
    let Some(root) = canonical_dir(&base) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    walker.files(&root, &base, MAX_DEPTH, &mut files);
    let names: Vec<String> = files
        .iter()
        .filter_map(|path| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();
    let has_wrappers = names.iter().any(|name| !name.contains('.'));
    files
        .iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            if name.starts_with("internal_") || name == "all" {
                return None;
            }
            let wrapper = !name.contains('.');
            if has_wrappers != wrapper || (!wrapper && extension(path) != "hpp") {
                return None;
            }
            let body = if wrapper {
                let hpp = path.with_file_name(format!("{name}.hpp"));
                if hpp.is_file() { hpp } else { path.clone() }
            } else {
                path.clone()
            };
            let real = body
                .canonicalize()
                .ok()
                .filter(|real| real.starts_with(&root))?;
            let file_name = body.file_name()?.to_string_lossy().into_owned();
            let app_path = format!("{}/atcoder/{file_name}", crate::cpp::AC_LIBRARY_DIR);
            let text = read_small(&real);
            let shown = name.trim_end_matches(".hpp").to_string();
            Some(Library {
                language: Language::Cpp,
                group: Group::AcLibrary,
                insert: format!("#include <atcoder/{name}>"),
                name: shown,
                description: None,
                items: text
                    .as_deref()
                    .map(|t| cpp_items(t, &app_path))
                    .unwrap_or_default(),
                head: text.as_deref().map(head_of).unwrap_or_default(),
                skipped: text.is_none(),
                path: app_path,
                uri: Some(crate::lsp::file_uri(&real)),
                krate: None,
            })
        })
        .collect()
}

// Rust のモジュールの名前（src/ からの相対。lib.rs は空、seg.rs は seg、a/mod.rs は a、a/b.rs は a::b）
fn rust_module(relative: &Path) -> Option<Vec<String>> {
    let mut parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let last = parts.pop()?;
    let stem = last.strip_suffix(".rs")?;
    if parts.is_empty() && (stem == "lib" || stem == "main") {
        return Some(Vec::new());
    }
    if stem != "mod" {
        parts.push(stem.to_string());
    }
    parts.iter().all(|p| is_ident(p)).then_some(parts)
}

fn rust_libraries(app_dir: &Path, walker: &mut Walker) -> Vec<Library> {
    let base = app_dir.join(crate::rust_bundle::LIBRARY_DIR);
    let Some(root) = canonical_dir(&base) else {
        return Vec::new();
    };
    let Ok(entries) = fs::read_dir(&base) else {
        return Vec::new();
    };
    let mut crates: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.canonicalize()
                .is_ok_and(|real| real.starts_with(&root) && real.join("Cargo.toml").is_file())
        })
        .collect();
    crates.sort();
    crates
        .into_iter()
        .filter_map(|dir| {
            let name = dir.file_name()?.to_string_lossy().into_owned();
            let krate = name.replace('-', "_");
            let src = dir.join("src");
            let mut files = Vec::new();
            if src.canonicalize().is_ok_and(|real| real.starts_with(&root)) {
                walker.files(&root, &src, 1, &mut files);
            }
            let lib = src.join("lib.rs");
            let lib_text = read_small(&lib);
            let mut items = Vec::new();
            let mut skipped = false;
            for file in files.iter().filter(|f| extension(f) == "rs") {
                let Some(module) = file.strip_prefix(&src).ok().and_then(rust_module) else {
                    continue;
                };
                let Some(text) = read_small(file) else {
                    skipped = true;
                    continue;
                };
                let app_path = format!(
                    "{}/{}",
                    crate::rust_bundle::LIBRARY_DIR,
                    slash(file.strip_prefix(&base).ok()?)
                );
                items.extend(rust_items(&text, &app_path, &krate, &module));
            }
            let description = lib_text
                .as_deref()
                .and_then(slash_comment_description)
                .or_else(|| {
                    read_small(&dir.join("Cargo.toml")).and_then(|toml| {
                        toml.lines().find_map(|line| {
                            let value = line
                                .trim()
                                .strip_prefix("description")?
                                .trim_start()
                                .strip_prefix('=')?;
                            Some(value.trim().trim_matches('"').to_string())
                        })
                    })
                });
            Some(Library {
                language: Language::Rust,
                group: Group::Own,
                insert: format!("use {krate}::*;"),
                path: format!("{}/{name}/src/lib.rs", crate::rust_bundle::LIBRARY_DIR),
                head: lib_text.as_deref().map(head_of).unwrap_or_default(),
                skipped: skipped || lib_text.is_none(),
                description,
                items,
                krate: Some(name.clone()),
                name,
                uri: None,
            })
        })
        .collect()
}

// Python のモジュールの名前（libraries/python/ からの相対。mylib.py は mylib、mylib/__init__.py は mylib、mylib/seg.py は mylib.seg）
fn python_module(relative: &Path) -> Option<Vec<String>> {
    let mut parts: Vec<String> = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    let last = parts.pop()?;
    let stem = last.strip_suffix(".py")?;
    if stem != "__init__" {
        parts.push(stem.to_string());
    }
    (!parts.is_empty() && parts.iter().all(|p| is_ident(p))).then_some(parts)
}

fn python_libraries(app_dir: &Path, walker: &mut Walker) -> Vec<Library> {
    let base = app_dir.join(crate::py_bundle::LIBRARY_DIR);
    let Some(root) = canonical_dir(&base) else {
        return Vec::new();
    };
    let mut files = Vec::new();
    walker.files(&root, &base, 0, &mut files);
    files
        .into_iter()
        .filter(|path| extension(path) == "py")
        .filter_map(|path| {
            let relative = path.strip_prefix(&base).ok()?;
            let module = python_module(relative)?;
            let app_path = format!("{}/{}", crate::py_bundle::LIBRARY_DIR, slash(relative));
            let text = read_small(&path);
            let insert = match module.split_last() {
                Some((last, parents)) if !parents.is_empty() => {
                    format!("from {} import {last}", parents.join("."))
                }
                _ => format!("import {}", module.join(".")),
            };
            Some(Library {
                language: Language::Python,
                group: Group::Own,
                name: module.join("."),
                insert,
                description: text.as_deref().and_then(python_description),
                items: text
                    .as_deref()
                    .map(|t| python_items(t, &app_path))
                    .unwrap_or_default(),
                head: text.as_deref().map(head_of).unwrap_or_default(),
                skipped: text.is_none(),
                path: app_path,
                uri: None,
                krate: None,
            })
        })
        .collect()
}

// 同じ名前（オーバーロード・特殊化・宣言と定義）は、最初の 1 つだけにする
fn dedup_items(items: &mut Vec<Item>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert((item.kind.clone(), item.name.clone(), item.insert.clone())));
}

// アプリのフォルダの、すべての言語のライブラリ
pub fn list(app_dir: &Path) -> Libraries {
    let mut walker = Walker {
        count: 0,
        truncated: false,
    };
    let mut libraries = cpp_libraries(app_dir, &mut walker);
    libraries.extend(ac_libraries(app_dir, &mut walker));
    libraries.extend(rust_libraries(app_dir, &mut walker));
    libraries.extend(python_libraries(app_dir, &mut walker));
    for library in &mut libraries {
        dedup_items(&mut library.items);
    }
    Libraries {
        libraries,
        truncated: walker.truncated,
    }
}

// アプリのフォルダからの相対パス（libraries/cpp/…・libraries/rust/…・libraries/python/…）を、タブで開いて書けるファイルとして確かめ、
// 場所（canonicalize 済み）を返す。AC Library・ほかの場所・外を指すリンク・`..`・ソースでない拡張子は拒む
pub fn locate_file(app_dir: &Path, name: &str) -> Option<PathBuf> {
    let (language_dir, extensions): (&str, &[&str]) =
        if name.starts_with(&format!("{}/", crate::cpp::LIBRARY_DIR)) {
            (
                crate::cpp::LIBRARY_DIR,
                &["hpp", "h", "hh", "hxx", "cpp", "cc"],
            )
        } else if name.starts_with(&format!("{}/", crate::rust_bundle::LIBRARY_DIR)) {
            (crate::rust_bundle::LIBRARY_DIR, &["rs", "toml"])
        } else if name.starts_with(&format!("{}/", crate::py_bundle::LIBRARY_DIR)) {
            (crate::py_bundle::LIBRARY_DIR, &["py"])
        } else {
            return None;
        };
    let path = Path::new(name);
    if path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        || !extensions.contains(&extension(path).as_str())
    {
        return None;
    }
    let root = app_dir.join(language_dir).canonicalize().ok()?;
    let real = app_dir.join(path).canonicalize().ok()?;
    (real.starts_with(&root) && real.is_file()).then_some(real)
}

// ---- 入れる場所 ----

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsertPlan {
    // 同じ行がもうある
    pub already: bool,
    // この行（0 始まり）の前に入れる。行の数と同じなら最後
    pub line: usize,
}

fn same_include(line: &str, wanted: &str) -> bool {
    let squash = |text: &str| text.split_whitespace().collect::<String>();
    squash(line) == squash(wanted)
}

// 並びの頭の、飛ばしてよい行（空行・コメント）
fn is_comment(trimmed: &str, language: Language) -> bool {
    trimmed.is_empty()
        || match language {
            Language::Python => trimmed.starts_with('#'),
            Language::Rust | Language::Cpp => {
                trimmed.starts_with("//") || trimmed.starts_with("/*") || trimmed.starts_with('*')
            }
        }
}

// text のどこに wanted（1 行）を入れるか
pub fn plan_insert(language: Language, text: &str, wanted: &str) -> InsertPlan {
    let lines: Vec<&str> = text.lines().collect();
    let wanted = wanted.trim();
    if lines.iter().any(|line| same_include(line, wanted)) {
        return InsertPlan {
            already: true,
            line: 0,
        };
    }
    let line = match language {
        Language::Cpp => cpp_insert_line(&lines),
        Language::Rust => rust_insert_line(&lines),
        Language::Python => python_insert_line(&lines),
    };
    InsertPlan {
        already: false,
        line,
    }
}

// C++：最初の `{` を含む行より前の、最後の `#include` の次。なければ、頭のコメント・`#pragma once` の次
fn cpp_insert_line(lines: &[&str]) -> usize {
    let mut last_include = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("#include") {
            last_include = Some(index);
        } else if trimmed.contains('{') && !trimmed.starts_with("//") {
            break;
        }
    }
    if let Some(index) = last_include {
        return index + 1;
    }
    let mut line = 0;
    for (index, text) in lines.iter().enumerate() {
        let trimmed = text.trim();
        if trimmed.starts_with("#pragma once")
            || (is_comment(trimmed, Language::Cpp) && !trimmed.is_empty())
        {
            line = index + 1;
        } else if !trimmed.is_empty() {
            break;
        }
    }
    line
}

// Rust：行の頭の `use`（複数の行にわたるものは `;` まで）の並びの最後の次。なければ、頭の `//!`・`#![…]`・コメントの次
fn rust_insert_line(lines: &[&str]) -> usize {
    let mut last_use = None;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        if line.starts_with("use ") || line.starts_with("pub use ") {
            let mut end = index;
            while end < lines.len() && !lines[end].trim_end().ends_with(';') {
                end += 1;
            }
            last_use = Some(end.min(lines.len() - 1));
            index = end + 1;
            continue;
        }
        let trimmed = line.trim();
        if !trimmed.is_empty()
            && !is_comment(trimmed, Language::Rust)
            && !trimmed.starts_with("#")
            && !line.starts_with("mod ")
        {
            break;
        }
        index += 1;
    }
    if let Some(end) = last_use {
        return end + 1;
    }
    let mut line = 0;
    for (index, text) in lines.iter().enumerate() {
        let trimmed = text.trim();
        if trimmed.starts_with("#![")
            || (is_comment(trimmed, Language::Rust) && !trimmed.is_empty())
        {
            line = index + 1;
        } else if !trimmed.is_empty() {
            break;
        }
    }
    line
}

// Python：行の頭の `import`・`from … import`（かっこで複数の行にわたるものは閉じるまで）の並びの最後の次。
// なければ、頭のコメント・説明の文字列・`from __future__` の次
fn python_insert_line(lines: &[&str]) -> usize {
    let mut last_import = None;
    let mut after_head = 0;
    let mut index = 0;
    let mut seen_code = false;
    while index < lines.len() {
        let line = lines[index];
        let trimmed = line.trim();
        if line.starts_with("import ") || line.starts_with("from ") {
            let mut end = index;
            if line.contains('(') && !line.contains(')') {
                while end + 1 < lines.len() && !lines[end].contains(')') {
                    end += 1;
                }
            }
            while end + 1 < lines.len() && lines[end].trim_end().ends_with('\\') {
                end += 1;
            }
            last_import = Some(end);
            index = end + 1;
            continue;
        }
        if is_comment(trimmed, Language::Python) {
            if !seen_code && !trimmed.is_empty() {
                after_head = index + 1;
            }
            index += 1;
            continue;
        }
        // 頭の説明の文字列
        if !seen_code
            && last_import.is_none()
            && (trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''"))
        {
            let delimiter = &trimmed[..3];
            let mut end = index;
            if trimmed.len() < 6 || !trimmed[3..].contains(delimiter) {
                end += 1;
                while end < lines.len() && !lines[end].contains(delimiter) {
                    end += 1;
                }
            }
            seen_code = true;
            after_head = (end + 1).min(lines.len());
            index = end + 1;
            continue;
        }
        break;
    }
    match last_import {
        Some(end) => end + 1,
        None => after_head,
    }
}

// ---- Cargo.toml ----

// 自作のライブラリのクレートの、写して使う依存の行（設定の画面の「テンプレート」に出している行と同じ。rust_bundle::dependency_lines）
pub fn dependency_line(app_dir: &Path, krate: &str) -> Option<String> {
    let dir = app_dir.join(crate::rust_bundle::LIBRARY_DIR).join(krate);
    if !is_crate_name(krate) || !dir.join("Cargo.toml").is_file() {
        return None;
    }
    let quoted = serde_json::to_string(&dir.to_string_lossy()).ok()?;
    Some(format!("{krate} = {{ path = {quoted} }}"))
}

fn is_crate_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

// manifest（Cargo.toml の中身）の [dependencies] に、krate があるか
pub fn has_dependency(manifest: &str, krate: &str) -> bool {
    let mut in_dependencies = false;
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let header = trimmed.trim_start_matches('[').trim_end_matches(']').trim();
            if header == format!("dependencies.{krate}") {
                return true;
            }
            in_dependencies = header == "dependencies";
            continue;
        }
        if in_dependencies
            && let Some((key, _)) = trimmed.split_once('=')
            && key.trim().trim_matches('"') == krate
        {
            return true;
        }
    }
    false
}

// manifest の [dependencies] の最後の行の次に line を足したもの（[dependencies] がなければ、最後に作る）
pub fn add_dependency(manifest: &str, line: &str) -> String {
    let lines: Vec<&str> = manifest.lines().collect();
    let header = lines.iter().position(|l| l.trim() == "[dependencies]");
    let Some(header) = header else {
        let mut out = manifest.trim_end().to_string();
        if !out.is_empty() {
            out.push_str("\n\n");
        }
        out.push_str("[dependencies]\n");
        out.push_str(line);
        out.push('\n');
        return out;
    };
    // 節の終わり（次の [ の行の前）の、最後の空でない行
    let mut last = header;
    for (index, l) in lines.iter().enumerate().skip(header + 1) {
        let trimmed = l.trim();
        if trimmed.starts_with('[') {
            break;
        }
        if !trimmed.is_empty() {
            last = index;
        }
    }
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    out.insert(last + 1, line.to_string());
    let mut text = out.join("\n");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().canonicalize().unwrap().join(format!(
                "atcoder-studio-library-{name}-{}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }

        fn write(&self, relative: &str, content: &str) {
            let path = self.0.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn find<'a>(libraries: &'a Libraries, name: &str) -> &'a Library {
        libraries
            .libraries
            .iter()
            .find(|library| library.name == name)
            .unwrap_or_else(|| panic!("{name}：{libraries:?}"))
    }

    fn names(items: &[Item]) -> Vec<&str> {
        items.iter().map(|item| item.name.as_str()).collect()
    }

    #[test]
    fn lists_libraries_of_each_language() {
        let dir = TempDir::new("list");
        dir.write(
            "libraries/cpp/segtree.hpp",
            "#pragma once\n// セグメント木（一点更新・区間取得）\n#include <vector>\ntemplate <class S, S (*op)(S, S)>\nstruct SegTree {\n  int size() const;\n};\nlong long power(long long a, long long b) {\n  if (b == 0) return 1;\n}\nint power(int a, int b);\nstruct Fwd;\n",
        );
        dir.write(
            "libraries/cpp/graph/dijkstra.hpp",
            "/* ダイクストラ法 */\nstd::vector<long long> dijkstra(const Graph& g, int s);\n",
        );
        dir.write("libraries/cpp/notes.txt", "x");
        dir.write(
            "libraries/ac-library/atcoder/dsu",
            "#include \"atcoder/dsu.hpp\"\n",
        );
        dir.write(
            "libraries/ac-library/atcoder/dsu.hpp",
            "namespace atcoder {\nstruct dsu {\n};\n}\n",
        );
        dir.write("libraries/ac-library/atcoder/internal_math", "");
        dir.write("libraries/ac-library/atcoder/internal_math.hpp", "");
        dir.write(
            "libraries/rust/my-lib/Cargo.toml",
            "[package]\nname = \"my-lib\"\ndescription = \"使わない説明\"\n",
        );
        dir.write(
            "libraries/rust/my-lib/src/lib.rs",
            "//! 自作のライブラリ\npub mod seg;\npub fn gcd(a: u64, b: u64) -> u64 { a }\n#[macro_export]\nmacro_rules! chmin { () => {} }\nfn private() {}\n",
        );
        dir.write(
            "libraries/rust/my-lib/src/seg.rs",
            "pub struct SegTree;\nimpl SegTree {\n    pub fn new() -> Self { SegTree }\n}\npub const fn id() -> u32 { 0 }\npub(crate) fn hidden() {}\n",
        );
        dir.write(
            "libraries/rust/my-lib/src/a/mod.rs",
            "pub trait Monoid {}\n",
        );
        dir.write(
            "libraries/rust/other/Cargo.toml",
            "[package]\nname = \"other\"\ndescription = \"説明は Cargo.toml から\"\n",
        );
        dir.write("libraries/rust/other/src/lib.rs", "pub enum E {}\n");
        dir.write("libraries/rust/not-a-crate/src/lib.rs", "pub fn x() {}\n");
        dir.write(
            "libraries/python/mylib/__init__.py",
            "\"\"\"\n自作のライブラリ\n\"\"\"\n",
        );
        dir.write(
            "libraries/python/mylib/seg.py",
            "# セグメント木\nclass SegTree:\n    def get(self):\n        pass\n\ndef build(a):\n    pass\n\ndef _helper():\n    pass\n",
        );
        dir.write(
            "libraries/python/util.py",
            "#!/usr/bin/env python3\n\"\"\"便利な関数\"\"\"\nasync def run():\n    pass\n",
        );
        dir.write("libraries/python/__pycache__/util.cpython-311.py", "");
        dir.write("libraries/python/bad-name.py", "");

        let libraries = list(&dir.0);
        assert!(!libraries.truncated);

        let seg = find(&libraries, "segtree.hpp");
        assert_eq!(seg.language, Language::Cpp);
        assert_eq!(seg.group, Group::Own);
        assert_eq!(seg.insert, "#include \"segtree.hpp\"");
        assert_eq!(seg.path, "libraries/cpp/segtree.hpp");
        assert_eq!(
            seg.description.as_deref(),
            Some("セグメント木（一点更新・区間取得）")
        );
        assert_eq!(names(&seg.items), vec!["SegTree", "power"]);
        assert_eq!(seg.items[0].line, 5);
        assert!(seg.head.starts_with("#pragma once\n"));
        let dijkstra = find(&libraries, "graph/dijkstra.hpp");
        assert_eq!(dijkstra.insert, "#include \"graph/dijkstra.hpp\"");
        assert_eq!(dijkstra.description.as_deref(), Some("ダイクストラ法"));
        assert_eq!(names(&dijkstra.items), vec!["dijkstra"]);
        assert!(!libraries.libraries.iter().any(|l| l.name.contains("notes")));

        let dsu = find(&libraries, "dsu");
        assert_eq!(dsu.group, Group::AcLibrary);
        assert_eq!(dsu.insert, "#include <atcoder/dsu>");
        assert_eq!(dsu.path, "libraries/ac-library/atcoder/dsu.hpp");
        assert_eq!(names(&dsu.items), vec!["dsu"]);
        let uri = dsu.uri.as_deref().unwrap();
        assert!(
            uri.starts_with("file:///") && uri.ends_with("atcoder/dsu.hpp"),
            "{uri}"
        );
        assert!(
            !libraries
                .libraries
                .iter()
                .any(|l| l.name.starts_with("internal_"))
        );

        let mylib = find(&libraries, "my-lib");
        assert_eq!(mylib.language, Language::Rust);
        assert_eq!(mylib.insert, "use my_lib::*;");
        assert_eq!(mylib.krate.as_deref(), Some("my-lib"));
        assert_eq!(mylib.path, "libraries/rust/my-lib/src/lib.rs");
        assert_eq!(mylib.description.as_deref(), Some("自作のライブラリ"));
        let inserts: Vec<(&str, &str)> = mylib
            .items
            .iter()
            .map(|item| (item.name.as_str(), item.insert.as_deref().unwrap()))
            .collect();
        assert_eq!(
            inserts,
            vec![
                ("Monoid", "use my_lib::a::Monoid;"),
                ("gcd", "use my_lib::gcd;"),
                ("chmin", "use my_lib::chmin;"),
                ("SegTree", "use my_lib::seg::SegTree;"),
                ("id", "use my_lib::seg::id;"),
            ]
        );
        assert_eq!(mylib.items[3].path, "libraries/rust/my-lib/src/seg.rs");
        assert_eq!(
            find(&libraries, "other").description.as_deref(),
            Some("説明は Cargo.toml から")
        );
        assert!(!libraries.libraries.iter().any(|l| l.name == "not-a-crate"));

        let package = find(&libraries, "mylib");
        assert_eq!(package.insert, "import mylib");
        assert_eq!(package.description.as_deref(), Some("自作のライブラリ"));
        let seg = find(&libraries, "mylib.seg");
        assert_eq!(seg.insert, "from mylib import seg");
        assert_eq!(seg.description.as_deref(), Some("セグメント木"));
        assert_eq!(names(&seg.items), vec!["SegTree", "build"]);
        let util = find(&libraries, "util");
        assert_eq!(util.insert, "import util");
        assert_eq!(util.description.as_deref(), Some("便利な関数"));
        assert_eq!(names(&util.items), vec!["run"]);
        assert!(
            !libraries
                .libraries
                .iter()
                .any(|l| l.name.contains("cpython") || l.name.contains("bad"))
        );
    }

    #[test]
    fn does_not_follow_links_outside_or_read_large_files() {
        let dir = TempDir::new("links");
        let outside = TempDir::new("links-outside");
        outside.write("secret.hpp", "// 外のファイル\nstruct Secret {};\n");
        outside.write("lib/inner.hpp", "struct Inner {};\n");
        dir.write("libraries/cpp/own.hpp", "struct Own {};\n");
        std::os::unix::fs::symlink(
            outside.0.join("secret.hpp"),
            dir.0.join("libraries/cpp/secret.hpp"),
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.0.join("lib"), dir.0.join("libraries/cpp/lib")).unwrap();
        // 中を指すリンクはたどる
        std::os::unix::fs::symlink(
            dir.0.join("libraries/cpp/own.hpp"),
            dir.0.join("libraries/cpp/alias.hpp"),
        )
        .unwrap();
        dir.write(
            "libraries/cpp/big.hpp",
            &format!("// 大きい\n{}", "x".repeat(MAX_FILE_BYTES as usize)),
        );
        let libraries = list(&dir.0);
        let shown: Vec<&str> = libraries
            .libraries
            .iter()
            .map(|l| l.name.as_str())
            .collect();
        assert_eq!(shown, vec!["alias.hpp", "big.hpp", "own.hpp"]);
        let big = find(&libraries, "big.hpp");
        assert!(big.skipped && big.items.is_empty() && big.description.is_none());

        // タブで開けるファイル
        assert!(locate_file(&dir.0, "libraries/cpp/own.hpp").is_some());
        assert!(locate_file(&dir.0, "libraries/cpp/alias.hpp").is_some());
        for name in [
            "libraries/cpp/secret.hpp",
            "libraries/cpp/lib/inner.hpp",
            "libraries/cpp/../cpp/own.hpp",
            "libraries/cpp/missing.hpp",
            "libraries/ac-library/atcoder/dsu.hpp",
            "settings.json",
            "/etc/hosts",
            "libraries/cpp/notes.txt",
        ] {
            assert!(locate_file(&dir.0, name).is_none(), "{name}");
        }
    }

    #[test]
    fn stops_at_the_file_limit() {
        let dir = TempDir::new("limit");
        for i in 0..MAX_FILES + 5 {
            dir.write(&format!("libraries/python/m{i:04}.py"), "");
        }
        let libraries = list(&dir.0);
        assert!(libraries.truncated);
        assert_eq!(libraries.libraries.len(), MAX_FILES);
    }

    fn insert(language: Language, text: &str, wanted: &str) -> String {
        let plan = plan_insert(language, text, wanted);
        assert!(!plan.already, "{text}");
        let mut lines: Vec<&str> = text.lines().collect();
        lines.insert(plan.line, wanted);
        lines.join("\n")
    }

    #[test]
    fn places_include_lines() {
        let text = "#include <bits/stdc++.h>\n#include \"debug.hpp\"\nusing namespace std;\n\nint main() {\n#include \"inner.hpp\"\n}";
        assert_eq!(
            insert(Language::Cpp, text, "#include \"segtree.hpp\""),
            "#include <bits/stdc++.h>\n#include \"debug.hpp\"\n#include \"segtree.hpp\"\nusing namespace std;\n\nint main() {\n#include \"inner.hpp\"\n}"
        );
        // 同じ行（空白の違いは見ない）
        assert!(plan_insert(Language::Cpp, text, "#include  \"debug.hpp\"").already);
        // #include がなければ、頭のコメントの次
        assert_eq!(
            insert(
                Language::Cpp,
                "// 解答\n\nint main() {}",
                "#include <atcoder/dsu>"
            ),
            "// 解答\n#include <atcoder/dsu>\n\nint main() {}"
        );
        assert_eq!(
            insert(Language::Cpp, "", "#include \"a.hpp\""),
            "#include \"a.hpp\""
        );
    }

    #[test]
    fn places_use_lines() {
        let text = "use proconio::input;\nuse std::collections::{\n    HashMap,\n};\n\nfn main() {\n    use inner::x;\n}";
        assert_eq!(
            insert(Language::Rust, text, "use mylib::*;"),
            "use proconio::input;\nuse std::collections::{\n    HashMap,\n};\nuse mylib::*;\n\nfn main() {\n    use inner::x;\n}"
        );
        assert!(plan_insert(Language::Rust, text, "use proconio::input;").already);
        // use がなければ、頭の //!・#![…] の次
        assert_eq!(
            insert(
                Language::Rust,
                "//! 解答\n#![allow(unused)]\n\nfn main() {}",
                "use mylib::*;"
            ),
            "//! 解答\n#![allow(unused)]\nuse mylib::*;\n\nfn main() {}"
        );
        // mod の行を挟んだ use の並び
        assert_eq!(
            insert(
                Language::Rust,
                "mod lib;\nuse lib::a;\nfn main() {}",
                "use mylib::*;"
            ),
            "mod lib;\nuse lib::a;\nuse mylib::*;\nfn main() {}"
        );
    }

    #[test]
    fn places_import_lines() {
        let text =
            "import sys\nfrom collections import (\n    deque,\n)\n\ndef main():\n    import os\n";
        assert_eq!(
            insert(Language::Python, text, "import mylib"),
            "import sys\nfrom collections import (\n    deque,\n)\nimport mylib\n\ndef main():\n    import os"
        );
        assert!(plan_insert(Language::Python, text, "import sys").already);
        // import がなければ、説明の文字列とコメントの次
        assert_eq!(
            insert(
                Language::Python,
                "#!/usr/bin/env python3\n\"\"\"\n解答\n\"\"\"\n\ndef main():\n    pass",
                "import mylib"
            ),
            "#!/usr/bin/env python3\n\"\"\"\n解答\n\"\"\"\nimport mylib\n\ndef main():\n    pass"
        );
        assert_eq!(
            insert(
                Language::Python,
                "from __future__ import annotations\nprint(1)",
                "import mylib"
            ),
            "from __future__ import annotations\nimport mylib\nprint(1)"
        );
        assert_eq!(
            insert(Language::Python, "print(1)", "import mylib"),
            "import mylib\nprint(1)"
        );
    }

    #[test]
    fn adds_dependencies_to_cargo_toml() {
        let manifest = "[package]\nname = \"abc\"\n\n[dependencies]\nproconio = \"0.4\"\n\n[profile.release]\nopt-level = 3\n";
        assert!(has_dependency(manifest, "proconio"));
        assert!(!has_dependency(manifest, "mylib"));
        assert!(!has_dependency(manifest, "opt-level"));
        let added = add_dependency(manifest, "mylib = { path = \"/x/mylib\" }");
        assert_eq!(
            added,
            "[package]\nname = \"abc\"\n\n[dependencies]\nproconio = \"0.4\"\nmylib = { path = \"/x/mylib\" }\n\n[profile.release]\nopt-level = 3\n"
        );
        assert!(has_dependency(&added, "mylib"));
        assert!(has_dependency(
            "[dependencies.mylib]\npath = \"x\"\n",
            "mylib"
        ));
        assert!(has_dependency(
            "[dependencies]\n\"mylib\" = { path = \"x\" }\n",
            "mylib"
        ));
        assert_eq!(
            add_dependency("[package]\nname = \"abc\"\n", "mylib = { path = \"x\" }"),
            "[package]\nname = \"abc\"\n\n[dependencies]\nmylib = { path = \"x\" }\n"
        );
        assert_eq!(
            add_dependency("[dependencies]\n", "mylib = { path = \"x\" }"),
            "[dependencies]\nmylib = { path = \"x\" }\n"
        );

        let dir = TempDir::new("dependency");
        dir.write(
            "libraries/rust/mylib/Cargo.toml",
            "[package]\nname = \"mylib\"\n",
        );
        let line = dependency_line(&dir.0, "mylib").unwrap();
        assert!(
            line.starts_with("mylib = { path = \"") && line.ends_with("libraries/rust/mylib\" }"),
            "{line}"
        );
        assert_eq!(dependency_line(&dir.0, "missing"), None);
        assert_eq!(dependency_line(&dir.0, "../mylib"), None);
    }
}
