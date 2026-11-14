// AtCoder Problems（kenkoooo.com。非公式の API）のデータ（Phase 24。D-241）
//
// 決まりは「1 秒より長くあけて読む」（api.md の Caution）。IDE は 2 秒あける。
// データのファイルはアプリのフォルダの atcoder-problems/ に保存し、保存から 24 時間たっていたら読み直す。
// 読めなかったときは保存してあるものを使う。中身の形を確かめてから保存する（形が違うものは保存しない）。
// 読み方：/usr/bin/curl・https だけ・--compressed・リダイレクトはたどらない・Cookie は付けない・30 秒・50 MB
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use serde_json::Value;

const BASE: &str = "https://kenkoooo.com/atcoder/";
const DIR: &str = "atcoder-problems";
// AtCoder Problems への読み取りどうしの間隔
const SPACING: Duration = Duration::from_secs(2);
// データのファイルを読み直すまで
pub const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const TIMEOUT_SECS: &str = "30";
const MAX_BYTES_ARG: &str = "50000000";
// 一度に聞いてよい問題の数
const MAX_IDS: usize = 200;

// 1 つのファイルを読む（URL を渡す）。テストでは差し替える
pub type Fetch<'a> = dyn FnMut(&str) -> Result<String, String> + 'a;

fn dir(app_dir: &Path) -> PathBuf {
    app_dir.join(DIR)
}

// 保存したファイルが、今から max_age より新しいか。更新日時が今より後（時計を戻したなど）でも、
// その差が max_age より小さければ新しいとする（毎回読み直さないように）
fn is_fresh(path: &Path, now: SystemTime, max_age: Duration) -> bool {
    let Some(modified) = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
    else {
        return false;
    };
    match now.duration_since(modified) {
        Ok(age) => age < max_age,
        Err(ahead) => ahead.duration() < max_age,
    }
}

// resources/<name> を、保存してあればそれを、古ければ（force なら新しくても）読み直して返す。
// parse で形を確かめてから保存する。読めなかったときは、古くても保存してあるものを使い、その理由も返す
pub fn load_resource_reporting<T>(
    app_dir: &Path,
    name: &str,
    now: SystemTime,
    force: bool,
    parse: fn(&str) -> Result<T, String>,
    fetch: &mut Fetch,
) -> Result<(T, Option<String>), String> {
    let path = dir(app_dir).join(name);
    let saved = || -> Option<T> {
        let text = std::fs::read_to_string(&path).ok()?;
        parse(&text).ok()
    };
    if !force
        && is_fresh(&path, now, MAX_AGE)
        && let Some(value) = saved()
    {
        return Ok((value, None));
    }
    let fetched = fetch(&format!("{BASE}resources/{name}")).and_then(|body| {
        let value = parse(&body)?;
        crate::storage::write_atomic(&path, body.as_bytes())?;
        Ok(value)
    });
    match fetched {
        Ok(value) => Ok((value, None)),
        Err(message) => match saved() {
            Some(value) => Ok((value, Some(message))),
            None => Err(message),
        },
    }
}

// 読めなかった理由は使わない形
pub fn load_resource<T>(
    app_dir: &Path,
    name: &str,
    now: SystemTime,
    parse: fn(&str) -> Result<T, String>,
    fetch: &mut Fetch,
) -> Result<T, String> {
    load_resource_reporting(app_dir, name, now, false, parse, fetch).map(|(value, _)| value)
}

// ---- 難易度 ----

// 問題の diff（AtCoder Problems の画面と同じく、400 未満は直した値）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DifficultyView {
    pub value: i64,
    // 試験的な推定（AtCoder Problems の is_experimental）
    pub experimental: bool,
}

// 400 未満は 400 / e^(1 − d/400) に直す（AtCoder Problems の画面と同じ）
pub fn clip(difficulty: f64) -> i64 {
    if difficulty >= 400.0 {
        difficulty.round() as i64
    } else {
        (400.0 / (1.0 - difficulty / 400.0).exp()).round() as i64
    }
}

pub type Models = HashMap<String, DifficultyView>;

// problem-models.json：問題の ID をキーにしたオブジェクト。difficulty のない問題は入れない。
// いちばん上がオブジェクトでない・値がオブジェクトでないものがあるときは、読めないと返す
pub fn parse_models(text: &str) -> Result<Models, String> {
    let wrong = || "AtCoder Problems の難易度のデータの形が違います".to_string();
    let value: Value = serde_json::from_str(text).map_err(|_| wrong())?;
    let object = value.as_object().ok_or_else(wrong)?;
    let mut models = HashMap::new();
    for (id, model) in object {
        let model = model.as_object().ok_or_else(wrong)?;
        let Some(difficulty) = model.get("difficulty").and_then(Value::as_f64) else {
            continue;
        };
        if !difficulty.is_finite() {
            continue;
        }
        let experimental = model
            .get("is_experimental")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        models.insert(
            id.clone(),
            DifficultyView {
                value: clip(difficulty),
                experimental,
            },
        );
    }
    if models.is_empty() {
        return Err(wrong());
    }
    Ok(models)
}

// 読んだ難易度をメモリにも覚える（ファイルの更新日時が同じ間）
struct Cached {
    path: PathBuf,
    modified: SystemTime,
    models: Arc<Models>,
}

fn cache() -> &'static Mutex<Option<Cached>> {
    static CACHE: OnceLock<Mutex<Option<Cached>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

// 難易度を返す。同時に呼ばれても、読むのは 1 回（cache の鍵を持ったまま読む）
pub fn models(app_dir: &Path, now: SystemTime, fetch: &mut Fetch) -> Result<Arc<Models>, String> {
    let path = dir(app_dir).join("problem-models.json");
    let mut cached = cache().lock().unwrap_or_else(|p| p.into_inner());
    if is_fresh(&path, now, MAX_AGE)
        && let (Some(entry), Some(modified)) = (cached.as_ref(), modified(&path))
        && entry.path == path
        && entry.modified == modified
    {
        return Ok(entry.models.clone());
    }
    let models = Arc::new(load_resource(
        app_dir,
        "problem-models.json",
        now,
        parse_models,
        fetch,
    )?);
    *cached = modified(&path).map(|modified| Cached {
        path: path.clone(),
        modified,
        models: models.clone(),
    });
    Ok(models)
}

// 問題の ID の形（英数字・_・-。64 文字まで）。AtCoder の問題の ID には大文字のものもある（APG4b_a など）
fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// 問題ごとの diff（ないものは入れない）
pub fn difficulties(
    app_dir: &Path,
    ids: &[String],
    now: SystemTime,
    fetch: &mut Fetch,
) -> Result<HashMap<String, DifficultyView>, String> {
    if ids.len() > MAX_IDS || !ids.iter().all(|id| valid_id(id)) {
        return Err("問題の ID が正しくありません".to_string());
    }
    let models = models(app_dir, now, fetch)?;
    Ok(ids
        .iter()
        .filter_map(|id| models.get(id).map(|view| (id.clone(), *view)))
        .collect())
}

// ---- 過去問の表（Phase 24 作業 2。D-242） ----

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contest {
    pub id: String,
    pub start: i64,
    pub title: String,
}

fn wrong_shape(what: &str) -> String {
    format!("AtCoder Problems の{what}のデータの形が違います")
}

// contests.json：{id, start_epoch_second, title, …} の一覧
pub fn parse_contests(text: &str) -> Result<Vec<Contest>, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| wrong_shape("コンテスト"))?;
    let items = value
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| wrong_shape("コンテスト"))?;
    items
        .iter()
        .map(|item| {
            Some(Contest {
                id: item.get("id")?.as_str()?.to_string(),
                start: item.get("start_epoch_second")?.as_i64()?,
                title: item.get("title")?.as_str()?.to_string(),
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| wrong_shape("コンテスト"))
}

// contest-problem.json：{contest_id, problem_id, problem_index} の一覧
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContestProblem {
    pub contest: String,
    pub problem: String,
    pub index: String,
}

pub fn parse_contest_problems(text: &str) -> Result<Vec<ContestProblem>, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| wrong_shape("問題"))?;
    let items = value
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or_else(|| wrong_shape("問題"))?;
    items
        .iter()
        .map(|item| {
            Some(ContestProblem {
                contest: item.get("contest_id")?.as_str()?.to_string(),
                problem: item.get("problem_id")?.as_str()?.to_string(),
                index: item.get("problem_index")?.as_str()?.to_string(),
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| wrong_shape("問題"))
}

// 表の種類（D-242・D-255）
pub const CATEGORIES: [&str; 4] = ["abc", "arc", "agc", "awc"];

// コンテストの ID が、種類の 3 文字と数字 3 桁以上のものか（ABC・ARC・AGC は数字 3 桁、AWC は 4 桁）
pub fn category_of(contest: &str, category: &str) -> bool {
    contest.len() >= 6
        && contest.starts_with(category)
        && contest[3..].bytes().all(|b| b.is_ascii_digit())
}

// JOI の表の小分け（Phase 36。D-260）。コンテスト ID の形で決める。どれにも当てはまらない joi で始まるものは「その他」
pub const JOI_KINDS: [&str; 7] = [
    "joi-yo1",
    "joi-yo",
    "joi-ho",
    "joi-sc",
    "joi-open",
    "joig",
    "joi-other",
];

// 頭の 4 文字が数字（年）なら、その後ろ
fn after_year(text: &str) -> Option<&str> {
    let year = text.get(..4)?;
    year.bytes().all(|b| b.is_ascii_digit()).then(|| &text[4..])
}

// JOI のコンテストの小分け。JOI でなければ None
// - joi-yo1：一次予選（joi2020yo1a・yo1b・yo1c）
// - joi-yo：予選（joi2006yo〜joi2019yo）・二次予選（joi2020yo2〜）
// - joi-ho：本選（joi2006ho〜joi2025ho）・セミファイナルステージ（joi2026sf）・ファイナルステージ（joi2026final）
// - joi-sc：春合宿（joisc2007・joisc2013-day1・joisp2023 など）
// - joi-open：JOI Open Contest（joiopen2012・joiopen2012b など）
// - joig：JOIG（joig2021-open・joigsp2023・joigsc2022・joig2026final など。joig で始まるもの）
pub fn joi_kind(contest: &str) -> Option<&'static str> {
    let rest = contest.strip_prefix("joi")?;
    if rest.starts_with('g') {
        return Some("joig");
    }
    let one_letter = |r: &str| r.len() == 1 && r.bytes().all(|b| b.is_ascii_lowercase());
    if let Some(r) = rest.strip_prefix("open").and_then(after_year)
        && (r.is_empty() || one_letter(r))
    {
        return Some("joi-open");
    }
    if let Some(r) = rest.strip_prefix("sc").and_then(after_year)
        && (r.is_empty()
            || r.strip_prefix("-day")
                .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit())))
    {
        return Some("joi-sc");
    }
    if rest.strip_prefix("sp").and_then(after_year) == Some("") {
        return Some("joi-sc");
    }
    if let Some(r) = after_year(rest) {
        match r {
            "ho" | "sf" | "final" => return Some("joi-ho"),
            "yo" | "yo2" => return Some("joi-yo"),
            _ if r.strip_prefix("yo1").is_some_and(one_letter) => return Some("joi-yo1"),
            _ => {}
        }
    }
    Some("joi-other")
}

// 過去問の表のどれか（ABC・ARC・AGC・AWC・JOI）に入るコンテストか（一覧に出す場所を選ぶのに使う）
fn is_table_contest(contest: &str) -> bool {
    CATEGORIES
        .iter()
        .any(|category| category_of(contest, category))
        || joi_kind(contest).is_some()
}

// コンテストがその表に入るか
fn in_category(contest: &str, category: &str) -> bool {
    if JOI_KINDS.contains(&category) {
        joi_kind(contest) == Some(category)
    } else {
        category_of(contest, category)
    }
}

// JOI の回の中の問題の並べ方：記号の数字の部分は数として比べる（K1・K2 … K6・L、1・2 … 、英単語はアルファベット順）
fn natural_key(index: &str) -> Vec<(u8, u64, String)> {
    let mut key = Vec::new();
    let mut rest = index;
    while let Some(first) = rest.chars().next() {
        let digit = first.is_ascii_digit();
        let end = rest
            .find(|c: char| c.is_ascii_digit() != digit)
            .unwrap_or(rest.len());
        let (run, next) = rest.split_at(end);
        key.push(if digit {
            (0, run.parse().unwrap_or(u64::MAX), String::new())
        } else {
            (1, 0, run.to_string())
        });
        rest = next;
    }
    key
}

// 列の並べ方：記号の順。Ex は H の後ろ（ABC の 8 問目。H と同じ回にはない）
fn index_key(index: &str) -> String {
    if index == "Ex" {
        "H\u{10FFFF}".to_string()
    } else {
        index.to_string()
    }
}

// 問題ごとの AC 状況
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Solved {
    Accepted,
    Tried,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cell {
    pub problem: String,
    // 問題の記号（例：A・K1・anagram）。JOI の表では、ますに diff の代わりに出す（D-260）
    pub index: String,
    pub difficulty: Option<DifficultyView>,
    pub solved: Option<Solved>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub contest: String,
    pub title: String,
    // columns と同じ並び。その回にない問題は None
    pub cells: Vec<Option<Cell>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
    pub accepted: usize,
    pub total: usize,
}

// 表を組み立てる。始まっていないコンテストは入れない。新しい順。
// JOI の小分け（D-260）では、列は「1・2・3 …」（その回の何問目か）にし、回の中は記号の順（natural_key）に並べる
pub fn build_table(
    category: &str,
    contests: &[Contest],
    problems: &[ContestProblem],
    models: &Models,
    solved: &HashMap<String, Solved>,
    now: i64,
) -> Table {
    let mut chosen: Vec<&Contest> = contests
        .iter()
        .filter(|c| in_category(&c.id, category) && c.start <= now)
        .collect();
    chosen.sort_by(|a, b| b.start.cmp(&a.start).then(b.id.cmp(&a.id)));
    let mut by_contest: HashMap<&str, Vec<&ContestProblem>> = HashMap::new();
    for problem in problems {
        by_contest
            .entry(problem.contest.as_str())
            .or_default()
            .push(problem);
    }
    let by_position = JOI_KINDS.contains(&category);
    if by_position {
        for list in by_contest.values_mut() {
            list.sort_by_key(|p| natural_key(&p.index));
        }
    }
    let mut columns: Vec<String> = if by_position {
        let most = chosen
            .iter()
            .map(|c| by_contest.get(c.id.as_str()).map_or(0, Vec::len))
            .max()
            .unwrap_or(0);
        (1..=most).map(|n| n.to_string()).collect()
    } else {
        chosen
            .iter()
            .flat_map(|c| by_contest.get(c.id.as_str()).into_iter().flatten())
            .map(|p| p.index.clone())
            .collect()
    };
    if !by_position {
        columns.sort_by_key(|index| index_key(index));
        columns.dedup();
    }
    let (mut accepted, mut total) = (0, 0);
    let rows = chosen
        .iter()
        .map(|contest| {
            let list = by_contest
                .get(contest.id.as_str())
                .cloned()
                .unwrap_or_default();
            let cells = columns
                .iter()
                .enumerate()
                .map(|(position, column)| {
                    let problem = if by_position {
                        list.get(position)?
                    } else {
                        list.iter().find(|p| &p.index == column)?
                    };
                    let state = solved.get(&problem.problem).copied();
                    total += 1;
                    if state == Some(Solved::Accepted) {
                        accepted += 1;
                    }
                    Some(Cell {
                        problem: problem.problem.clone(),
                        index: problem.index.clone(),
                        difficulty: models.get(&problem.problem).copied(),
                        solved: state,
                    })
                })
                .collect();
            Row {
                contest: contest.id.clone(),
                title: contest.title.clone(),
                cells,
            }
        })
        .collect();
    Table {
        columns,
        rows,
        accepted,
        total,
    }
}

// ---- 提出（AC 状況） ----

// 1 回に返る提出の数の上限（api.md）と、1 回の読み直しで読む回数の上限
const PAGE: usize = 500;
pub const MAX_PAGES: usize = 20;

// AtCoder の ID（英数字と _。16 文字まで）
pub fn valid_user(user: &str) -> bool {
    !user.is_empty()
        && user.len() <= 16
        && user.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

// 保存する AC 状況（submissions-<ID>.json）。提出そのものは保存しない
#[derive(Debug, Clone, PartialEq, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolvedState {
    pub user: String,
    // 次に読むときの from_second（前回のいちばん新しい提出の時刻。境目の提出は 2 回読むことがあるが、
    // 覚えるのは問題ごとの AC 状況だけなので、数え方は変わらない）
    pub from_second: i64,
    // 前回の読み直しで、最後まで読めた（上限の回数で止めていない）
    pub complete: bool,
    pub problems: HashMap<String, Solved>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Submission {
    pub epoch: i64,
    pub problem: String,
    pub accepted: bool,
}

// user/submissions：{id, epoch_second, problem_id, result, …} の一覧（空でもよい）
pub fn parse_submissions(text: &str) -> Result<Vec<Submission>, String> {
    let value: Value = serde_json::from_str(text).map_err(|_| wrong_shape("提出"))?;
    let items = value.as_array().ok_or_else(|| wrong_shape("提出"))?;
    items
        .iter()
        .map(|item| {
            Some(Submission {
                epoch: item.get("epoch_second")?.as_i64()?,
                problem: item.get("problem_id")?.as_str()?.to_string(),
                accepted: item.get("result")?.as_str()? == "AC",
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| wrong_shape("提出"))
}

fn solved_path(app_dir: &Path, user: &str) -> PathBuf {
    dir(app_dir).join(format!("submissions-{user}.json"))
}

pub fn saved_solved(app_dir: &Path, user: &str) -> Option<SolvedState> {
    let text = std::fs::read_to_string(solved_path(app_dir, user)).ok()?;
    serde_json::from_str::<SolvedState>(&text)
        .ok()
        .filter(|state| state.user == user)
}

// 前回の続きから提出を読み、AC 状況を覚え直す（読むのは MAX_PAGES 回まで）。途中で読めなくなったときは、
// そこまでの分を保存して、その理由も返す
pub fn update_solved(
    app_dir: &Path,
    user: &str,
    fetch: &mut Fetch,
) -> Result<(SolvedState, Option<String>), String> {
    if !valid_user(user) {
        return Err("AtCoder の ID が正しくありません".to_string());
    }
    let mut state = saved_solved(app_dir, user).unwrap_or(SolvedState {
        user: user.to_string(),
        from_second: 0,
        complete: false,
        problems: HashMap::new(),
    });
    let mut failure = None;
    state.complete = false;
    for _ in 0..MAX_PAGES {
        let url = format!(
            "{BASE}atcoder-api/v3/user/submissions?user={user}&from_second={}",
            state.from_second
        );
        let page = match fetch(&url).and_then(|body| parse_submissions(&body)) {
            Ok(page) => page,
            Err(message) => {
                failure = Some(message);
                break;
            }
        };
        let latest = page
            .iter()
            .map(|s| s.epoch)
            .max()
            .unwrap_or(state.from_second);
        for submission in &page {
            let entry = state
                .problems
                .entry(submission.problem.clone())
                .or_insert(Solved::Tried);
            if submission.accepted {
                *entry = Solved::Accepted;
            }
        }
        // 1 ページが同じ時刻の提出で埋まっていたら、次の秒から読む（同じところを読み続けない）
        let next = if page.len() >= PAGE && latest <= state.from_second {
            state.from_second + 1
        } else {
            latest.max(state.from_second)
        };
        state.from_second = next;
        if page.len() < PAGE {
            state.complete = true;
            break;
        }
    }
    let json = serde_json::to_vec(&state).map_err(|_| "AC 状況を保存できません".to_string())?;
    crate::storage::write_atomic(&solved_path(app_dir, user), &json)?;
    Ok((state, failure))
}

// ---- 表を返す ----

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PastView {
    pub table: Table,
    // AC 状況に使った ID（設定にないときは None。表は diff だけ）
    pub user: Option<String>,
    // 提出を最後まで読めている（上限の回数で止めたときは false。続きは次の「更新」で読む）
    pub solved_complete: bool,
    // 読めなかったもの（保存してあるもので出しているときも）
    pub messages: Vec<String>,
    // JOI の「その他」に入る、始まったコンテストがあるか（D-260。あるときだけ画面に「その他」を出す）
    pub joi_other: bool,
}

// 表を返す（D-242）。refresh なら、データのファイルも 24 時間を待たずに読み直す。update_solved なら提出の続きを読む
pub fn past_view(
    app_dir: &Path,
    category: &str,
    user: Option<&str>,
    refresh: bool,
    read_submissions: bool,
    now: SystemTime,
    fetch: &mut Fetch,
) -> Result<PastView, String> {
    if !CATEGORIES.contains(&category) && !JOI_KINDS.contains(&category) {
        return Err("表の種類が正しくありません".to_string());
    }
    let mut messages = Vec::new();
    let mut note = |what: &str, message: Option<String>| {
        if let Some(message) = message {
            messages.push(format!(
                "{what}を読めなかったので、保存してあるものを使います：{message}"
            ));
        }
    };
    let (contests, failed) = load_resource_reporting(
        app_dir,
        "contests.json",
        now,
        refresh,
        parse_contests,
        fetch,
    )?;
    note("コンテストの一覧", failed);
    let (problems, failed) = load_resource_reporting(
        app_dir,
        "contest-problem.json",
        now,
        refresh,
        parse_contest_problems,
        fetch,
    )?;
    note("問題の一覧", failed);
    let (models, failed) = load_resource_reporting(
        app_dir,
        "problem-models.json",
        now,
        refresh,
        parse_models,
        fetch,
    )?;
    note("難易度", failed);
    let user = user.filter(|user| valid_user(user));
    let (solved, solved_complete) = match user {
        None => (HashMap::new(), true),
        Some(user) if read_submissions => match update_solved(app_dir, user, fetch) {
            Ok((state, failed)) => {
                if let Some(message) = failed {
                    messages.push(format!("提出を最後まで読めませんでした：{message}"));
                }
                (state.problems, state.complete)
            }
            Err(message) => {
                messages.push(format!("提出を読めませんでした：{message}"));
                let saved = saved_solved(app_dir, user);
                let complete = saved.as_ref().is_some_and(|s| s.complete);
                (saved.map(|s| s.problems).unwrap_or_default(), complete)
            }
        },
        Some(user) => {
            let saved = saved_solved(app_dir, user);
            let complete = saved.as_ref().is_some_and(|s| s.complete);
            (saved.map(|s| s.problems).unwrap_or_default(), complete)
        }
    };
    let now_secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let joi_other = contests
        .iter()
        .any(|c| c.start <= now_secs && joi_kind(&c.id) == Some("joi-other"));
    Ok(PastView {
        table: build_table(category, &contests, &problems, &models, &solved, now_secs),
        user: user.map(str::to_string),
        solved_complete,
        messages,
        joi_other,
    })
}

// ---- 解き直しの一覧（Phase 34 作業 1。D-258） ----

// 問題が入っているコンテストと記号（例：abc042・C）
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub contest: String,
    pub index: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryRow {
    pub problem: String,
    // 入っているコンテスト（コンテストの ID の順。データがなければ空）
    pub places: Vec<Place>,
    // 行を押したときに問題文の欄に出すコンテスト（問題 ID の頭と同じコンテスト、なければ最初のもの、
    // データがなければ問題 ID の最後の `_` より前）
    pub contest: String,
    pub difficulty: Option<DifficultyView>,
    pub solved: Option<Solved>,
    pub solved_by: Option<crate::problem_marks::SolvedBy>,
    // 一覧に入れた日。「解き直し」は印を付けた日、「解説 AC」（D-261）は「解説」を付けた日（前の版の記録にはないので None）
    pub since: Option<String>,
    // 解き直しの印を付けた日（印がなければ None）
    pub retry: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryView {
    pub rows: Vec<RetryRow>,
    // AC 状況に使った ID（分からなければ None）
    pub user: Option<String>,
    pub messages: Vec<String>,
}

// 保存してあるデータのファイルだけを読む（古くても読み直さない）
fn saved_resource<T>(
    app_dir: &Path,
    name: &str,
    parse: fn(&str) -> Result<T, String>,
) -> Option<T> {
    let text = std::fs::read_to_string(dir(app_dir).join(name)).ok()?;
    parse(&text).ok()
}

// 解き直しの印を付けた問題の一覧（付けた日の新しい順、同じ日は問題 ID の順）。AtCoder Problems のデータは
// 保存してあるものだけを使い、新しく読まない。AC 状況も保存してあるもの（過去問の表が読んだもの）
// 一覧の種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkList {
    // 解き直しの印を付けた問題（D-258）
    Retry,
    // 「解説」を付けた問題（AC 状況は問わない。D-261）
    Editorial,
    // メモのある問題（D-263）。入れた日は最後に書いた日
    Memo,
}

// 解き直しの一覧（D-258）
pub fn retry_view(
    app_dir: &Path,
    marks: &crate::problem_marks::Marks,
    user: Option<&str>,
) -> RetryView {
    marks_view(app_dir, marks, user, MarkList::Retry)
}

// 印・記録の一覧を作る。並べ方は入れた日の新しい順（日付のないものは後ろ）、同じなら問題 ID の順。
// 「解説 AC」の並べ方の切り替えは画面側で行う（D-261）
pub fn marks_view(
    app_dir: &Path,
    marks: &crate::problem_marks::Marks,
    user: Option<&str>,
    list: MarkList,
) -> RetryView {
    let mut messages = Vec::new();
    let problems = saved_resource(app_dir, "contest-problem.json", parse_contest_problems);
    let models = saved_resource(app_dir, "problem-models.json", parse_models);
    if problems.is_none() || models.is_none() {
        messages.push(
            "コンテスト・記号・diff は、「ABC」などの表を一度開いて AtCoder Problems のデータを読むと出ます"
                .to_string(),
        );
    }
    let user = user.filter(|user| valid_user(user));
    let solved = user
        .and_then(|user| saved_solved(app_dir, user))
        .map(|state| state.problems)
        .unwrap_or_default();
    let mut rows: Vec<RetryRow> = marks
        .iter()
        .filter_map(|(problem, mark)| {
            let since = match list {
                MarkList::Retry => Some(mark.retry.clone()?),
                MarkList::Editorial => {
                    if mark.solved != Some(crate::problem_marks::SolvedBy::Editorial) {
                        return None;
                    }
                    mark.solved_on.clone()
                }
                MarkList::Memo => {
                    mark.memo.as_ref()?;
                    mark.memo_on.clone()
                }
            };
            let mut places: Vec<Place> = problems
                .iter()
                .flatten()
                .filter(|entry| &entry.problem == problem)
                .map(|entry| Place {
                    contest: entry.contest.clone(),
                    index: entry.index.clone(),
                })
                .collect();
            places.sort_by(|a, b| a.contest.cmp(&b.contest));
            // 出す場所は、過去問の表の種類（ABC・ARC・AGC・AWC・JOI）のコンテストだけにする。AtCoder Daily Training
            // （adt_…）などは ABC の問題をそのまま使うので、全部を出すと何十にもなる（2026-11-14、ユーザーの指摘）。
            // 表の種類のコンテストにない問題は、今までどおり最初の 1 つを出す
            let main: Vec<Place> = places
                .iter()
                .filter(|place| is_table_contest(&place.contest))
                .cloned()
                .collect();
            let places = if main.is_empty() {
                places.into_iter().take(1).collect()
            } else {
                main
            };
            let head = problem.rfind('_').map(|at| &problem[..at]);
            let contest = places
                .iter()
                .find(|place| Some(place.contest.as_str()) == head)
                .or(places.first())
                .map(|place| place.contest.clone())
                .or(head.map(str::to_string))
                .unwrap_or_else(|| problem.clone());
            Some(RetryRow {
                problem: problem.clone(),
                places,
                contest,
                difficulty: models
                    .as_ref()
                    .and_then(|models| models.get(problem).copied()),
                solved: solved.get(problem).copied(),
                solved_by: mark.solved,
                since,
                retry: mark.retry.clone(),
            })
        })
        .collect();
    // Option の比べ方では None がいちばん小さいので、降順にすると日付のないものが後ろになる
    rows.sort_by(|a, b| {
        b.since
            .cmp(&a.since)
            .then_with(|| a.problem.cmp(&b.problem))
    });
    RetryView {
        rows,
        user: user.map(str::to_string),
        messages,
    }
}

// 提出を前に読んだ時刻（ID ごと。開いたときは、前回から 1 分たっていれば読む）
pub fn should_read_submissions(user: &str, refresh: bool) -> bool {
    static LAST: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
    let mut last = LAST
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let due = refresh
        || last
            .get(user)
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(60));
    if due {
        last.insert(user.to_string(), Instant::now());
    }
    due
}

// ---- 読み取り ----

// 前回の読み取りの時刻（AtCoder Problems の分。AtCoder の分とは別）
static LAST: Mutex<Option<Instant>> = Mutex::new(None);

fn wait_for_spacing() {
    let mut last = LAST.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(at) = *last {
        let since = at.elapsed();
        if since < SPACING {
            std::thread::sleep(SPACING - since);
        }
    }
    *last = Some(Instant::now());
}

// 1 つの URL を curl で読む（kenkoooo.com の https だけ）
pub fn fetch(url: &str) -> Result<String, String> {
    if !url.starts_with(BASE) {
        return Err("AtCoder Problems の URL ではありません".to_string());
    }
    wait_for_spacing();
    let output = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--compressed",
            "--proto",
            "=https",
            "--max-time",
            TIMEOUT_SECS,
            "--max-filesize",
            MAX_BYTES_ARG,
            url,
        ])
        .output()
        .map_err(|error| format!("curl を起動できません：{error}"))?;
    if !output.status.success() {
        return Err(crate::session::curl_failure(
            "AtCoder Problems のデータを読めません",
            output.status.code(),
            &String::from_utf8_lossy(&output.stderr),
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "AtCoder Problems のデータを読めません（UTF-8 ではない）".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("atcoder-studio-ap-{name}-{}", std::process::id()));
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

    const MODELS: &str = r#"{"abc481_d":{"difficulty":581,"is_experimental":false,"irt_users":8237},"abc481_a":{"difficulty":-867.4},"abc001_1":{"difficulty":-305,"is_experimental":true},"abc481_g":{"irt_users":3},"x_1":{"difficulty":2400.6}}"#;

    #[test]
    fn clips_low_difficulties_like_atcoder_problems() {
        assert_eq!(clip(581.0), 581);
        assert_eq!(clip(400.0), 400);
        assert_eq!(clip(2400.6), 2401);
        // 400 / e^(1 − d/400)
        assert_eq!(clip(0.0), 147);
        assert_eq!(clip(-867.4), 17);
        assert_eq!(clip(-10000.0), 0);
        assert_eq!(clip(399.0), 399);
    }

    #[test]
    fn parses_models_and_rejects_other_shapes() {
        let models = parse_models(MODELS).unwrap();
        assert_eq!(models.len(), 4);
        assert_eq!(
            models["abc481_d"],
            DifficultyView {
                value: 581,
                experimental: false
            }
        );
        assert_eq!(models["abc481_a"].value, 17);
        assert!(models["abc001_1"].experimental);
        // difficulty のない問題は入れない
        assert!(!models.contains_key("abc481_g"));
        for text in [
            "[]",
            "{}",
            r#"{"a":1}"#,
            "<html>",
            r#"{"a":{"irt_users":1}}"#,
        ] {
            assert!(parse_models(text).is_err(), "{text}");
        }
    }

    #[test]
    fn saves_and_reloads_after_a_day() {
        let dir = TempDir::new("reload");
        let now = SystemTime::now();
        let asked = std::cell::RefCell::new(Vec::new());
        let mut fetch = |url: &str| {
            asked.borrow_mut().push(url.to_string());
            Ok(MODELS.to_string())
        };
        let first =
            load_resource(&dir.0, "problem-models.json", now, parse_models, &mut fetch).unwrap();
        assert_eq!(first.len(), 4);
        // 24 時間たつまでは読まない
        let again =
            load_resource(&dir.0, "problem-models.json", now, parse_models, &mut fetch).unwrap();
        assert_eq!(again, first);
        assert_eq!(
            *asked.borrow(),
            ["https://kenkoooo.com/atcoder/resources/problem-models.json"]
        );
        // 保存してから 24 時間たったら読み直す
        let path = dir.0.join("atcoder-problems/problem-models.json");
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(now - MAX_AGE - Duration::from_secs(1))
            .unwrap();
        load_resource(&dir.0, "problem-models.json", now, parse_models, &mut fetch).unwrap();
        assert_eq!(asked.borrow().len(), 2);
    }

    #[test]
    fn keeps_saved_data_when_reading_fails() {
        let dir = TempDir::new("fail");
        let now = SystemTime::now();
        // 保存がなく読めないときは理由を返す
        let mut failing = |_: &str| Err("つながりません".to_string());
        assert_eq!(
            load_resource(
                &dir.0,
                "problem-models.json",
                now,
                parse_models,
                &mut failing
            ),
            Err("つながりません".to_string())
        );
        // 形が違うものは保存しない
        let mut wrong = |_: &str| Ok("<html>".to_string());
        assert!(
            load_resource(&dir.0, "problem-models.json", now, parse_models, &mut wrong).is_err()
        );
        assert!(!dir.0.join("atcoder-problems/problem-models.json").exists());
        // 古い保存があれば、読めなくてもそれを使う
        let mut ok = |_: &str| Ok(MODELS.to_string());
        load_resource(&dir.0, "problem-models.json", now, parse_models, &mut ok).unwrap();
        let later = now + MAX_AGE + Duration::from_secs(1);
        assert_eq!(
            load_resource(
                &dir.0,
                "problem-models.json",
                later,
                parse_models,
                &mut failing
            )
            .unwrap()
            .len(),
            4
        );
        assert_eq!(
            load_resource(
                &dir.0,
                "problem-models.json",
                later,
                parse_models,
                &mut wrong
            )
            .unwrap()
            .len(),
            4
        );
    }

    #[test]
    fn answers_difficulties_for_valid_ids_only() {
        let dir = TempDir::new("ids");
        let now = SystemTime::now();
        let mut fetch = |_: &str| Ok(MODELS.to_string());
        let ids: Vec<String> = ["abc481_d", "abc481_g", "APG4b_a"]
            .map(String::from)
            .to_vec();
        let found = difficulties(&dir.0, &ids, now, &mut fetch).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found["abc481_d"].value, 581);
        for bad in ["../x", "a/b", "a b", ""] {
            assert!(difficulties(&dir.0, &[bad.to_string()], now, &mut fetch).is_err());
        }
        let many: Vec<String> = (0..=MAX_IDS).map(|i| format!("p{i}")).collect();
        assert!(difficulties(&dir.0, &many, now, &mut fetch).is_err());
    }

    const CONTESTS: &str = r#"[{"id":"abc481","start_epoch_second":2000,"duration_second":6000,"title":"ABC 481","rate_change":"~ 1999"},{"id":"abc480","start_epoch_second":1000,"duration_second":6000,"title":"ABC 480","rate_change":"~ 1999"},{"id":"abc482","start_epoch_second":9000,"duration_second":6000,"title":"ABC 482","rate_change":"~ 1999"},{"id":"abc200a","start_epoch_second":500,"duration_second":1,"title":"x","rate_change":"-"},{"id":"arc200","start_epoch_second":1500,"duration_second":1,"title":"ARC 200","rate_change":"-"}]"#;
    const PROBLEMS: &str = r#"[{"contest_id":"abc481","problem_id":"abc481_a","problem_index":"A"},{"contest_id":"abc481","problem_id":"abc481_h","problem_index":"H"},{"contest_id":"abc480","problem_id":"abc480_a","problem_index":"A"},{"contest_id":"abc480","problem_id":"abc480_ex","problem_index":"Ex"},{"contest_id":"abc480","problem_id":"abc480_g","problem_index":"G"},{"contest_id":"arc200","problem_id":"arc200_a","problem_index":"A"}]"#;

    #[test]
    fn builds_the_table_for_a_category() {
        let contests = parse_contests(CONTESTS).unwrap();
        let problems = parse_contest_problems(PROBLEMS).unwrap();
        let models =
            parse_models(r#"{"abc481_a":{"difficulty":-100},"abc480_g":{"difficulty":2000}}"#)
                .unwrap();
        let solved: HashMap<String, Solved> = [
            ("abc481_a".to_string(), Solved::Accepted),
            ("abc480_g".to_string(), Solved::Tried),
            ("arc200_a".to_string(), Solved::Accepted),
        ]
        .into();
        let table = build_table("abc", &contests, &problems, &models, &solved, 5000);
        // 始まっていない abc482・abc200a（数字 3 桁でない）・ARC は入らない。新しい順。Ex は H の後ろ
        assert_eq!(table.columns, ["A", "G", "H", "Ex"]);
        let ids: Vec<&str> = table.rows.iter().map(|r| r.contest.as_str()).collect();
        assert_eq!(ids, ["abc481", "abc480"]);
        let first = &table.rows[0].cells;
        assert_eq!(first[0].as_ref().unwrap().solved, Some(Solved::Accepted));
        assert_eq!(first[0].as_ref().unwrap().difficulty.unwrap().value, 115);
        assert!(first[1].is_none());
        assert_eq!(first[2].as_ref().unwrap().problem, "abc481_h");
        let second = &table.rows[1].cells;
        assert_eq!(second[1].as_ref().unwrap().solved, Some(Solved::Tried));
        assert_eq!(second[3].as_ref().unwrap().problem, "abc480_ex");
        assert_eq!((table.accepted, table.total), (1, 5));
        assert!(
            category_of("agc001", "agc")
                && category_of("awc0001", "awc")
                && !category_of("agc001a", "agc")
                && !category_of("abcxyz", "abc")
                && !category_of("awc01", "awc")
                && !category_of("awc0001", "abc")
        );
        // AWC（D-255）：数字 4 桁の ID。diff がなくても行を出し、回ごとに問題の数が違えば列はすべての記号
        let contests = parse_contests(
            r#"[{"id":"awc0001","start_epoch_second":100,"title":"AtCoder Weekday Contest 0001 Beta"},
                {"id":"awc0002","start_epoch_second":200,"title":"AtCoder Weekday Contest 0002 Beta"},
                {"id":"abc001","start_epoch_second":300,"title":"ABC 001"}]"#,
        )
        .unwrap();
        let problems = parse_contest_problems(
            r#"[{"contest_id":"awc0001","problem_id":"awc0001_a","problem_index":"A"},
                {"contest_id":"awc0002","problem_id":"awc0002_a","problem_index":"A"},
                {"contest_id":"awc0002","problem_id":"awc0002_f","problem_index":"F"},
                {"contest_id":"abc001","problem_id":"abc001_a","problem_index":"A"}]"#,
        )
        .unwrap();
        let solved: HashMap<String, Solved> = [("awc0002_f".to_string(), Solved::Accepted)].into();
        let table = build_table(
            "awc",
            &contests,
            &problems,
            &Models::default(),
            &solved,
            5000,
        );
        assert_eq!(table.columns, ["A", "F"]);
        let ids: Vec<&str> = table.rows.iter().map(|r| r.contest.as_str()).collect();
        assert_eq!(ids, ["awc0002", "awc0001"]);
        assert!(table.rows[1].cells[1].is_none());
        assert!(
            table.rows[0].cells[0]
                .as_ref()
                .unwrap()
                .difficulty
                .is_none()
        );
        assert_eq!((table.accepted, table.total), (1, 3));
        for text in ["{}", "[]", r#"[{"id":1}]"#] {
            assert!(
                parse_contests(text).is_err() && parse_contest_problems(text).is_err(),
                "{text}"
            );
        }
    }

    #[test]
    fn sorts_joi_contests_into_kinds() {
        // AtCoder Problems のデータにある形（docs/phase36/01-pre-start.md §2.1）
        let cases = [
            ("joi2020yo1a", "joi-yo1"),
            ("joi2027yo1c", "joi-yo1"),
            ("joi2006yo", "joi-yo"),
            ("joi2026yo2", "joi-yo"),
            ("joi2025ho", "joi-ho"),
            ("joi2026sf", "joi-ho"),
            ("joi2026final", "joi-ho"),
            ("joisc2007", "joi-sc"),
            ("joisc2013-day4", "joi-sc"),
            ("joisp2025", "joi-sc"),
            ("joiopen2012", "joi-open"),
            ("joiopen2012b", "joi-open"),
            ("joig2021-open", "joig"),
            ("joigsp2023", "joig"),
            ("joigsc2022", "joig"),
            ("joig2026final", "joig"),
            // これから増えるかもしれない形は「その他」
            ("joi2028yo3", "joi-other"),
            ("joisc2013-day", "joi-other"),
            ("joiopen2012bc", "joi-other"),
            ("joi20x6ho", "joi-other"),
            ("joi", "joi-other"),
        ];
        for (contest, kind) in cases {
            assert_eq!(joi_kind(contest), Some(kind), "{contest}");
        }
        for contest in ["abc001", "jag2020", "j0i2020ho", ""] {
            assert_eq!(joi_kind(contest), None, "{contest}");
        }
        // JOI の小分けは ABC などの表には入らない
        assert!(!in_category("joi2025ho", "abc") && in_category("joi2025ho", "joi-ho"));
    }

    #[test]
    fn builds_joi_tables_by_position() {
        let contests = parse_contests(
            r#"[{"id":"joisc2007","start_epoch_second":100,"title":"2007年 日本情報オリンピック春合宿OJ"},
                {"id":"joisc2020","start_epoch_second":200,"title":"JOI 春合宿 2020"},
                {"id":"joisp2025","start_epoch_second":300,"title":"JOI 2024/2025 春季トレーニング"},
                {"id":"joisp2027","start_epoch_second":9000,"title":"まだ始まっていない"},
                {"id":"joi2025ho","start_epoch_second":400,"title":"本選"}]"#,
        )
        .unwrap();
        let problems = parse_contest_problems(
            r#"[{"contest_id":"joisc2007","problem_id":"joisc2007_packin2","problem_index":"packing2"},
                {"contest_id":"joisc2007","problem_id":"joisc2007_anagra","problem_index":"anagram"},
                {"contest_id":"joisc2007","problem_id":"joisc2007_packin10","problem_index":"packing10"},
                {"contest_id":"joisc2020","problem_id":"joisc2020_l","problem_index":"L"},
                {"contest_id":"joisc2020","problem_id":"joisc2020_k2","problem_index":"K2"},
                {"contest_id":"joisc2020","problem_id":"joisc2020_k1","problem_index":"K1"},
                {"contest_id":"joisc2020","problem_id":"joisc2020_a","problem_index":"A"},
                {"contest_id":"joisp2025","problem_id":"joisp2025_a","problem_index":"A"},
                {"contest_id":"joisp2027","problem_id":"joisp2027_a","problem_index":"A"},
                {"contest_id":"joi2025ho","problem_id":"joi2025ho_a","problem_index":"A"}]"#,
        )
        .unwrap();
        let solved: HashMap<String, Solved> =
            [("joisc2020_k1".to_string(), Solved::Accepted)].into();
        let table = build_table(
            "joi-sc",
            &contests,
            &problems,
            &Models::default(),
            &solved,
            5000,
        );
        // 列は何問目か（いちばん問題の多い回の数まで）。始まっていない回・本選は入らない。新しい順
        assert_eq!(table.columns, ["1", "2", "3", "4"]);
        let ids: Vec<&str> = table.rows.iter().map(|r| r.contest.as_str()).collect();
        assert_eq!(ids, ["joisp2025", "joisc2020", "joisc2007"]);
        let indexes = |row: &Row| -> Vec<Option<String>> {
            row.cells
                .iter()
                .map(|cell| cell.as_ref().map(|c| c.index.clone()))
                .collect()
        };
        let some = |items: &[&str]| -> Vec<Option<String>> {
            items.iter().map(|i| Some(i.to_string())).collect()
        };
        // 回の中は記号の順。数字の部分は数として比べる（packing2 は packing10 より前、K1・K2 は L より前）
        assert_eq!(indexes(&table.rows[1]), some(&["A", "K1", "K2", "L"]));
        let mut words = some(&["anagram", "packing2", "packing10"]);
        words.push(None);
        assert_eq!(indexes(&table.rows[2]), words);
        assert_eq!(
            indexes(&table.rows[0]),
            [Some("A".to_string()), None, None, None]
        );
        let k1 = table.rows[1].cells[1].as_ref().unwrap();
        assert_eq!(
            (k1.problem.as_str(), k1.solved),
            ("joisc2020_k1", Some(Solved::Accepted))
        );
        assert_eq!((table.accepted, table.total), (1, 8));
        // ABC などの表は今までどおり、列は記号で、ますにも記号を持つ
        let table = build_table(
            "joi-ho",
            &contests,
            &problems,
            &Models::default(),
            &solved,
            5000,
        );
        assert_eq!(table.columns, ["1"]);
        assert_eq!(table.rows[0].cells[0].as_ref().unwrap().index, "A");
    }

    fn submissions(range: std::ops::Range<i64>, problem: &str, result: &str) -> String {
        let items: Vec<String> = range
            .map(|epoch| format!(r#"{{"id":{epoch},"epoch_second":{epoch},"problem_id":"{problem}","result":"{result}"}}"#))
            .collect();
        format!("[{}]", items.join(","))
    }

    #[test]
    fn reads_submissions_from_where_it_stopped() {
        let dir = TempDir::new("solved");
        let asked = std::cell::RefCell::new(Vec::new());
        // 1 ページ目は 500 件（満杯）、2 ページ目は境目の 1 件を含む 3 件
        let mut fetch = |url: &str| {
            asked.borrow_mut().push(url.to_string());
            Ok(if url.ends_with("from_second=0") {
                submissions(1..501, "abc001_a", "WA")
            } else {
                format!(
                    "[{},{}]",
                    &submissions(500..501, "abc001_a", "WA")
                        [1..submissions(500..501, "abc001_a", "WA").len() - 1],
                    &submissions(600..602, "abc001_a", "AC")
                        [1..submissions(600..602, "abc001_a", "AC").len() - 1]
                )
            })
        };
        let (state, failed) = update_solved(&dir.0, "user_1", &mut fetch).unwrap();
        assert_eq!(failed, None);
        assert!(state.complete);
        assert_eq!(state.from_second, 601);
        assert_eq!(state.problems["abc001_a"], Solved::Accepted);
        assert_eq!(
            *asked.borrow(),
            [
                "https://kenkoooo.com/atcoder/atcoder-api/v3/user/submissions?user=user_1&from_second=0",
                "https://kenkoooo.com/atcoder/atcoder-api/v3/user/submissions?user=user_1&from_second=500"
            ]
        );
        // 次は前回の続きから。AC のあとに WA が来ても AC のまま
        let mut later = |url: &str| {
            assert!(url.ends_with("from_second=601"), "{url}");
            Ok(submissions(700..701, "abc001_a", "WA"))
        };
        let (state, _) = update_solved(&dir.0, "user_1", &mut later).unwrap();
        assert_eq!(state.problems["abc001_a"], Solved::Accepted);
        assert_eq!(saved_solved(&dir.0, "user_1").unwrap().from_second, 700);
        // ほかの ID の保存は使わない。macOS のファイルの名前は大文字と小文字を区別しないので、
        // USER_1 で読むと user_1 のファイルが見つかるが、中の ID が違うので使わない
        assert!(saved_solved(&dir.0, "user_2").is_none());
        assert!(saved_solved(&dir.0, "USER_1").is_none());
        assert!(update_solved(&dir.0, "../x", &mut later).is_err());
    }

    #[test]
    fn stops_reading_submissions_at_the_limit() {
        let dir = TempDir::new("limit");
        let mut count = 0;
        // ずっと満杯で、どの提出も同じ時刻（100 秒）のとき：同じところを読み続けず、1 秒ずつ進めて、上限の回数で止める
        let same: Vec<String> = (0..500)
            .map(|id| {
                format!(r#"{{"id":{id},"epoch_second":100,"problem_id":"abc001_a","result":"WA"}}"#)
            })
            .collect();
        let page = format!("[{}]", same.join(","));
        let mut fetch = |_: &str| {
            count += 1;
            Ok(page.clone())
        };
        let (state, _) = update_solved(&dir.0, "user_1", &mut fetch).unwrap();
        assert_eq!(count, MAX_PAGES);
        assert!(!state.complete);
        assert_eq!(state.from_second, 100 + MAX_PAGES as i64 - 1);
        // 途中で読めなくなったら、そこまでを保存して理由を返す
        let dir2 = TempDir::new("partial");
        let mut nth = 0;
        let mut failing = |_: &str| {
            nth += 1;
            if nth == 1 {
                Ok(submissions(1..501, "abc002_a", "AC"))
            } else {
                Err("つながりません".to_string())
            }
        };
        let (state, failed) = update_solved(&dir2.0, "user_1", &mut failing).unwrap();
        assert_eq!(failed.as_deref(), Some("つながりません"));
        assert!(!state.complete);
        assert_eq!(
            saved_solved(&dir2.0, "user_1").unwrap().problems["abc002_a"],
            Solved::Accepted
        );
    }

    #[test]
    fn builds_the_view_with_messages() {
        let dir = TempDir::new("view");
        let now = SystemTime::UNIX_EPOCH + Duration::from_secs(5000);
        let mut fetch = |url: &str| {
            Ok(if url.ends_with("contests.json") {
                CONTESTS.to_string()
            } else if url.ends_with("contest-problem.json") {
                PROBLEMS.to_string()
            } else if url.ends_with("problem-models.json") {
                MODELS.to_string()
            } else {
                submissions(1..3, "abc480_a", "AC")
            })
        };
        let view = past_view(&dir.0, "abc", Some("user_1"), false, true, now, &mut fetch).unwrap();
        assert_eq!(view.user.as_deref(), Some("user_1"));
        assert!(view.solved_complete && view.messages.is_empty());
        assert_eq!(view.table.accepted, 1);
        // ID がなければ diff だけ
        let view = past_view(&dir.0, "abc", None, false, false, now, &mut fetch).unwrap();
        assert_eq!((view.user, view.table.accepted), (None, 0));
        // 「更新」で読めなければ、保存してあるもので出して理由を知らせる
        let mut failing = |_: &str| Err("つながりません".to_string());
        let view = past_view(&dir.0, "abc", Some("user_1"), true, true, now, &mut failing).unwrap();
        assert_eq!(view.table.rows.len(), 2);
        assert_eq!(view.table.accepted, 1);
        assert_eq!(view.messages.len(), 4, "{:?}", view.messages);
        assert!(past_view(&dir.0, "ahc", None, false, false, now, &mut failing).is_err());
        // JOI の小分けも読める（D-260）。テストのデータに JOI はないので、表は空で「その他」もない
        let view = past_view(&dir.0, "joi-ho", None, false, false, now, &mut failing).unwrap();
        assert!(view.table.rows.is_empty() && !view.joi_other);
        assert!(past_view(&dir.0, "joi", None, false, false, now, &mut failing).is_err());
    }

    #[test]
    fn reads_only_atcoder_problems_urls() {
        assert!(fetch("https://example.com/atcoder/resources/x.json").is_err());
        assert!(fetch("http://kenkoooo.com/atcoder/resources/x.json").is_err());
    }

    #[test]
    fn lists_retry_marks_from_saved_data_only() {
        use crate::problem_marks::{Mark, Marks, SolvedBy};
        let dir = TempDir::new("retry");
        let mark = |solved: Option<SolvedBy>, retry: Option<&str>| Mark {
            solved,
            retry: retry.map(str::to_string),
            ..Mark::default()
        };
        let marks: Marks = [
            (
                "arc058_a".to_string(),
                mark(Some(SolvedBy::Editorial), Some("2026-11-10")),
            ),
            ("abc001_1".to_string(), mark(None, Some("2026-11-12"))),
            ("abc002_1".to_string(), mark(None, Some("2026-11-10"))),
            // 印のない記録は出さない
            ("abc003_1".to_string(), mark(Some(SolvedBy::Myself), None)),
        ]
        .into_iter()
        .collect();

        // データがなければ、問題 ID から決めたコンテストだけで出し、知らせる（読まない）
        let view = retry_view(&dir.0, &marks, None);
        assert_eq!(
            view.rows
                .iter()
                .map(|row| row.problem.as_str())
                .collect::<Vec<_>>(),
            ["abc001_1", "abc002_1", "arc058_a"]
        );
        assert_eq!(view.rows[2].contest, "arc058");
        assert!(view.rows[2].places.is_empty());
        assert_eq!(view.messages.len(), 1);

        let data = dir.0.join(DIR);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(
            data.join("contest-problem.json"),
            r#"[{"contest_id":"arc058","problem_id":"arc058_a","problem_index":"C"},
                {"contest_id":"abc042","problem_id":"arc058_a","problem_index":"C"},
                {"contest_id":"abc001","problem_id":"abc001_1","problem_index":"A"}]"#,
        )
        .unwrap();
        std::fs::write(
            data.join("problem-models.json"),
            r#"{"arc058_a":{"difficulty":900.2,"is_experimental":false}}"#,
        )
        .unwrap();
        std::fs::write(
            data.join("submissions-me.json"),
            r#"{"user":"me","fromSecond":0,"complete":true,"problems":{"arc058_a":"accepted","abc001_1":"tried"}}"#,
        )
        .unwrap();
        let view = retry_view(&dir.0, &marks, Some("me"));
        assert!(view.messages.is_empty());
        assert_eq!(view.user.as_deref(), Some("me"));
        let shared = &view.rows[2];
        assert_eq!(
            shared.places,
            [
                Place {
                    contest: "abc042".to_string(),
                    index: "C".to_string()
                },
                Place {
                    contest: "arc058".to_string(),
                    index: "C".to_string()
                },
            ]
        );
        // 押したときは、問題 ID の頭と同じコンテスト
        assert_eq!(shared.contest, "arc058");
        // ABC の問題をそのまま使う AtCoder Daily Training の回は、場所に出さない（2026-11-14）
        std::fs::write(
            data.join("contest-problem.json"),
            r#"[{"contest_id":"abc001","problem_id":"abc001_1","problem_index":"A"},
                {"contest_id":"adt_all_20260209_1","problem_id":"abc001_1","problem_index":"A"},
                {"contest_id":"adt_easy_20260210_2","problem_id":"abc001_1","problem_index":"B"},
                {"contest_id":"adt_all_20260101_1","problem_id":"abc002_1","problem_index":"C"},
                {"contest_id":"adt_all_20260102_1","problem_id":"abc002_1","problem_index":"D"}]"#,
        )
        .unwrap();
        let view = retry_view(&dir.0, &marks, Some("me"));
        let row = |id: &str| view.rows.iter().find(|row| row.problem == id).unwrap();
        assert_eq!(
            row("abc001_1").places,
            [Place {
                contest: "abc001".to_string(),
                index: "A".to_string()
            }]
        );
        // 表の種類のコンテストに入っていない問題は、最初の 1 つだけ
        assert_eq!(row("abc002_1").places.len(), 1);
        assert_eq!(row("abc002_1").places[0].contest, "adt_all_20260101_1");
        assert_eq!(shared.difficulty.map(|d| d.value), Some(900));
        assert_eq!(shared.solved, Some(Solved::Accepted));
        assert_eq!(shared.solved_by, Some(SolvedBy::Editorial));
        assert_eq!(shared.since.as_deref(), Some("2026-11-10"));
        assert_eq!(shared.retry.as_deref(), Some("2026-11-10"));
        assert_eq!(view.rows[0].solved, Some(Solved::Tried));
        assert_eq!(view.rows[1].solved, None);
    }

    #[test]
    fn lists_editorial_marks_with_their_dates() {
        use crate::problem_marks::{Mark, Marks, SolvedBy};
        let dir = TempDir::new("editorial");
        let mark = |solved: Option<SolvedBy>, on: Option<&str>, retry: Option<&str>| Mark {
            solved,
            solved_on: on.map(str::to_string),
            retry: retry.map(str::to_string),
            ..Mark::default()
        };
        let marks: Marks = [
            // 前の版で付けた（日付のない）記録
            (
                "abc001_1".to_string(),
                mark(Some(SolvedBy::Editorial), None, None),
            ),
            (
                "abc002_1".to_string(),
                mark(
                    Some(SolvedBy::Editorial),
                    Some("2026-11-12"),
                    Some("2026-11-13"),
                ),
            ),
            (
                "abc003_1".to_string(),
                mark(Some(SolvedBy::Editorial), Some("2026-11-13"), None),
            ),
            // 「自力」と、解き直しの印だけのものは出さない
            (
                "abc004_1".to_string(),
                mark(Some(SolvedBy::Myself), Some("2026-11-13"), None),
            ),
            ("abc005_1".to_string(), mark(None, None, Some("2026-11-13"))),
        ]
        .into_iter()
        .collect();
        let data = dir.0.join(DIR);
        std::fs::create_dir_all(&data).unwrap();
        // AC していない問題も出す（AC 状況は問わない）
        std::fs::write(
            data.join("submissions-me.json"),
            r#"{"user":"me","fromSecond":0,"complete":true,"problems":{"abc002_1":"accepted","abc003_1":"tried"}}"#,
        )
        .unwrap();
        let view = marks_view(&dir.0, &marks, Some("me"), MarkList::Editorial);
        // 付けた日の新しい順。日付のないものは後ろ
        let rows: Vec<(&str, Option<&str>)> = view
            .rows
            .iter()
            .map(|row| (row.problem.as_str(), row.since.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                ("abc003_1", Some("2026-11-13")),
                ("abc002_1", Some("2026-11-12")),
                ("abc001_1", None),
            ]
        );
        assert_eq!(view.rows[0].solved, Some(Solved::Tried));
        assert_eq!(view.rows[1].solved, Some(Solved::Accepted));
        assert_eq!(view.rows[2].solved, None);
        // 解き直しの印も持つ
        assert_eq!(view.rows[1].retry.as_deref(), Some("2026-11-13"));
        assert_eq!(view.rows[0].retry, None);
        // 解き直しの一覧には、印のあるものだけ
        let view = retry_view(&dir.0, &marks, Some("me"));
        let ids: Vec<&str> = view.rows.iter().map(|row| row.problem.as_str()).collect();
        assert_eq!(ids, ["abc002_1", "abc005_1"]);
    }

    #[test]
    fn lists_memos_by_the_day_they_were_written() {
        use crate::problem_marks::{Mark, Marks, SolvedBy};
        let dir = TempDir::new("memo");
        let memo = |text: &str, on: Option<&str>| Mark {
            memo: Some(text.to_string()),
            memo_on: on.map(str::to_string),
            ..Mark::default()
        };
        let marks: Marks = [
            ("abc001_1".to_string(), memo("古い", Some("2026-11-10"))),
            ("abc002_1".to_string(), memo("新しい", Some("2026-11-14"))),
            ("abc003_1".to_string(), memo("日付なし", None)),
            // メモのない記録は出さない
            (
                "abc004_1".to_string(),
                Mark {
                    solved: Some(SolvedBy::Editorial),
                    ..Mark::default()
                },
            ),
        ]
        .into_iter()
        .collect();
        let view = marks_view(&dir.0, &marks, None, MarkList::Memo);
        let rows: Vec<(&str, Option<&str>)> = view
            .rows
            .iter()
            .map(|row| (row.problem.as_str(), row.since.as_deref()))
            .collect();
        assert_eq!(
            rows,
            [
                ("abc002_1", Some("2026-11-14")),
                ("abc001_1", Some("2026-11-10")),
                ("abc003_1", None),
            ]
        );
    }
}
