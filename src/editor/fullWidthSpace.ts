// 全角スペース（U+3000）に印を付ける（D-274）
//
// - 文字はそのまま残し、点線の枠と薄い背景で囲む（幅は変えない）。色は tokens.css の --fullwidth-space・--fullwidth-space-bg
// - 印を付けるのは画面に見えている範囲だけ（MatchDecorator。大きなファイルでも重くしないため）
// - すべてのタブ（読み取り専用・JSON・テキストを含む）で付ける。マウスを重ねると「全角スペース」と出る
import { Decoration, MatchDecorator, ViewPlugin, type DecorationSet, type EditorView, type ViewUpdate } from "@codemirror/view";

const mark = Decoration.mark({ class: "cm-fullwidth-space", attributes: { title: "全角スペース" } });

const matcher = new MatchDecorator({ regexp: /　/g, decoration: () => mark });

export const fullWidthSpace = ViewPlugin.fromClass(
  class {
    decorations: DecorationSet;

    constructor(view: EditorView) {
      this.decorations = matcher.createDeco(view);
    }

    update(update: ViewUpdate) {
      this.decorations = matcher.updateDeco(update, this.decorations);
    }
  },
  { decorations: (plugin) => plugin.decorations },
);
