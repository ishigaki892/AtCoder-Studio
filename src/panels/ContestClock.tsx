// ツールバーの右端の、コンテストの開始までの時間・残り時間（Phase 22 作業 1。D-238）。
// どのコンテストかは App が決める（今の問題のコンテスト。選択欄で「一覧」を押したら、そのコンテスト）。
// 開催期間は Rust 側がコンテストのトップページから読む（コンテストが変わったときだけ頼む）。
// 数えるのはこの部品の中で 1 秒ごと（App 全体は描き直さない）。時計は Mac の時計を使う。
// 終わったあと・問題が決まっていないとき・読めないときは何も出さない
import { useEffect, useRef, useState } from "react";
import { contestPeriod, watchContestStart, type ContestPeriod } from "../ipc/submit";

// 開催中で、残りがこれを切ったら注意の色にする（5 分）
const WARN_SECONDS = 5 * 60;

// 秒数を「42:15」「1:42:15」「3 日 4:05:00」の形にする
export function formatSpan(seconds: number): string {
  const total = Math.max(0, Math.ceil(seconds));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  if (days > 0) return `${days} 日 ${hours}:${two(minutes)}:${two(rest)}`;
  if (hours > 0) return `${hours}:${two(minutes)}:${two(rest)}`;
  return `${minutes}:${two(rest)}`;
}

// 「2026-10-26 21:00〜22:40」（日付が変われば終わりにも日付を付ける）
function formatPeriod(period: ContestPeriod): string {
  const parts = (seconds: number) => {
    const d = new Date(seconds * 1000);
    const two = (n: number) => String(n).padStart(2, "0");
    return {
      date: `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`,
      time: `${two(d.getHours())}:${two(d.getMinutes())}`,
    };
  };
  const start = parts(period.start);
  const end = parts(period.end);
  const endText = start.date === end.date ? end.time : `${end.date} ${end.time}`;
  return `${start.date} ${start.time}〜${endText}`;
}

function ContestClock({ contest }: { contest: string | null }) {
  // 読んだ開催期間（どのコンテストのものか、と合わせて覚える）。readAt は読んだときの時刻（秒）
  const [period, setPeriod] = useState<(ContestPeriod & { contest: string; readAt: number }) | null>(null);
  // 終了時刻を過ぎたあとに読み直した「コンテスト@終了時刻」（同じ終了時刻では 1 回だけ。C-3）
  const recheckedRef = useRef<string | null>(null);
  const [now, setNow] = useState(() => Date.now() / 1000);

  // コンテストが変わったときだけ頼む（同じコンテストの別の問題では頼まない）
  useEffect(() => {
    if (contest === null) return;
    let cancelled = false;
    contestPeriod(contest)
      .then((read) => {
        if (cancelled) return;
        // 数えるのを止めていた間に古くなった「今」を、ここで合わせる（終わったコンテストを一瞬出さない）
        const readAt = Date.now() / 1000;
        setNow(readAt);
        setPeriod({ ...read, contest, readAt });
      })
      .catch(() => {
        // 読めないときは何も出さない（ほかの機能の邪魔をしない）
      });
    return () => {
      cancelled = true;
    };
  }, [contest]);

  const shown = period !== null && period.contest === contest ? period : null;
  const ended = shown === null || now >= shown.end;

  // 数えている間に終了時刻を過ぎたら、1 回だけ読み直す（延長に追いつく。C-3）。延びていれば数え続ける。
  // 読んだときにもう終わっていたコンテスト（過去のコンテスト）は読み直さない
  const passedEnd = shown !== null && now >= shown.end && shown.readAt < shown.end;
  useEffect(() => {
    if (!passedEnd || shown === null) return;
    const key = `${shown.contest}@${shown.end}`;
    if (recheckedRef.current === key) return;
    recheckedRef.current = key;
    const { contest: target, end } = shown;
    let cancelled = false;
    contestPeriod(target, true)
      .then((read) => {
        if (cancelled || read.end <= end) return;
        const readAt = Date.now() / 1000;
        setNow(readAt);
        setPeriod({ ...read, contest: target, readAt });
      })
      .catch(() => {
        // 読めなければ、終わったものとして何も出さない
      });
    return () => {
      cancelled = true;
    };
  }, [passedEnd, shown?.contest, shown?.end]);

  // 出しているコンテストを Rust 側に伝える。開始の通知は、Rust 側が開始の時刻まで待って出す（D-239。窓が後ろにある
  // 間は画面のタイマーが間引かれることがあるため、画面では数えない）。始まっても伝え直さない（伝え直すと待つのをやめる）
  const watched = shown?.contest ?? null;
  useEffect(() => {
    watchContestStart(watched).catch(() => {
      // 伝えられなくても、残り時間はそのまま出す
    });
  }, [watched]);

  // 終わるまで、秒の変わり目ごとに数え直す
  useEffect(() => {
    if (ended) return;
    let timer = 0;
    const tick = () => {
      const current = Date.now();
      setNow(current / 1000);
      timer = window.setTimeout(tick, 1000 - (current % 1000) + 5);
    };
    tick();
    return () => window.clearTimeout(timer);
  }, [ended]);

  if (shown === null || ended) return null;
  const before = now < shown.start;
  const left = (before ? shown.start : shown.end) - now;
  const warn = !before && left <= WARN_SECONDS;
  return (
    <span className="contest-clock-wrap" data-tip={formatPeriod(shown)}>
      <span className={warn ? "contest-clock contest-clock-warn" : "contest-clock"} aria-live="off">
        {before ? "開始まで " : "残り "}
        {formatSpan(left)}
      </span>
    </span>
  );
}

export default ContestClock;
