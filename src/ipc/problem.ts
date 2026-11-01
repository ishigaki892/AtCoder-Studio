// 問題の読み込み（Rust 側は src-tauri/src/commands.rs の load_problem と src-tauri/src/problem.rs）
import { invoke } from "@tauri-apps/api/core";
import type { Language, Target } from "./workspace";

export type ProblemId = { contest: string; task: string };

export type ProblemView = {
  contest: string;
  task: string;
  url: string;
  title: string;
  timeLimit: string;
  memoryLimit: string;
  // 無害化した問題文の HTML（D-113・D-118）
  statementHtml: string;
  sampleCount: number;
  // 保存していない画像がある（画像の所は「読み込んでいます」。loadProblemImages で取得して差し替える。D-240）
  pendingImages: boolean;
};

// 保存してあればそれを返し、なければ（または refresh のとき）取得する（D-110〜D-112）
export function loadProblem(id: ProblemId, refresh: boolean): Promise<ProblemView> {
  return invoke<ProblemView>("load_problem", { ...id, refresh });
}

// 保存していない問題文の画像を取得して（img.atcoder.jp だけ。1 秒の間隔）、無害化した問題文の HTML を返す（Phase 23。D-240）
export function loadProblemImages(id: ProblemId): Promise<string> {
  return invoke<string>("load_problem_images", { ...id });
}

// 実行の対象のパッケージのフォルダから決まる問題（D-117・D-155）。決まらなければ null。
// コンテスト ID は、親のフォルダ名（保存した問題の一覧か名前で確かめたとき）か、最後の「_」より前（Rust 側で決める）
export function packageProblem(packagePath: string): Promise<ProblemId | null> {
  return invoke<ProblemId | null>("package_problem", { package: packagePath });
}

// コンテストの問題の一覧の 1 行（D-117）
export type TaskEntry = { task: string; label: string; title: string };

// コンテストの問題の一覧。保存してあればそれを使う（D-111）
export function loadTaskList(contest: string, refresh: boolean): Promise<TaskEntry[]> {
  return invoke<TaskEntry[]>("load_task_list", { contest, refresh });
}

// 公式の問題ページをブラウザで開く（D-110 の代替手段）
export function openProblemPage(id: ProblemId): Promise<void> {
  return invoke<void>("open_problem_page", { ...id });
}

// パッケージで選んだ問題（D-117）。なければ null
export function problemChoice(packagePath: string): Promise<ProblemId | null> {
  return invoke<ProblemId | null>("problem_choice", { package: packagePath });
}

export function setProblemChoice(packagePath: string, id: ProblemId): Promise<void> {
  return invoke<void>("set_problem_choice", { package: packagePath, ...id });
}

// サンプルを保存した結果（D-112）。ファイル名は folder（パッケージのディレクトリからの相対パス）の中
export type SavedSamples = {
  folder: string;
  written: string[];
  unchanged: string[];
  skipped: string[];
  // 問題のサンプルの数より大きい番号の sample-N（消さずに知らせる）
  leftover: string[];
};

// サンプルを、実行の対象のテストケースのフォルダ（D-106）に書く。すでにあるファイルは上書きしない
export function saveSamples(target: Target, id: ProblemId): Promise<SavedSamples> {
  return invoke<SavedSamples>("save_samples", { target, ...id });
}

// 問題文から作った input! の中身（Phase 15。D-194。Rust 側は src-tauri/src/input_gen.rs）。
// C++ では、宣言と cin の行（Phase 17 作業 8。D-213）
export type GeneratedInput = {
  // 作った言語（問題のパッケージの言語か、設定の「問題を作るときの言語」）
  language: Language;
  // solve の input! { } の中の行（例：`n: usize,`）。C++ なら solve() に書く行（例：`long long N;`）
  solve: string[];
  // テストケースが複数のとき、main の input! { } の中の行（例：`t: usize,`）。C++ なら main() に書く行
  main: string[] | null;
  // 注意（作れなかった部分・決められなかった型・入力例と合わないなど）
  notes: string[];
  // 形式を最後まで読めたか
  complete: boolean;
  samplesChecked: number;
  samplesMatched: number;
};

// 保存してある問題から input! の中身を作る（取得し直さない）
export function makeInput(id: ProblemId): Promise<GeneratedInput> {
  return invoke<GeneratedInput>("make_input", { ...id });
}

// C++ のファイルの中身 text の solve()（なければ main()）に、作った入力のコードを目印で囲んで書いた結果（D-213）。
// ファイルは読み書きしない（画面がエディタの文書を置き換える）
export function fillCppInput(
  text: string,
  generated: GeneratedInput,
  mainLoop: boolean,
): Promise<{ text: string; message: string }> {
  return invoke("fill_cpp_input", { text, solve: generated.solve, main: generated.main, mainLoop });
}

// Python のファイルの中身 text の solve()（なければ main()）に、作った入力のコードを目印で囲んで書いた結果（D-227）。
// ファイルは読み書きしない（画面がエディタの文書を置き換える）
export function fillPythonInput(
  text: string,
  generated: GeneratedInput,
  mainLoop: boolean,
): Promise<{ text: string; message: string }> {
  return invoke("fill_python_input", { text, solve: generated.solve, main: generated.main, mainLoop });
}
