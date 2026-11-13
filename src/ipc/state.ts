// 設定・レイアウト・前回の作業領域の読み書き（Rust 側は src-tauri/src/storage.rs、commands.rs。作業 6）
import { invoke } from "@tauri-apps/api/core";
import type { Language } from "./workspace";

export type Layout = {
  sidebarWidth: number;
  sidebarVisible: boolean;
  rightWidth: number;
  rightVisible: boolean;
  bottomRatio: number;
  bottomVisible: boolean;
  bottomRightRatio: number;
  bottomRightVisible: boolean;
  // サイドバーの中身（Phase 12 作業 1。D-182）。知らない値はエクスプローラーとして扱う（App.tsx）
  sidebarView: string;
  // 中央の「過去問」のタブ（Phase 24。D-242）：closed・open・active。前の版の state.json にはない（空なら閉じている）
  pastProblems: string;
  // 中央の「設定」のタブ（Phase 26。D-246）：closed・open・active。前の版の state.json にはない
  settingsTab: string;
  // 「過去問」の「JOI」で選んでいた小分け（Phase 36。D-260）。前の版の state.json にはない（知らない値は一次予選）
  pastJoi: string;
};

export type LoadedKeyBinding = {
  command: string;
  key: string;
  meta: boolean;
  ctrl: boolean;
  alt: boolean;
  shift: boolean;
};

// settings.json の見た目（D-159）。読めない項目は、Rust 側で既定の値にしてある
export type Appearance = {
  // 色の変数の名前（先頭の -- を除く）と値。名前と値は、画面側（appearance.ts）で確かめる
  colors: Record<string, string>;
  indentRainbow: boolean;
  errorLens: boolean;
  // パンくずリスト（Phase 18 作業 1。D-216）
  breadcrumbs: boolean;
  editorFontSize: number;
  problemFontSize: number;
  // 設定の画面の形（Phase 26。D-246）：dialog（窓に重ねる今の形）・editor（中央のタブの VS Code 風）
  settingsUi: SettingsUi;
};

export type SettingsUi = "dialog" | "editor";

export const SETTINGS_UIS: { value: SettingsUi; label: string }[] = [
  // 画面に出す名前は「設定1」「設定2」（2026-11-02、ユーザーの依頼）
  { value: "dialog", label: "設定1（窓に重ねる）" },
  { value: "editor", label: "設定2（VS Code 風。中央のタブ）" },
];

// settings.json の提出の設定（Phase 9 作業 2。D-162）。提出のときは、Rust 側が settings.json から読み直す
export type SubmitSettings = {
  // 「提出」で、まずサンプルテストを動かす。すべて AC でなければ提出しない
  testBeforeSubmit: boolean;
  // false なら、サンプルテストがすべて AC のときだけ、確認のダイアログを省く
  confirmDialog: boolean;
};

// settings.json の通知の設定（Phase 22 作業 2。D-239）。初期値はどちらもオフ。知らせるときは Rust 側が読み直す
export type NotifySettings = {
  // IDE から出した提出の判定が決まったとき
  judge: boolean;
  // 残り時間を出しているコンテストが始まったとき
  contestStart: boolean;
};

export const DEFAULT_NOTIFY_SETTINGS: NotifySettings = { judge: false, contestStart: false };

// settings.json の AtCoder Problems の設定（Phase 24。D-241）。user は AtCoder の ID（空なら AC 状況を出さない）
export type AtcoderProblemsSettings = { user: string };

export const DEFAULT_ATCODER_PROBLEMS_SETTINGS: AtcoderProblemsSettings = { user: "" };

// settings.json の順位表の設定（Phase 31。D-253）。follow は追跡するユーザーの AtCoder の ID。
// 設定の画面では、書いている途中の行（空・正しくないもの）もそのまま持ち、保存のときに Rust 側が確かめる
// （前後の空白を外し、空の行と同じ ID を除く。正しくない ID・31 人目があれば保存しない）
export type StandingsSettings = { follow: string[] };

export const DEFAULT_STANDINGS_SETTINGS: StandingsSettings = { follow: [] };

// settings.json の問題文の欄の設定（Phase 15 作業 3。D-196・D-197）
export type ProblemSettings = {
  // 見出しの行に出すボタン（PROBLEM_BUTTONS の名前）。出さないものは「…」の一覧に入る
  buttons: string[];
  // テストケースが複数のとき、main() に input! { t } と for _ in 0..t { solve(); } まで書く
  mainLoop: boolean;
  // 問題・コンテストを作るときに input! を入れておく（D-198。Rust だけ）
  fillInput: boolean;
  // 問題・コンテストを作るときの言語（Phase 16 作業 3。D-208）
  language: Language;
};

// 選べる言語と、画面に出す名前（D-208）
export const LANGUAGES: { value: Language; label: string }[] = [
  { value: "rust", label: "Rust" },
  { value: "cpp", label: "C++" },
  { value: "python", label: "Python" },
];

// 見出しの行に出せるボタン（Rust 側の storage.rs の PROBLEM_BUTTONS と同じ順）
export const PROBLEM_BUTTONS = [
  { name: "makeInput", label: "input! を作る（C++・Python は入力のコードを作る）" },
  { name: "saveSamples", label: "サンプルを保存" },
  { name: "refresh", label: "再取得" },
  { name: "openPage", label: "公式ページ" },
  // 解説の一覧を既定のブラウザで開く（Phase 25 作業 2。D-244）
  { name: "editorial", label: "解説" },
  { name: "createProblem", label: "この問題を作る" },
  // ランダムテストの生成器と愚直解を examples/ に置く（Phase 35 作業 2。D-259）
  { name: "makeRandom", label: "生成器と愚直解を作る" },
  // 問題のフォルダの main を old/ に移して雛形に戻す（Phase 34 作業 3。D-258）
  { name: "restart", label: "白紙から解き直す" },
] as const;

export const DEFAULT_PROBLEM_SETTINGS: ProblemSettings = {
  buttons: ["makeInput", "saveSamples"],
  mainLoop: false,
  fillInput: false,
  language: "rust",
};

// settings.json の python の欄（Phase 19。D-221）。パスが空なら探す。設定の画面は処理系だけを変える
export type Interpreter = "cpython" | "pypy";

export type PythonSettings = {
  interpreter: Interpreter;
  cpython: string;
  pypy: string;
};

export const DEFAULT_PYTHON_SETTINGS: PythonSettings = { interpreter: "cpython", cpython: "", pypy: "" };

// 選べる処理系と、画面に出す名前（ジャッジの言語の名前に合わせる）
export const INTERPRETERS: { value: Interpreter; label: string }[] = [
  { value: "cpython", label: "CPython 3.13" },
  { value: "pypy", label: "PyPy 3.11" },
];

export type SettingsView = {
  keyBindings: LoadedKeyBinding[];
  invalidKeyBindings: string[];
  appearance: Appearance;
  submit: SubmitSettings;
  notify: NotifySettings;
  atcoderProblems: AtcoderProblemsSettings;
  standings: StandingsSettings;
  problem: ProblemSettings;
  python: PythonSettings;
  appearanceProblems: string[];
};

// 設定の画面の保存の結果。changedOutside は外で変わっていて書かなかった、cancelled は確認のダイアログを省く設定を
// OS のダイアログでキャンセルした（D-162）
export type SaveSettingsOutcome = "saved" | "changedOutside" | "cancelled";

export type StartupState = {
  layout: Layout | null;
  // 前回の作業領域の名前。Rust 側ですでに作業領域として開いている
  workspaceName: string | null;
  // 前回の作業領域を信頼しているか（D-128。記録がなければ、Rust 側が確認の画面で聞いた結果）
  workspaceTrusted: boolean;
  tabs: string[];
  activeTab: string | null;
  settings: SettingsView;
};

// 起動したときに読む。読めない設定ファイルの退避と、その知らせは Rust 側が行う（D-039）
export function loadAppState(): Promise<StartupState> {
  return invoke<StartupState>("load_app_state");
}

// 窓に戻ったときに、settings.json が変わっていれば読み直す（D-159）。変わっていなければ null。
// 読めないときは誤り（退避はしない。今の設定のまま使う）
export function reloadSettings(): Promise<SettingsView | null> {
  return invoke<SettingsView | null>("reload_settings");
}

// 設定の画面の保存（作業 6b。D-159）。確認のダイアログを省く設定に変えるときは、Rust 側が OS のダイアログで確かめる（D-162）。
// 通知をオンにしたときは、Rust 側がテストの通知を出す（D-239）
export function saveSettings(
  keyBindings: LoadedKeyBinding[],
  appearance: Appearance,
  submit: SubmitSettings,
  notify: NotifySettings,
  atcoderProblems: AtcoderProblemsSettings,
  problem: ProblemSettings,
  interpreter: Interpreter,
  standings: StandingsSettings,
): Promise<SaveSettingsOutcome> {
  return invoke<SaveSettingsOutcome>("save_settings", {
    keyBindings,
    appearance,
    submit,
    notify,
    atcoderProblems,
    problem,
    interpreter,
    standings,
  });
}

export function saveAppState(
  layout: Layout,
  tabs: string[],
  activeTab: string | null,
): Promise<void> {
  return invoke<void>("save_app_state", { layout, tabs, activeTab });
}

// 閉じる・終える前の保存が終わったら呼ぶ（D-084）
export function finishClose(): Promise<void> {
  return invoke<void>("finish_close");
}

// 閉じる・終える前の保存に失敗したら呼ぶ。閉じるのをやめる（D-084）
export function cancelClose(): Promise<void> {
  return invoke<void>("cancel_close");
}
