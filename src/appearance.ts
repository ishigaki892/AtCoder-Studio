// 見た目の設定（settings.json の colors・editor・problem。Phase 8 作業 6。D-159）を画面に当てる。
//
// - 色・文字の大きさは、tokens.css の :root の変数を body で上書きする（:root の既定の値は残るので、外せば既定に戻る）
// - 表示の入切は、body に印のクラスを付け、App.css で消す（エディタの状態は作り直さない）
import type { Appearance } from "./ipc/state";

// 変えられる色の変数（tokens.css の色の変数。先頭の -- を除く）と、設定の画面（作業 6b）で見せる名前・分類
export const COLOR_GROUPS: { title: string; items: { name: string; label: string }[] }[] = [
  {
    title: "画面",
    items: [
      { name: "bg-base", label: "背景" },
      { name: "bg-toolbar", label: "ツールバーの背景" },
      { name: "bg-activitybar", label: "アクティビティバーの背景" },
      { name: "bg-sidebar", label: "ファイル一覧の背景" },
      { name: "fg-default", label: "文字" },
      { name: "fg-strong", label: "強い文字" },
      { name: "fg-secondary", label: "控えめな文字" },
      { name: "fg-muted", label: "薄い文字" },
      { name: "accent-focus", label: "強調（選んだもの・リンク）" },
      { name: "bg-list-selected", label: "選んだ行の背景" },
      { name: "bg-tab-active", label: "開いているタブの背景" },
      { name: "border", label: "境界線" },
    ],
  },
  {
    title: "エディタ",
    items: [
      { name: "fg-line-number", label: "行番号" },
      { name: "bg-active-line", label: "今いる行" },
      { name: "bg-selection", label: "選択範囲" },
    ],
  },
  {
    title: "構文",
    items: [
      { name: "syntax-keyword", label: "キーワード" },
      { name: "syntax-keyword-definition", label: "定義のキーワード（fn・let など）" },
      { name: "syntax-type", label: "型・名前空間" },
      { name: "syntax-number", label: "数" },
      { name: "syntax-comment", label: "コメント" },
      { name: "syntax-function", label: "関数" },
      { name: "syntax-macro", label: "マクロ" },
      { name: "syntax-string", label: "文字列" },
      { name: "syntax-variable", label: "変数" },
    ],
  },
  {
    title: "テストの判定",
    items: [
      { name: "verdict-ac", label: "AC" },
      { name: "verdict-ng", label: "AC 以外" },
    ],
  },
  {
    title: "問題文",
    items: [
      { name: "hl-number", label: "数字" },
      { name: "hl-variable", label: "変数" },
      { name: "hl-mod", label: "mod の値" },
      { name: "hl-mod-bg", label: "mod の値の背景" },
      { name: "problem-image-bg", label: "画像の背景" },
    ],
  },
  {
    title: "検索",
    items: [{ name: "search-hit", label: "検索の当たり" }],
  },
  {
    title: "デバッグ",
    items: [
      { name: "debug-breakpoint", label: "ブレークポイント" },
      { name: "debug-stopped-line", label: "止まった行" },
    ],
  },
  {
    title: "ファイル一覧",
    items: [
      { name: "tree-guide", label: "インデントの線" },
      { name: "tree-guide-active", label: "インデントの線（カーソルのあるところ）" },
    ],
  },
  {
    title: "アイコン",
    items: [
      { name: "icon-folder", label: "フォルダ" },
      { name: "icon-rust", label: "Rust（.rs）" },
      { name: "icon-cpp", label: "C++（.cpp）" },
      { name: "icon-header", label: "ヘッダー（.h・.hpp）" },
      { name: "icon-cargo", label: "Cargo.toml" },
      { name: "icon-config", label: "設定（.toml）" },
      { name: "icon-lock", label: ".lock" },
      { name: "icon-doc", label: "文書（.md）" },
      { name: "icon-json", label: "JSON" },
      { name: "icon-input", label: "入力（.in・input.txt）" },
      { name: "icon-output", label: "出力（.out）" },
      { name: "icon-text", label: "テキスト（.txt）" },
      { name: "icon-file", label: "そのほかのファイル" },
    ],
  },
  {
    title: "インデントと診断",
    items: [
      { name: "indent-rainbow-1", label: "インデント 1 段目" },
      { name: "indent-rainbow-2", label: "インデント 2 段目" },
      { name: "indent-rainbow-3", label: "インデント 3 段目" },
      { name: "indent-rainbow-4", label: "インデント 4 段目" },
      { name: "lens-error", label: "エラーのメッセージ" },
      { name: "lens-error-line", label: "エラーの行" },
      { name: "lens-warning", label: "警告のメッセージ" },
      { name: "lens-warning-line", label: "警告の行" },
    ],
  },
];

const COLOR_NAMES = new Set(COLOR_GROUPS.flatMap((group) => group.items.map((item) => item.name)));

// 既定の色（tokens.css の :root の値。上書きは body にあるので、:root を見れば既定が分かる）
export function defaultColor(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(`--${name}`).trim();
}

// 色を、色を選ぶ欄の #rrggbb と、濃さ（0〜100）に分ける。読めなければ null。
// 読むのは canvas に任せる（CSS の色の書き方を、#rrggbb か rgba(…) にそろえて返す）
let colorContext: CanvasRenderingContext2D | null = null;
export function splitColor(value: string): { hex: string; alpha: number } | null {
  if (!CSS.supports("color", value)) {
    return null;
  }
  colorContext ??= document.createElement("canvas").getContext("2d");
  if (!colorContext) {
    return null;
  }
  colorContext.fillStyle = "#000000";
  colorContext.fillStyle = value;
  const normalized = String(colorContext.fillStyle);
  if (normalized.startsWith("#")) {
    return { hex: normalized, alpha: 100 };
  }
  const match = /^rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)$/.exec(normalized);
  if (!match) {
    return null;
  }
  const hex = `#${[match[1], match[2], match[3]].map((part) => Number(part).toString(16).padStart(2, "0")).join("")}`;
  return { hex, alpha: Math.round(Number(match[4] ?? "1") * 100) };
}

// #rrggbb と濃さから、CSS の色を作る。濃さが 100 なら #rrggbb のまま
export function joinColor(hex: string, alpha: number): string {
  if (alpha >= 100) {
    return hex;
  }
  const [r, g, b] = [1, 3, 5].map((start) => parseInt(hex.slice(start, start + 2), 16));
  return `rgba(${r}, ${g}, ${b}, ${Math.max(alpha, 0) / 100})`;
}

// 今 body で上書きしている色の変数（読み直したときに、書かれなくなったものを外すため）
let appliedColors = new Set<string>();

// 見た目の設定を当てる。使えなかった色（知らない名前・色として読めない値）を返す
export function applyAppearance(appearance: Appearance): string[] {
  const style = document.body.style;
  const problems: string[] = [];
  const next = new Set<string>();
  for (const [name, value] of Object.entries(appearance.colors)) {
    if (!COLOR_NAMES.has(name)) {
      problems.push(`colors の「${name}」（知らない名前）`);
    } else if (!CSS.supports("color", value)) {
      problems.push(`colors の「${name}」（色として読めない値）`);
    } else {
      style.setProperty(`--${name}`, value);
      next.add(name);
    }
  }
  for (const name of appliedColors) {
    if (!next.has(name)) {
      style.removeProperty(`--${name}`);
    }
  }
  appliedColors = next;
  style.setProperty("--font-size-editor", `${appearance.editorFontSize}px`);
  style.setProperty("--font-size-problem", `${appearance.problemFontSize}px`);
  document.body.classList.toggle("hide-indent-rainbow", !appearance.indentRainbow);
  document.body.classList.toggle("hide-error-lens", !appearance.errorLens);
  document.body.classList.toggle("hide-breadcrumbs", !appearance.breadcrumbs);
  return problems;
}
