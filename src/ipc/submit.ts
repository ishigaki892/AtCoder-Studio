// AtCoder のログインと提出のコマンドの呼び出し（Rust 側は src-tauri/src/session.rs・submit.rs。Phase 6。D-147・D-148）
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ProblemId } from "./problem";
import type { Target } from "./workspace";

// ログインの状態。userName は、ログインの窓か提出のフォームで確かめたもの（起動し直すと、次に確かめるまで null）
export type SessionView = {
  loggedIn: boolean;
  userName: string | null;
  // ログインの窓で失敗したときの理由
  error: string | null;
};

// 判定（状態の JSON から。AtCoder のジャッジの値）
export type Judge = {
  // 判定中
  waiting: boolean;
  // ページの表記（例：AC、WA、WJ。判定中は進み具合が入ることがある）
  label: string;
  // 表記の英語名（例：Accepted）
  title: string | null;
  score: string | null;
  timeMs: number | null;
  memoryKib: number | null;
};

// rejected：受け付けられなかった、unknownId：受け付けられたが ID が分からない、waiting：判定待ち、
// judging：判定中、done：確定、unreadable：読めなかった（「更新」で読み直せる）
export type SubmissionState = "rejected" | "unknownId" | "waiting" | "judging" | "done" | "unreadable";

export type SubmissionView = {
  // このアプリの中での番号（「更新」に使う）
  key: number;
  contest: string;
  task: string;
  taskTitle: string;
  // 送った時刻（UNIX 時間のミリ秒）
  timeMs: number;
  language: string;
  // 作業領域のルートからのパス
  file: string;
  submissionId: number | null;
  state: SubmissionState;
  // 判定を読み続けている
  tracking: boolean;
  judge: Judge | null;
  message: string | null;
  detailUrl: string | null;
  listUrl: string;
};

// 「提出」の結果。送らなかったもの（outOfPeriod・notLoggedIn・refused）は、
// 「コードをコピーして公式の提出ページを開く」に使うファイルと注意を持つ
export type SubmitOutcome =
  | { kind: "outOfPeriod"; start: string; end: string; file: string; warnings: string[]; submitUrl: string }
  | { kind: "notLoggedIn"; message: string; file: string; warnings: string[]; submitUrl: string }
  | { kind: "refused"; reason: string; file: string; warnings: string[]; submitUrl: string }
  | { kind: "cancelled" }
  // dialogSkipped は、確認のダイアログを省いたときの、サンプルテストのケースの数（D-162）
  | { kind: "sent"; submission: SubmissionView; dialogSkipped?: number };

// ログインの窓を開く（済んだかどうかは onSession で届く）
export function atcoderLogin(): Promise<void> {
  return invoke("atcoder_login");
}

export function atcoderSession(): Promise<SessionView> {
  return invoke<SessionView>("atcoder_session");
}

// Keychain の値を消す（サーバーには何も送らない）
export function atcoderLogout(): Promise<SessionView> {
  return invoke<SessionView>("atcoder_logout");
}

// 提出する。確認のダイアログは Rust 側が出し、「提出する」のときだけ送る
export function atcoderSubmit(target: Target, problem: ProblemId): Promise<SubmitOutcome> {
  return invoke<SubmitOutcome>("atcoder_submit", {
    target,
    contest: problem.contest,
    task: problem.task,
  });
}

// このアプリを起動してから出した提出（新しい順）
export function atcoderSubmissions(): Promise<SubmissionView[]> {
  return invoke<SubmissionView[]>("atcoder_submissions");
}

// 「更新」：判定を読み直す（まだ判定中なら、また追う）
export function refreshSubmission(key: number): Promise<SubmissionView> {
  return invoke<SubmissionView>("atcoder_refresh_submission", { key });
}

// 公式の提出のページを既定のブラウザで開く
export function openSubmitPage(problem: ProblemId): Promise<void> {
  return invoke("open_submit_page", { contest: problem.contest, task: problem.task });
}

// 提出の詳細（submissionId があるとき）か、自分の提出の一覧を、既定のブラウザで開く
export function openSubmissionPage(contest: string, submissionId: number | null): Promise<void> {
  return invoke("open_submission_page", { contest, submissionId });
}

// サイドバーの「コンテスト」（Phase 18 作業 4。D-218）。問題ごとの提出状況
export type TaskStatus = {
  task: string;
  // 例：「A」。問題の一覧にない問題は空
  label: string;
  title: string;
  // AC の提出がある
  accepted: boolean;
  // いちばん新しい提出の判定と ID（提出がなければ null）
  last: string | null;
  lastId: number | null;
  count: number;
};

// 自分の順位（Phase 21。D-230）
export type Place = {
  // 全体の順位
  rank: number;
  // Rated の中での順位（同順位は平均。Unrated の参加なら、Rated の人の中に入れたときの順位）
  ratedRank: number;
  isRated: boolean;
};

export type TaskDifficulty = {
  task: string;
  // 予想 diff（400 未満は直した値）。出せなければ null
  difficulty: number | null;
};

// 順位表から計算したもの（Rust 側は src-tauri/src/standings.rs。D-230〜D-232）
export type StandingsSummary = {
  // 順位が確定した（終わったあと）
  fixed: boolean;
  participants: number;
  ratedCount: number;
  // 参加していなければ null
  place: Place | null;
  // 予想パフォーマンス（近似。D-231）
  performance: number | null;
  difficulties: TaskDifficulty[];
  // 予想 perf・予想 diff を出さない理由（Rated でない・AHC）
  note: string | null;
};

// 追跡の表（Phase 31。D-253。Rust 側は standings.rs の FollowTable）
export type FollowTask = {
  task: string;
  // 列の見出し（A・B…）
  label: string;
};

export type FollowCell = {
  // 得点
  score: number;
  // 得点がある（AC。部分点を含む）
  accepted: boolean;
  // 得点を取った時刻（開始からの秒）。得点がなければ null
  elapsedSecs: number | null;
  // 得点を取るまでの誤答の数
  penalty: number;
  // 判定を受けた提出の数（得点がないときは、誤答の数）
  failure: number;
  // 提出の数
  count: number;
  pending: boolean;
};

export type FollowRow = {
  user: string;
  me: boolean;
  // 順位表に行がない（参加していない・ID の誤り）
  missing: boolean;
  rank: number | null;
  // レート（0 なら null）
  rating: number | null;
  isRated: boolean;
  submitted: boolean;
  score: number;
  // 時間（開始からの秒。ペナルティの時間を含む）
  elapsedSecs: number;
  penalty: number;
  // tasks と同じ順。出していない問題は null
  cells: (FollowCell | null)[];
};

export type FollowTable = {
  fixed: boolean;
  tasks: FollowTask[];
  // 自分（分かれば一番上）、次に設定の順
  rows: FollowRow[];
};

export type StandingsOutcome =
  | { kind: "read"; summary: StandingsSummary; follow: FollowTable }
  // 始まる前（順位表は読まない）
  | { kind: "notStarted" }
  // ログインしていない（「追跡」の「更新」のとき）
  | { kind: "notLoggedIn" }
  | { kind: "failed"; message: string };

export type ContestStatus = {
  contest: string;
  tasks: TaskStatus[];
  // 読んだ時刻（UNIX 時間のミリ秒）
  readAtMs: number;
  // 上限のページまで読んでも続きがあった
  truncated: boolean;
  pages: number;
  // 順位・予想パフォーマンス・予想 diff（Phase 21。D-230）と追跡の表（D-253）。読めなくても、提出状況は出る
  standings: StandingsOutcome;
  // 「自分の提出」のページから分かった、ログインしているユーザー
  user: string | null;
};

export type StatusOutcome =
  | { kind: "read"; status: ContestStatus }
  | { kind: "notLoggedIn" }
  | { kind: "failed"; message: string };

// 「自分の提出」のページを読み、問題ごとに数える。続けて順位表を読み、順位・予想 perf・予想 diff を計算する（D-230）
export function contestStatus(contest: string): Promise<StatusOutcome> {
  return invoke<StatusOutcome>("contest_status", { contest });
}

// 右下の「追跡」の「更新」（Phase 31。D-253）：順位表だけを読み、自分と追跡するユーザーの行を取り出す
export function contestStandings(contest: string): Promise<StandingsOutcome> {
  return invoke<StandingsOutcome>("contest_standings", { contest });
}

// コンテストの開始・終了の時刻（UNIX 時間の秒。Phase 22 作業 1。D-238）。
// Rust 側がトップページをログインなしで読み、アプリを開いている間は覚えておく
export type ContestPeriod = { start: number; end: number };

// refresh なら、Rust 側が覚えているものを使わずに読み直す（覚えている終了時刻を過ぎたとき。C-3）
export function contestPeriod(contest: string, refresh = false): Promise<ContestPeriod> {
  return invoke<ContestPeriod>("contest_period", { contest, refresh });
}

// 残り時間を出しているコンテストを Rust 側に伝える（出していなければ null）。まだ始まっていなければ、Rust 側が
// 開始まで待ち、始まったときに通知する（設定でオンで、窓が手前にないときだけ。Phase 22 作業 2。D-239）
export function watchContestStart(contest: string | null): Promise<void> {
  return invoke("watch_contest_start", { contest });
}

// ログインの状態が変わった（ログインの窓・提出のとき）
export function onSession(handler: (view: SessionView) => void): Promise<UnlistenFn> {
  return listen<SessionView>("atcoder-session", (event) => handler(event.payload));
}

// 提出の判定が変わった
export function onSubmission(handler: (view: SubmissionView) => void): Promise<UnlistenFn> {
  return listen<SubmissionView>("atcoder-submission", (event) => handler(event.payload));
}
