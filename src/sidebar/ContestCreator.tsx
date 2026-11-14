import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  cancelReservation,
  createContestPackages,
  describePackage,
  mainFile,
  reserveContest,
  stopScaffold,
  type ContestCreated,
  type PackageResult,
  type ScaffoldEvent,
} from "../ipc/scaffold";
import { atcoderSession, contestPeriod } from "../ipc/submit";

// 「コンテストを作る」（Phase 7a。D-151、docs/phase7/01-pre-start.md §3.3）。ファイル一覧の上の段に出す。
// コンテスト ID を入れて「作る」を押すと、問題の一覧を読み、問題ごとのパッケージを作業領域の直下に作る。
// 開始前で読めなかったら、開催期間を読み、24 時間以内なら「開始したら作る（予約）」を出す（Phase 43。D-267）。
// 予約すると Rust 側が開始の 1 秒後まで待って作る。待っている間も、この部品が答えを待つ（閉じても続く）

// 開始までこれより先のコンテストは予約しない（Rust 側の reserve.rs の MAX_AHEAD_SECS と同じ）
const MAX_AHEAD_SECS = 24 * 3600;

type Running = {
  kind: "running";
  contest: string;
  // total は一覧を読むまで null。current は今取っている問題
  total: number | null;
  current: { index: number; task: string } | null;
  results: PackageResult[];
  stopping: boolean;
};

type Phase =
  | { kind: "idle" }
  | Running
  | { kind: "done"; created: ContestCreated }
  // reserve は、予約できるときの開始時刻（UNIX 時間の秒）
  | { kind: "error"; message: string; reserve?: { contest: string; start: number } }
  | { kind: "cancelled"; contest: string };

// 予約して、開始を待っている（待っている間も、ほかのコンテストを作れる）。retry は読み直しを待っているときの知らせ
type Reservation = {
  contest: string;
  start: number;
  retry: string | null;
  loggedOut: boolean;
  cancelling: boolean;
};

type Props = {
  // 閉じている（作っている途中・予約している間でも、閉じて続けられる）
  hidden: boolean;
  // 入力欄の初めの値（今の問題のコンテスト）
  initialContest: string;
  // 作り終えた（止めたときも）。first は、作れた最初の問題の src/main.rs（なければ null）
  onCreated: (first: string | null) => void;
  // 予約した・予約が終わった（null）。ツールバーに予約を出すのに使う
  onReserve: (contest: string | null) => void;
  onClose: () => void;
};

function Results({ results }: { results: PackageResult[] }) {
  return (
    <ul className="contest-creator-results">
      {results.map((result) => (
        <li key={result.task} className={result.error !== null ? "failed" : undefined}>
          <span className="contest-creator-task">{result.task}</span>
          {describePackage(result)}
        </li>
      ))}
    </ul>
  );
}

// 終わったときの 1 行
function summary(created: ContestCreated): string {
  const made = created.packages.filter((result) => result.error === null).length;
  const head = `${created.contest}：${created.total} 問のうち ${made} 問を作りました`;
  return created.stopped ? `${head}（途中で止めました）` : head;
}

// 「2026-11-14 21:00」
export function formatStart(seconds: number): string {
  const d = new Date(seconds * 1000);
  const two = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}:${two(d.getMinutes())}`;
}

// 作っている間の知らせを、表示に足す
function applyEvent(current: Phase, event: ScaffoldEvent): Phase {
  if (current.kind !== "running") {
    return current;
  }
  switch (event.type) {
    case "listed":
      return { ...current, total: event.total };
    case "started":
      return { ...current, total: event.total, current: { index: event.index, task: event.task } };
    case "finished":
      return { ...current, current: null, results: [...current.results, event.result] };
  }
}

function runningOf(contest: string): Running {
  return { kind: "running", contest, total: null, current: null, results: [], stopping: false };
}

function ContestCreator({ hidden, initialContest, onCreated, onReserve, onClose }: Props) {
  const [contest, setContest] = useState(initialContest);
  const [phase, setPhase] = useState<Phase>({ kind: "idle" });
  const [reservation, setReservation] = useState<Reservation | null>(null);
  const running = phase.kind === "running";
  const inputRef = useRef<HTMLInputElement>(null);
  // 予約している（閉じる・フォルダを変えるなどで、この部品がなくなるときに取り消す）
  const reservedRef = useRef(false);
  // 今の「作っている」は予約から始めたもの（止めるときは予約を取り消す）
  const fromReservationRef = useRef(false);
  const onReserveRef = useRef(onReserve);
  onReserveRef.current = onReserve;

  // 開いたら、入力欄に移る
  useEffect(() => {
    if (!hidden) {
      inputRef.current?.focus();
    }
  }, [hidden]);

  useEffect(
    () => () => {
      if (reservedRef.current) {
        reservedRef.current = false;
        cancelReservation().catch(() => {});
        onReserveRef.current(null);
      }
    },
    [],
  );

  function finished(created: ContestCreated) {
    setPhase({ kind: "done", created });
    const first = created.packages.find((result) => result.error === null);
    onCreated(first ? mainFile(first) : null);
  }

  // 読めなかったとき。開始前なら、開催期間を読んで予約できるか決める（トップページ。覚えてあれば読まない）
  async function failed(id: string, message: string) {
    let period;
    try {
      period = await contestPeriod(id);
    } catch {
      setPhase({ kind: "error", message });
      return;
    }
    const now = Date.now() / 1000;
    if (now >= period.start) {
      setPhase({ kind: "error", message });
      return;
    }
    const head = `${id} は ${formatStart(period.start)} に始まります。開始前は問題の一覧を読めません`;
    if (period.start - now > MAX_AHEAD_SECS) {
      setPhase({ kind: "error", message: `${head}（開始まで 24 時間より先のコンテストは予約できません）` });
      return;
    }
    setPhase({ kind: "error", message: head, reserve: { contest: id, start: period.start } });
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const id = contest.trim();
    if (!id || running) {
      return;
    }
    setPhase(runningOf(id));
    try {
      const created = await createContestPackages(id, (event) => setPhase((current) => applyEvent(current, event)));
      finished(created);
    } catch (e) {
      await failed(id, String(e));
    }
  }

  async function reserve(id: string, start: number) {
    setPhase({ kind: "idle" });
    setReservation({ contest: id, start, retry: null, loggedOut: false, cancelling: false });
    reservedRef.current = true;
    onReserve(id);
    try {
      const created = await reserveContest(
        id,
        (event) => {
          switch (event.type) {
            case "waiting":
              // 予約できた。ログインしていなければ知らせる（予約はそのまま）
              atcoderSession()
                .then((view) => {
                  if (!view.loggedIn) {
                    setReservation((current) => current && { ...current, loggedOut: true });
                  }
                })
                .catch(() => {});
              setReservation((current) => current && { ...current, start: event.start });
              break;
            case "retrying":
              setReservation(
                (current) =>
                  current && {
                    ...current,
                    retry: `問題の一覧をまだ読めません（${event.message}）。${event.wait} 秒あとに読み直します（${event.attempt} 回目）`,
                  },
              );
              break;
            case "creating":
              // ここからは「コンテストを作る」と同じに出す
              setReservation(null);
              fromReservationRef.current = true;
              setPhase(runningOf(id));
              break;
          }
        },
        (event) => setPhase((current) => applyEvent(current, event)),
      );
      if (created) {
        finished(created);
      } else {
        setPhase({ kind: "cancelled", contest: id });
      }
    } catch (e) {
      setPhase({ kind: "error", message: String(e) });
    } finally {
      reservedRef.current = false;
      fromReservationRef.current = false;
      setReservation(null);
      onReserve(null);
    }
  }

  function stop() {
    setPhase((current) => (current.kind === "running" ? { ...current, stopping: true } : current));
    // 予約から作っているときは、取り消すと止まる
    (fromReservationRef.current ? cancelReservation() : stopScaffold()).catch(() => {});
  }

  function cancel() {
    setReservation((current) => current && { ...current, cancelling: true });
    cancelReservation().catch(() => {});
  }

  return (
    <div className="contest-creator" hidden={hidden}>
      <form className="contest-creator-form" onSubmit={(event) => void submit(event)}>
        <input
          ref={inputRef}
          className="contest-creator-input"
          value={contest}
          placeholder="コンテスト ID（例：abc480）"
          aria-label="作るコンテストの ID"
          spellCheck={false}
          disabled={running}
          onChange={(event) => setContest(event.target.value)}
        />

        {running ? (
          <button
            type="button"
            className="run-button contest-creator-button"
            aria-disabled={phase.stopping}
            onClick={stop}
          >
            {phase.stopping ? "止めています…" : "止める"}
          </button>
        ) : (
          <button type="submit" className="run-button contest-creator-button">
            作る
          </button>
        )}
        <button
          type="button"
          className="file-tree-action"
          title="閉じる"
          aria-label="閉じる"
          onClick={onClose}
        >
          ×
        </button>
      </form>
      {phase.kind === "running" && (
        <>
          <div className="contest-creator-status">
            {phase.total === null
              ? `${phase.contest} の問題の一覧を読んでいます…`
              : phase.current
                ? `${phase.current.index + 1} / ${phase.total}：${phase.current.task}`
                : `${phase.results.length} / ${phase.total}`}
          </div>
          <Results results={phase.results} />
        </>
      )}
      {phase.kind === "done" && (
        <>
          <div className="contest-creator-status">{summary(phase.created)}</div>
          {phase.created.configCreated.length > 0 && (
            <div className="contest-creator-note">
              {phase.created.configCreated.join("、")} を作りました（ビルドの置き場所を共有する）
            </div>
          )}
          <Results results={phase.created.packages} />
        </>
      )}
      {phase.kind === "error" && (
        <>
          <div className="contest-creator-status failed" role="alert">
            {phase.message}
          </div>
          {phase.reserve && (
            <button
              type="button"
              className="run-button contest-creator-reserve"
              onClick={() => phase.reserve && void reserve(phase.reserve.contest, phase.reserve.start)}
            >
              開始したら作る（予約）
            </button>
          )}
        </>
      )}
      {reservation && (
        <div className="contest-creator-reservation">
          <div className="contest-creator-status">
            予約：{reservation.contest} を、開始（{formatStart(reservation.start)}）の 1 秒後に作ります。アプリを閉じると予約は消えます
            <button
              type="button"
              className="contest-creator-cancel"
              aria-disabled={reservation.cancelling}
              onClick={cancel}
            >
              {reservation.cancelling ? "取り消しています…" : "取り消す"}
            </button>
          </div>
          {reservation.retry && <div className="contest-creator-note">{reservation.retry}</div>}
          {reservation.loggedOut && (
            <div className="contest-creator-status failed" role="alert">
              開催中のコンテストは、ログインしていないと読めません。開始までにログインしてください（予約はそのままです）
            </div>
          )}
        </div>
      )}
      {phase.kind === "cancelled" && (
        <div className="contest-creator-status">{phase.contest} の予約を取り消しました</div>
      )}
    </div>
  );
}

export default ContestCreator;
