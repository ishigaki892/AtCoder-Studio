// 開始と同時に「コンテストを作る」の予約（Phase 43。D-267）
//
// 開始前のコンテストを予約すると、開始時刻を Mac の時計で待ち、開始の 1 秒後に問題の一覧を読む。読めなければ
// RETRY_WAITS の間をあけて読み直す（5 回で止める）。読めたら「コンテストを作る」と同じに作る（commands.rs の
// reserve_contest）。予約は 1 つだけで、保存しない（アプリを閉じたら消える）。
// 待つのは Rust 側のスレッド（ウィンドウが隠れている間に、画面側のタイマーが間引かれることがあるため。D-239 と同じ）
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// 開始から作り始めるまで（ミリ秒。ユーザーが決めた。2026-11-14）
pub const START_DELAY_MS: i64 = 1000;
// 開始までこれより先のコンテストは予約しない（入れ間違いを防ぐ）
pub const MAX_AHEAD_SECS: i64 = 24 * 3600;
// 問題の一覧を読めなかったときに、次に読むまでの間（秒）。初めの 1 回と合わせて 5 回読む
pub const RETRY_WAITS: [u64; 4] = [5, 10, 20, 40];
// 読み直しても問題の一覧を読めないときに、開催期間を読み直して待ち直す回数の上限（開始が遅れたとき。B-4）
pub const MAX_PERIOD_REREADS: usize = 2;
// 待つスレッドが起きて確かめる間隔の上限（取り消しに早く気づく。眠っていた間も、時計で確かめ直す）
const STEP: Duration = Duration::from_millis(500);

// 今の予約。取り消すたびに generation を進め、前の予約を待つスレッドは止まる。
// token は画面が予約ごとに作る番号。始める前（開催期間を読んでいる間）に取り消された番号は cancelled に覚え、
// その予約は始めない（docs/bug-review-2026-11-14.md の B-2）
struct State {
    contest: Option<String>,
    token: u64,
    generation: u64,
    // 開始して、作っている途中
    creating: bool,
    cancelled: Vec<u64>,
}

// 覚えておく、始める前に取り消された番号の数
const MAX_CANCELLED: usize = 16;

static STATE: Mutex<State> = Mutex::new(State {
    contest: None,
    token: 0,
    generation: 0,
    creating: false,
    cancelled: Vec::new(),
});

fn state() -> std::sync::MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

// 予約できるか（start・end は UNIX 時間の秒、now はミリ秒）
pub fn check(start: i64, end: i64, now: i64) -> Result<(), String> {
    if now >= end * 1000 {
        return Err("このコンテストは終わっています".to_string());
    }
    if now >= start * 1000 {
        return Err("このコンテストは始まっています".to_string());
    }
    if start * 1000 - now > MAX_AHEAD_SECS * 1000 {
        return Err("開始まで 24 時間より先のコンテストは予約できません".to_string());
    }
    Ok(())
}

// 予約を始める（1 つだけ）。予約の世代を返す。始める前に取り消された番号なら None
pub fn begin(contest: &str, token: u64) -> Result<Option<u64>, String> {
    let mut state = state();
    if let Some(at) = state
        .cancelled
        .iter()
        .position(|cancelled| *cancelled == token)
    {
        state.cancelled.remove(at);
        return Ok(None);
    }
    if let Some(reserved) = &state.contest {
        return Err(format!(
            "{reserved} を予約しています。予約は 1 つだけです。取り消してから予約してください"
        ));
    }
    state.contest = Some(contest.to_string());
    state.token = token;
    state.generation += 1;
    state.creating = false;
    Ok(Some(state.generation))
}

// 予約がまだ生きているか
pub fn active(generation: u64) -> bool {
    let state = state();
    state.generation == generation && state.contest.is_some()
}

// 開始して作り始めた（このあとの取り消しは、作るのを止める）
pub fn set_creating(generation: u64) {
    let mut state = state();
    if state.generation == generation {
        state.creating = true;
    }
}

// 番号 token の予約を取り消す。その予約がまだ始まっていなければ、番号を覚えておく（始めない）。
// 取り消した予約がなければ None、あれば作っている途中か
pub fn cancel(token: u64) -> Option<bool> {
    let mut state = state();
    if state.contest.is_none() || state.token != token {
        if !state.cancelled.contains(&token) {
            state.cancelled.push(token);
            if state.cancelled.len() > MAX_CANCELLED {
                state.cancelled.remove(0);
            }
        }
        return None;
    }
    let creating = state.creating;
    state.contest = None;
    state.generation += 1;
    state.creating = false;
    Some(creating)
}

// 予約を終えた（作り終えた・読めなかった）ときに、予約を消す。取り消したあとなら何もしない
pub struct Finish(pub u64);

impl Drop for Finish {
    fn drop(&mut self) {
        let mut state = state();
        if state.generation == self.0 {
            state.contest = None;
            state.creating = false;
        }
    }
}

// target（ミリ秒）まで待つ。取り消したら false
pub fn wait_until(
    target: i64,
    cancelled: &dyn Fn() -> bool,
    now: &dyn Fn() -> i64,
    sleep: &dyn Fn(Duration),
) -> bool {
    loop {
        if cancelled() {
            return false;
        }
        let left = target - now();
        if left <= 0 {
            return true;
        }
        sleep(Duration::from_millis(left as u64).min(STEP));
    }
}

// 問題の一覧を読む。読めなければ RETRY_WAITS の間をあけて読み直す（on_retry で、何回目か・待つ秒数・理由を知らせる）。
// wait は、その間だけ待って、取り消されたら false を返す。取り消されたら Ok(None)
pub fn list_with_retries<T>(
    list: &dyn Fn() -> Result<T, String>,
    wait: &dyn Fn(Duration) -> bool,
    on_retry: &dyn Fn(usize, u64, &str),
) -> Result<Option<T>, String> {
    let mut attempt = 0;
    loop {
        let error = match list() {
            Ok(tasks) => return Ok(Some(tasks)),
            Err(error) => error,
        };
        let Some(&seconds) = RETRY_WAITS.get(attempt) else {
            return Err(format!(
                "問題の一覧を {} 回読みましたが、読めませんでした（{error}）。もう一度「作る」を押してください",
                RETRY_WAITS.len() + 1
            ));
        };
        attempt += 1;
        on_retry(attempt, seconds, &error);
        if !wait(Duration::from_secs(seconds)) {
            return Ok(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[test]
    fn checks_the_start_time() {
        let start = 100_000;
        let end = start + 6000;
        // 開始の 1 分前・24 時間ちょうど前は予約できる
        assert!(check(start, end, (start - 60) * 1000).is_ok());
        assert!(check(start, end, (start - MAX_AHEAD_SECS) * 1000).is_ok());
        // 24 時間より先・始まっている・終わっている
        assert!(
            check(start, end, (start - MAX_AHEAD_SECS) * 1000 - 1)
                .unwrap_err()
                .contains("24 時間")
        );
        assert!(
            check(start, end, start * 1000)
                .unwrap_err()
                .contains("始まって")
        );
        assert!(
            check(start, end, end * 1000)
                .unwrap_err()
                .contains("終わって")
        );
    }

    #[test]
    fn reserves_only_one_and_cancels() {
        // 予約の状態はアプリで 1 つなので、このテストの中だけで確かめる
        let first = begin("abc480", 1).unwrap().unwrap();
        assert!(active(first));
        assert!(
            begin("abc481", 2)
                .unwrap_err()
                .contains("abc480 を予約しています")
        );
        // 別の番号では取り消さない（その番号を覚えるだけ）
        assert_eq!(cancel(99), None);
        assert!(active(first));
        // 待っている間の取り消し
        assert_eq!(cancel(1), Some(false));
        assert!(!active(first));
        // 作っている途中の取り消し
        let second = begin("abc481", 3).unwrap().unwrap();
        set_creating(second);
        assert_eq!(cancel(3), Some(true));
        // 終えたら予約は消える。取り消したあとの終わりは、次の予約を消さない
        let third = begin("abc482", 4).unwrap().unwrap();
        drop(Finish(second));
        assert!(active(third));
        drop(Finish(third));
        assert!(!active(third));
        // 始める前に取り消された番号の予約は始めない（B-2）。次の番号はふつうに始まる
        assert_eq!(cancel(5), None);
        assert_eq!(begin("abc483", 5).unwrap(), None);
        let sixth = begin("abc483", 6).unwrap().unwrap();
        assert_eq!(cancel(6), Some(false));
        assert!(!active(sixth));
    }

    #[test]
    fn waits_until_the_target_and_stops_when_cancelled() {
        let now = Cell::new(0_i64);
        let slept = RefCell::new(Vec::new());
        let sleep = |d: Duration| {
            slept.borrow_mut().push(d.as_millis());
            now.set(now.get() + d.as_millis() as i64);
        };
        assert!(wait_until(1200, &|| false, &|| now.get(), &sleep));
        assert_eq!(*slept.borrow(), [500, 500, 200]);
        // 取り消したら、すぐやめる
        assert!(!wait_until(
            5000,
            &|| now.get() >= 2000,
            &|| now.get(),
            &sleep
        ));
        assert_eq!(now.get(), 2200);
        // 時刻を過ぎていたら（スリープから起きたなど）、待たない
        slept.borrow_mut().clear();
        assert!(wait_until(100, &|| false, &|| now.get(), &sleep));
        assert!(slept.borrow().is_empty());
    }

    #[test]
    fn rereads_the_list_four_times_with_longer_waits() {
        // 3 回目で読めた
        let calls = Cell::new(0);
        let waits = RefCell::new(Vec::new());
        let retries = RefCell::new(Vec::new());
        let result = list_with_retries(
            &|| {
                calls.set(calls.get() + 1);
                if calls.get() < 3 {
                    Err("404".to_string())
                } else {
                    Ok(7)
                }
            },
            &|d| {
                waits.borrow_mut().push(d.as_secs());
                true
            },
            &|attempt, seconds, error| {
                retries
                    .borrow_mut()
                    .push((attempt, seconds, error.to_string()))
            },
        );
        assert_eq!(result, Ok(Some(7)));
        assert_eq!(*waits.borrow(), [5, 10]);
        assert_eq!(retries.borrow()[1], (2, 10, "404".to_string()));
        // 5 回読んでも読めない
        calls.set(0);
        waits.borrow_mut().clear();
        let result = list_with_retries::<u8>(
            &|| {
                calls.set(calls.get() + 1);
                Err("404".to_string())
            },
            &|d| {
                waits.borrow_mut().push(d.as_secs());
                true
            },
            &|_, _, _| {},
        );
        assert_eq!(calls.get(), 5);
        assert_eq!(*waits.borrow(), [5, 10, 20, 40]);
        assert!(result.unwrap_err().contains("5 回読みましたが"));
        // 待っている間に取り消した
        calls.set(0);
        let result = list_with_retries::<u8>(
            &|| {
                calls.set(calls.get() + 1);
                Err("404".to_string())
            },
            &|_| false,
            &|_, _, _| {},
        );
        assert_eq!(result, Ok(None));
        assert_eq!(calls.get(), 1);
    }
}
