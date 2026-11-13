// 「過去問」のタブの「解き直し」（Phase 34 作業 2。D-258）と「解説 AC」（Phase 37。D-261）の一覧。
// 「解き直し」：解き直しの印を付けた問題を、付けた日の新しい順に出す。「外す」で印を外す。
// 「解説 AC」：「解説」を付けた問題を出す。「付けた日」「問題」「diff」の見出しで並べ方（昇順・降順）を選び、
// 「解き直し」の列で解き直しの印を付け外しする。「解説」の記録を外すのは、問題文の欄だけ。
// どちらも、行を押すとその問題を問題文の欄に出す。作るのは Rust 側（atcoder_problems.rs の marks_view）で、
// AtCoder Problems のデータと AC 状況は保存してあるものだけを使う（新しく読まない）
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ProblemId } from "../ipc/problem";
import { difficultyHint } from "../ipc/atcoderProblems";
import {
  editorialProblems,
  retryProblems,
  SOLVED_LABELS,
  type ProblemMarks,
  type RetryRow,
  type RetryView,
} from "../ipc/problemMarks";
import { ratingClass } from "../rating";

export type MarkListMode = "retry" | "editorial";

type Props = {
  mode: MarkListMode;
  // 種類の切り替え（PastProblems が作る）
  tabs: ReactNode;
  // 変わったら読み直す（AC 状況に使う ID）
  user: string;
  loggedIn: boolean;
  current: ProblemId | null;
  onSelect: (id: ProblemId) => void;
  // 変わったら読み直す（印・記録を付け外ししたとき）
  marks: ProblemMarks;
  onRetry: (id: string, on: boolean) => Promise<void>;
};

// 「解説 AC」の並べ方（D-261）。since は入れた順（付けた日）、id は問題 ID の順、diff は diff の順
type SortKey = "since" | "id" | "diff";
type Sort = { key: SortKey; ascending: boolean };
// アプリを開いている間だけ覚える（タブを切り替えても残す。開き直すと入れた順の降順）
let editorialSort: Sort = { key: "since", ascending: false };

// 並べる値。null は、どちらの向きでも後ろ
function sortValue(row: RetryRow, key: SortKey): string | number | null {
  return key === "since" ? row.since : key === "id" ? row.problem : (row.difficulty?.value ?? null);
}

function sortRows(rows: RetryRow[], sort: Sort): RetryRow[] {
  return [...rows].sort((a, b) => {
    const x = sortValue(a, sort.key);
    const y = sortValue(b, sort.key);
    if (x === null || y === null) {
      return x === y ? a.problem.localeCompare(b.problem) : x === null ? 1 : -1;
    }
    const order = x < y ? -1 : x > y ? 1 : 0;
    return (sort.ascending ? order : -order) || a.problem.localeCompare(b.problem);
  });
}

// 問題の場所（例：ABC042 C / ARC058 C）。データがなければ問題 ID
function placeText(row: RetryRow): string {
  return row.places.length > 0
    ? row.places.map((place) => `${place.contest.toUpperCase()} ${place.index}`).join(" / ")
    : row.problem;
}

function solvedText(row: RetryRow): string {
  return row.solved === "accepted" ? "AC" : row.solved === "tried" ? "提出あり" : "—";
}

function RetryList({ mode, tabs, user, loggedIn, current, onSelect, marks, onRetry }: Props) {
  const [view, setView] = useState<RetryView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [sort, setSort] = useState<Sort>(editorialSort);
  const seq = useRef(0);
  const editorial = mode === "editorial";

  async function read() {
    const mine = ++seq.current;
    try {
      const next = await (editorial ? editorialProblems() : retryProblems());
      if (mine === seq.current) {
        setView(next);
        setError(null);
      }
    } catch (e) {
      if (mine === seq.current) {
        setError(String(e));
      }
    }
  }

  useEffect(() => {
    void read();
  }, [mode, marks, user, loggedIn]);

  // 見出しを押したとき：同じ見出しなら向きを替え、違う見出しなら、入れた順と diff は降順、ID は昇順から
  function chooseSort(key: SortKey) {
    const next = sort.key === key ? { key, ascending: !sort.ascending } : { key, ascending: key === "id" };
    editorialSort = next;
    setSort(next);
  }

  // 並べ方を選べる見出し（「解説 AC」だけ）
  function sortHeader(key: SortKey, label: string) {
    if (!editorial) {
      return <th>{label}</th>;
    }
    const chosen = sort.key === key;
    return (
      <th aria-sort={chosen ? (sort.ascending ? "ascending" : "descending") : "none"}>
        <button
          type="button"
          className={`retry-sort ${chosen ? "selected" : ""}`}
          title={`${label}の順に並べます（もう一度押すと逆の順）`}
          onClick={() => chooseSort(key)}
        >
          {label}
          {chosen ? (sort.ascending ? " ▲" : " ▼") : ""}
        </button>
      </th>
    );
  }

  const rows = view ? (editorial ? sortRows(view.rows, sort) : view.rows) : [];
  const empty = editorial
    ? "解説 AC の問題はありません。問題文の欄の「解説」で記録すると、ここに出ます"
    : "解き直す問題はありません。問題文の欄の「解き直す」で印を付けると、ここに出ます";
  const count = editorial ? `${rows.length} 問` : `${rows.length} 問（印を付けた日の新しい順）`;

  return (
    <div className="past-problems">
      <div className="past-head">
        {tabs}
        <button type="button" className="contest-refresh" onClick={() => void read()}>
          更新
        </button>
        <span className="past-meta">{view ? (rows.length > 0 ? count : empty) : ""}</span>
      </div>
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
      {view && rows.length > 0 && (
        <div className="past-scroll">
          <table className="past-table retry-table">
            <thead>
              <tr>
                {sortHeader("id", "問題")}
                {sortHeader("diff", "diff")}
                <th>AC 状況</th>
                {!editorial && <th>自力・解説</th>}
                {sortHeader("since", "付けた日")}
                {editorial ? <th>解き直し</th> : <th aria-label="操作" />}
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => {
                const selected = current?.task === row.problem;
                const background =
                  row.solved === "accepted" && row.solvedBy === "editorial"
                    ? "past-editorial"
                    : row.solved
                      ? `past-${row.solved}`
                      : "";
                return (
                  <tr key={row.problem} className={selected ? "current" : ""}>
                    <td className="retry-problem">
                      <button
                        type="button"
                        title={`${row.problem}\n押すと問題文の欄に出します`}
                        onClick={() => onSelect({ contest: row.contest, task: row.problem })}
                      >
                        {placeText(row)}
                      </button>
                    </td>
                    <td>
                      {row.difficulty ? (
                        <span className={ratingClass(row.difficulty.value)} title={difficultyHint(row.difficulty)}>
                          {row.difficulty.value}
                          {row.difficulty.experimental ? "?" : ""}
                        </span>
                      ) : (
                        <span className="past-no-diff">—</span>
                      )}
                    </td>
                    <td className={background}>{solvedText(row)}</td>
                    {!editorial && <td>{row.solvedBy ? SOLVED_LABELS[row.solvedBy] : "—"}</td>}
                    <td title={editorial && !row.since ? "付けた日を記録する前（0.2.1 まで）に付けた記録です" : undefined}>
                      {row.since ?? "—"}
                    </td>
                    <td>
                      {editorial ? (
                        <button
                          type="button"
                          className={`problem-mark mark-retry ${row.retry ? "pressed" : ""}`}
                          aria-pressed={row.retry !== null}
                          title={row.retry ? `解き直す（${row.retry} から）。押すと印を外します` : "押すと解き直しの印を付けます"}
                          onClick={() => void onRetry(row.problem, row.retry === null).catch((e) => setError(String(e)))}
                        >
                          {row.retry ? `解き直す（${row.retry}）` : "解き直す"}
                        </button>
                      ) : (
                        <button
                          type="button"
                          className="problem-link"
                          onClick={() => void onRetry(row.problem, false).catch((e) => setError(String(e)))}
                        >
                          外す
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {view && rows.length > 0 && !view.user && (
        <p className="contest-note">AC 状況は、「ABC」などの表で AC 状況を読んだあとに出ます</p>
      )}
    </div>
  );
}

export default RetryList;
