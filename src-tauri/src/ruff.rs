// Python の整形（Phase 41。D-265）
//
// ruff（`brew install ruff`。IDE はインストールしない）の `ruff format` を、ソースを標準入力で渡して動かし、
// 標準出力を整形したソースとして返す。`--stdin-filename` にファイルのパスを渡し、ファイルのフォルダで動かすので、
// そこから上にある pyproject.toml・ruff.toml を ruff が読む。ruff はソースを動かさない（整形するだけ）
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

// 整形の時間の上限（ほかの言語の整形と同じ 3 秒。D-135）
pub const TIMEOUT: Duration = Duration::from_secs(3);
// 出力の大きさの上限（整形したソース。エディタで開けるファイルは 1 MB まで）
const MAX_OUTPUT: usize = 4 * 1024 * 1024;

pub const NOT_FOUND: &str = "ruff が見つかりません。Python の整形には ruff を使います。ターミナルで brew install ruff を実行してください";

// 使う ruff。設定（settings.json の python.ruff）があればそれだけを使う。なければ /opt/homebrew/bin、PATH、
// /usr/local/bin の順（処理系の探し方と同じ。D-221）
pub fn find(configured: &str) -> Result<PathBuf, String> {
    if !configured.is_empty() {
        let path = PathBuf::from(configured);
        if path.is_absolute() && path.is_file() {
            return Ok(path);
        }
        return Err(format!(
            "settings.json の python.ruff（{configured}）が見つかりません。ruff の絶対パスを書くか、空にしてください"
        ));
    }
    crate::python::find_in(
        &["ruff"],
        std::env::var_os("PATH").as_deref(),
        &[
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
        ],
    )
    .ok_or_else(|| NOT_FOUND.to_string())
}

// source を ruff で整形して返す。file は整形するファイルの絶対パス（設定ファイルを探す場所と、エラーの表示に使う）
pub fn format(ruff: &Path, file: &Path, source: &str, timeout: Duration) -> Result<String, String> {
    let dir = file.parent().unwrap_or(Path::new("/"));
    let mut command = Command::new(ruff);
    command
        .current_dir(dir)
        .arg("format")
        .arg("--stdin-filename")
        .arg(file)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::toolchain::clear_ide_env(&mut command);
    let mut child = command
        .spawn()
        .map_err(|error| format!("ruff を起動できません：{error}"))?;
    // 渡す・受け取るを別のスレッドで行う（大きなソースで、パイプが詰まって止まらないように）
    let mut stdin = child.stdin.take().expect("piped");
    let input = source.as_bytes().to_vec();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let read_all = |mut pipe: Box<dyn Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe
                .by_ref()
                .take(MAX_OUTPUT as u64 + 1)
                .read_to_end(&mut bytes);
            bytes
        })
    };
    let stdout = read_all(Box::new(child.stdout.take().expect("piped")));
    let stderr = read_all(Box::new(child.stderr.take().expect("piped")));
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10))
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "ruff の整形が {} 秒で終わらなかったので、止めました",
                    timeout.as_secs()
                ));
            }
            Err(error) => return Err(format!("ruff を待てません：{error}")),
        }
    };
    let _ = writer.join();
    let stdout = stdout.join().unwrap_or_default();
    let stderr = stderr.join().unwrap_or_default();
    if !status.success() {
        // 構文エラーなど。ruff のメッセージの頭を出す
        let message = String::from_utf8_lossy(&stderr);
        let message: Vec<&str> = message
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(5)
            .collect();
        return Err(if message.is_empty() {
            "ruff で整形できませんでした".to_string()
        } else {
            format!("ruff で整形できませんでした：{}", message.join(" / "))
        });
    }
    if stdout.len() > MAX_OUTPUT {
        return Err("整形したソースが大きすぎるので、置き換えません".to_string());
    }
    String::from_utf8(stdout).map_err(|_| "ruff の出力を読めません".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("atcoder-studio-ruff-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    // ruff の代わりのスクリプト（本物の ruff がなくても、渡し方・受け取り方・失敗・時間切れを確かめる）
    fn fake(dir: &Path, body: &str) -> PathBuf {
        let path = dir.join("ruff");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn passes_the_source_and_arguments_and_returns_the_output() {
        let dir = TempDir::new("pass");
        // 引数と、動かしたフォルダを出し、標準入力を大文字にして返す
        let ruff = fake(&dir.0, r#"echo "$1|$2|$3|$4|$(pwd -P)" >&2; tr a-z A-Z"#);
        let file = dir.0.join("main.py");
        assert_eq!(format(&ruff, &file, "x=1\n", TIMEOUT).unwrap(), "X=1\n");
        // 大きなソースでも詰まらない
        let big = "a = 1\n".repeat(200_000);
        assert_eq!(
            format(&ruff, &file, &big, TIMEOUT).unwrap().len(),
            big.len()
        );
    }

    #[test]
    fn reports_failures_and_timeouts() {
        let dir = TempDir::new("fail");
        let ruff = fake(
            &dir.0,
            "cat >/dev/null; echo 'error: Failed to parse main.py:1:3: Expected an expression' >&2; exit 2",
        );
        let error = format(&ruff, &dir.0.join("main.py"), "x = (\n", TIMEOUT).unwrap_err();
        assert!(error.contains("Failed to parse"), "{error}");
        let slow = fake(&dir.0, "sleep 5");
        let started = Instant::now();
        let error = format(
            &slow,
            &dir.0.join("main.py"),
            "x\n",
            Duration::from_millis(300),
        )
        .unwrap_err();
        assert!(error.contains("終わらなかった"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(3));
        // 設定のパスが見つからなければ、そのことを出す
        assert!(find("/nowhere/ruff").unwrap_err().contains("python.ruff"));
        assert!(find("relative/ruff").is_err());
    }

    // 本物の ruff があれば、整形できること（なければ確かめずに終わる）
    #[test]
    fn formats_with_the_real_ruff_when_installed() {
        let Ok(ruff) = find("") else {
            return;
        };
        let dir = TempDir::new("real");
        let out = format(
            &ruff,
            &dir.0.join("main.py"),
            "x=[1,2 ,3]\nif x :  print( x )\n",
            TIMEOUT,
        )
        .unwrap();
        assert_eq!(out, "x = [1, 2, 3]\nif x:\n    print(x)\n");
        assert!(format(&ruff, &dir.0.join("main.py"), "x = (\n", TIMEOUT).is_err());
    }
}
