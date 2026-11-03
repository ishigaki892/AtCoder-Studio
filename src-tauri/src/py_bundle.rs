// Python の自作のモジュールを main.py に埋め込む（Phase 30 作業 1。D-249・D-252）。
//
// - 探す場所は main.py のフォルダ → アプリのフォルダの libraries/python/（手元の実行の sys.path と同じ順）
// - main.py から import をたどる（FIND。処理系の ast で読み、importlib.machinery.PathFinder で場所を探す。
//   ファイルは動かさない）。相対 import・サブモジュール・入れ子もたどる。標準ライブラリはたどらない
// - 見つかったモジュールのソースを main.py の先頭（docstring と `from __future__` の後ろ）に文字列で入れ、
//   sys.meta_path の finder で読ませる（検討：docs/phase27/03-rust-python-study.md §2.2 の案 P1）
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// 自作のライブラリの置き場所（アプリのフォルダの中。D-252）。この下に、モジュールの .py かパッケージのフォルダを置く
pub const LIBRARY_DIR: &str = "libraries/python";

// アプリのフォルダに自作のライブラリのフォルダがあれば、その場所（canonicalize 済み）
pub fn library_dir(app_dir: &Path) -> Option<PathBuf> {
    app_dir
        .join(LIBRARY_DIR)
        .canonicalize()
        .ok()
        .filter(|dir| dir.is_dir())
}

// 自作のライブラリのフォルダ。なければ空のフォルダを作る（設定の画面の「ライブラリのフォルダを開く（Python）」）
pub fn ensure_library_dir(app_dir: &Path) -> Result<PathBuf, String> {
    let dir = app_dir.join(LIBRARY_DIR);
    std::fs::create_dir_all(&dir).map_err(|_| "ライブラリのフォルダを作れません".to_string())?;
    Ok(dir)
}

// import をたどる小さな命令（`<処理系> -I -B -c FIND main.py 探す場所…`）。1 行に 1 つ、タブで区切って出す：
// - `P<TAB>行`：main.py の、入れる場所（この行の後ろ。docstring と from __future__ の最後の行。なければ 0）
// - `M<TAB>名前<TAB>パッケージなら 1<TAB>場所<TAB>__init__.py のないパッケージなら 1`：入れるモジュール（見つけた順）
// - `I<TAB>名前`：探す場所にない、標準ライブラリでもない一番上の名前（ジャッジにあるかを確かめる）
// - `S<TAB>名前<TAB>場所`：標準ライブラリと同じ名前で、探す場所にあるもの（入れない）
// - `R`：main.py の中の相対 import（展開できない）
// - `E<TAB>ファイル<TAB>行<TAB>メッセージ`：読めないファイル（構文の誤りなど）。1 で終わる
// 手元の CPython 3.11 以降・PyPy 3.11 で動く書き方にする
const FIND: &str = r#"import ast, sys, importlib.machinery as m
main, dirs = sys.argv[1], sys.argv[2:]
std = set(sys.stdlib_module_names) | set(sys.builtin_module_names)
found, others, shadows, relative = set(), [], [], [False]
def out(*parts):
    print(*parts, sep='\t')
def spec_of(name):
    path, parts, spec = dirs, name.split('.'), None
    for i, part in enumerate(parts):
        if not part.isidentifier():
            return None
        spec = m.PathFinder.find_spec('.'.join(parts[:i + 1]), path)
        if spec is None:
            return None
        if i < len(parts) - 1:
            path = spec.submodule_search_locations
            if not path:
                return None
    return spec
def read(file):
    try:
        with open(file, 'rb') as f:
            return ast.parse(f.read(), file)
    except SyntaxError as e:
        out('E', file, e.lineno or 0, e.msg)
    except (ValueError, OSError) as e:
        out('E', file, 0, str(e))
    sys.exit(1)
def names_of(node, package, is_main):
    if isinstance(node, ast.Import):
        return [a.name for a in node.names]
    if not isinstance(node, ast.ImportFrom):
        return []
    if node.level:
        if is_main:
            relative[0] = True
            return []
        base = package.split('.') if package else []
        if not base or node.level - 1 >= len(base):
            return []
        base = base[:len(base) - (node.level - 1)]
        module = '.'.join(base + ([node.module] if node.module else []))
    else:
        module = node.module
    return [module] + [module + '.' + a.name for a in node.names if a.name != '*']
def visit(file, package, is_main):
    tree = read(file)
    if is_main:
        at = 0
        for i, node in enumerate(tree.body):
            docstring = i == 0 and isinstance(node, ast.Expr) and isinstance(node.value, ast.Constant) and isinstance(node.value.value, str)
            future = isinstance(node, ast.ImportFrom) and node.module == '__future__' and not node.level
            if not (docstring or future):
                break
            at = node.end_lineno
        out('P', at)
    for node in ast.walk(tree):
        for name in names_of(node, package, is_main):
            parts = name.split('.')
            top = parts[0]
            if top in std:
                if top not in shadows:
                    shadows.append(top)
                    spec = spec_of(top)
                    if spec is not None:
                        out('S', top, spec.origin or list(spec.submodule_search_locations or [''])[0])
                continue
            for i in range(len(parts)):
                sub = '.'.join(parts[:i + 1])
                if sub in found:
                    continue
                spec = spec_of(sub)
                if spec is None:
                    if i == 0 and top not in others:
                        others.append(top)
                    break
                found.add(sub)
                package_like = spec.submodule_search_locations is not None
                namespace = spec.origin is None or spec.origin == 'namespace'
                where = list(spec.submodule_search_locations)[0] if namespace else spec.origin
                out('M', sub, int(package_like), where, int(namespace))
                if not namespace:
                    visit(spec.origin, sub if package_like else sub.rpartition('.')[0], False)
visit(main, '', True)
for name in others:
    out('I', name)
if relative[0]:
    out('R')
"#;

// 入れるモジュール
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
    // import の名前（`mylib.seg` など）
    pub name: String,
    // パッケージ（`__init__.py` か、`__init__.py` のないフォルダ）
    pub package: bool,
    // ファイルの場所（`__init__.py` のないパッケージはフォルダ）
    pub path: PathBuf,
    // `__init__.py` のないパッケージ（中身は空として入れる）
    pub namespace: bool,
}

// import をたどった結果
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Found {
    // main.py の、入れる場所（この行の後ろ）
    pub insert_after: usize,
    pub modules: Vec<Module>,
    // 探す場所になく、標準ライブラリでもない一番上の名前
    pub others: Vec<String>,
    // 標準ライブラリと同じ名前で、探す場所にあるもの（名前と場所）
    pub shadows: Vec<(String, PathBuf)>,
    // main.py の中に相対 import がある
    pub relative: bool,
}

// 見せ方：探す場所からの相対パス（libraries/python/ のものは `libraries/python/…`）
pub fn shown(path: &Path, package_dir: &Path, library: Option<&Path>) -> String {
    let join = |relative: &Path| {
        relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/")
    };
    if let Ok(relative) = path.strip_prefix(package_dir) {
        return join(relative);
    }
    if let Some(library) = library
        && let Ok(relative) = path.strip_prefix(library)
    {
        return format!("{LIBRARY_DIR}/{}", join(relative));
    }
    path.to_string_lossy().into_owned()
}

// たどれなかったわけ
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FindError {
    // 読めないファイル（構文の誤りなど）がある。提出では送らない
    Unreadable(String),
    // 処理系を動かせないなど。提出では、入れずに注意を出す
    Failed(String),
}

impl FindError {
    pub fn message(&self) -> &str {
        match self {
            FindError::Unreadable(message) | FindError::Failed(message) => message,
        }
    }
}

// main.py から import をたどる。package_dir は main.py のフォルダ、library は libraries/python/（なければ None）。
// どれも canonicalize 済み。読めないファイル（構文の誤りなど）があれば、その場所を返す
pub fn find(
    interpreter: &Path,
    main: &Path,
    package_dir: &Path,
    library: Option<&Path>,
) -> Result<Found, FindError> {
    let mut command = Command::new(interpreter);
    command
        .current_dir(package_dir)
        .args(["-I", "-B", "-c", FIND])
        .arg(main)
        .arg(package_dir)
        .args(library);
    crate::toolchain::clear_ide_env(&mut command);
    let output = command.output().map_err(|error| {
        FindError::Failed(format!(
            "{} を起動できません：{error}",
            interpreter.display()
        ))
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut found = Found::default();
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        match parts.as_slice() {
            ["P", at] => found.insert_after = at.trim().parse().unwrap_or(0),
            ["M", name, package, path, namespace] => found.modules.push(Module {
                name: name.to_string(),
                package: *package == "1",
                path: PathBuf::from(path),
                namespace: namespace.trim() == "1",
            }),
            ["I", name] => found.others.push(name.to_string()),
            ["S", name, path] => found.shadows.push((name.to_string(), PathBuf::from(path))),
            ["R"] => found.relative = true,
            ["E", file, row, message @ ..] => {
                let file = shown(Path::new(file), package_dir, library);
                let row = row.trim();
                let at = if row == "0" {
                    format!("「{file}」")
                } else {
                    format!("「{file}」の {row} 行目")
                };
                return Err(FindError::Unreadable(format!(
                    "自分のモジュールを入れるために import をたどると、{at}を読めません：{}",
                    message.join("\t")
                )));
            }
            _ => {}
        }
    }
    if !output.status.success() {
        return Err(FindError::Failed(
            "import を読めません（処理系を動かせませんでした）".to_string(),
        ));
    }
    Ok(found)
}

// Python の文字列（`'''…'''`）にする。中身の行はそのまま見えるようにし、`\`・続く `'`・最後の `'`・
// 改行とタブ以外の制御文字だけを書き換える（最初の改行は `\` で消す）
fn literal(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    out.push_str("'''\\\n");
    for (at, &c) in chars.iter().enumerate() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\'' if at + 1 == chars.len() || chars[at + 1] == '\'' => out.push_str("\\'"),
            '\r' => out.push_str("\\r"),
            '\n' | '\t' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{7f}' => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push_str("'''");
    out
}

// ソースの頭の BOM を外し、改行を LF にそろえる（処理系がファイルを読むときと同じ形にする）
fn normalized(text: &str) -> String {
    text.strip_prefix('\u{feff}')
        .unwrap_or(text)
        .replace("\r\n", "\n")
}

// 埋め込んだ結果
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundled {
    pub source: String,
    // 確認のダイアログに出す見せ方（`util（util.py）` など）
    pub modules: Vec<String>,
    // 入れたファイル（提出前の「テストのあとに変わったか」で見る。C-10）
    pub files: Vec<PathBuf>,
}

// main.py に、found のモジュールを入れる。入れるものがなければ None。read はファイルを読む方法（テストで差し替える）
pub fn bundle(
    main_source: &str,
    found: &Found,
    package_dir: &Path,
    library: Option<&Path>,
    read: &dyn Fn(&Path) -> Result<String, String>,
) -> Result<Option<Bundled>, String> {
    if found.modules.is_empty() {
        return Ok(None);
    }
    let mut table = String::new();
    let mut modules = Vec::new();
    let mut files = Vec::new();
    for module in &found.modules {
        let place = shown(&module.path, package_dir, library);
        let text = if module.namespace {
            String::new()
        } else {
            files.push(module.path.clone());
            normalized(&read(&module.path)?)
        };
        modules.push(format!("{}（{place}）", module.name));
        table.push_str(&format!(
            "# {}（{place}）\n_ATS_SOURCES[\"{}\"] = ({}, {})\n",
            module.name,
            module.name,
            if module.package { "True" } else { "False" },
            literal(&text)
        ));
    }
    let names: Vec<&str> = found.modules.iter().map(|m| m.name.as_str()).collect();
    let block = format!(
        "# ---- 展開：自作のモジュール（{}）。AtCoder Studio が入れた ----\n\
         import sys as _ats_sys, importlib.abc as _ats_abc, importlib.util as _ats_util\n\
         _ATS_SOURCES = {{}}\n\
         {table}\
         class _AtsFinder(_ats_abc.MetaPathFinder, _ats_abc.Loader):\n    \
             def find_spec(self, name, path=None, target=None):\n        \
                 if name not in _ATS_SOURCES:\n            \
                     return None\n        \
                 return _ats_util.spec_from_loader(name, self, origin=\"<\" + name + \">\", is_package=_ATS_SOURCES[name][0])\n    \
             def create_module(self, spec):\n        \
                 return None\n    \
             def exec_module(self, module):\n        \
                 exec(compile(_ATS_SOURCES[module.__name__][1], \"<\" + module.__name__ + \">\", \"exec\"), module.__dict__)\n\
         _ats_sys.meta_path.insert(0, _AtsFinder())\n\
         # ---- 展開ここまで ----\n",
        names.join("・")
    );
    let main = normalized(main_source);
    let lines: Vec<&str> = main.split_inclusive('\n').collect();
    // docstring・from __future__ がなければ、頭の `#!` とエンコーディングの宣言（2 行目まで）の後ろに入れる
    let mut at = found.insert_after.min(lines.len());
    if at == 0 {
        while at < lines.len().min(2)
            && (lines[at].starts_with("#!")
                || (lines[at].starts_with('#') && lines[at].contains("coding")))
        {
            at += 1;
        }
    }
    let mut source = String::with_capacity(main.len() + block.len() + 1);
    for line in &lines[..at] {
        source.push_str(line);
    }
    if !source.is_empty() && !source.ends_with('\n') {
        source.push('\n');
    }
    source.push_str(&block);
    for line in &lines[at..] {
        source.push_str(line);
    }
    Ok(Some(Bundled {
        source,
        modules,
        files,
    }))
}

// 構文だけを確かめる小さな命令（標準入力のソースを compile する。動かさない）。誤りなら「行<TAB>メッセージ」を出して 1 で終わる
const CHECK: &str = "import sys\ntry:\n    compile(sys.stdin.buffer.read(), 'main.py', 'exec')\nexcept (SyntaxError, ValueError) as e:\n    print(getattr(e, 'lineno', None) or 0, getattr(e, 'msg', None) or str(e), sep='\\t')\n    sys.exit(1)\n";

// まとめた main.py の構文を、手元の処理系で確かめる（D-252。C++ の -fsyntax-only に当たる）
pub fn check_syntax(interpreter: &Path, source: &str) -> Result<(), String> {
    use std::io::Write;
    let mut command = Command::new(interpreter);
    command
        .args(["-I", "-B", "-c", CHECK])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::toolchain::clear_ide_env(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("{} を起動できません：{error}", interpreter.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        // 書き終えたら閉じる（処理系が読み終えられるように）
        let _ = stdin.write_all(source.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("{} を待てません：{error}", interpreter.display()))?;
    if output.status.success() {
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = match stdout.trim().split_once('\t') {
        Some((row, message)) if row != "0" => format!("{row} 行目：{message}"),
        Some((_, message)) => message.to_string(),
        None => stdout.trim().to_string(),
    };
    Err(format!(
        "自分のモジュールを入れた main.py に構文の誤りがあるので送りません（行は、入れたあとの行です）：{detail}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::python::{Interpreter, PythonSettings};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "atcoder-studio-py-bundle-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.canonicalize().unwrap()
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    // 手元の処理系（CPython 3.13 と PyPy のうち、あるもの）
    fn interpreters() -> Vec<PathBuf> {
        [Interpreter::Cpython, Interpreter::Pypy]
            .into_iter()
            .filter_map(|interpreter| {
                crate::python::find_interpreter(&PythonSettings {
                    interpreter,
                    ..PythonSettings::default()
                })
                .ok()
            })
            .collect()
    }

    fn run(interpreter: &Path, dir: &Path, script: &str) -> std::process::Output {
        let mut command = Command::new(interpreter);
        command.current_dir(dir).args(["-E", "-B", script]);
        crate::toolchain::clear_ide_env(&mut command);
        command.output().unwrap()
    }

    #[test]
    fn writes_python_string_literals() {
        let texts = [
            "",
            "a = 'x'\nb = \"y\"\n",
            "s = '''doc'''\n",
            "ends with '",
            "ends with ''",
            "back\\slash \\n \\\\ \\'",
            "crlf\r\nlone\rcr",
            "tab\tand\u{7}bell and \u{0} nul",
            "日本語 と é",
            "x = r'\\''\n",
        ];
        // 行がそのまま見える
        assert_eq!(literal("a\nb\n"), "'''\\\na\nb\n'''");
        assert_eq!(literal("x'"), "'''\\\nx\\''''");
        let Some(interpreter) = interpreters().into_iter().next() else {
            eprintln!("Python の処理系がないので、文字列を読み戻すのは飛ばす");
            return;
        };
        let dir = temp_dir("literal");
        let mut script = String::from("import json\nvalues = [\n");
        for text in texts {
            script.push_str(&literal(text));
            script.push_str(",\n");
        }
        script.push_str("]\nprint(json.dumps(values))\n");
        write(&dir.join("check.py"), &script);
        let output = run(&interpreter, &dir, "check.py");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let values: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(values, texts);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn inserts_after_the_docstring_and_future_imports() {
        let found = |insert_after| Found {
            insert_after,
            modules: vec![Module {
                name: "util".into(),
                package: false,
                path: PathBuf::from("/p/util.py"),
                namespace: false,
            }],
            ..Found::default()
        };
        let read = |_: &Path| Ok("X = 1\n".to_string());
        let dir = Path::new("/p");
        let bundled = bundle(
            "\u{feff}\"\"\"doc\"\"\"\r\nfrom __future__ import annotations\r\nimport util\r\n",
            &found(2),
            dir,
            None,
            &read,
        )
        .unwrap()
        .unwrap();
        assert!(bundled.source.starts_with(
            "\"\"\"doc\"\"\"\nfrom __future__ import annotations\n# ---- 展開：自作のモジュール（util）"
        ));
        assert!(
            bundled
                .source
                .ends_with("# ---- 展開ここまで ----\nimport util\n")
        );
        assert_eq!(bundled.modules, ["util（util.py）"]);
        assert_eq!(bundled.files, [PathBuf::from("/p/util.py")]);
        // 頭の #! とエンコーディングの宣言の後ろ
        let bundled = bundle(
            "#!/usr/bin/env python3\n# -*- coding: utf-8 -*-\nimport util",
            &found(0),
            dir,
            None,
            &read,
        )
        .unwrap()
        .unwrap();
        assert!(
            bundled
                .source
                .starts_with("#!/usr/bin/env python3\n# -*- coding: utf-8 -*-\n# ---- 展開")
        );
        assert!(bundled.source.ends_with("ここまで ----\nimport util"));
        // 入れるものがなければ何もしない
        assert_eq!(
            bundle("import sys\n", &Found::default(), dir, None, &read).unwrap(),
            None
        );
    }

    // 本物の処理系で、たどって入れたものを、別のフォルダに 1 つだけ置いて動かす（CPython・PyPy のうち、あるもの）
    #[test]
    fn bundles_modules_that_run_alone() {
        let interpreters = interpreters();
        if interpreters.is_empty() {
            eprintln!("Python の処理系がないので飛ばす");
            return;
        }
        let base = temp_dir("run");
        let package = base.join("abc/a");
        let library = base.join("libraries/python");
        write(
            &package.join("main.py"),
            "\"\"\"main\"\"\"\nfrom __future__ import annotations\nimport sys\nfrom util import twice\nfrom mylib.seg import Seg\nimport mylib\nimport space.inner as inner\nimport heapq\ntry:\n    import sortedcontainers\nexcept ImportError:\n    pass\nprint(twice(3), Seg().v, mylib.NAME, inner.VALUE, heapq.nsmallest(1, [3, 1]))\n",
        );
        write(
            &package.join("util.py"),
            "def twice(x):\n    return 2 * x\n",
        );
        // 隣のものが先（libraries/python/ の util は入らない）
        write(&library.join("util.py"), "def twice(x):\n    return 0\n");
        write(
            &library.join("mylib/__init__.py"),
            "from .seg import Seg\nNAME = 'lib'\n",
        );
        write(
            &library.join("mylib/seg.py"),
            "import heapq\nfrom . import helper\nfrom .. import nothing_above\nclass Seg:\n    v = helper.ONE\n",
        );
        write(
            &library.join("mylib/helper.py"),
            "import numpy_like_missing\nONE = '''1'''\n",
        );
        // __init__.py のないパッケージ
        write(&library.join("space/inner.py"), "VALUE = 'ns'\n");
        // 標準ライブラリと同じ名前は入れない
        write(&library.join("heapq.py"), "raise RuntimeError\n");
        // helper の import numpy_like_missing と seg の from .. import は動かすと失敗するので、読むときに外して動かす
        for interpreter in &interpreters {
            let found = find(
                interpreter,
                &package.join("main.py"),
                &package,
                Some(&library),
            )
            .unwrap();
            assert_eq!(found.insert_after, 2);
            let names: Vec<&str> = found.modules.iter().map(|m| m.name.as_str()).collect();
            assert_eq!(
                names,
                [
                    "util",
                    "mylib",
                    "mylib.seg",
                    "mylib.helper",
                    "space",
                    "space.inner"
                ],
                "{interpreter:?}"
            );
            assert!(found.modules[0].path.starts_with(&package));
            assert!(found.modules[4].namespace && found.modules[4].package);
            // 入れ子のモジュールの中で先に見つかる
            assert_eq!(found.others, ["numpy_like_missing", "sortedcontainers"]);
            assert_eq!(
                found.shadows,
                [("heapq".to_string(), library.join("heapq.py"))]
            );
            assert!(!found.relative);
            let read = |path: &Path| {
                let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
                Ok(text
                    .replace("import numpy_like_missing\n", "")
                    .replace("from .. import nothing_above\n", ""))
            };
            let bundled = bundle(
                &std::fs::read_to_string(package.join("main.py")).unwrap(),
                &found,
                &package,
                Some(&library),
                &read,
            )
            .unwrap()
            .unwrap();
            assert!(
                bundled
                    .modules
                    .contains(&"mylib.seg（libraries/python/mylib/seg.py）".to_string())
            );
            assert!(
                bundled
                    .modules
                    .contains(&"space（libraries/python/space）".to_string())
            );
            assert_eq!(bundled.files.len(), 5);
            check_syntax(interpreter, &bundled.source).unwrap();
            let alone = base.join("alone");
            write(&alone.join("Main.py"), &bundled.source);
            let output = run(interpreter, &alone, "Main.py");
            assert!(
                output.status.success(),
                "{interpreter:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                String::from_utf8_lossy(&output.stdout),
                "6 1 lib ns [1]\n",
                "{interpreter:?}"
            );
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn reports_unreadable_modules_and_relative_imports() {
        let Some(interpreter) = interpreters().into_iter().next() else {
            eprintln!("Python の処理系がないので飛ばす");
            return;
        };
        let base = temp_dir("errors");
        let package = base.join("a");
        let library = base.join("lib");
        write(&package.join("main.py"), "from . import x\nimport broken\n");
        write(&library.join("broken.py"), "def f(:\n    pass\n");
        let error = find(
            &interpreter,
            &package.join("main.py"),
            &package,
            Some(&library),
        )
        .unwrap_err();
        assert!(matches!(error, FindError::Unreadable(_)));
        assert!(
            error
                .message()
                .contains("「libraries/python/broken.py」の 1 行目"),
            "{error:?}"
        );
        write(&package.join("main.py"), "from . import x\nprint(1)\n");
        let found = find(
            &interpreter,
            &package.join("main.py"),
            &package,
            Some(&library),
        )
        .unwrap();
        assert!(found.relative && found.modules.is_empty());
        let error = check_syntax(&interpreter, "x = 1\ndef f(:\n").unwrap_err();
        assert!(error.contains("2 行目"), "{error}");
        assert_eq!(check_syntax(&interpreter, "print(1)\n"), Ok(()));
        let _ = std::fs::remove_dir_all(&base);
    }
}
