// 右下パネル（原典 §4.8、D-043）：「提出結果」（Phase 6 作業 3。D-148）と「追跡」（Phase 31。D-253）のタブ。
// デバッグ情報は、サイドバーの「実行とデバッグ」に置く（Phase 12 作業 1。D-182）
import type { ReactNode } from "react";

export type BottomRightTab = "submissions" | "follow";

type Props = {
  tab: BottomRightTab;
  onTab: (tab: BottomRightTab) => void;
  // 判定を待っている提出がある（見出しに印を出す）
  judging: boolean;
  children: ReactNode;
};

function BottomRightPanel({ tab, onTab, judging, children }: Props) {
  const tabs: [BottomRightTab, string][] = [
    ["submissions", `提出結果${judging ? "（判定中）" : ""}`],
    ["follow", "追跡"],
  ];
  return (
    <div className="right-panel">
      <div className="panel-tabs" role="tablist">
        {tabs.map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            className={`panel-tab ${tab === id ? "active" : ""}`}
            onClick={() => onTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      {children}
    </div>
  );
}

export default BottomRightPanel;
