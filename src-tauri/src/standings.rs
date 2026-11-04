// 順位・予想パフォーマンス・予想 diff（Phase 21 作業 1。D-230〜D-233。docs/phase21/01-pre-start.md）
//
// - 順位表の JSON（`/contests/<コンテスト>/standings/json`）をセッション付きで読み、メモリの中で計算に使う。
//   ディスクには書かない（覚えるのは計算した結果だけ。D-233）
// - 予想パフォーマンス：Rated の参加者の APerf から、Σ 1/(1+6^((x−APerf_i)/400)) = 順位 − 0.5 を満たす x（D-231）。
//   APerf は、レートと参加回数から内部のレートを逆算した近似（AtCoder Problems の inverse_adjust_rating と同じ式）
// - 予想 diff：AtCoder Problems（estimator/main.py。MIT）と同じく、解けた人数の期待値が実際の人数と等しくなる値（D-232）
// - Rated の範囲は、コンテストのトップページ（ログインなしで読める）から読み、アプリを開いている間だけ覚える
// - 追跡するユーザーの AC 状況（Phase 31。D-253）：同じ順位表から、自分と追跡するユーザーの行だけを取り出す
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use dom_query::Document;
use serde::{Deserialize, Serialize};

// 初参加の内部レートが 1200 になった時期（AtCoder のお知らせ https://atcoder.jp/posts/1591 。ABC430 から。
// ac-predictor と同じく、2025-11-01 00:00 UTC より後に始まるコンテストを 1200 とする）
const DEFAULT_1200_SINCE: i64 = 1_761_955_200;
// 予想 diff を出すのに要る人数（AtCoder Problems と同じ）
const MIN_DIFFICULTY_USERS: usize = 40;
// 展開した順位表の大きさの上限（D-233。ABC481 は 9.7 MB）
const MAX_STANDINGS_BYTES: usize = 50_000_000;

// ---- 順位表の JSON（使う欄だけ） ----

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StandingsJson {
    // 順位が確定した（終わったあと）
    pub fixed: bool,
    pub task_info: Vec<TaskInfo>,
    pub standings_data: Vec<Row>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskInfo {
    pub task_screen_name: String,
    // 問題の記号（A・B…）
    #[serde(default)]
    pub assignment: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Row {
    pub rank: i64,
    pub user_screen_name: String,
    pub is_rated: bool,
    // コンテスト中は今のレート。終わったあとは新しいレート
    pub rating: i64,
    // 参加する前のレート
    pub old_rating: i64,
    // 過去の Rated の参加の回数
    pub competitions: i64,
    #[serde(default)]
    pub task_results: HashMap<String, TaskResult>,
    pub total_result: TotalResult,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TaskResult {
    // 得点の 100 倍
    pub score: f64,
    // 1 なら得点がある（AC）。提出したが得点がなければ 10（【手元】ABC481 の順位表。2026-11-03）
    pub status: i64,
    // 提出の数
    #[serde(default)]
    pub count: i64,
    // 判定を受けた提出の数（コンパイルエラーは数えない【推測】。得点がないときは、誤答の数として出す）
    #[serde(default)]
    pub failure: i64,
    // 得点を取るまでの誤答の数（得点があるときだけ）
    #[serde(default)]
    pub penalty: i64,
    // 得点を取った時刻（開始からのナノ秒）
    #[serde(default)]
    pub elapsed: i64,
    // 判定待ちの提出がある
    #[serde(default)]
    pub pending: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct TotalResult {
    // 提出の数（0 なら提出していない）
    pub count: i64,
    // 得点の 100 倍
    #[serde(default)]
    pub score: f64,
    // 時間（開始からのナノ秒。ペナルティの時間を含む）
    #[serde(default)]
    pub elapsed: i64,
    #[serde(default)]
    pub penalty: i64,
}

pub fn parse_standings(text: &str) -> Result<StandingsJson, String> {
    if text.len() > MAX_STANDINGS_BYTES {
        return Err(format!(
            "順位表が大きすぎるので読みません（{} MB を超えています）",
            MAX_STANDINGS_BYTES / 1_000_000
        ));
    }
    serde_json::from_str(text)
        .map_err(|_| "順位表を読めません（形が変わった可能性があります）".to_string())
}

// ---- コンテストの種類と Rated の範囲 ----

// Rated の範囲（上端がなければ「X 以上」か「All」）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatedRange {
    pub lower: i64,
    pub upper: Option<i64>,
}

// トップページの「Rated対象: ~ 1999」「Rated Range: - 1999」などを読む。
// 見出しがなければ None、Rated でなければ（「-」）Some(None)
pub fn parse_rated_range(html: &str) -> Option<Option<RatedRange>> {
    let document = Document::from(html);
    let text = document.select("span").iter().find_map(|span| {
        let text = span.text().to_string();
        let text = text.trim();
        ["Rated対象:", "Rated Range:"]
            .iter()
            .find_map(|label| text.strip_prefix(label))
            .map(|rest| rest.trim().to_string())
    })?;
    Some(range_of(&text))
}

// 「-」「All」「~ 1999」「1200 - 」「1200 ~ 2399」（2025 年の終わりに `~` から `-` に変わった。AtCoder Problems と同じ読み方）
fn range_of(text: &str) -> Option<RatedRange> {
    let text = text.trim();
    if text.is_empty() || text == "-" {
        return None;
    }
    if text == "All" {
        return Some(RatedRange {
            lower: 0,
            upper: None,
        });
    }
    let (lower, upper) = text.split_once(['~', '-'])?;
    let number = |part: &str| -> Option<Option<i64>> {
        let part = part.trim();
        if part.is_empty() {
            Some(None)
        } else {
            part.parse().ok().map(Some)
        }
    };
    Some(RatedRange {
        lower: number(lower)?.unwrap_or(0),
        upper: number(upper)?,
    })
}

// 予想パフォーマンス・予想 diff を出すか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    // アルゴリズムの Rated のコンテスト
    Rated(RatedRange),
    // Rated でない
    Unrated,
    // ヒューリスティック（AHC）。式が違うので出さない
    Heuristic,
}

// ヒューリスティックかは、ID が `ahc` で始まるか、コンテストの名前（トップページの題名）に「Heuristic」「ヒューリスティック」が
// 入っているかで決める（企業のコンテストの「…（AtCoder Heuristic Contest 031）」など、ID が `ahc` で始まらないもの。C-4）
pub fn kind_of(contest: &str, title: &str, range: Option<RatedRange>) -> Kind {
    let title = title.to_lowercase();
    if contest.starts_with("ahc")
        || title.contains("heuristic")
        || title.contains("ヒューリスティック")
    {
        return Kind::Heuristic;
    }
    match range {
        Some(range) => Kind::Rated(range),
        None => Kind::Unrated,
    }
}

// 上限（Rated の範囲の上端＋401。上端がない・4000 以上なら上限なし）
fn performance_cap(range: RatedRange) -> Option<f64> {
    range
        .upper
        .filter(|&upper| upper < 4000)
        .map(|upper| (upper + 401) as f64)
}

// 初めて参加する人の APerf（D-231）
fn default_aperf(range: RatedRange, start: i64) -> f64 {
    if start > DEFAULT_1200_SINCE {
        return 1200.0;
    }
    match range.upper {
        Some(1199) | Some(1999) => 800.0,
        Some(2799) => 1000.0,
        _ => 1200.0,
    }
}

// ---- 計算 ----

// レートと参加回数から内部のレートを逆算する（AtCoder Problems の inverse_adjust_rating）。
// レートが 0 以下・参加したことがなければ None
pub fn inner_rating(rating: i64, competitions: i64) -> Option<f64> {
    if rating <= 0 || competitions <= 0 {
        return None;
    }
    let mut rating = rating as f64;
    if rating <= 400.0 {
        rating = 400.0 * (1.0 - (400.0 / rating).ln());
    }
    let n = competitions as i32;
    let adjustment = ((1.0 - 0.9f64.powi(2 * n)).sqrt() / (1.0 - 0.9f64.powi(n)) - 1.0)
        / (19f64.sqrt() - 1.0)
        * 1200.0;
    Some(rating + adjustment)
}

// 400 以下を 400/e^((400−x)/400) に直す（表示する値）
fn positivize(x: f64) -> f64 {
    if x >= 400.0 {
        x
    } else {
        400.0 / ((400.0 - x) / 400.0).exp()
    }
}

// 順位（1 始まり。同順位は平均）から、内部のパフォーマンス（直す前）を求める
pub fn raw_performance(aperfs: &[f64], rank: f64) -> f64 {
    let expected_rank = |x: f64| -> f64 {
        aperfs
            .iter()
            .map(|aperf| 1.0 / (1.0 + 6f64.powf((x - aperf) / 400.0)))
            .sum::<f64>()
            + 0.5
    };
    let (mut lower, mut upper) = (-2048.0, 6144.0);
    while upper - lower > 0.5 {
        let middle = (upper + lower) / 2.0;
        if rank > expected_rank(middle) {
            upper = middle;
        } else {
            lower = middle;
        }
    }
    (upper + lower) / 2.0
}

// 解けた人数の期待値が accepted と等しくなる難易度（直す前。AtCoder Problems の _fit_1plm_binary_search）
pub fn raw_difficulty(ratings: &[f64], accepted: usize) -> i64 {
    let (mut lower, mut upper) = (-10000i64, 10000i64);
    while upper - lower > 1 {
        let middle = (upper + lower).div_euclid(2);
        let expected: f64 = ratings
            .iter()
            .map(|rating| 1.0 / (1.0 + 6f64.powf((middle as f64 - rating) / 400.0)))
            .sum();
        if expected < accepted as f64 {
            upper = middle;
        } else {
            lower = middle;
        }
    }
    lower
}

// ABC の A・B（ABC042 から）。高いレートの人が飛ばすことがあるので、Rated の人だけで推定する（AtCoder Problems と同じ）
fn very_easy(task: &str) -> bool {
    task.strip_prefix("abc").is_some_and(|rest| {
        rest.get(..3)
            .and_then(|number| number.parse::<u32>().ok())
            .is_some_and(|number| number >= 42)
    }) && (task.ends_with("_a") || task.ends_with("_b"))
}

// 自分の順位
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    // 全体の順位
    pub rank: i64,
    // Rated の中での順位（同順位は平均。Unrated の参加なら、Rated の人の中に入れたときの順位）
    pub rated_rank: f64,
    pub is_rated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDifficulty {
    pub task: String,
    // 予想 diff（400 未満は直した値）。出せなければ None
    pub difficulty: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    // 順位が確定した（終わったあと）
    pub fixed: bool,
    // 順位表の人数（全体）と、Rated の人数
    pub participants: usize,
    pub rated_count: usize,
    // 自分の順位。参加していなければ None
    pub place: Option<Place>,
    // 予想パフォーマンス（近似）
    pub performance: Option<i64>,
    // 予想 diff（Rated のアルゴリズムのコンテストのときだけ）
    pub difficulties: Vec<TaskDifficulty>,
    // 予想パフォーマンス・予想 diff を出さない理由
    pub note: Option<String>,
}

// 順位表から、自分（user）の順位・予想パフォーマンス・各問題の予想 diff を計算する。start はコンテストの開始時刻（UNIX 時間の秒）
pub fn summarize(standings: &StandingsJson, user: Option<&str>, kind: Kind, start: i64) -> Summary {
    let rows = &standings.standings_data;
    let rated: Vec<&Row> = rows.iter().filter(|row| row.is_rated).collect();
    let me = user.and_then(|user| rows.iter().find(|row| row.user_screen_name == user));
    let place = me.map(|me| {
        let better = rated.iter().filter(|row| row.rank < me.rank).count() as f64;
        let ties = rated.iter().filter(|row| row.rank == me.rank).count() as f64;
        Place {
            rank: me.rank,
            // 自分が Rated なら ties に自分を含む
            rated_rank: if me.is_rated {
                better + 1.0 + (ties - 1.0) / 2.0
            } else {
                better + 1.0 + ties / 2.0
            },
            is_rated: me.is_rated,
        }
    });
    let mut summary = Summary {
        fixed: standings.fixed,
        participants: rows.len(),
        rated_count: rated.len(),
        place: place.clone(),
        performance: None,
        difficulties: Vec::new(),
        note: None,
    };
    let range = match kind {
        Kind::Rated(range) => range,
        Kind::Unrated => {
            summary.note = Some(
                "Rated のコンテストではないので、予想 perf と予想 diff は出しません".to_string(),
            );
            return summary;
        }
        Kind::Heuristic => {
            summary.note = Some(
                "AHC（ヒューリスティック）なので、予想 perf と予想 diff は出しません".to_string(),
            );
            return summary;
        }
    };
    // コンテスト中は Rating、終わったあとは OldRating が参加する前のレート（ac-predictor と同じ）
    let before = |row: &Row| {
        if standings.fixed {
            row.old_rating
        } else {
            row.rating
        }
    };
    let default = default_aperf(range, start);
    if let Some(place) = &place
        && !rated.is_empty()
    {
        let aperfs: Vec<f64> = rated
            .iter()
            .map(|row| inner_rating(before(row), row.competitions).unwrap_or(default))
            .collect();
        let mut x = raw_performance(&aperfs, place.rated_rank);
        if let Some(cap) = performance_cap(range) {
            x = x.min(cap);
        }
        summary.performance = Some(positivize(x).round() as i64);
    }
    summary.difficulties = standings
        .task_info
        .iter()
        .map(|info| TaskDifficulty {
            task: info.task_screen_name.clone(),
            difficulty: difficulty_of(rows, &info.task_screen_name, &before),
        })
        .collect();
    summary
}

// 1 問の予想 diff（D-232）。満点で AC の人を「解けた」とする
fn difficulty_of(rows: &[Row], task: &str, before: &dyn Fn(&Row) -> i64) -> Option<i64> {
    let score = |row: &Row| {
        row.task_results
            .get(task)
            .map_or(0.0, |result| result.score)
    };
    let max_score = rows.iter().map(score).fold(0.0, f64::max);
    if max_score <= 0.0 {
        return None;
    }
    let easy = very_easy(task);
    let users: Vec<(&Row, f64)> = rows
        .iter()
        .filter(|row| row.total_result.count > 0 && (!easy || row.is_rated))
        .filter_map(|row| Some((row, inner_rating(before(row), row.competitions)?)))
        .collect();
    let accepted = users
        .iter()
        .filter(|(row, _)| {
            score(row) == max_score && row.task_results.get(task).is_some_and(|r| r.status == 1)
        })
        .count();
    if users.len() < MIN_DIFFICULTY_USERS || accepted == 0 || accepted == users.len() {
        return None;
    }
    let ratings: Vec<f64> = users.iter().map(|(_, rating)| *rating).collect();
    Some(positivize(raw_difficulty(&ratings, accepted) as f64).round() as i64)
}

// ---- 追跡するユーザー（Phase 31。D-253） ----

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowTask {
    pub task: String,
    // 列の見出し（A・B…。なければ問題の ID）
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowCell {
    // 得点（100 倍を直した値）
    pub score: f64,
    // 得点がある（AC。部分点を含む）
    pub accepted: bool,
    // 得点を取った時刻（開始からの秒。得点がなければ None）
    pub elapsed_secs: Option<i64>,
    // 得点を取るまでの誤答の数
    pub penalty: i64,
    // 判定を受けた提出の数（得点がないときは、誤答の数）
    pub failure: i64,
    // 提出の数
    pub count: i64,
    pub pending: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowRow {
    // 順位表にあればその書き方、なければ設定の書き方
    pub user: String,
    // 自分の行
    pub me: bool,
    // 順位表に行がない（参加していない・ID の誤り）。ほかの欄は空
    pub missing: bool,
    pub rank: Option<i64>,
    // 追跡の表の中の順位（提出のある行だけで数える。公式の順位が同じなら同じ数。D-254）
    pub group_rank: Option<i64>,
    // レート（0 なら None）。色に使う
    pub rating: Option<i64>,
    pub is_rated: bool,
    // 提出がある
    pub submitted: bool,
    pub score: f64,
    // 時間（開始からの秒。ペナルティの時間を含む）
    pub elapsed_secs: i64,
    pub penalty: i64,
    // 問題の順（FollowTable.tasks と同じ）。出していない問題は None
    pub cells: Vec<Option<FollowCell>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FollowTable {
    pub fixed: bool,
    pub tasks: Vec<FollowTask>,
    // 提出のある行を公式の順位の順（同じ順位なら自分、次に設定の順）、次に提出のない行、
    // 最後に順位表にいない行（どちらも自分、次に設定の順）。D-254
    pub rows: Vec<FollowRow>,
}

const NANOS: i64 = 1_000_000_000;

fn follow_cell(result: &TaskResult) -> FollowCell {
    let accepted = result.status == 1 && result.score > 0.0;
    FollowCell {
        score: result.score / 100.0,
        accepted,
        elapsed_secs: accepted.then_some(result.elapsed / NANOS),
        penalty: result.penalty,
        failure: result.failure,
        count: result.count,
        pending: result.pending,
    }
}

fn follow_row(tasks: &[TaskInfo], user: &str, me: bool, row: Option<&Row>) -> FollowRow {
    let Some(row) = row else {
        return FollowRow {
            user: user.to_string(),
            me,
            missing: true,
            rank: None,
            group_rank: None,
            rating: None,
            is_rated: false,
            submitted: false,
            score: 0.0,
            elapsed_secs: 0,
            penalty: 0,
            cells: vec![None; tasks.len()],
        };
    };
    FollowRow {
        user: row.user_screen_name.clone(),
        me,
        missing: false,
        rank: Some(row.rank),
        group_rank: None,
        rating: (row.rating > 0).then_some(row.rating),
        is_rated: row.is_rated,
        submitted: row.total_result.count > 0,
        score: row.total_result.score / 100.0,
        elapsed_secs: row.total_result.elapsed / NANOS,
        penalty: row.total_result.penalty,
        cells: tasks
            .iter()
            .map(|task| {
                row.task_results
                    .get(&task.task_screen_name)
                    .map(follow_cell)
            })
            .collect(),
    }
}

// 行を順位の順に並べ、追跡の表の中の順位を付ける（D-254）。並べ替えは安定なので、
// 同じ順位・提出のない行・順位表にいない行は、渡した順（自分、次に設定の順）のまま
fn rank_follow_rows(rows: &mut [FollowRow]) {
    let ranked = |row: &FollowRow| row.submitted.then_some(row.rank).flatten();
    rows.sort_by_key(|row| match ranked(row) {
        Some(rank) => (0, rank),
        None if !row.missing => (1, 0),
        None => (2, 0),
    });
    let ranks: Vec<i64> = rows.iter().filter_map(ranked).collect();
    for row in rows.iter_mut() {
        row.group_rank =
            ranked(row).map(|rank| 1 + ranks.iter().filter(|&&other| other < rank).count() as i64);
    }
}

// 順位表から、自分（me）と追跡するユーザー（follow）の行を取り出し、順位の順に並べる。
// ID の大文字・小文字は区別しない。自分を追跡の一覧にも書いていれば、1 行だけにする
pub fn follow_table(standings: &StandingsJson, me: Option<&str>, follow: &[String]) -> FollowTable {
    let find = |user: &str| {
        standings
            .standings_data
            .iter()
            .find(|row| row.user_screen_name.eq_ignore_ascii_case(user))
    };
    let tasks = &standings.task_info;
    let mut rows: Vec<FollowRow> = me
        .map(|me| follow_row(tasks, me, true, find(me)))
        .into_iter()
        .collect();
    for user in follow {
        if me.is_some_and(|me| me.eq_ignore_ascii_case(user)) {
            continue;
        }
        rows.push(follow_row(tasks, user, false, find(user)));
    }
    rank_follow_rows(&mut rows);
    FollowTable {
        fixed: standings.fixed,
        tasks: tasks
            .iter()
            .map(|task| FollowTask {
                task: task.task_screen_name.clone(),
                label: if task.assignment.is_empty() {
                    task.task_screen_name.clone()
                } else {
                    task.assignment.clone()
                },
            })
            .collect(),
        rows,
    }
}

// ---- 読む ----

// コンテストのトップページから読んだもの（アプリを開いている間だけ覚える）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContestInfo {
    pub kind: Kind,
    // 開始・終了の時刻（UNIX 時間の秒）
    pub start: i64,
    pub end: i64,
}

fn infos() -> &'static Mutex<HashMap<String, ContestInfo>> {
    static INFOS: OnceLock<Mutex<HashMap<String, ContestInfo>>> = OnceLock::new();
    INFOS.get_or_init(|| Mutex::new(HashMap::new()))
}

// 覚えている種類と開催期間を忘れる（次の contest_info で読み直す。延長に追いつくため。C-3）
pub fn forget_contest_info(contest: &str) {
    if let Ok(mut map) = infos().lock() {
        map.remove(contest);
    }
}

// トップページの題名（`<title>`。例：「AtCoder Heuristic Contest 050 - AtCoder」）
fn contest_title(html: &str) -> String {
    Document::from(html).select("title").text().to_string()
}

// トップページを読んで、種類と開催期間を決める。覚えてあれば読まない
pub fn contest_info(
    contest: &str,
    read_top: impl FnOnce() -> Result<String, String>,
) -> Result<ContestInfo, String> {
    if let Some(info) = infos()
        .lock()
        .ok()
        .and_then(|map| map.get(contest).copied())
    {
        return Ok(info);
    }
    let html = read_top()?;
    let period = crate::submit::parse_period(&html)
        .ok_or_else(|| "コンテストのページから開催期間を読めません".to_string())?;
    let range = parse_rated_range(&html)
        .ok_or_else(|| "コンテストのページから Rated の範囲を読めません".to_string())?;
    let info = ContestInfo {
        kind: kind_of(contest, &contest_title(&html), range),
        start: period.start,
        end: period.end,
    };
    if let Ok(mut map) = infos().lock() {
        map.insert(contest.to_string(), info);
    }
    Ok(info)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    tag = "kind"
)]
pub enum StandingsOutcome {
    // follow は追跡の表（Phase 31。D-253）
    Read {
        summary: Summary,
        follow: FollowTable,
    },
    // 始まる前（読まない）
    NotStarted,
    // ログインしていない（「追跡」の「更新」で、Keychain にない・ログインのページへ移された）
    NotLoggedIn,
    Failed {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(
        name: &str,
        rank: i64,
        rated: bool,
        rating: i64,
        competitions: i64,
        solved: &[&str],
    ) -> String {
        let results: Vec<String> = solved
            .iter()
            .map(|task| format!("\"{task}\":{{\"Score\":10000,\"Status\":1,\"Count\":1}}"))
            .collect();
        format!(
            "{{\"Rank\":{rank},\"UserScreenName\":\"{name}\",\"IsRated\":{rated},\"Rating\":{rating},\"OldRating\":{rating},\"Competitions\":{competitions},\"TaskResults\":{{{}}},\"TotalResult\":{{\"Count\":{}}}}}",
            results.join(","),
            if solved.is_empty() { 0 } else { 1 }
        )
    }

    fn standings(fixed: bool, tasks: &[&str], rows: &[String]) -> StandingsJson {
        let tasks: Vec<String> = tasks
            .iter()
            .map(|task| {
                format!("{{\"Assignment\":\"A\",\"TaskName\":\"t\",\"TaskScreenName\":\"{task}\"}}")
            })
            .collect();
        parse_standings(&format!(
            "{{\"Fixed\":{fixed},\"AdditionalColumns\":null,\"TaskInfo\":[{}],\"StandingsData\":[{}],\"Translation\":{{}}}}",
            tasks.join(","),
            rows.join(",")
        ))
        .unwrap()
    }

    const ABC: RatedRange = RatedRange {
        lower: 0,
        upper: Some(1999),
    };

    #[test]
    fn reads_rated_ranges() {
        // 本物のトップページ（ABC481。2026-10-31）の書き方
        let ja = "<span class=\"mr-2\">Rated対象:  ~ 1999</span>";
        assert_eq!(parse_rated_range(ja), Some(Some(ABC)));
        let en = "<span class=\"mr-2\">Rated Range: - 1999</span>";
        assert_eq!(parse_rated_range(en), Some(Some(ABC)));
        let open = "<span>Rated対象: 1200 - </span>";
        assert_eq!(
            parse_rated_range(open),
            Some(Some(RatedRange {
                lower: 1200,
                upper: None
            }))
        );
        assert_eq!(
            parse_rated_range("<span>Rated対象: All</span>"),
            Some(Some(RatedRange {
                lower: 0,
                upper: None
            }))
        );
        assert_eq!(parse_rated_range("<span>Rated対象: - </span>"), Some(None));
        assert_eq!(parse_rated_range("<span>ほかの文字</span>"), None);
        assert_eq!(kind_of("ahc050", "", Some(ABC)), Kind::Heuristic);
        assert_eq!(kind_of("abc481", "", None), Kind::Unrated);
        assert_eq!(
            kind_of(
                "abc481",
                "AtCoder Beginner Contest 481 - AtCoder",
                Some(ABC)
            ),
            Kind::Rated(ABC)
        );
        // ID が ahc で始まらなくても、名前でヒューリスティックと分かれば出さない（C-4）
        for title in [
            "トヨタ自動車プログラミングコンテスト2024#2（AtCoder Heuristic Contest 031） - AtCoder",
            "Example Heuristic Contest - AtCoder",
            "サンプル ヒューリスティックコンテスト - AtCoder",
        ] {
            assert_eq!(
                kind_of("toyota2024", title, Some(ABC)),
                Kind::Heuristic,
                "{title}"
            );
        }
        assert_eq!(
            contest_title(
                "<html><head><title>AtCoder Heuristic Contest 050 - AtCoder</title></head></html>"
            ),
            "AtCoder Heuristic Contest 050 - AtCoder"
        );
        assert_eq!(performance_cap(ABC), Some(2400.0));
        assert_eq!(
            performance_cap(RatedRange {
                lower: 1200,
                upper: None
            }),
            None
        );
    }

    #[test]
    fn computes_inner_ratings_like_atcoder_problems() {
        // Python の inverse_adjust_rating で求めた値
        assert!((inner_rating(1228, 40).unwrap() - 1233.3202).abs() < 1e-3);
        assert!((inner_rating(1000, 1).unwrap() - 2200.0).abs() < 1e-6);
        assert!((inner_rating(200, 3).unwrap() - 667.8772).abs() < 1e-3);
        assert_eq!(inner_rating(0, 5), None);
        assert_eq!(inner_rating(1500, 0), None);
    }

    #[test]
    fn computes_performance_like_ac_predictor() {
        // ac-predictor の elo.test.ts と同じ値（上限 2000、直す前）
        let aperfs = [2000.0, 1000.0, 1000.0, 0.0, -1000.0];
        let perf = |rank: f64| (raw_performance(&aperfs, rank).min(2000.0)).round();
        assert_eq!(perf(1.0), 2000.0);
        assert_eq!(perf(2.0), 1230.0);
        assert_eq!(perf(3.5), 415.0);
        assert_eq!(perf(5.0), -1010.0);
        // 400 以下は直す
        assert_eq!(positivize(400.0), 400.0);
        assert!((positivize(0.0) - 147.15).abs() < 0.01);
    }

    #[test]
    fn computes_difficulty_where_half_is_expected_to_solve() {
        // 全員が 1000 なら、半分が解けたときの難易度は 1000
        let ratings = vec![1000.0; 100];
        assert_eq!(raw_difficulty(&ratings, 50), 1000);
        // 解けた人が多いほど易しい
        assert!(raw_difficulty(&ratings, 90) < 1000);
        assert!(very_easy("abc481_a") && very_easy("abc042_b"));
        assert!(!very_easy("abc481_c") && !very_easy("abc041_a") && !very_easy("arc200_a"));
    }

    #[test]
    fn summarizes_place_performance_and_difficulty() {
        // 50 人（Rated 49 人、Unrated の me2）。上位 25 人が x_c を、上位 45 人が x_d を解いた（残りは一覧にない問題だけ）。
        // 全員のレートは 1000・参加 10 回
        let mut rows: Vec<String> = (1..=50)
            .map(|i| {
                let solved: &[&str] = match i {
                    ..=25 => &["x_c", "x_d"],
                    26..=45 => &["x_d"],
                    _ => &["x_z"],
                };
                let name = match i {
                    10 => "me".to_string(),
                    11 => "me2".to_string(),
                    _ => format!("user{i}"),
                };
                row(&name, i, i != 11, 1000, 10, solved)
            })
            .collect();
        // 提出していない人（数えない）と、参加したことのない人
        rows.push(row("idle", 51, false, 0, 0, &[]));
        rows.push(row("new", 52, true, 0, 0, &["x_d"]));
        let data = standings(false, &["x_c", "x_d"], &rows);
        let summary = summarize(&data, Some("me"), Kind::Rated(ABC), 1_800_000_000);
        assert_eq!((summary.participants, summary.rated_count), (52, 50));
        assert_eq!(
            summary.place,
            Some(Place {
                rank: 10,
                rated_rank: 10.0,
                is_rated: true
            })
        );
        // 内部のレートは全員 1157 ほど（参加 10 回の補正。初参加は 1200）。上位 2 割なので 1480 ほど
        let inner = inner_rating(1000, 10).unwrap();
        assert!((inner - 1156.8).abs() < 0.5, "{inner}");
        let performance = summary.performance.unwrap();
        assert!((1450..1520).contains(&performance), "{performance}");
        // x_c は 50 人（提出あり・参加したことがある。Unrated の me2 も入る）のうち 25 人が解いた：内部のレートと同じ
        let c = summary.difficulties[0].difficulty.unwrap();
        assert!((c as f64 - inner).abs() <= 2.0, "{c}");
        // x_d は 45 人が解いた。x_c より易しい
        assert!(summary.difficulties[1].difficulty.unwrap() < c);
        assert_eq!(summary.note, None);

        // Unrated の参加：Rated の人の中に入れたときの順位（10 位の me の次）
        let unrated = summarize(&data, Some("me2"), Kind::Rated(ABC), 1_800_000_000);
        assert_eq!(unrated.place.as_ref().unwrap().rated_rank, 11.0);
        assert!(!unrated.place.unwrap().is_rated);
        // 参加していない：順位も予想 perf もなく、予想 diff は出る
        let absent = summarize(&data, Some("nobody"), Kind::Rated(ABC), 1_800_000_000);
        assert_eq!((absent.place, absent.performance), (None, None));
        assert!(absent.difficulties[0].difficulty.is_some());
        // Rated でない・AHC：順位だけ出し、理由を出す
        for kind in [Kind::Unrated, Kind::Heuristic] {
            let other = summarize(&data, Some("me"), kind, 1_800_000_000);
            assert!(other.place.is_some());
            assert_eq!(other.performance, None);
            assert!(other.difficulties.is_empty());
            assert!(other.note.is_some());
        }
        // 人数が 40 人より少なければ、予想 diff は出さない
        let few = standings(false, &["x_c"], &rows[..30]);
        let summary = summarize(&few, Some("me"), Kind::Rated(ABC), 1_800_000_000);
        assert_eq!(summary.difficulties[0].difficulty, None);
    }

    #[test]
    fn uses_the_old_rating_after_the_contest_and_the_newcomer_default() {
        // 終わったあとは OldRating を使う。Rating（新しいレート）を変えても結果は同じ
        let make = |rating_after: i64| {
            let rows: Vec<String> = (1..=3)
                .map(|i| {
                    row(&format!("u{i}"), i, true, 1000, 5, &["x_a"])
                        .replace("\"Rating\":1000", &format!("\"Rating\":{rating_after}"))
                })
                .collect();
            standings(true, &["x_a"], &rows)
        };
        let a = summarize(&make(1000), Some("u2"), Kind::Rated(ABC), 0);
        let b = summarize(&make(3000), Some("u2"), Kind::Rated(ABC), 0);
        assert_eq!(a.performance, b.performance);
        // 初参加の APerf：2025-11-01 より後は 1200、前の ABC は 800
        assert_eq!(default_aperf(ABC, DEFAULT_1200_SINCE + 1), 1200.0);
        assert_eq!(default_aperf(ABC, DEFAULT_1200_SINCE - 1), 800.0);
        assert_eq!(
            default_aperf(
                RatedRange {
                    lower: 0,
                    upper: Some(2799)
                },
                0
            ),
            1000.0
        );
    }

    #[test]
    fn rejects_broken_or_huge_standings() {
        assert!(parse_standings("{}").is_err());
        assert!(parse_standings(&" ".repeat(MAX_STANDINGS_BYTES + 1)).is_err());
    }

    // 本物の順位表（ABC481。2026-11-03 に確かめた）と同じ書き方の行
    #[test]
    fn picks_followed_users() {
        let data = parse_standings(r#"{"Fixed":true,"AdditionalColumns":null,
            "TaskInfo":[{"Assignment":"A","TaskName":"a","TaskScreenName":"x_a"},{"Assignment":"B","TaskName":"b","TaskScreenName":"x_b"}],
            "StandingsData":[
              {"Rank":1,"UserScreenName":"Alice","IsRated":true,"Rating":2400,"OldRating":2380,"Competitions":30,
               "TaskResults":{
                 "x_a":{"Count":1,"Failure":1,"Penalty":0,"Score":10000,"Elapsed":72000000000,"Status":1,"Pending":false,"Frozen":false,"SubmissionID":5},
                 "x_b":{"Count":3,"Failure":3,"Penalty":2,"Score":20000,"Elapsed":1677000000000,"Status":1,"Pending":false,"Frozen":false,"SubmissionID":6}},
               "TotalResult":{"Count":4,"Accepted":2,"Penalty":2,"Score":30000,"Elapsed":2277000000000,"Frozen":false}},
              {"Rank":2,"UserScreenName":"me","IsRated":false,"Rating":0,"OldRating":0,"Competitions":0,
               "TaskResults":{
                 "x_a":{"Count":2,"Failure":2,"Penalty":0,"Score":0,"Elapsed":0,"Status":10,"Pending":true,"Frozen":false,"SubmissionID":0}},
               "TotalResult":{"Count":2,"Accepted":0,"Penalty":0,"Score":0,"Elapsed":0,"Frozen":false}},
              {"Rank":3,"UserScreenName":"quiet","IsRated":true,"Rating":800,"OldRating":800,"Competitions":3,
               "TaskResults":{},"TotalResult":{"Count":0,"Accepted":0,"Penalty":0,"Score":0,"Elapsed":0,"Frozen":false}}
            ],"Translation":{}}"#).unwrap();
        let follow: Vec<String> = ["alice", "ME", "nobody", "quiet"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let table = follow_table(&data, Some("me"), &follow);
        assert!(table.fixed);
        assert_eq!(
            table
                .tasks
                .iter()
                .map(|t| (t.task.as_str(), t.label.as_str()))
                .collect::<Vec<_>>(),
            [("x_a", "A"), ("x_b", "B")]
        );
        // 提出のある行を順位の順、次に提出のない行、最後に順位表にいない行（追跡の一覧の「ME」は
        // 重ねない）。名前は順位表の書き方
        let users: Vec<(&str, bool, bool, Option<i64>)> = table
            .rows
            .iter()
            .map(|r| (r.user.as_str(), r.me, r.missing, r.group_rank))
            .collect();
        assert_eq!(
            users,
            [
                ("Alice", false, false, Some(1)),
                ("me", true, false, Some(2)),
                ("quiet", false, false, None),
                ("nobody", false, true, None)
            ]
        );
        let alice = &table.rows[0];
        assert_eq!(
            (
                alice.rank,
                alice.rating,
                alice.score,
                alice.elapsed_secs,
                alice.penalty
            ),
            (Some(1), Some(2400), 300.0, 2277, 2)
        );
        assert_eq!(
            alice.cells[1],
            Some(FollowCell {
                score: 200.0,
                accepted: true,
                elapsed_secs: Some(1677),
                penalty: 2,
                failure: 3,
                count: 3,
                pending: false
            })
        );
        let me = &table.rows[1];
        assert_eq!((me.rating, me.submitted), (None, true));
        assert_eq!(
            me.cells,
            [
                Some(FollowCell {
                    score: 0.0,
                    accepted: false,
                    elapsed_secs: None,
                    penalty: 0,
                    failure: 2,
                    count: 2,
                    pending: true
                }),
                None
            ]
        );
        assert_eq!(table.rows[3].cells, [None, None]);
        assert!(!table.rows[2].submitted);
        // 自分が分からなければ、追跡する人だけ
        let table = follow_table(&data, None, &follow);
        assert_eq!(table.rows.len(), 4);
        assert_eq!(table.rows[1].user, "me");
        assert!(!table.rows[1].me);
    }

    // 追跡の表の中の順位（D-254）：公式の順位が同じなら同じ数で、次の数は飛ぶ。同じ順位の行・
    // 提出のない行・順位表にいない行は、自分、次に設定の順
    #[test]
    fn ranks_followed_users_among_themselves() {
        let row = |user: &str, rank: i64, count: i64| {
            format!(
                r#"{{"Rank":{rank},"UserScreenName":"{user}","IsRated":true,"Rating":1200,"OldRating":1200,"Competitions":5,
                "TaskResults":{{}},"TotalResult":{{"Count":{count},"Accepted":0,"Penalty":0,"Score":0,"Elapsed":0,"Frozen":false}}}}"#
            )
        };
        let rows = [
            row("a", 3, 1),
            row("b", 10, 1),
            row("c", 3, 1),
            row("d", 50, 0),
            row("e", 50, 0),
            row("me", 3, 1),
            row("f", 1, 1),
        ];
        let data = parse_standings(&format!(
            r#"{{"Fixed":false,"TaskInfo":[],"StandingsData":[{}],"Translation":{{}}}}"#,
            rows.join(",")
        ))
        .unwrap();
        let follow: Vec<String> = ["e", "x", "c", "b", "d", "a", "f", "y"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let ranked = |me: Option<&str>| {
            follow_table(&data, me, &follow)
                .rows
                .iter()
                .map(|r| (r.user.clone(), r.group_rank))
                .collect::<Vec<_>>()
        };
        let expect = |rows: &[(&str, Option<i64>)]| {
            rows.iter()
                .map(|(user, rank)| (user.to_string(), *rank))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ranked(Some("me")),
            expect(&[
                ("f", Some(1)),
                ("me", Some(2)),
                ("c", Some(2)),
                ("a", Some(2)),
                ("b", Some(5)),
                ("e", None),
                ("d", None),
                ("x", None),
                ("y", None)
            ])
        );
        assert_eq!(
            ranked(None),
            expect(&[
                ("f", Some(1)),
                ("c", Some(2)),
                ("a", Some(2)),
                ("b", Some(4)),
                ("e", None),
                ("d", None),
                ("x", None),
                ("y", None)
            ])
        );
        // 自分が順位表にいなければ、自分の行は順位表にいない行の先頭
        let missing_me = follow_table(&data, Some("ghost"), &follow);
        let last: Vec<(&str, bool)> = missing_me.rows[6..]
            .iter()
            .map(|r| (r.user.as_str(), r.me))
            .collect();
        assert_eq!(last, [("ghost", true), ("x", false), ("y", false)]);
    }
}
