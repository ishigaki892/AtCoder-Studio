// 自力 AC・解説 AC の記録（Phase 33。D-257）と解き直しの印（Phase 34。D-258）。保存は Rust 側（src-tauri/src/problem_marks.rs）。
// アプリのフォルダの problem-marks.json に、問題 ID ごとに手で付けたものを置く。AtCoder には何も送らない
import { invoke } from "@tauri-apps/api/core";
import type { DifficultyView, Solved } from "./atcoderProblems";

export type SolvedBy = "self" | "editorial";
// retry は解き直しの印を付けた日（例 "2026-11-12"）。solvedOn は「自力」「解説」を付けた日（Phase 37。D-261。前の版の記録にはない）
// memo は問題ごとのメモ、memoOn はメモを最後に書いた日（Phase 39。D-263）
export type ProblemMark = { solved?: SolvedBy; solvedOn?: string; retry?: string; memo?: string; memoOn?: string };

// メモの長さの上限（文字の数。Rust 側の MEMO_LIMIT と同じ）
export const MEMO_LIMIT = 10_000;
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
  // 一覧に入れた日（「解き直し」は印を付けた日、「解説 AC」は「解説」を付けた日。前の版の記録は null）
  since: string | null;
  // 解き直しの印を付けた日（なければ null）
  retry: string | null;
};
export type RetryView = { rows: RetryRow[]; user: string | null; messages: string[] };

export function retryProblems(): Promise<RetryView> {
  return invoke<RetryView>("retry_problems");
}

// メモを書く（D-263）。空白だけなら消す。上限を超えると保存せずにエラー。変えたあとのすべての記録を返す
export function setProblemMemo(id: string, memo: string): Promise<ProblemMarks> {
  return invoke<ProblemMarks>("set_problem_memo", { id, memo });
}

// サイドバーの「メモ」の一覧（D-263）。メモのある問題を、最後に書いた日の新しい順に
export function memoProblems(): Promise<RetryView> {
  return invoke<RetryView>("memo_problems");
}

// 「過去問」の「解説 AC」の一覧（D-261）。「解説」を付けた問題すべて（AC 状況は問わない）。並びは付けた日の新しい順
export function editorialProblems(): Promise<RetryView> {
  return invoke<RetryView>("editorial_problems");
}
