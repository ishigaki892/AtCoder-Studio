// 問題ごとの経過時間（Phase 42。D-266）。保存は Rust 側（src-tauri/src/problem_times.rs）の problem-times.json。
// 数えるのは画面側（ProblemClock.tsx）
import { invoke } from "@tauri-apps/api/core";

// seconds は問題文の欄に出していた時間の合計（秒）、acSeconds は AC が分かったときの時間（なければ数え続ける）
export type ProblemTime = { seconds: number; acSeconds?: number };
// キーは問題 ID
export type ProblemTimes = Record<string, ProblemTime>;

export function problemTimes(): Promise<ProblemTimes> {
  return invoke<ProblemTimes>("problem_times");
}

// 1 問の時間を書く（秒は整数にして送る。0 秒で AC もなければ消える）
export function setProblemTime(id: string, time: ProblemTime): Promise<void> {
  return invoke<void>("set_problem_time", {
    id,
    time: { seconds: Math.floor(time.seconds), acSeconds: time.acSeconds },
  });
}

// 「⟲」：確かめてから 0 に戻す。戻したら true
export function resetProblemTime(id: string): Promise<boolean> {
  return invoke<boolean>("reset_problem_time", { id });
}
