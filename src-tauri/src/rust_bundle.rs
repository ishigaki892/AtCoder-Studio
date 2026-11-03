// Rust の自作のライブラリのクレートの展開（Phase 29 作業 1。D-249・D-251）。
//
// 問題のパッケージのパスの依存のうち、場所が libraries/rust/ の中のもの（自作のライブラリ）を、main.rs の後ろに
// `mod <名前> { … }` として入れ、1 つのファイルにする。ライブラリの中の `mod x;` も展開する（rust_expand.rs）。
// 書き換え（rust_lex.rs で字句を読み、文字列・コメントの中は変えない）：
// - ライブラリの中：`crate::` → `crate::<名前>::`（`$crate::` も同じ）、`#[macro_export]` のマクロは属性を外し、
//   すぐあとに `pub(crate) use <マクロ>;`（モジュールの中にあれば、ライブラリの頭にも `pub(crate) use self::…;`）
// - 使う側（main と、ほかのライブラリを使うライブラリ）：パスの頭の `<使う名前>::`・`::<使う名前>::`・`use <使う名前>` →
//   `crate::<名前>`。`extern crate <使う名前>;` は消し、`extern crate <使う名前> as x;` は `use crate::<名前> as x;`
// - 入れるときの名前は、ライブラリの lib の名前（名前を変えた依存〔package = …〕でも、使う側の名前を書き換える）
// - 使っていないライブラリは入れない。proc-macro と build.rs のあるクレートは入れずに知らせる
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::rust_lex::{self, Kind, Token};

// 自作のライブラリの置き場所（アプリのフォルダの中。D-251）。この下に、クレートのフォルダ（Cargo.toml と src/lib.rs）を置く
pub const LIBRARY_DIR: &str = "libraries/rust";

// アプリのフォルダに自作のライブラリのフォルダがあれば、その場所（canonicalize 済み）
pub fn library_dir(app_dir: &Path) -> Option<PathBuf> {
    app_dir
        .join(LIBRARY_DIR)
        .canonicalize()
        .ok()
        .filter(|dir| dir.is_dir())
}

// 自作のライブラリのフォルダ。なければ空のフォルダを作る（設定の画面の「ライブラリのフォルダを開く（Rust）」）
pub fn ensure_library_dir(app_dir: &Path) -> Result<PathBuf, String> {
    let dir = app_dir.join(LIBRARY_DIR);
    std::fs::create_dir_all(&dir).map_err(|_| "ライブラリのフォルダを作れません".to_string())?;
    Ok(dir)
}

// 自作のライブラリのクレート（libraries/rust/ の直下の、Cargo.toml のあるフォルダ）の、写して使う依存の行（名前の順）
pub fn dependency_lines(app_dir: &Path) -> Vec<String> {
    let dir = app_dir.join(LIBRARY_DIR);
    let mut lines: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.join("Cargo.toml").is_file())
                .filter_map(|path| {
                    let name = path.file_name()?.to_string_lossy().into_owned();
                    let quoted = serde_json::to_string(&path.to_string_lossy()).ok()?;
                    Some(format!("{name} = {{ path = {quoted} }}"))
                })
                .collect()
        })
        .unwrap_or_default();
    lines.sort();
    lines
}

// ---- `cargo metadata --no-deps` の出力（使うところだけ） ----

#[derive(Debug, Clone, Deserialize)]
pub struct Metadata {
    pub packages: Vec<Package>,
    pub workspace_root: PathBuf,
    #[serde(default)]
    pub target_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Package {
    pub name: String,
    #[serde(default)]
    pub edition: String,
    pub manifest_path: PathBuf,
    pub targets: Vec<Target>,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Target {
    pub name: String,
    pub kind: Vec<String>,
    pub src_path: PathBuf,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Dependency {
    pub name: String,
    // crates.io なら `registry+…`、git なら `git+…`、パスなら null
    pub source: Option<String>,
    // 版の決まり（例：`^0.5.0`）
    #[serde(default)]
    pub req: String,
    // 通常の依存は null、`dev`・`build`
    pub kind: Option<String>,
    // `foo = { package = "bar" }` の foo
    #[serde(default)]
    pub rename: Option<String>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default = "yes")]
    pub uses_default_features: bool,
    #[serde(default)]
    pub features: Vec<String>,
    // パスの依存の場所
    #[serde(default)]
    pub path: Option<PathBuf>,
    // `[target.'cfg(…)'.dependencies]` の条件
    #[serde(default)]
    pub target: Option<String>,
}

impl Dependency {
    // テストなどで、名前と出どころだけを決めて作る
    #[cfg(test)]
    pub fn named(name: &str, source: Option<&str>, kind: Option<&str>) -> Self {
        Dependency {
            name: name.to_string(),
            source: source.map(String::from),
            req: "*".to_string(),
            kind: kind.map(String::from),
            rename: None,
            optional: false,
            uses_default_features: true,
            features: Vec::new(),
            path: None,
            target: None,
        }
    }
}

pub fn parse_metadata(json: &str) -> Result<Metadata, String> {
    serde_json::from_str(json).map_err(|_| "cargo metadata の出力を読めません".to_string())
}

// metadata の中の、manifest のフォルダが dir のパッケージ
pub fn package_in<'a>(metadata: &'a Metadata, dir: &Path) -> Option<&'a Package> {
    let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    metadata.packages.iter().find(|package| {
        package
            .manifest_path
            .parent()
            .and_then(|parent| parent.canonicalize().ok())
            .is_some_and(|parent| parent == dir)
    })
}

// ---- 展開 ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundled {
    pub source: String,
    // 入れたライブラリの見せ方（例：`mylib（src/lib.rs・src/seg.rs）`。入れた順）
    pub libraries: Vec<String>,
    // 入れたライブラリのファイルの場所（提出前の「テストのあとに変わったか」に使う。C-10）
    pub files: Vec<PathBuf>,
    // 入れた、main の依存のパッケージの名前（「パスの依存」の注意を出さない）
    pub bundled: Vec<String>,
    // 入れたライブラリの crates.io などの依存（ジャッジにあるかを確かめ、送る前のビルドに使う）
    pub extra_dependencies: Vec<Dependency>,
    // 入れなかった理由など
    pub warnings: Vec<String>,
}

// 置き換え（バイトの範囲 → 文字）
struct Edit {
    start: usize,
    end: usize,
    text: String,
}

fn apply(text: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|edit| (edit.start, edit.end));
    let mut out = String::with_capacity(text.len() + 256);
    let mut copied = 0;
    for edit in edits {
        if edit.start < copied {
            continue;
        }
        out.push_str(&text[copied..edit.start]);
        out.push_str(&edit.text);
        copied = edit.end;
    }
    out.push_str(&text[copied..]);
    out
}

// tokens[index] と tokens[index + 1] が続けて書かれた `::` か
fn is_path_sep(tokens: &[Token], index: usize) -> bool {
    match (tokens.get(index), tokens.get(index + 1)) {
        (Some(a), Some(b)) => a.is_punct(':') && b.is_punct(':') && a.end == b.start,
        _ => false,
    }
}

// index の名前が、パスの頭（`name::`・`::name::`・`use name`）として使われているか。
// 使われていれば、置き換える範囲の頭（`::name` なら `::` の頭）を返す
fn path_head(tokens: &[Token], index: usize) -> Option<usize> {
    let token = &tokens[index];
    let followed = is_path_sep(tokens, index + 1);
    let after_use = index > 0
        && tokens[index - 1].is_ident("use")
        && tokens
            .get(index + 1)
            .is_some_and(|next| next.is_punct(';') || next.is_ident("as") || next.is_punct(','));
    if index >= 2 && is_path_sep(tokens, index - 2) {
        // `…::name`：前に何もない（`::name::`）ときだけパスの頭
        let before = index.checked_sub(3).map(|at| &tokens[at]);
        let global = match before {
            None => true,
            Some(before) => match &before.kind {
                Kind::Ident(word) => word == "use",
                Kind::Punct('>') => index >= 4 && tokens[index - 4].is_punct('-'),
                Kind::Punct(_) => true,
                _ => false,
            },
        };
        return (global && followed).then_some(tokens[index - 2].start);
    }
    if index > 0 && tokens[index - 1].is_punct('.') {
        return None;
    }
    (followed || after_use).then_some(token.start)
}

// 使う側の書き換え：`<使う名前>` のパスの頭を `crate::<名前>` に。`extern crate` も直す
fn rewrite_uses(
    text: &str,
    tokens: &[Token],
    renames: &BTreeMap<String, String>,
    edits: &mut Vec<Edit>,
) {
    for (index, token) in tokens.iter().enumerate() {
        let Kind::Ident(word) = &token.kind else {
            continue;
        };
        let Some(module) = renames.get(word) else {
            continue;
        };
        // extern crate name; ・ extern crate name as x;
        if index >= 2 && tokens[index - 1].is_ident("crate") && tokens[index - 2].is_ident("extern")
        {
            let start = tokens[index - 2].start;
            match (
                tokens.get(index + 1),
                tokens.get(index + 2),
                tokens.get(index + 3),
            ) {
                (Some(semicolon), _, _) if semicolon.is_punct(';') => edits.push(Edit {
                    start,
                    end: semicolon.end,
                    text: String::new(),
                }),
                (Some(as_word), Some(alias), Some(semicolon))
                    if as_word.is_ident("as") && semicolon.is_punct(';') =>
                {
                    edits.push(Edit {
                        start,
                        end: semicolon.end,
                        text: format!("use crate::{module} as {};", &text[alias.start..alias.end]),
                    });
                }
                _ => {}
            }
            continue;
        }
        if let Some(start) = path_head(tokens, index) {
            edits.push(Edit {
                start,
                end: token.end,
                text: format!("crate::{module}"),
            });
        }
    }
}

// ライブラリの中の書き換え：`crate::`・`$crate::` と `#[macro_export]`。返す 2 つ目は、ライブラリの頭に足す行
fn rewrite_library(tokens: &[Token], module: &str, edits: &mut Vec<Edit>) -> Vec<String> {
    let mut root_exports = Vec::new();
    // インラインのモジュールの入れ子（名前と、閉じかっこの位置）
    let mut inline: Vec<(String, usize)> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        while inline.last().is_some_and(|(_, close)| *close < index) {
            inline.pop();
        }
        let token = &tokens[index];
        if token.is_ident("crate") && is_path_sep(tokens, index + 1) {
            edits.push(Edit {
                start: token.end,
                end: token.end,
                text: format!("::{module}"),
            });
        } else if token.is_ident("mod")
            && let (
                Some(Token {
                    kind: Kind::Ident(name),
                    ..
                }),
                Some(open),
            ) = (tokens.get(index + 1), tokens.get(index + 2))
            && open.is_punct('{')
            && let Some(close) = matching(tokens, index + 2, '{', '}')
        {
            inline.push((name.clone(), close));
        } else if token.is_punct('#')
            && tokens.get(index + 1).is_some_and(|t| t.is_punct('['))
            && tokens
                .get(index + 2)
                .is_some_and(|t| t.is_ident("macro_export"))
            && let Some(attr_close) = matching(tokens, index + 1, '[', ']')
            && tokens
                .get(attr_close + 1)
                .is_some_and(|t| t.is_ident("macro_rules"))
            && tokens.get(attr_close + 2).is_some_and(|t| t.is_punct('!'))
            && let Some(Token {
                kind: Kind::Ident(name),
                ..
            }) = tokens.get(attr_close + 3)
            && let Some(open) = tokens.get(attr_close + 4)
        {
            let pair = match open.kind {
                Kind::Punct('{') => Some(('{', '}')),
                Kind::Punct('(') => Some(('(', ')')),
                Kind::Punct('[') => Some(('[', ']')),
                _ => None,
            };
            if let Some((left, right)) = pair
                && let Some(close) = matching(tokens, attr_close + 4, left, right)
            {
                // `macro_rules! m ( … );` の `;` まで
                let end = if left != '{' && tokens.get(close + 1).is_some_and(|t| t.is_punct(';')) {
                    tokens[close + 1].end
                } else {
                    tokens[close].end
                };
                edits.push(Edit {
                    start: token.start,
                    end: tokens[attr_close].end,
                    text: String::new(),
                });
                edits.push(Edit {
                    start: end,
                    end,
                    text: format!("\n#[allow(unused_imports)]\npub(crate) use {name};"),
                });
                if !inline.is_empty() {
                    let path: Vec<&str> = inline.iter().map(|(name, _)| name.as_str()).collect();
                    root_exports.push(format!(
                        "#[allow(unused_imports)]\npub(crate) use self::{}::{name};",
                        path.join("::")
                    ));
                }
                // マクロの中（`$crate::` など）も続けて見る
                index = attr_close + 1;
                continue;
            }
        }
        index += 1;
    }
    root_exports
}

fn matching(tokens: &[Token], open: usize, left: char, right: char) -> Option<usize> {
    let mut depth = 0;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        if token.is_punct(left) {
            depth += 1;
        } else if token.is_punct(right) {
            depth -= 1;
            if depth == 0 {
                return Some(index);
            }
        }
    }
    None
}

// コードの中で、その名前がパスの頭として使われているか（使っていないライブラリは入れない）
fn uses_name(tokens: &[Token], name: &str) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        token.is_ident(name)
            && (path_head(tokens, index).is_some()
                || (index >= 2
                    && tokens[index - 1].is_ident("crate")
                    && tokens[index - 2].is_ident("extern")))
    })
}

// 依存の、コードの中で使う名前（rename、なければライブラリの lib の名前、なければパッケージの名前の - を _ に）
fn code_name(dependency: &Dependency, lib_name: Option<&str>) -> String {
    dependency
        .rename
        .clone()
        .or_else(|| lib_name.map(String::from))
        .unwrap_or_else(|| dependency.name.replace('-', "_"))
}

fn is_inside(path: &Path, root: &Path) -> bool {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    path.starts_with(root)
}

// libraries/rust/ の中のパスの依存か（D-251。main で使っていなくても、「パスの依存」の注意は出さない）
pub fn in_library(dependency: &Dependency, library_root: &Path) -> bool {
    dependency
        .path
        .as_deref()
        .is_some_and(|path| is_inside(path, library_root))
}

// 読んだライブラリ
struct Library {
    module: String,
    dir: PathBuf,
    package: Package,
    lib: Target,
}

// main（Phase 28 の展開のあと）に、使っている自作のライブラリを入れる。dependencies は main のパッケージの依存、
// library_root は libraries/rust/（canonicalize 済み）。read_metadata は、ライブラリのフォルダの `cargo metadata --no-deps` の出力
pub fn bundle(
    main: &str,
    dependencies: &[Dependency],
    library_root: &Path,
    read_metadata: &dyn Fn(&Path) -> Result<String, String>,
) -> Result<Bundled, String> {
    let mut bundled = Bundled {
        source: main.to_string(),
        libraries: Vec::new(),
        files: Vec::new(),
        bundled: Vec::new(),
        extra_dependencies: Vec::new(),
        warnings: Vec::new(),
    };
    // ライブラリを読む（同じフォルダは 1 回だけ）
    let mut libraries: Vec<Library> = Vec::new();
    let load = |dir: &Path,
                libraries: &mut Vec<Library>,
                warnings: &mut Vec<String>|
     -> Result<Option<usize>, String> {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        if let Some(at) = libraries.iter().position(|library| library.dir == dir) {
            return Ok(Some(at));
        }
        let metadata = parse_metadata(&read_metadata(&dir)?)?;
        let Some(package) = package_in(&metadata, &dir).cloned() else {
            return Err(format!(
                "ライブラリ（{}）の Cargo.toml を読めません",
                dir.display()
            ));
        };
        if package
            .targets
            .iter()
            .any(|target| target.kind.iter().any(|kind| kind == "custom-build"))
        {
            warnings.push(format!(
                "ライブラリ「{}」には build.rs があるので、展開していません",
                package.name
            ));
            return Ok(None);
        }
        let Some(lib) = package
            .targets
            .iter()
            .find(|target| {
                target
                    .kind
                    .iter()
                    .any(|kind| kind == "lib" || kind == "rlib")
            })
            .cloned()
        else {
            warnings.push(format!(
                "ライブラリ「{}」は、ふつうのライブラリのクレート（lib）ではないので、展開していません（proc-macro など）",
                package.name
            ));
            return Ok(None);
        };
        libraries.push(Library {
            module: lib.name.clone(),
            dir,
            package,
            lib,
        });
        Ok(Some(libraries.len() - 1))
    };

    // main から使っているライブラリ
    let main_tokens = rust_lex::tokens(main);
    let mut main_renames: BTreeMap<String, String> = BTreeMap::new();
    let mut queue: Vec<usize> = Vec::new();
    for dependency in dependencies.iter().filter(|d| d.kind.is_none()) {
        let Some(path) = &dependency.path else {
            continue;
        };
        if !is_inside(path, library_root) {
            continue;
        }
        // 使う名前は rename か lib の名前。使っていないライブラリの知らせ（proc-macro など）は出さない
        let guess = code_name(dependency, None);
        let mut notes = Vec::new();
        let Some(at) = load(path, &mut libraries, &mut notes)? else {
            if uses_name(&main_tokens, &guess) {
                bundled.warnings.extend(notes);
            }
            continue;
        };
        let name = code_name(dependency, Some(&libraries[at].lib.name));
        if !uses_name(&main_tokens, &name) {
            continue;
        }
        main_renames.insert(name, libraries[at].module.clone());
        bundled.bundled.push(dependency.name.clone());
        if !queue.contains(&at) {
            queue.push(at);
        }
    }
    if queue.is_empty() {
        return Ok(bundled);
    }

    // ライブラリを順に展開し、そのライブラリが使うライブラリも入れる
    let mut parts: Vec<String> = Vec::new();
    let mut done = 0;
    while done < queue.len() {
        let at = queue[done];
        done += 1;
        let (module, lib_path, package_dir, deps) = {
            let library = &libraries[at];
            (
                library.module.clone(),
                library.lib.src_path.clone(),
                library.dir.clone(),
                library.package.dependencies.clone(),
            )
        };
        let content = std::fs::read(&lib_path)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .ok_or_else(|| {
                format!(
                    "ライブラリ「{module}」の {} を読めません",
                    lib_path.display()
                )
            })?;
        let expanded = crate::rust_expand::expand(&lib_path, &content, &package_dir)?;
        for name in expanded.missing.iter().chain(&expanded.path_attributes) {
            bundled.warnings.push(format!(
                "ライブラリ「{module}」の mod {name}; は展開できませんでした"
            ));
        }
        let text = expanded.source;
        let tokens = rust_lex::tokens(&text);
        // このライブラリが使うライブラリ
        let mut renames: BTreeMap<String, String> = BTreeMap::new();
        for dependency in deps.iter().filter(|d| d.kind.is_none()) {
            match &dependency.path {
                Some(path) if is_inside(path, library_root) => {
                    let Some(next) = load(path, &mut libraries, &mut bundled.warnings)? else {
                        continue;
                    };
                    let name = code_name(dependency, Some(&libraries[next].lib.name));
                    if uses_name(&tokens, &name) {
                        renames.insert(name, libraries[next].module.clone());
                        if !queue.contains(&next) {
                            queue.push(next);
                        }
                    }
                }
                Some(_) => bundled.warnings.push(format!(
                    "ライブラリ「{module}」の依存「{}」は、libraries/rust/ の外のパスにあるので、ジャッジでは使えません",
                    dependency.name
                )),
                None => {
                    if !bundled.extra_dependencies.contains(dependency) {
                        bundled.extra_dependencies.push(dependency.clone());
                    }
                }
            }
        }
        let mut edits = Vec::new();
        let root_exports = rewrite_library(&tokens, &module, &mut edits);
        rewrite_uses(&text, &tokens, &renames, &mut edits);
        let body = apply(&text, edits);
        let lib_label = |path: &Path| {
            path.strip_prefix(&package_dir)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| PathBuf::from(path.file_name().unwrap_or_default()))
                .to_string_lossy()
                .into_owned()
        };
        let real_lib = lib_path.canonicalize().unwrap_or(lib_path.clone());
        let mut labels = vec![lib_label(&real_lib)];
        labels.extend(expanded.files.iter().map(|path| lib_label(path)));
        bundled
            .libraries
            .push(format!("{module}（{}）", labels.join("・")));
        bundled.files.push(real_lib);
        bundled.files.extend(expanded.files);
        let place = library_root
            .parent()
            .and_then(|libraries| package_dir.strip_prefix(libraries.parent()?).ok())
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| module.clone());
        let mut part = format!(
            "\n// ---- 展開：ライブラリ {module}（{place}）----\n#[allow(dead_code, unused_imports, unused_macros)]\nmod {module} {{\n"
        );
        part.push_str(&body);
        if !body.ends_with('\n') {
            part.push('\n');
        }
        for line in root_exports {
            part.push_str(&line);
            part.push('\n');
        }
        part.push_str(&format!("}}\n// ---- ここまで：ライブラリ {module} ----\n"));
        parts.push(part);
    }

    let mut edits = Vec::new();
    rewrite_uses(main, &main_tokens, &main_renames, &mut edits);
    let mut source = apply(main, edits);
    if !source.ends_with('\n') {
        source.push('\n');
    }
    for part in parts {
        source.push_str(&part);
    }
    bundled.source = source;
    Ok(bundled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "atcoder-studio-rust-bundle-{}-{}",
                name,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path.canonicalize().unwrap())
        }

        fn write(&self, path: &str, text: &str) -> PathBuf {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    // ライブラリのクレートを作り、その `cargo metadata --no-deps` の形を返す関数を用意する
    fn library(dir: &TempDir, name: &str, deps: &[Dependency]) -> PathBuf {
        let path = dir.0.join("libraries/rust").join(name);
        fs::create_dir_all(path.join("src")).unwrap();
        fs::write(path.join("Cargo.toml"), "").unwrap();
        let metadata = serde_json::json!({
            "packages": [{
                "name": name,
                "edition": "2024",
                "manifest_path": path.join("Cargo.toml"),
                "targets": [{"name": name.replace('-', "_"), "kind": ["lib"], "src_path": path.join("src/lib.rs")}],
                "dependencies": deps.iter().map(|d| serde_json::json!({
                    "name": d.name, "source": d.source, "req": d.req, "kind": d.kind, "rename": d.rename,
                    "optional": d.optional, "uses_default_features": d.uses_default_features,
                    "features": d.features, "path": d.path, "target": d.target,
                })).collect::<Vec<_>>(),
            }],
            "workspace_root": path,
        });
        fs::write(path.join("metadata.json"), metadata.to_string()).unwrap();
        path
    }

    fn read_metadata(dir: &Path) -> Result<String, String> {
        fs::read_to_string(dir.join("metadata.json")).map_err(|e| e.to_string())
    }

    fn path_dep(name: &str, path: &Path) -> Dependency {
        Dependency {
            path: Some(path.to_path_buf()),
            ..Dependency::named(name, None, None)
        }
    }

    #[test]
    fn rewrites_crate_paths_macros_and_uses() {
        let dir = TempDir::new("rewrite");
        let mylib = library(&dir, "mylib", &[]);
        dir.write(
            "libraries/rust/mylib/src/lib.rs",
            "pub mod seg;\npub mod math;\n#[macro_export]\nmacro_rules! chmin { ($a:expr, $b:expr) => { if $b < $a { $a = $b; true } else { false } }; }\n#[macro_export]\nmacro_rules! seg_of { ($n:expr) => { $crate::seg::Seg::new($n) } }\npub(crate) fn hidden() -> &'static str { \"crate::x\" }\n",
        );
        dir.write(
            "libraries/rust/mylib/src/math.rs",
            "pub fn gcd(a: usize, b: usize) -> usize { if b == 0 { a } else { gcd(b, a % b) } }\n",
        );
        dir.write(
            "libraries/rust/mylib/src/seg.rs",
            "use crate::math::gcd;\npub struct Seg { pub n: usize }\nimpl Seg { pub fn new(n: usize) -> Self { Seg { n: gcd(n, 12) } } }\n",
        );
        let root = dir.0.join("libraries/rust");
        let main = "use mylib::seg::Seg;\nuse mylib::{chmin, seg_of};\nmod inner { pub fn g() -> usize { ::mylib::math::gcd(4, 6) } }\nfn main() {\n    let s = Seg::new(8);\n    let mut x = 5;\n    let changed = chmin!(x, 3);\n    let t = seg_of!(18);\n    println!(\"{} {} {} {} {} mylib::\", s.n, x, changed, t.n, mylib::math::gcd(12, 18) + inner::g());\n}\n";
        let bundled = bundle(main, &[path_dep("mylib", &mylib)], &root, &read_metadata).unwrap();
        assert_eq!(bundled.bundled, ["mylib"]);
        assert_eq!(
            bundled.libraries,
            ["mylib（src/lib.rs・src/seg.rs・src/math.rs）"]
        );
        assert_eq!(bundled.files.len(), 3);
        let source = &bundled.source;
        assert!(source.starts_with("use crate::mylib::seg::Seg;\nuse crate::mylib::{chmin, seg_of};\nmod inner { pub fn g() -> usize { crate::mylib::math::gcd(4, 6) } }"), "{source}");
        // 文字列の中は変えない
        assert!(source.contains("mylib::\", s.n"));
        assert!(source.contains("crate::mylib::math::gcd(12, 18)"));
        assert!(source.contains("use crate::mylib::math::gcd;"));
        assert!(source.contains("$crate::mylib::seg::Seg::new($n)"));
        assert!(source.contains("\"crate::x\""));
        assert!(source.contains("pub(crate) fn hidden"));
        assert!(!source.contains("#[macro_export]"));
        assert!(source.contains("pub(crate) use chmin;"));
        assert!(source.contains("#[allow(dead_code, unused_imports, unused_macros)]\nmod mylib {"));
        // 手元の rustc（edition 2024）でコンパイルして動かす。rustc がなければ飛ばす
        if std::process::Command::new("rustc")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            let single = dir.write("single.rs", source);
            let binary = dir.0.join("single");
            let status = std::process::Command::new("rustc")
                .args(["--edition", "2024", "-o"])
                .arg(&binary)
                .arg(&single)
                .status()
                .unwrap();
            assert!(status.success());
            let output = std::process::Command::new(&binary).output().unwrap();
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                "4 3 true 6 8 mylib::\n"
            );
        }
    }

    // モジュールの中で #[macro_export] したマクロも、ライブラリの頭から使える（mylib::hi!）
    #[test]
    fn exports_macros_defined_in_modules() {
        let dir = TempDir::new("nested-macro");
        let mylib = library(&dir, "mylib", &[]);
        dir.write(
            "libraries/rust/mylib/src/lib.rs",
            "pub mod util {\n    #[macro_export]\n    macro_rules! hi { () => { $crate::util::one() }; }\n    pub fn one() -> i32 { 1 }\n}\n",
        );
        let root = dir.0.join("libraries/rust");
        let main = "fn main() { println!(\"{}\", mylib::hi!()); }\n";
        let bundled = bundle(main, &[path_dep("mylib", &mylib)], &root, &read_metadata).unwrap();
        assert!(bundled.source.contains("pub(crate) use self::util::hi;"));
        assert!(bundled.source.contains("$crate::mylib::util::one()"));
        if std::process::Command::new("rustc")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
        {
            let single = dir.write("single.rs", &bundled.source);
            let binary = dir.0.join("single");
            let status = std::process::Command::new("rustc")
                .args(["--edition", "2024", "-o"])
                .arg(&binary)
                .arg(&single)
                .status()
                .unwrap();
            assert!(status.success(), "{}", bundled.source);
            let output = std::process::Command::new(&binary).output().unwrap();
            assert_eq!(String::from_utf8_lossy(&output.stdout), "1\n");
        }
    }

    #[test]
    fn bundles_library_dependencies_and_renames() {
        let dir = TempDir::new("deps");
        let b = library(
            &dir,
            "lib-b",
            &[Dependency {
                req: "^0.5.0".into(),
                ..Dependency::named(
                    "proconio",
                    Some("registry+https://github.com/rust-lang/crates.io-index"),
                    None,
                )
            }],
        );
        dir.write(
            "libraries/rust/lib-b/src/lib.rs",
            "pub fn base() -> i32 { 7 }\n",
        );
        let a = library(&dir, "lib-a", &[path_dep("lib-b", &b)]);
        dir.write(
            "libraries/rust/lib-a/src/lib.rs",
            "extern crate lib_b;\npub fn twice() -> i32 { 2 * lib_b::base() }\n",
        );
        let unused = library(&dir, "unused", &[]);
        dir.write("libraries/rust/unused/src/lib.rs", "pub fn u() {}\n");
        let root = dir.0.join("libraries/rust");
        // main は lib-a を la という名前で使う
        let main = "fn main() { println!(\"{}\", la::twice()); }\n";
        let renamed = Dependency {
            rename: Some("la".to_string()),
            ..path_dep("lib-a", &a)
        };
        let outside = path_dep("other", &dir.0.join("elsewhere"));
        let bundled = bundle(
            main,
            &[renamed, path_dep("unused", &unused), outside],
            &root,
            &read_metadata,
        )
        .unwrap();
        assert_eq!(bundled.bundled, ["lib-a"]);
        assert_eq!(
            bundled.libraries,
            ["lib_a（src/lib.rs）", "lib_b（src/lib.rs）"]
        );
        assert!(bundled.source.contains("crate::lib_a::twice()"));
        assert!(bundled.source.contains("2 * crate::lib_b::base()"));
        assert!(!bundled.source.contains("extern crate"));
        assert!(!bundled.source.contains("mod unused"));
        assert_eq!(bundled.extra_dependencies.len(), 1);
        assert_eq!(bundled.extra_dependencies[0].name, "proconio");
        // 使っていなければ、元のまま
        let plain = "fn main() {}\n";
        let none = bundle(plain, &[path_dep("unused", &unused)], &root, &read_metadata).unwrap();
        assert_eq!(none.source, plain);
        assert!(none.libraries.is_empty());
    }

    #[test]
    fn skips_proc_macros_and_build_scripts() {
        let dir = TempDir::new("skip");
        let path = dir.0.join("libraries/rust/pm");
        fs::create_dir_all(path.join("src")).unwrap();
        let metadata = serde_json::json!({
            "packages": [{
                "name": "pm", "edition": "2024", "manifest_path": path.join("Cargo.toml"),
                "targets": [{"name": "pm", "kind": ["proc-macro"], "src_path": path.join("src/lib.rs")}],
                "dependencies": [],
            }],
            "workspace_root": path,
        });
        fs::write(path.join("metadata.json"), metadata.to_string()).unwrap();
        let root = dir.0.join("libraries/rust");
        let bundled = bundle(
            "fn main() { pm::x(); }\n",
            &[path_dep("pm", &path)],
            &root,
            &read_metadata,
        )
        .unwrap();
        assert!(bundled.libraries.is_empty());
        assert_eq!(bundled.warnings.len(), 1);
        assert!(bundled.warnings[0].contains("proc-macro"));
    }

    #[test]
    fn finds_path_heads() {
        let tokens = rust_lex::tokens(
            "a::b; x.a::c; ::a::d; use a; use a as z; fn f() -> ::a::T {} Vec<::a::U> b::a::e",
        );
        let heads: Vec<usize> = (0..tokens.len())
            .filter(|&i| tokens[i].is_ident("a") && path_head(&tokens, i).is_some())
            .collect();
        // a::b・::a::d・use a;・use a as・-> ::a::T・<::a::U は頭。x.a::c と b::a::e は頭ではない
        assert_eq!(heads.len(), 6);
    }
}
