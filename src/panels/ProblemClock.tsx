// 問題文の欄の制限の行の、問題ごとの経過時間（Phase 42。D-266）。その問題を問題文の欄に出していた時間を足していき、
// 「⏱ 12:34」と出す。AC が分かったら止めて「AC まで 12:34」と出す。「⟲」で（確かめてから）0 に戻す。
// 数えるのはこの部品の中で 1 秒ごと（問題文の欄は描き直さない）。時計は Mac の時計を使い、前に数えたときとの差を足す。
// アプリが手前にないときも数える。差が MAX_GAP より大きいとき（スリープから起きたなど）は、その間を数えない。
// 時間は Rust 側（problem_times.rs）の problem-times.json に、数えている間は 5 秒ごと、問題を切り替えたとき・AC のときに書く。
// 止めるのは、数え始めた時刻（startedAt）より後の AC だけ。前に AC した問題は「⏱ —」と出し、「⟲」で数え始める
// （docs/bug-review-2026-11-14.md の B-1。startedAt のない Phase 42 の記録は、前と同じく AC が分かれば止める）
import { useEffect, useRef, useState } from "react";
import { problemTimes, resetProblemTime, setProblemTime, type ProblemTime, type ProblemTimes } from "../ipc/problemTimes";

const TICK_MS = 1000;
const SAVE_EVERY_MS = 5000;
// これより大きい差は、スリープなどで止まっていたとみなして数えない。ウィンドウが隠れている間は、macOS が
// 画面側のタイマーを間引く（1 分ごとくらいになる）ことがあるので、それより長くしておく【推測】
const MAX_GAP_MS = 120_000;

// 読んだ時間（アプリで 1 回だけ読み、あとはここを書き換える）
let store: ProblemTimes = {};
// 送っている保存（保存と「⟲」は 1 つずつ、順に送る。古い時間が後から書かれないように。B-9）
let saving: Promise<void> = Promise.resolve();
let loading: Promise<void> | null = null;
function loadStore(): Promise<void> {
  if (!loading) {
    loading = problemTimes()
      .then((times) => {
        store = times;
      })
      .catch(() => {});
  }
  return loading;
}

// 秒数を「4:05」「1:02:03」の形にする（切り捨て）
export function formatElapsed(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return hours > 0 ? `${hours}:${two(minutes)}:${two(rest)}` : `${minutes}:${two(rest)}`;
}

type Props = {
  // 問題 ID
  task: string;
  // 設定の「経過時間を出す」（オフなら出さず、数えない）
  enabled: boolean;
  // この問題のいちばん新しい AC の時刻（UNIX 時間の秒。IDE から提出した判定か、サイドバーの「コンテスト」の提出状況。
  // 時刻が分からない AC は 0）。AC がなければ null
  acceptedAt: number | null;
  onError: (message: string) => void;
};

// 止める AC があるか：数え始めた時刻より後の AC（数え始めた時刻のない記録は、どの AC でも）
function acceptedSinceStart(time: ProblemTime, acceptedAt: number | null): boolean {
  return acceptedAt !== null && (time.startedAt === undefined || acceptedAt >= time.startedAt);
}

type Current = { task: string; time: ProblemTime; dirty: boolean; savedAt: number };

function ProblemClock({ task, enabled, acceptedAt, onError }: Props) {
  const [ready, setReady] = useState(false);
  const [, setTick] = useState(0);
  const current = useRef<Current | null>(null);
  const last = useRef(Date.now());
  const acceptedRef = useRef(acceptedAt);
  acceptedRef.current = acceptedAt;
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  // 「⟲」の確かめから 0 に戻すまでの間は、保存を送らない（0 に戻したあとに、前の時間が書かれないように。B-9）
  const resetting = useRef(false);

  function save() {
    const c = current.current;
    if (!c || !c.dirty || resetting.current) {
      return;
    }
    c.dirty = false;
    c.savedAt = Date.now();
    const time: ProblemTime = { ...c.time, seconds: Math.floor(c.time.seconds) };
    store[c.task] = time;
    const id = c.task;
    saving = saving
      .then(() => setProblemTime(id, time))
      .catch((e) => onErrorRef.current(`経過時間を保存できません：${String(e)}`));
  }

  useEffect(() => {
    void loadStore().then(() => setReady(true));
  }, []);

  // 問題が変わったら、前の問題の時間を書いてから、その問題の時間を出す
  useEffect(() => {
    if (!ready) {
      return;
    }
    current.current = { task, time: { ...(store[task] ?? { seconds: 0 }) }, dirty: false, savedAt: Date.now() };
    last.current = Date.now();
    setTick((n) => n + 1);
    return () => save();
  }, [task, ready]);

  // 1 秒ごとに数える。AC が分かっている・止まっている問題は数えない
  useEffect(() => {
    if (!ready || !enabled) {
      return;
    }
    last.current = Date.now();
    const timer = window.setInterval(() => {
      const now = Date.now();
      const gap = now - last.current;
      last.current = now;
      const c = current.current;
      if (!c || c.time.acSeconds !== undefined || acceptedSinceStart(c.time, acceptedRef.current)) {
        return;
      }
      if (gap > 0 && gap <= MAX_GAP_MS) {
        // 初めて数えるときに、数え始めた時刻を覚える
        c.time.startedAt ??= Math.floor((now - gap) / 1000);
        c.time.seconds += gap / 1000;
        c.dirty = true;
      }
      if (now - c.savedAt >= SAVE_EVERY_MS) {
        save();
      }
      setTick((n) => n + 1);
    }, TICK_MS);
    return () => {
      window.clearInterval(timer);
      save();
    };
  }, [ready, enabled]);

  // 数え始めたあとの AC が分かったら止める（数えていた問題だけ。前に AC していて、時間を数えていない問題は何も書かない）
  useEffect(() => {
    const c = current.current;
    if (!ready || !enabled || !c || c.task !== task || c.time.acSeconds !== undefined) {
      return;
    }
    if (!acceptedSinceStart(c.time, acceptedAt)) {
      return;
    }
    if (c.time.seconds < 1) {
      return;
    }
    c.time.seconds = Math.floor(c.time.seconds);
    c.time.acSeconds = c.time.seconds;
    c.dirty = true;
    save();
    setTick((n) => n + 1);
  }, [acceptedAt, task, ready, enabled]);

  async function reset() {
    let time: ProblemTime | null;
    resetting.current = true;
    try {
      // 先に送った保存が終わってから 0 に戻す
      await saving;
      time = await resetProblemTime(task);
    } catch (e) {
      onError(`経過時間を戻せません：${String(e)}`);
      return;
    } finally {
      resetting.current = false;
    }
    if (!time) {
      return;
    }
    store[task] = time;
    if (current.current?.task === task) {
      current.current = { task, time: { ...time }, dirty: false, savedAt: Date.now() };
    }
    last.current = Date.now();
    setTick((n) => n + 1);
  }

  const c = current.current;
  if (!enabled || !ready || !c || c.task !== task) {
    return null;
  }
  const stopped = c.time.acSeconds !== undefined;
  // 前に AC していて、時間を数えていない問題は「⏱ —」（「⟲」で数え始める）
  const before = !stopped && acceptedSinceStart(c.time, acceptedAt) && c.time.seconds < 1;
  return (
    <span className="problem-clock">
      {stopped ? (
        <span title="AC が分かるまでに、この問題を問題文の欄に出していた時間">AC まで {formatElapsed(c.time.acSeconds ?? 0)}</span>
      ) : before ? (
        <span title="前に AC した問題なので数えていません。「⟲」を押すと、0 から数え始めます（そのあとの AC で止まります）">⏱ —</span>
      ) : (
        <span title="この問題を問題文の欄に出していた時間（AC が分かったら止まります）">⏱ {formatElapsed(c.time.seconds)}</span>
      )}
      <button type="button" className="problem-clock-reset" title="経過時間を 0 に戻して数え直す" aria-label="経過時間を数え直す" onClick={() => void reset()}>
        ⟲
      </button>
    </span>
  );
}

export default ProblemClock;
