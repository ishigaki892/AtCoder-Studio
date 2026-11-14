// 問題ごとの経過時間（Phase 42。D-266）。保存は Rust 側（src-tauri/src/problem_times.rs）の problem-times.json。
// 数えるのは画面側（ProblemClock.tsx）
import { invoke } from "@tauri-apps/api/core";

// seconds は問題文の欄に出していた時間の合計（秒）、acSeconds は AC が分かったときの時間（なければ数え続ける）。
// startedAt は数え始めた時刻（UNIX 時間の秒。「⟲」で今にする）。これより後の AC だけで止める（B-1。Phase 42 の記録にはない）
export type ProblemTime = { seconds: number; acSeconds?: number; startedAt?: number };
// キーは問題 ID
export type ProblemTimes = Record<string, ProblemTime>;

export function problemTimes(): Promise<ProblemTimes> {
  return invoke<ProblemTimes>("problem_times");
}

// 1 問の時間を書く（秒は整数にして送る。0 秒で AC も数え始めた時刻もなければ消える）
export function setProblemTime(id: string, time: ProblemTime): Promise<void> {
  return invoke<void>("set_problem_time", {
    id,
    time: { seconds: Math.floor(time.seconds), acSeconds: time.acSeconds, startedAt: time.startedAt },
  });
}

// 「⟲」：確かめてから 0 に戻し、数え始めた時刻を今にする。戻したら新しい記録、やめたら null
export function resetProblemTime(id: string): Promise<ProblemTime | null> {
  return invoke<ProblemTime | null>("reset_problem_time", { id });
}
