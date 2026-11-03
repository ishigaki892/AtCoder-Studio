// アプリのフォルダのファイルを、IDE のタブで開いて編集する（D-247）。
//
// - 開けるのは settings.json と、言語のスニペットのファイル（snippets-rust.json・snippets-cpp.json・snippets-python.json）だけ。
//   画面からは名前だけを受け取り、この一覧にない名前は拒む（アプリのフォルダのほかのファイル〔state.json など〕は開かない）
// - なければ、今までの「外のエディタで開く」と同じく、既定の中身で作ってから開く
// - 読み書きの決まり（1 MB まで・UTF-8・外での変更の確かめ・直接の上書き）は、作業領域のファイルと同じ（workspace.rs）。
//   シンボリックリンクなら、リンクの先を読み書きする（C-7 と同じ扱い）
use std::path::{Path, PathBuf};

use crate::language::Language;
use crate::snippets;
use crate::storage;
use crate::workspace::{self, ReadResult, Stamp, WriteResult};

const LANGUAGES: [Language; 3] = [Language::Rust, Language::Cpp, Language::Python];

// 名前から、場所と（スニペットなら）言語を決める。一覧にない名前なら Err
fn locate(dir: &Path, name: &str) -> Result<(PathBuf, Option<Language>), String> {
    if name == storage::SETTINGS_FILE {
        return Ok((dir.join(storage::SETTINGS_FILE), None));
    }
    LANGUAGES
        .into_iter()
        .find(|&language| snippets::file_name(language) == name)
        .map(|language| (dir.join(name), Some(language)))
        .ok_or_else(|| format!("「{name}」は開けません"))
}

// なければ既定の中身で作り、場所を返す
fn ensure(dir: &Path, name: &str) -> Result<PathBuf, String> {
    let (path, language) = locate(dir, name)?;
    match language {
        Some(language) => snippets::ensure_file(dir, language),
        None => {
            if std::fs::symlink_metadata(&path).is_err() {
                storage::write_default_settings(dir)?;
            }
            Ok(path)
        }
    }
}

pub fn read(dir: &Path, name: &str) -> Result<ReadResult, String> {
    workspace::read_text_at(&ensure(dir, name)?)
}

// 既存のファイルに書く。expected を渡すと、外で変わっていたら書かずに Conflict を返す（D-038 と同じ）
pub fn write(
    dir: &Path,
    name: &str,
    content: &str,
    expected: Option<&Stamp>,
) -> Result<WriteResult, String> {
    let (path, _) = locate(dir, name)?;
    workspace::write_existing_at(&path, content, expected)
}

// 今の更新日時と大きさ（窓に戻ったときの、外での変更の確かめ）。ない・開けない名前なら None
pub fn stamps(dir: &Path, names: &[String]) -> Vec<Option<Stamp>> {
    names
        .iter()
        .map(|name| {
            let (path, _) = locate(dir, name).ok()?;
            path.is_file()
                .then(|| workspace::stamp_of(&path).ok())
                .flatten()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "atcoder-studio-app-files-{}-{}",
                name,
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            TempDir(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn opens_only_settings_and_snippets() {
        let dir = TempDir::new("names");
        for name in [
            "state.json",
            "trusted.json",
            "../settings.json",
            "templates/main.rs",
            "",
        ] {
            assert!(read(&dir.0, name).is_err(), "{name}");
            assert!(write(&dir.0, name, "x", None).is_err(), "{name}");
        }
        assert!(!dir.0.join("state.json").exists());
        assert_eq!(stamps(&dir.0, &["state.json".to_string()]), vec![None]);
    }

    #[test]
    fn creates_missing_files_with_defaults() {
        let dir = TempDir::new("defaults");
        let settings = read(&dir.0, "settings.json").unwrap();
        assert!(settings.content.contains("\"keyBindings\""));
        let snippets = read(&dir.0, "snippets-cpp.json").unwrap();
        assert_eq!(snippets.content, "{\n}\n");
        assert!(dir.0.join("snippets-cpp.json").is_file());
        // 書いて、読み直す。外で変わっていたら書かない
        let stamp = snippets.stamp;
        let WriteResult::Saved { stamp: saved } =
            write(&dir.0, "snippets-cpp.json", "{ \"a\": 1 }\n", Some(&stamp)).unwrap()
        else {
            panic!("書けるはず");
        };
        assert_eq!(
            read(&dir.0, "snippets-cpp.json").unwrap().content,
            "{ \"a\": 1 }\n"
        );
        assert_eq!(
            stamps(&dir.0, &["snippets-cpp.json".to_string()]),
            vec![Some(saved.clone())]
        );
        assert_eq!(
            write(&dir.0, "snippets-cpp.json", "x", Some(&stamp)).unwrap(),
            WriteResult::Conflict
        );
        // 書くときは作らない（消えていれば書けない）
        fs::remove_file(dir.0.join("snippets-cpp.json")).unwrap();
        assert!(write(&dir.0, "snippets-cpp.json", "x", None).is_err());
    }

    #[test]
    fn writes_through_symbolic_links() {
        let dir = TempDir::new("link");
        let real = dir.0.join("dotfiles-settings.json");
        fs::write(&real, "{}\n").unwrap();
        std::os::unix::fs::symlink(&real, dir.0.join("settings.json")).unwrap();
        let read_back = read(&dir.0, "settings.json").unwrap();
        assert_eq!(read_back.content, "{}\n");
        write(&dir.0, "settings.json", "{ }\n", Some(&read_back.stamp)).unwrap();
        assert_eq!(fs::read_to_string(&real).unwrap(), "{ }\n");
        assert!(
            fs::symlink_metadata(dir.0.join("settings.json"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }
}
