// サイドバーの「コンテスト」（Phase 18 作業 4。D-218。docs/phase18/05-contest-plan.md）。
// 問題文の欄の問題のコンテストについて、問題ごとに AC・最後の判定・回数を出す。
// 読むのは、開いたとき（前に読んでから 1 分たっていれば）・コンテストが替わったとき・「更新」・ログインしたときだけ。
// IDE から出した提出は、通信せずに行に重ねる（判定を追っている間も、その表記を出す）。
// 続けて順位表を読み、自分の順位・予想パフォーマンス・各問題の予想 diff を出す（Phase 21。D-230〜D-233）。
// AtCoder Problems の diff がある問題（終わったコンテスト）は、そちらを出す（Phase 24。D-241）
import { useEffect, useRef, useState } from "react";
import type { ProblemId } from "../ipc/problem";
import {
  contestStatus,
  openSubmissionPage,
  type ContestStatus,
  type StandingsSummary,
  type SubmissionView,
} from "../ipc/submit";
import { overlaySubmissions } from "./contestStatus";
import { ratingClass } from "../rating";
import { difficultyHint, difficultyText, problemDifficulties, type DifficultyView } from "../ipc/atcoderProblems";

// 開き直したときに読み直すまでの間
const STALE_MS = 60_000;

type Props = {
  // 問題文の欄の問題のコンテスト。なければ null
  contest: string | null;
  // 問題文の欄の問題（その行を強調する）
  currentTask: string | null;
  // サイドバーで「コンテスト」を出している
  visible: boolean;
  loggedIn: boolean;
  // このアプリで出した提出
  submissions: SubmissionView[];
  onSelectTask: (id: ProblemId) => void;
  onLogin: () => void;
  // 中央の「過去問」のタブを開く（Phase 24。D-242）
  onOpenPast: () => void;
};

type Entry = {
  status: ContestStatus | null;
  // 最後に読もうとしたときの知らせ
  error: string | null;
  notLoggedIn: boolean;
  // 最後に読もうとした時刻（ミリ秒）
  triedAt: number;
  // 順位表から計算したもの。順位表だけ読めなかったときは、前のものを残す
  standings: StandingsSummary | null;
  // 順位表についての知らせ（始まる前・読めなかった理由）
  standingsMessage: string | null;
};

// 予想 perf の説明（D-231）
const PERFORMANCE_HINT =
  "各参加者の APerf を、順位表のレートと参加回数から逆算した近似で計算しています。公式の値とずれることがあります。開催中は、順位と参加者が変わるので値も変わります";
// 予想 diff の説明（D-232）
const DIFFICULTY_HINT = "AtCoder Problems と同じ方法で、順位表から推定した難易度（50% の確率で解けるレート）です";


// 順位の表示（同順位の平均は小数で出る）
function rankText(rank: number): string {
  return Number.isInteger(rank) ? String(rank) : rank.toFixed(1);
}

// コンテストごとに覚えておく（サイドバーを切り替えても、読み直さずに出す）
const cache = new Map<string, Entry>();

function clock(ms: number): string {
  const date = new Date(ms);
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function ContestView({
  contest,
  currentTask,
  visible,
  loggedIn,
  submissions,
  onSelectTask,
  onLogin,
  onOpenPast,
}: Props) {
  const [entry, setEntry] = useState<Entry | null>(null);
  const [loading, setLoading] = useState(false);
  // 読む順番（遅れて届いた、前のコンテストの結果を使わない）
  const seq = useRef(0);

  async function read(target: string) {
    const mine = ++seq.current;
    setLoading(true);
    const before = cache.get(target);
    const previous = before?.status ?? null;
    // 読めなかったときは、前の順位表のものと知らせを残す
    const kept = { standings: before?.standings ?? null, standingsMessage: before?.standingsMessage ?? null };
    let next: Entry;
    try {
      const outcome = await contestStatus(target);
      if (outcome.kind === "read") {
        const standings = outcome.status.standings;
        next = {
          status: outcome.status,
          error: null,
          notLoggedIn: false,
          triedAt: Date.now(),
          ...(standings.kind === "read"
            ? { standings: standings.summary, standingsMessage: null }
            : standings.kind === "notStarted"
              ? { standings: null, standingsMessage: "始まる前なので、順位表は読みません" }
              : { standings: kept.standings, standingsMessage: `順位表を読めませんでした：${standings.message}` }),
        };
      } else if (outcome.kind === "notLoggedIn") {
        next = { status: previous, error: null, notLoggedIn: true, triedAt: Date.now(), ...kept };
      } else {
        next = { status: previous, error: outcome.message, notLoggedIn: false, triedAt: Date.now(), ...kept };
      }
    } catch (e) {
      next = { status: previous, error: String(e), notLoggedIn: false, triedAt: Date.now(), ...kept };
    }
    cache.set(target, next);
    if (mine === seq.current) {
      setEntry(next);
      setLoading(false);
    }
  }

  // 開いたとき・コンテストが替わったとき。前に読んでから 1 分たっていなければ、覚えているものを出す
  useEffect(() => {
    if (!visible || !contest) {
      return;
    }
    const cached = cache.get(contest);
    if (cached && Date.now() - cached.triedAt < STALE_MS) {
      seq.current += 1;
      setEntry(cached);
      setLoading(false);
      return;
    }
    setEntry(cached ?? null);
    void read(contest);
  }, [visible, contest]);

  // 問題の行の AtCoder Problems の diff（Phase 24。D-241）。行の問題が変わったときだけ聞く
  const [apDiffs, setApDiffs] = useState<Record<string, DifficultyView>>({});
  const taskIds = entry?.status && entry.status.contest === contest ? entry.status.tasks.map((task) => task.task) : [];
  const taskKey = taskIds.join(",");
  useEffect(() => {
    if (!visible || taskIds.length === 0) {
      return;
    }
    let cancelled = false;
    problemDifficulties(taskIds)
      .then((found) => {
        if (!cancelled) {
          setApDiffs(found);
        }
      })
      .catch(() => {
        // 読めなければ、順位表からの予想 diff だけを出す
        if (!cancelled) {
          setApDiffs({});
        }
      });
    return () => {
      cancelled = true;
    };
  }, [visible, taskKey]);

  // ログインしたら、ログインを促していたときだけ読み直す
  useEffect(() => {
    if (loggedIn && visible && contest && entry?.notLoggedIn) {
      void read(contest);
    }
  }, [loggedIn]);

  if (!contest) {
    return (
      <div className="contest-view">
        <div className="sidebar-title">コンテスト</div>
        <p className="contest-note">問題文の欄で問題を選ぶと、そのコンテストの提出状況が出ます。</p>
        <button type="button" className="contest-link" onClick={onOpenPast}>
          過去問の一覧
        </button>
      </div>
    );
  }
  const status = entry?.status && entry.status.contest === contest ? entry.status : null;
  const tasks = status ? overlaySubmissions(status.tasks, contest, submissions) : [];
  const standings = status ? (entry?.standings ?? null) : null;
  const difficulties = new Map((standings?.difficulties ?? []).map((item) => [item.task, item.difficulty]));
  return (
    <div className="contest-view">
      <div className="contest-header">
        <span className="sidebar-title contest-title">{contest} の提出状況</span>
        <button type="button" className="contest-refresh" disabled={loading} onClick={() => void read(contest)}>
          更新
        </button>
      </div>
      <div className="contest-meta">
        {loading ? "読んでいます…" : status ? `${clock(status.readAtMs)} に読みました` : ""}
        {status?.truncated && `（新しい ${status.pages} ページ分だけを見ました）`}
      </div>
      {entry?.notLoggedIn && (
        <div className="contest-login">
          <p className="contest-note">ログインすると提出状況が出ます。</p>
          <button type="button" className="settings-button" onClick={onLogin}>
            ログイン
          </button>
        </div>
      )}
      {standings && (
        <div className="contest-standings">
          {standings.place ? (
            <div>
              順位 {standings.place.rank} 位（Rated 内 {rankText(standings.place.ratedRank)} 位 / {standings.ratedCount} 人
              {standings.place.isRated ? "" : "・Unrated の参加"}）
            </div>
          ) : (
            <div>参加していません</div>
          )}
          {standings.performance !== null && (
            <div title={PERFORMANCE_HINT}>
              予想 perf{" "}
              <span className={`contest-performance ${ratingClass(standings.performance)}`}>
                {standings.performance}
              </span>
              （近似）
            </div>
          )}
          {standings.note && <p className="contest-note">{standings.note}</p>}
        </div>
      )}
      {status && entry?.standingsMessage && <p className="contest-note">{entry.standingsMessage}</p>}
      {entry?.error && (
        <p className="contest-error" role="alert">
          {entry.error}
        </p>
      )}
      <ul className="contest-tasks">
        {tasks.map((task) => {
          const mark = task.accepted ? "✓" : task.count > 0 ? "✗" : "・";
          const state = task.accepted ? "accepted" : task.count > 0 ? "tried" : "none";
          const verdict = task.accepted ? "AC" : task.last ? `${task.last}（${task.count} 回）` : "未提出";
          const apDiff = apDiffs[task.task] ?? null;
          // AtCoder Problems の diff がなければ（開催中・直後など）、順位表からの予想 diff
          const difficulty = apDiff ? null : (difficulties.get(task.task) ?? null);
          return (
            <li key={task.task}>
              <button
                type="button"
                className={`contest-task contest-task-${state} ${task.task === currentTask ? "current" : ""}`}
                title={`${task.label ? `${task.label} - ` : ""}${task.title}${apDiff ? `\n${difficultyText(apDiff)}：${difficultyHint(apDiff)}` : ""}${difficulty !== null ? `\n予想 diff ${difficulty}：${DIFFICULTY_HINT}` : ""}`}
                onClick={() => onSelectTask({ contest, task: task.task })}
              >
                <span className="contest-mark" aria-hidden="true">
                  {mark}
                </span>
                <span className="contest-label">{task.label}</span>
                <span className="contest-name">{task.title}</span>
                {apDiff && (
                  <span className={`contest-diff ${ratingClass(apDiff.value)}`}>{difficultyText(apDiff)}</span>
                )}
                {difficulty !== null && (
                  <span className={`contest-diff ${ratingClass(difficulty)}`}>diff {difficulty}</span>
                )}
                <span className="contest-verdict">{verdict}</span>
              </button>
            </li>
          );
        })}
      </ul>
      <button
        type="button"
        className="contest-link"
        onClick={() => void openSubmissionPage(contest, null).catch(() => {})}
      >
        自分の提出を開く
      </button>
      <button type="button" className="contest-link" onClick={onOpenPast}>
        過去問の一覧
      </button>
    </div>
  );
}

export default ContestView;
