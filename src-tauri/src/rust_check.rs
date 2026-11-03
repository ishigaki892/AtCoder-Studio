// 展開した Rust のコードを、送る前にビルドして確かめる（Phase 29 作業 2。D-250・D-251）。
//
// パッケージの target/ の中に作業用のパッケージ（target/atcoder-studio-check/）を作り、まとめた main.rs と、
// 元のパッケージの依存（入れたライブラリを除き、ライブラリが使う依存を足したもの）・Cargo.lock を写して、
// CARGO_TARGET_DIR を元の target/ にして `cargo build --release` する。サンプルテストでリリースのビルドをした依存を
// 使い回すので、ふつうは数秒【推測】。依存の表は cargo metadata の答えから作る（toml などの依存は足さない）
use std::path::{Path, PathBuf};

use crate::rust_bundle::{Dependency, Metadata, Package};

const CHECK_DIR: &str = "atcoder-studio-check";
const CHECK_NAME: &str = "atcoder-studio-check";
// エラーを見せる数の上限
const MAX_ERRORS: usize = 3;

// TOML の文字列（JSON の文字列の書き方は、TOML の基本の文字列としても読める）
fn quote(text: &str) -> String {
    serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string())
}

// 依存の 1 行（`名前 = { … }`）。`package =` は名前を変えているときだけ書く（書くと、変えていなくても
// rename の扱いになり、コードの中の名前が lib の名前〔ac-library-rs なら ac_library〕でなく、キーの - を _ にしたものになる）
fn dependency_line(dependency: &Dependency) -> String {
    let (key, mut fields) = match &dependency.rename {
        Some(rename) => (
            rename.clone(),
            vec![format!("package = {}", quote(&dependency.name))],
        ),
        None => (dependency.name.clone(), Vec::new()),
    };
    match (&dependency.path, dependency.source.as_deref()) {
        (Some(path), _) => fields.push(format!("path = {}", quote(&path.to_string_lossy()))),
        (None, Some(source)) if source.starts_with("git+") => {
            let rest = &source["git+".len()..];
            let url = rest.split(['?', '#']).next().unwrap_or(rest);
            fields.push(format!("git = {}", quote(url)));
            if let Some((_, rev)) = rest.split_once('#') {
                fields.push(format!("rev = {}", quote(rev)));
            }
        }
        _ => {}
    }
    let from_git = dependency
        .source
        .as_deref()
        .is_some_and(|source| source.starts_with("git+"));
    if (dependency.path.is_none() || dependency.req != "*") && !from_git {
        fields.push(format!("version = {}", quote(&dependency.req)));
    }
    if !dependency.uses_default_features {
        fields.push("default-features = false".to_string());
    }
    if !dependency.features.is_empty() {
        let features: Vec<String> = dependency.features.iter().map(|f| quote(f)).collect();
        fields.push(format!("features = [{}]", features.join(", ")));
    }
    format!("{} = {{ {} }}", quote(&key), fields.join(", "))
}

// 作業用のパッケージの Cargo.toml
pub fn manifest(edition: &str, dependencies: &[Dependency]) -> String {
    let mut plain = Vec::new();
    let mut targeted: Vec<(String, String)> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    for dependency in dependencies.iter().filter(|d| d.kind.is_none()) {
        let key = dependency
            .rename
            .clone()
            .unwrap_or_else(|| dependency.name.clone());
        // 同じ名前は 1 つだけ（先のもの）
        if keys.contains(&key) {
            continue;
        }
        keys.push(key);
        let line = dependency_line(dependency);
        match &dependency.target {
            Some(target) => targeted.push((target.clone(), line)),
            None => plain.push(line),
        }
    }
    let edition = if edition.is_empty() { "2024" } else { edition };
    let mut text = format!(
        "# AtCoder Studio が、展開したコードを送る前に確かめるために作った（Phase 29。消してよい）\n[package]\nname = \"{CHECK_NAME}\"\nversion = \"0.1.0\"\nedition = {}\npublish = false\n\n[dependencies]\n",
        quote(edition)
    );
    for line in plain {
        text.push_str(&line);
        text.push('\n');
    }
    for (target, line) in targeted {
        text.push_str(&format!(
            "\n[target.{}.dependencies]\n{line}\n",
            quote(&target)
        ));
    }
    text.push_str("\n[workspace]\n");
    text
}

// ビルドの JSON の出力から、エラーの見せ方（rendered）を取り出す
fn errors(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| {
            value["reason"] == "compiler-message" && value["message"]["level"] == "error"
        })
        .filter_map(|value| value["message"]["rendered"].as_str().map(str::to_string))
        .take(MAX_ERRORS)
        .collect()
}

// 確かめる。package・metadata は元のパッケージのもの、bundled は入れたライブラリの依存の名前（作業用のパッケージに入れない）、
// extra はライブラリが使う依存（足す）
pub fn check(
    cargo: &Path,
    metadata: &Metadata,
    package: &Package,
    source: &str,
    bundled: &[String],
    extra: &[Dependency],
) -> Result<(), String> {
    let target_dir: PathBuf = metadata
        .target_directory
        .clone()
        .unwrap_or_else(|| metadata.workspace_root.join("target"));
    let dir = target_dir.join(CHECK_DIR);
    let failed =
        |error: std::io::Error| format!("展開したコードを確かめる準備ができません（{error}）");
    std::fs::create_dir_all(dir.join("src")).map_err(failed)?;
    let dependencies: Vec<Dependency> = package
        .dependencies
        .iter()
        .filter(|dependency| !bundled.contains(&dependency.name))
        .chain(extra)
        .cloned()
        .collect();
    std::fs::write(
        dir.join("Cargo.toml"),
        manifest(&package.edition, &dependencies),
    )
    .map_err(failed)?;
    std::fs::write(dir.join("src/main.rs"), source).map_err(failed)?;
    // 元と同じ版のクレートを使う（なければ cargo が決める）
    match std::fs::read(metadata.workspace_root.join("Cargo.lock")) {
        Ok(lock) => std::fs::write(dir.join("Cargo.lock"), lock).map_err(failed)?,
        Err(_) => {
            let _ = std::fs::remove_file(dir.join("Cargo.lock"));
        }
    }
    let output = crate::toolchain::cargo_command(cargo, &dir)
        .args(["build", "--release", "--message-format=json"])
        .env("CARGO_TARGET_DIR", &target_dir)
        .output()
        .map_err(|error| {
            format!("展開したコードを確かめられません（cargo を起動できません：{error}）")
        })?;
    if output.status.success() {
        return Ok(());
    }
    let found = errors(&String::from_utf8_lossy(&output.stdout));
    let detail = if found.is_empty() {
        // コンパイルのエラーではない失敗（依存を解決できないなど）
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .filter(|line| !line.trim().is_empty())
            .take(12)
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        found.join("\n")
    };
    Err(format!(
        "モジュール・ライブラリを展開したコードをビルドすると、エラーがあるので送りません（src/main.rs の行は、展開したあとの行です）：\n{detail}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_the_dependency_table() {
        let registry = Some("registry+https://github.com/rust-lang/crates.io-index");
        let deps = vec![
            Dependency {
                req: "^0.5.0".into(),
                ..Dependency::named("proconio", registry, None)
            },
            Dependency {
                req: "^0.2.0".into(),
                rename: Some("ac".into()),
                uses_default_features: false,
                features: vec!["x".into()],
                ..Dependency::named("ac-library-rs", registry, None)
            },
            Dependency {
                path: Some(PathBuf::from("/lib/other")),
                ..Dependency::named("other", None, None)
            },
            Dependency {
                req: "^1".into(),
                target: Some("cfg(unix)".into()),
                ..Dependency::named("libc", registry, None)
            },
            Dependency::named("dev-only", registry, Some("dev")),
            // 同じ名前は先のもの
            Dependency {
                req: "^9".into(),
                ..Dependency::named("proconio", registry, None)
            },
        ];
        let text = manifest("2024", &deps);
        assert!(text.contains("edition = \"2024\""));
        assert!(text.contains("\"proconio\" = { version = \"^0.5.0\" }\n"));
        assert!(text.contains("\"ac\" = { package = \"ac-library-rs\", version = \"^0.2.0\", default-features = false, features = [\"x\"] }"));
        assert!(text.contains("\"other\" = { path = \"/lib/other\" }"));
        assert!(text.contains("[target.\"cfg(unix)\".dependencies]\n\"libc\""));
        assert!(!text.contains("dev-only"));
        assert!(!text.contains("^9"));
        assert!(text.trim_end().ends_with("[workspace]"));
    }

    #[test]
    fn reads_compile_errors() {
        let stdout = r#"{"reason":"compiler-artifact"}
{"reason":"compiler-message","message":{"level":"warning","rendered":"warning: x"}}
{"reason":"compiler-message","message":{"level":"error","rendered":"error[E0425]: cannot find value `y`"}}
not json"#;
        assert_eq!(errors(stdout), ["error[E0425]: cannot find value `y`"]);
    }

    // 本物の cargo で、std だけのコードを確かめる（通る・通らない）。cargo がなければ飛ばす
    #[test]
    fn builds_a_check_package_with_cargo() {
        let Some(cargo) = crate::toolchain::find_cargo() else {
            eprintln!("cargo が見つからないので飛ばす");
            return;
        };
        let started = std::time::Instant::now();
        let dir =
            std::env::temp_dir().join(format!("atcoder-studio-rust-check-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let metadata = Metadata {
            packages: Vec::new(),
            workspace_root: dir.clone(),
            target_directory: Some(dir.join("target")),
        };
        let package: Package = serde_json::from_value(serde_json::json!({
            "name": "pkg", "edition": "2024", "manifest_path": dir.join("Cargo.toml"),
            "targets": [], "dependencies": [],
        }))
        .unwrap();
        assert_eq!(
            check(
                &cargo,
                &metadata,
                &package,
                "mod a { pub fn f() -> i32 { 1 } }\nfn main() { println!(\"{}\", a::f()); }\n",
                &[],
                &[]
            ),
            Ok(())
        );
        let error = check(
            &cargo,
            &metadata,
            &package,
            "fn main() { y(); }\n",
            &[],
            &[],
        )
        .unwrap_err();
        assert!(
            error.contains("E0425") || error.contains("cannot find function"),
            "{error}"
        );
        assert!(dir.join("target/atcoder-studio-check/Cargo.toml").is_file());
        assert!(dir.join("target/release/atcoder-studio-check").is_file());
        // lib の名前がパッケージの名前と違う依存（ac-library-rs の ac_library と同じ形）を、lib の名前で使える
        let lib = dir.join("other-pkg");
        std::fs::create_dir_all(lib.join("src")).unwrap();
        std::fs::write(
            lib.join("Cargo.toml"),
            "[package]\nname = \"other-pkg\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\nname = \"other_lib\"\n",
        )
        .unwrap();
        std::fs::write(lib.join("src/lib.rs"), "pub fn two() -> i32 { 2 }\n").unwrap();
        let dependency = Dependency {
            path: Some(lib),
            ..Dependency::named("other-pkg", None, None)
        };
        assert_eq!(
            check(
                &cargo,
                &metadata,
                &package,
                "fn main() { println!(\"{}\", other_lib::two()); }\n",
                &[],
                &[dependency]
            ),
            Ok(())
        );
        eprintln!("cargo で確かめた（{:?}）", started.elapsed());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
