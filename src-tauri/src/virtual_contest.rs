// バーチャル参加（Phase 44。D-268）
//
// 過去のコンテストを本番と同じ長さで解く。始めた時刻と終わる時刻、終えた結果を、アプリのフォルダの virtual.json に残す。
// 形：{ "version": 1, "current": { "contest": "abc437", "start": 1763600000, "end": 1763606000 } | null, "results": [...] }。
// AC の時刻は「自分の提出」のページの行（提出日時・得点・判定）から、バーチャルの時間の中のものだけを拾う（score）。
// 順位・予想 perf は standings.rs の virtual_place（当時の順位表に当てはめる）。読むのは submit.rs（virtual_result）。
// 書き方・権限・読めないときの退避は、ほかの作業データと同じ（D-034〜D-041。storage.rs の load_json・write_atomic）
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::problem::{self, TaskEntry};
use crate::storage::{self, Loaded, VERSION};
use crate::submit::MineRow;

pub const VIRTUAL_FILE: &str = "virtual.json";
// 残す結果の数（新しいものから）
pub const MAX_RESULTS: usize = 200;
// ペナルティ 1 つあたりの時間（秒）。順位表から求められなかったときに使う（ABC・ARC・AGC の決まり【推測】）
pub const DEFAULT_PENALTY_SECS: i64 = 300;
// 長さの上限（秒。長いコンテストの入れ間違いを防ぐ。AtCoder のアルゴリズムのコンテストは長くても数時間）
pub const MAX_DURATION_SECS: i64 = 24 * 3600;

// 読んでから書くまでを 1 つずつ行う
static LOCK: Mutex<()> = Mutex::new(());

// 今のバーチャル参加。start・end は UNIX 時間の秒（end は start ＋ 本番の長さ）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub contest: String,
    pub start: i64,
    pub end: i64,
}

// 1 問の結果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskScore {
    pub task: String,
    // 例：「A」
    #[serde(default)]
    pub label: String,
    // 得点の 100 倍（順位表と同じ）
    pub score: u64,
    // 得点を取った時刻（開始からの秒）。取っていなければ None
    pub ac_seconds: Option<i64>,
    // 誤答の数（得点を取っていれば、その前の誤答。取っていなければ、すべての誤答）
    pub penalty: u32,
    // 判定中の提出がある
    #[serde(default)]
    pub pending: bool,
}

// バーチャルの時間の中の結果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Score {
    pub tasks: Vec<TaskScore>,
    // 得点の 100 倍
    pub score: u64,
    // 時間（秒）：最後に得点を取った時刻 ＋ ペナルティの時間（得点を取った問題の誤答だけ）
    pub elapsed: i64,
    // 得点を取った問題の誤答の数の合計
    pub penalty: u32,
    // ペナルティ 1 つあたりの時間（秒）
    pub penalty_seconds: i64,
}

// 終えたバーチャル参加
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Record {
    pub contest: String,
    pub start: i64,
    // 時間が終わる時刻（本番の長さ）と、終えた時刻（「やめる」なら end より前）
    pub end: i64,
    pub finished: i64,
    pub result: Score,
    // 当時の順位表に当てはめた順位と人数。読めなければ None
    pub rank: Option<i64>,
    pub participants: Option<usize>,
    // 予想 perf（近似）。出さないときは note に理由
    pub performance: Option<i64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VirtualFile {
    pub current: Option<Current>,
    // 新しいものから
    pub results: Vec<Record>,
}

fn valid_current(current: &Current) -> bool {
    problem::ProblemId::new(&current.contest, "x").is_ok()
        && current.start < current.end
        && current.end - current.start <= MAX_DURATION_SECS
}

fn parse(value: Value) -> Option<VirtualFile> {
    let current: Option<Current> = match value.get("current") {
        None | Some(Value::Null) => None,
        Some(current) => Some(serde_json::from_value(current.clone()).ok()?),
    };
    let results: Vec<Record> = match value.get("results") {
        None => Vec::new(),
        Some(results) => serde_json::from_value(results.clone()).ok()?,
    };
    Some(VirtualFile {
        // 形が正しくない今のバーチャル参加・結果は使わない（次に書くときに消える）
        current: current.filter(valid_current),
        results: results
            .into_iter()
            .filter(|record| problem::ProblemId::new(&record.contest, "x").is_ok())
            .take(MAX_RESULTS)
            .collect(),
    })
}

fn read(dir: &Path) -> Result<(VirtualFile, Option<PathBuf>), String> {
    match storage::load_json(&dir.join(VIRTUAL_FILE), parse)? {
        Loaded::Ok(file) => Ok((file, None)),
        Loaded::Missing => Ok((VirtualFile::default(), None)),
        Loaded::Broken(backup) => Ok((VirtualFile::default(), Some(backup))),
    }
}

fn write(dir: &Path, file: &VirtualFile) -> Result<(), String> {
    let value = serde_json::json!({
        "version": VERSION,
        "current": file.current,
        "results": file.results,
    });
    let bytes = serde_json::to_vec_pretty(&value)
        .map_err(|_| "バーチャル参加を保存できません".to_string())?;
    storage::write_atomic(&dir.join(VIRTUAL_FILE), &bytes)
}

// 今のバーチャル参加と、終えた結果。読めなければ退避して空から始め、退避先も返す（呼び出し側が知らせる）
pub fn load(dir: &Path) -> Result<(VirtualFile, Option<PathBuf>), String> {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    read(dir)
}

// 始める（1 つずつ）
pub fn start(dir: &Path, current: Current) -> Result<(VirtualFile, Option<PathBuf>), String> {
    if !valid_current(&current) {
        return Err("バーチャル参加の時間が正しくありません".to_string());
    }
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut file, backup) = read(dir)?;
    if let Some(running) = &file.current {
        return Err(format!(
            "{} でバーチャル参加しています。バーチャル参加は 1 つずつです",
            running.contest
        ));
    }
    file.current = Some(current);
    write(dir, &file)?;
    Ok((file, backup))
}

// 結果を残して終える（今のバーチャル参加が、その結果のものなら消す）
pub fn finish(dir: &Path, record: Record) -> Result<(VirtualFile, Option<PathBuf>), String> {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut file, backup) = read(dir)?;
    if file
        .current
        .as_ref()
        .is_some_and(|current| current.contest == record.contest && current.start == record.start)
    {
        file.current = None;
    }
    file.results.insert(0, record);
    file.results.truncate(MAX_RESULTS);
    write(dir, &file)?;
    Ok((file, backup))
}

// 結果を残さずにやめる
pub fn cancel(dir: &Path) -> Result<(VirtualFile, Option<PathBuf>), String> {
    let _guard = LOCK.lock().unwrap_or_else(|p| p.into_inner());
    let (mut file, backup) = read(dir)?;
    if file.current.take().is_some() {
        write(dir, &file)?;
    }
    Ok((file, backup))
}

// ---- 得点と時間 ----

// 判定がまだ決まっていない（WJ・WR・「3/15 WA」のような途中の表記・読めなかった判定）
pub fn is_pending(verdict: &str) -> bool {
    matches!(verdict, "WJ" | "WR" | "?" | "") || verdict.contains('/')
}

// ペナルティにならない判定（AC と、コンパイルエラー【推測：AtCoder の決まり】）
fn no_penalty(verdict: &str) -> bool {
    matches!(verdict, "AC" | "CE")
}

// 「自分の提出」の行から、バーチャルの時間（current.start 〜 until。until は current.end か、やめた時刻）の中の
// 提出だけを使って、問題ごとの得点・時刻・誤答の数と、合計を出す。問題は一覧の順（一覧にない問題の提出は使わない）
pub fn score(
    tasks: &[TaskEntry],
    rows: &[MineRow],
    current: &Current,
    until: i64,
    penalty_seconds: i64,
) -> Score {
    let until = until.min(current.end);
    let mut inside: Vec<(&MineRow, i64)> = rows
        .iter()
        .filter_map(|row| {
            let time = row.time?;
            (current.start <= time && time < until).then_some((row, time))
        })
        .collect();
    inside.sort_by_key(|(row, time)| (*time, row.id));
    let mut result = Score {
        tasks: Vec::new(),
        score: 0,
        elapsed: 0,
        penalty: 0,
        penalty_seconds,
    };
    let mut last = None;
    for entry in tasks {
        let submissions: Vec<&(&MineRow, i64)> = inside
            .iter()
            .filter(|(row, _)| row.task == entry.task)
            .collect();
        let pending = submissions.iter().any(|(row, _)| is_pending(&row.verdict));
        let judged: Vec<&(&MineRow, i64)> = submissions
            .into_iter()
            .filter(|(row, _)| !is_pending(&row.verdict))
            .collect();
        let best = judged
            .iter()
            .map(|(row, _)| row.score.unwrap_or(0))
            .max()
            .unwrap_or(0);
        let wrong = |list: &[&(&MineRow, i64)]| {
            list.iter()
                .filter(|(row, _)| !no_penalty(&row.verdict))
                .count() as u32
        };
        let task = if best > 0 {
            let index = judged
                .iter()
                .position(|(row, _)| row.score.unwrap_or(0) == best)
                .expect("best is in judged");
            let ac = judged[index].1 - current.start;
            let penalty = wrong(&judged[..index]);
            result.score += best;
            result.penalty += penalty;
            last = last.max(Some(ac));
            TaskScore {
                task: entry.task.clone(),
                label: entry.label.clone(),
                score: best,
                ac_seconds: Some(ac),
                penalty,
                pending,
            }
        } else {
            TaskScore {
                task: entry.task.clone(),
                label: entry.label.clone(),
                score: 0,
                ac_seconds: None,
                penalty: wrong(&judged),
                pending,
            }
        };
        result.tasks.push(task);
    }
    if let Some(last) = last {
        result.elapsed = last + i64::from(result.penalty) * penalty_seconds;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "atcoder-studio-virtual-{name}-{}",
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

    fn row(id: u64, task: &str, verdict: &str, time: i64, score: u64) -> MineRow {
        MineRow {
            id,
            task: task.to_string(),
            verdict: verdict.to_string(),
            time: Some(time),
            score: Some(score),
        }
    }

    fn entries() -> Vec<TaskEntry> {
        ["a", "b", "c", "d"]
            .iter()
            .map(|label| TaskEntry {
                task: format!("x_{label}"),
                label: label.to_uppercase(),
                title: String::new(),
            })
            .collect()
    }

    const CURRENT: Current = Current {
        contest: String::new(),
        start: 1000,
        end: 7000,
    };

    fn current() -> Current {
        Current {
            contest: "x".to_string(),
            ..CURRENT
        }
    }

    fn record(contest: &str, start: i64) -> Record {
        Record {
            contest: contest.to_string(),
            start,
            end: start + 6000,
            finished: start + 6000,
            result: score(&[], &[], &current(), 0, DEFAULT_PENALTY_SECS),
            rank: Some(10),
            participants: Some(100),
            performance: Some(1234),
            note: None,
        }
    }

    #[test]
    fn scores_only_the_submissions_in_the_virtual_time() {
        let rows = [
            // 始める前の練習の AC（数えない）
            row(1, "x_a", "AC", 900, 10000),
            // A：WA・CE のあとに AC（CE は誤答にしない）
            row(2, "x_a", "WA", 1100, 0),
            row(3, "x_a", "CE", 1150, 0),
            row(4, "x_a", "AC", 1300, 10000),
            // A の AC のあとの WA は数えない
            row(5, "x_a", "WA", 1400, 0),
            // B：部分点のあとに満点（満点を取った提出まで。部分点の提出は AC でないので誤答）
            row(6, "x_b", "WA", 2000, 5000),
            row(7, "x_b", "AC", 2600, 20000),
            // C：誤答だけ（合計のペナルティには入れない）
            row(8, "x_c", "TLE", 3000, 0),
            row(9, "x_c", "RE", 3100, 0),
            // D：判定中と、時間が終わったあとの AC（数えない）
            row(10, "x_d", "3/15 WA", 6900, 0),
            row(11, "x_d", "AC", 7000, 40000),
        ];
        let result = score(&entries(), &rows, &current(), 7000, 300);
        let brief: Vec<_> = result
            .tasks
            .iter()
            .map(|t| {
                (
                    t.label.as_str(),
                    t.score,
                    t.ac_seconds,
                    t.penalty,
                    t.pending,
                )
            })
            .collect();
        assert_eq!(
            brief,
            [
                ("A", 10000, Some(300), 1, false),
                ("B", 20000, Some(1600), 1, false),
                ("C", 0, None, 2, false),
                ("D", 0, None, 0, true),
            ]
        );
        assert_eq!(result.score, 30000);
        assert_eq!(result.penalty, 2);
        // 最後に得点を取った 1600 秒 ＋ 誤答 2 つ × 300 秒
        assert_eq!(result.elapsed, 2200);
        // 途中でやめたら、やめた時刻より後の提出は数えない（B は部分点の 50 点を 1000 秒に取ったところまで）
        let stopped = score(&entries(), &rows, &current(), 2500, 300);
        assert_eq!(stopped.score, 15000);
        assert_eq!(stopped.elapsed, 1000 + 300);
        // 何も解いていなければ時間は 0
        assert_eq!(score(&entries(), &[], &current(), 7000, 300).elapsed, 0);
    }

    #[test]
    fn keeps_one_virtual_contest_and_the_results() {
        let dir = TempDir::new("save");
        assert_eq!(load(&dir.0).unwrap(), (VirtualFile::default(), None));
        let first = Current {
            contest: "abc437".to_string(),
            start: 1000,
            end: 7000,
        };
        start(&dir.0, first.clone()).unwrap();
        // 1 つずつ
        assert!(
            start(
                &dir.0,
                Current {
                    contest: "abc438".to_string(),
                    ..first.clone()
                }
            )
            .unwrap_err()
            .contains("abc437")
        );
        // 開き直しても続く
        assert_eq!(load(&dir.0).unwrap().0.current, Some(first.clone()));
        // 結果を残して終える
        let (file, _) = finish(&dir.0, record("abc437", 1000)).unwrap();
        assert_eq!(file.current, None);
        assert_eq!(file.results.len(), 1);
        // 新しいものが前
        start(
            &dir.0,
            Current {
                contest: "abc438".to_string(),
                ..first.clone()
            },
        )
        .unwrap();
        finish(&dir.0, record("abc438", 1000)).unwrap();
        let (file, _) = load(&dir.0).unwrap();
        assert_eq!(
            file.results
                .iter()
                .map(|r| r.contest.as_str())
                .collect::<Vec<_>>(),
            ["abc438", "abc437"]
        );
        // 結果を残さずにやめる
        start(&dir.0, first.clone()).unwrap();
        let (file, _) = cancel(&dir.0).unwrap();
        assert_eq!(file.current, None);
        assert_eq!(file.results.len(), 2);
        // 長すぎる・逆の時間は始めない
        assert!(
            start(
                &dir.0,
                Current {
                    end: 1000 + MAX_DURATION_SECS + 1,
                    ..first.clone()
                }
            )
            .is_err()
        );
        assert!(
            start(
                &dir.0,
                Current {
                    end: 1000,
                    ..first.clone()
                }
            )
            .is_err()
        );
    }

    #[test]
    fn moves_broken_files_aside() {
        let dir = TempDir::new("broken");
        for broken in [
            "{",
            r#"{"version":2,"current":null,"results":[]}"#,
            r#"{"version":1,"current":{"contest":"abc437"},"results":[]}"#,
        ] {
            std::fs::write(dir.0.join(VIRTUAL_FILE), broken).unwrap();
            let (file, backup) = load(&dir.0).unwrap();
            assert_eq!(file, VirtualFile::default());
            assert_eq!(
                std::fs::read_to_string(backup.expect(broken)).unwrap(),
                broken
            );
        }
        // 形が正しくない ID の今のバーチャル参加は使わない
        std::fs::write(
            dir.0.join(VIRTUAL_FILE),
            r#"{"version":1,"current":{"contest":"../x","start":1,"end":2},"results":[]}"#,
        )
        .unwrap();
        assert_eq!(load(&dir.0).unwrap(), (VirtualFile::default(), None));
    }
}
