// 自力 AC・解説 AC の記録（Phase 33。D-257）と、解き直しの印（Phase 34。D-258）
//
// 問題 ID ごとに「自力」「解説」と「解き直す」を手で付け、アプリのフォルダの problem-marks.json に保存する。
// 形：{ "version": 1, "problems": { "abc001_1": { "solved": "editorial", "solvedOn": "2026-11-13", "retry": "2026-11-12" } } }。
// retry は印を付けた日、solvedOn は「自力」「解説」を付けた日（Phase 37。D-261。前の版の記録にはない）。
// 書き方・権限・読めないときの退避は、ほかの作業データと同じ（D-034〜D-041。storage.rs の load_json・write_atomic）。
// AtCoder・AtCoder Problems には何も送らない

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::problem;
use crate::storage::{self, Loaded, VERSION};

pub const MARKS_FILE: &str = "problem-marks.json";

// 読んでから書くまでを 1 つずつ行う（2 つの変更が重なって、片方が消えないように）
static LOCK: Mutex<()> = Mutex::new(());

// どう解いたか
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolvedBy {
    #[serde(rename = "self")]
    Myself,
    #[serde(rename = "editorial")]
    Editorial,
}

// 1 問の記録。何もなければ保存しない
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solved: Option<SolvedBy>,
    // 「自力」「解説」を付けた日（手元の日付。D-261）。付けたとき・切り替えたときに変わり、外すと消える。
    // Phase 37 より前に付けた記録にはない
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub solved_on: Option<String>,
    // 解き直しの印を付けた日（手元の日付。例 "2026-11-12"。D-258）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<String>,
}

impl Mark {
    fn is_empty(&self) -> bool {
        self.solved.is_none() && self.retry.is_none()
    }
}

// 日付の形（YYYY-MM-DD）
fn valid_date(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(i, b)| {
            if i == 4 || i == 7 {
                *b == b'-'
            } else {
                b.is_ascii_digit()
            }
        })
}

// 手元の日時（年・月・日・時・分・秒）。読めなければ None
pub fn local_time(secs: i64) -> Option<(i32, u32, u32, u32, u32, u32)> {
    let secs = secs as libc::time_t;
    // SAFETY: localtime_r は、渡した tm に書くだけ（スレッドごとの領域を使わない）
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&secs, &mut tm) }.is_null() {
        return None;
    }
    Some((
        tm.tm_year + 1900,
        (tm.tm_mon + 1) as u32,
        tm.tm_mday as u32,
        tm.tm_hour as u32,
        tm.tm_min as u32,
        tm.tm_sec as u32,
    ))
}

// 今日の日付（手元の時刻。例 "2026-11-12"）
pub fn today() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    local_time(now)
        .map(|(y, mo, d, ..)| format!("{y:04}-{mo:02}-{d:02}"))
        .unwrap_or_else(|| "1970-01-01".to_string())
}

pub type Marks = BTreeMap<String, Mark>;

// 形が正しくない問題 ID・日付と、中身のない記録は使わない（次に書くときに消える）
fn parse(value: Value) -> Option<Marks> {
    let problems: Marks = serde_json::from_value(value.get("problems")?.clone()).ok()?;
    Some(
        problems
            .into_iter()
            .map(|(id, mut mark)| {
                mark.retry = mark.retry.filter(|date| valid_date(date));
                mark.solved_on = mark
                    .solved_on
                    .filter(|date| mark.solved.is_some() && valid_date(date));
                (id, mark)
            })
            .filter(|(id, mark)| problem::valid_id(id) && !mark.is_empty())
            .collect(),
    )
}

// 記録を読む。なければ空。読めなければ退避して空から始め、退避先も返す（呼び出し側が知らせる）
fn read(dir: &Path) -> Result<(Marks, Option<PathBuf>), String> {
    match storage::load_json(&dir.join(MARKS_FILE), parse)? {
        Loaded::Ok(marks) => Ok((marks, None)),
        Loaded::Missing => Ok((Marks::new(), None)),
        Loaded::Broken(backup) => Ok((Marks::new(), Some(backup))),
    }
}

pub fn load(dir: &Path) -> Result<(Marks, Option<PathBuf>), String> {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    read(dir)
}

// 1 問の「どう解いたか」を変える（None で記録なし）。変えたあとのすべての記録と、読むときに退避した先を返す。
// 付けたとき・切り替えたときは、付けた日を date にする（同じものをもう一度付けても変えない。D-261）。外すと付けた日も消す
pub fn set_solved(
    dir: &Path,
    id: &str,
    solved: Option<SolvedBy>,
    date: &str,
) -> Result<(Marks, Option<PathBuf>), String> {
    if !valid_date(date) {
        return Err("日付の形が正しくありません".to_string());
    }
    update(dir, id, |mark| {
        if mark.solved != solved {
            mark.solved_on = solved.map(|_| date.to_string());
        }
        mark.solved = solved;
    })
}

// 1 問の解き直しの印を変える（Some(日付) で付ける、None で外す。D-258）。すでに付いていれば、付けた日は変えない
pub fn set_retry(
    dir: &Path,
    id: &str,
    since: Option<String>,
) -> Result<(Marks, Option<PathBuf>), String> {
    if since.as_deref().is_some_and(|date| !valid_date(date)) {
        return Err("日付の形が正しくありません".to_string());
    }
    update(dir, id, |mark| {
        mark.retry = match since {
            Some(date) => Some(mark.retry.take().unwrap_or(date)),
            None => None,
        }
    })
}

fn update(
    dir: &Path,
    id: &str,
    change: impl FnOnce(&mut Mark),
) -> Result<(Marks, Option<PathBuf>), String> {
    if !problem::valid_id(id) {
        return Err("問題の ID が正しくありません".to_string());
    }
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut marks, backup) = read(dir)?;
    let mut mark = marks.remove(id).unwrap_or_default();
    change(&mut mark);
    if !mark.is_empty() {
        marks.insert(id.to_string(), mark);
    }
    let value = serde_json::json!({ "version": VERSION, "problems": marks });
    let bytes =
        serde_json::to_vec_pretty(&value).map_err(|_| "記録を保存できません".to_string())?;
    storage::write_atomic(&dir.join(MARKS_FILE), &bytes)?;
    Ok((marks, backup))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "atcoder-studio-marks-{name}-{}",
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

    // テストの日付
    const D: &str = "2026-11-13";

    #[test]
    fn saves_changes_and_clears_marks() {
        let dir = TempDir::new("save");
        assert_eq!(load(&dir.0).unwrap(), (Marks::new(), None));

        set_solved(&dir.0, "abc001_1", Some(SolvedBy::Editorial), D).unwrap();
        let (marks, backup) = set_solved(&dir.0, "arc058_a", Some(SolvedBy::Myself), D).unwrap();
        assert_eq!(backup, None);
        assert_eq!(marks.len(), 2);
        // ファイルの形（D-257）
        let text = std::fs::read_to_string(dir.0.join(MARKS_FILE)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["version"], 1);
        assert_eq!(value["problems"]["abc001_1"]["solved"], "editorial");
        assert_eq!(value["problems"]["arc058_a"]["solved"], "self");

        // 替える・外す。外した問題はファイルから消える
        set_solved(&dir.0, "abc001_1", Some(SolvedBy::Myself), D).unwrap();
        let (marks, _) = set_solved(&dir.0, "arc058_a", None, D).unwrap();
        assert_eq!(
            marks.into_iter().collect::<Vec<_>>(),
            vec![(
                "abc001_1".to_string(),
                Mark {
                    solved: Some(SolvedBy::Myself),
                    solved_on: Some(D.to_string()),
                    retry: None
                }
            )]
        );
        let text = std::fs::read_to_string(dir.0.join(MARKS_FILE)).unwrap();
        assert!(!text.contains("arc058_a"));
        assert_eq!(load(&dir.0).unwrap().0.len(), 1);
    }

    #[test]
    fn rejects_wrong_problem_ids() {
        let dir = TempDir::new("ids");
        for id in ["", "ABC001_1", "../x", "a b", &"a".repeat(65)] {
            assert!(
                set_solved(&dir.0, id, Some(SolvedBy::Myself), D).is_err(),
                "{id}"
            );
        }
        assert!(!dir.0.join(MARKS_FILE).exists());
        // ファイルの中の、形が正しくない ID と中身のない記録は使わない
        std::fs::write(
            dir.0.join(MARKS_FILE),
            r#"{"version":1,"problems":{"abc001_1":{"solved":"self"},"../x":{"solved":"self"},"abc002_1":{}}}"#,
        )
        .unwrap();
        let (marks, backup) = load(&dir.0).unwrap();
        assert_eq!(backup, None);
        assert_eq!(marks.keys().collect::<Vec<_>>(), ["abc001_1"]);
    }

    #[test]
    fn moves_broken_files_aside() {
        let dir = TempDir::new("broken");
        for broken in [
            "{",
            r#"{"version":2,"problems":{}}"#,
            r#"{"version":1,"problems":{"abc001_1":{"solved":"maybe"}}}"#,
        ] {
            std::fs::write(dir.0.join(MARKS_FILE), broken).unwrap();
            let (marks, backup) = load(&dir.0).unwrap();
            assert!(marks.is_empty());
            let backup = backup.expect(broken);
            assert_eq!(std::fs::read_to_string(&backup).unwrap(), broken);
            assert!(!dir.0.join(MARKS_FILE).exists());
        }
        // 退避したあとに付けた記録は保存される
        std::fs::write(dir.0.join(MARKS_FILE), "{").unwrap();
        let (marks, backup) = set_solved(&dir.0, "abc001_1", Some(SolvedBy::Editorial), D).unwrap();
        assert!(backup.is_some());
        assert_eq!(marks.len(), 1);
        assert_eq!(load(&dir.0).unwrap().0.len(), 1);
    }

    #[test]
    fn keeps_retry_marks_with_their_dates() {
        let dir = TempDir::new("retry");
        set_retry(&dir.0, "abc001_1", Some("2026-11-10".to_string())).unwrap();
        // もう一度付けても、付けた日は変わらない
        let (marks, _) = set_retry(&dir.0, "abc001_1", Some("2026-11-12".to_string())).unwrap();
        assert_eq!(marks["abc001_1"].retry.as_deref(), Some("2026-11-10"));
        // 自力・解説とは別に持つ
        set_solved(&dir.0, "abc001_1", Some(SolvedBy::Editorial), D).unwrap();
        let (marks, _) = set_solved(&dir.0, "abc001_1", None, D).unwrap();
        assert_eq!(marks["abc001_1"].retry.as_deref(), Some("2026-11-10"));
        let text = std::fs::read_to_string(dir.0.join(MARKS_FILE)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["problems"]["abc001_1"]["retry"], "2026-11-10");
        assert!(value["problems"]["abc001_1"].get("solved").is_none());
        // 外すと、ほかに記録がなければファイルから消える
        let (marks, _) = set_retry(&dir.0, "abc001_1", None).unwrap();
        assert!(marks.is_empty());
        // 日付の形を確かめる（書くときも、読むときも）
        assert!(set_retry(&dir.0, "abc001_1", Some("2026/11/12".to_string())).is_err());
        std::fs::write(
            dir.0.join(MARKS_FILE),
            r#"{"version":1,"problems":{"abc001_1":{"retry":"yesterday"},"abc002_1":{"solved":"self","retry":"x"}}}"#,
        )
        .unwrap();
        let (marks, _) = load(&dir.0).unwrap();
        assert_eq!(marks.keys().collect::<Vec<_>>(), ["abc002_1"]);
        assert_eq!(marks["abc002_1"].retry, None);
        assert!(valid_date(&today()));
    }

    #[test]
    fn records_when_solved_marks_are_set() {
        let dir = TempDir::new("solved-on");
        set_solved(&dir.0, "abc001_1", Some(SolvedBy::Editorial), "2026-11-10").unwrap();
        // 同じものをもう一度付けても、付けた日は変わらない
        let (marks, _) =
            set_solved(&dir.0, "abc001_1", Some(SolvedBy::Editorial), "2026-11-12").unwrap();
        assert_eq!(marks["abc001_1"].solved_on.as_deref(), Some("2026-11-10"));
        let text = std::fs::read_to_string(dir.0.join(MARKS_FILE)).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["problems"]["abc001_1"]["solvedOn"], "2026-11-10");
        // 切り替えると付け直す
        let (marks, _) =
            set_solved(&dir.0, "abc001_1", Some(SolvedBy::Myself), "2026-11-13").unwrap();
        assert_eq!(marks["abc001_1"].solved_on.as_deref(), Some("2026-11-13"));
        // 外すと付けた日も消える（解き直しの印は残る）
        set_retry(&dir.0, "abc001_1", Some("2026-11-11".to_string())).unwrap();
        let (marks, _) = set_solved(&dir.0, "abc001_1", None, "2026-11-14").unwrap();
        assert_eq!(marks["abc001_1"].solved_on, None);
        assert_eq!(marks["abc001_1"].retry.as_deref(), Some("2026-11-11"));
        assert!(set_solved(&dir.0, "abc001_1", Some(SolvedBy::Myself), "11/14").is_err());
        // 前の版の記録（solvedOn がない）も読める。形の正しくない日付と、solved のない solvedOn は使わない
        std::fs::write(
            dir.0.join(MARKS_FILE),
            r#"{"version":1,"problems":{"abc001_1":{"solved":"editorial"},"abc002_1":{"solved":"self","solvedOn":"x"},"abc003_1":{"solvedOn":"2026-11-13","retry":"2026-11-13"}}}"#,
        )
        .unwrap();
        let (marks, backup) = load(&dir.0).unwrap();
        assert_eq!(backup, None);
        assert_eq!(marks["abc001_1"].solved, Some(SolvedBy::Editorial));
        assert!(marks.values().all(|mark| mark.solved_on.is_none()));
    }
}
