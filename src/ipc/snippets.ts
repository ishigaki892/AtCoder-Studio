// スニペット（Rust 側は src-tauri/src/snippets.rs。Phase 13 作業 1。D-186）。
// ファイルは言語ごと（snippets-rust.json・snippets-cpp.json・snippets-python.json。Phase 17 作業 9。D-213。Python は D-227）
import { invoke } from "@tauri-apps/api/core";
import type { Language } from "./workspace";

// template はエディタ（@codemirror/autocomplete の snippet）の書き方に直したもの
export type Snippet = { name: string; prefixes: string[]; template: string; description: string | null };

// problems は読めなかった項目（「名前」と理由）
export type LoadedSnippets = { snippets: Snippet[]; problems: string[] };

// 言語のファイルの名前
export function snippetsFile(language: Language): string {
  return `snippets-${language}.json`;
}

// force でなければ、最後に読んだときから言語のファイルが変わっていないとき null。読めなければ失敗する
export function reloadSnippets(force: boolean, language: Language): Promise<LoadedSnippets | null> {
  return invoke<LoadedSnippets | null>("reload_snippets", { force, language });
}

// VS Code の言語のスニペット（rust.json・cpp.json・python.json）を言語のファイルに写した結果。backup は前のファイルを退避したファイルの名前
export type ImportedSnippets = { cancelled: boolean; count: number; problems: number; backup: string | null };

// 確認のダイアログは Rust 側が出す
export function importVscodeSnippets(language: Language): Promise<ImportedSnippets> {
  return invoke<ImportedSnippets>("import_vscode_snippets", { language });
}
