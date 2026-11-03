// 設定の画面（Phase 8 作業 6b。D-159）。キー・色・表示（入切と文字の大きさ）・提出（Phase 9 作業 2。D-162）・
// Python の処理系（Phase 19 作業 3。D-221）を変え、
// 「保存」で settings.json に書く。「スニペット」（Phase 13 作業 2。D-187）・「テンプレート」（Phase 13 作業 3。D-188）は
// 一覧とボタンだけで、保存とは関係しない。
//
// - 色・表示は、変えるとすぐに画面に出す（試し見）。「キャンセル」・Esc で、開く前の見た目に戻す
// - キーは「変更」を押してから押したキーを割り当てる。ぶつかるキー・修飾キーのないキー・決まったキーは断る
// - 画面を開いている間に settings.json が外で変わったら（App が読み直したら resetSeq が進む）、中身を読み直して知らせる
import { useEffect, useState } from "react";
import { applyAppearance, defaultColor, splitColor } from "../appearance";
import {
  INTERPRETERS,
  LANGUAGES,
  PROBLEM_BUTTONS,
  type Appearance,
  type Interpreter,
  type AtcoderProblemsSettings,
  type StandingsSettings,
  type LoadedKeyBinding,
  type NotifySettings,
  type ProblemSettings,
  type PythonSettings,
  type SettingsUi,
  type SubmitSettings,
  SETTINGS_UIS,
} from "../ipc/state";
import type { LoadedSnippets } from "../ipc/snippets";
import type { Language } from "../ipc/workspace";
import { saveSettings } from "../ipc/state";
import { bindingFromEvent, COMMANDS, isImeKey, setKeyBindings, type Command } from "../keybindings";
import { remeasureEditor } from "../editor/Editor";
import {
  ColorsSection,
  currentAssigned,
  FOLLOW_NOTE,
  FollowUsersField,
  KeysSection,
  MAX_FONT_SIZE,
  MIN_FONT_SIZE,
  refusal,
  SizeField,
  SnippetsSection,
  TemplatesSection,
  type Assigned,
  type Keys,
} from "./settingsParts";

// 言語ごとの、今読めているスニペットと、ファイルを読めなかったときの理由（D-187・D-213）
export type SnippetsViews = Record<Language, { loaded: LoadedSnippets | null; error: string | null }>;

type Section =
  | "keys"
  | "colors"
  | "view"
  | "problem"
  | "submit"
  | "notify"
  | "past"
  | "follow"
  | "python"
  | "snippets"
  | "templates";

type Props = {
  // 今使っている見た目（開いたとき・キャンセルしたときに戻すもの）
  appearance: Appearance;
  // 今使っている提出の設定（D-162）
  submit: SubmitSettings;
  // 今の通知の設定（D-239）
  notify: NotifySettings;
  // 今の AtCoder Problems の設定（AtCoder の ID。D-241）
  atcoderProblems: AtcoderProblemsSettings;
  // 今の順位表の設定（追跡するユーザー。D-253）
  standings: StandingsSettings;
  // 今使っている問題文の欄の設定（D-196・D-197）
  problem: ProblemSettings;
  // 今の Python の設定（D-221）。画面で変えるのは処理系だけ
  python: PythonSettings;
  // App が settings.json を読み直したら進む。進んだら、中身を読み直す
  resetSeq: number;
  onClose: () => void;
  // 保存できた。App は見た目と提出の設定を覚える
  onSaved: (
    appearance: Appearance,
    submit: SubmitSettings,
    notify: NotifySettings,
    atcoderProblems: AtcoderProblemsSettings,
    problem: ProblemSettings,
    interpreter: Interpreter,
    standings: StandingsSettings,
  ) => void;
  // 外で変わっていて保存できなかった。App が読み直す（resetSeq が進む）
  onReload: () => Promise<void>;
  onOpenJson: () => void;
  // 言語ごとの、今読めているスニペットと、ファイルを読めなかったときの理由（D-187・D-213）
  snippets: SnippetsViews;
  onOpenSnippets: (language: Language) => Promise<void>;
  // 取り込んで読み直す。出す知らせを返す（キャンセルなら null）
  onImportSnippets: (language: Language) => Promise<string | null>;
  // 「設定2で開く」（Phase 26。D-246）：設定の画面の形を editor にして保存できたら、App が中央のタブで開き直す
  onSwitchToEditor: () => void;
};

function SettingsDialog({
  appearance,
  submit,
  notify,
  atcoderProblems,
  standings,
  problem,
  python,
  resetSeq,
  onClose,
  onSaved,
  onReload,
  onOpenJson,
  snippets,
  onOpenSnippets,
  onImportSnippets,
  onSwitchToEditor,
}: Props) {
  const [section, setSection] = useState<Section>("keys");
  const [assigned, setAssigned] = useState<Assigned>(currentAssigned);
  const [draft, setDraft] = useState<Appearance>(appearance);
  const [submitDraft, setSubmitDraft] = useState<SubmitSettings>(submit);
  const [notifyDraft, setNotifyDraft] = useState<NotifySettings>(notify);
  const [pastDraft, setPastDraft] = useState<AtcoderProblemsSettings>(atcoderProblems);
  const [followDraft, setFollowDraft] = useState<StandingsSettings>(standings);
  const [problemDraft, setProblemDraft] = useState<ProblemSettings>(problem);
  const [interpreterDraft, setInterpreterDraft] = useState<Interpreter>(python.interpreter);
  const [capturing, setCapturing] = useState<Command | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [firstSeq] = useState(resetSeq);

  // 外で変わって読み直したら、画面の中身も読み直す
  useEffect(() => {
    if (resetSeq === firstSeq) {
      return;
    }
    setAssigned(currentAssigned());
    setDraft(appearance);
    setSubmitDraft(submit);
    setNotifyDraft(notify);
    setPastDraft(atcoderProblems);
    setFollowDraft(standings);
    setProblemDraft(problem);
    setInterpreterDraft(python.interpreter);
    setCapturing(null);
    setNotice("settings.json が外で変わったので、読み直しました。必要なら、もう一度変えてください");
  }, [resetSeq]);

  // 試し見
  useEffect(() => {
    applyAppearance(draft);
    remeasureEditor();
  }, [draft]);

  function cancel() {
    applyAppearance(appearance);
    remeasureEditor();
    onClose();
  }

  // キーの受け取り（「変更」を押したあと）。アプリのキー割り当てより先に受け取り、ほかへ渡さない。
  // 受け取っていないときの Esc は、キャンセル
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (capturing === null) {
        // 日本語入力の変換を取り消す Esc では、設定の画面を閉じない（C-6）
        if (event.key === "Escape" && !isImeKey(event)) {
          event.preventDefault();
          cancel();
        }
        return;
      }
      event.preventDefault();
      event.stopImmediatePropagation();
      if (event.repeat) {
        return;
      }
      if (event.key === "Escape" && !event.metaKey && !event.ctrlKey && !event.altKey && !event.shiftKey) {
        setCapturing(null);
        setNotice(null);
        return;
      }
      const keys = bindingFromEvent(event);
      if (!keys) {
        return;
      }
      const reason = refusal(keys, capturing, assigned);
      if (reason) {
        setNotice(reason);
        return;
      }
      setAssigned((current) => ({ ...current, [capturing]: keys }));
      setCapturing(null);
      setNotice(null);
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, assigned, appearance]);

  function setKeys(command: Command, keys: Keys | null) {
    if (keys) {
      const reason = refusal(keys, command, assigned);
      if (reason) {
        setNotice(reason);
        return;
      }
    }
    setAssigned((current) => ({ ...current, [command]: keys }));
    setNotice(null);
  }

  function setColor(name: string, value: string | null) {
    setDraft((current) => {
      const colors = { ...current.colors };
      // 既定と同じ色は書かない（colors には変えたものだけを書く）
      const base = splitColor(defaultColor(name));
      const next = value === null ? null : splitColor(value);
      if (value === null || (base && next && base.hex === next.hex && base.alpha === next.alpha)) {
        delete colors[name];
      } else {
        colors[name] = value;
      }
      return { ...current, colors };
    });
  }

  function setFontSize(which: "editorFontSize" | "problemFontSize", size: number) {
    if (!Number.isInteger(size)) {
      return;
    }
    const clamped = Math.min(Math.max(size, MIN_FONT_SIZE), MAX_FONT_SIZE);
    setDraft((current) => ({ ...current, [which]: clamped }));
  }

  // next が "editor" なら、設定の画面の形をそれにして保存する（「設定2で開く」）。関数なら、保存して閉じたあとに呼ぶ
  // （「JSON で開く」「ファイルを開く」：IDE のタブで開くので、ダイアログの裏に隠れないように閉じる。D-247）
  async function save(next?: "editor" | (() => void)) {
    const ui = next === "editor" ? next : undefined;
    const appearanceToSave = ui ? { ...draft, settingsUi: ui } : draft;
    const bindings: LoadedKeyBinding[] = COMMANDS.map((command) => {
      const keys = assigned[command];
      return keys
        ? { command, ...keys }
        : { command, key: "", meta: false, ctrl: false, alt: false, shift: false };
    });
    setSaving(true);
    try {
      const outcome = await saveSettings(
        bindings,
        appearanceToSave,
        submitDraft,
        notifyDraft,
        pastDraft,
        problemDraft,
        interpreterDraft,
        followDraft,
      );
      if (outcome === "changedOutside") {
        await onReload();
        return;
      }
      if (outcome === "cancelled") {
        setNotice("確認のダイアログを省く設定にしなかったので、保存しませんでした");
        return;
      }
      setKeyBindings(bindings);
      onSaved(appearanceToSave, submitDraft, notifyDraft, pastDraft, problemDraft, interpreterDraft, followDraft);
      onClose();
      if (ui) {
        onSwitchToEditor();
      } else if (typeof next === "function") {
        next();
      }
    } catch (e) {
      setNotice(String(e));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="settings-backdrop">
      <div className="settings-dialog" role="dialog" aria-modal="true" aria-label="設定">
        <div className="settings-head">
          <div
            className="settings-tabs"
            role="tablist"
            onWheel={(event) => {
              // ホイールを縦に回したら、横に送る（エディタのタブの列と同じ。D-191）
              if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
                event.currentTarget.scrollLeft += event.deltaY;
              }
            }}
          >
            {(
              [
                ["keys", "キー"],
                ["colors", "色"],
                ["view", "表示"],
                ["problem", "問題文"],
                ["submit", "提出"],
                ["notify", "通知"],
                ["past", "過去問"],
                ["follow", "追跡"],
                ["python", "Python"],
                ["snippets", "スニペット"],
                ["templates", "テンプレート"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                type="button"
                role="tab"
                aria-selected={section === id}
                className={`settings-tab ${section === id ? "selected" : ""}`}
                onClick={() => setSection(id)}
              >
                {label}
              </button>
            ))}
          </div>
          <button
            type="button"
            className="settings-link"
            title="今の変更も保存してから、設定2（VS Code 風。中央のタブ）で開き直します（設定の「表示」で戻せます）"
            disabled={saving || capturing !== null}
            onClick={() => void save("editor")}
          >
            設定2で開く
          </button>
          <button
            type="button"
            className="settings-link"
            title="今の変更も保存してから、settings.json を IDE のタブで開きます"
            disabled={saving || capturing !== null}
            onClick={() => void save(onOpenJson)}
          >
            JSON で開く
          </button>
        </div>
        <div className="settings-body">
          {section === "keys" && (
            <KeysSection
              assigned={assigned}
              capturing={capturing}
              onCapture={(command) => {
                setCapturing(command);
                setNotice(null);
              }}
              onSet={setKeys}
            />
          )}
          {section === "colors" && <ColorsSection colors={draft.colors} onSet={setColor} />}
          {section === "problem" && <ProblemSection draft={problemDraft} onChange={setProblemDraft} />}
          {section === "submit" && (
            <SubmitSection
              draft={submitDraft}
              onToggle={(which, on) => setSubmitDraft((current) => ({ ...current, [which]: on }))}
            />
          )}
          {section === "notify" && (
            <NotifySection
              draft={notifyDraft}
              onToggle={(which, on) => setNotifyDraft((current) => ({ ...current, [which]: on }))}
            />
          )}
          {section === "past" && (
            <div className="settings-view">
              <label className="settings-field">
                AtCoder の ID
                <input
                  className="settings-text"
                  value={pastDraft.user}
                  placeholder="空ならログインしているアカウント"
                  maxLength={16}
                  spellCheck={false}
                  onChange={(event) => setPastDraft({ user: event.target.value })}
                />
              </label>
              <p className="settings-note">
                中央の「過去問」のタブ（サイドバーの「コンテスト」の「過去問の一覧」で開きます）で、問題ごとの AC 状況を出すのに使います。空なら、ログインしているアカウントの
                ID を使います（ログインしていなければ AC 状況は出さず、diff だけを出します）。ID は AtCoder Problems（kenkoooo.com。AtCoder
                とは別の人が運営している非公式のサービス）に送り、公開されている提出を読みます。
              </p>
            </div>
          )}
          {section === "follow" && (
            <div className="settings-view">
              <label className="settings-field settings-field-column">
                追跡するユーザー（AtCoder の ID）
                <FollowUsersField follow={followDraft.follow} onChange={(follow) => setFollowDraft({ follow })} />
              </label>
              <p className="settings-note">{FOLLOW_NOTE}</p>
            </div>
          )}
          {section === "snippets" && (
            <div className="settings-view">
              <p className="settings-note">
                アプリのフォルダの言語ごとのファイルに、VS Code と同じ書き方で書きます。その言語のファイル（Rust は .rs、C++ は
                .cpp・.hpp・.h など、Python は .py）で、呼び出す文字を打つと補完に出ます。「ファイルを開く」で IDE のタブに開き、保存するとすぐ使います。「VS
                Code から取り込む」は、VS Code のその言語のスニペット（rust.json・cpp.json・python.json）を写します（今のファイルは退避します）。
              </p>
              {LANGUAGES.map(({ value: language, label }) => (
                <SnippetsSection
                  key={language}
                  language={language}
                  label={label}
                  snippets={snippets[language]}
                  onOpen={() => void save(() => void onOpenSnippets(language))}
                  onImport={async () => {
                    setNotice(null);
                    try {
                      setNotice(await onImportSnippets(language));
                    } catch (e) {
                      setNotice(String(e));
                    }
                  }}
                />
              ))}
            </div>
          )}
          {section === "python" && (
            <PythonSection python={python} interpreter={interpreterDraft} onChange={setInterpreterDraft} />
          )}
          {section === "templates" && <TemplatesSection onError={setNotice} />}
          {section === "view" && (
            <ViewSection
              draft={draft}
              onToggle={(which, on) => setDraft((current) => ({ ...current, [which]: on }))}
              onFontSize={setFontSize}
              onUi={(ui) => setDraft((current) => ({ ...current, settingsUi: ui }))}
            />
          )}
        </div>
        <div className="settings-foot">
          <div className="settings-notice" role="status">
            {notice}
          </div>
          <button type="button" className="settings-button" onClick={cancel} disabled={saving}>
            キャンセル
          </button>
          <button
            type="button"
            className="settings-button primary"
            onClick={() => void save()}
            disabled={saving || capturing !== null}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}

function ViewSection({
  draft,
  onToggle,
  onFontSize,
  onUi,
}: {
  draft: Appearance;
  onToggle: (which: "indentRainbow" | "errorLens" | "breadcrumbs", on: boolean) => void;
  onFontSize: (which: "editorFontSize" | "problemFontSize", size: number) => void;
  onUi: (ui: SettingsUi) => void;
}) {
  const sizes = [
    ["editorFontSize", "エディタの文字の大きさ"],
    ["problemFontSize", "問題文の文字の大きさ"],
  ] as const;
  return (
    <div className="settings-view">
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.indentRainbow}
          onChange={(event) => onToggle("indentRainbow", event.target.checked)}
        />
        インデントの段に色を付ける
      </label>
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.errorLens}
          onChange={(event) => onToggle("errorLens", event.target.checked)}
        />
        診断のある行に色とメッセージを出す
      </label>
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.breadcrumbs}
          onChange={(event) => onToggle("breadcrumbs", event.target.checked)}
        />
        エディタの上にパンくずリスト（ファイルの場所）を出す
      </label>
      {sizes.map(([which, label]) => (
        <SizeField key={which} label={label} size={draft[which]} onChange={(size) => onFontSize(which, size)} />
      ))}
      <label className="settings-check">
        設定の画面
        <select
          className="settings-select"
          value={draft.settingsUi}
          onChange={(event) => onUi(event.target.value as SettingsUi)}
        >
          {SETTINGS_UIS.map(({ value, label }) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <p className="settings-note">保存すると、次に設定を開くときから、その形で開きます。</p>
    </div>
  );
}

// 提出の設定（D-162）。確認のダイアログを省くのは、Rust 側がサンプルテストの合格を確かめたときだけ
function SubmitSection({
  draft,
  onToggle,
}: {
  draft: SubmitSettings;
  onToggle: (which: "testBeforeSubmit" | "confirmDialog", on: boolean) => void;
}) {
  return (
    <div className="settings-view">
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.testBeforeSubmit}
          onChange={(event) => onToggle("testBeforeSubmit", event.target.checked)}
        />
        提出の前にサンプルテストを動かし、すべて AC でなければ提出しない
      </label>
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.confirmDialog}
          onChange={(event) => onToggle("confirmDialog", event.target.checked)}
        />
        提出の確認のダイアログを出す
      </label>
      <p className="settings-note">
        確認のダイアログを出さないようにしても、省くのは、今のソースのサンプルテストがすべて AC で、提出の前の検査の注意がないときだけです。
        サンプルがない問題・目視確認のケースがある問題・注意があるときは、今までどおり確認します。出さないようにして保存するときは、OS
        の確認が出ます。
      </p>
    </div>
  );
}

// 通知（Phase 22 作業 2。D-239）。知らせるのは IDE の窓が手前にないときだけ。初期値はオフ
function NotifySection({
  draft,
  onToggle,
}: {
  draft: NotifySettings;
  onToggle: (which: "judge" | "contestStart", on: boolean) => void;
}) {
  return (
    <div className="settings-view">
      <label className="settings-check">
        <input type="checkbox" checked={draft.judge} onChange={(event) => onToggle("judge", event.target.checked)} />
        IDE から出した提出の判定が決まったときに知らせる
      </label>
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.contestStart}
          onChange={(event) => onToggle("contestStart", event.target.checked)}
        />
        ツールバーに残り時間を出しているコンテストが始まったときに知らせる
      </label>
      <p className="settings-note">
        どちらも、IDE の窓が手前にないときだけ知らせます。オンにして保存すると、テストの通知を 1 回出します。初めてのときは macOS
        が通知を許可するかを聞くので、「許可」を押してください。通知が出ないときは、システム設定の「通知」で「AtCoder
        Studio」を許可してください（IDE からは、許可されているかが分かりません）。集中モードの間は、バナーが出ません。
      </p>
    </div>
  );
}

// Python（Phase 19 作業 3。D-221）：手元で使う処理系。処理系のパスは settings.json で書く
function PythonSection({
  python,
  interpreter,
  onChange,
}: {
  python: PythonSettings;
  interpreter: Interpreter;
  onChange: (next: Interpreter) => void;
}) {
  const path = interpreter === "cpython" ? python.cpython : python.pypy;
  return (
    <div className="settings-view">
      <label className="settings-check">
        処理系
        <select
          className="settings-select"
          value={interpreter}
          onChange={(event) => onChange(event.target.value as Interpreter)}
        >
          {INTERPRETERS.map(({ value, label }) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <p className="settings-note">
        main.py の実行・テストに使います。ジャッジの「Python (CPython 3.13.7)」「Python (PyPy 3.11-v7.3.20)」に合わせてください。
        {path
          ? `処理系のパス：${path}（settings.json で書いたもの）。`
          : "処理系は /opt/homebrew/bin・PATH・/usr/local/bin から探します（決まったパスを使うときは、settings.json の python.cpython・python.pypy に書きます）。"}
      </p>
    </div>
  );
}

// 問題文の欄（Phase 15 作業 3。D-196・D-197）：見出しの行に出すボタンと、main() に for まで書くか
function ProblemSection({
  draft,
  onChange,
}: {
  draft: ProblemSettings;
  onChange: (next: ProblemSettings) => void;
}) {
  function toggleButton(name: string, on: boolean) {
    const buttons = on ? [...draft.buttons, name] : draft.buttons.filter((b) => b !== name);
    // 並びは決まった順に揃えて書く
    onChange({ ...draft, buttons: PROBLEM_BUTTONS.map((b) => b.name).filter((b) => buttons.includes(b)) });
  }
  return (
    <div className="settings-view">
      <p className="settings-note">問題文の欄の見出しの行に出すボタン（出さないものは「…」の一覧に入ります）</p>
      {PROBLEM_BUTTONS.map(({ name, label }) => (
        <label key={name} className="settings-check">
          <input
            type="checkbox"
            checked={draft.buttons.includes(name)}
            onChange={(event) => toggleButton(name, event.target.checked)}
          />
          {label}
        </label>
      ))}
      <p className="settings-note">
        「この問題を作る」は、選択欄で選んだ問題のときだけ出ます。すべてを見出しの行に出すと、「…」は出ません。
      </p>
      <label className="settings-check">
        問題を作るときの言語
        <select
          className="settings-select"
          value={draft.language}
          onChange={(event) => onChange({ ...draft, language: event.target.value as Language })}
        >
          {LANGUAGES.map(({ value, label }) => (
            <option key={value} value={value}>
              {label}
            </option>
          ))}
        </select>
      </label>
      <p className="settings-note">
        「コンテストを作る」「この問題を作る」は、この言語で作ります。
      </p>
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.fillInput}
          onChange={(event) => onChange({ ...draft, fillInput: event.target.checked })}
        />
        問題を作るときに input! を入れておく（C++・Python は入力のコード）
      </label>
      <p className="settings-note">
        「コンテストを作る」「この問題を作る」で main.rs を書くときに、テンプレートの solve() の input!
        を、問題文から作ったものにしておきます（テストケースが複数なら、下の設定に従います）。すでにある main.rs は変えません。
        C++ では main.cpp の、Python では main.py の solve()（なければ main()）に、目印のコメントで囲んで入れます。
      </p>
      <label className="settings-check">
        <input
          type="checkbox"
          checked={draft.mainLoop}
          onChange={(event) => onChange({ ...draft, mainLoop: event.target.checked })}
        />
        テストケースが複数のとき、main() に for も書く
      </label>
      <p className="settings-note">
        「solve() の input! を置き換える」で、main() の input! を t: usize にし（なければ先頭に入れ）、main() の中の solve();
        を for _ in 0..t {"{"} solve(); {"}"} にします。すでに for の中で呼んでいれば、そのままにします。C++ では、main() に T
        を読むコードを入れ、solve(); を for (int i = 0; i &lt; T; i++) solve(); にします。Python では、main() に T
        を読むコードを入れ、solve() を for _ in range(T): の中にします。
      </p>
    </div>
  );
}

export default SettingsDialog;
