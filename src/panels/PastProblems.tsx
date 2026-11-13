// 中央の「過去問」のタブ（Phase 24 作業 2。D-242）。ABC・ARC・AGC・AWC（D-255）を切り替えて、行はコンテスト（新しい順）、
// 列は問題の記号の表を出す。ますは AtCoder Problems の diff の色、背景で AC 状況（AC は緑、AC していない提出があれば黄）。
// 解説 AC（手で付けた記録。D-257）は、AC の緑の代わりに別の色。解き直しの印（D-258）は、ますの右上の三角。
// ますを押すと、その問題を問題文の欄に出す。読むのは Rust 側（src-tauri/src/atcoder_problems.rs）。
// 「解き直し」（D-258）は、印を付けた問題の一覧、「解説 AC」（D-261）は「解説」を付けた問題の一覧（どちらも RetryList.tsx）。
// 「JOI」（D-260）は小分けを選んで出す。JOI の表は、列が何問目かで、ますには diff の代わりに問題の記号を出す（diff はない）
import { useEffect, useRef, useState } from "react";
import type { ProblemId } from "../ipc/problem";
import {
  difficultyHint,
  JOI_KINDS,
  pastProblems,
  type JoiKind,
  type PastCategory,
  type PastCell,
  type PastView,
} from "../ipc/atcoderProblems";
import { ratingClass } from "../rating";
import type { ProblemMarks } from "../ipc/problemMarks";
import RetryList from "./RetryList";

const CATEGORIES: { id: "abc" | "arc" | "agc" | "awc"; label: string }[] = [
  { id: "abc", label: "ABC" },
  { id: "arc", label: "ARC" },
  { id: "agc", label: "AGC" },
  // AtCoder Weekday Contest。AtCoder Problems に diff がないので、ますは「—」と AC 状況の背景だけ
  { id: "awc", label: "AWC" },
];

// 上の切り替え。「JOI」は小分け（joiKind）の表を出す
type Choice = "abc" | "arc" | "agc" | "awc" | "joi" | "retry" | "editorial";

type Props = {
  // 設定の AtCoder の ID（変わったら読み直す。読むときは Rust 側が設定から読む）。空なら、ログインしているアカウントの ID を使う
  user: string;
  // ログインしているか（変わったら読み直す）
  loggedIn: boolean;
  // 問題文の欄の問題（そのますを強調する）
  current: ProblemId | null;
  onSelect: (id: ProblemId) => void;
  // 自力 AC・解説 AC の記録（Phase 33。D-257）。AC のますのうち解説 AC だけ色を変え、説明にはどのますにも出す。
  // 解き直しの印（Phase 34。D-258）は、ますの右上の三角と説明
  marks: ProblemMarks;
  // 解き直しの印を外すとき（「解き直し」の一覧の「外す」）
  onRetry: (id: string, on: boolean) => Promise<void>;
  // 「JOI」で選んでいる小分け（D-260。App.tsx が state.json に残す）
  joiKind: JoiKind;
  onJoiKind: (kind: JoiKind) => void;
};

// ますの説明の AC 状況と記録の行（D-257）
function solvedHint(cell: PastCell, marks: ProblemMarks): string {
  const solved = marks[cell.problem]?.solved;
  if (cell.solved === "accepted") {
    return solved === "editorial" ? "解説 AC" : solved === "self" ? "自力 AC" : "AC（自力・解説の記録なし）";
  }
  const status = cell.solved === "tried" ? "提出あり（AC なし）" : "";
  const mark = solved === "editorial" ? "記録：解説" : solved === "self" ? "記録：自力" : "";
  return [status, mark].filter(Boolean).join("\n");
}

// ますの説明の解き直しの行（D-258）
function retryHint(problem: string, marks: ProblemMarks): string {
  const since = marks[problem]?.retry;
  return since ? `解き直す（${since} から）` : "";
}

function PastProblems({ user, loggedIn, current, onSelect, marks, onRetry, joiKind, onJoiKind }: Props) {
  // 表の種類、「JOI」、または「解き直し」の一覧
  const [category, setCategory] = useState<Choice>("abc");
  // 読む表（「JOI」なら小分け）
  const target: PastCategory | "retry" | "editorial" = category === "joi" ? joiKind : category;
  const joi = category === "joi";
  const [view, setView] = useState<PastView | null>(null);
  // view の表の種類
  const [shown, setShown] = useState<PastCategory | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  // 読む順番（遅れて届いた、前の種類の結果を使わない）
  const seq = useRef(0);

  async function read(target: PastCategory, refresh: boolean) {
    const mine = ++seq.current;
    setLoading(true);
    try {
      const next = await pastProblems(target, refresh);
      if (mine === seq.current) {
        setView(next);
        setShown(target);
        setError(null);
      }
    } catch (e) {
      if (mine === seq.current) {
        setError(String(e));
      }
    } finally {
      if (mine === seq.current) {
        setLoading(false);
      }
    }
  }

  // 開いたとき・種類を替えたとき・ID を変えたとき・ログインの状態が変わったとき（「解き直し」は RetryList が読む）
  useEffect(() => {
    if (target !== "retry" && target !== "editorial") {
      void read(target, false);
    }
  }, [target, user, loggedIn]);

  const tabs = (
    <div className="past-categories" role="tablist" aria-label="コンテストの種類">
      {[...CATEGORIES, { id: "joi" as const, label: "JOI" }, { id: "retry" as const, label: "解き直し" }, { id: "editorial" as const, label: "解説 AC" }].map(({ id, label }) => (
        <button
          key={id}
          type="button"
          role="tab"
          aria-selected={category === id}
          className={`past-category ${category === id ? "selected" : ""}`}
          onClick={() => setCategory(id)}
        >
          {label}
        </button>
      ))}
    </div>
  );
  if (target === "retry" || target === "editorial") {
    return (
      <RetryList key={target} mode={target} tabs={tabs} user={user} loggedIn={loggedIn} current={current} onSelect={onSelect} marks={marks} onRetry={onRetry} />
    );
  }

  const table = view?.table ?? null;
  // 出している表が JOI のものか（読み終えるまでは前の表が残るので、選択ではなく出している表で決める）
  const joiTable = shown !== null && JOI_KINDS.some(({ id }) => id === shown);
  // 今の表の解説 AC の数（D-257）
  const editorialCount = table
    ? table.rows
        .flatMap((row) => row.cells)
        .filter((cell) => cell?.solved === "accepted" && marks[cell.problem]?.solved === "editorial").length
    : 0;
  return (
    <div className="past-problems">
      <div className="past-head">
        {tabs}
        <button type="button" className="contest-refresh" disabled={loading} onClick={() => void read(target, true)}>
          更新
        </button>
        <span className="past-meta">
          {loading
            ? "読んでいます…"
            : table && view?.user
              ? `${view.user}${user.trim() ? "" : "（ログインしているアカウント）"}：AC ${table.accepted} / ${table.total} 問${editorialCount > 0 ? `（解説 ${editorialCount}）` : ""}${view.solvedComplete ? "" : "（提出の続きは、もう一度「更新」で読みます）"}`
              : table
                ? `${table.total} 問。ログインするか、設定の「過去問」に AtCoder の ID を書くと、AC 状況も出ます`
                : ""}
        </span>
      </div>
      {joi && (
        <div className="past-categories past-subkinds" role="tablist" aria-label="JOI の種類">
          {JOI_KINDS.filter(({ id }) => id !== "joi-other" || view?.joiOther || joiKind === id).map(({ id, label }) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={joiKind === id}
              className={`past-category ${joiKind === id ? "selected" : ""}`}
              onClick={() => onJoiKind(id)}
            >
              {label}
            </button>
          ))}
        </div>
      )}
      {error && (
        <p className="contest-error" role="alert">
          {error}
        </p>
      )}
      {view?.messages.map((message) => (
        <p key={message} className="contest-note">
          {message}
        </p>
      ))}
      {table && (
        <div className="past-scroll">
          <table className="past-table">
            <thead>
              <tr>
                <th>コンテスト</th>
                {table.columns.map((column) => (
                  <th key={column} title={joiTable ? `${column} 問目` : undefined}>
                    {column}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.rows.map((row) => (
                <tr key={row.contest}>
                  <th className="past-contest" title={row.title}>
                    {row.contest.toUpperCase()}
                  </th>
                  {row.cells.map((cell, index) => {
                    if (!cell) {
                      return <td key={table.columns[index]} className="past-empty" />;
                    }
                    const selected = current?.contest === row.contest && current.task === cell.problem;
                    const hint = cell.difficulty ? difficultyHint(cell.difficulty) : "diff はありません";
                    const solvedLine = [solvedHint(cell, marks), retryHint(cell.problem, marks)]
                      .filter(Boolean)
                      .join("\n");
                    // 解説 AC は、AC の緑の代わりに別の色（D-257）
                    const background =
                      cell.solved === "accepted" && marks[cell.problem]?.solved === "editorial"
                        ? "past-editorial"
                        : cell.solved
                          ? `past-${cell.solved}`
                          : "";
                    return (
                      <td
                        key={table.columns[index]}
                        className={`past-cell ${background} ${selected ? "current" : ""} ${marks[cell.problem]?.retry ? "past-retry" : ""}`}
                      >
                        <button
                          type="button"
                          title={`${cell.problem}${joiTable ? `\n記号：${cell.index}` : ""}\n${hint}${solvedLine ? `\n${solvedLine}` : ""}`}
                          onClick={() => onSelect({ contest: row.contest, task: cell.problem })}
                        >
                          {joiTable && !cell.difficulty ? (
                            <span className="past-index">{cell.index}</span>
                          ) : cell.difficulty ? (
                            <span className={ratingClass(cell.difficulty.value)}>
                              {cell.difficulty.value}
                              {cell.difficulty.experimental ? "?" : ""}
                            </span>
                          ) : (
                            <span className="past-no-diff">—</span>
                          )}
                        </button>
                      </td>
                    );
                  })}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

export default PastProblems;
