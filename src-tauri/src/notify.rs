// 通知（Phase 22 作業 2。D-235・D-239）。`tauri-plugin-notification` を Rust 側だけで使う（WebView には権限を与えない）。
// 知らせるのは、IDE から出した提出の判定が決まったときと、残り時間を出しているコンテストが始まったとき。
// どちらも settings.json の notify でオンのときだけ、IDE の窓が手前にないときだけ、同じものは 1 回だけ。
// macOS では、利用者が通知を許可したかを IDE からは見分けられない（プラグインはいつも許可と返す）
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_notification::NotificationExt;

use crate::storage::{self, NotifySettings};
use crate::submit::{SubmissionState, SubmissionView};

// 始まってからこれより後に気づいたとき（Mac が眠っていたときなど）は、開始の通知を出さない
const START_GRACE_SECS: i64 = 60;
// 開始を待つスレッドが起きて確かめる間隔の上限（眠っていた間も、時計で確かめ直す）
const WATCH_STEP: Duration = Duration::from_secs(30);

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0)
}

fn settings<R: Runtime>(app: &AppHandle<R>) -> NotifySettings {
    app.path()
        .app_config_dir()
        .map(|dir| storage::notify_settings(&dir))
        .unwrap_or_default()
}

// IDE の窓が手前にある（分からないときは、手前にないとする）
fn focused<R: Runtime>(app: &AppHandle<R>) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
}

// 同じ鍵では 1 回だけ true（アプリを開いている間）
fn first_time(key: String) -> bool {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    SEEN.get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .map(|mut seen| seen.insert(key))
        .unwrap_or(false)
}

// 通知を出す。macOS に届いたと分かるまで数秒待つことがあるので、別のスレッドで出す
fn show<R: Runtime>(app: &AppHandle<R>, title: String, body: String) {
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = app.notification().builder().title(title).body(body).show();
    });
}

// ---- 判定 ----

// 判定が決まった提出の、通知の題と本文（例：「AC：abc480_c」「C - 問題名・123 ms・3600 KiB」）
pub fn judge_text(view: &SubmissionView) -> Option<(String, String)> {
    if view.state != SubmissionState::Done {
        return None;
    }
    let judge = view.judge.as_ref()?;
    let mut body = view.task_title.clone();
    if let Some(score) = &judge.score {
        body.push_str(&format!("・{score} 点"));
    }
    if let Some(ms) = judge.time_ms {
        body.push_str(&format!("・{ms} ms"));
    }
    if let Some(kib) = judge.memory_kib {
        body.push_str(&format!("・{kib} KiB"));
    }
    Some((format!("{}：{}", judge.label, view.task), body))
}

// 判定を追う処理（submit::spawn_tracker）から、読むたびに呼ぶ。決まったときだけ知らせる
pub fn judge_done<R: Runtime>(app: &AppHandle<R>, view: &SubmissionView) {
    let Some((title, body)) = judge_text(view) else {
        return;
    };
    let key = format!(
        "judge:{}",
        view.submission_id
            .map_or(view.key.to_string(), |id| id.to_string())
    );
    if settings(app).judge && first_time(key) && !focused(app) {
        show(app, title, body);
    }
}

// ---- コンテストの開始 ----

// 今、残り時間を出しているコンテスト。変わるたびに generation を進め、前のコンテストを待つスレッドは止まる
#[derive(Default)]
struct Watch {
    contest: Option<String>,
    generation: u64,
}

fn watch() -> &'static Mutex<Watch> {
    static WATCH: OnceLock<Mutex<Watch>> = OnceLock::new();
    WATCH.get_or_init(|| Mutex::new(Watch::default()))
}

fn still_watching(contest: &str, generation: u64) -> bool {
    watch().lock().is_ok_and(|watch| {
        watch.generation == generation && watch.contest.as_deref() == Some(contest)
    })
}

// 開始を待つスレッドが、次に何をするか
#[derive(Debug, PartialEq, Eq)]
pub enum StartStep {
    // これだけ待ってから確かめ直す
    Wait(Duration),
    // 始まった。知らせる
    Announce,
    // 始まってから時間がたちすぎた（眠っていたなど）。知らせずに終える
    Missed,
}

pub fn start_step(start: i64, now: i64) -> StartStep {
    if now < start {
        StartStep::Wait(Duration::from_secs((start - now) as u64).min(WATCH_STEP))
    } else if now - start <= START_GRACE_SECS {
        StartStep::Announce
    } else {
        StartStep::Missed
    }
}

fn duration_text(seconds: i64) -> String {
    let minutes = seconds.max(0) / 60;
    if minutes < 24 * 60 {
        format!("{minutes} 分")
    } else {
        let (days, hours) = (minutes / (24 * 60), minutes % (24 * 60) / 60);
        if hours == 0 {
            format!("{days} 日")
        } else {
            format!("{days} 日 {hours} 時間")
        }
    }
}

// 画面が、残り時間を出すコンテストを伝える（出していなければ None）。まだ始まっていなければ、開始まで待つ
// スレッドを立てる。開催期間は、残り時間のためにすでに読んだもの（覚えてあるもの）だけを使い、ここでは読まない
pub fn watch_contest_start<R: Runtime>(
    app: &AppHandle<R>,
    contest: Option<String>,
) -> Result<(), String> {
    if let Some(contest) = &contest {
        crate::problem::ProblemId::new(contest, "x")?;
    }
    let generation = {
        let mut watch = watch()
            .lock()
            .map_err(|_| "開始の通知を準備できません".to_string())?;
        watch.contest = contest.clone();
        watch.generation += 1;
        watch.generation
    };
    let Some(contest) = contest else {
        return Ok(());
    };
    let info = crate::standings::contest_info(&contest, || {
        Err("開催期間をまだ読んでいません".to_string())
    })?;
    if now_secs() >= info.start {
        return Ok(());
    }
    let app = app.clone();
    std::thread::spawn(move || {
        loop {
            if !still_watching(&contest, generation) {
                return;
            }
            match start_step(info.start, now_secs()) {
                StartStep::Wait(wait) => std::thread::sleep(wait),
                StartStep::Missed => return,
                StartStep::Announce => break,
            }
        }
        if settings(&app).contest_start && first_time(format!("start:{contest}")) && !focused(&app)
        {
            show(
                &app,
                format!("{contest} が始まりました"),
                format!("時間：{}", duration_text(info.end - info.start)),
            );
        }
    });
    Ok(())
}

// ---- テスト ----

// 設定の画面で通知をオンにして保存したとき（オフからオンに変わったものがあるとき）に 1 回出す。
// 初めてなら、このときに macOS が許可の確認を出す
pub fn turned_on(before: NotifySettings, after: NotifySettings) -> bool {
    (!before.judge && after.judge) || (!before.contest_start && after.contest_start)
}

pub fn show_test<R: Runtime>(app: &AppHandle<R>) {
    show(
        app,
        "AtCoder Studio の通知".to_string(),
        "通知のテストです。IDE の窓が手前にないときに知らせます".to_string(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::submit::Judge;

    fn view(state: SubmissionState, judge: Option<Judge>) -> SubmissionView {
        SubmissionView {
            key: 1,
            contest: "abc480".to_string(),
            task: "abc480_c".to_string(),
            task_title: "C - 問題名".to_string(),
            time_ms: 0,
            language: "Rust".to_string(),
            file: "src/bin/c.rs".to_string(),
            submission_id: Some(123),
            state,
            tracking: false,
            judge,
            message: None,
            detail_url: None,
            list_url: String::new(),
        }
    }

    fn judge(label: &str, time_ms: Option<u64>, memory_kib: Option<u64>) -> Judge {
        Judge {
            waiting: false,
            label: label.to_string(),
            title: None,
            score: Some("300".to_string()),
            time_ms,
            memory_kib,
        }
    }

    #[test]
    fn writes_the_judge_text_only_when_done() {
        assert_eq!(
            judge_text(&view(
                SubmissionState::Done,
                Some(judge("AC", Some(123), Some(3600)))
            )),
            Some((
                "AC：abc480_c".to_string(),
                "C - 問題名・300 点・123 ms・3600 KiB".to_string()
            ))
        );
        // 時間・メモリがない判定（CE など）
        assert_eq!(
            judge_text(&view(SubmissionState::Done, Some(judge("CE", None, None))))
                .unwrap()
                .1,
            "C - 問題名・300 点"
        );
        // 判定中・決まっていないときは知らせない
        assert_eq!(
            judge_text(&view(
                SubmissionState::Judging,
                Some(judge("WJ", None, None))
            )),
            None
        );
        assert_eq!(judge_text(&view(SubmissionState::Done, None)), None);
    }

    #[test]
    fn waits_for_the_start_and_skips_late_wakeups() {
        assert_eq!(
            start_step(1000, 900),
            StartStep::Wait(Duration::from_secs(30))
        );
        assert_eq!(
            start_step(1000, 990),
            StartStep::Wait(Duration::from_secs(10))
        );
        assert_eq!(start_step(1000, 1000), StartStep::Announce);
        assert_eq!(start_step(1000, 1060), StartStep::Announce);
        assert_eq!(start_step(1000, 1061), StartStep::Missed);
    }

    #[test]
    fn notifies_once_per_key() {
        assert!(first_time("test:once".to_string()));
        assert!(!first_time("test:once".to_string()));
        assert!(first_time("test:other".to_string()));
    }

    #[test]
    fn tests_only_when_turned_on() {
        let off = NotifySettings::default();
        let judge = NotifySettings {
            judge: true,
            contest_start: false,
        };
        let both = NotifySettings {
            judge: true,
            contest_start: true,
        };
        assert!(turned_on(off, judge));
        assert!(turned_on(judge, both));
        assert!(!turned_on(judge, judge));
        assert!(!turned_on(both, off));
    }

    #[test]
    fn writes_the_contest_length() {
        assert_eq!(duration_text(100 * 60), "100 分");
        assert_eq!(duration_text(4 * 3600), "240 分");
        assert_eq!(duration_text(10 * 24 * 3600), "10 日");
        assert_eq!(duration_text(24 * 3600 + 5 * 3600), "1 日 5 時間");
    }
}
