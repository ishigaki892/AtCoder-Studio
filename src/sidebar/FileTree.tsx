import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import FileIcon from "./FileIcon";
import { isImeKey } from "../keybindings";
import {
  createNewFile,
  createNewFolder,
  deleteFile,
  listDirectory,
  renameExistingFile,
  type DirEntry,
} from "../ipc/workspace";

type Props = {
  // 作業領域の名前（一覧の上端に表示する）
  workspaceName: string;
  // 開いているファイルの相対パス（一覧で強調する）
  activePath: string | null;
  // 未保存の変更があるタブのファイル（原典 §4.3「未保存状態の表示」）
  dirtyPaths: string[];
  // 一覧を読み直すきっかけ。値が変わると、開いているフォルダを読み直す
  refreshKey: number;
  onOpenFile: (path: string) => void;
  onCreated: (path: string) => void;
  // 名前を変える前に呼ぶ。false なら名前を変えない（開いているタブを保存できなかったとき）
  onBeforeRename: (path: string) => Promise<boolean>;
  onRenamed: (from: string, to: string) => void;
  onDeleted: (path: string) => void;
  // フォルダの名前を変える前に呼ぶ（中のタブを保存する）。false なら変えない（D-157）
  onBeforeRenameFolder: (path: string) => Promise<boolean>;
  // フォルダの名前を変えた・消した（中のタブ・ブレークポイントを付け替える・閉じる。D-157）
  onRenamedFolder: (from: string, to: string) => void;
  onDeletedFolder: (path: string) => void;
  onRefresh: () => void;
  onError: (message: string) => void;
  // 「コンテストを作る」（Phase 7a。D-151）：見出しのボタンで開け閉めし、中身は見出しと一覧の間に出す
  onToggleContest: () => void;
  contestPanel: ReactNode;
};

// 名前を入力している状態。新しいファイル・フォルダはフォルダの先頭に、名前の変更はその項目の場所に入力欄を出す
type Editing =
  | { kind: "create"; dir: string; folder: boolean }
  | { kind: "rename"; path: string; name: string; isDir: boolean };

// 右クリックのメニュー（アプリの画面の中に描く。D-071）。フォルダにも出す（D-157）
type Menu = { path: string; name: string; isDir: boolean; x: number; y: number };

function isUnder(path: string, folder: string) {
  return path.startsWith(`${folder}/`);
}

type TreeState = Props & {
  expanded: Set<string>;
  toggle: (path: string) => void;
  // フォルダを閉じる（外で消えたフォルダ。中のフォルダも閉じる。D-156）
  collapse: (path: string) => void;
  renameExpanded: (from: string, to: string) => void;
  select: (dir: string) => void;
  editing: Editing | null;
  setEditing: (editing: Editing | null) => void;
  openMenu: (menu: Menu) => void;
};

function parentOf(path: string) {
  return path.includes("/") ? path.slice(0, path.lastIndexOf("/")) : "";
}

// 名前の入力欄。Enter で決定、Esc で取り消し、ほかを押して入力欄から離れても取り消す
function NameInput({
  initial,
  depth,
  onSubmit,
  onCancel,
}: {
  initial: string;
  depth: number;
  onSubmit: (name: string) => void;
  onCancel: () => void;
}) {
  const [value, setValue] = useState(initial);
  function handleKeyDown(event: KeyboardEvent<HTMLInputElement>) {
    // ⌘B などのアプリのキー割り当てを、入力中は働かせない
    event.stopPropagation();
    // 日本語入力の変換を確定・取り消す Enter・Esc は、名前の確定・取り消しに使わない（C-6）
    if (isImeKey(event.nativeEvent)) {
      return;
    }
    if (event.key === "Enter" && value.trim() !== "") {
      onSubmit(value.trim());
    } else if (event.key === "Escape") {
      onCancel();
    }
  }
  return (
    <input
      className="file-tree-input"
      style={{ marginLeft: 12 + depth * 12 }}
      value={value}
      autoFocus
      spellCheck={false}
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={handleKeyDown}
      onBlur={onCancel}
    />
  );
}

// フォルダの中身。開いたときに読み込む（遅延読み込み）
function Directory({ path, depth, tree }: { path: string; depth: number; tree: TreeState }) {
  const [entries, setEntries] = useState<DirEntry[] | null>(null);
  const { onError, refreshKey, collapse } = tree;

  // 窓に戻ったときなどに読み直す（refreshKey。D-156）。フォルダが外で消えていたら（null）、知らせずに閉じる。
  // 作業領域のルートが消えたときだけは知らせる
  useEffect(() => {
    listDirectory(path)
      .then((listed) => {
        if (listed !== null) {
          setEntries(listed);
        } else if (path === "") {
          setEntries([]);
          onError("作業領域のフォルダが見つかりません");
        } else {
          collapse(path);
        }
      })
      .catch((error) => onError(String(error)));
  }, [path, onError, refreshKey, collapse]);

  if (!entries) {
    return null;
  }

  const creatingHere = tree.editing?.kind === "create" && tree.editing.dir === path;
  const creatingFolder = tree.editing?.kind === "create" && tree.editing.folder;

  async function submitCreate(name: string, folder: boolean) {
    tree.setEditing(null);
    try {
      if (folder) {
        await createNewFolder(path, name);
      } else {
        tree.onCreated(await createNewFile(path, name));
      }
      tree.onRefresh();
    } catch (e) {
      onError(String(e));
    }
  }

  async function submitRename(from: string, name: string, isDir: boolean) {
    tree.setEditing(null);
    try {
      if (isDir) {
        // フォルダ：中のタブを保存してから変え、タブ・ブレークポイント・開いている印を付け替える（D-157）
        if (!(await tree.onBeforeRenameFolder(from))) {
          return;
        }
        const to = await renameExistingFile(from, name);
        tree.renameExpanded(from, to);
        tree.onRenamedFolder(from, to);
      } else {
        if (!(await tree.onBeforeRename(from))) {
          return;
        }
        tree.onRenamed(from, await renameExistingFile(from, name));
      }
      tree.onRefresh();
    } catch (e) {
      onError(String(e));
    }
  }

  // 深さ 1 以上の一覧には、親のフォルダの印の下に縦の線（インデントの境）を引く（D-156）
  const nested = depth > 0;
  const guide = nested ? ({ "--guide-left": `${12 + (depth - 1) * 12 + 5}px` } as CSSProperties) : undefined;
  return (
    <ul className={`file-tree-list ${nested ? "nested" : ""}`} style={guide}>
      {creatingHere && (
        <li>
          <NameInput
            initial=""
            depth={depth}
            onSubmit={(name) => submitCreate(name, creatingFolder)}
            onCancel={() => tree.setEditing(null)}
          />
        </li>
      )}
      {entries.map((entry) => {
        const isActive = entry.path === tree.activePath;
        const isExpanded = tree.expanded.has(entry.path);
        const renaming = tree.editing?.kind === "rename" && tree.editing.path === entry.path;
        return (
          <li key={entry.path}>
            {renaming ? (
              <NameInput
                initial={entry.name}
                depth={depth}
                onSubmit={(name) => submitRename(entry.path, name, entry.is_dir)}
                onCancel={() => tree.setEditing(null)}
              />
            ) : (
              <button
                type="button"
                className={`file-tree-item ${isActive ? "active" : ""}`}
                style={{ paddingLeft: 12 + depth * 12 }}
                title={entry.path}
                onClick={() => {
                  if (entry.is_dir) {
                    tree.toggle(entry.path);
                    tree.select(entry.path);
                  } else {
                    tree.select(parentOf(entry.path));
                    tree.onOpenFile(entry.path);
                  }
                }}
                onContextMenu={(event) => {
                  event.preventDefault();
                  // ファイルにもフォルダにも出す（D-157。D-073 を置き換え）
                  tree.openMenu({
                    path: entry.path,
                    name: entry.name,
                    isDir: entry.is_dir,
                    x: event.clientX,
                    y: event.clientY,
                  });
                }}
              >
                <span className="file-tree-marker">
                  {entry.is_dir ? (isExpanded ? "▾" : "▸") : ""}
                </span>
                <FileIcon name={entry.name} isDir={entry.is_dir} open={isExpanded} />
                <span className="file-tree-name">{entry.name}</span>
                {tree.dirtyPaths.includes(entry.path) && (
                  <span className="file-tree-dirty" aria-label="未保存">
                    ●
                  </span>
                )}
              </button>
            )}
            {entry.is_dir && isExpanded && (
              <Directory path={entry.path} depth={depth + 1} tree={tree} />
            )}
          </li>
        );
      })}
    </ul>
  );
}

// ファイル一覧（原典 §4.3）。作業領域のルートから表示する
function FileTree(props: Props) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  // 新しいファイルを作るフォルダ（最後に押したフォルダ、または開いたファイルのフォルダ）
  const [selectedDir, setSelectedDir] = useState("");
  const [editing, setEditing] = useState<Editing | null>(null);
  const [menu, setMenu] = useState<Menu | null>(null);

  // 名前を変えたフォルダの、開いている印を新しいパスに付け替える（D-157）
  function renameExpanded(from: string, to: string) {
    const moved = (item: string) => (item === from || isUnder(item, from) ? to + item.slice(from.length) : item);
    setExpanded((current) => new Set([...current].map(moved)));
    setSelectedDir((current) => moved(current));
  }

  // 外で消えたフォルダを閉じる。中の開いていたフォルダも閉じる（D-156）
  const collapse = useCallback((path: string) => {
    setExpanded((current) => {
      const next = new Set([...current].filter((item) => item !== path && !item.startsWith(`${path}/`)));
      return next.size === current.size ? current : next;
    });
  }, []);

  function toggle(path: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  // 新しいファイル・フォルダ：作るフォルダを開いてから、先頭に入力欄を出す（フォルダは D-157）
  function startCreate(dir: string, folder: boolean) {
    setMenu(null);
    if (dir !== "") {
      setExpanded((current) => new Set(current).add(dir));
    }
    setSelectedDir(dir);
    setEditing({ kind: "create", dir, folder });
  }

  async function remove(target: Menu) {
    setMenu(null);
    const { path, isDir } = target;
    try {
      // 確認の画面は Rust 側が開く。取り消されたら false が返る（D-072）。
      // フォルダは、中のタブに保存していない変更があることを添える（D-157）
      const unsaved = isDir
        ? props.dirtyPaths.some((dirty) => isUnder(dirty, path))
        : props.dirtyPaths.includes(path);
      const deleted = await deleteFile(path, unsaved);
      if (deleted) {
        if (isDir) {
          collapse(path);
          props.onDeletedFolder(path);
        } else {
          props.onDeleted(path);
        }
        props.onRefresh();
      }
    } catch (e) {
      props.onError(String(e));
    }
  }

  // メニューの外を押したら・Esc で閉じる（Esc は Phase 10 作業 12）
  useEffect(() => {
    if (!menu) {
      return;
    }
    const close = () => setMenu(null);
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") {
        setMenu(null);
      }
    };
    window.addEventListener("click", close);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [menu]);

  // メニューを窓の中に収める（Phase 10 作業 12）。描いたあとに大きさを測って置き直す。測るまでは見せない
  const menuRef = useRef<HTMLDivElement>(null);
  const [menuPos, setMenuPos] = useState<{ left: number; top: number } | null>(null);
  useLayoutEffect(() => {
    const box = menuRef.current;
    if (!menu || !box) {
      setMenuPos(null);
      return;
    }
    const { width, height } = box.getBoundingClientRect();
    setMenuPos({
      left: Math.max(4, Math.min(menu.x, window.innerWidth - width - 4)),
      top: Math.max(4, Math.min(menu.y, window.innerHeight - height - 4)),
    });
  }, [menu]);

  const tree: TreeState = {
    ...props,
    expanded,
    toggle,
    collapse,
    renameExpanded,
    select: setSelectedDir,
    editing,
    setEditing,
    openMenu: setMenu,
  };

  return (
    <>
      {/* 作業領域の名前と同じ列に、操作のボタンを置く（ユーザーの意見、2026-10-16） */}
      <div className="sidebar-title file-tree-header">
        <span className="file-tree-header-name" title={props.workspaceName}>
          {props.workspaceName}
        </span>
        <button
          type="button"
          className="file-tree-action"
          title="コンテストを作る"
          aria-label="コンテストを作る"
          onClick={props.onToggleContest}
        >
          ⤓
        </button>
        <button
          type="button"
          className="file-tree-action"
          title="新しいファイル"
          aria-label="新しいファイル"
          onClick={() => startCreate(selectedDir, false)}
        >
          ＋
        </button>
        <button
          type="button"
          className="file-tree-action"
          title="新しいフォルダ"
          aria-label="新しいフォルダ"
          onClick={() => startCreate(selectedDir, true)}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" aria-hidden="true">
            <path d="M1.5 3.5h4.5l1.5 1.5h7v8.5h-13z" fill="none" stroke="currentColor" strokeWidth="1.2" strokeLinejoin="round" />
            <path d="M8 7.5v4M6 9.5h4" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
          </svg>
        </button>
      </div>
      {props.contestPanel}
      <div className="file-tree">
        <Directory path="" depth={0} tree={tree} />
      </div>
      {menu && (
        <div
          ref={menuRef}
          className="context-menu"
          style={menuPos ? { left: menuPos.left, top: menuPos.top } : { left: menu.x, top: menu.y, visibility: "hidden" }}
          role="menu"
        >
          {menu.isDir && (
            <>
              <button type="button" role="menuitem" onClick={() => startCreate(menu.path, false)}>
                新しいファイル
              </button>
              <button type="button" role="menuitem" onClick={() => startCreate(menu.path, true)}>
                新しいフォルダ
              </button>
            </>
          )}
          <button
            type="button"
            role="menuitem"
            onClick={() => {
              setMenu(null);
              setEditing({ kind: "rename", path: menu.path, name: menu.name, isDir: menu.isDir });
            }}
          >
            名前の変更
          </button>
          <button type="button" role="menuitem" onClick={() => void remove(menu)}>
            削除
          </button>
        </div>
      )}
    </>
  );
}

export default FileTree;
