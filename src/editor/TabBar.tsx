import type { WheelEvent } from "react";
import FileIcon from "../sidebar/FileIcon";
import type { Tab } from "./useTabs";
import { appName, isAppKey } from "./readOnlyKey";

// ファイルでないタブ（Phase 24 の「過去問」〔D-242〕・Phase 26 の「設定」〔D-246〕）。ファイルのタブの後ろに出す
export type ExtraTab = {
  label: string;
  active: boolean;
  onSelect: () => void;
  onClose: () => void;
};

type Props = {
  tabs: Tab[];
  activePath: string | null;
  // ファイルでないタブ（過去問・設定）。どれかが手前にあるときは、ファイルのタブはどれも手前にしない
  extras: ExtraTab[];
  onSelect: (path: string) => void;
  onClose: (path: string) => void;
};

function baseName(path: string) {
  return path.slice(path.lastIndexOf("/") + 1);
}

// 読み取り専用のタブのキーは URI なので、名前はパーセントエンコードを戻して出す
function decodeName(name: string) {
  try {
    return decodeURIComponent(name);
  } catch {
    return name;
  }
}

function folderParts(path: string) {
  return path.includes("/") ? path.slice(0, path.lastIndexOf("/")).split("/") : [];
}

// 同じ名前のほかのタブと見分けられる、一番内側のフォルダの名前（D-191）。内側から順に、同じ深さ（内側から数えて）の
// フォルダの名前がほかのどのタブとも違うものを探す。その外にまだフォルダがあれば「…/」を付ける。
// 見分けられないときは、フォルダ全体（作業領域の直下なら「./」）
export function shortFolder(parts: string[], others: string[][]): string {
  for (let depth = 0; depth < parts.length; depth++) {
    const part = parts[parts.length - 1 - depth];
    if (others.every((other) => other[other.length - 1 - depth] !== part)) {
      return parts.length - 1 - depth > 0 ? `…/${part}` : part;
    }
  }
  return parts.length > 0 ? parts.join("/") : "./";
}

// 同じ名前のファイルが複数開いているときは、見分けられるフォルダの名前を添える
function label(tab: Tab, tabs: Tab[]) {
  if (tab.readOnly) {
    return { name: decodeName(baseName(tab.path)), folder: null };
  }
  // アプリのフォルダのファイル（D-247）：作業領域に同じ名前のファイルのタブがあれば「アプリのフォルダ」を添える
  if (isAppKey(tab.path)) {
    const name = appName(tab.path);
    const same = tabs.some((t) => t !== tab && !t.readOnly && !isAppKey(t.path) && baseName(t.path) === name);
    return { name, folder: same ? "アプリのフォルダ" : null };
  }
  const name = baseName(tab.path);
  const others = tabs.filter((t) => t !== tab && !t.readOnly && !isAppKey(t.path) && baseName(t.path) === name);
  if (others.length === 0) {
    return { name, folder: null };
  }
  const folder = shortFolder(
    folderParts(tab.path),
    others.map((t) => folderParts(t.path)),
  );
  return { name, folder };
}

// タブの列の上でホイールを縦に回すと、横に送る（横の動きがあるときはそのまま。D-191）
function scrollSideways(event: WheelEvent<HTMLDivElement>) {
  if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
    event.currentTarget.scrollLeft += event.deltaY;
  }
}

// エディタのタブの列（作業 4b、原典 §4.5「複数ファイルのタブ」）
function TabBar({ tabs, activePath, extras, onSelect, onClose }: Props) {
  if (tabs.length === 0 && extras.length === 0) {
    return null;
  }
  const extraActive = extras.some((extra) => extra.active);
  return (
    <div className="tab-bar" role="tablist" onWheel={scrollSideways}>
      {tabs.map((tab) => {
        const { name, folder } = label(tab, tabs);
        const active = tab.path === activePath && !extraActive;
        return (
          <div
            key={tab.path}
            className={`tab ${active ? "active" : ""}`}
            role="tab"
            aria-selected={active}
            title={tab.readOnly ? tab.readOnly.location : isAppKey(tab.path) ? `アプリのフォルダの ${appName(tab.path)}` : tab.path}
            onClick={() => onSelect(tab.path)}
          >
            <FileIcon name={name} isDir={false} open={false} />
            <span className="tab-name">{name}</span>
            {folder && <span className="tab-folder">{folder}</span>}
            {tab.readOnly && <span className="tab-readonly">読み取り専用</span>}
            {tab.dirty && (
              <span className="tab-dirty" aria-label="未保存">
                ●
              </span>
            )}
            <button
              type="button"
              className="tab-close"
              aria-label={`${name} を閉じる`}
              onClick={(event) => {
                event.stopPropagation();
                onClose(tab.path);
              }}
            >
              ×
            </button>
          </div>
        );
      })}
      {extras.map((extra) => (
        <div
          key={extra.label}
          className={`tab ${extra.active ? "active" : ""}`}
          role="tab"
          aria-selected={extra.active}
          title={extra.label}
          onClick={extra.onSelect}
        >
          <span className="tab-name">{extra.label}</span>
          <button
            type="button"
            className="tab-close"
            aria-label={`${extra.label} を閉じる`}
            onClick={(event) => {
              event.stopPropagation();
              extra.onClose();
            }}
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}

export default TabBar;
