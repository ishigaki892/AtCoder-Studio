// 中央の「過去問」のタブ（Phase 24 作業 2。D-242）。ABC・ARC・AGC・AWC（D-255）を切り替えて、行はコンテスト（新しい順）、
// 列は問題の記号の表を出す。ますは AtCoder Problems の diff の色、背景で AC 状況（AC は緑、AC していない提出があれば黄）。
// 解説 AC（手で付けた記録。D-257）は、AC の緑の代わりに別の色。解き直しの印（D-258）は、ますの右上の三角。
// ますを押すと、その問題を問題文の欄に出す。読むのは Rust 側（src-tauri/src/atcoder_problems.rs）。
// 「解き直し」（D-258）は、印を付けた問題の一覧、「解説 AC」（D-261）は「解説」を付けた問題の一覧（どちらも RetryList.tsx）。
// 「JOI」（D-260）は小分けを選んで出す。JOI の表は、列が何問目かで、ますには diff の代わりに問題の記号を出す（diff はない）。
// 絞り込み（D-264）：状況と diff の範囲（数）で、合わないますを薄くし、合うますのない回を隠す。画面側だけで行う
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
import { firstLine } from "./MemoPanel";

const CATEGORIES: { id: "abc" | "arc" | "agc" | "awc"; label: string }[] = [
  { id: "abc", label: "ABC" },
  { id: "arc", label: "ARC" },
  { id: "agc", label: "AGC" },
  // AtCoder Weekday Contest。AtCoder Problems に diff がないので、ますは「—」と AC 状況の背景だけ
  { id: "awc", label: "AWC" },
];

// 上の切り替え。「JOI」は小分け（joiKind）の表を出す
type Choice = "abc" | "arc" | "agc" | "awc" | "joi" | "retry" | "editorial";

// 絞り込み（D-264）。status は状況、min・max は diff の下限と上限（両端を含む）。入れたままの文字で持ち、
// 空か整数として読めなければ、その側は決めない
type Status = "all" | "unsolved" | "untouched" | "editorial" | "self" | "retry";
type Filter = { status: Status; min: string; max: string };
const STATUSES: { id: Status; label: string }[] = [
  { id: "all", label: "すべて" },
  { id: "unsolved", label: "AC していない" },
  { id: "untouched", label: "手を付けていない" },
  { id: "editorial", label: "解説 AC" },
  { id: "self", label: "自力 AC" },
  { id: "retry", label: "解き直し" },
];
const NO_FILTER: Filter = { status: "all", min: "", max: "" };
// 表の種類によらず 1 つ。アプリを開いている間だけ覚える（タブを閉じても残す）
let savedFilter: Filter = NO_FILTER;

// 欄の文字を整数にする。空・整数でなければ null（その側は決めない）
function bound(text: string): number | null {
  const trimmed = text.trim();
  return /^-?\d+$/.test(trimmed) ? Number(trimmed) : null;
}

function diffFiltered(filter: Filter): boolean {
  return bound(filter.min) !== null || bound(filter.max) !== null;
}

// ますが絞り込みに合うか。useDiff は、diff のある表で diff の絞り込みを使うか
function matches(cell: PastCell, marks: ProblemMarks, filter: Filter, useDiff: boolean): boolean {
  const mark = marks[cell.problem];
  const accepted = cell.solved === "accepted";
  const status =
    filter.status === "all" ||
    (filter.status === "unsolved" && !accepted) ||
    (filter.status === "untouched" && cell.solved === null) ||
    (filter.status === "editorial" && accepted && mark?.solved === "editorial") ||
    (filter.status === "self" && accepted && mark?.solved === "self") ||
    (filter.status === "retry" && mark?.retry !== undefined);
  if (!status || !useDiff || !diffFiltered(filter)) {
    return status;
  }
  if (!cell.difficulty) {
    return false;
  }
  // 比べるのは、ますに出している diff（400 未満を AtCoder Problems と同じく直した値）
  const value = cell.difficulty.value;
  const min = bound(filter.min);
  const max = bound(filter.max);
  return (min === null || min <= value) && (max === null || value <= max);
}

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
  const [filter, setFilterState] = useState<Filter>(savedFilter);

  function setFilter(next: Filter) {
    savedFilter = next;
    setFilterState(next);
  }

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
  // diff のない表（AWC・JOI）では、diff の絞り込みは押せず、使わない（D-264）
  const hasDiff = shown !== null && shown !== "awc" && !joiTable;
  const filtering = filter.status !== "all" || (hasDiff && diffFiltered(filter));
  // 絞り込みに合うかを、ますごとに決める（合うますのない回は隠す）
  const rows = table
    ? table.rows
        .map((row) => ({
          row,
          hits: row.cells.map((cell) => cell !== null && matches(cell, marks, filter, hasDiff)),
        }))
        .filter(({ hits }) => !filtering || hits.some(Boolean))
    : [];
  const hitCount = rows.reduce((sum, { hits }) => sum + hits.filter(Boolean).length, 0);
  // diff の欄の書き方の誤り（読めない文字・下限が上限より大きい）
  const [minText, maxText] = [filter.min.trim(), filter.max.trim()];
  const [minValue, maxValue] = [bound(filter.min), bound(filter.max)];
  const diffNote =
    (minText !== "" && minValue === null) || (maxText !== "" && maxValue === null)
      ? "diff は整数で入れてください（読めない側は使いません）"
      : minValue !== null && maxValue !== null && minValue > maxValue
        ? "下限が上限より大きいので、合う問題はありません"
        : null;
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
      <div className="past-filter" role="group" aria-label="絞り込み">
        <label>
          状況
          <select value={filter.status} onChange={(e) => setFilter({ ...filter, status: e.target.value as Status })}>
            {STATUSES.map(({ id, label }) => (
              <option key={id} value={id}>
                {label}
              </option>
            ))}
          </select>
        </label>
        <label title={hasDiff ? undefined : "この表の問題には diff がないので、diff では絞り込めません"}>
          diff
          <input
            className="past-diff-input"
            type="text"
            inputMode="numeric"
            placeholder="下限"
            aria-label="diff の下限"
            value={filter.min}
            disabled={!hasDiff}
            onChange={(e) => setFilter({ ...filter, min: e.target.value })}
          />
          〜
          <input
            className="past-diff-input"
            type="text"
            inputMode="numeric"
            placeholder="上限"
            aria-label="diff の上限"
            value={filter.max}
            disabled={!hasDiff}
            onChange={(e) => setFilter({ ...filter, max: e.target.value })}
          />
        </label>
        {hasDiff && diffNote && <span className="contest-error">{diffNote}</span>}
        {(filter.status !== "all" || diffFiltered(filter)) && (
          <button type="button" className="problem-link" onClick={() => setFilter(NO_FILTER)}>
            解除
          </button>
        )}
        {table && filtering && <span className="past-meta">絞り込み：{hitCount} 問</span>}
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
              {rows.map(({ row, hits }) => (
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
                    const memo = firstLine(marks[cell.problem]?.memo);
                    const solvedLine = [solvedHint(cell, marks), retryHint(cell.problem, marks), memo ? `メモ：${memo}` : ""]
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
                        className={`past-cell ${background} ${selected ? "current" : ""} ${marks[cell.problem]?.retry ? "past-retry" : ""} ${filtering && !hits[index] ? "past-dim" : ""}`}
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
