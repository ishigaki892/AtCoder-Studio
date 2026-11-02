import { useEffect, useRef, useState } from "react";
import { difficultyHint, difficultyText, problemDifficulties, type DifficultyView } from "../ipc/atcoderProblems";
import { ratingClass } from "../rating";
import katex from "katex";
import renderMathInElement from "katex/contrib/auto-render";
import "katex/dist/katex.min.css";
import {
  fillCppInput,
  fillPythonInput,
  loadProblem,
  loadProblemImages,
  makeInput,
  openEditorialPage,
  openProblemPage,
  type GeneratedInput,
  saveSamples,
  type ProblemId,
  type ProblemView,
  type SavedSamples,
} from "../ipc/problem";
import { createProblemPackage, describePackage, mainFile } from "../ipc/scaffold";
import type { Target } from "../ipc/workspace";
import { PROBLEM_BUTTONS, type ProblemSettings } from "../ipc/state";
import { UNTRUSTED } from "./runState";
import { copyText } from "./copyText";
import { markImportant, markSections, trustOnlyHighlight } from "./highlight";
import { activeEditorTarget } from "../editor/Editor";
import { replaceDocument, replaceInputMacros } from "../editor/inputMacro";
import { isCppPath, isPythonPath } from "../editor/language";
import type { Language } from "../ipc/workspace";
import { isReadOnlyKey } from "../editor/readOnlyKey";

// 問題文表示ゾーン（原典 §4.6。Phase 3a 作業 3）

type State =
  | { kind: "none" }
  | { kind: "loading"; id: ProblemId }
  | { kind: "error"; id: ProblemId; message: string }
  | { kind: "ok"; problem: ProblemView };

// KaTeX の設定（D-109）。危ないコマンドは描かず、誤りは文字のまま出す。
// ハイライトの印（\htmlClass のうち自分で付けたクラス）だけは許す（D-142）。HTML の拡張は strict の警告から外す
const KATEX_OPTIONS = {
  throwOnError: false,
  trust: trustOnlyHighlight,
  strict: (code: string) => (code === "htmlExtension" ? "ignore" : "warn"),
} as const;

// AtCoder と同じ区切り（docs/phase3a/01-pre-start-research.md §1）
const DELIMITERS = [
  { left: "$$", right: "$$", display: true },
  { left: "\\(", right: "\\)", display: false },
  { left: "\\[", right: "\\]", display: true },
];

// 問題文の数式を描く。<var> の中は TeX として描き（区切りがあれば外す）、
// それ以外の文の中の区切り（\( \) など）は auto-render で描く
function renderMath(container: HTMLElement) {
  for (const element of container.querySelectorAll("var")) {
    // 描いた後の中身（MathML と HTML）を TeX として描き直さないように、元の TeX を覚えておく。
    // 開発のときの React（StrictMode）は、描画の後の処理を 2 回呼ぶ
    const html = element as HTMLElement;
    html.dataset.tex ??= html.textContent ?? "";
    let tex = html.dataset.tex;
    const inline = tex.match(/^\s*\\\(([\s\S]*)\\\)\s*$/);
    if (inline) {
      tex = inline[1];
    }
    // mod の値に印を付けてから描く（D-142）
    katex.render(markImportant(tex), html, KATEX_OPTIONS);
  }
  // 「問題文」「制約」の区画の数字と変数を色付けする（D-142）
  markSections(container);
  renderMathInElement(container, {
    ...KATEX_OPTIONS,
    delimiters: DELIMITERS,
    ignoredTags: ["script", "noscript", "style", "textarea", "code", "option", "var"],
  });
}

// 見出しが「入力例 N」「出力例 N」の区画（Rust 側がサンプルを読む区画と同じ。problem.rs）
const SAMPLE_HEADING = /^(入力例|出力例)\s*\d+$/;

// 入力例・出力例の見出しの右に「コピー」ボタンを足す（Phase 11 作業 1。D-177）。
// 中身は数式を描く前に読んでおく。ボタンは画面側で作る要素で、問題文の HTML（Rust 側の無害化）には足さない
function addCopyButtons(container: HTMLElement) {
  for (const section of container.querySelectorAll("section")) {
    const heading = section.querySelector("h3");
    const pre = section.querySelector("pre");
    // 開発のときの React（StrictMode）は描画の後の処理を 2 回呼ぶので、ボタンがあれば足さない
    if (!heading || !pre || heading.querySelector(".sample-copy")) {
      continue;
    }
    if (!SAMPLE_HEADING.test(heading.textContent?.trim() ?? "")) {
      continue;
    }
    const text = (pre.textContent ?? "").replace(/\r\n/g, "\n");
    const button = document.createElement("button");
    button.type = "button";
    button.className = "sample-copy";
    button.textContent = "コピー";
    let timer: ReturnType<typeof setTimeout> | null = null;
    button.addEventListener("click", () => {
      void copyText(text).then((copied) => {
        // 「コピーしました」を少しの間だけ出す（実行ログのコピーと同じ文と長さ）
        button.textContent = copied ? "コピーしました" : "コピーできませんでした";
        if (timer !== null) {
          clearTimeout(timer);
        }
        timer = setTimeout(() => {
          button.textContent = "コピー";
        }, 2000);
      });
    });
    heading.appendChild(button);
  }
}

function Statement({ html }: { html: string }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (ref.current) {
      addCopyButtons(ref.current);
      renderMath(ref.current);
    }
  }, [html]);
  // html は Rust 側で無害化してある（D-113・D-118）
  return <div ref={ref} className="problem-statement" dangerouslySetInnerHTML={{ __html: html }} />;
}

// 保存の結果を 1 行で説明する
function describeSaved(saved: SavedSamples): string {
  const folder = saved.folder || "（パッケージの直下）";
  const parts = [`${folder} に ${saved.written.length} 件書きました`];
  if (saved.unchanged.length > 0) {
    parts.push(`${saved.unchanged.length} 件は同じ内容なのでそのままです`);
  }
  if (saved.skipped.length > 0) {
    parts.push(`${saved.skipped.join("、")} は内容が違うので上書きしていません`);
  }
  if (saved.leftover.length > 0) {
    parts.push(
      `問題のサンプルより大きい番号の ${saved.leftover.join("、")} があります（自分で足したケースなら、そのままで構いません）`,
    );
  }
  return parts.join("。");
}

// 作った input! を、そのまま貼れる形にする（テストケースが複数なら main() と solve() の 2 つ。D-195）
function inputBlock(lines: string[]) {
  return ["input! {", ...lines.map((line) => `    ${line}`), "}"].join("\n");
}

// 作るものの名前の付いた文（Rust は input!、C++・Python は入力のコード。D-213・D-227）
function inputName(language: Language) {
  return language === "rust" ? "input! " : "入力のコード";
}

function inputText(generated: GeneratedInput) {
  if (generated.language === "python") {
    if (!generated.main) {
      return generated.solve.join("\n");
    }
    const count = generated.main[0]?.split(" = ")[0] || "T";
    return [
      "# main()",
      ...generated.main,
      `for _ in range(${count}):`,
      "    solve()",
      "",
      "# solve()",
      ...generated.solve,
    ].join("\n");
  }
  if (generated.language === "cpp") {
    if (!generated.main) {
      return generated.solve.join("\n");
    }
    const count = generated.main[0]?.replace(/;$/, "").split(" ").pop() || "T";
    return [
      "// main()",
      ...generated.main,
      `for (int i = 0; i < ${count}; i++) solve();`,
      "",
      "// solve()",
      ...generated.solve,
    ].join("\n");
  }
  if (!generated.main) {
    return inputBlock(generated.solve);
  }
  const count = generated.main[0]?.split(":")[0] ?? "t";
  return [
    "// main()",
    inputBlock(generated.main),
    `for _ in 0..${count} {`,
    "    solve();",
    "}",
    "",
    "// solve()",
    inputBlock(generated.solve),
  ].join("\n");
}

function samplesText(generated: GeneratedInput) {
  if (generated.samplesChecked === 0) {
    return "入力例がないので確かめていません";
  }
  return `入力例 ${generated.samplesChecked} 件のうち ${generated.samplesMatched} 件と合います`;
}

type Props = {
  id: ProblemId | null;
  // 実行の対象（サンプルを書く先のパッケージとバイナリ）。なければ保存できない
  target: Target | null;
  // 作業領域を信頼していない（D-128）。対象が決まらない理由として、信頼していないことを示す
  untrusted: boolean;
  // サンプルを書いた後に呼ぶ（ファイル一覧を読み直す）
  onSamplesSaved: () => void;
  // 「この問題を作る」を出す（選択欄で選んだ問題のとき。パッケージのフォルダから決まった問題には出さない。D-151。
  // 紐付けを外しているときは出す。D-178）
  canCreate: boolean;
  // 問題のパッケージを作った後に呼ぶ（ファイル一覧を読み直し、main を開く）
  onPackageCreated: (main: string) => void;
  // 見出しの行に出すボタンと、main() に for まで書くか（Phase 15 作業 3。D-196・D-197）
  settings: ProblemSettings;
};

function ProblemPane({
  id,
  target,
  untrusted,
  onSamplesSaved,
  canCreate,
  onPackageCreated,
  settings,
}: Props) {
  const [state, setState] = useState<State>({ kind: "none" });
  // 「サンプルを保存」の結果の知らせ
  const [notice, setNotice] = useState<string | null>(null);
  const key = id ? `${id.contest}/${id.task}` : null;
  // 「再取得」「もう一度試す」を押した回数。増えたら取得し直す（D-111：利用者の操作のときだけ）
  const [refreshCount, setRefreshCount] = useState(0);
  const [refreshKey, setRefreshKey] = useState<string | null>(null);
  // 「この問題を作る」を押してから終わるまで
  const [creating, setCreating] = useState(false);
  // 「input! を作る」の結果と、置き換え・コピーの知らせ（Phase 15 作業 2。D-195）
  const [made, setMade] = useState<{ generated: GeneratedInput; message: string | null } | null>(null);
  const [making, setMaking] = useState(false);
  // 「…」の一覧（再取得・公式ページ・この問題を作る）が開いているか（ユーザーの依頼、2026-10-28）
  const [moreOpen, setMoreOpen] = useState(false);

  // 一覧の外を押したら・Esc で閉じる（ファイル一覧の右クリックのメニューと同じ）
  useEffect(() => {
    if (!moreOpen) {
      return;
    }
    const close = () => setMoreOpen(false);
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setMoreOpen(false);
      }
    };
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [moreOpen]);

  // 問題の AtCoder Problems の diff（Phase 24。D-241）。問題が変わったときだけ聞く。ないとき・読めないときは出さない
  const [difficulty, setDifficulty] = useState<{ key: string; view: DifficultyView } | null>(null);
  useEffect(() => {
    if (!id || key === null) {
      return;
    }
    const shownKey = key;
    let cancelled = false;
    problemDifficulties([id.task])
      .then((found) => {
        const view = found[id.task];
        if (!cancelled) {
          setDifficulty(view ? { key: shownKey, view } : null);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setDifficulty(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [key]);

  // 問題が変わったら読み込む。保存してあれば取得しない（D-111）
  useEffect(() => {
    if (!id) {
      setState({ kind: "none" });
      return;
    }
    let cancelled = false;
    setNotice(null);
    setMade(null);
    const refresh = refreshKey === `${key}#${refreshCount}`;
    setState({ kind: "loading", id });
    loadProblem(id, refresh)
      .then((problem) => {
        if (cancelled) {
          return;
        }
        setState({ kind: "ok", problem });
        // 文を先に出し、保存していない画像はあとから取得して差し替える（Phase 23。D-240）
        if (problem.pendingImages) {
          loadProblemImages(id)
            .then((statementHtml) => {
              if (!cancelled) {
                setState((current) =>
                  current.kind === "ok" &&
                  current.problem.contest === problem.contest &&
                  current.problem.task === problem.task
                    ? { kind: "ok", problem: { ...current.problem, statementHtml, pendingImages: false } }
                    : current,
                );
              }
            })
            .catch(() => {
              // 読み込めなくても、文はそのまま読める（次に表示したときにまた読む）
            });
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setState({ kind: "error", id, message: String(e) });
        }
      });
    return () => {
      cancelled = true;
    };
    // id はレンダーのたびに作り直されるので、中身（key）で比べる
  }, [key, refreshCount]);

  function refresh() {
    setRefreshKey(`${key}#${refreshCount + 1}`);
    setRefreshCount(refreshCount + 1);
  }

  function openPage(page: ProblemId) {
    openProblemPage(page).catch(() => {});
  }

  // 「サンプルを保存」（D-112）
  async function save(problemId: ProblemId) {
    if (!target) {
      return;
    }
    try {
      setNotice(describeSaved(await saveSamples(target, problemId)));
      onSamplesSaved();
    } catch (e) {
      setNotice(`サンプルを保存できません：${String(e)}`);
    }
  }

  // 「この問題を作る」（Phase 7a。D-151）：作業領域の直下の `<コンテスト>/<問題 ID>/` に作る
  async function create(problemId: ProblemId) {
    if (creating) {
      return;
    }
    setCreating(true);
    try {
      const created = await createProblemPackage(problemId);
      const result = created.package;
      setNotice(`${result.path}：${describePackage(result)}`);
      if (result.error === null) {
        onPackageCreated(mainFile(result));
      }
    } catch (e) {
      setNotice(`問題を作れません：${String(e)}`);
    } finally {
      setCreating(false);
    }
  }

  // 「input! を作る」（D-194・D-195）：保存してある問題から作り、結果の枠に出す
  async function make(problemId: ProblemId) {
    if (making) {
      return;
    }
    setMaking(true);
    try {
      setMade({ generated: await makeInput(problemId), message: null });
    } catch (e) {
      setMade(null);
      setNotice(`${inputName(settings.language)}を作れません：${String(e)}`);
    } finally {
      setMaking(false);
    }
  }

  // C++・Python：開いているタブの solve()（なければ main()）の、目印で囲んだ入力のコードを置き換える（なければ入れる。
  // D-213・D-227）
  async function replaceMarkedInEditor(generated: GeneratedInput) {
    const python = generated.language === "python";
    const editor = activeEditorTarget();
    const fits = editor && (python ? isPythonPath(editor.key) : isCppPath(editor.key));
    if (!editor || !fits || isReadOnlyKey(editor.key)) {
      setMade({
        generated,
        message: python ? "置き換える .py のファイルをエディタで開いてください" : "置き換える C++ のファイルをエディタで開いてください",
      });
      return;
    }
    const before = editor.view.state.doc.toString();
    try {
      const filled = await (python ? fillPythonInput : fillCppInput)(before, generated, settings.mainLoop);
      // 待つ間に編集・タブの切り替えがあったら、古い文書への結果なので使わない
      const now = activeEditorTarget();
      if (now?.key !== editor.key || now.view.state.doc.toString() !== before) {
        setMade({ generated, message: "書いている間にファイルが変わったので、置き換えていません。もう一度押してください" });
        return;
      }
      replaceDocument(editor.view, filled.text);
      setMade({ generated, message: `${editor.key}：${filled.message}` });
    } catch (e) {
      setMade({ generated, message: String(e) });
    }
  }

  // 開いているタブの solve()（と main()）の input! を置き換える
  function replaceInEditor(generated: GeneratedInput) {
    if (generated.language === "cpp" || generated.language === "python") {
      void replaceMarkedInEditor(generated);
      return;
    }
    const editor = activeEditorTarget();
    if (!editor || !editor.key.endsWith(".rs") || isReadOnlyKey(editor.key)) {
      setMade({ generated, message: "置き換える .rs のファイルをエディタで開いてください" });
      return;
    }
    const result = replaceInputMacros(editor.view, generated.solve, generated.main, settings.mainLoop);
    let message = result.ok ? `${editor.key}：${result.message}` : result.message;
    // 対象が作業領域のルートのパッケージ（package が空の文字）なら、どのファイルも中とする
    if (result.ok && target && target.package !== "" && !editor.key.startsWith(`${target.package}/`)) {
      message += `（このファイルは、実行の対象のパッケージ ${target.package} の外です）`;
    }
    setMade({ generated, message });
  }

  async function copyInput(generated: GeneratedInput) {
    const copied = await copyText(inputText(generated));
    setMade({ generated, message: copied ? "コピーしました" : "コピーできませんでした" });
  }

  // ボタンの文字（「サンプルを保存」は件数、作っている間は「作っています…」）
  function buttonLabel(name: string) {
    if (name === "makeInput") {
      return making ? "作っています…" : `${inputName(settings.language)}を作る`;
    }
    if (name === "saveSamples") {
      return `サンプルを保存（${state.kind === "ok" ? state.problem.sampleCount : 0} 件）`;
    }
    if (name === "createProblem") {
      return creating ? "作っています…" : "この問題を作る";
    }
    return PROBLEM_BUTTONS.find((b) => b.name === name)?.label ?? name;
  }

  function runButton(name: string, problemId: ProblemId) {
    switch (name) {
      case "makeInput":
        void make(problemId);
        break;
      case "saveSamples":
        void save(problemId);
        break;
      case "refresh":
        refresh();
        break;
      case "openPage":
        openPage(problemId);
        break;
      case "editorial":
        openEditorialPage(problemId).catch(() => {});
        break;
      case "createProblem":
        void create(problemId);
        break;
    }
  }

  // 「サンプルを保存」を押せない理由（D-128）
  const saveBlocked = untrusted ? UNTRUSTED : target ? undefined : "実行の対象のファイルを開いてください";

  // 見出しの行のボタン
  function headButton(name: string, problemId: ProblemId) {
    const busy = (name === "makeInput" && making) || (name === "createProblem" && creating);
    const button = (
      <button
        key={name}
        type="button"
        className="problem-link"
        aria-disabled={busy || (name === "saveSamples" && !target)}
        onClick={() => runButton(name, problemId)}
      >
        {buttonLabel(name)}
      </button>
    );
    return name === "saveSamples" ? (
      <span key={name} className="run-button-wrap" data-tip={saveBlocked}>
        {button}
      </span>
    ) : (
      button
    );
  }

  // 「…」の一覧のボタン
  function menuButton(name: string, problemId: ProblemId) {
    const blocked = name === "saveSamples" ? saveBlocked : undefined;
    return (
      <button
        key={name}
        type="button"
        role="menuitem"
        disabled={blocked !== undefined}
        title={blocked}
        onClick={() => runButton(name, problemId)}
      >
        {buttonLabel(name)}
      </button>
    );
  }

  switch (state.kind) {
    case "none":
      return (
        <div className="problem-empty">
          問題が選ばれていません。「コンテストを作る」で作った問題のパッケージ（例：abc480/abc480_a）のファイルを開くか、上の選択欄で問題を選ぶと、その問題を表示します
        </div>
      );
    case "loading":
      return <div className="problem-empty">{state.id.task} を読み込んでいます…</div>;
    case "error":
      // 取得・読み取りに失敗したときは、公式ページを開く手段を示す（D-110、原典 §4.6）
      return (
        <div className="problem-empty">
          <div>
            {state.id.task} を表示できません：{state.message}
          </div>
          <div className="problem-actions">
            <button type="button" className="run-button" onClick={() => openPage(state.id)}>
              公式ページを開く
            </button>
            <button type="button" className="run-button" onClick={refresh}>
              もう一度試す
            </button>
          </div>
        </div>
      );
    case "ok": {
      const problemId = { contest: state.problem.contest, task: state.problem.task };
      // 出せるボタンのうち、設定（problem.buttons）にあるものを見出しの行に、ほかを「…」の一覧に（D-196）
      const available = PROBLEM_BUTTONS.map((b) => b.name).filter(
        (name) => name !== "createProblem" || canCreate,
      );
      const shown = available.filter((name) => settings.buttons.includes(name));
      const hidden = available.filter((name) => !settings.buttons.includes(name));
      return (
        <div className="problem-view">
          <div className="problem-head">
            <div className="problem-title-row">
              <div className="problem-title">{state.problem.title}</div>
              {shown.map((name) => headButton(name, problemId))}
              {hidden.length > 0 && (
                <span className="problem-more">
                  <button
                    type="button"
                    className="problem-link"
                    aria-label="ほかの操作"
                    aria-expanded={moreOpen}
                    onClick={(event) => {
                      // 開いたクリックが窓まで届くと、外を押したときの「閉じる」が動いてしまうので止める
                      event.stopPropagation();
                      setMoreOpen(!moreOpen);
                    }}
                  >
                    …
                  </button>
                  {moreOpen && (
                    <div className="context-menu problem-more-menu" role="menu">
                      {hidden.map((name) => menuButton(name, problemId))}
                    </div>
                  )}
                </span>
              )}
            </div>
            {/* 制限（D-116） */}
            <div className="problem-limits">
              実行時間制限 <span className="problem-limit-value">{state.problem.timeLimit}</span> /
              メモリ制限 <span className="problem-limit-value">{state.problem.memoryLimit}</span>
              {difficulty && difficulty.key === key && (
                <>
                  {" / "}
                  <span
                    className={`problem-diff ${ratingClass(difficulty.view.value)}`}
                    title={difficultyHint(difficulty.view)}
                  >
                    {difficultyText(difficulty.view)}
                  </span>
                </>
              )}
            </div>
          </div>
          {notice && <div className="problem-notice">{notice}</div>}
          {made && (
            <div className="problem-input">
              <div className="problem-input-head">
                作った{made.generated.language === "rust" ? " input!" : "入力のコード"}（{samplesText(made.generated)}）
              </div>
              <pre className="problem-input-code">{inputText(made.generated)}</pre>
              {made.generated.notes.length > 0 && (
                <ul className="problem-input-notes">
                  {made.generated.notes.map((note) => (
                    <li key={note}>{note}</li>
                  ))}
                </ul>
              )}
              <div className="problem-actions">
                <button type="button" className="run-button" onClick={() => replaceInEditor(made.generated)}>
                  {made.generated.language === "rust" ? "solve() の input! を置き換える" : "solve() の入力を置き換える"}
                </button>
                <button type="button" className="run-button" onClick={() => void copyInput(made.generated)}>
                  コピー
                </button>
                <button type="button" className="run-button" onClick={() => setMade(null)}>
                  閉じる
                </button>
              </div>
              {made.message && <div className="problem-input-message">{made.message}</div>}
            </div>
          )}
          <Statement html={state.problem.statementHtml} />
        </div>
      );
    }
  }
}

export default ProblemPane;
