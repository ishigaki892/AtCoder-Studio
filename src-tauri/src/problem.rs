// 問題ページの取得・読み取り・保存（Phase 3a 作業 1。D-110〜D-112）
//
// - 公開されている問題ページ（ログインなし）を、macOS の curl コマンドで取得する（P3a-02。依存を増やさない）。
//   開催中のコンテストの問題は、ログインしていないと 404 になるので、そのときだけ、ログインしていればセッションを付けて読み直す（D-149）
// - 取得は利用者の操作のときだけ。続けて取得するときは 1 秒以上あける（D-111）
// - 取得したものはアプリのフォルダに保存し、2 回目からは保存したものを使う（D-111・D-112）
// - 問題名・制限・問題文（日本語）・サンプルを、ページの作り（docs/phase3a/01-pre-start-research.md §2）から読み取る。
//   読み取れなければ理由を返し、画面は公式ページを開く手段を示す（D-110）
//
// 問題文の HTML の無害化は作業 2（D-113）で行う。ここでは取り出すだけ

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use dom_query::{Document, Selection};
use serde::{Deserialize, Serialize};

use crate::session::{Client, NOT_LOGGED_IN};
use crate::storage;

// 続けて取得するときの間隔（D-111）
const MIN_INTERVAL: Duration = Duration::from_secs(1);
// 1 回の取得の時間の上限と、大きさの上限（問題ページは数十 KB ほど。推測）
const FETCH_TIMEOUT_SECS: &str = "20";
const MAX_PAGE_BYTES: &str = "5000000";

// 問題の ID（例：コンテスト abc430、問題 abc430_a）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemId {
    pub contest: String,
    pub task: String,
}

impl ProblemId {
    // URL やファイル名に使うので、英小文字・数字・`_`・`-` だけを許す
    pub fn new(contest: &str, task: &str) -> Result<Self, String> {
        if !valid_id(contest) || !valid_id(task) {
            return Err("コンテスト・問題の ID が正しくありません".to_string());
        }
        Ok(ProblemId {
            contest: contest.to_string(),
            task: task.to_string(),
        })
    }

    // 公式の問題ページ（取得できないときに、利用者が開く先でもある）
    pub fn url(&self) -> String {
        format!(
            "https://atcoder.jp/contests/{}/tasks/{}",
            self.contest, self.task
        )
    }
}

// コンテスト・問題の ID の形（英小文字・数字・`_`・`-`、64 文字まで）。自力・解説の記録のキーにも使う（D-257）
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

// 読み取った問題
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
    pub title: String,
    // 例：「2 sec」「1024 MiB」（ページの表記のまま）
    pub time_limit: String,
    pub memory_limit: String,
    // 問題文（日本語）の HTML。無害化の前（作業 2 で無害化する）
    pub statement_html: String,
    // サンプル（入力・期待出力）
    pub samples: Vec<Sample>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    pub input: String,
    pub output: String,
}

// 前回の取得の時刻（アプリ全体で 1 つ）
static LAST_FETCH: Mutex<Option<Instant>> = Mutex::new(None);

// 前回の取得から MIN_INTERVAL たつまで待つ（問題文の画像の取得も同じ間隔。D-240）
pub(crate) fn wait_for_interval() {
    let mut last = LAST_FETCH
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(at) = *last {
        let since = at.elapsed();
        if since < MIN_INTERVAL {
            std::thread::sleep(MIN_INTERVAL - since);
        }
    }
    *last = Some(Instant::now());
}

// ログインなしで読んだときの失敗
#[derive(Debug, PartialEq, Eq)]
enum FetchError {
    // 404（開催中のコンテストの問題は、ログインしていないとこうなる。2026-10-24 に ABC481 の開催中に確かめた）
    NotFound,
    Other(String),
}

// curl の --fail の失敗のうち、404 か。macOS の curl は、このとき終了コード 56 で
// 「curl: (56) The requested URL returned error: 404」を出す（2026-10-24 に確かめた）ので、終了コードではなく文で見る
fn is_not_found(stderr: &str) -> bool {
    stderr.contains("returned error: 404")
}

const NOT_FOUND_LOGGED_OUT: &str = "ページが見つかりません（404）。開催中のコンテストの問題は、AtCoder にログインしていないと読めません。「提出結果」タブの「ログイン」からログインしてください";

// 公開されているページを取得する。ログインなしで 404 なら、ログインしているときだけセッションを付けてもう 1 回読む（D-149）
fn fetch(url: &str) -> Result<String, String> {
    with_session_fallback(url, fetch_public(url), &Client::app())
}

fn with_session_fallback(
    url: &str,
    first: Result<String, FetchError>,
    client: &Client,
) -> Result<String, String> {
    match first {
        Ok(body) => Ok(body),
        Err(FetchError::Other(error)) => Err(error),
        Err(FetchError::NotFound) => {
            if !client.keychain().exists()? {
                return Err(NOT_FOUND_LOGGED_OUT.to_string());
            }
            wait_for_interval();
            let reply = client.request(url, &[]).map_err(|error| {
                if error == NOT_LOGGED_IN {
                    // ログインが切れていた（Keychain の値は消えている）
                    NOT_FOUND_LOGGED_OUT.to_string()
                } else {
                    format!("ページを取得できません：{error}")
                }
            })?;
            if reply.status == 200 {
                Ok(reply.body)
            } else {
                Err(format!(
                    "ページを取得できません（ログインして読んでも HTTP {}）",
                    reply.status
                ))
            }
        }
    }
}

// 公開されているページを、ログインなしで curl で取得する。https だけを許し、リダイレクトの先も https に限る
fn fetch_public(url: &str) -> Result<String, FetchError> {
    wait_for_interval();
    let output = Command::new("/usr/bin/curl")
        .args([
            "--silent",
            "--show-error",
            "--fail",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--max-time",
            FETCH_TIMEOUT_SECS,
            "--max-filesize",
            MAX_PAGE_BYTES,
            url,
        ])
        .output()
        .map_err(|error| FetchError::Other(format!("curl を起動できません：{error}")))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_not_found(&stderr) {
            return Err(FetchError::NotFound);
        }
        // 問題の一覧のページにも使うので、「問題ページ」とは書かない
        return Err(FetchError::Other(crate::session::curl_failure(
            "AtCoder のページを取得できません",
            output.status.code(),
            &stderr,
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|_| FetchError::Other("問題ページを読めません（UTF-8 ではない）".to_string()))
}

// 空白をまとめて前後を切る
fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

// 問題文の区画（D-256）：見出し（h3）ごとに、その h3 の親の要素が、その h3 だけを含むなら、それを区画とする
// （その h3 だけを含む、いちばん近い祖先の要素）。見出しの文字（空白をまとめたもの）と区画の要素を、問題文の順に返す。
// 今の問題と abc020・abc041 の形は section、abc001 の形（div.part > h3 と section が並ぶ）は div.part になる。
// 同じ親に h3 が並ぶページは、その区画を読めない（docs/phase32/01-pre-start.md §3.2）
pub(crate) fn sections<'a>(root: &Selection<'a>) -> Vec<(String, Selection<'a>)> {
    root.select("h3")
        .nodes()
        .iter()
        .filter_map(|h3| {
            let block = Selection::from(h3.parent()?);
            (block.select("h3").length() == 1).then(|| (squash(h3.text().as_ref()), block))
        })
        .collect()
}

// 問題ページの HTML から問題を読み取る（ページの作りは 01 §2）
pub fn parse_problem(html: &str) -> Result<Problem, String> {
    let document = Document::from(html);
    parse_in(&document.select("html"))
}

// root の中から問題を読み取る。問題ページ全体（parse_problem）と、印刷用のページの問題ごとの区画（parse_print_page。D-262）で使う
fn parse_in(document: &Selection) -> Result<Problem, String> {
    // 問題名：span.h2 の、リンク（「解説」）を除いた文字
    let heading = document.select("span.h2");
    if !heading.exists() {
        return Err("問題名が見つかりません（ページの作りが変わった可能性があります）".to_string());
    }
    let link_text = heading.select("a").text().to_string();
    let title = squash(heading.text().as_ref().replacen(&link_text, "", 1).as_str());

    // 制限：「実行時間制限: 2 sec / メモリ制限: 1024 MiB」の 1 行。
    // 英語の画面では「Time Limit: 2 sec / Memory Limit: 1024 MiB」になる
    let limits = document
        .select("p")
        .iter()
        .map(|p| squash(p.text().as_ref()))
        .find(|text| text.starts_with("実行時間制限:") || text.starts_with("Time Limit:"))
        .ok_or_else(|| {
            "実行時間制限が見つかりません（ページの作りが変わった可能性があります）".to_string()
        })?;
    let (time_part, memory_part) = limits
        .split_once(" / ")
        .ok_or_else(|| "制限の行を読めません".to_string())?;
    let value = |part: &str| {
        part.split_once(':')
            .map(|(_, value)| value.trim().to_string())
            .unwrap_or_default()
    };
    let time_limit = value(time_part);
    let memory_limit = value(memory_part);

    // 問題文（日本語）。古い ABC（ABC001〜041 など）は日本語だけで lang-ja がないので、最初の #task-statement を使う
    // （abc020 などは #task-statement が入れ子になっているので、外側の 1 つだけ。D-256）
    let mut statement = document.select("#task-statement .lang-ja");
    if !statement.exists() {
        statement = document.select("#task-statement").first();
    }
    if !statement.exists() {
        return Err(
            "問題文（日本語）が見つかりません（ページの作りが変わった可能性があります）"
                .to_string(),
        );
    }
    let statement_html = statement.inner_html().to_string();

    // サンプル：見出し（h3）が「入力例 N」「出力例 N」の区画の、最初の pre。番号で入力と出力を組にする。
    // 取得したページの pre には id がない（ブラウザで開いたあとの pre-sample0… の id は、ページのスクリプトが付けたものと推測。
    // 2026-10-25 に、保存した問題で確かめた）ので、id では探さない
    let mut inputs: BTreeMap<u32, String> = BTreeMap::new();
    let mut outputs: BTreeMap<u32, String> = BTreeMap::new();
    for (heading, section) in sections(&statement) {
        let (found, rest) = if let Some(rest) = heading.strip_prefix("入力例") {
            (&mut inputs, rest)
        } else if let Some(rest) = heading.strip_prefix("出力例") {
            (&mut outputs, rest)
        } else {
            continue;
        };
        let Ok(number) = rest.trim().parse::<u32>() else {
            continue;
        };
        let pre = section.select("pre").first();
        if !pre.exists() {
            return Err(format!("「{heading}」の中身が見つかりません"));
        }
        found
            .entry(number)
            .or_insert_with(|| pre.text().to_string());
    }
    if !inputs.keys().eq(outputs.keys()) {
        return Err("サンプルの入力と出力の数が合いません".to_string());
    }
    let with_newline = |text: &str| {
        let mut text = text.replace("\r\n", "\n");
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text
    };
    let samples = inputs
        .values()
        .zip(outputs.values())
        .map(|(input, output)| Sample {
            input: with_newline(input),
            output: with_newline(output),
        })
        .collect();

    Ok(Problem {
        title,
        time_limit,
        memory_limit,
        statement_html,
        samples,
    })
}

// 出力の誤差の範囲（Phase 9 作業 1。D-161）。abs・rel は、問題文が絶対誤差・相対誤差のどちらを書いているか
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tolerance {
    pub abs: bool,
    pub rel: bool,
    // 10^{-k} の k
    pub exponent: u32,
}

// 問題文（日本語）の「出力」の区画に「誤差」と 10^{-k} があれば、その範囲。読めなければ None。
// 「絶対誤差」「相対誤差」のどちらも書いていなければ、両方とする【推測】
pub fn output_tolerance(statement_html: &str) -> Option<Tolerance> {
    let document = Document::from(statement_html);
    let (_, section) = sections(&document.select("body"))
        .into_iter()
        .find(|(heading, _)| heading == "出力")?;
    let text = squash(section.text().as_ref());
    if !text.contains("誤差") {
        return None;
    }
    let exponent = power_of_ten(&text)?;
    let (abs, rel) = (text.contains("絶対誤差"), text.contains("相対誤差"));
    let (abs, rel) = if abs || rel { (abs, rel) } else { (true, true) };
    Some(Tolerance { abs, rel, exponent })
}

// 答えが複数ありうる問題か（Phase 10 作業 5。D-168）。問題文（日本語）に「どれを出力しても」「いずれか 1 つを出力」
// 「出力しても正解（正答）」などの書き方があれば true。空白を除いてから探す【推測】書き方が違う問題は読めない
pub fn multiple_answers(statement_html: &str) -> bool {
    const PHRASES: [&str; 6] = [
        "どれを出力しても",
        "いずれを出力しても",
        "いずれか1つを出力",
        "いずれか一つを出力",
        "出力しても正解",
        "出力しても正答",
    ];
    let text: String = Document::from(statement_html)
        .select("body")
        .text()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    PHRASES.iter().any(|phrase| text.contains(phrase))
}

// 「10^{-6}」「10^{−6}」（U+2212）「10^-6」の 6。最初に読めたもの
fn power_of_ten(text: &str) -> Option<u32> {
    let mut rest = text;
    while let Some(index) = rest.find("10^") {
        rest = &rest[index + 3..];
        let after = rest.strip_prefix('{').unwrap_or(rest).trim_start();
        let Some(after) = after
            .strip_prefix('-')
            .or_else(|| after.strip_prefix('−'))
            .map(str::trim_start)
        else {
            continue;
        };
        let digits: String = after.chars().take_while(char::is_ascii_digit).collect();
        if let Ok(exponent) = digits.parse::<u32>()
            && (1..=18).contains(&exponent)
        {
            return Some(exponent);
        }
    }
    None
}

// 保存先：アプリのフォルダの problems/<コンテスト>/<問題>.json（D-112）
fn cache_path(app_dir: &Path, id: &ProblemId) -> PathBuf {
    app_dir
        .join("problems")
        .join(&id.contest)
        .join(format!("{}.json", id.task))
}

// 問題を返す。保存してあればそれを使い、refresh が true（「再取得」）か保存がなければ取得する（D-111）
pub fn load_problem(app_dir: &Path, id: &ProblemId, refresh: bool) -> Result<Problem, String> {
    let path = cache_path(app_dir, id);
    if !refresh
        && let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(problem) = serde_json::from_str::<Problem>(&text)
    {
        return Ok(problem);
    }
    // 画面の言語を日本語にして取得する（指定しないと英語の画面になり、制限の行が「Time Limit:」になる。推測）
    let problem = parse_problem(&fetch(&format!("{}?lang=ja", id.url()))?)?;
    let json =
        serde_json::to_vec_pretty(&problem).map_err(|_| "問題を保存できません".to_string())?;
    storage::write_atomic(&path, &json)?;
    Ok(problem)
}

// 印刷用のページ（`/contests/<ID>/tasks_print`。D-262）から、問題の一覧（tasks）にある問題を取り出す。
// ページには問題 ID がないので、問題ごとの区画（span.h2 の親）の見出しの頭「<記号> - 」を、一覧の記号と突き合わせる。
// 同じ記号が一覧かページに 2 つ以上ある問題と、読めない区画は使わない（1 問ずつ問題ページを読む）
pub fn parse_print_page(html: &str, tasks: &[TaskEntry]) -> Vec<(String, Problem)> {
    let document = Document::from(html);
    let headings: Vec<Selection> = document.select("span.h2").iter().collect();
    let label_of = |heading: &Selection| {
        squash(heading.text().as_ref())
            .split_once(" - ")
            .map(|(label, _)| label.to_string())
    };
    let labels: Vec<Option<String>> = headings.iter().map(label_of).collect();
    let count = |label: &str| {
        labels
            .iter()
            .filter(|l| l.as_deref() == Some(label))
            .count()
    };
    headings
        .iter()
        .zip(&labels)
        .filter_map(|(heading, label)| {
            let label = label.as_deref()?;
            let mut matched = tasks.iter().filter(|entry| entry.label == label);
            let entry = matched.next()?;
            if matched.next().is_some() || count(label) != 1 {
                return None;
            }
            let problem = parse_in(&heading.parent()).ok()?;
            Some((entry.task.clone(), problem))
        })
        .collect()
}

// 「コンテストを作る」の前に、保存していない問題が 2 問以上あれば、印刷用のページを 1 回読み、取り出せた問題を保存する（D-262）。
// 保存した問題の数を返す。読めなかったとき・取り出せなかった問題は何もしない（あとで load_problem が 1 問ずつ読む）
pub fn prefetch_contest(app_dir: &Path, contest: &str, tasks: &[TaskEntry]) -> usize {
    let unsaved: Vec<ProblemId> = tasks
        .iter()
        .filter_map(|entry| ProblemId::new(contest, &entry.task).ok())
        .filter(|id| {
            std::fs::read_to_string(cache_path(app_dir, id))
                .ok()
                .and_then(|text| serde_json::from_str::<Problem>(&text).ok())
                .is_none()
        })
        .collect();
    if unsaved.len() < 2 || ProblemId::new(contest, "x").is_err() {
        return 0;
    }
    let url = format!("https://atcoder.jp/contests/{contest}/tasks_print?lang=ja");
    let Ok(html) = fetch(&url) else {
        return 0;
    };
    save_printed(app_dir, &unsaved, parse_print_page(&html, tasks))
}

// 取り出した問題のうち、保存していなかったものを保存する。保存した数を返す
fn save_printed(app_dir: &Path, unsaved: &[ProblemId], problems: Vec<(String, Problem)>) -> usize {
    problems
        .into_iter()
        .filter_map(|(task, problem)| {
            let id = unsaved.iter().find(|id| id.task == task)?;
            let json = serde_json::to_vec_pretty(&problem).ok()?;
            storage::write_atomic(&cache_path(app_dir, id), &json).ok()
        })
        .count()
}

// 保存してある問題だけを返す（取得しない。Phase 15 の input! を作るときに使う。D-194）
pub fn load_saved(app_dir: &Path, id: &ProblemId) -> Result<Problem, String> {
    let text = std::fs::read_to_string(cache_path(app_dir, id)).map_err(|_| {
        "問題がまだ読み込まれていません。問題文を表示してから作ってください".to_string()
    })?;
    serde_json::from_str(&text)
        .map_err(|_| "保存してある問題を読めません。「再取得」を押してください".to_string())
}

// サンプルを保存した結果（D-112）
#[derive(Debug, Default, PartialEq, Serialize)]
pub struct SavedSamples {
    // 新しく書いたファイル
    pub written: Vec<String>,
    // すでに同じ内容だったファイル
    pub unchanged: Vec<String>,
    // 内容が違うので書かなかったファイル（上書きしない）
    pub skipped: Vec<String>,
    // 問題のサンプルの数より大きい番号の sample-N（「再取得」でサンプルが減ったときなど）。消さずに知らせる
    // （Phase 10 作業 12。利用者が中身を変えているかもしれないので）
    pub leftover: Vec<String>,
}

// サンプルをフォルダに sample-N.in・sample-N.out として書く（N は 1 から）。
// すでにあるファイルは上書きしない。同じ内容なら unchanged、違えば skipped に入れる
pub fn save_samples(folder: &Path, samples: &[Sample]) -> Result<SavedSamples, String> {
    std::fs::create_dir_all(folder)
        .map_err(|_| "テストケースのフォルダを作れません".to_string())?;
    let mut saved = SavedSamples::default();
    for (index, sample) in samples.iter().enumerate() {
        for (extension, content) in [("in", &sample.input), ("out", &sample.output)] {
            let name = format!("sample-{}.{extension}", index + 1);
            let path = folder.join(&name);
            match std::fs::read(&path) {
                Ok(existing) if existing == content.as_bytes() => saved.unchanged.push(name),
                Ok(_) => saved.skipped.push(name),
                Err(_) => {
                    // 「なければ作る」を 1 回の操作で行う（確かめてから書くまでの間に作られても上書きしない）
                    let mut file = std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&path)
                        .map_err(|_| format!("{name} を書けません"))?;
                    std::io::Write::write_all(&mut file, content.as_bytes())
                        .map_err(|_| format!("{name} を書けません"))?;
                    saved.written.push(name);
                }
            }
        }
    }
    let mut leftover: Vec<(usize, String)> = std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let stem = name
                        .strip_suffix(".in")
                        .or_else(|| name.strip_suffix(".out"))?;
                    let number: usize = stem.strip_prefix("sample-")?.parse().ok()?;
                    (number > samples.len()).then_some((number, name))
                })
                .collect()
        })
        .unwrap_or_default();
    leftover.sort();
    saved.leftover = leftover.into_iter().map(|(_, name)| name).collect();
    Ok(saved)
}

// 貼り付けたテストケースを足す（Phase 25 作業 1。D-243）。1 つの欄の上限
pub const MAX_CASE_BYTES: usize = 4 * 1024 * 1024;

// 改行を LF にそろえ、空でなければ最後に改行を 1 つ付ける（AtCoder のサンプルと同じ形）
pub fn normalize_case_text(text: &str) -> String {
    let mut text = text.replace("\r\n", "\n").replace('\r', "\n");
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text
}

// フォルダの sample-N（.in か .out）のいちばん大きい N の次。なければ 1。random-N などは数えない
pub fn next_sample_number(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .map(|entries| {
            entries
                .flatten()
                .filter_map(|entry| {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    let stem = name
                        .strip_suffix(".in")
                        .or_else(|| name.strip_suffix(".out"))?;
                    let digits = stem.strip_prefix("sample-")?;
                    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
                        return None;
                    }
                    digits.parse::<usize>().ok()
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
        + 1
}

// 入力と期待出力を sample-N.in・sample-N.out として足す（上書きしない）。期待出力が空なら .out は作らない
// （目視確認のケースになる）。足したケースの名前（sample-N）を返す
pub fn add_case(folder: &Path, input: &str, expected: &str) -> Result<String, String> {
    if input.len() > MAX_CASE_BYTES || expected.len() > MAX_CASE_BYTES {
        return Err("4 MB より大きいので足せません".to_string());
    }
    let input = normalize_case_text(input);
    let expected = normalize_case_text(expected);
    if input.trim().is_empty() {
        return Err("入力が空です".to_string());
    }
    std::fs::create_dir_all(folder)
        .map_err(|_| "テストケースのフォルダを作れません".to_string())?;
    let name = format!("sample-{}", next_sample_number(folder));
    let write = |extension: &str, content: &str| -> Result<(), String> {
        let file_name = format!("{name}.{extension}");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(folder.join(&file_name))
            .map_err(|_| format!("{file_name} を作れません（すでにあるかもしれません）"))?;
        std::io::Write::write_all(&mut file, content.as_bytes())
            .map_err(|_| format!("{file_name} を書けません"))
    };
    write("in", &input)?;
    if !expected.is_empty() {
        write("out", &expected)?;
    }
    Ok(name)
}

// 問題の解説の一覧のページ（Phase 25 作業 2。D-244）
pub fn editorial_url(id: &ProblemId) -> String {
    format!(
        "https://atcoder.jp/contests/{}/tasks/{}/editorial?lang=ja",
        id.contest, id.task
    )
}

// コンテストの問題の一覧の 1 行（選択欄に出す。D-117）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskEntry {
    // 問題 ID（例：abc475_a）
    pub task: String,
    // 例：「A」
    pub label: String,
    // 例：「mnclr」
    pub title: String,
}

// コンテストの問題の一覧のページ（`/contests/<ID>/tasks`）から、問題を読み取る。
// 表の各行の 1 列目のリンク（`/contests/<ID>/tasks/<問題 ID>`）と 2 列目の問題名を使う
pub fn parse_task_list(html: &str, contest: &str) -> Result<Vec<TaskEntry>, String> {
    let document = Document::from(html);
    let prefix = format!("/contests/{contest}/tasks/");
    let tasks: Vec<TaskEntry> = document
        .select("table tbody tr")
        .iter()
        .filter_map(|row| {
            let cells = row.select("td");
            let first = cells.first();
            let href = first.select("a").attr("href")?;
            let task = href.strip_prefix(prefix.as_str())?.to_string();
            ProblemId::new(contest, &task).ok()?;
            let title = row.select("td:nth-child(2)").text();
            Some(TaskEntry {
                task,
                label: squash(first.text().as_ref()),
                title: squash(title.as_ref()),
            })
        })
        .collect();
    if tasks.is_empty() {
        return Err(
            "問題の一覧が見つかりません（コンテスト ID を確かめてください。ページの作りが変わった可能性もあります）"
                .to_string(),
        );
    }
    Ok(tasks)
}

// 保存した問題の一覧の場所（contest は形を確かめてから渡す）
fn task_list_path(app_dir: &Path, contest: &str) -> PathBuf {
    app_dir.join("task-lists").join(format!("{contest}.json"))
}

// 実行の対象のパッケージ（作業領域のルートからの相対パス。例：joi2027yo1b/joi2027_yo1b_a）から問題を決める（D-155）。
// フォルダ名を問題 ID とし、コンテスト ID は次の順で決める。決まらなければ None（選択欄で選ぶ。D-117）
// 1. 親のフォルダ名のコンテストの、保存した問題の一覧に、その問題 ID があれば親のフォルダ名
//    （「コンテストを作る」「この問題を作る」で作ったもの。JOI の joi2027yo1b/joi2027_yo1b_a、ABC042 の abc042/arc058_a など）
// 2. 最後の `_` より前と親のフォルダ名を、`_`・`-` を除いて比べて同じなら親のフォルダ名（一覧を消したとき）
// 3. 最後の `_` より前（今までの決まり。problems/abc001_a → abc001）
pub fn package_problem(app_dir: &Path, package: &str) -> Option<ProblemId> {
    let mut parts = package.rsplit('/');
    let name = parts.next()?;
    let index = name.rfind('_');
    if let Some(parent) = parts.next()
        && let Ok(id) = ProblemId::new(parent, name)
    {
        let listed = std::fs::read_to_string(task_list_path(app_dir, parent))
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<TaskEntry>>(&text).ok())
            .is_some_and(|tasks| tasks.iter().any(|entry| entry.task == name));
        let squash = |text: &str| text.replace(['_', '-'], "");
        let same_name =
            index.is_some_and(|index| index > 0 && squash(&name[..index]) == squash(parent));
        if listed || same_name {
            return Some(id);
        }
    }
    let index = index.filter(|&index| index > 0 && index + 1 < name.len())?;
    ProblemId::new(&name[..index], name).ok()
}

// コンテストの問題の一覧を返す。保存してあればそれを使い、refresh のときか保存がなければ取得する（D-111・D-117）。
// 保存先はアプリのフォルダの task-lists/<コンテスト>.json
pub fn load_task_list(
    app_dir: &Path,
    contest: &str,
    refresh: bool,
) -> Result<Vec<TaskEntry>, String> {
    // ID の形を確かめる（問題 ID の部分は仮のもの）
    ProblemId::new(contest, "x")?;
    let path = task_list_path(app_dir, contest);
    if !refresh
        && let Ok(text) = std::fs::read_to_string(&path)
        && let Ok(tasks) = serde_json::from_str::<Vec<TaskEntry>>(&text)
    {
        return Ok(tasks);
    }
    let url = format!("https://atcoder.jp/contests/{contest}/tasks?lang=ja");
    let tasks = parse_task_list(&fetch(&url)?, contest)?;
    let json =
        serde_json::to_vec_pretty(&tasks).map_err(|_| "問題の一覧を保存できません".to_string())?;
    storage::write_atomic(&path, &json)?;
    Ok(tasks)
}

// 公式の問題ページを、macOS の open コマンドで既定のブラウザで開く（D-110 の代替手段）。
// URL は ID を確かめてから IDE が組み立てたもの
pub fn open_in_browser(id: &ProblemId) -> Result<(), String> {
    open_url(&id.url())
}

// 解説の一覧を既定のブラウザで開く（D-244。IDE は読まない）
pub fn open_editorial(id: &ProblemId) -> Result<(), String> {
    open_url(&editorial_url(id))
}

fn open_url(url: &str) -> Result<(), String> {
    let status = Command::new("/usr/bin/open")
        .arg(url)
        .status()
        .map_err(|error| format!("ブラウザを開けません：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("ブラウザを開けません".to_string())
    }
}

// パッケージごとに選んだ問題（D-117）。アプリのフォルダの problem-choices.json。
// キーはパッケージのディレクトリの絶対パス
fn choices_path(app_dir: &Path) -> PathBuf {
    app_dir.join("problem-choices.json")
}

fn read_choices(app_dir: &Path) -> BTreeMap<String, ProblemId> {
    std::fs::read_to_string(choices_path(app_dir))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn choice_for(app_dir: &Path, package_dir: &Path) -> Option<ProblemId> {
    read_choices(app_dir).remove(package_dir.to_string_lossy().as_ref())
}

pub fn set_choice(app_dir: &Path, package_dir: &Path, id: ProblemId) -> Result<(), String> {
    let mut choices = read_choices(app_dir);
    choices.insert(package_dir.to_string_lossy().into_owned(), id);
    let json = serde_json::to_vec_pretty(&choices)
        .map_err(|_| "選んだ問題を保存できません".to_string())?;
    storage::write_atomic(&choices_path(app_dir), &json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::tests::{TestKeychain, serve_once};

    #[test]
    fn recognizes_curl_not_found() {
        assert!(is_not_found(
            "curl: (56) The requested URL returned error: 404"
        ));
        assert!(is_not_found(
            "curl: (22) The requested URL returned error: 404"
        ));
        assert!(!is_not_found(
            "curl: (56) The requested URL returned error: 403"
        ));
        assert!(!is_not_found(
            "curl: (6) Could not resolve host: atcoder.jp"
        ));
    }

    #[test]
    fn reads_with_the_session_only_after_a_404_when_logged_in() {
        let keychain = TestKeychain::new("problem-fallback");
        let client = keychain.client();
        // 読めたもの・404 以外の失敗は、そのまま（セッションを使わない）
        assert_eq!(
            with_session_fallback("http://127.0.0.1:9/", Ok("page".to_string()), &client),
            Ok("page".to_string())
        );
        assert_eq!(
            with_session_fallback(
                "http://127.0.0.1:9/",
                Err(FetchError::Other("つながりません".to_string())),
                &client
            ),
            Err("つながりません".to_string())
        );
        // ログインしていなければ、送らずにログインを勧める
        let not_found =
            with_session_fallback("http://127.0.0.1:9/", Err(FetchError::NotFound), &client);
        assert_eq!(not_found, Err(NOT_FOUND_LOGGED_OUT.to_string()));
        // ログインしていれば、セッションを付けてもう 1 回読む
        keychain.0.save("session-value").unwrap();
        let (url, server) = serve_once(
            "HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\ntask page",
        );
        assert_eq!(
            with_session_fallback(&url, Err(FetchError::NotFound), &client),
            Ok("task page".to_string())
        );
        assert!(
            server
                .join()
                .unwrap()
                .contains("Cookie: REVEL_SESSION=session-value")
        );
        // ログインしても読めなければ、その状態を出す
        let (url, server) =
            serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        assert!(
            with_session_fallback(&url, Err(FetchError::NotFound), &client)
                .unwrap_err()
                .contains("HTTP 404")
        );
        server.join().unwrap();
    }

    // 問題ページの作りをまねた小さな例（01 §2 の観察と、2026-10-25 に保存した問題の HTML による。実際のページはテストで取得しない）。
    // 取得したページの pre には id がない。「入力」の区画の pre（入力の形式）と、出力例の説明の中の pre は、サンプルではない
    const PAGE: &str = r#"<html><body>
<span class="h2">
    A - Happy Birthday!
    <a class="btn btn-default btn-sm" href="/contests/abc100/tasks/abc100_a/editorial">解説</a>
</span>
<p>実行時間制限: 2 sec / メモリ制限: 1024 MiB</p>
<div id="task-statement"><span class="lang">
<span class="lang-ja">
<p>配点 : <var>100</var> 点</p>
<div class="part"><section><h3>問題文</h3><p><var>A</var> 個と <var>B</var> 個</p></section></div>
<div class="io-style">
<div class="part"><section><h3>入力</h3><p>入力は以下の形式で与えられる。</p><pre><var>A</var> <var>B</var>
</pre></section></div>
</div>
<hr>
<div class="part">
<section>
<h3>入力例 1</h3><pre>5 4
</pre>
</section>
</div>
<div class="part"><section><h3>出力例 1</h3><pre>Yay!
</pre><p>説明の中の例：</p><pre>not a sample</pre></section></div>
<div class="part"><section><h3>入力例 2</h3><pre>8 8</pre></section></div>
<div class="part"><section><h3>出力例 2</h3><pre>Yay!</pre></section></div>
</span>
<span class="lang-en"><p>Score : <var>100</var> points</p>
<div class="part"><section><h3>Sample Input 1</h3><pre>5 4</pre></section></div></span>
</span></div>
</body></html>"#;

    #[test]
    fn parses_title_limits_statement_and_samples() {
        let problem = parse_problem(PAGE).unwrap();
        assert_eq!(problem.title, "A - Happy Birthday!");
        assert_eq!(problem.time_limit, "2 sec");
        assert_eq!(problem.memory_limit, "1024 MiB");
        assert!(problem.statement_html.contains("問題文"));
        assert!(problem.statement_html.contains("<var>A</var>"));
        // 英語の問題文は含めない
        assert!(!problem.statement_html.contains("Score"));
        assert_eq!(
            problem.samples,
            vec![
                Sample {
                    input: "5 4\n".to_string(),
                    output: "Yay!\n".to_string()
                },
                Sample {
                    input: "8 8\n".to_string(),
                    output: "Yay!\n".to_string()
                },
            ]
        );
    }

    #[test]
    fn reads_limits_on_english_page() {
        let english = PAGE.replace(
            "実行時間制限: 2 sec / メモリ制限: 1024 MiB",
            "Time Limit: 2 sec / Memory Limit: 1024 MiB",
        );
        let problem = parse_problem(&english).unwrap();
        assert_eq!(problem.time_limit, "2 sec");
        assert_eq!(problem.memory_limit, "1024 MiB");
    }

    #[test]
    fn reports_pages_with_other_structure() {
        assert!(parse_problem("<html><body><p>404 Not Found</p></body></html>").is_err());
        let without_statement = PAGE.replace("task-statement", "other");
        assert!(parse_problem(&without_statement).is_err());
        let odd_samples = PAGE.replace(
            r#"<div class="part"><section><h3>出力例 2</h3><pre>Yay!</pre></section></div>"#,
            "",
        );
        assert!(parse_problem(&odd_samples).is_err());
        let empty_sample = PAGE.replace("<h3>入力例 2</h3><pre>8 8</pre>", "<h3>入力例 2</h3>");
        // 中身のない区画は、その見出しを理由に出す
        assert!(
            parse_problem(&empty_sample)
                .unwrap_err()
                .contains("「入力例 2」")
        );
    }

    // D-155：パッケージのフォルダから問題を決める
    #[test]
    fn decides_the_problem_from_the_package_folder() {
        let dir = std::env::temp_dir().join(format!(
            "atcoder-studio-package-problem-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let id = |contest: &str, task: &str| Some(ProblemId::new(contest, task).unwrap());
        // 一覧がないとき：JOI は名前で比べて親のフォルダ名、ほかは今までの決まり
        assert_eq!(
            package_problem(&dir, "joi2027yo1b/joi2027_yo1b_a"),
            id("joi2027yo1b", "joi2027_yo1b_a")
        );
        assert_eq!(
            package_problem(&dir, "abc480/abc480_a"),
            id("abc480", "abc480_a")
        );
        assert_eq!(
            package_problem(&dir, "problems/abc001_a"),
            id("abc001", "abc001_a")
        );
        assert_eq!(package_problem(&dir, "abc001_a"), id("abc001", "abc001_a"));
        assert_eq!(
            package_problem(&dir, "abc042/arc058_a"),
            id("arc058", "arc058_a")
        );
        // 保存した一覧にあれば、親のフォルダ名（問題 ID に別のコンテストの名前が付く形）
        std::fs::create_dir_all(dir.join("task-lists")).unwrap();
        std::fs::write(
            dir.join("task-lists/abc042.json"),
            r#"[{"task":"arc058_a","label":"C","title":"x"}]"#,
        )
        .unwrap();
        assert_eq!(
            package_problem(&dir, "abc042/arc058_a"),
            id("abc042", "arc058_a")
        );
        // 一覧にない問題は、一覧からは決めない
        assert_eq!(
            package_problem(&dir, "abc042/arc058_b"),
            id("arc058", "arc058_b")
        );
        // 決まらないもの
        for package in [
            "",
            "perf-output",
            "solution",
            "x/abc001_",
            "x/_a",
            "X/abc001_a/Y",
        ] {
            assert_eq!(package_problem(&dir, package), None, "{package}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn accepts_only_simple_ids() {
        assert_eq!(
            ProblemId::new("abc430", "abc430_a").unwrap().url(),
            "https://atcoder.jp/contests/abc430/tasks/abc430_a"
        );
        assert!(ProblemId::new("abc430", "../x").is_err());
        assert!(ProblemId::new("ABC430", "abc430_a").is_err());
        assert!(ProblemId::new("", "abc430_a").is_err());
    }

    #[test]
    fn uses_saved_problem_without_fetching() {
        let dir =
            std::env::temp_dir().join(format!("atcoder-studio-problem-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let id = ProblemId::new("abc100", "abc100_a").unwrap();
        let problem = parse_problem(PAGE).unwrap();
        storage::write_atomic(
            &cache_path(&dir, &id),
            &serde_json::to_vec(&problem).unwrap(),
        )
        .unwrap();
        // 保存してあるので、取得せずに返す（テストでは AtCoder につながない）
        assert_eq!(load_problem(&dir, &id, false).unwrap(), problem);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_task_list() {
        let html = r#"<table><thead><tr><th>問題</th></tr></thead><tbody>
<tr><td class="text-center no-break"><a href="/contests/abc475/tasks/abc475_a">A</a></td><td><a href="/contests/abc475/tasks/abc475_a">mnclr</a></td><td>2 sec</td></tr>
<tr><td><a href="/contests/abc475/tasks/abc475_b">B</a></td><td><a href="/contests/abc475/tasks/abc475_b">Change</a></td><td>2 sec</td></tr>
<tr><td><a href="https://example.com/x">C</a></td><td>外のリンク</td></tr>
</tbody></table>"#;
        assert_eq!(
            parse_task_list(html, "abc475").unwrap(),
            vec![
                TaskEntry {
                    task: "abc475_a".to_string(),
                    label: "A".to_string(),
                    title: "mnclr".to_string()
                },
                TaskEntry {
                    task: "abc475_b".to_string(),
                    label: "B".to_string(),
                    title: "Change".to_string()
                },
            ]
        );
        assert!(parse_task_list("<p>404</p>", "abc475").is_err());
    }

    #[test]
    fn remembers_choice_per_package() {
        let dir =
            std::env::temp_dir().join(format!("atcoder-studio-choice-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let package = Path::new("/ws/A");
        assert_eq!(choice_for(&dir, package), None);
        let id = ProblemId::new("abc011", "abc011_a").unwrap();
        set_choice(&dir, package, id.clone()).unwrap();
        assert_eq!(choice_for(&dir, package), Some(id));
        assert_eq!(choice_for(&dir, Path::new("/ws/B")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_samples_without_overwriting() {
        let dir =
            std::env::temp_dir().join(format!("atcoder-studio-samples-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = dir.join("test");
        let samples = vec![
            Sample {
                input: "1\n".to_string(),
                output: "2\n".to_string(),
            },
            Sample {
                input: "3\n".to_string(),
                output: "4\n".to_string(),
            },
        ];
        let first = save_samples(&folder, &samples).unwrap();
        assert_eq!(first.written.len(), 4);
        assert_eq!(
            std::fs::read_to_string(folder.join("sample-2.out")).unwrap(),
            "4\n"
        );
        // 利用者が書き換えたファイルは上書きしない
        std::fs::write(folder.join("sample-1.out"), "edited\n").unwrap();
        let second = save_samples(&folder, &samples).unwrap();
        assert_eq!(second.written, Vec::<String>::new());
        assert_eq!(second.skipped, vec!["sample-1.out".to_string()]);
        assert_eq!(second.unchanged.len(), 3);
        assert_eq!(
            std::fs::read_to_string(folder.join("sample-1.out")).unwrap(),
            "edited\n"
        );
        assert!(second.leftover.is_empty());
        // サンプルが減ったら（「再取得」など）、残った番号のファイルを知らせる。消さない
        std::fs::write(folder.join("sample-3.in"), "5\n").unwrap();
        std::fs::write(folder.join("sample-10.out"), "6\n").unwrap();
        let third = save_samples(&folder, &samples).unwrap();
        assert_eq!(
            third.leftover,
            vec!["sample-3.in".to_string(), "sample-10.out".to_string()]
        );
        assert!(folder.join("sample-3.in").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn waits_between_fetches() {
        wait_for_interval();
        let started = Instant::now();
        wait_for_interval();
        assert!(started.elapsed() >= Duration::from_millis(900));
    }

    #[test]
    fn adds_pasted_cases_after_the_largest_sample() {
        let folder =
            std::env::temp_dir().join(format!("atcoder-studio-add-case-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        // フォルダがなければ作って sample-1
        assert_eq!(add_case(&folder, "1 2", "3").unwrap(), "sample-1");
        assert_eq!(
            std::fs::read_to_string(folder.join("sample-1.in")).unwrap(),
            "1 2\n"
        );
        assert_eq!(
            std::fs::read_to_string(folder.join("sample-1.out")).unwrap(),
            "3\n"
        );
        // 間が空いていても、いちばん大きい番号の次。random-N・名前の違うものは数えない
        std::fs::write(folder.join("sample-7.out"), "x\n").unwrap();
        std::fs::write(folder.join("random-20.in"), "x\n").unwrap();
        std::fs::write(folder.join("sample-x.in"), "x\n").unwrap();
        std::fs::write(folder.join("sample-+9.in"), "x\n").unwrap();
        assert_eq!(next_sample_number(&folder), 8);
        // 改行は LF にそろえ、最後の改行は 1 つ（もとからあれば足さない）。期待出力が空なら .out は作らない
        assert_eq!(add_case(&folder, "a\r\nb\rc\r\n", "").unwrap(), "sample-8");
        assert_eq!(
            std::fs::read_to_string(folder.join("sample-8.in")).unwrap(),
            "a\nb\nc\n"
        );
        assert!(!folder.join("sample-8.out").exists());
        // 入力が空・大きすぎるものは足さない
        assert!(add_case(&folder, " \n", "1").is_err());
        assert!(add_case(&folder, &"1".repeat(MAX_CASE_BYTES + 1), "").is_err());
        assert_eq!(next_sample_number(&folder), 9);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn writes_the_editorial_url() {
        let id = ProblemId::new("abc481", "abc481_d").unwrap();
        assert_eq!(
            editorial_url(&id),
            "https://atcoder.jp/contests/abc481/tasks/abc481_d/editorial?lang=ja"
        );
    }

    // 古い ABC の 3 通りの作り（D-256。2026-11-12 に公開ページを 1 回ずつ読んで確かめた形を、短く写したもの）。
    // どれも lang-ja がない。abc001 は見出しが section の外（div.part > h3 と section が並ぶ）で、「問題文」の section が閉じていない
    fn old_page(statement: &str) -> String {
        format!(
            r#"<html><body>
<span class="h2">
    A - 古い問題
    <a class="btn btn-default btn-sm" href="/contests/abc001/tasks/abc001_1/editorial">解説</a>
</span>
<p>実行時間制限: 2 sec / メモリ制限: 64 MiB</p>
<div id="task-statement">{statement}</div>
</body></html>"#
        )
    }

    const OLD_ABC001: &str = r#"
<div class="part">
<h4>注意</h4>
<p><b>この問題は古い問題です。</b></p>
<h3>問題文</h3>
<section>
積雪深 <var>H_1</var> と <var>H_2</var> の差を出力してください。<br />
</div>
<hr />
<div class="io-style">
<div class="part">
<h3>入力</h3>
<section>
入力は以下の形式で標準入力から与えられる。
<pre>
<var>H_1</var>
<var>H_2</var>
</pre>
</section>
</div>
<div class="part">
<h3>出力</h3>
<section>
差を出力せよ。絶対誤差が <var>10^{-6}</var> 以下であれば正解とする。
</section>
</div>
</div>
<hr />
<div class="part">
<h3>入力例 1</h3>
<section>
<pre class="prettyprint linenums">
15
10
</pre>
</section>
</div>
<div class="part">
<h3>出力例 1</h3>
<section>
<pre class="prettyprint linenums">
5
</pre>
<ul><li><var>15-10</var> <var>=5</var></li></ul>
</section>
</div>
<div class="part">
<h3>入力例 2</h3>
<section>
<pre class="prettyprint linenums">
0
0
</pre>
</section>
</div>
<div class="part">
<h3>出力例 2</h3>
<section>
<pre class="prettyprint linenums">
0
</pre>
</section>
</div>"#;

    const OLD_ABC020: &str = r#"
<div id="task-statement">
  <div id="task-statement">
<div class="part">
<section>
<h3>問題文</h3>
<p>整数 <var>Q</var> が与えられます。</p>
</section>
</div>
<hr />
<div class="io-style">
<div class="part">
<section>
<h3>入力</h3>
<p>入力は以下の形式で標準入力から与えられる。</p>
<pre>
<var>Q</var>
</pre>
</section>
</div>
</div>
<hr />
<div class="part">
<section>
<h3>入力例1</h3>
<pre>
1
</pre>
</section>
</div>
<div class="part">
<section>
<h3>出力例1</h3>
<pre>
ABC
</pre>
</section>
</div>
  </div>
</div>"#;

    const OLD_ABC041: &str = r#"
<section>
<h3>問題文</h3>
<p>文字列 <var>s</var> と整数 <var>i</var> が与えられます。</p>
</section>
<hr />
<section>
<h3>制約</h3>
<ul><li><var>1 ≦ i ≦ |s| ≦ 100</var></li></ul>
</section>
<hr />
<section>
<h3>入力</h3>
<p>入力は以下の形式で標準入力から与えられる。</p>
<pre>
<var>s</var>
<var>i</var>
</pre>
</section>
<section>
<h3>入力例1</h3>
<pre>
atcoder
3
</pre>
</section>
<section>
<h3>出力例1</h3>
<pre>
c
</pre>
</section>"#;

    fn samples(pairs: &[(&str, &str)]) -> Vec<Sample> {
        pairs
            .iter()
            .map(|(input, output)| Sample {
                input: input.to_string(),
                output: output.to_string(),
            })
            .collect()
    }

    #[test]
    fn parses_old_abc_pages_without_lang_ja() {
        let problem = parse_problem(&old_page(OLD_ABC001)).unwrap();
        assert_eq!(problem.title, "A - 古い問題");
        assert_eq!(problem.time_limit, "2 sec");
        assert_eq!(problem.memory_limit, "64 MiB");
        assert!(problem.statement_html.contains("積雪深"));
        assert_eq!(
            problem.samples,
            samples(&[("15\n10\n", "5\n"), ("0\n0\n", "0\n")])
        );

        // #task-statement の入れ子は、外側の 1 つだけ（問題文が重ならない）
        let problem = parse_problem(&old_page(OLD_ABC020)).unwrap();
        assert_eq!(problem.statement_html.matches("問題文").count(), 1);
        assert_eq!(problem.samples, samples(&[("1\n", "ABC\n")]));

        let problem = parse_problem(&old_page(OLD_ABC041)).unwrap();
        assert_eq!(problem.samples, samples(&[("atcoder\n3\n", "c\n")]));
    }

    #[test]
    fn finds_sections_of_every_shape() {
        for (page, headings) in [
            (
                OLD_ABC001,
                &[
                    "問題文",
                    "入力",
                    "出力",
                    "入力例 1",
                    "出力例 1",
                    "入力例 2",
                    "出力例 2",
                ][..],
            ),
            (OLD_ABC020, &["問題文", "入力", "入力例1", "出力例1"][..]),
            (
                OLD_ABC041,
                &["問題文", "制約", "入力", "入力例1", "出力例1"][..],
            ),
        ] {
            let document = Document::from(page);
            let found: Vec<String> = sections(&document.select("body"))
                .into_iter()
                .map(|(heading, _)| heading)
                .collect();
            assert_eq!(found, headings);
        }
        // 同じ親に h3 が並ぶときは、区画にしない
        let document =
            Document::from("<div><h3>入力例 1</h3><pre>1</pre><h3>出力例 1</h3><pre>1</pre></div>");
        assert!(sections(&document.select("body")).is_empty());
    }

    #[test]
    fn reads_tolerance_from_old_pages() {
        let problem = parse_problem(&old_page(OLD_ABC001)).unwrap();
        assert_eq!(
            output_tolerance(&problem.statement_html),
            Some(Tolerance {
                abs: true,
                rel: false,
                exponent: 6
            })
        );
    }

    // 印刷用のページ（tasks_print）の作りをまねた例（D-262。2026-11-13 に abc430・abc001・joisc2007 のページで観察した形）。
    // 問題ごとに div.col-sm-12 があり、その中が問題ページと同じ作り。区画の間に空の div.next-page がある
    const PRINT_PAGE: &str = r#"<html><body><div id="main-container" class="container">
<script>var PRINT = true;</script>
<div class="row">
<div class="col-sm-12">
<span class="h2">A - Happy Birthday!</span>
<hr />
<p>実行時間制限: 2 sec / メモリ制限: 1024 MiB</p>
<div id="task-statement"><span class="lang"><span class="lang-ja">
<div class="part"><section><h3>問題文</h3><p><var>A</var> 個と <var>B</var> 個</p></section></div>
<div class="part"><section><h3>入力例 1</h3><pre>5 4
</pre></section></div>
<div class="part"><section><h3>出力例 1</h3><pre>Yay!
</pre></section></div>
</span><span class="lang-en"><p>Score</p></span></span></div>
</div>
<div class="next-page col-sm-12"></div>
<div class="col-sm-12">
<span class="h2">B - 視程の通報</span>
<hr />
<p>実行時間制限: 3 sec / メモリ制限: 256 MiB</p>
<div id="task-statement">
<div class="part"><section><h3>問題文</h3><p>古い ABC の形（lang-ja がない）</p></section></div>
<div class="part"><section><h3>入力例 1</h3><pre>15000</pre></section></div>
<div class="part"><section><h3>出力例 1</h3><pre>65</pre></section></div>
</div>
</div>
<div class="next-page col-sm-12"></div>
<div class="col-sm-12">
<span class="h2">packing10 - 箱詰め (Packing)</span>
<hr />
<p>実行時間制限: 1 sec / メモリ制限: 256 MiB</p>
<div id="task-statement"><span class="lang"><span class="lang-ja">
<div class="part"><section><h3>問題文</h3><p>記号が英単語の問題</p></section></div>
</span></span></div>
</div>
<div class="next-page col-sm-12"></div>
<div class="col-sm-12">
<span class="h2">C - 一覧にない問題</span>
<hr />
<p>実行時間制限: 2 sec / メモリ制限: 1024 MiB</p>
<div id="task-statement"><span class="lang"><span class="lang-ja"><p>x</p></span></span></div>
</div>
<div class="next-page col-sm-12"></div>
<div class="col-sm-12">
<span class="h2">D - 制限の行がない</span>
<hr />
<div id="task-statement"><span class="lang"><span class="lang-ja"><p>x</p></span></span></div>
</div>
<div class="next-page col-sm-12"></div>
<div class="col-sm-12"><span class="h2">E - 同じ記号 1</span><hr /><p>実行時間制限: 2 sec / メモリ制限: 1024 MiB</p><div id="task-statement"><p>x</p></div></div>
<div class="col-sm-12"><span class="h2">E - 同じ記号 2</span><hr /><p>実行時間制限: 2 sec / メモリ制限: 1024 MiB</p><div id="task-statement"><p>y</p></div></div>
</div></div></body></html>"#;

    fn entry(task: &str, label: &str) -> TaskEntry {
        TaskEntry {
            task: task.to_string(),
            label: label.to_string(),
            title: String::new(),
        }
    }

    #[test]
    fn reads_problems_from_the_print_page() {
        let tasks = [
            entry("abc100_a", "A"),
            entry("abc001_2", "B"),
            entry("joisc2007_packin10", "packing10"),
            entry("abc100_d", "D"),
            entry("abc100_e", "E"),
            entry("abc100_f", "F"),
        ];
        let problems = parse_print_page(PRINT_PAGE, &tasks);
        // 一覧にない C・読めない D・ページに同じ記号が 2 つある E・ページにない F は使わない
        let ids: Vec<&str> = problems.iter().map(|(task, _)| task.as_str()).collect();
        assert_eq!(ids, ["abc100_a", "abc001_2", "joisc2007_packin10"]);
        // 問題ページから読んだときと同じ中身（問題名・制限・問題文・サンプル）
        let (_, a) = &problems[0];
        let page = parse_problem(&format!(
            "<html><body>{}</body></html>",
            Document::from(PRINT_PAGE)
                .select("div.col-sm-12")
                .first()
                .html()
        ))
        .unwrap();
        assert_eq!(a, &page);
        assert_eq!(a.title, "A - Happy Birthday!");
        assert_eq!(
            (a.time_limit.as_str(), a.memory_limit.as_str()),
            ("2 sec", "1024 MiB")
        );
        assert!(a.statement_html.contains("問題文") && !a.statement_html.contains("Score"));
        assert_eq!(a.samples.len(), 1);
        assert_eq!(a.samples[0].output, "Yay!\n");
        // 古い ABC の形・英単語の記号も読める。区画ごとに読むので、ほかの問題の中身は入らない
        let (_, b) = &problems[1];
        assert_eq!(
            (b.time_limit.as_str(), b.memory_limit.as_str()),
            ("3 sec", "256 MiB")
        );
        assert!(b.statement_html.contains("古い ABC") && !b.statement_html.contains("Happy"));
        assert_eq!(b.samples[0].input, "15000\n");
        assert_eq!(problems[2].1.title, "packing10 - 箱詰め (Packing)");
        assert!(problems[2].1.samples.is_empty());
        // 一覧に同じ記号が 2 つあれば使わない
        let doubled = [entry("abc100_a", "A"), entry("abc100_a2", "A")];
        assert!(parse_print_page(PRINT_PAGE, &doubled).is_empty());
        // 問題の区画がないページ
        assert!(parse_print_page("<html><body><p>x</p></body></html>", &tasks).is_empty());
    }

    #[test]
    fn saves_only_unsaved_problems_from_the_print_page() {
        let dir = std::env::temp_dir().join(format!("atcoder-studio-print-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let saved = ProblemId::new("abc100", "abc100_a").unwrap();
        let unsaved = ProblemId::new("abc100", "abc100_b").unwrap();
        let old = parse_problem(PAGE).unwrap();
        storage::write_atomic(
            &cache_path(&dir, &saved),
            &serde_json::to_vec(&old).unwrap(),
        )
        .unwrap();
        // 保存していない問題が 1 問だけなら、印刷用のページは読まない（テストでは AtCoder につながない）
        let tasks = [entry("abc100_a", "A"), entry("abc100_b", "B")];
        assert_eq!(prefetch_contest(&dir, "abc100", &tasks), 0);
        assert!(!cache_path(&dir, &unsaved).exists());
        // 形の正しくないコンテスト ID でも読まない
        assert_eq!(
            prefetch_contest(&dir, "../x", &[entry("a", "A"), entry("b", "B")]),
            0
        );
        // 取り出したもののうち、保存していなかったものだけを書く（保存してあったものは変えない）
        let printed = parse_print_page(
            PRINT_PAGE,
            &[entry("abc100_a", "A"), entry("abc100_b", "B")],
        );
        assert_eq!(printed.len(), 2);
        assert_eq!(
            save_printed(&dir, std::slice::from_ref(&unsaved), printed),
            1
        );
        assert_eq!(load_problem(&dir, &saved, false).unwrap(), old);
        assert_eq!(
            load_problem(&dir, &unsaved, false).unwrap().time_limit,
            "3 sec"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
