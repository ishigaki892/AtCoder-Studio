// C++ のライブラリの展開（Phase 27 作業 1。D-237・D-248）。
//
// 提出と「コードをコピー」のときに、`#include "…"` の行を、そのヘッダーの中身に置き換えて 1 つのファイルにする。
// - 探す順は g++ と同じ：読む側のファイルのフォルダ、なければ自作のライブラリのフォルダ（アプリのフォルダの libraries/cpp/）
// - 入れ子も展開する。同じファイルは 1 回だけ入れる（#pragma once・インクルードガードと同じ結果。#pragma once の行は消す）
// - `#include <…>` と、AC Library のフォルダで見つかるもの（`#include "atcoder/…"`）は展開しない（ジャッジにある）
// - 見つからないものは、行を残して missing に入れる（呼び出し側が注意を出す）
// - 限界：行ごとに見るだけで、`/* … */` の中や `#if` の中かどうかは区別しない（今の注意と同じ決め方）
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// 入れ子の深さの上限（循環は 1 回だけ入れる決まりで止まるので、ふつうは届かない）
const MAX_DEPTH: usize = 64;

// ヘッダーを探す場所
pub struct SearchDirs<'a> {
    // 自作のライブラリのフォルダ（libraries/cpp/）。なければ None
    pub library: Option<&'a Path>,
    // AC Library のフォルダ（libraries/ac-library/）。ここで見つかるものは展開しない
    pub ac_library: Option<&'a Path>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    pub source: String,
    // 展開したヘッダーの見せ方（入れた順。ライブラリのものは libraries/cpp/ からの相対、隣のものは main.cpp のフォルダからの相対）
    pub headers: Vec<String>,
    // 展開したヘッダーの場所（提出前の「テストのあとに変わったか」に使う。C-10）
    pub files: Vec<PathBuf>,
    // 見つからなかった `#include "…"` の名前（重ならない）
    pub missing: Vec<String>,
}

// 行が `#include "name"` なら name。`#` と include の間・前後の空白は許す
pub fn quoted_include(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix('#')?.trim_start();
    let rest = rest.strip_prefix("include")?.trim_start();
    let name = rest.strip_prefix('"')?;
    let name = &name[..name.find('"')?];
    (!name.is_empty()).then_some(name)
}

fn is_pragma_once(line: &str) -> bool {
    let Some(rest) = line.trim_start().strip_prefix('#') else {
        return false;
    };
    let mut words = rest.split_whitespace();
    words.next() == Some("pragma") && words.next() == Some("once") && words.next().is_none()
}

struct State<'a> {
    dirs: &'a SearchDirs<'a>,
    main_dir: PathBuf,
    seen: HashSet<PathBuf>,
    out: String,
    headers: Vec<String>,
    files: Vec<PathBuf>,
    missing: Vec<String>,
}

enum Found {
    Expand(PathBuf),
    // ジャッジにあるので、行をそのまま残す
    Keep,
    Missing,
}

impl State<'_> {
    fn find(&self, from_dir: &Path, name: &str) -> Found {
        if let Some(path) = [Some(from_dir), self.dirs.library]
            .into_iter()
            .flatten()
            .map(|dir| dir.join(name))
            .find(|path| path.is_file())
        {
            return Found::Expand(path);
        }
        if self
            .dirs
            .ac_library
            .is_some_and(|dir| dir.join(name).is_file())
        {
            return Found::Keep;
        }
        Found::Missing
    }

    // 見せ方：ライブラリのフォルダか main.cpp のフォルダからの相対。どちらでもなければファイルの名前
    fn label(&self, path: &Path) -> String {
        let canonical = |dir: &Path| dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        [
            self.dirs.library.map(canonical),
            Some(canonical(&self.main_dir)),
        ]
        .into_iter()
        .flatten()
        .find_map(|dir| path.strip_prefix(&dir).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from(path.file_name().unwrap_or_default()))
        .to_string_lossy()
        .into_owned()
    }

    fn expand(&mut self, text: &str, dir: &Path, header: bool, depth: usize) -> Result<(), String> {
        for line in text.split_inclusive('\n') {
            if header && is_pragma_once(line) {
                continue;
            }
            let Some(name) = quoted_include(line) else {
                self.out.push_str(line);
                if !line.ends_with('\n') {
                    self.out.push('\n');
                }
                continue;
            };
            match self.find(dir, name) {
                Found::Keep => {
                    self.out.push_str(line.trim_end_matches(['\r', '\n']));
                    self.out.push('\n');
                }
                Found::Missing => {
                    self.out.push_str(line.trim_end_matches(['\r', '\n']));
                    self.out.push('\n');
                    if !self.missing.iter().any(|known| known == name) {
                        self.missing.push(name.to_string());
                    }
                }
                Found::Expand(path) => {
                    let real = path.canonicalize().unwrap_or(path);
                    // 2 回目からは入れない（1 回目の場所で入っている）
                    if !self.seen.insert(real.clone()) {
                        continue;
                    }
                    if depth >= MAX_DEPTH {
                        return Err(format!(
                            "#include \"{name}\" の入れ子が深すぎます（{MAX_DEPTH} 段まで）"
                        ));
                    }
                    let label = self.label(&real);
                    let content = std::fs::read(&real)
                        .ok()
                        .and_then(|bytes| String::from_utf8(bytes).ok())
                        .ok_or_else(|| {
                            format!(
                                "ヘッダー「{label}」を読めません（UTF-8 のテキストではありません）"
                            )
                        })?;
                    self.headers.push(label.clone());
                    self.files.push(real.clone());
                    self.out.push_str(&format!("// ---- 展開：{label} ----\n"));
                    let parent = real.parent().unwrap_or(dir).to_path_buf();
                    self.expand(&content, &parent, true, depth + 1)?;
                    self.out
                        .push_str(&format!("// ---- ここまで：{label} ----\n"));
                }
            }
        }
        Ok(())
    }
}

// main_path のソース（source）の `#include "…"` を展開する。展開するものがなければ、source をそのまま返す
pub fn expand(main_path: &Path, source: &str, dirs: &SearchDirs) -> Result<Expanded, String> {
    let main_dir = main_path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut state = State {
        dirs,
        main_dir: main_dir.clone(),
        seen: HashSet::new(),
        out: String::with_capacity(source.len()),
        headers: Vec::new(),
        files: Vec::new(),
        missing: Vec::new(),
    };
    // main.cpp 自身を読み込むヘッダーがあっても、もう 1 度は入れない
    state
        .seen
        .insert(main_path.canonicalize().unwrap_or(main_path.to_path_buf()));
    state.expand(source, &main_dir, false, 0)?;
    let source = if state.headers.is_empty() {
        // 展開しなかったときは、改行の形も含めて元のまま
        source.to_string()
    } else {
        state.out
    };
    Ok(Expanded {
        source,
        headers: state.headers,
        files: state.files,
        missing: state.missing,
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
                "atcoder-studio-expand-{}-{}",
                name,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
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
    fn reads_quoted_includes() {
        assert_eq!(quoted_include("#include \"a.hpp\""), Some("a.hpp"));
        assert_eq!(
            quoted_include("  #  include  \"x/b.h\" // c"),
            Some("x/b.h")
        );
        assert_eq!(quoted_include("#include <vector>"), None);
        assert_eq!(quoted_include("// #include \"a.hpp\""), None);
        assert_eq!(quoted_include("#include \"\""), None);
        assert!(is_pragma_once("#pragma once\n"));
        assert!(is_pragma_once("  # pragma   once  "));
        assert!(!is_pragma_once("#pragma GCC optimize(\"O3\")"));
    }

    #[test]
    fn expands_nested_headers_once_with_markers() {
        let dir = TempDir::new("nested");
        let lib = dir.0.join("lib");
        dir.write(
            "lib/segtree.hpp",
            "#pragma once\n#include \"util/common.hpp\"\nstruct Seg {};\n",
        );
        dir.write(
            "lib/fenwick.hpp",
            "#pragma once\n#include \"util/common.hpp\"\nstruct Fen {};",
        );
        dir.write(
            "lib/util/common.hpp",
            "#pragma once\n#include <vector>\nusing ll = long long;\n",
        );
        let main = dir.write("pkg/main.cpp", "");
        let source = "#include <bits/stdc++.h>\n#include \"segtree.hpp\"\n#include \"fenwick.hpp\"\nint main() {}\n";
        let dirs = SearchDirs {
            library: Some(&lib),
            ac_library: None,
        };
        let expanded = expand(&main, source, &dirs).unwrap();
        assert_eq!(
            expanded.source,
            "#include <bits/stdc++.h>\n\
             // ---- 展開：segtree.hpp ----\n\
             // ---- 展開：util/common.hpp ----\n\
             #include <vector>\n\
             using ll = long long;\n\
             // ---- ここまで：util/common.hpp ----\n\
             struct Seg {};\n\
             // ---- ここまで：segtree.hpp ----\n\
             // ---- 展開：fenwick.hpp ----\n\
             struct Fen {};\n\
             // ---- ここまで：fenwick.hpp ----\n\
             int main() {}\n"
        );
        assert_eq!(
            expanded.headers,
            ["segtree.hpp", "util/common.hpp", "fenwick.hpp"]
        );
        assert_eq!(expanded.files.len(), 3);
        assert!(expanded.missing.is_empty());
    }

    #[test]
    fn prefers_the_header_next_to_main() {
        let dir = TempDir::new("next");
        let lib = dir.0.join("lib");
        dir.write("lib/a.hpp", "int from_library;\n");
        dir.write("pkg/a.hpp", "int next_to_main;\n");
        let main = dir.write("pkg/main.cpp", "");
        let dirs = SearchDirs {
            library: Some(&lib),
            ac_library: None,
        };
        let expanded = expand(&main, "#include \"a.hpp\"\n", &dirs).unwrap();
        assert!(expanded.source.contains("int next_to_main;"));
        assert!(!expanded.source.contains("from_library"));
        assert_eq!(expanded.headers, ["a.hpp"]);
    }

    #[test]
    fn keeps_ac_library_and_reports_missing_headers() {
        let dir = TempDir::new("missing");
        let acl = dir.0.join("acl");
        dir.write("acl/atcoder/dsu", "// dsu\n");
        let main = dir.write("pkg/main.cpp", "");
        let dirs = SearchDirs {
            library: None,
            ac_library: Some(&acl),
        };
        let source = "#include \"atcoder/dsu\"\r\n#include \"nothing.hpp\"\r\n#include \"nothing.hpp\"\r\nint main() {}\r\n";
        let expanded = expand(&main, source, &dirs).unwrap();
        // 展開するものがなければ、元のまま（改行の形も）
        assert_eq!(expanded.source, source);
        assert!(expanded.headers.is_empty());
        assert_eq!(expanded.missing, ["nothing.hpp"]);
    }

    #[test]
    fn stops_cycles_and_skips_main_itself() {
        let dir = TempDir::new("cycle");
        dir.write("pkg/a.hpp", "#include \"b.hpp\"\nint a;\n");
        dir.write(
            "pkg/b.hpp",
            "#include \"a.hpp\"\n#include \"main.cpp\"\nint b;\n",
        );
        let main = dir.write("pkg/main.cpp", "");
        let dirs = SearchDirs {
            library: None,
            ac_library: None,
        };
        let expanded = expand(&main, "#include \"a.hpp\"\nint main() {}\n", &dirs).unwrap();
        assert_eq!(expanded.headers, ["a.hpp", "b.hpp"]);
        assert_eq!(expanded.source.matches("int a;").count(), 1);
        assert_eq!(expanded.source.matches("int b;").count(), 1);
        assert!(!expanded.source.contains("#include \"main.cpp\""));
        // UTF-8 でないヘッダーは読めないと返す
        fs::write(dir.0.join("pkg/bad.hpp"), [0xff, 0xfe]).unwrap();
        assert!(
            expand(&main, "#include \"bad.hpp\"\n", &dirs)
                .unwrap_err()
                .contains("bad.hpp")
        );
    }
}
