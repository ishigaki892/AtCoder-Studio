// 設定の画面の部品（Phase 26。D-246）。設定1（SettingsDialog。窓に重ねる）と設定2（SettingsEditor。VS Code 風）で使い回す。
// キーの割り当ての確かめ、キーの表・色の表（検索で行を絞れる）・スニペット・テンプレート・文字の大きさの欄
import { useEffect, useMemo, useState } from "react";
import { COLOR_GROUPS, defaultColor, joinColor, splitColor } from "../appearance";
import { snippetsFile, type LoadedSnippets } from "../ipc/snippets";
import type { Language } from "../ipc/workspace";
import { openTemplatesFolder, templateFiles, type TemplateFiles } from "../ipc/scaffold";
import {
  COMMAND_LABELS,
  COMMANDS,
  DEFAULT_KEY_BINDINGS,
  getKeyBindings,
  keyLabel,
  type Command,
  type KeyBinding,
} from "../keybindings";

export type Keys = Omit<KeyBinding, "command">;
// コマンドごとの割り当て。null は「なし」
export type Assigned = Record<Command, Keys | null>;

export const MIN_FONT_SIZE = 8;
export const MAX_FONT_SIZE = 32;

// macOS とメニューの決まったキー（割り当てると、そちらが先に動く・使えなくなる）。押しても届かないものもある
const RESERVED = new Set(["⌘Q", "⌘H", "⌥⌘H", "⌘M", "⌘C", "⌘V", "⌘X", "⌘Z", "⇧⌘Z", "⌘A", "⌃⌘F"]);

// 今の割り当てを、コマンドごとの最初の 1 つにする（settings.json に同じコマンドが 2 つ以上あれば、先のもの）
export function currentAssigned(): Assigned {
  const assigned = {} as Assigned;
  for (const command of COMMANDS) {
    const binding = getKeyBindings().find((item) => item.command === command);
    assigned[command] = binding && binding.key !== "" ? toKeys(binding) : null;
  }
  return assigned;
}

function toKeys(binding: KeyBinding): Keys {
  return { key: binding.key, meta: binding.meta, ctrl: binding.ctrl, alt: binding.alt, shift: binding.shift };
}

export function defaultKeys(command: Command): Keys | null {
  const binding = DEFAULT_KEY_BINDINGS.find((item) => item.command === command);
  return binding ? toKeys(binding) : null;
}

export function sameKeys(a: Keys | null, b: Keys | null): boolean {
  return a === null || b === null ? a === b : keyLabel(a) === keyLabel(b);
}

// 割り当てられない理由。割り当てられるなら null
export function refusal(keys: Keys, command: Command, assigned: Assigned): string | null {
  const label = keyLabel(keys);
  if (RESERVED.has(label)) {
    return `${label} は macOS やメニューで使うキーなので、割り当てられません`;
  }
  const functionKey = /^f([1-9]|1[0-9]|20)$/.test(keys.key);
  if (!keys.meta && !keys.ctrl && !keys.alt && !functionKey) {
    return "⌘・⌃・⌥ のどれかと一緒に押してください（F1〜F20 は、そのままでも使えます）";
  }
  for (const other of COMMANDS) {
    if (other !== command && sameKeys(assigned[other], keys)) {
      return `${label} は「${COMMAND_LABELS[other]}」に割り当てられています`;
    }
  }
  return null;
}

export function KeysSection({
  assigned,
  capturing,
  onCapture,
  onSet,
  shown,
}: {
  assigned: Assigned;
  capturing: Command | null;
  onCapture: (command: Command | null) => void;
  onSet: (command: Command, keys: Keys | null) => void;
  // 出すコマンド（設定の検索。VS Code 風の画面。D-246）。なければすべて
  shown?: (command: Command) => boolean;
}) {
  return (
    <table className="settings-table">
      <tbody>
        {COMMANDS.filter((command) => !shown || shown(command)).map((command) => {
          const keys = assigned[command];
          const fallback = defaultKeys(command);
          return (
            <tr key={command}>
              <td className="settings-name">{COMMAND_LABELS[command]}</td>
              <td className="settings-key">
                {capturing === command ? (
                  <span className="settings-capturing">キーを押してください（Esc でやめる）</span>
                ) : keys ? (
                  <kbd>{keyLabel(keys)}</kbd>
                ) : (
                  <span className="settings-none">なし</span>
                )}
              </td>
              <td className="settings-actions">
                <button
                  type="button"
                  className="settings-link"
                  onClick={() => onCapture(capturing === command ? null : command)}
                >
                  {capturing === command ? "やめる" : "変更"}
                </button>
                <button
                  type="button"
                  className="settings-link"
                  disabled={keys === null || capturing !== null}
                  onClick={() => onSet(command, null)}
                >
                  なし
                </button>
                <button
                  type="button"
                  className="settings-link"
                  disabled={sameKeys(keys, fallback) || capturing !== null}
                  onClick={() => onSet(command, fallback)}
                >
                  既定に戻す
                </button>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

export function ColorsSection({
  colors,
  onSet,
  shown,
  groupIds,
}: {
  colors: Record<string, string>;
  onSet: (name: string, value: string | null) => void;
  // 出す色（設定の検索。D-246）。なければすべて。行のない分類は出さない
  shown?: (group: string, item: { name: string; label: string }) => boolean;
  // 分類の見出しに付ける印（data-nav。VS Code 風の画面の左の木から移るため）
  groupIds?: (group: string) => string;
}) {
  // 既定の色は、開いている間は変わらないので、一度だけ読む
  const defaults = useMemo(
    () =>
      Object.fromEntries(
        COLOR_GROUPS.flatMap((group) => group.items.map((item) => [item.name, defaultColor(item.name)])),
      ),
    [],
  );
  return (
    <div>
      {COLOR_GROUPS.map((group) => ({ group, items: group.items.filter((item) => !shown || shown(group.title, item)) }))
        .filter(({ items }) => items.length > 0)
        .map(({ group, items }) => (
        <section key={group.title} className="settings-group" data-nav={groupIds?.(group.title)}>
          <h3>{group.title}</h3>
          <table className="settings-table">
            <tbody>
              {items.map((item) => {
                // settings.json に手で書いた読めない色は、既定の色として見せる
                const written = colors[item.name];
                const value = written !== undefined && splitColor(written) ? written : defaults[item.name];
                const parts = splitColor(value) ?? { hex: "#000000", alpha: 100 };
                return (
                  <tr key={item.name}>
                    <td className="settings-name" title={item.name}>
                      {item.label}
                    </td>
                    <td className="settings-color">
                      <span
                        className="settings-swatch"
                        style={{ backgroundImage: `linear-gradient(${value}, ${value})` }}
                      />
                      <input
                        type="color"
                        aria-label={`${item.label}の色`}
                        value={parts.hex}
                        onChange={(event) => onSet(item.name, joinColor(event.target.value, parts.alpha))}
                      />
                      <input
                        type="range"
                        min={0}
                        max={100}
                        aria-label={`${item.label}の濃さ`}
                        value={parts.alpha}
                        onChange={(event) => onSet(item.name, joinColor(parts.hex, Number(event.target.value)))}
                      />
                      <span className="settings-alpha">{parts.alpha}%</span>
                    </td>
                    <td className="settings-actions">
                      <button
                        type="button"
                        className="settings-link"
                        disabled={colors[item.name] === undefined}
                        onClick={() => onSet(item.name, null)}
                      >
                        既定に戻す
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </section>
      ))}
    </div>
  );
}

// スニペット（Phase 13 作業 2。D-187）：言語ごとの、今読めているものの一覧と、ファイルを開く・VS Code から取り込む（D-213）
export function SnippetsSection({
  language,
  label,
  snippets,
  onOpen,
  onImport,
}: {
  language: Language;
  label: string;
  snippets: { loaded: LoadedSnippets | null; error: string | null };
  onOpen: () => void;
  onImport: () => Promise<void>;
}) {
  const [importing, setImporting] = useState(false);
  const list = [...(snippets.loaded?.snippets ?? [])].sort((a, b) =>
    a.prefixes[0].localeCompare(b.prefixes[0], "ja"),
  );
  const problems = snippets.loaded?.problems ?? [];
  return (
    <div className="settings-snippets">
      <div className="settings-actions-row">
        <span className="settings-name">
          {label}（{snippetsFile(language)}）：{list.length} 件
        </span>
        <button type="button" className="settings-link" onClick={onOpen}>
          ファイルを開く
        </button>
        <button
          type="button"
          className="settings-link"
          disabled={importing}
          onClick={() => {
            setImporting(true);
            void onImport().finally(() => setImporting(false));
          }}
        >
          VS Code から取り込む
        </button>
      </div>
      {snippets.error && <p className="settings-warning">{snippets.error}</p>}
      {problems.length > 0 && <p className="settings-warning">読めない項目：{problems.join("、")}</p>}
      {list.length > 0 && (
        <table className="settings-table">
          <tbody>
            {list.map((item) => (
              <tr key={item.name}>
                <td className="settings-snippet-prefix">{item.prefixes.join("、")}</td>
                <td className="settings-name">{item.description ?? item.name}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

// テンプレート（Phase 13 作業 3。D-188）：雛形のファイルの説明とあるかどうか、フォルダを開く。
// Finder から戻ったときに、あるかどうかを確かめ直す
export function TemplatesSection({ onError }: { onError: (message: string) => void }) {
  const [files, setFiles] = useState<TemplateFiles | null>(null);
  useEffect(() => {
    const refresh = () => {
      templateFiles()
        .then(setFiles)
        .catch((e) => onError(String(e)));
    };
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, []);
  const state = (present: boolean | undefined) =>
    present === undefined ? "" : present ? "あり" : "なし（IDE の最小の雛形を使います）";
  return (
    <div className="settings-view">
      <div className="settings-actions-row">
        <span className="settings-name">templates/</span>
        <button
          type="button"
          className="settings-link"
          onClick={() => openTemplatesFolder().catch((e) => onError(String(e)))}
        >
          テンプレートのフォルダを開く
        </button>
      </div>
      <p className="settings-note">
        「コンテストを作る」「この問題を作る」で作る問題の中身になります。アプリのフォルダの templates/ に次のファイルを置きます（フォルダがなければ、開くときに作ります）。
      </p>
      <table className="settings-table">
        <tbody>
          <tr>
            <td className="settings-snippet-prefix">main.rs</td>
            <td className="settings-name">src/main.rs の中身</td>
            <td className="settings-name">{state(files?.mainRs)}</td>
          </tr>
          <tr>
            <td className="settings-snippet-prefix">dependencies.toml</td>
            <td className="settings-name">Cargo.toml の [dependencies] の中身</td>
            <td className="settings-name">{state(files?.dependencies)}</td>
          </tr>
          <tr>
            <td className="settings-snippet-prefix">main.cpp</td>
            <td className="settings-name">C++ の main.cpp の中身</td>
            <td className="settings-name">{state(files?.mainCpp)}</td>
          </tr>
          <tr>
            <td className="settings-snippet-prefix">main.py</td>
            <td className="settings-name">Python の main.py の中身</td>
            <td className="settings-name">{state(files?.mainPy)}</td>
          </tr>
        </tbody>
      </table>
    </div>
  );
}

// 文字の大きさの欄。打っている途中の数（範囲の外・空）は、当てずに欄にだけ残す。欄を離れたら、今の大きさに戻す
export function SizeField({ label, size, onChange }: { label: string; size: number; onChange: (size: number) => void }) {
  const [text, setText] = useState(String(size));
  useEffect(() => setText(String(size)), [size]);
  return (
    <div className="settings-size">
      <span className="settings-name">{label}</span>
      <button
        type="button"
        className="settings-step"
        aria-label={`${label}を小さく`}
        disabled={size <= MIN_FONT_SIZE}
        onClick={() => onChange(size - 1)}
      >
        −
      </button>
      <input
        type="number"
        min={MIN_FONT_SIZE}
        max={MAX_FONT_SIZE}
        step={1}
        aria-label={label}
        value={text}
        onChange={(event) => {
          setText(event.target.value);
          const next = Number(event.target.value);
          if (Number.isInteger(next) && next >= MIN_FONT_SIZE && next <= MAX_FONT_SIZE) {
            onChange(next);
          }
        }}
        onBlur={() => setText(String(size))}
      />
      <button
        type="button"
        className="settings-step"
        aria-label={`${label}を大きく`}
        disabled={size >= MAX_FONT_SIZE}
        onClick={() => onChange(size + 1)}
      >
        ＋
      </button>
      <span className="settings-unit">
        px（{MIN_FONT_SIZE}〜{MAX_FONT_SIZE}）
      </span>
    </div>
  );
}

