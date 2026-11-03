// Rust の `mod x;` の展開（Phase 28 作業 1。D-249・D-250）。
//
// 提出と「コードをコピー」のときに、bin のソースの `mod x;`（別のファイルを読むモジュールの宣言）を
// `mod x { <ファイルの中身> }` に置き換えて、1 つのファイルにする。中身は書き換えない（rustc がファイルを読むのと同じ）。
// - ファイルの場所は Rust の決まりどおり：クレートの根（bin のファイル）と mod.rs の中は同じフォルダ、ほかのファイル y.rs の中は y/。
//   インラインのモジュール（mod a { … }）の中は a/ だけ深い。x.rs か x/mod.rs（両方あれば誤り）
// - 入れ子も展開する。文字列・文字・コメントの中は数えない（rust_lex.rs）
// - 見つからないもの・`#[path = "…"]` の付いたものは展開しない（呼び出し側が注意を出す）
// - 展開したファイルの中の include!・include_str!・include_bytes! は、相対パスがずれるので知らせる
use std::path::{Path, PathBuf};

use crate::rust_lex::{self, Kind, Token};

// 入れ子の深さの上限（#[path] がなければ循環はないので、ふつうは届かない）
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub source: String,
    // 展開したモジュールのファイルの見せ方（入れた順。パッケージのフォルダからの相対）
    pub modules: Vec<String>,
    // 展開したファイルの場所（提出前の「テストのあとに変わったか」に使う。C-10）
    pub files: Vec<PathBuf>,
    // 見つからなかった `mod x;` の名前（重ならない）
    pub missing: Vec<String>,
    // `#[path]` の付いた `mod x;` の名前（展開しない）
    pub path_attributes: Vec<String>,
    // include! などを使っている、展開したファイルの見せ方
    pub includes: Vec<String>,
}

// トークンの位置（index）から、対応する閉じかっこの位置。なければ None
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

// `mod` の前に付いた属性に `#[path …]` があるか。mod_index は `mod` の位置
fn has_path_attribute(tokens: &[Token], mod_index: usize) -> bool {
    let mut index = mod_index;
    // 見え方（pub・pub(crate)・pub(in …)）を飛ばす
    if index > 0 && tokens[index - 1].is_punct(')') {
        let mut depth = 0;
        let mut at = index - 1;
        loop {
            if tokens[at].is_punct(')') {
                depth += 1;
            } else if tokens[at].is_punct('(') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            if at == 0 {
                return false;
            }
            at -= 1;
        }
        if at > 0 && tokens[at - 1].is_ident("pub") {
            index = at - 1;
        }
    } else if index > 0 && tokens[index - 1].is_ident("pub") {
        index -= 1;
    }
    // 属性（#[…]）を後ろから順に見る
    while index >= 2 && tokens[index - 1].is_punct(']') {
        let close = index - 1;
        let mut depth = 0;
        let mut open = close;
        loop {
            if tokens[open].is_punct(']') {
                depth += 1;
            } else if tokens[open].is_punct('[') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            if open == 0 {
                return false;
            }
            open -= 1;
        }
        if open == 0 || !tokens[open - 1].is_punct('#') {
            return false;
        }
        if tokens
            .get(open + 1)
            .is_some_and(|token| token.is_ident("path"))
        {
            return true;
        }
        index = open - 1;
    }
    false
}

struct State<'a> {
    package_dir: &'a Path,
    modules: Vec<String>,
    files: Vec<PathBuf>,
    missing: Vec<String>,
    path_attributes: Vec<String>,
    includes: Vec<String>,
}

impl State<'_> {
    fn label(&self, path: &Path) -> String {
        let package = self
            .package_dir
            .canonicalize()
            .unwrap_or_else(|_| self.package_dir.to_path_buf());
        path.strip_prefix(&package)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| PathBuf::from(path.file_name().unwrap_or_default()))
            .to_string_lossy()
            .into_owned()
    }

    // text は base（モジュールのファイルを探すフォルダ）のファイルの中身。展開したものを返す
    fn expand(&mut self, text: &str, base: &Path, depth: usize) -> Result<String, String> {
        let tokens = rust_lex::tokens(text);
        let mut out = String::with_capacity(text.len());
        let mut copied = 0;
        // インラインのモジュールの入れ子（名前と、閉じかっこの位置）
        let mut inline: Vec<(String, usize)> = Vec::new();
        let mut index = 0;
        while index < tokens.len() {
            while inline.last().is_some_and(|(_, close)| *close < index) {
                inline.pop();
            }
            let token = &tokens[index];
            let name = match (tokens.get(index + 1), tokens.get(index + 2)) {
                (
                    Some(Token {
                        kind: Kind::Ident(name),
                        ..
                    }),
                    Some(next),
                ) if token.is_ident("mod") => Some((name.clone(), next)),
                _ => None,
            };
            let Some((name, next)) = name else {
                index += 1;
                continue;
            };
            // インラインのモジュール：中のファイルを探すフォルダが 1 段深くなる
            if next.is_punct('{') {
                if let Some(close) = matching(&tokens, index + 2, '{', '}') {
                    inline.push((name, close));
                }
                index += 3;
                continue;
            }
            if !next.is_punct(';') {
                index += 1;
                continue;
            }
            if has_path_attribute(&tokens, index) {
                if !self.path_attributes.contains(&name) {
                    self.path_attributes.push(name);
                }
                index += 3;
                continue;
            }
            let dir = inline
                .iter()
                .fold(base.to_path_buf(), |dir, (inner, _)| dir.join(inner));
            let flat = dir.join(format!("{name}.rs"));
            let nested = dir.join(&name).join("mod.rs");
            let file = match (flat.is_file(), nested.is_file()) {
                (true, true) => {
                    return Err(format!(
                        "mod {name}; のファイルが 2 つあります（{} と {}）。どちらか一方にしてください",
                        self.label(&flat),
                        self.label(&nested)
                    ));
                }
                (true, false) => flat,
                (false, true) => nested,
                (false, false) => {
                    if !self.missing.contains(&name) {
                        self.missing.push(name);
                    }
                    index += 3;
                    continue;
                }
            };
            if depth >= MAX_DEPTH {
                return Err(format!(
                    "mod {name}; の入れ子が深すぎます（{MAX_DEPTH} 段まで）"
                ));
            }
            let real = file.canonicalize().unwrap_or(file);
            let label = self.label(&real);
            let content = std::fs::read(&real)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
                .ok_or_else(|| {
                    format!("モジュール「{label}」を読めません（UTF-8 のテキストではありません）")
                })?;
            if rust_lex::tokens(&content).windows(2).any(|pair| {
                matches!(&pair[0].kind, Kind::Ident(name) if ["include", "include_str", "include_bytes"].contains(&name.as_str()))
                    && pair[1].is_punct('!')
            }) && !self.includes.contains(&label)
            {
                self.includes.push(label.clone());
            }
            self.modules.push(label.clone());
            self.files.push(real.clone());
            // このファイルの中の mod y; を探すフォルダ：mod.rs なら同じフォルダ、x.rs なら x/
            let parent = real.parent().unwrap_or(&dir).to_path_buf();
            let child_base = if real.file_name().is_some_and(|name| name == "mod.rs") {
                parent
            } else {
                parent.join(&name)
            };
            let inner = self.expand(&content, &child_base, depth + 1)?;
            // `mod x` までは元のまま、`;` を中身に置き換える
            let semicolon = &tokens[index + 2];
            out.push_str(&text[copied..semicolon.start]);
            out.push_str(&format!(" {{\n// ---- 展開：{label} ----\n"));
            out.push_str(&inner);
            if !inner.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(&format!("// ---- ここまで：{label} ----\n}}"));
            copied = semicolon.end;
            index += 3;
        }
        out.push_str(&text[copied..]);
        Ok(out)
    }
}

// bin のソース（main_path の中身が source）の `mod x;` を展開する。package_dir は見せ方に使う。
// 展開するものがなければ、source をそのまま返す
pub fn expand(main_path: &Path, source: &str, package_dir: &Path) -> Result<Expanded, String> {
    let base = main_path.parent().unwrap_or(Path::new("."));
    let mut state = State {
        package_dir,
        modules: Vec::new(),
        files: Vec::new(),
        missing: Vec::new(),
        path_attributes: Vec::new(),
        includes: Vec::new(),
    };
    let expanded = state.expand(source, base, 0)?;
    Ok(Expanded {
        source: if state.modules.is_empty() {
            source.to_string()
        } else {
            expanded
        },
        modules: state.modules,
        files: state.files,
        missing: state.missing,
        path_attributes: state.path_attributes,
        includes: state.includes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "atcoder-studio-rust-expand-{}-{}",
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

    #[test]
    fn expands_modules_with_markers_and_attributes() {
        let dir = TempDir::new("basic");
        dir.write(
            "src/geo.rs",
            "#![allow(dead_code)]\n//! 幾何\npub fn area() -> i64 { 1 }\n",
        );
        dir.write("src/tests.rs", "fn t() {}");
        let main = dir.write("src/main.rs", "");
        let source = "mod geo;\n#[cfg(test)]\npub(crate) mod tests;\nfn main() { println!(\"mod x;\"); geo::area(); }\n";
        let expanded = expand(&main, source, &dir.0).unwrap();
        assert_eq!(
            expanded.source,
            "mod geo {\n\
             // ---- 展開：src/geo.rs ----\n\
             #![allow(dead_code)]\n\
             //! 幾何\n\
             pub fn area() -> i64 { 1 }\n\
             // ---- ここまで：src/geo.rs ----\n\
             }\n\
             #[cfg(test)]\n\
             pub(crate) mod tests {\n\
             // ---- 展開：src/tests.rs ----\n\
             fn t() {}\n\
             // ---- ここまで：src/tests.rs ----\n\
             }\n\
             fn main() { println!(\"mod x;\"); geo::area(); }\n"
        );
        assert_eq!(expanded.modules, ["src/geo.rs", "src/tests.rs"]);
        assert_eq!(expanded.files.len(), 2);
        assert!(expanded.missing.is_empty());
    }

    #[test]
    fn follows_the_rust_file_rules() {
        let dir = TempDir::new("rules");
        // bin のファイル（クレートの根）は同じフォルダ。util.rs の中は util/。mod.rs の中は同じフォルダ
        dir.write("src/bin/util.rs", "pub mod math;\n");
        dir.write("src/bin/util/math.rs", "pub fn gcd() {}\n");
        dir.write("src/bin/graph/mod.rs", "pub mod dijkstra;\n");
        dir.write("src/bin/graph/dijkstra.rs", "pub fn run() {}\n");
        // インラインのモジュールの中は 1 段深い
        dir.write("src/bin/inner/deep.rs", "pub fn deep() {}\n");
        let main = dir.write("src/bin/a.rs", "");
        let source = "mod util;\nmod graph;\nmod inner { pub mod deep; }\nfn main() {}\n";
        let expanded = expand(&main, source, &dir.0).unwrap();
        assert_eq!(
            expanded.modules,
            [
                "src/bin/util.rs",
                "src/bin/util/math.rs",
                "src/bin/graph/mod.rs",
                "src/bin/graph/dijkstra.rs",
                "src/bin/inner/deep.rs",
            ]
        );
        assert!(
            expanded.source.contains(
                "pub mod math {\n// ---- 展開：src/bin/util/math.rs ----\npub fn gcd() {}"
            )
        );
        assert!(expanded.source.contains("mod inner { pub mod deep {"));
        // 両方あれば誤り
        dir.write("src/bin/util/mod.rs", "");
        assert!(
            expand(&main, source, &dir.0)
                .unwrap_err()
                .contains("2 つあります")
        );
    }

    #[test]
    fn reports_missing_path_attributes_and_includes() {
        let dir = TempDir::new("report");
        dir.write("src/x.rs", "fn x() {}\n");
        dir.write(
            "src/data.rs",
            "pub const D: &str = include_str!(\"d.txt\");\n",
        );
        let main = dir.write("src/main.rs", "");
        let source = "mod nothing;\n#[path = \"x.rs\"]\npub mod x;\n#[cfg(test)] #[path = \"t.rs\"] mod t;\nmod data;\n// mod hidden;\nconst S: &str = \"mod s;\";\nfn main() {}\n";
        let expanded = expand(&main, source, &dir.0).unwrap();
        assert_eq!(expanded.missing, ["nothing"]);
        assert_eq!(expanded.path_attributes, ["x", "t"]);
        assert_eq!(expanded.includes, ["src/data.rs"]);
        assert_eq!(expanded.modules, ["src/data.rs"]);
        assert!(
            expanded
                .source
                .contains("mod nothing;\n#[path = \"x.rs\"]\npub mod x;")
        );
        // 展開するものがなければ、元のまま
        let plain = "// mod a;\nfn main() {}\r\n";
        assert_eq!(expand(&main, plain, &dir.0).unwrap().source, plain);
    }

    #[test]
    fn expanded_source_compiles_with_rustc() {
        // 展開したものを、手元の rustc（edition 2024）でコンパイルして動かす。rustc がなければ飛ばす
        let Ok(output) = std::process::Command::new("rustc")
            .arg("--version")
            .output()
        else {
            return;
        };
        if !output.status.success() {
            return;
        }
        let dir = TempDir::new("rustc");
        dir.write(
            "src/util.rs",
            "pub mod math;\npub fn twice(x: i64) -> i64 { 2 * x }\n",
        );
        dir.write(
            "src/util/math.rs",
            "pub fn gcd(a: i64, b: i64) -> i64 { if b == 0 { a } else { gcd(b, a % b) } }\n",
        );
        let main = dir.write("src/main.rs", "");
        let source = "mod util;\nuse util::math::gcd;\nfn main() { println!(\"{} {}\", util::twice(gcd(12, 18)), 'x'); }\n";
        let expanded = expand(&main, source, &dir.0).unwrap();
        let single = dir.write("single.rs", &expanded.source);
        let binary = dir.0.join("single");
        let status = std::process::Command::new("rustc")
            .args(["--edition", "2024", "-o"])
            .arg(&binary)
            .arg(&single)
            .status()
            .unwrap();
        assert!(status.success());
        let output = std::process::Command::new(&binary).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&output.stdout), "12 x\n");
    }
}
