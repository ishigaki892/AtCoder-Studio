// 中央の「過去問」のタブ（Phase 24 作業 2。D-242）。ABC・ARC・AGC・AWC（D-255）を切り替えて、行はコンテスト（新しい順）、
// 列は問題の記号の表を出す。ますは AtCoder Problems の diff の色、背景で AC 状況（AC は緑、AC していない提出があれば黄）。
// ますを押すと、その問題を問題文の欄に出す。読むのは Rust 側（src-tauri/src/atcoder_problems.rs）
import { useEffect, useRef, useState } from "react";
import type { ProblemId } from "../ipc/problem";
import {
  difficultyHint,
  pastProblems,
  type PastCategory,
  type PastView,
} from "../ipc/atcoderProblems";
import { ratingClass } from "../rating";

const CATEGORIES: { id: PastCategory; label: string }[] = [
  { id: "abc", label: "ABC" },
  { id: "arc", label: "ARC" },
  { id: "agc", label: "AGC" },
  // AtCoder Weekday Contest。AtCoder Problems に diff がないので、ますは「—」と AC 状況の背景だけ
  { id: "awc", label: "AWC" },
];

type Props = {
  // 設定の AtCoder の ID（変わったら読み直す。読むときは Rust 側が設定から読む）。空なら、ログインしているアカウントの ID を使う
  user: string;
  // ログインしているか（変わったら読み直す）
  loggedIn: boolean;
  // 問題文の欄の問題（そのますを強調する）
  current: ProblemId | null;
  onSelect: (id: ProblemId) => void;
};

function PastProblems({ user, loggedIn, current, onSelect }: Props) {
  const [category, setCategory] = useState<PastCategory>("abc");
  const [view, setView] = useState<PastView | null>(null);
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

  // 開いたとき・種類を替えたとき・ID を変えたとき・ログインの状態が変わったとき
  useEffect(() => {
    void read(category, false);
  }, [category, user, loggedIn]);

  const table = view?.table ?? null;
  return (
    <div className="past-problems">
      <div className="past-head">
        <div className="past-categories" role="tablist" aria-label="コンテストの種類">
          {CATEGORIES.map(({ id, label }) => (
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
        <button type="button" className="contest-refresh" disabled={loading} onClick={() => void read(category, true)}>
          更新
        </button>
        <span className="past-meta">
          {loading
            ? "読んでいます…"
            : table && view?.user
              ? `${view.user}${user.trim() ? "" : "（ログインしているアカウント）"}：AC ${table.accepted} / ${table.total} 問${view.solvedComplete ? "" : "（提出の続きは、もう一度「更新」で読みます）"}`
              : table
                ? `${table.total} 問。ログインするか、設定の「過去問」に AtCoder の ID を書くと、AC 状況も出ます`
                : ""}
        </span>
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
      {table && (
        <div className="past-scroll">
          <table className="past-table">
            <thead>
              <tr>
                <th>コンテスト</th>
                {table.columns.map((column) => (
                  <th key={column}>{column}</th>
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
                    return (
                      <td
                        key={table.columns[index]}
                        className={`past-cell ${cell.solved ? `past-${cell.solved}` : ""} ${selected ? "current" : ""}`}
                      >
                        <button
                          type="button"
                          title={`${cell.problem}\n${hint}${cell.solved === "accepted" ? "\nAC 済み" : cell.solved === "tried" ? "\n提出あり（AC なし）" : ""}`}
                          onClick={() => onSelect({ contest: row.contest, task: cell.problem })}
                        >
                          {cell.difficulty ? (
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
