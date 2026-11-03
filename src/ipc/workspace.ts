// 作業領域のコマンドの呼び出し（Rust 側は src-tauri/src/commands.rs）。
// パスは作業領域のルートからの相対パス（区切りは `/`）。ルートは "" で表す
import { invoke } from "@tauri-apps/api/core";

export type DirEntry = {
  name: string;
  path: string;
  is_dir: boolean;
};

export type OpenedWorkspace = {
  name: string;
  // 信頼している（rust-analyzer・実行・サンプルテストが使える）か。記録がなければ、Rust 側が確認の画面で聞く（D-128）
  trusted: boolean;
};

// フォルダ選択の画面を開く。戻り値は作業領域の名前と信頼の有無。キャンセルなら null
export function openWorkspace(): Promise<OpenedWorkspace | null> {
  return invoke<OpenedWorkspace | null>("open_workspace");
}

// コンパイルエラーや panic のメッセージに出るファイル名（例：src/main.rs）を、作業領域の中のファイルの相対パスにする
// （Phase 4a 作業 1。D-137）。package は実行の対象のパッケージ。作業領域の外・ない場合は null
export function locateSource(pkg: string, file: string): Promise<string | null> {
  return invoke<string | null>("locate_source", { package: pkg, file });
}

// 作業領域の外の、標準ライブラリ・依存クレートのソース（~/.rustup/toolchains・~/.cargo/registry/src の .rs）と、
// C++ の標準ヘッダー・AC Library（Phase 17 作業 5。D-210）を、読み取り専用で読む（Phase 3b-2 作業 5。D-133）。
// uri は言語サーバーが返した file: の URI。許可した場所の外は拒否される。language は色分けに使う
export function readSource(uri: string): Promise<{ content: string; location: string; language: Language }> {
  return invoke("read_source", { uri });
}

// フォルダが見つからない（外で消えた）・フォルダでなくなったときは null（D-156）
export function listDirectory(path: string): Promise<DirEntry[] | null> {
  return invoke<DirEntry[] | null>("list_directory", { path });
}

// ファイルの更新日時（ナノ秒の文字列）と大きさ。外部での変更の検出に使う（D-038）
export type Stamp = { modified: string; size: number };

export function readFile(path: string): Promise<{ content: string; stamp: Stamp }> {
  return invoke("read_file", { path });
}

// expected を渡すと、外部で変更されていたときは書かずに conflict を返す。null なら確かめずに書く
export type WriteResult = { kind: "saved"; stamp: Stamp } | { kind: "conflict" };

export function writeFile(
  path: string,
  content: string,
  expected: Stamp | null,
): Promise<WriteResult> {
  return invoke<WriteResult>("write_file", { path, content, expected });
}

// 開いているファイルの今の更新日時と大きさ。ファイルがなければ null
export function fileStamps(paths: string[]): Promise<(Stamp | null)[]> {
  return invoke<(Stamp | null)[]>("file_stamps", { paths });
}

// アプリのフォルダのファイル（settings.json・言語のスニペットのファイル）を、名前で読み書きする（D-247）。
// 読むときは、なければ既定の中身で作る。書き方・外での変更の確かめは、作業領域のファイルと同じ
export function readAppFile(name: string): Promise<{ content: string; stamp: Stamp }> {
  return invoke("read_app_file", { name });
}

export function writeAppFile(name: string, content: string, expected: Stamp | null): Promise<WriteResult> {
  return invoke<WriteResult>("write_app_file", { name, content, expected });
}

export function appFileStamps(names: string[]): Promise<(Stamp | null)[]> {
  return invoke<(Stamp | null)[]>("app_file_stamps", { names });
}

// 「再読み込み／上書き」を選ぶ画面を Rust 側で出す。再読み込みなら true
export function askExternalChange(name: string): Promise<boolean> {
  return invoke<boolean>("ask_external_change", { name });
}

// 信頼していない作業領域を、信頼し直す（確認の画面は Rust 側が出す。Phase 10 作業 12。D-172）。信頼したら true
export function trustWorkspace(): Promise<boolean> {
  return invoke<boolean>("trust_workspace");
}

// 保存できなかったファイルの変更を捨てるかを聞く（Phase 10 作業 1。D-164）。捨てるなら true
export function askDiscardChanges(paths: string[], purpose: "close" | "quit" | "folder"): Promise<boolean> {
  return invoke<boolean>("ask_discard_changes", { paths, purpose });
}

// 空のファイルを作る。戻り値は作ったファイルの相対パス
export function createNewFile(dir: string, name: string): Promise<string> {
  return invoke<string>("create_new_file", { dir, name });
}

// 作業領域の中のフォルダに、新しいフォルダを作る（D-157）。戻り値は作ったフォルダの相対パス
export function createNewFolder(dir: string, name: string): Promise<string> {
  return invoke<string>("create_new_folder", { dir, name });
}

// ファイル・フォルダの名前を変える（同じフォルダの中だけ。フォルダは D-157）。戻り値は変えた後の相対パス
export function renameExistingFile(path: string, newName: string): Promise<string> {
  return invoke<string>("rename_existing_file", { path, newName });
}

// サンプルテストの実際の出力を、期待出力（sample-N.out）として保存する（Phase 4a 作業 3。D-137）。
// すでに内容の違う .out があるときは、上書きの確認の画面を Rust 側で出す。
// written は書いた、unchanged はすでに同じ内容、cancelled は上書きを断られた
export function saveExpected(
  path: string,
  content: string,
): Promise<"written" | "unchanged" | "cancelled"> {
  return invoke("save_expected", { path, content });
}

// ファイルをゴミ箱に移す。確認の画面は Rust 側が開き、取り消されたら false を返す。
// unsaved が true なら、確認の画面に「保存されていない変更があります」と添える
export function deleteFile(path: string, unsaved: boolean): Promise<boolean> {
  return invoke<boolean>("delete_file", { path, unsaved });
}

// 言語（D-204）。Rust 側の Language と同じ名前
export type Language = "rust" | "cpp" | "python";

// 実行の対象（D-088）。package は作業領域のルートからのパッケージのディレクトリ（ルートなら ""）
export type Target = { package: string; bin: string; language: Language };

// 開いているファイルから実行の対象を決める。決まらなければ null
export function resolveTarget(path: string): Promise<Target | null> {
  return invoke<Target | null>("resolve_target", { path });
}
