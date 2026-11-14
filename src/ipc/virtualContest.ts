// バーチャル参加（Phase 44。D-268。Rust 側は src-tauri/src/virtual_contest.rs）。
// 時刻は UNIX 時間の秒。得点は 100 倍（順位表と同じ）
import { invoke } from "@tauri-apps/api/core";

export type VirtualCurrent = { contest: string; start: number; end: number };

export type VirtualTaskScore = {
  task: string;
  label: string;
  score: number;
  // 得点を取った時刻（開始からの秒）
  acSeconds: number | null;
  // 誤答の数（得点を取っていれば、その前の誤答）
  penalty: number;
  // 判定中の提出がある
  pending: boolean;
};

export type VirtualScore = {
  tasks: VirtualTaskScore[];
  score: number;
  // 時間（秒）：最後に得点を取った時刻 ＋ ペナルティの時間
  elapsed: number;
  penalty: number;
  penaltySeconds: number;
};

export type VirtualRecord = {
  contest: string;
  start: number;
  end: number;
  finished: number;
  result: VirtualScore;
  // 当時の順位表に当てはめた順位と人数
  rank: number | null;
  participants: number | null;
  performance: number | null;
  note: string | null;
};

export type VirtualFile = { current: VirtualCurrent | null; results: VirtualRecord[] };

export function virtualState(): Promise<VirtualFile> {
  return invoke<VirtualFile>("virtual_state");
}

// 確かめてから始める。始めなければ null
export function startVirtual(contest: string): Promise<VirtualFile | null> {
  return invoke<VirtualFile | null>("start_virtual", { contest });
}

// 終えて結果を出す（時間が残っていれば確かめてから）。終えなければ null
export function finishVirtual(): Promise<VirtualFile | null> {
  return invoke<VirtualFile | null>("finish_virtual");
}

// 結果を残さずにやめる（確かめてから）。やめなければ null
export function cancelVirtual(): Promise<VirtualFile | null> {
  return invoke<VirtualFile | null>("cancel_virtual");
}

// 得点（100 倍）を「600」「12.5」の形に
export function scoreText(score: number): string {
  return String(Number((score / 100).toFixed(2)));
}
