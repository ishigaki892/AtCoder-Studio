import { useEffect, useRef } from "react";
import { basicSetup } from "codemirror";
import { EditorState } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { indentWithTab } from "@codemirror/commands";
import { acceptCompletion } from "@codemirror/autocomplete";
import { snippetCompletions } from "./snippets";
import { closeReferencePanel } from "@codemirror/lsp-client";
import { cpp } from "@codemirror/lang-cpp";
import { rustLanguage } from "@codemirror/lang-rust";
import { python } from "@codemirror/lang-python";
import { jsonLanguage, jsonParseLinter } from "@codemirror/lang-json";
import { linter } from "@codemirror/lint";
import { HighlightStyle, LanguageSupport, indentUnit, syntaxHighlighting } from "@codemirror/language";
import { styleTags, tags } from "@lezer/highlight";
import { followActiveFile, lspCompartment, withoutLsp } from "./lsp";
import { isAppKey, isReadOnlyKey } from "./readOnlyKey";
import { isCppPath, isPythonPath } from "./language";
import type { Language } from "../ipc/workspace";
import { japanesePhrases } from "./phrases";
import { indentRainbow } from "./indentRainbow";
import { errorLens } from "./errorLens";
import { keepScrollOnClickFocus } from "./clickFocus";
import { publishCursor } from "./breadcrumbTrail";
import {
  breakpointField,
  debugExtensions,
  entriesOf,
  setBreakpoints,
  stoppedLineEffect,
  subscribeBreakpoints,
  syncEffect,
  toggleAt,
} from "./breakpoints";

// エディタの見た目。色は暫定デザイントークン（D-019、src/styles/tokens.css）を使う
const editorTheme = EditorView.theme(
  {
    "&": {
      height: "100%",
      backgroundColor: "var(--bg-base)",
      color: "var(--fg-default)",
      // 文字の大きさは settings.json の editor.fontSize（D-159。既定は 13px）
      fontSize: "var(--font-size-editor)",
    },
    // スニペットの候補の印（Phase 13 作業 1。D-186）
    ".cm-completionIcon-snippet:after": { content: "'✂'" },
    ".cm-scroller": {
      fontFamily: "var(--font-code)",
      // 行の高さは、文字の大きさに合わせる（13px のときに 20px）
      lineHeight: "calc(var(--font-size-editor) * 20 / 13)",
    },
    ".cm-content": {
      caretColor: "var(--fg-strong)",
    },
    ".cm-cursor, .cm-dropCursor": {
      borderLeftColor: "var(--fg-strong)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--bg-base)",
      color: "var(--fg-line-number)",
      border: "none",
    },
    ".cm-activeLine, .cm-activeLineGutter": {
      backgroundColor: "var(--bg-active-line)",
    },
    // CodeMirror の既定のテーマと同じ詳しさのセレクターで、選択範囲の色を上書きする
    ".cm-selectionBackground, &.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground":
      {
        backgroundColor: "var(--bg-selection)",
      },
    ".cm-panels": {
      backgroundColor: "var(--bg-toolbar)",
      color: "var(--fg-default)",
    },
    // ブレークポイントの印と、デバッグで止まった行（Phase 5 作業 2。D-146 2.6）。
    // 押せる幅を 18px にし、印のない行にマウスを乗せると薄い印を出す
    ".cm-breakpoint-gutter .cm-gutterElement": {
      cursor: "pointer",
      width: "18px",
      padding: "0",
      textAlign: "center",
    },
    ".cm-breakpoint-gutter .cm-gutterElement:hover:not(:has(.cm-breakpoint))::before": {
      content: '"●"',
      color: "var(--debug-breakpoint)",
      opacity: "0.35",
    },
    ".cm-breakpoint": {
      color: "var(--debug-breakpoint)",
    },
    ".cm-breakpoint-unverified": {
      opacity: "0.4",
    },
    // インデントの段の色（Phase 8 作業 5。D-158）
    ".cm-indent-rainbow-1": { backgroundColor: "var(--indent-rainbow-1)" },
    ".cm-indent-rainbow-2": { backgroundColor: "var(--indent-rainbow-2)" },
    ".cm-indent-rainbow-3": { backgroundColor: "var(--indent-rainbow-3)" },
    ".cm-indent-rainbow-4": { backgroundColor: "var(--indent-rainbow-4)" },
    // 診断のある行の色と行末のメッセージ（Phase 8 作業 5。D-158）。行の色は背景の画像として重ね、
    // 今いる行の色と合わせて見えるようにする
    ".cm-errorLens-line-error": {
      backgroundImage: "linear-gradient(var(--lens-error-line), var(--lens-error-line))",
    },
    ".cm-errorLens-line-warning": {
      backgroundImage: "linear-gradient(var(--lens-warning-line), var(--lens-warning-line))",
    },
    ".cm-errorLens-message": {
      marginLeft: "2em",
      fontStyle: "italic",
      whiteSpace: "pre",
    },
    ".cm-errorLens-message-error": {
      color: "var(--lens-error)",
    },
    ".cm-errorLens-message-warning": {
      color: "var(--lens-warning)",
    },
    // デバッグで止まった行の色は、診断の行の色より優先する
    ".cm-debug-stopped-line": {
      backgroundColor: "var(--debug-stopped-line)",
      backgroundImage: "none",
    },
  },
  { dark: true },
);

// 構文の色（01 §13.2 のシンタックスハイライトの推定値）。名前空間（use proconio::input の proconio、
// Vec::new の Vec など）は型と同じ色（D-192。VS Code の Dark Modern も同じ色）
const highlightStyle = HighlightStyle.define([
  { tag: tags.keyword, color: "var(--syntax-keyword)" },
  { tag: tags.definitionKeyword, color: "var(--syntax-keyword-definition)" },
  { tag: [tags.typeName, tags.className, tags.namespace], color: "var(--syntax-type)" },
  { tag: tags.number, color: "var(--syntax-number)" },
  { tag: [tags.lineComment, tags.blockComment], color: "var(--syntax-comment)" },
  {
    tag: [tags.function(tags.variableName), tags.function(tags.propertyName)],
    color: "var(--syntax-function)",
  },
  { tag: tags.macroName, color: "var(--syntax-macro)" },
  { tag: [tags.string, tags.character], color: "var(--syntax-string)" },
  { tag: tags.variableName, color: "var(--syntax-variable)" },
]);

// Rust の言語。パスつきの呼び出し（Vec::new の new、std::cmp::max の max）の最後の名前も関数として色を付ける
// （@lezer/rust は、パスのない呼び出しの名前だけを関数とする。D-192）
const rustSupport = new LanguageSupport(
  rustLanguage.configure({
    props: [styleTags({ "CallExpression/ScopedIdentifier/Identifier": tags.function(tags.variableName) })],
  }),
);

// C++ の色分け（D-207）。どのファイルを C++ とするかは language.ts
const cppSupport = cpp();

// Python の色分け（Phase 19 作業 1。D-222）。.py のファイル
const pythonSupport = python();

// JSON の色分け（settings.json・言語のスニペットのファイル。D-247）。キーと true・false・null の色は、JSON にだけ効かせる
// （ほかの言語の true などの色は変えない）。書き方の誤りには印を付ける（崩れている間は、今の設定のまま動くため）
const jsonSupport = new LanguageSupport(jsonLanguage, [
  syntaxHighlighting(
    HighlightStyle.define(
      [
        { tag: [tags.propertyName, tags.definition(tags.propertyName)], color: "var(--syntax-variable)" },
        { tag: [tags.bool, tags.null], color: "var(--syntax-keyword)" },
      ],
      { scope: jsonLanguage },
    ),
  ),
  linter(jsonParseLinter()),
]);

const languageSupport: Record<Language, LanguageSupport> = {
  rust: rustSupport,
  cpp: cppSupport,
  python: pythonSupport,
};

// パスから色分けの言語を決める。C++・Python のファイルでなければ、今までどおり Rust
function languageOfPath(path: string | undefined): Language {
  if (path === undefined) {
    return "rust";
  }
  return isCppPath(path) ? "cpp" : isPythonPath(path) ? "python" : "rust";
}

// タブごとのエディタの状態を作る。拡張（色分け・キー割り当てなど）はすべてのタブで同じ。
// Tab は、補完の一覧が出ていれば確定し、なければインデントする（⇧Tab は戻す）。basicSetup は Tab を割り当てず、
// フォーカスが別のペインに移ってしまうため。エディタから出るには Esc の直後に Tab（D-127）。
// インデントの幅は rustfmt と同じ 4 スペース。
// LSP の拡張（Phase 3b-1）は空で入れておき、開いているタブにだけ付ける（lsp.ts）
// readOnly なら、編集できない（作業領域の外のソースを開く、読み取り専用のタブ。D-133）。
// ブレークポイントのガターは行番号の左に置く（basicSetup より前に入れる）。読み取り専用のタブには置かない（D-146 2.6）。
// snippets なら、その言語のスニペットを補完に出す（.rs・C++・.py のタブ。D-186・D-213・D-227）。path が C++ のファイルなら C++、
// .py なら Python の色分け（D-207・D-222）。
// language は色分けの言語を決めるとき（拡張子のない C++ の標準ヘッダーを読み取り専用で開くとき。D-210）
export function createEditorState(
  text: string,
  options?: { readOnly?: boolean; snippets?: boolean; path?: string; language?: Language },
): EditorState {
  const readOnly = options?.readOnly === true;
  const language = options?.language ?? languageOfPath(options?.path);
  const isJson = options?.path?.endsWith(".json") === true;
  // アプリのフォルダのファイル（D-247）は、デバッグの対象ではないので、ブレークポイントを付けない
  const appFile = options?.path !== undefined && isAppKey(options.path);
  return EditorState.create({
    doc: text,
    extensions: [
      EditorState.readOnly.of(readOnly),
      debugExtensions({ breakpoints: !readOnly && !appFile }),
      // Esc は、参照の一覧（⇧F12）が出ていれば閉じる（なければ何もしない）。F12・⇧F12 はキー割り当ての表で扱う（keybindings.ts）
      keymap.of([
        { key: "Tab", run: acceptCompletion },
        indentWithTab,
        { key: "Escape", run: closeReferencePanel },
      ]),
      indentUnit.of("    "),
      basicSetup,
      // 検索・置換などの画面の文字を日本語にする（Phase 4a 作業 5）
      japanesePhrases,
      isJson ? jsonSupport : languageSupport[language],
      editorTheme,
      syntaxHighlighting(highlightStyle),
      indentRainbow,
      errorLens,
      keepScrollOnClickFocus,
      lspCompartment.of([]),
      options?.snippets === true && !readOnly ? snippetCompletions(language) : [],
    ],
  });
}

type Props = {
  // 開いているタブのファイルのパス。変わったら、そのタブの状態に入れ替える
  activeKey: string | null;
  stateOf: (key: string) => EditorState | undefined;
  // rust-analyzer につなぐか。信頼していない作業領域では false（D-128）
  lspEnabled: boolean;
  // 編集・カーソルの移動などで状態が変わったときに呼ぶ
  onUpdate: (key: string, state: EditorState, docChanged: boolean) => void;
  // エラー一覧・実行ログの場所へ移る要求（Phase 4a 作業 1。D-137）。seq が変わるたびに、
  // そのタブが開いていれば、行（1 始まり）と列（1 始まり。なければ行頭）にカーソルを置いて表示する
  reveal: Reveal | null;
  // デバッグで止まった行（D-146 2.6）。なければ null
  stoppedLine: StoppedLine | null;
};

export type Reveal = { key: string; line: number; column: number | null; seq: number };

// デバッグで止まった行（1 始まり）。そのタブが開いているときに強調する
export type StoppedLine = { key: string; line: number };

// 開いているエディタ（F9 でカーソルの行のブレークポイントを付け外しするのに使う）
let activeEditor: { view: EditorView; key: string } | null = null;

// 開いているエディタとそのタブ（問題文の欄の「input! を作る」で置き換える先。D-195）
export function activeEditorTarget(): { view: EditorView; key: string } | null {
  return activeEditor;
}

// パンくずリストの一覧で選んだ記号へ移る（Phase 18 作業 2。D-216）。開いているタブが key のときだけ
export function moveCursorTo(key: string, pos: number) {
  const editor = activeEditor;
  if (!editor || editor.key !== key) {
    return;
  }
  const at = Math.min(Math.max(pos, 0), editor.view.state.doc.length);
  editor.view.dispatch({ selection: { anchor: at }, effects: EditorView.scrollIntoView(at, { y: "center" }) });
  editor.view.focus();
}

// 文字の大きさを変えたあとに、開いているエディタの寸法を測り直す（D-159）
export function remeasureEditor() {
  activeEditor?.view.requestMeasure();
}

// カーソルの行のブレークポイントを付け外しする（F9。D-146 2.5）。読み取り専用のタブ・開いていないときは何もしない
export function toggleBreakpointAtCursor() {
  const editor = activeEditor;
  if (!editor || isReadOnlyKey(editor.key) || editor.view.state.field(breakpointField, false) === undefined) {
    return;
  }
  editor.view.dispatch({ effects: toggleAt(editor.view.state.selection.main.head) });
}

// コードエディタ（CodeMirror 6）。エディタは 1 つだけ置き、タブを切り替えるときに状態を入れ替える
function Editor({ activeKey, stateOf, lspEnabled, onUpdate, reveal, stoppedLine }: Props) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const keyRef = useRef(activeKey);
  const lspEnabledRef = useRef(lspEnabled);
  const onUpdateRef = useRef(onUpdate);
  onUpdateRef.current = onUpdate;

  // エディタを一度だけ作る
  useEffect(() => {
    if (!containerRef.current) {
      return;
    }
    const view = new EditorView({
      parent: containerRef.current,
      dispatchTransactions: (transactions, view) => {
        const before = view.state.field(breakpointField, false);
        view.update(transactions);
        const key = keyRef.current;
        if (key) {
          const docChanged = transactions.some((tr) => tr.docChanged);
          // 付け外し・編集でブレークポイントが変わったら、保存の場所に書く（D-146 2.6）
          if (!isReadOnlyKey(key) && view.state.field(breakpointField, false) !== before) {
            setBreakpoints(key, entriesOf(view.state), "user");
          }
          onUpdateRef.current(key, view.state, docChanged);
          // パンくずリストに、カーソルの位置と文書を知らせる（D-216）
          publishCursor({ key, state: view.state });
        }
      },
    });
    viewRef.current = view;
    // 保存の場所が変わったら（アダプターが行を動かした・一覧で消したなど）、開いているタブのエディタに入れる
    const unsubscribe = subscribeBreakpoints((path) => {
      if (path === keyRef.current) {
        const effect = syncEffect(view.state, path);
        if (effect) {
          view.dispatch({ effects: effect });
        }
      }
    });
    return () => {
      unsubscribe();
      activeEditor = null;
      view.destroy();
      viewRef.current = null;
    };
  }, []);

  // 開いているタブが変わったら、そのタブの状態に入れ替える
  useEffect(() => {
    const keyChanged = keyRef.current !== activeKey || lspEnabledRef.current !== lspEnabled;
    keyRef.current = activeKey;
    lspEnabledRef.current = lspEnabled;
    const view = viewRef.current;
    if (!view) {
      return;
    }
    const state = activeKey ? stateOf(activeKey) : undefined;
    let replaced = false;
    if (activeKey && state && view.state !== state) {
      // LSP の拡張は外して入れる（前に開いていたときのものが残っていても、別のサーバーに didOpen しないように）。
      // 付け直すのは followActiveFile
      const plain = withoutLsp(state);
      view.setState(plain);
      if (plain !== state) {
        onUpdateRef.current(activeKey, plain, false);
      }
      view.focus();
      replaced = true;
    }
    activeEditor = activeKey ? { view, key: activeKey } : null;
    publishCursor(activeKey ? { key: activeKey, state: view.state } : null);
    // 開いたタブのブレークポイントを、保存の場所に合わせる（タブを閉じている間に変わったものも入れる）
    if (activeKey && replaced) {
      const effect = syncEffect(view.state, activeKey);
      if (effect) {
        view.dispatch({ effects: effect });
      }
    }
    if (keyChanged || replaced) {
      // 信頼していなければ、rust-analyzer につながない（Rust 側でも拒否する）。
      // 読み取り専用のタブ（作業領域の外のソース）も、つながない（D-131 の許可は広げない）
      followActiveFile(view, activeKey, lspEnabled && !(activeKey && isReadOnlyKey(activeKey)));
    }
  }, [activeKey, stateOf, lspEnabled]);

  // 場所へ移る。タブの切り替え（上の効果）のあとに行うため、ここに置く。
  // 行・列が文書の範囲を超えていたら、文書の中に収める
  useEffect(() => {
    const view = viewRef.current;
    if (!reveal || !view || keyRef.current !== reveal.key) {
      return;
    }
    const doc = view.state.doc;
    const line = doc.line(Math.min(Math.max(reveal.line, 1), doc.lines));
    const offset = Math.min(Math.max((reveal.column ?? 1) - 1, 0), line.length);
    const position = line.from + offset;
    view.dispatch({
      selection: { anchor: position },
      effects: EditorView.scrollIntoView(position, { y: "center" }),
    });
    view.focus();
    // 同じ要求は一度だけ使う（seq が変わったときだけ動かす）
  }, [reveal?.seq]);

  // デバッグで止まった行を強調する。別のタブなら、このタブの強調は消す（タブを切り替えたときも合わせる）
  useEffect(() => {
    const view = viewRef.current;
    if (!view || !activeKey) {
      return;
    }
    const line = stoppedLine && stoppedLine.key === activeKey ? stoppedLine.line : null;
    const effect = stoppedLineEffect(view.state, line);
    if (effect) {
      view.dispatch({ effects: effect });
    }
  }, [stoppedLine, activeKey, stateOf]);

  return (
    <div className="editor-stack">
      <div className="editor-host" ref={containerRef} hidden={!activeKey} />
      {!activeKey && <div className="editor-host unimplemented">ファイルが開かれていません</div>}
    </div>
  );
}

export default Editor;
