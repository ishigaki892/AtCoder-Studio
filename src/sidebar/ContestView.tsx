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
  type StandingsOutcome,
  type StandingsSummary,
  type SubmissionView,
} from "../ipc/submit";
import { overlaySubmissions } from "./contestStatus";
import { ratingClass } from "../rating";
import { difficultyHint, difficultyText, problemDifficulties, type DifficultyView } from "../ipc/atcoderProblems";
import {
  cancelVirtual,
  finishVirtual,
  scoreText,
  startVirtual,
  type VirtualFile,
  type VirtualRecord,
  type VirtualScore,
} from "../ipc/virtualContest";
import { formatSpan } from "../panels/ContestClock";
import { formatElapsed } from "../panels/ProblemClock";
import { useNowSeconds } from "../panels/VirtualClock";

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
  // 順位表を読んだ（右下の「追跡」の表にも使う。Phase 31。D-253）
  onStandings: (contest: string, standings: StandingsOutcome) => void;
  // 提出状況を読めたとき（AC した問題を App が覚え、経過時間を止める。Phase 42。D-266）
  onStatus: (status: ContestStatus) => void;
  // バーチャル参加（Phase 44。D-268）。読めていなければ null
  virtualFile: VirtualFile | null;
  onVirtual: (file: VirtualFile) => void;
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

// 「2026-11-14 18:00」
function dateText(seconds: number): string {
  const d = new Date(seconds * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

// 「得点 600・時間 45:12（誤答 1）」
function scoreLine(score: VirtualScore): string {
  const penalty = score.penalty > 0 ? `（誤答 ${score.penalty}）` : "";
  return `得点 ${scoreText(score.score)}・時間 ${formatElapsed(score.elapsed)}${penalty}`;
}

// 問題ごと：「A 3:12」「B 10:05（誤答 1）」「C —（誤答 2）」
function TaskScores({ score }: { score: VirtualScore }) {
  return (
    <ul className="virtual-tasks">
      {score.tasks.map((task) => (
        <li key={task.task} className={task.acSeconds !== null ? "solved" : undefined}>
          <span className="virtual-task-label">{task.label || task.task}</span>
          <span>{task.acSeconds !== null ? formatElapsed(task.acSeconds) : "—"}</span>
          {task.penalty > 0 && <span className="virtual-task-penalty">（誤答 {task.penalty}）</span>}
          {task.pending && <span className="virtual-task-penalty">判定中</span>}
        </li>
      ))}
    </ul>
  );
}

// 終えたバーチャル参加の 1 行目：「得点 600・時間 45:12・123 位 / 9000 人・予想 perf 1234」
function RecordLine({ record }: { record: VirtualRecord }) {
  return (
    <>
      {scoreLine(record.result)}
      {record.rank !== null && `・${record.rank} 位 / ${record.participants ?? "?"} 人`}
      {record.performance !== null && (
        <>
          ・予想 perf{" "}
          <span className={`contest-performance ${ratingClass(record.performance)}`} title={PERFORMANCE_HINT}>
            {record.performance}
          </span>
        </>
      )}
    </>
  );
}

type VirtualProps = {
  contest: string;
  file: VirtualFile | null;
  // 読んだ提出状況の、バーチャルの時間の中の結果
  live: VirtualScore | null;
  onVirtual: (file: VirtualFile) => void;
};

// バーチャル参加の欄（Phase 44。D-268）：始める・残り時間と今の結果・やめる／結果を出す・このコンテストの前の結果・一覧
function VirtualSection({ contest, file, live, onVirtual }: VirtualProps) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showAll, setShowAll] = useState(false);
  const current = file?.current ?? null;
  const mine = current !== null && current.contest === contest;
  const now = useNowSeconds(mine);

  async function run(action: () => Promise<VirtualFile | null>) {
    setBusy(true);
    setError(null);
    try {
      const next = await action();
      if (next) {
        onVirtual(next);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const results = file?.results ?? [];
  const last = results.find((record) => record.contest === contest) ?? null;
  const left = mine && current ? current.end - now : 0;
  return (
    <div className="virtual-section">
      {mine && current ? (
        <>
          <div className="virtual-title">
            バーチャル参加中：{left > 0 ? `残り ${formatSpan(left)}` : "時間が終わりました"}
          </div>
          {live ? (
            <>
              <div>{scoreLine(live)}</div>
              <TaskScores score={live} />
              <p className="contest-note">
                始めてからの提出を「更新」で読み直します（ペナルティは 1 つ 5 分として計算。結果を出すときは順位表から求めます）
              </p>
            </>
          ) : (
            <p className="contest-note">提出状況を読むと（「更新」）、始めてからの AC の時刻が出ます</p>
          )}
          <div className="virtual-actions">
            <button type="button" className="contest-link" disabled={busy} onClick={() => void run(finishVirtual)}>
              {busy ? "読んでいます…" : left > 0 ? "やめて結果を出す" : "結果を出す"}
            </button>
            <button type="button" className="contest-link" disabled={busy} onClick={() => void run(cancelVirtual)}>
              結果を残さずにやめる
            </button>
          </div>
        </>
      ) : current ? (
        <>
          <p className="contest-note">{current.contest} でバーチャル参加しています（バーチャル参加は 1 つずつです）</p>
          {/* そのコンテストの問題を出さなくても終えられる（B-6） */}
          <div className="virtual-actions">
            <button type="button" className="contest-link" disabled={busy} onClick={() => void run(finishVirtual)}>
              {busy ? "読んでいます…" : `${current.contest} の結果を出す`}
            </button>
            <button type="button" className="contest-link" disabled={busy} onClick={() => void run(cancelVirtual)}>
              結果を残さずにやめる
            </button>
          </div>
        </>
      ) : (
        <button
          type="button"
          className="contest-link"
          disabled={busy || file === null}
          title="終わったコンテストを、本番と同じ長さで解きます。提出はブラウザから行い、始めてからの提出で順位と予想 perf を出します"
          onClick={() => void run(() => startVirtual(contest))}
        >
          このコンテストでバーチャル参加
        </button>
      )}
      {error && (
        <p className="contest-error" role="alert">
          {error}
        </p>
      )}
      {last && !mine && (
        <div className="virtual-last">
          <div className="virtual-title">前のバーチャル参加（{dateText(last.start)}）</div>
          <div>
            <RecordLine record={last} />
          </div>
          <TaskScores score={last.result} />
          {last.note && <p className="contest-note">{last.note}</p>}
        </div>
      )}
      {results.length > 0 && (
        <details className="virtual-history" open={showAll} onToggle={(e) => setShowAll(e.currentTarget.open)}>
          <summary>前のバーチャル参加（{results.length}）</summary>
          <ul>
            {results.map((record) => (
              <li key={`${record.contest}-${record.start}`} title={record.note ?? undefined}>
                <span className="virtual-history-contest">{record.contest}</span>
                <span className="virtual-history-date">{dateText(record.start)}</span>
                <span>
                  <RecordLine record={record} />
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
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
  onStandings,
  onStatus,
  virtualFile,
  onVirtual,
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
        onStandings(target, standings);
        onStatus(outcome.status);
        next = {
          status: outcome.status,
          error: null,
          notLoggedIn: false,
          triedAt: Date.now(),
          ...(standings.kind === "read"
            ? { standings: standings.summary, standingsMessage: null }
            : standings.kind === "notStarted"
              ? { standings: null, standingsMessage: "始まる前なので、順位表は読みません" }
              : standings.kind === "notLoggedIn"
                ? { standings: kept.standings, standingsMessage: "ログインしていないので、順位表を読めませんでした" }
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
      <VirtualSection
        contest={contest}
        file={virtualFile}
        live={status?.virtualScore ?? null}
        onVirtual={onVirtual}
      />
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
