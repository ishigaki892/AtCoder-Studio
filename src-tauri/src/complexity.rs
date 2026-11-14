// 計算量の見当（Phase 45。D-270）
//
// 生成器に大きさの割合（2 つ目の引数。1/128〜1 の 8 段）を渡して大きな入力を作り、解答だけを各段 3 回動かして、
// 時間（真ん中の値）とメモリ（いちばん大きい値）を測る。割合と時間の両対数に直線を当てはめた傾き（指数 k）から
// 伸び方の見当を出し、最大の大きさ（割合 1）の時間を、compare.json の時間の制限と並べる。
// 1 回の時間が上限（時間の制限の 3 倍か 10 秒の小さいほう）を超えたら、それより大きい段は動かさない。
// 生成器が大きさの引数を読まない（割合 1 の入力が 1/128 の 4 倍より大きくならない）ときは、測らずに止める。
// 結果は保存しない（実行履歴にも入れない）
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;

use crate::compare::{self, TestSettings};
use crate::language::LangSettings;
use crate::process::{self, Limits, OUTPUT_LIMIT, Outcome};
use crate::random::{Helper, command, pipe_input, run_helper_with};
use crate::run::{Finish, Program, RunEvent, info, millis, prepare, send, shown};
use crate::toolchain::{self, Target};

// 大きさの割合（小さいほうから）
pub const SCALES: [f64; 8] = [
    1.0 / 128.0,
    1.0 / 64.0,
    1.0 / 32.0,
    1.0 / 16.0,
    1.0 / 8.0,
    1.0 / 4.0,
    1.0 / 2.0,
    1.0,
];
// 1 段で動かす回数（seed を変える）
pub const REPEATS: usize = 3;
// これより短い段は、起動の時間に埋もれるので、伸び方の当てはめに使わない（ミリ秒）
pub const MIN_FIT_MS: f64 = 50.0;
// 1 回の時間の上限（ミリ秒）。時間の制限の 3 倍と、この値の小さいほう
pub const MAX_STEP_MS: u64 = 10_000;
// 生成器の出力（入力）の大きさの上限
pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;

// 1 段の結果
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub scale: f64,
    // 入力の大きさ（バイト。1 回目の seed のもの）
    pub input_bytes: u64,
    // 各回の時間（ミリ秒）
    pub times_ms: Vec<u64>,
    // 真ん中の値（ミリ秒）
    pub median_ms: u64,
    // いちばん大きいメモリ（KiB。測れなければ None）
    pub memory_kib: Option<u64>,
    // 上限を超えた（この段の時間は「上限より長い」）
    pub timed_out: bool,
}

// 測った結果（Finish::Measured に入れる）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measured {
    pub steps: Vec<Step>,
    // 伸び方の指数（当てはめられなければ None）と、近い形の名前
    pub exponent: Option<f64>,
    pub shape: Option<String>,
    // 割合 1 の時間（ミリ秒）。estimated なら伸び方からの見積もり
    pub full_ms: Option<u64>,
    pub estimated: bool,
    // 割合 1（なければ最後の段）のメモリ（KiB）
    pub memory_kib: Option<u64>,
    // compare.json の時間の制限・メモリの制限
    pub time_limit_ms: u64,
    pub memory_limit_mib: u64,
    // 1 回の時間の上限（ミリ秒）
    pub step_limit_ms: u64,
    pub interrupted: bool,
    // 伸び方を出せない理由など
    pub note: Option<String>,
}

// 1 回の時間の上限
pub fn step_limit_ms(time_limit_ms: u64) -> u64 {
    time_limit_ms.saturating_mul(3).clamp(1, MAX_STEP_MS)
}

// 真ん中の値（偶数個なら小さいほう寄りの平均）
pub fn median(values: &[u64]) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    match sorted.len() {
        0 => 0,
        n if n % 2 == 1 => sorted[n / 2],
        n => (sorted[n / 2 - 1] + sorted[n / 2]) / 2,
    }
}

// (割合, 時間) の両対数に直線を当てはめた傾き。点が 2 つ未満・割合が同じなら None
pub fn fit(points: &[(f64, f64)]) -> Option<f64> {
    let logs: Vec<(f64, f64)> = points
        .iter()
        .filter(|(x, y)| *x > 0.0 && *y > 0.0)
        .map(|(x, y)| (x.ln(), y.ln()))
        .collect();
    if logs.len() < 2 {
        return None;
    }
    let n = logs.len() as f64;
    let mean_x = logs.iter().map(|(x, _)| x).sum::<f64>() / n;
    let mean_y = logs.iter().map(|(_, y)| y).sum::<f64>() / n;
    let sxx: f64 = logs.iter().map(|(x, _)| (x - mean_x).powi(2)).sum();
    if sxx <= f64::EPSILON {
        return None;
    }
    let sxy: f64 = logs.iter().map(|(x, y)| (x - mean_x) * (y - mean_y)).sum();
    Some(sxy / sxx)
}

// 指数に近い形の名前（見当。N と N log N は、この方法ではほとんど見分けられない）
pub fn shape(exponent: f64) -> &'static str {
    match exponent {
        k if k < 0.5 => "定数か log N くらい（大きさにほとんどよらない）",
        k if k < 1.35 => "N・N log N くらい",
        k if k < 1.75 => "N√N・N log² N くらい",
        k if k < 2.5 => "N² くらい",
        k if k < 3.5 => "N³ くらい",
        _ => "N³ より速く伸びる",
    }
}

// 割合 scale で ms かかったものを、伸び方 exponent で割合 1 に引き伸ばした時間
pub fn extrapolate(scale: f64, ms: f64, exponent: f64) -> f64 {
    ms * (1.0 / scale).powf(exponent)
}

// 測った段から、結果をまとめる
pub fn summarize(steps: Vec<Step>, settings: &TestSettings, interrupted: bool) -> Measured {
    let finished: Vec<&Step> = steps.iter().filter(|step| !step.timed_out).collect();
    let points: Vec<(f64, f64)> = finished
        .iter()
        .filter(|step| step.median_ms as f64 >= MIN_FIT_MS)
        .map(|step| (step.scale, step.median_ms as f64))
        .collect();
    let exponent = fit(&points);
    let mut note = None;
    if exponent.is_none() {
        note = Some(if finished.is_empty() {
            "いちばん小さい大きさでも上限を超えたので、伸び方を測れませんでした".to_string()
        } else if points.len() < 2 {
            format!(
                "{} ms 以上かかった段が 2 つより少ないので、伸び方を出せません（短すぎる時間は、起動の時間に埋もれるため）。結果を使わない計算や、式で置き換えられるループ（1 を足すだけの二重ループなど）は、コンパイラの最適化で消えることがあります",
                MIN_FIT_MS
            )
        } else {
            "伸び方を出せませんでした".to_string()
        });
    }
    let last = finished.last();
    let (full_ms, estimated) = match last {
        Some(step) if (step.scale - 1.0).abs() < 1e-9 => (Some(step.median_ms), false),
        Some(step) => match exponent {
            Some(k) => (
                Some(extrapolate(step.scale, step.median_ms.max(1) as f64, k).round() as u64),
                true,
            ),
            None => (None, false),
        },
        None => (None, false),
    };
    let memory_kib = last.and_then(|step| step.memory_kib);
    Measured {
        exponent,
        shape: exponent.map(|k| shape(k).to_string()),
        full_ms,
        estimated,
        memory_kib,
        time_limit_ms: settings.time_limit_ms,
        memory_limit_mib: settings.memory_limit_mib,
        step_limit_ms: step_limit_ms(settings.time_limit_ms),
        interrupted,
        note,
        steps,
    }
}

// 生成器の名前を決める（ランダムテストと同じ決め方。愚直解は要らない）
pub(crate) fn pick_generator(
    extension: &str,
    bins: &[String],
    examples: &[String],
    bin: &str,
) -> Result<String, String> {
    let own = format!("{bin}_gen");
    if examples.contains(&own) {
        return Ok(own);
    }
    if bins.len() == 1 && examples.iter().any(|name| name == "gen") {
        return Ok("gen".to_string());
    }
    Err(format!(
        "計算量を測るには、生成器の example が要ります。examples/{bin}_gen.{extension} を作ってください（バイナリが 1 つなら、examples/gen.{extension} でも使えます）"
    ))
}

// 測る材料
pub(crate) struct Plan<'a> {
    pub dir: &'a Path,
    pub solution: &'a Program,
    pub generator: &'a Program,
    pub settings: &'a TestSettings,
    pub start_seed: u64,
}

fn scale_text(scale: f64) -> String {
    format!("{scale}")
}

// 1/n の形（割合の見せ方）
fn scale_label(scale: f64) -> String {
    if (scale - 1.0).abs() < 1e-9 {
        "1".to_string()
    } else {
        format!("1/{}", (1.0 / scale).round())
    }
}

enum Generated {
    Input(Vec<u8>),
    Stopped,
    Failed(String),
}

fn generate(
    plan: &Plan,
    seed: u64,
    scale: f64,
    cancel: &Arc<AtomicBool>,
    emit: &mut dyn FnMut(RunEvent),
) -> Generated {
    let text = scale_text(scale);
    match run_helper_with(
        "生成器",
        plan.generator,
        plan.dir,
        seed,
        Some(&text),
        None,
        MAX_INPUT_BYTES,
        cancel,
        emit,
    ) {
        Helper::Done(bytes) => Generated::Input(bytes),
        Helper::Stopped => Generated::Stopped,
        Helper::Failed(message) => Generated::Failed(message),
    }
}

// 測る本体
pub(crate) fn run_complexity(
    plan: &Plan,
    cancel: &Arc<AtomicBool>,
    emit: &mut dyn FnMut(RunEvent),
) -> Finish {
    let limit_ms = step_limit_ms(plan.settings.time_limit_ms);
    // 生成器が大きさの引数を読んでいるか：同じ seed で、割合 1/128 と 1 の入力が同じなら読んでいない。
    // 大きさは入力のバイト数でなく値に出ることもある（入力が N だけの問題）ので、バイト数では比べない
    let small = match generate(plan, plan.start_seed, SCALES[0], cancel, emit) {
        Generated::Input(bytes) => bytes,
        Generated::Stopped => {
            return Finish::Measured(Box::new(summarize(Vec::new(), plan.settings, true)));
        }
        Generated::Failed(message) => return Finish::Failed { message },
    };
    let large = match generate(plan, plan.start_seed, 1.0, cancel, emit) {
        Generated::Input(bytes) => bytes,
        Generated::Stopped => {
            return Finish::Measured(Box::new(summarize(Vec::new(), plan.settings, true)));
        }
        Generated::Failed(message) => {
            return Finish::Failed {
                message: format!(
                    "{message}。大きい入力が上限（{} MiB）を超えたか、生成器が大きい入力で失敗しています",
                    MAX_INPUT_BYTES / 1024 / 1024
                ),
            };
        }
    };
    if small == large {
        return Finish::Failed {
            message: "生成器が大きさの引数（2 つ目の引数の割合）を読んでいないようです（同じ seed で、割合 1/128 と 1 の入力が同じでした）。AtCoder Studio が作った今の雛形は、この引数を読みます。前に作った生成器なら、examples/ の生成器を消してから、問題文の欄の「…」の「生成器と愚直解を作る」で作り直すか、2 つ目の引数を読むように直してください".to_string(),
        };
    }
    let mut steps = Vec::new();
    for scale in SCALES {
        let mut times = Vec::new();
        let mut memory: Option<u64> = None;
        let mut input_bytes = 0;
        let mut timed_out = false;
        for repeat in 0..REPEATS {
            if cancel.load(Ordering::SeqCst) {
                return Finish::Measured(Box::new(summarize(steps, plan.settings, true)));
            }
            let seed = plan.start_seed + repeat as u64;
            let input = match generate(plan, seed, scale, cancel, emit) {
                Generated::Input(bytes) => bytes,
                Generated::Stopped => {
                    return Finish::Measured(Box::new(summarize(steps, plan.settings, true)));
                }
                Generated::Failed(message) => return Finish::Failed { message },
            };
            if repeat == 0 {
                input_bytes = input.len() as u64;
            }
            let stdin = match pipe_input(&input) {
                Ok(file) => file,
                Err(message) => return Finish::Failed { message },
            };
            let result = match process::run(
                command(plan.solution, plan.dir),
                Some(stdin),
                &Limits {
                    time: Some(Duration::from_millis(limit_ms)),
                    output: OUTPUT_LIMIT,
                    memory: None,
                },
                cancel,
                |_, _| {},
            ) {
                Ok(result) => result,
                Err(message) => return Finish::Failed { message },
            };
            if cancel.load(Ordering::SeqCst) || matches!(result.outcome, Outcome::Stopped) {
                return Finish::Measured(Box::new(summarize(steps, plan.settings, true)));
            }
            memory = memory.max(result.max_rss_bytes.map(|bytes| bytes.div_ceil(1024)));
            match result.outcome {
                Outcome::Exited(0) | Outcome::OutputLimit(_) => times.push(millis(result.elapsed)),
                Outcome::TimedOut => {
                    timed_out = true;
                    times.push(limit_ms);
                    break;
                }
                outcome => {
                    let reason = match outcome {
                        Outcome::Exited(code) => format!("終了コード {code}"),
                        Outcome::Signaled(signal) => format!("シグナル {signal}"),
                        _ => "異常終了".to_string(),
                    };
                    let stderr = shown(&result.stderr);
                    if !stderr.is_empty() {
                        emit(RunEvent::Log {
                            kind: crate::run::LogKind::Stderr,
                            text: format!("[解答の標準エラー]\n{stderr}\n"),
                        });
                    }
                    return Finish::Failed {
                        message: format!(
                            "割合 {}（seed {seed}）の入力で、解答が異常終了しました（{reason}）。入力が制約を満たしているか、生成器を確かめてください",
                            scale_label(scale)
                        ),
                    };
                }
            }
        }
        let step = Step {
            scale,
            input_bytes,
            median_ms: median(&times),
            times_ms: times,
            memory_kib: memory,
            timed_out,
        };
        emit(RunEvent::Log {
            kind: crate::run::LogKind::Info,
            text: format!(
                "割合 {}：入力 {} バイト、{}{}\n",
                scale_label(scale),
                step.input_bytes,
                if timed_out {
                    format!("{limit_ms} ms の上限を超えました")
                } else {
                    format!("{} ms（{:?} ms）", step.median_ms, step.times_ms)
                },
                step.memory_kib
                    .map(|kib| format!("、{} KiB", kib))
                    .unwrap_or_default()
            ),
        });
        emit(RunEvent::ComplexityStep { step: step.clone() });
        let stop = timed_out || step.median_ms > limit_ms;
        steps.push(step);
        if stop {
            break;
        }
    }
    Finish::Measured(Box::new(summarize(steps, plan.settings, false)))
}

// ビルドから始めて測る。dir はパッケージのディレクトリ
pub(crate) fn complexity_target(
    dir: &Path,
    target: &Target,
    langs: &LangSettings,
    start_seed: u64,
    cancel: &Arc<AtomicBool>,
    channel: &Channel<RunEvent>,
) -> Finish {
    let extension = target.language.extension();
    let (prepared, (generator, settings)) = match prepare(
        dir,
        target,
        true,
        langs,
        cancel,
        channel,
        |bins, examples| {
            let tests = toolchain::test_dir(dir, bins, &target.bin);
            let generator = pick_generator(extension, bins, examples, &target.bin)
                .map_err(|message| Finish::Failed { message })?;
            let settings = tests
                .dir
                .as_deref()
                .map(compare::read_settings)
                .unwrap_or_else(|| Ok(TestSettings::default()))
                .map_err(|message| Finish::Failed { message })?;
            let examples = vec![generator.clone()];
            Ok(((generator, settings), examples))
        },
    ) {
        Ok(prepared) => prepared,
        Err(finish) => return finish,
    };
    let executable = prepared.executable.clone();
    let generator_exe = match prepared.example(&generator, "生成器") {
        Ok(path) => path,
        Err(finish) => return finish,
    };
    info(
        channel,
        format!(
            "\n計算量：生成器 examples/{generator}.{extension} に、seed と大きさの割合（1/128〜1）を渡して入力を作り、解答だけを各段 {REPEATS} 回動かします。1 回の上限 {} ms（compare.json の時間の制限 {} ms の 3 倍か 10 秒）\n\n",
            step_limit_ms(settings.time_limit_ms),
            settings.time_limit_ms
        ),
    );
    let plan = Plan {
        dir,
        solution: &executable,
        generator: &generator_exe,
        settings: &settings,
        start_seed,
    };
    run_complexity(&plan, cancel, &mut |event| send(channel, event))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fits_the_growth_on_log_log_scales() {
        // 時間が割合の 2 乗に比べる
        let points: Vec<(f64, f64)> = SCALES.iter().map(|s| (*s, 4000.0 * s * s)).collect();
        assert!((fit(&points).unwrap() - 2.0).abs() < 1e-9);
        // 1 乗
        let points: Vec<(f64, f64)> = SCALES.iter().map(|s| (*s, 900.0 * s)).collect();
        assert!((fit(&points).unwrap() - 1.0).abs() < 1e-9);
        // 点が足りない・割合が同じ
        assert_eq!(fit(&[(0.5, 10.0)]), None);
        assert_eq!(fit(&[(0.5, 10.0), (0.5, 20.0)]), None);
        assert_eq!(shape(2.0), "N² くらい");
        assert_eq!(shape(1.1), "N・N log N くらい");
        assert!((extrapolate(0.25, 100.0, 2.0) - 1600.0).abs() < 1e-9);
        assert_eq!(median(&[30, 10, 20]), 20);
        assert_eq!(median(&[10, 20]), 15);
        assert_eq!(step_limit_ms(2500), 7500);
        assert_eq!(step_limit_ms(5000), MAX_STEP_MS);
    }

    fn step(scale: f64, ms: u64, timed_out: bool) -> Step {
        Step {
            scale,
            input_bytes: 0,
            times_ms: vec![ms],
            median_ms: ms,
            memory_kib: Some(1000),
            timed_out,
        }
    }

    #[test]
    fn summarizes_and_estimates_the_full_size() {
        let settings = TestSettings::default();
        // 割合 1 まで測れた：測った時間をそのまま
        let steps: Vec<Step> = SCALES
            .iter()
            .map(|s| step(*s, (2000.0 * s).round() as u64, false))
            .collect();
        let measured = summarize(steps, &settings, false);
        assert_eq!((measured.full_ms, measured.estimated), (Some(2000), false));
        assert!((measured.exponent.unwrap() - 1.0).abs() < 0.05);
        // 途中で上限を超えた：最後に測れた段から見積もる
        let steps = vec![
            step(1.0 / 128.0, 60, false),
            step(1.0 / 64.0, 240, false),
            step(1.0 / 32.0, 960, false),
            step(1.0 / 16.0, 7500, true),
        ];
        let measured = summarize(steps, &settings, false);
        assert!((measured.exponent.unwrap() - 2.0).abs() < 1e-9);
        assert_eq!(measured.full_ms, Some(960 * 32 * 32));
        assert!(measured.estimated);
        // 短すぎる段しかない：伸び方は出さない
        let steps: Vec<Step> = SCALES.iter().map(|s| step(*s, 5, false)).collect();
        let measured = summarize(steps, &settings, false);
        assert_eq!(measured.exponent, None);
        assert_eq!(measured.full_ms, Some(5));
        let note = measured.note.unwrap();
        assert!(note.contains("50 ms") && note.contains("最適化"), "{note}");
    }

    // ---- 実際に cargo でビルドして測る確認（cargo が要る。ネットワークは使わない） ----

    struct Dir(std::path::PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    // O(N²) の解答
    const SQUARE: &str = r#"
use std::io::Read;
fn main() {
    let mut s = String::new();
    std::io::stdin().read_to_string(&mut s).unwrap();
    let n: u64 = s.split_whitespace().next().unwrap().parse().unwrap();
    let mut acc = 0u64;
    for i in 0..n {
        for j in 0..n {
            acc = acc.wrapping_add(std::hint::black_box(i ^ j));
        }
    }
    println!("{acc}");
}
"#;
    // 割合 × 30000 を N にする生成器（割合がなければ 5）。入力の大きさも N に比べる
    const SCALED: &str = r#"
fn main() {
    let mut args = std::env::args().skip(1);
    let _seed: u64 = args.next().unwrap().parse().unwrap();
    let scale: f64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    let n = if scale > 0.0 { (30000.0 * scale) as usize } else { 5 };
    println!("{n}");
    println!("{}", "1 ".repeat(n));
}
"#;
    // 大きさを読まない生成器（seed だけを読む。seed で大きさが変わっても、割合では変わらない）
    const FIXED: &str = r#"
fn main() {
    let seed: usize = std::env::args().nth(1).unwrap().parse().unwrap();
    println!("{}", "1 ".repeat(seed * 1000));
}
"#;
    // 入力が N だけで、大きさが値に出る生成器
    const VALUE: &str = r#"
fn main() {
    let scale: f64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0.0);
    println!("{}", if scale > 0.0 { (30000.0 * scale) as usize } else { 5 });
}
"#;

    fn measure(pkg: &Path) -> Finish {
        let target = Target {
            package: "pkg".to_string(),
            bin: "solution".to_string(),
            language: crate::language::Language::Rust,
        };
        let channel = Channel::new(|_| Ok(()));
        complexity_target(
            pkg,
            &target,
            &LangSettings::default(),
            1,
            &Arc::new(AtomicBool::new(false)),
            &channel,
        )
    }

    #[test]
    fn measures_real_programs() {
        let base =
            std::env::temp_dir().join(format!("atcoder-studio-complexity-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let ws = Dir(base.canonicalize().unwrap());
        let pkg = ws.0.join("pkg");
        write(
            &pkg.join("Cargo.toml"),
            "[package]\nname = \"pkg\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[[bin]]\nname = \"solution\"\npath = \"src/solution.rs\"\n",
        );
        write(&pkg.join("src/solution.rs"), SQUARE);
        write(&pkg.join("examples/gen.rs"), SCALED);
        // 1. O(N²)：指数が 2 に近い
        let Finish::Measured(measured) = measure(&pkg) else {
            panic!("測れませんでした");
        };
        assert!(!measured.interrupted);
        assert_eq!(measured.steps.len(), SCALES.len(), "{measured:?}");
        let k = measured.exponent.expect("伸び方");
        assert!((1.6..2.4).contains(&k), "{measured:?}");
        assert_eq!(measured.shape.as_deref(), Some("N² くらい"));
        assert!(!measured.estimated);
        // 2. 時間の制限を小さくする（1 回の上限は 3 倍の 150 ms）：途中で止め、割合 1 を見積もる
        write(&pkg.join("test/compare.json"), r#"{"timeLimitMs": 50}"#);
        let Finish::Measured(measured) = measure(&pkg) else {
            panic!("測れませんでした");
        };
        assert!(measured.steps.len() < SCALES.len(), "{measured:?}");
        assert!(
            measured.steps.last().unwrap().timed_out
                || measured.steps.last().unwrap().median_ms > 150
        );
        assert_eq!(measured.step_limit_ms, 150);
        let _ = std::fs::remove_file(pkg.join("test/compare.json"));
        // 3. 大きさを読まない生成器：測らずに止め、理由を出す
        write(&pkg.join("examples/gen.rs"), FIXED);
        let Finish::Failed { message } = measure(&pkg) else {
            panic!("止まりませんでした");
        };
        assert!(message.contains("大きさの引数"), "{message}");
        // 4. 入力が N だけ（入力のバイト数はほとんど変わらない）：止めずに測り、指数が 2 に近い
        write(&pkg.join("examples/gen.rs"), VALUE);
        let Finish::Measured(measured) = measure(&pkg) else {
            panic!("測れませんでした");
        };
        assert_eq!(measured.steps.len(), SCALES.len(), "{measured:?}");
        // 手元の時間の揺れがあるので、1 乗・0 乗と見分けられる範囲で確かめる
        let k = measured.exponent.expect("伸び方");
        assert!((1.4..2.6).contains(&k), "{measured:?}");
    }

    #[test]
    fn picks_the_generator() {
        let bins = vec!["main".to_string()];
        assert_eq!(
            pick_generator("rs", &bins, &["gen".to_string()], "main").unwrap(),
            "gen"
        );
        assert_eq!(
            pick_generator(
                "rs",
                &bins,
                &["main_gen".to_string(), "gen".to_string()],
                "main"
            )
            .unwrap(),
            "main_gen"
        );
        assert!(
            pick_generator("rs", &bins, &[], "main")
                .unwrap_err()
                .contains("examples/main_gen.rs")
        );
    }
}
