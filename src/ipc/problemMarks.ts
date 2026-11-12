// 自力 AC・解説 AC の記録（Phase 33。D-257）と解き直しの印（Phase 34。D-258）。保存は Rust 側（src-tauri/src/problem_marks.rs）。
// アプリのフォルダの problem-marks.json に、問題 ID ごとに手で付けたものを置く。AtCoder には何も送らない
import { invoke } from "@tauri-apps/api/core";
import type { DifficultyView, Solved } from "./atcoderProblems";

export type SolvedBy = "self" | "editorial";
// retry は解き直しの印を付けた日（例 "2026-11-12"）
export type ProblemMark = { solved?: SolvedBy; retry?: string };
// キーは問題 ID（例：abc001_1）。記録のない問題は入らない
export type ProblemMarks = Record<string, ProblemMark>;

export const SOLVED_LABELS: Record<SolvedBy, string> = { self: "自力", editorial: "解説" };

export function problemMarks(): Promise<ProblemMarks> {
  return invoke<ProblemMarks>("problem_marks");
}

// solved が null なら記録なし。変えたあとのすべての記録を返す
export function setProblemMark(id: string, solved: SolvedBy | null): Promise<ProblemMarks> {
  return invoke<ProblemMarks>("set_problem_mark", { id, solved });
}

// 解き直しの印を付け外しする（付けるときは今日の日付。すでに付いていれば日付は変えない）。変えたあとのすべての記録を返す
export function setProblemRetry(id: string, on: boolean): Promise<ProblemMarks> {
  return invoke<ProblemMarks>("set_problem_retry", { id, on });
}

// 「過去問」の「解き直し」の一覧（D-258）。AtCoder Problems のデータと AC 状況は保存してあるものだけを使う
export type RetryPlace = { contest: string; index: string };
export type RetryRow = {
  problem: string;
  // 入っているコンテスト（データがなければ空）
  places: RetryPlace[];
  // 行を押したときに問題文の欄に出すコンテスト
  contest: string;
  difficulty: DifficultyView | null;
  solved: Solved | null;
  solvedBy: SolvedBy | null;
  since: string;
};
export type RetryView = { rows: RetryRow[]; user: string | null; messages: string[] };

export function retryProblems(): Promise<RetryView> {
  return invoke<RetryView>("retry_problems");
}
