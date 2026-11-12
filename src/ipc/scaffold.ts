// 問題・コンテストのパッケージを作る（Phase 7a。D-151。Rust 側は src-tauri/src/scaffold.rs）
import { Channel, invoke } from "@tauri-apps/api/core";
import type { ProblemId } from "./problem";
import type { Target } from "./workspace";

// 1 問のパッケージを作った結果。パスは作業領域のルートからの相対パス
export type PackageResult = {
  task: string;
  // パッケージのフォルダ（例：abc480/abc480_a）
  path: string;
  // 新しく作ったファイル
  created: string[];
  // すでにあったので、そのままにしたファイル
  existed: string[];
  // すでにあって中身が違うので、書かなかったサンプル
  differed: string[];
  // 作れなかった理由。作れたら null
  error: string | null;
  // input! を入れたか（Phase 15 作業 4。D-198。入れる設定で main.rs を新しく書いたときだけ）
  input: string | null;
  // 生成器の知らせ（読めなかったところ・満たしていない制約。生成器を新しく書いたときだけ。Phase 35。D-259）
  generator: string | null;
  // 作ったあとに開くファイル（例：abc480/abc480_a/src/main.rs、C++ なら abc480/abc480_a/main.cpp、Python なら main.py）
  main: string;
};

export type ProblemCreated = {
  // 作った .cargo/config.toml（すでにあれば空）
  configCreated: string[];
  package: PackageResult;
};

export type ContestCreated = {
  contest: string;
  configCreated: string[];
  // 問題ごとの結果（一覧の順。止めたら、そこまで）
  packages: PackageResult[];
  // 一覧の問題の数
  total: number;
  // 途中で止めた
  stopped: boolean;
};

// コンテストを作っている間の知らせ。index は 0 から
export type ScaffoldEvent =
  | { type: "listed"; total: number }
  | { type: "started"; index: number; total: number; task: string }
  | { type: "finished"; result: PackageResult };

// 選択欄で選んだ問題のパッケージを作る。言語は settings.json の problem.language（Phase 16 作業 3。D-208）。
// すでにあるファイルは変えない
export function createProblemPackage(id: ProblemId): Promise<ProblemCreated> {
  return invoke<ProblemCreated>("create_problem_package", { ...id });
}

// コンテストの問題をすべて作る。終わるまで（止めたら、そこまでで）返らない。経過は onEvent で受け取る
// 言語は settings.json の problem.language（D-208）
export function createContestPackages(
  contest: string,
  onEvent: (event: ScaffoldEvent) => void,
): Promise<ContestCreated> {
  const channel = new Channel<ScaffoldEvent>();
  channel.onmessage = onEvent;
  return invoke<ContestCreated>("create_contest_packages", { contest, onEvent: channel });
}

// コンテストの問題を作るのを止める（今の問題を取り終えたところで止まる）
export function stopScaffold(): Promise<void> {
  return invoke<void>("stop_scaffold");
}

// 作ったパッケージで開くファイル（Rust は src/main.rs、C++ は main.cpp、Python は main.py）
export function mainFile(result: PackageResult): string {
  return result.main;
}

// 1 問の結果を短く説明する（例：「ファイルを 8 件作りました」）
export function describePackage(result: PackageResult): string {
  if (result.error !== null) {
    return `作れませんでした：${result.error}`;
  }
  const parts: string[] = [];
  if (result.created.length > 0) {
    parts.push(`ファイルを ${result.created.length} 件作りました`);
  }
  if (result.existed.length > 0) {
    parts.push(`${result.existed.length} 件はすでにありました`);
  }
  if (result.differed.length > 0) {
    const names = result.differed.map((file) => file.slice(file.lastIndexOf("/") + 1));
    parts.push(`${names.join("、")} は中身が違うので書いていません`);
  }
  if (result.input !== null) {
    parts.push(result.input);
  }
  if (result.generator !== null) {
    parts.push(result.generator);
  }
  return parts.join("。");
}

// 雛形のファイル（アプリのフォルダの templates/main.rs・templates/dependencies.toml・main.cpp・main.py）があるか
// （Phase 13 作業 3。D-188。Python は Phase 19 作業 3。D-223）
export type TemplateFiles = { mainRs: boolean; dependencies: boolean; mainCpp: boolean; mainPy: boolean };

export function templateFiles(): Promise<TemplateFiles> {
  return invoke<TemplateFiles>("template_files");
}

// テンプレートのフォルダを Finder で開く（なければ空のフォルダを作る）
export function openTemplatesFolder(): Promise<void> {
  return invoke("open_templates_folder");
}

// 自作の C++ のライブラリのフォルダ（libraries/cpp/）を Finder で開く（なければ空のフォルダを作る。D-248）
export function openLibraryFolder(): Promise<void> {
  return invoke("open_library_folder");
}

// 自作の Rust のライブラリのフォルダ（libraries/rust/）を Finder で開く（なければ空のフォルダを作る。D-251）
export function openRustLibraryFolder(): Promise<void> {
  return invoke("open_rust_library_folder");
}

// 自作の Python のライブラリのフォルダ（libraries/python/）を Finder で開く（なければ空のフォルダを作る。D-252）
export function openPythonLibraryFolder(): Promise<void> {
  return invoke("open_python_library_folder");
}

// 自作の Rust のライブラリを問題から読む、写して使う依存の行（D-251）。dir は libraries/rust/ の場所
export type RustLibraryLines = { dir: string; lines: string[] };

export function rustLibraryLines(): Promise<RustLibraryLines> {
  return invoke<RustLibraryLines>("rust_library_lines");
}

// 「コードをコピー」でコピーする中身。C++ なら `#include "…"` を展開したもの（D-248）。
// expanded は展開したヘッダー、missing は見つからなかったもの
export type CopySource = { content: string; expanded: string[]; missing: string[] };

export function copySource(path: string): Promise<CopySource> {
  return invoke<CopySource>("copy_source", { path });
}

// 白紙から解き直した結果（Phase 34 作業 3。D-258）。パスは作業領域のルートからの相対パス
export type Restarted = {
  // 書き直した main（例：abc480/abc480_a/src/main.rs）
  main: string;
  // 前の main を移した先（例：abc480/abc480_a/old/main-20261112-153000.rs）
  old: string;
  // 入力のコードについての知らせ（入れる設定のときだけ）
  input: string | null;
};

// 白紙から解き直す前の確認のダイアログ（Rust 側が出す）。「解き直す」なら true
export function askRestartPackage(target: Target): Promise<boolean> {
  return invoke<boolean>("ask_restart_package", { target });
}

// target のパッケージの main を old/ に移し、雛形で書き直す（確認は askRestartPackage で済ませておく）
export function restartPackage(target: Target, id: ProblemId): Promise<Restarted> {
  return invoke<Restarted>("restart_package", { target, contest: id.contest, task: id.task });
}

// 生成器と愚直解を作った結果（Phase 35 作業 2。D-259）。パスは作業領域のルートからの相対パス
export type RandomPrograms = { created: string[]; existed: string[]; note: string | null };

// target のパッケージの examples/ に、生成器（gen）と愚直解（naive）を、なければ書く
export function createRandomPrograms(target: Target, id: ProblemId): Promise<RandomPrograms> {
  return invoke<RandomPrograms>("create_random_programs", { target, contest: id.contest, task: id.task });
}
