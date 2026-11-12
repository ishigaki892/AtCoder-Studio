// 問題文のハイライト（4a のあとの追加。D-142）。
// - 重要な値（mod の 998244353・1000000007・10^9+7）：<var> の TeX に、KaTeX の \htmlClass で印（MOD_CLASS）を付ける
// - 「問題文」「制約」の区画の数字と変数：区画に印（SECTION_CLASS）を付け、KaTeX の出力の数字・変数を CSS で色付けする
// 無害化した後の問題文（D-113）の中だけで行い、要素は増やさない（KaTeX が作る要素にクラスを付けるだけ）

import { statementSections } from "./sections";

export const MOD_CLASS = "hl-mod";
export const SECTION_CLASS = "hl-section";

// 色付けする区画の見出し（日本語・英語の問題ページ）
const SECTION_TITLES = new Set(["問題文", "制約", "Problem Statement", "Constraints"]);

// mod によく使う値。前後に数字が続くもの（例：19982443530）は対象にしない
const MOD_VALUES = [
  /(^|[^0-9])(998244353)(?![0-9])/g,
  /(^|[^0-9])(1000000007)(?![0-9])/g,
  /(^|[^0-9])(10\s*\^\s*(?:9|\{\s*9\s*\})\s*\+\s*7)(?![0-9])/g,
];

// TeX の中の mod の値を \htmlClass{hl-mod}{…} で囲む
export function markImportant(tex: string): string {
  let marked = tex;
  for (const pattern of MOD_VALUES) {
    marked = marked.replace(pattern, (_, before: string, value: string) => {
      return `${before}\\htmlClass{${MOD_CLASS}}{${value}}`;
    });
  }
  return marked;
}

// KaTeX の trust：\htmlClass は、自分で付けた印のクラスだけを許す（それ以外の命令は、これまでどおり描かない。D-109）
export function trustOnlyHighlight(context: { command: string; class?: string }): boolean {
  return context.command === "\\htmlClass" && context.class === MOD_CLASS;
}

// 見出しが「問題文」「制約」の区画に印を付ける（区画の見分け方は D-256。古い ABC は div の区画になる）
export function markSections(container: HTMLElement) {
  for (const { heading, section } of statementSections(container)) {
    const title = heading.textContent?.trim() ?? "";
    section.classList.toggle(SECTION_CLASS, SECTION_TITLES.has(title));
  }
}
