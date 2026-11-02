// AtCoder Problems（kenkoooo.com。非公式の API）のデータ（Phase 24。D-241）。
// Rust 側は src-tauri/src/atcoder_problems.rs。読むのは Rust 側で、2 秒あけ、保存から 24 時間たったら読み直す
import { invoke } from "@tauri-apps/api/core";

// 問題の diff（400 未満は AtCoder Problems の画面と同じく直した値）。experimental は試験的な推定
export type DifficultyView = { value: number; experimental: boolean };

// 問題ごとの diff。ないものは入らない
export function problemDifficulties(ids: string[]): Promise<Record<string, DifficultyView>> {
  return invoke<Record<string, DifficultyView>>("problem_difficulties", { ids });
}

// diff のマウスを乗せたときの説明
export function difficultyHint(view: DifficultyView): string {
  return `AtCoder Problems の推定の難易度（50% の確率で解けるレート）${view.experimental ? "。元にした結果が少ない、試験的な推定です" : ""}`;
}

// 表に出す文字（試験的な推定には「?」を付ける）
export function difficultyText(view: DifficultyView): string {
  return `diff ${view.value}${view.experimental ? "?" : ""}`;
}

// 中央の「過去問」のタブの表（Phase 24 作業 2。D-242）
export type PastCategory = "abc" | "arc" | "agc";
export type Solved = "accepted" | "tried";
export type PastCell = { problem: string; difficulty: DifficultyView | null; solved: Solved | null };
export type PastRow = { contest: string; title: string; cells: (PastCell | null)[] };
export type PastView = {
  table: { columns: string[]; rows: PastRow[]; accepted: number; total: number };
  // AC 状況に使った ID（設定にないときは null。表は diff だけ）
  user: string | null;
  // 提出を最後まで読めている（上限の回数で止めたときは false）
  solvedComplete: boolean;
  messages: string[];
};

// refresh（「更新」）なら、データのファイルも読み直し、提出の続きを読む。開いたときは、提出は 1 分ごと
export function pastProblems(category: PastCategory, refresh: boolean): Promise<PastView> {
  return invoke<PastView>("past_problems", { category, refresh });
}
