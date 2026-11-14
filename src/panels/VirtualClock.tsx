// ツールバーの右端の、バーチャル参加の残り時間（Phase 44。D-268）。「バーチャル abc437 残り 1:23:45」。
// 時間が終わったら「バーチャル abc437 終了」（結果はサイドバーの「コンテスト」の「結果を出す」で出す）。
// 数えるのはこの部品の中で 1 秒ごと（App 全体は描き直さない）。時計は Mac の時計を使う
import { useEffect, useState } from "react";
import type { VirtualCurrent } from "../ipc/virtualContest";
import { formatSpan } from "./ContestClock";

// active の間、秒の変わり目ごとに今の時刻（UNIX 時間の秒）を返し直す
export function useNowSeconds(active: boolean): number {
  const [now, setNow] = useState(() => Date.now() / 1000);
  useEffect(() => {
    if (!active) return;
    let timer = 0;
    const tick = () => {
      const current = Date.now();
      setNow(current / 1000);
      timer = window.setTimeout(tick, 1000 - (current % 1000) + 5);
    };
    tick();
    return () => window.clearTimeout(timer);
  }, [active]);
  return now;
}

function VirtualClock({ current }: { current: VirtualCurrent | null }) {
  const now = useNowSeconds(current !== null);
  if (!current) return null;
  const left = current.end - now;
  return (
    <span
      className={left > 0 ? "virtual-clock" : "virtual-clock virtual-clock-ended"}
      title="バーチャル参加（サイドバーの「コンテスト」で、やめる・結果を出す）"
    >
      バーチャル {current.contest} {left > 0 ? `残り ${formatSpan(left)}` : "終了"}
    </span>
  );
}

export default VirtualClock;
