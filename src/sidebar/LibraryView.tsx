// サイドバーの「ライブラリ」（Phase 46。D-271）。アプリのフォルダの libraries/ の自作のライブラリ（C++ は AC Library も）を、
// 言語ごとに一覧にする。上に言語の切り替えと検索の欄。行の「入れる」で、開いているファイルに #include・import・use の行を入れ、
// 「開く」でタブで開く。▸ で中の名前を出し、名前を押すとその行を開く（Rust の名前は、その名前だけの use も入れられる）
import { useEffect, useMemo, useState } from "react";
import { listLibraries, type Libraries, type Library, type LibraryItem } from "../ipc/library";
import type { Language } from "../ipc/workspace";
import { openLibraryFolder, openPythonLibraryFolder, openRustLibraryFolder } from "../ipc/scaffold";

type Props = {
  // サイドバーで「ライブラリ」を出しているか（出したときに読み直す）
  visible: boolean;
  // 開いているファイルの言語（変わったら、その言語に切り替える）
  activeLanguage: Language | null;
  // 入れた・入れなかった結果の知らせを返す（サイドバーに出す）
  onInsert: (library: Library, item: LibraryItem | null) => Promise<string>;
  // 開けなかった理由（開けたら null）
  onOpen: (library: Library, item: LibraryItem | null) => Promise<string | null>;
};

const LANGUAGES: { language: Language; label: string; folder: string }[] = [
  { language: "cpp", label: "C++", folder: "libraries/cpp/" },
  { language: "rust", label: "Rust", folder: "libraries/rust/" },
  { language: "python", label: "Python", folder: "libraries/python/" },
];

function matches(library: Library, query: string): boolean {
  if (query === "") {
    return true;
  }
  const words = [library.name, library.description ?? "", ...library.items.map((item) => item.name)];
  return words.some((word) => word.toLowerCase().includes(query));
}

function openFolder(language: Language) {
  return language === "cpp" ? openLibraryFolder() : language === "rust" ? openRustLibraryFolder() : openPythonLibraryFolder();
}

function LibraryView({ visible, activeLanguage, onInsert, onOpen }: Props) {
  const [data, setData] = useState<Libraries | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [language, setLanguage] = useState<Language>(activeLanguage ?? "cpp");
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [notice, setNotice] = useState<string | null>(null);

  async function insert(library: Library, item: LibraryItem | null) {
    try {
      setNotice(await onInsert(library, item));
    } catch (e) {
      setNotice(String(e));
    }
  }

  async function open(library: Library, item: LibraryItem | null) {
    try {
      setNotice(await onOpen(library, item));
    } catch (e) {
      setNotice(String(e));
    }
  }

  async function load() {
    try {
      setData(await listLibraries());
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    if (visible) {
      void load();
    }
  }, [visible]);

  useEffect(() => {
    if (activeLanguage) {
      setLanguage(activeLanguage);
    }
  }, [activeLanguage]);

  const q = query.trim().toLowerCase();
  const shown = useMemo(
    () => (data?.libraries ?? []).filter((library) => library.language === language && matches(library, q)),
    [data, language, q],
  );
  const own = shown.filter((library) => library.group === "own");
  const ac = shown.filter((library) => library.group === "acLibrary");
  const total = (data?.libraries ?? []).filter((library) => library.language === language).length;
  const folder = LANGUAGES.find((item) => item.language === language)?.folder ?? "";

  function toggle(key: string) {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  }

  function row(library: Library) {
    const key = `${library.group}:${library.path}`;
    const unfolded = expanded.has(key) || (q !== "" && library.items.some((item) => item.name.toLowerCase().includes(q)));
    const items = q === "" || matches({ ...library, items: [] }, q)
      ? library.items
      : library.items.filter((item) => item.name.toLowerCase().includes(q));
    return (
      <li key={key} className="library-row">
        <div className="library-line">
          <button
            type="button"
            className="library-toggle"
            aria-label={unfolded ? "中の名前を隠す" : "中の名前を出す"}
            aria-expanded={unfolded}
            disabled={library.items.length === 0}
            onClick={() => toggle(key)}
          >
            {library.items.length === 0 ? "" : unfolded ? "▾" : "▸"}
          </button>
          <span className="library-name" title={library.head || library.path}>
            {library.name}
          </span>
          <button type="button" className="library-action" title={library.insert} onClick={() => void insert(library, null)}>
            入れる
          </button>
          <button
            type="button"
            className="library-action"
            title={library.uri ? "読み取り専用のタブで開きます" : library.path}
            onClick={() => void open(library, null)}
          >
            開く
          </button>
        </div>
        {library.description && <div className="library-description">{library.description}</div>}
        {library.skipped && <div className="library-description">大きすぎるファイルがあるので、中の名前を読んでいません</div>}
        {unfolded && items.length > 0 && (
          <ul className="library-items">
            {items.map((item) => (
              <li key={`${item.path}:${item.line}`}>
                <button
                  type="button"
                  className="library-item"
                  title={`${item.path}:${item.line}`}
                  onClick={() => void open(library, item)}
                >
                  <span className="library-kind">{item.kind}</span> {item.name}
                </button>
                {item.insert && (
                  <button
                    type="button"
                    className="library-action"
                    title={item.insert}
                    onClick={() => void insert(library, item)}
                  >
                    入れる
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
      </li>
    );
  }

  return (
    <div className="library-view">
      <div className="sidebar-title">ライブラリ</div>
      <div className="library-controls">
        <div className="library-languages" role="group" aria-label="言語">
          {LANGUAGES.map((item) => (
            <button
              key={item.language}
              type="button"
              className={`search-option ${item.language === language ? "on" : ""}`}
              aria-pressed={item.language === language}
              onClick={() => setLanguage(item.language)}
            >
              {item.label}
            </button>
          ))}
          <button type="button" className="library-refresh" title="読み直す" onClick={() => void load()}>
            更新
          </button>
        </div>
        <div className="search-form">
          <input
            className="search-input"
            type="search"
            placeholder="名前・説明・中の名前で絞り込む"
            aria-label="ライブラリを絞り込む"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
      </div>
      {error && (
        <p className="contest-error" role="alert">
          {error}
        </p>
      )}
      {notice && (
        <p className="library-notice" role="status">
          {notice}
        </p>
      )}
      {data?.truncated && <p className="contest-note">ファイルが多いので、一部だけを出しています（1000 ファイルまで）</p>}
      <div className="library-list">
        {data && total === 0 && (
          <p className="contest-note">
            {folder} にライブラリがありません（アプリのフォルダの中）。
            <button type="button" className="library-link" onClick={() => void openFolder(language).catch((e) => setError(String(e)))}>
              フォルダを開く
            </button>
          </p>
        )}
        {data && total > 0 && shown.length === 0 && <p className="contest-note">合うものはありません</p>}
        {own.length > 0 && (
          <>
            {language === "cpp" && <div className="library-heading">自作（{folder}）</div>}
            <ul>{own.map(row)}</ul>
          </>
        )}
        {ac.length > 0 && (
          <>
            <div className="library-heading">AC Library</div>
            <ul>{ac.map(row)}</ul>
          </>
        )}
      </div>
    </div>
  );
}

export default LibraryView;
