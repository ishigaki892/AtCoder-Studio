// 「過去問」のタブの「解き直し」（Phase 34 作業 2。D-258）。解き直しの印を付けた問題を、付けた日の新しい順に出す。
// 行を押すと、その問題を問題文の欄に出す。「外す」で印を外す。作るのは Rust 側（atcoder_problems.rs の retry_view）で、
// AtCoder Problems のデータと AC 状況は保存してあるものだけを使う（新しく読まない）
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ProblemId } from "../ipc/problem";
import { difficultyHint } from "../ipc/atcoderProblems";
import { retryProblems, SOLVED_LABELS, type ProblemMarks, type RetryRow, type RetryView } from "../ipc/problemMarks";
import { ratingClass } from "../rating";

type Props = {
  // 種類の切り替え（PastProblems が作る）
  tabs: ReactNode;
  // 変わったら読み直す（AC 状況に使う ID）
  user: string;
  loggedIn: boolean;
  current: ProblemId | null;
  onSelect: (id: ProblemId) => void;
  // 変わったら読み直す（印を付け外ししたとき）
  marks: ProblemMarks;
  onRetry: (id: string, on: boolean) => Promise<void>;
};

// 問題の場所（例：ABC042 C / ARC058 C）。データがなければ問題 ID
function placeText(row: RetryRow): string {
  return row.places.length > 0
    ? row.places.map((place) => `${place.contest.toUpperCase()} ${place.index}`).join(" / ")
    : row.problem;
}

function solvedText(row: RetryRow): string {
  return row.solved === "accepted" ? "AC" : row.solved === "tried" ? "提出あり" : "—";
}

function RetryList({ tabs, user, loggedIn, current, onSelect, marks, onRetry }: Props) {
  const [view, setView] = useState<RetryView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const seq = useRef(0);

  async function read() {
    const mine = ++seq.current;
    try {
      const next = await retryProblems();
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
  }, [marks, user, loggedIn]);

  return (
    <div className="past-problems">
      <div className="past-head">
        {tabs}
        <button type="button" className="contest-refresh" onClick={() => void read()}>
          更新
        </button>
        <span className="past-meta">
          {view
            ? view.rows.length > 0
              ? `${view.rows.length} 問（印を付けた日の新しい順）`
              : "解き直す問題はありません。問題文の欄の「解き直す」で印を付けると、ここに出ます"
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
      {view && view.rows.length > 0 && (
        <div className="past-scroll">
          <table className="past-table retry-table">
            <thead>
              <tr>
                <th>問題</th>
                <th>diff</th>
                <th>AC 状況</th>
                <th>自力・解説</th>
                <th>付けた日</th>
                <th aria-label="操作" />
              </tr>
            </thead>
            <tbody>
              {view.rows.map((row) => {
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
                    <td>{row.solvedBy ? SOLVED_LABELS[row.solvedBy] : "—"}</td>
                    <td>{row.since}</td>
                    <td>
                      <button
                        type="button"
                        className="problem-link"
                        onClick={() => void onRetry(row.problem, false).catch((e) => setError(String(e)))}
                      >
                        外す
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
      {view && view.rows.length > 0 && !view.user && (
        <p className="contest-note">AC 状況は、「ABC」などの表で AC 状況を読んだあとに出ます</p>
      )}
    </div>
  );
}

export default RetryList;
