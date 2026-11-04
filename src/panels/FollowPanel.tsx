// 右下パネルの「追跡」（Phase 31。D-253）：今のコンテストの順位表から、自分と追跡するユーザー
// （settings.json の standings.follow）の順位・得点・問題ごとの結果を表で出す。
// 行は順位の順に並べ（Rust 側で並べる）、左の列に追跡の中の順位を出す（D-254）。
// 順位表を読むのは「更新」と、サイドバーの「コンテスト」が読んだときだけ（決まった間隔では読まない。D-233）
import { useEffect, useState } from "react";
import type { FollowCell, FollowRow, FollowTable, StandingsOutcome } from "../ipc/submit";
import { ratingClass } from "../rating";

// 「更新」を押してから、もう一度押せるまで
const COOLDOWN_MS = 10_000;

// コンテストごとに覚えるもの（App が持つ）
export type FollowEntry = {
  table: FollowTable | null;
  // 読んだ時刻（手元の時刻のミリ秒）
  readAtMs: number | null;
  // 読めなかった理由・始まる前など
  message: string | null;
  notLoggedIn: boolean;
};

// 順位表を読んだ結果で、覚えているものを新しくする。読めなかったときは、前に読んだ表を残す
export function followEntryAfter(
  before: FollowEntry | undefined,
  outcome: StandingsOutcome,
  atMs: number,
): FollowEntry {
  const kept = { table: before?.table ?? null, readAtMs: before?.readAtMs ?? null };
  switch (outcome.kind) {
    case "read":
      return { table: outcome.follow, readAtMs: atMs, message: null, notLoggedIn: false };
    case "notStarted":
      return { table: null, readAtMs: null, message: "始まる前なので、順位表は読みません", notLoggedIn: false };
    case "notLoggedIn":
      return { ...kept, message: null, notLoggedIn: true };
    case "failed":
      return { ...kept, message: `順位表を読めませんでした：${outcome.message}`, notLoggedIn: false };
  }
}

type Props = {
  // 問題文の欄の問題のコンテスト（なければ null）
  contest: string | null;
  entry: FollowEntry | null;
  loggedIn: boolean;
  loading: boolean;
  onRefresh: () => void;
  onLogin: () => void;
};

// 経過時間（公式の順位表と同じく「分:秒」。60 分を超えても分で数える）
export function elapsedText(secs: number): string {
  const minutes = Math.floor(secs / 60);
  return `${minutes}:${String(secs % 60).padStart(2, "0")}`;
}

function scoreText(score: number): string {
  return Number.isInteger(score) ? String(score) : score.toFixed(2);
}

function clock(ms: number): string {
  const date = new Date(ms);
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
}

function cellTitle(cell: FollowCell): string {
  const parts = [`得点 ${scoreText(cell.score)}`];
  if (cell.elapsedSecs !== null) {
    parts.push(`時間 ${elapsedText(cell.elapsedSecs)}`);
  }
  if (cell.accepted && cell.penalty > 0) {
    parts.push(`AC までの誤答 ${cell.penalty}`);
  }
  if (!cell.accepted && cell.failure > 0) {
    parts.push(`誤答 ${cell.failure}`);
  }
  parts.push(`提出 ${cell.count} 回`);
  if (cell.pending) {
    parts.push("判定待ちあり");
  }
  return parts.join("・");
}

// 問題のます：AC は緑で経過時間と誤答の数、AC していなければ黄で誤答の数、判定待ちは「…」、出していなければ空
function Cell({ cell }: { cell: FollowCell | null }) {
  if (!cell) {
    return <td className="follow-cell" />;
  }
  const wait = cell.pending ? <span className="follow-pending">…</span> : null;
  if (cell.accepted) {
    return (
      <td className="follow-cell follow-ac" title={cellTitle(cell)}>
        <div>
          {cell.elapsedSecs !== null ? elapsedText(cell.elapsedSecs) : scoreText(cell.score)}
          {wait}
        </div>
        {cell.penalty > 0 && <div className="follow-sub">(+{cell.penalty})</div>}
      </td>
    );
  }
  return (
    <td className={`follow-cell ${cell.failure > 0 ? "follow-wa" : "follow-wait"}`} title={cellTitle(cell)}>
      {cell.failure > 0 ? `−${cell.failure}` : ""}
      {wait}
    </td>
  );
}

function Row({ row, columns }: { row: FollowRow; columns: number }) {
  const groupRank = <td className="follow-group-rank">{row.groupRank ?? "—"}</td>;
  const name = (
    <td className="follow-user">
      <span className={row.rating !== null ? ratingClass(row.rating) : "follow-unrated"} title={row.user}>
        {row.user}
      </span>
      {row.me && <span className="follow-me">（自分）</span>}
    </td>
  );
  if (row.missing) {
    return (
      <tr className={row.me ? "follow-row-me" : undefined}>
        {groupRank}
        {name}
        <td className="follow-missing" colSpan={columns + 2}>
          この順位表にいません（参加していないか、ID が違います）
        </td>
      </tr>
    );
  }
  return (
    <tr className={row.me ? "follow-row-me" : undefined}>
      {groupRank}
      {name}
      <td className="follow-rank">{row.submitted && row.rank !== null ? row.rank : "—"}</td>
      <td className="follow-score" title={row.submitted ? `時間 ${elapsedText(row.elapsedSecs)}（ペナルティを含む）` : undefined}>
        {row.submitted ? (
          <>
            <div>
              {scoreText(row.score)}
              {row.penalty > 0 && <span className="follow-penalty"> ({row.penalty})</span>}
            </div>
            <div className="follow-sub">{elapsedText(row.elapsedSecs)}</div>
          </>
        ) : (
          <span className="follow-sub">提出なし</span>
        )}
      </td>
      {row.cells.map((cell, index) => (
        <Cell key={index} cell={cell} />
      ))}
    </tr>
  );
}

function FollowPanel({ contest, entry, loggedIn, loading, onRefresh, onLogin }: Props) {
  // 「更新」を続けて押せないようにする（AtCoder への通信を増やさない）
  const [cooling, setCooling] = useState(false);
  useEffect(() => {
    if (!cooling) {
      return;
    }
    const timer = setTimeout(() => setCooling(false), COOLDOWN_MS);
    return () => clearTimeout(timer);
  }, [cooling]);

  if (!contest) {
    return (
      <div className="follow-body">
        <div className="panel-empty">問題文の欄で問題を選ぶと、そのコンテストの順位表から出します</div>
      </div>
    );
  }
  const table = entry?.table ?? null;
  const others = table ? table.rows.filter((row) => !row.me).length : 0;
  return (
    <div className="follow-body">
      <div className="follow-head">
        <span className="follow-contest">{contest}</span>
        {entry?.readAtMs != null && (
          <span className="follow-time">
            {clock(entry.readAtMs)} に読んだ{table?.fixed ? "（確定）" : ""}
          </span>
        )}
        <button
          type="button"
          className="case-action"
          title="順位表を読み直します（自分の提出は読みません）"
          disabled={loading || cooling || !loggedIn}
          onClick={() => {
            setCooling(true);
            onRefresh();
          }}
        >
          {loading ? "読んでいます…" : "更新"}
        </button>
      </div>
      {(!loggedIn || entry?.notLoggedIn) && (
        <div className="submit-notice">
          順位表を読むには、AtCoder にログインしてください
          <button type="button" className="case-action" onClick={onLogin}>
            ログイン
          </button>
        </div>
      )}
      {entry?.message && <div className="submit-message">{entry.message}</div>}
      {table ? (
        <div className="follow-scroll">
          <table className="follow-table">
            <thead>
              <tr>
                <th title="追跡している人（自分を含む）の中での順位">#</th>
                <th>ユーザー</th>
                <th title="公式の順位表の順位">順位</th>
                <th>得点</th>
                {table.tasks.map((task) => (
                  <th key={task.task} title={task.task}>
                    {task.label}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {table.rows.map((row) => (
                <Row key={`${row.me ? "me" : "user"}-${row.user}`} row={row} columns={table.tasks.length} />
              ))}
            </tbody>
          </table>
        </div>
      ) : (
        loggedIn &&
        !entry?.message && <div className="panel-empty">「更新」で、このコンテストの順位表を読みます</div>
      )}
      {table && others === 0 && (
        <div className="submit-meta">設定の「追跡」で、追跡するユーザーを足せます</div>
      )}
      <div className="submit-footnote">
        順位表を読むのは「更新」と、サイドバーの「コンテスト」で読んだときだけです（決まった間隔では読みません）。
        追跡する ID は AtCoder に送らず、順位表から手元で取り出します。
      </div>
    </div>
  );
}

export default FollowPanel;
