// 自作のライブラリの一覧と挿入（Rust 側は src-tauri/src/library.rs。Phase 46。D-271）
import { invoke } from "@tauri-apps/api/core";
import type { Language } from "./workspace";

// 中の名前。path はアプリのフォルダからの相対パス、line は 1 始まり。insert は、この名前だけを入れる行（Rust）
export type LibraryItem = { name: string; kind: string; path: string; line: number; insert: string | null };

export type Library = {
  language: Language;
  group: "own" | "acLibrary";
  // 見せる名前（C++ は libraries/cpp/ からの相対パス、Rust はクレート、Python はモジュール）
  name: string;
  // 開くファイル（アプリのフォルダからの相対パス）。AC Library は uri（file: の URI。読み取り専用のタブで開く）
  path: string;
  uri: string | null;
  description: string | null;
  items: LibraryItem[];
  // 入れる行
  insert: string;
  // Rust：Cargo.toml に足す依存の名前
  crate: string | null;
  head: string;
  skipped: boolean;
};

export type Libraries = { libraries: Library[]; truncated: boolean };

export function listLibraries(): Promise<Libraries> {
  return invoke<Libraries>("list_libraries");
}

// text のどこ（0 始まりの行の前）に line を入れるか。already なら同じ行がもうある
export function planLibraryInsert(
  language: Language,
  text: string,
  line: string,
): Promise<{ already: boolean; line: number }> {
  return invoke("plan_library_insert", { language, text, line });
}

// path（作業領域の .rs）の Cargo.toml に、krate の依存がなければ確かめてから足す（確かめの窓は Rust 側が出す）
export function addLibraryDependency(
  path: string,
  krate: string,
): Promise<{ status: "added" | "present" | "cancelled"; manifest: string }> {
  return invoke("add_library_dependency", { path, krate });
}
