// サイドバーの「検索」（Phase 12 作業 2。D-183）。作業領域のファイルを 1 行ずつ探す（Rust 側の search_workspace）。
// 入力を止めて 300 ms で探す（Enter ならすぐ）。結果はファイルごとにまとめ、押すとその場所へ移る。
// 置換（Phase 12 作業 3。D-184）：「▸」で置換の欄を開き、すべて・ファイルごとに置き換える。直後の置換は元に戻せる
import { useEffect, useRef, useState } from "react";
import {
  replaceInWorkspace,
  searchWorkspace,
  undoReplace,
  type FileMatches,
  type SearchQuery,
  type SearchResult,
} from "../ipc/search";
import FileIcon from "./FileIcon";
import { isImeKey } from "../keybindings";

// 入力を止めてから探すまで
const DEBOUNCE_MS = 300;

type Props = {
  // 開いている作業領域の名前。なければ探せない
  workspaceName: string | null;
  // ⇧⌘F・アクティビティバーで出したとき（seq が増える）。text があれば、それを探す文字にする（エディタで選んでいた文字）
  request: { seq: number; text: string | null };
  // 探す前に呼ぶ（未保存のタブを保存する。検索はディスクの中身を見る）
  onBeforeSearch: () => Promise<void>;
  onOpenMatch: (path: string, line: number, column: number) => void;
  // 置換・元に戻すで書き換えたファイル（開いているタブを読み直す）
  onReplaced: (paths: string[]) => Promise<void>;
};

// 置換の結果の知らせに出すファイルの数
const LISTED_FILES = 5;

function listFiles(paths: string[]): string {
  const shown = paths.slice(0, LISTED_FILES).join("、");
  return paths.length > LISTED_FILES ? `${shown} ほか ${paths.length - LISTED_FILES} 個` : shown;
}

function SearchView({ workspaceName, request, onBeforeSearch, onOpenMatch, onReplaced }: Props) {
  const [pattern, setPattern] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [wholeWord, setWholeWord] = useState(false);
  const [regex, setRegex] = useState(false);
  const [result, setResult] = useState<SearchResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [searching, setSearching] = useState(false);
  // 閉じたファイルのまとまり
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  // 置換の欄を開いている・置換の文・置換と元に戻すの知らせ・直前の置換を元に戻せる・置換か元に戻すの途中
  const [showReplace, setShowReplace] = useState(false);
  const [replacement, setReplacement] = useState("");
  const [notice, setNotice] = useState<string | null>(null);
  const [canUndo, setCanUndo] = useState(false);
  const [busy, setBusy] = useState(false);
  // 今の結果を出した条件（置換は、この条件で Rust 側が探し直す）
  const resultQuery = useRef<SearchQuery | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  // 最後に始めた検索の番号（古い検索の結果を使わない）
  const searchSeq = useRef(0);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 出すように頼まれたら、入力欄に移る。選んでいた文字があれば、それを探す
  useEffect(() => {
    if (request.seq === 0) {
      return;
    }
    if (request.text !== null) {
      setPattern(request.text);
    }
    inputRef.current?.focus();
    inputRef.current?.select();
  }, [request.seq]);

  async function run() {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    const seq = ++searchSeq.current;
    if (pattern === "" || !workspaceName) {
      setResult(null);
      setError(null);
      setSearching(false);
      return;
    }
    setSearching(true);
    try {
      await onBeforeSearch();
      const query = { pattern, caseSensitive, wholeWord, regex };
      const found = await searchWorkspace(query);
      if (seq !== searchSeq.current || found.cancelled) {
        return;
      }
      resultQuery.current = query;
      setResult(found);
      setError(null);
      setCollapsed(new Set());
    } catch (e) {
      if (seq === searchSeq.current) {
        setResult(null);
        setError(String(e));
      }
    } finally {
      if (seq === searchSeq.current) {
        setSearching(false);
      }
    }
  }

  // 条件が変わったら、少し待ってから探す（前の置換の知らせは消す）
  useEffect(() => {
    setNotice(null);
    if (timer.current !== null) {
      clearTimeout(timer.current);
    }
    timer.current = setTimeout(() => void run(), DEBOUNCE_MS);
    return () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
      }
    };
  }, [pattern, caseSensitive, wholeWord, regex, workspaceName]);

  // 置き換える（files は今の結果のうち、置き換えるファイル）。確認のダイアログは Rust 側が出す
  async function replace(files: FileMatches[]) {
    const query = resultQuery.current;
    if (busy || !query || files.length === 0) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      // 未保存のタブを保存してから（保存したファイルは、検索のあとで変わったものとして置き換えない）
      await onBeforeSearch();
      const done = await replaceInWorkspace(
        query,
        replacement,
        files.map((file) => ({ path: file.path, stamp: file.stamp })),
      );
      if (done.cancelled) {
        return;
      }
      const parts = [
        done.replaced > 0
          ? `${done.files.length} 個のファイルの ${done.replaced} 件を置き換えました`
          : "置き換えたものはありません",
      ];
      if (done.changed.length > 0) {
        parts.push(
          `検索のあとで変わったので置き換えなかったファイル：${listFiles(done.changed)}（もう一度探してから置換してください）`,
        );
      }
      if (done.failed.length > 0) {
        parts.push(`置き換えられなかったファイル：${listFiles(done.failed)}`);
      }
      setCanUndo(done.files.length > 0);
      await onReplaced(done.files);
      await run();
      setNotice(parts.join("。"));
    } catch (e) {
      setNotice(String(e));
    } finally {
      setBusy(false);
    }
  }

  // 直前の置換を元に戻す
  async function undo() {
    if (busy) {
      return;
    }
    setBusy(true);
    setNotice(null);
    try {
      const done = await undoReplace();
      const parts = [`${done.restored.length} 個のファイルを元に戻しました`];
      if (done.changed.length > 0) {
        parts.push(`置換のあとで変わったので戻さなかったファイル：${listFiles(done.changed)}`);
      }
      if (done.failed.length > 0) {
        parts.push(`戻せなかったファイル：${listFiles(done.failed)}`);
      }
      await onReplaced(done.restored);
      await run();
      setNotice(parts.join("。"));
    } catch (e) {
      setNotice(String(e));
    } finally {
      setCanUndo(false);
      setBusy(false);
    }
  }

  function toggleFile(path: string) {
    setCollapsed((current) => {
      const next = new Set(current);
      if (next.has(path)) {
        next.delete(path);
      } else {
        next.add(path);
      }
      return next;
    });
  }

  const options = [
    { label: "Aa", title: "大文字と小文字を区別する", on: caseSensitive, set: setCaseSensitive },
    { label: "ab", title: "単語単位で探す", on: wholeWord, set: setWholeWord },
    { label: ".*", title: "正規表現で探す", on: regex, set: setRegex },
  ];

  let summary: string | null = null;
  if (!workspaceName) {
    summary = "フォルダを開くと、検索できます";
  } else if (searching) {
    summary = "探しています…";
  } else if (result) {
    const parts = [
      result.total === 0 ? "見つかりませんでした" : `${result.total} 件（${result.files.length} ファイル）`,
    ];
    if (result.truncated) {
      parts.push(`${result.total.toLocaleString()} 件で止めました（条件を絞ってください）`);
    }
    if (result.skipped > 0) {
      parts.push(`読まなかったファイル：${result.skipped}（1 MB より大きい・文字のファイルでないなど）`);
    }
    summary = parts.join("。");
  }

  return (
    <div className="search-view">
      <div className="sidebar-title">検索</div>
      <div className="search-row">
      <button
        type="button"
        className="search-replace-toggle"
        title={showReplace ? "置換の欄を閉じる" : "置換の欄を開く"}
        aria-label={showReplace ? "置換の欄を閉じる" : "置換の欄を開く"}
        aria-expanded={showReplace}
        onClick={() => setShowReplace(!showReplace)}
      >
        {showReplace ? "▾" : "▸"}
      </button>
      <div className="search-fields">
      <div className="search-form">
        <input
          ref={inputRef}
          className="search-input"
          value={pattern}
          placeholder="検索"
          aria-label="検索する文字"
          spellCheck={false}
          onChange={(event) => setPattern(event.target.value)}
          onKeyDown={(event) => {
            // 日本語入力の変換を確定する Enter では探さない（C-6）
            if (event.key === "Enter" && !isImeKey(event.nativeEvent)) {
              event.preventDefault();
              void run();
            }
          }}
        />
        <div className="search-options">
          {options.map((option) => (
            <button
              key={option.label}
              type="button"
              className={`search-option ${option.on ? "on" : ""}`}
              title={option.title}
              aria-label={option.title}
              aria-pressed={option.on}
              onClick={() => option.set(!option.on)}
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>
      {showReplace && (
        <div className="search-replace-row">
          <div className="search-form">
            <input
              className="search-input"
              value={replacement}
              placeholder={regex ? "置換（$1 で当たりの組）" : "置換"}
              aria-label="置き換える文字"
              spellCheck={false}
              onChange={(event) => setReplacement(event.target.value)}
            />
          </div>
          <button
            type="button"
            className="search-replace-all"
            title="すべて置換"
            disabled={busy || searching || !result || result.total === 0}
            onClick={() => void replace(result?.files ?? [])}
          >
            すべて置換
          </button>
        </div>
      )}
      </div>
      </div>
      {(notice || canUndo) && (
        <div className="search-summary search-notice">
          {notice}
          {canUndo && (
            <button type="button" className="search-undo" disabled={busy} onClick={() => void undo()}>
              置換を元に戻す
            </button>
          )}
        </div>
      )}
      {error && (
        <div className="search-summary search-error" role="alert">
          {error}
        </div>
      )}
      {!error && summary && <div className="search-summary">{summary}</div>}
      {result && !error && (
        <div className="search-results">
          {result.files.map((file) => {
            const slash = file.path.lastIndexOf("/");
            const name = file.path.slice(slash + 1);
            const dir = slash >= 0 ? file.path.slice(0, slash) : "";
            const open = !collapsed.has(file.path);
            return (
              <div key={file.path} className="search-file">
                <div className="search-file-row">
                  <button
                    type="button"
                    className="search-file-header"
                    title={file.path}
                    aria-expanded={open}
                    onClick={() => toggleFile(file.path)}
                  >
                    <span className="search-twisty" aria-hidden="true">
                      {open ? "▾" : "▸"}
                    </span>
                    <FileIcon name={name} isDir={false} open={false} />
                    <span className="search-file-name">{name}</span>
                    <span className="search-file-dir">{dir}</span>
                    <span className="search-count">{file.matches.length}</span>
                  </button>
                  {showReplace && (
                    <button
                      type="button"
                      className="search-file-replace"
                      title={`${file.path} を置換`}
                      disabled={busy || searching}
                      onClick={() => void replace([file])}
                    >
                      置換
                    </button>
                  )}
                </div>
                {open &&
                  file.matches.map((match) => (
                    <button
                      key={`${match.line}:${match.column}`}
                      type="button"
                      className="search-match"
                      title={`${file.path}:${match.line}:${match.column}`}
                      onClick={() => onOpenMatch(file.path, match.line, match.column)}
                    >
                      {match.preview.slice(0, match.start)}
                      <mark className="search-hit">{match.preview.slice(match.start, match.end)}</mark>
                      {match.preview.slice(match.end)}
                    </button>
                  ))}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

export default SearchView;
