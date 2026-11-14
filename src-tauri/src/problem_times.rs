// 問題ごとの経過時間（Phase 42。D-266）
//
// 問題文の欄にその問題を出していた時間の合計（秒）と、AC が分かったときの時間（秒）を、アプリのフォルダの
// problem-times.json に保存する。数えるのは画面側（ProblemClock.tsx）で、ここは読み書きだけ。
// 形：{ "version": 1, "problems": { "abc001_1": { "seconds": 754, "acSeconds": 700 } } }。
// 書き方・権限・読めないときの退避は、ほかの作業データと同じ（D-034〜D-041。storage.rs の load_json・write_atomic）
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::problem;
use crate::storage::{self, Loaded, VERSION};

pub const TIMES_FILE: &str = "problem-times.json";

// 1 問の時間の上限（秒。約 3 年。これを超える値は使わない）
pub const MAX_SECONDS: u64 = 100_000_000;

// 読んでから書くまでを 1 つずつ行う
static LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProblemTime {
    // 問題文の欄に出していた時間の合計（秒）
    pub seconds: u64,
    // AC が分かったときの時間（秒）。あれば、それより先は数えない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ac_seconds: Option<u64>,
    // 数え始めた時刻（UNIX 時間の秒。「⟲」で今にする）。これより後の AC だけで止める。Phase 42 の記録にはない
    // （docs/bug-review-2026-11-14.md の B-1）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
}

pub type Times = BTreeMap<String, ProblemTime>;

fn valid(time: &ProblemTime) -> bool {
    time.seconds <= MAX_SECONDS && time.ac_seconds.is_none_or(|ac| ac <= time.seconds)
}

// 形が正しくない問題 ID・値と、0 秒の記録は使わない（次に書くときに消える）
fn parse(value: Value) -> Option<Times> {
    let problems: Times = serde_json::from_value(value.get("problems")?.clone()).ok()?;
    Some(
        problems
            .into_iter()
            .filter(|(id, time)| {
                problem::valid_id(id)
                    && valid(time)
                    && (time.seconds > 0 || time.ac_seconds.is_some() || time.started_at.is_some())
            })
            .collect(),
    )
}

fn read(dir: &Path) -> Result<(Times, Option<PathBuf>), String> {
    match storage::load_json(&dir.join(TIMES_FILE), parse)? {
        Loaded::Ok(times) => Ok((times, None)),
        Loaded::Missing => Ok((Times::new(), None)),
        Loaded::Broken(backup) => Ok((Times::new(), Some(backup))),
    }
}

// すべての時間を返す。読めなければ退避して空から始め、退避先も返す（呼び出し側が知らせる）
pub fn load(dir: &Path) -> Result<(Times, Option<PathBuf>), String> {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    read(dir)
}

// 1 問の時間を書く。0 秒で AC も数え始めた時刻もなければ消す
pub fn set(dir: &Path, id: &str, time: ProblemTime) -> Result<(Times, Option<PathBuf>), String> {
    if !problem::valid_id(id) {
        return Err("問題の ID が正しくありません".to_string());
    }
    if !valid(&time) {
        return Err("経過時間の値が正しくありません".to_string());
    }
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut times, backup) = read(dir)?;
    if time.seconds == 0 && time.ac_seconds.is_none() && time.started_at.is_none() {
        times.remove(id);
    } else {
        times.insert(id.to_string(), time);
    }
    let value = serde_json::json!({ "version": VERSION, "problems": times });
    let bytes =
        serde_json::to_vec_pretty(&value).map_err(|_| "経過時間を保存できません".to_string())?;
    storage::write_atomic(&dir.join(TIMES_FILE), &bytes)?;
    Ok((times, backup))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "atcoder-studio-times-{name}-{}",
                std::process::id()
            ));
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

    fn time(seconds: u64, ac_seconds: Option<u64>) -> ProblemTime {
        ProblemTime {
            seconds,
            ac_seconds,
            started_at: None,
        }
    }

    #[test]
    fn saves_times_and_clears_reset_ones() {
        let dir = TempDir::new("save");
        assert_eq!(load(&dir.0).unwrap(), (Times::new(), None));
        set(&dir.0, "abc001_1", time(754, None)).unwrap();
        let (times, backup) = set(&dir.0, "abc002_1", time(300, Some(290))).unwrap();
        assert_eq!(backup, None);
        assert_eq!(times.len(), 2);
        // ファイルの形
        let text = std::fs::read_to_string(dir.0.join(TIMES_FILE)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["problems"]["abc001_1"]["seconds"], 754);
        assert!(value["problems"]["abc001_1"].get("acSeconds").is_none());
        assert_eq!(value["problems"]["abc002_1"]["acSeconds"], 290);
        // 開き直しても同じ
        assert_eq!(load(&dir.0).unwrap().0, times);
        // 0 秒で AC もなければ消す（数え直し）
        let (times, _) = set(&dir.0, "abc001_1", time(0, None)).unwrap();
        assert_eq!(times.keys().collect::<Vec<_>>(), ["abc002_1"]);
        // 「⟲」：0 秒でも、数え始めた時刻があれば残す（前に AC した問題を数え直す。B-1）
        let restarted = ProblemTime {
            started_at: Some(1_800_000_000),
            ..time(0, None)
        };
        let (times, _) = set(&dir.0, "abc001_1", restarted).unwrap();
        assert_eq!(times.get("abc001_1"), Some(&restarted));
        assert_eq!(load(&dir.0).unwrap().0.get("abc001_1"), Some(&restarted));
    }

    #[test]
    fn rejects_wrong_ids_and_values() {
        let dir = TempDir::new("wrong");
        for id in ["", "ABC001_1", "../x"] {
            assert!(set(&dir.0, id, time(1, None)).is_err(), "{id}");
        }
        // AC の時間が合計より大きい・上限を超える
        assert!(set(&dir.0, "abc001_1", time(10, Some(11))).is_err());
        assert!(set(&dir.0, "abc001_1", time(MAX_SECONDS + 1, None)).is_err());
        assert!(!dir.0.join(TIMES_FILE).exists());
        // ファイルの中の、形が正しくない ID・値と、0 秒の記録は使わない
        std::fs::write(
            dir.0.join(TIMES_FILE),
            r#"{"version":1,"problems":{"abc001_1":{"seconds":5},"../x":{"seconds":5},"abc002_1":{"seconds":5,"acSeconds":9},"abc003_1":{"seconds":0}}}"#,
        )
        .unwrap();
        let (times, backup) = load(&dir.0).unwrap();
        assert_eq!(backup, None);
        assert_eq!(times.keys().collect::<Vec<_>>(), ["abc001_1"]);
    }

    #[test]
    fn moves_broken_files_aside() {
        let dir = TempDir::new("broken");
        for broken in [
            "{",
            r#"{"version":2,"problems":{}}"#,
            r#"{"version":1,"problems":{"abc001_1":{"seconds":-1}}}"#,
        ] {
            std::fs::write(dir.0.join(TIMES_FILE), broken).unwrap();
            let (times, backup) = load(&dir.0).unwrap();
            assert!(times.is_empty());
            assert_eq!(
                std::fs::read_to_string(backup.expect(broken)).unwrap(),
                broken
            );
        }
    }
}
