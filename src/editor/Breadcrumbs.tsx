import { Fragment, useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode, WheelEvent } from "react";
import FileIcon from "../sidebar/FileIcon";
import { listDirectory } from "../ipc/workspace";
import { headingsOf, isMarkdownPath, outlineTrail, subscribeCursor } from "./breadcrumbTrail";
import type { CursorInfo, OutlineNode } from "./breadcrumbTrail";
import { ATTACHED_EVENT, documentSymbols } from "./lsp";
import { QUIESCENT_EVENT } from "./inlayHints";
import { isCppPath } from "./language";
import { activeEditorTarget, moveCursorTo } from "./Editor";
import type { Tab } from "./useTabs";
import { appName, isAppKey } from "./readOnlyKey";

type Props = {
  tab: Tab | null;
  // 一覧で選んだファイルを開く（作業領域からの相対パス）
  onOpenFile: (path: string) => void;
};

// 編集が止まってから記号を聞くまでの待ち時間（D-216）
const SYMBOL_DELAY_MS = 500;

// 読み取り専用のタブのキーは URI なので、名前はパーセントエンコードを戻して出す
function decodeName(name: string) {
  try {
    return decodeURIComponent(name);
  } catch {
    return name;
  }
}

// 縦のホイールで横に送る（タブの列と同じ。D-191）
function scrollSideways(event: WheelEvent<HTMLElement>) {
  if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
    event.currentTarget.scrollLeft += event.deltaY;
  }
}

// 記号の種類（LSP の SymbolKind）の小さな印と色。見出し（null）には付けない
function kindIcon(kind: number | null): ReactNode {
  if (kind === null) {
    return null;
  }
  let glyph = "•";
  let group = "other";
  if ([6, 9, 12, 25].includes(kind)) {
    // メソッド・コンストラクター・関数・演算子
    glyph = "ƒ";
    group = "function";
  } else if ([5, 10, 11, 19, 23, 26].includes(kind)) {
    // クラス・列挙・インターフェース・オブジェクト（rust-analyzer の impl）・構造体・型引数
    glyph = "◇";
    group = "type";
  } else if ([2, 3, 4].includes(kind)) {
    // モジュール・名前空間・パッケージ
    glyph = "{}";
    group = "module";
  } else if ([7, 8, 13, 14, 20, 22, 24].includes(kind)) {
    // プロパティ・フィールド・変数・定数・キー・列挙の値・イベント
    glyph = "▪";
    group = "variable";
  }
  return (
    <span className={`symbol-icon symbol-icon-${group}`} aria-hidden="true">
      {glyph}
    </span>
  );
}

function isCodePath(path: string) {
  return path.endsWith(".rs") || isCppPath(path);
}

// エディタのカーソルの位置と文書（パンくずリストだけが受け取る）
function useCursor(): CursorInfo | null {
  const [cursor, setCursor] = useState<CursorInfo | null>(null);
  useEffect(() => subscribeCursor(setCursor), []);
  return cursor;
}

// 言語サーバーの記号（D-216）。タブを替えたとき・編集が止まったとき・サーバーにつないだとき・読み込みが終わったときに聞く
function useSymbols(key: string | null, cursor: CursorInfo | null): OutlineNode[] {
  const [symbols, setSymbols] = useState<{ key: string; list: OutlineNode[] } | null>(null);
  const doc = cursor && cursor.key === key ? cursor.state.doc : null;
  useEffect(() => {
    if (!key) {
      return;
    }
    let cancelled = false;
    const request = () => {
      void documentSymbols(key).then((result) => {
        if (!cancelled && result) {
          setSymbols({ key, list: result.symbols });
        }
      });
    };
    const timer = setTimeout(request, SYMBOL_DELAY_MS);
    window.addEventListener(ATTACHED_EVENT, request);
    window.addEventListener(QUIESCENT_EVENT, request);
    return () => {
      cancelled = true;
      clearTimeout(timer);
      window.removeEventListener(ATTACHED_EVENT, request);
      window.removeEventListener(QUIESCENT_EVENT, request);
    };
  }, [key, doc]);
  return symbols && symbols.key === key ? symbols.list : [];
}

type MenuItem = { key: string; label: string; icon: ReactNode; current: boolean; choose: () => void };
type Menu = { segment: string; left: number; items: MenuItem[] };

// 段を押したときの一覧（D-216）。↑↓で移り、Enter で選び、Esc で閉じる
function BreadcrumbMenu({ menu, onClose }: { menu: Menu; onClose: () => void }) {
  const listRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const buttons = listRef.current?.querySelectorAll<HTMLButtonElement>("button");
    const current = menu.items.findIndex((item) => item.current);
    buttons?.[Math.max(current, 0)]?.focus();
    buttons?.[Math.max(current, 0)]?.scrollIntoView({ block: "nearest" });
  }, [menu]);
  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const buttons = [...(listRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? [])];
    const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      const next = event.key === "ArrowDown" ? index + 1 : index - 1;
      buttons[Math.min(Math.max(next, 0), buttons.length - 1)]?.focus();
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onClose();
      activeEditorTarget()?.view.focus();
    }
  }
  return (
    <div className="breadcrumb-menu" role="menu" ref={listRef} style={{ left: menu.left }} onKeyDown={onKeyDown}>
      {menu.items.length === 0 && <div className="breadcrumb-menu-empty">（空です）</div>}
      {menu.items.map((item) => (
        <button
          key={item.key}
          type="button"
          role="menuitem"
          className={`breadcrumb-menu-item ${item.current ? "current" : ""}`}
          onClick={item.choose}
        >
          {item.icon}
          <span className="breadcrumb-menu-label">{item.label}</span>
        </button>
      ))}
    </div>
  );
}

// パンくずリスト（Phase 18 作業 1・2。D-216）。タブの列の下に、作業領域からの道筋・ファイル・記号（Markdown は見出し）を出す。
// 読み取り専用のタブは、ファイルの名前だけ。設定で切ったときは CSS で隠す（body.hide-breadcrumbs）。
// フォルダの段を押すとそのフォルダの中身、ファイルの段を押すと同じフォルダの中身、記号の段を押すと同じ段の記号の一覧が出る
function Breadcrumbs({ tab, onOpenFile }: Props) {
  const cursor = useCursor();
  const codeKey = tab && !tab.readOnly && isCodePath(tab.path) ? tab.path : null;
  const symbols = useSymbols(codeKey, cursor);
  const [menu, setMenu] = useState<Menu | null>(null);
  const barRef = useRef<HTMLDivElement>(null);
  // 一覧の読み込みの順番（遅れて届いた古い一覧を使わない）
  const menuSeq = useRef(0);

  // タブを替えたら一覧を閉じる
  useEffect(() => {
    setMenu(null);
  }, [tab?.path]);

  // 一覧の外を押したら閉じる（段を押したときは、その段の処理で開き直す）
  useEffect(() => {
    if (!menu) {
      return;
    }
    const onMouseDown = (event: MouseEvent) => {
      if (!barRef.current?.contains(event.target as Node)) {
        setMenu(null);
      }
    };
    document.addEventListener("mousedown", onMouseDown);
    return () => document.removeEventListener("mousedown", onMouseDown);
  }, [menu]);

  if (!tab) {
    return null;
  }
  const path = tab.path;
  // アプリのフォルダのファイル（D-247）は、作業領域のフォルダを辿れないので、読み取り専用のタブと同じく名前だけを出す
  const app = isAppKey(path);
  const parts = path.split("/");
  const file = app ? appName(path) : tab.readOnly ? decodeName(parts[parts.length - 1]) : parts[parts.length - 1];
  const folders = tab.readOnly || app ? [] : parts.slice(0, -1);

  let outline: OutlineNode[] = [];
  if (cursor && cursor.key === path && !tab.readOnly) {
    outline = isMarkdownPath(path) ? headingsOf(cursor.state.doc) : symbols;
  }
  const trail = cursor && cursor.key === path ? outlineTrail(outline, cursor.state.selection.main.head) : [];

  function leftOf(element: HTMLElement) {
    const bar = barRef.current?.getBoundingClientRect().left ?? 0;
    return element.getBoundingClientRect().left - bar;
  }

  function close() {
    menuSeq.current += 1;
    setMenu(null);
  }

  // フォルダの中身の一覧。フォルダを選ぶと、その中身に替える。ファイルを選ぶと開く
  function openFolder(segment: string, dir: string, left: number, currentName: string | null) {
    const seq = ++menuSeq.current;
    listDirectory(dir)
      .then((entries) => {
        if (seq !== menuSeq.current) {
          return;
        }
        setMenu({
          segment,
          left,
          items: (entries ?? []).map((entry) => ({
            key: entry.path,
            label: entry.name,
            icon: <FileIcon name={entry.name} isDir={entry.is_dir} open={false} />,
            current: entry.name === currentName,
            choose: entry.is_dir
              ? () => openFolder(segment, entry.path, left, null)
              : () => {
                  close();
                  onOpenFile(entry.path);
                },
          })),
        });
      })
      .catch(() => close());
  }

  function toggle(segment: string, element: HTMLElement, open: (left: number) => void) {
    if (menu?.segment === segment) {
      close();
      return;
    }
    open(leftOf(element));
  }

  function openOutline(segment: string, left: number, siblings: OutlineNode[], node: OutlineNode) {
    menuSeq.current += 1;
    setMenu({
      segment,
      left,
      items: siblings.map((sibling, index) => ({
        key: `${index}-${sibling.at}`,
        label: sibling.name,
        icon: kindIcon(sibling.kind),
        current: sibling === node,
        choose: () => {
          close();
          moveCursorTo(path, sibling.at);
        },
      })),
    });
  }

  const separator = (
    <span className="breadcrumb-separator" aria-hidden="true">
      ›
    </span>
  );
  return (
    <div className="breadcrumbs" ref={barRef}>
      <nav className="breadcrumbs-path" aria-label="パンくずリスト" onWheel={scrollSideways} onScroll={close}>
        {folders.map((folder, index) => {
          const segment = `folder-${index}`;
          const dir = parts.slice(0, index + 1).join("/");
          return (
            <Fragment key={segment}>
              <button
                type="button"
                className="breadcrumb"
                onClick={(event) => toggle(segment, event.currentTarget, (left) => openFolder(segment, dir, left, parts[index + 1]))}
              >
                {folder}
              </button>
              {separator}
            </Fragment>
          );
        })}
        {app && (
          <>
            <span className="breadcrumb">アプリのフォルダ</span>
            {separator}
          </>
        )}
        {tab.readOnly || app ? (
          <span className="breadcrumb breadcrumb-file">
            <FileIcon name={file} isDir={false} open={false} />
            {file}
          </span>
        ) : (
          <button
            type="button"
            className="breadcrumb breadcrumb-file"
            onClick={(event) =>
              toggle("file", event.currentTarget, (left) => openFolder("file", folders.join("/"), left, file))
            }
          >
            <FileIcon name={file} isDir={false} open={false} />
            {file}
          </button>
        )}
        {trail.map((step, index) => {
          const segment = `symbol-${index}`;
          return (
            <Fragment key={segment}>
              {separator}
              <button
                type="button"
                className="breadcrumb"
                onClick={(event) =>
                  toggle(segment, event.currentTarget, (left) => openOutline(segment, left, step.siblings, step.node))
                }
              >
                {kindIcon(step.node.kind)}
                {step.node.name}
              </button>
            </Fragment>
          );
        })}
      </nav>
      {menu && <BreadcrumbMenu menu={menu} onClose={close} />}
    </div>
  );
}

export default Breadcrumbs;
