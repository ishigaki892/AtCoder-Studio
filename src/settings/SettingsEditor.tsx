// VS Code 風の設定の画面（Phase 26。D-246）。中央の「設定」のタブに出す。
// 上に「設定の検索」、左に分類の木、右に分類の見出しと項目（太字の名前・settings.json の名前・説明・部品）を縦に並べる。
// 変えたらすぐ画面に出し（色・表示の試し見）、変えるのが止まってから 0.5 秒後に settings.json に書く（順番に 1 つずつ）。
// 提出の確認を省く設定の OS の確認（D-162）でキャンセルしたら、元に戻す。外で settings.json が変わったら読み直す（D-159）
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { applyAppearance, COLOR_GROUPS, defaultColor, splitColor } from "../appearance";
import {
  INTERPRETERS,
  LANGUAGES,
  PROBLEM_BUTTONS,
  SETTINGS_UIS,
  saveSettings,
  type Appearance,
  type AtcoderProblemsSettings,
  type Interpreter,
  type LoadedKeyBinding,
  type NotifySettings,
  type ProblemSettings,
  type PythonSettings,
  type SettingsUi,
  type SubmitSettings,
} from "../ipc/state";
import { snippetsFile } from "../ipc/snippets";
import type { Language } from "../ipc/workspace";
import { bindingFromEvent, COMMAND_LABELS, COMMANDS, keyLabel, setKeyBindings, type Command } from "../keybindings";
import { remeasureEditor } from "../editor/Editor";
import type { SnippetsViews } from "./SettingsDialog";
import {
  ColorsSection,
  currentAssigned,
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

// 変えるのが止まってから保存するまで
const SAVE_DELAY_MS = 500;

type Props = {
  appearance: Appearance;
  submit: SubmitSettings;
  notify: NotifySettings;
  atcoderProblems: AtcoderProblemsSettings;
  problem: ProblemSettings;
  python: PythonSettings;
  // App が settings.json を読み直したら進む。進んだら、中身を読み直す
  resetSeq: number;
  // 中央のほかのタブが手前で、隠れている（C-8）
  hidden: boolean;
  onSaved: (
    appearance: Appearance,
    submit: SubmitSettings,
    notify: NotifySettings,
    atcoderProblems: AtcoderProblemsSettings,
    problem: ProblemSettings,
    interpreter: Interpreter,
  ) => void;
  // 外で変わっていて保存できなかった。App が読み直す（resetSeq が進む）
  onReload: () => Promise<void>;
  onOpenJson: () => void;
  snippets: SnippetsViews;
  onOpenSnippets: (language: Language) => Promise<void>;
  onImportSnippets: (language: Language) => Promise<string | null>;
  // 「設定1で開く」：設定の画面の形を dialog にして保存できたら、App がこのタブを閉じてダイアログで開き直す
  onSwitchToDialog: () => void;
};

// 左の木の 1 つ。children の id は右の見出しの id
type NavItem = { id: string; label: string; children?: { id: string; label: string }[] };

// 右の 1 項目
function Item({
  id,
  title,
  jsonKey,
  description,
  children,
}: {
  id?: string;
  title: string;
  jsonKey?: string;
  description?: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className="setting-item" id={id}>
      <div className="setting-title">
        {title}
        {jsonKey && <span className="setting-json">{jsonKey}</span>}
      </div>
      {description && <div className="setting-desc">{description}</div>}
      {children}
    </div>
  );
}

// チェックの項目（VS Code と同じく、説明の左にチェック）
function CheckItem({
  title,
  jsonKey,
  label,
  checked,
  onChange,
  note,
}: {
  title: string;
  jsonKey: string;
  label: string;
  checked: boolean;
  onChange: (on: boolean) => void;
  note?: ReactNode;
}) {
  return (
    <Item title={title} jsonKey={jsonKey}>
      <label className="setting-check">
        <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
        <span>{label}</span>
      </label>
      {note && <div className="setting-desc">{note}</div>}
    </Item>
  );
}

function SettingsEditor(props: Props) {
  const { appearance, submit, notify, atcoderProblems, problem, python, resetSeq } = props;
  const [assigned, setAssigned] = useState<Assigned>(currentAssigned);
  const [draft, setDraft] = useState<Appearance>(appearance);
  const [submitDraft, setSubmitDraft] = useState<SubmitSettings>(submit);
  const [notifyDraft, setNotifyDraft] = useState<NotifySettings>(notify);
  const [pastDraft, setPastDraft] = useState<AtcoderProblemsSettings>(atcoderProblems);
  const [problemDraft, setProblemDraft] = useState<ProblemSettings>(problem);
  const [interpreterDraft, setInterpreterDraft] = useState<Interpreter>(python.interpreter);
  const [capturing, setCapturing] = useState<Command | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [active, setActive] = useState<string>("view");
  const [firstSeq] = useState(resetSeq);
  const listRef = useRef<HTMLDivElement>(null);

  // 保存に使う最新の中身（タイマーの中から読む）
  const latest = useRef({ assigned, draft, submitDraft, notifyDraft, pastDraft, problemDraft, interpreterDraft });
  latest.current = { assigned, draft, submitDraft, notifyDraft, pastDraft, problemDraft, interpreterDraft };
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const saving = useRef(false);
  const pending = useRef(false);
  const onSavedRef = useRef(props.onSaved);
  onSavedRef.current = props.onSaved;
  const onReloadRef = useRef(props.onReload);
  onReloadRef.current = props.onReload;

  // 保存する。保存の途中で変わったら、終わってからもう一度保存する（順番に 1 つずつ）
  async function flush() {
    if (saving.current) {
      pending.current = true;
      return;
    }
    saving.current = true;
    try {
      do {
        pending.current = false;
        const v = latest.current;
        const bindings: LoadedKeyBinding[] = COMMANDS.map((command) => {
          const keys = v.assigned[command];
          return keys
            ? { command, ...keys }
            : { command, key: "", meta: false, ctrl: false, alt: false, shift: false };
        });
        const outcome = await saveSettings(
          bindings,
          v.draft,
          v.submitDraft,
          v.notifyDraft,
          v.pastDraft,
          v.problemDraft,
          v.interpreterDraft,
        );
        if (outcome === "changedOutside") {
          await onReloadRef.current();
          break;
        }
        if (outcome === "cancelled") {
          // 確認のダイアログを省く設定をキャンセルした。元に戻して、ほかの変更は保存し直す
          const restored = { ...v.submitDraft, confirmDialog: true };
          latest.current = { ...latest.current, submitDraft: restored };
          setSubmitDraft(restored);
          setNotice("確認のダイアログを省く設定にしなかったので、元に戻しました");
          pending.current = true;
          continue;
        }
        setKeyBindings(bindings);
        onSavedRef.current(v.draft, v.submitDraft, v.notifyDraft, v.pastDraft, v.problemDraft, v.interpreterDraft);
      } while (pending.current);
    } catch (e) {
      setNotice(String(e));
    } finally {
      saving.current = false;
    }
  }

  function schedule() {
    if (timer.current !== null) {
      clearTimeout(timer.current);
    }
    timer.current = setTimeout(() => {
      timer.current = null;
      void flush();
    }, SAVE_DELAY_MS);
  }

  // タブを閉じたときに、まだ保存していない変更があれば、すぐ保存する
  useEffect(
    () => () => {
      if (timer.current !== null) {
        clearTimeout(timer.current);
        timer.current = null;
        void flush();
      }
    },
    [],
  );

  // 外で変わって読み直したら、中身も読み直す（保存を待っている変更は捨てる）
  useEffect(() => {
    if (resetSeq === firstSeq) {
      return;
    }
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    setAssigned(currentAssigned());
    setDraft(appearance);
    setSubmitDraft(submit);
    setNotifyDraft(notify);
    setPastDraft(atcoderProblems);
    setProblemDraft(problem);
    setInterpreterDraft(python.interpreter);
    setCapturing(null);
    setNotice("settings.json が外で変わったので、読み直しました");
  }, [resetSeq]);

  // 開いたまま隠れている間に、設定1・settings.json の読み直しで App の設定が変わったら、中身を合わせる
  // （古い中身で上書きしないように。C-8）。保存を待っている・保存している間は、こちらの変更を優先する
  useEffect(() => {
    if (timer.current !== null || saving.current) {
      return;
    }
    setAssigned(currentAssigned());
    setDraft(appearance);
    setSubmitDraft(submit);
    setNotifyDraft(notify);
    setPastDraft(atcoderProblems);
    setProblemDraft(problem);
    setInterpreterDraft(python.interpreter);
  }, [appearance, submit, notify, atcoderProblems, problem, python]);

  // 隠れたら、キーの受け取りをやめる（隠れている間に押したキーを、割り当てに使わない。C-8）
  useEffect(() => {
    if (props.hidden) {
      setCapturing(null);
    }
  }, [props.hidden]);

  // 試し見（色・表示はすぐ画面に出す）
  useEffect(() => {
    applyAppearance(draft);
    remeasureEditor();
  }, [draft]);

  // キーの受け取り（「変更」を押したあと）。アプリのキー割り当てより先に受け取り、ほかへ渡さない
  useEffect(() => {
    if (capturing === null) {
      return;
    }
    function onKeyDown(event: KeyboardEvent) {
      if (capturing === null) {
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
      schedule();
    }
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, [capturing, assigned]);

  // 中身を変える（すぐ画面に出し、少しあとに保存する）
  function change<T>(set: (update: (current: T) => T) => void, update: (current: T) => T) {
    set(update);
    schedule();
  }

  function setKeys(command: Command, keys: Keys | null) {
    if (keys) {
      const reason = refusal(keys, command, assigned);
      if (reason) {
        setNotice(reason);
        return;
      }
    }
    change(setAssigned, (current) => ({ ...current, [command]: keys }));
    setNotice(null);
  }

  function setColor(name: string, value: string | null) {
    change(setDraft, (current) => {
      const colors = { ...current.colors };
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
    change(setDraft, (current) => ({ ...current, [which]: clamped }));
  }

  // 「設定1で開く」：形を dialog にしてすぐ保存し、App がダイアログで開き直す
  async function switchToDialog() {
    if (timer.current !== null) {
      clearTimeout(timer.current);
      timer.current = null;
    }
    const next = { ...latest.current.draft, settingsUi: "dialog" as SettingsUi };
    latest.current = { ...latest.current, draft: next };
    setDraft(next);
    await flush();
    props.onSwitchToDialog();
  }

  // ---- 検索 ----
  const q = query.trim().toLowerCase();
  const hit = (...texts: (string | undefined)[]) => q === "" || texts.some((t) => t?.toLowerCase().includes(q));
  const commandShown = (command: Command) => {
    const keys = assigned[command];
    return hit(COMMAND_LABELS[command], command, keys ? keyLabel(keys) : "なし", "キー", "keyBindings");
  };
  const colorShown = (group: string, item: { name: string; label: string }) =>
    hit(group, item.label, item.name, "色", "colors");
  const colorGroups = COLOR_GROUPS.map((group, index) => ({
    id: `colors-${index}`,
    title: group.title,
    shown: group.items.some((item) => colorShown(group.title, item)),
  }));
  const groupIds = useMemo(
    () => new Map(COLOR_GROUPS.map((group, index) => [group.title, `colors-${index}`])),
    [],
  );

  const show = {
    rainbow: hit("インデントの色分け", "インデントの段に色を付ける", "editor.indentRainbow", "表示"),
    lens: hit("診断のある行", "診断のある行に色とメッセージを出す", "editor.errorLens", "エラー", "表示"),
    crumbs: hit("パンくずリスト", "エディタの上にパンくずリスト（ファイルの場所）を出す", "editor.breadcrumbs", "表示"),
    editorSize: hit("エディタの文字の大きさ", "editor.fontSize", "font", "フォント", "文字"),
    problemSize: hit("問題文の文字の大きさ", "problem.fontSize", "font", "フォント", "文字"),
    ui: hit("設定の画面", "editor.settingsUi", "設定1", "設定2", "VS Code", "ダイアログ", "表示"),
    keys: COMMANDS.some(commandShown),
    buttons: hit("見出しに並べるボタン", "problem.buttons", "問題文", ...PROBLEM_BUTTONS.map((b) => b.label)),
    language: hit("問題を作るときの言語", "problem.language", "Rust", "C++", "Python", "問題文"),
    fillInput: hit("input! を入れておく", "problem.fillInput", "入力のコード", "問題文"),
    mainLoop: hit("main() に for も書く", "problem.mainLoop", "テストケースが複数", "問題文"),
    testFirst: hit("提出の前のサンプルテスト", "submit.testBeforeSubmit", "提出"),
    confirm: hit("提出の確認のダイアログ", "submit.confirmDialog", "提出"),
    judge: hit("判定の通知", "notify.judge", "通知", "提出の判定が決まったときに知らせる"),
    start: hit("コンテストの開始の通知", "notify.contestStart", "通知", "残り時間"),
    user: hit("AtCoder の ID", "atcoderProblems.user", "過去問", "AtCoder Problems", "AC 状況"),
    python: hit("Python の処理系", "python.interpreter", "CPython", "PyPy", "言語"),
    snippets: LANGUAGES.filter(({ value, label }) => hit("スニペット", label, snippetsFile(value), "snippets")),
    templates: hit("テンプレート", "templates", "雛形", "main.rs", "main.cpp", "main.py", "dependencies.toml"),
  };
  const sections = {
    view: show.rainbow || show.lens || show.crumbs || show.editorSize || show.problemSize || show.ui,
    keys: show.keys,
    colors: colorGroups.some((g) => g.shown),
    problem: show.buttons || show.language || show.fillInput || show.mainLoop,
    submit: show.testFirst || show.confirm,
    notify: show.judge || show.start,
    past: show.user,
    python: show.python,
    snippets: show.snippets.length > 0,
    templates: show.templates,
  };

  const nav: NavItem[] = [
    {
      id: "cat-editor",
      label: "エディター",
      children: [
        { id: "view", label: "表示" },
        { id: "keys", label: "キーの割り当て" },
      ],
    },
    { id: "cat-colors", label: "色", children: colorGroups.map((g) => ({ id: g.id, label: g.title })) },
    {
      id: "cat-problem",
      label: "問題とテスト",
      children: [
        { id: "problem", label: "問題文" },
        { id: "submit", label: "提出" },
      ],
    },
    { id: "notify", label: "通知" },
    { id: "past", label: "過去問" },
    { id: "cat-lang", label: "言語", children: [{ id: "python", label: "Python" }] },
    {
      id: "cat-files",
      label: "スニペットとテンプレート",
      children: [
        { id: "snippets", label: "スニペット" },
        { id: "templates", label: "テンプレート" },
      ],
    },
  ];
  const visible = (id: string): boolean => {
    if (id in sections) {
      return sections[id as keyof typeof sections];
    }
    const group = colorGroups.find((g) => g.id === id);
    if (group) {
      return group.shown;
    }
    const item = nav.find((n) => n.id === id);
    return item?.children?.some((child) => visible(child.id)) ?? false;
  };
  const nothing = nav.every((item) => !visible(item.id));

  // 左を押したら、右をその見出しまで移す
  function jump(id: string) {
    const list = listRef.current;
    const target = list?.querySelector<HTMLElement>(`[data-nav="${id}"]`);
    if (list && target) {
      list.scrollTop = target.offsetTop - 4;
      setActive(id);
    }
  }

  // 右をスクロールしたら、今いる見出しを左で示す
  function spy() {
    const list = listRef.current;
    if (!list) {
      return;
    }
    let current: string | null = null;
    for (const element of list.querySelectorAll<HTMLElement>("[data-nav]")) {
      if (element.offsetTop - 8 <= list.scrollTop) {
        current = element.dataset.nav ?? current;
      }
    }
    if (current) {
      setActive(current);
    }
  }

  const heading = (id: string, title: string, level: 2 | 3) =>
    level === 2 ? (
      <h2 className="setting-category" data-nav={id}>
        {title}
      </h2>
    ) : (
      <h3 className="setting-section" data-nav={id}>
        {title}
      </h3>
    );

  return (
    <div className="settings-editor" hidden={props.hidden}>
      <div className="settings-editor-head">
        <input
          className="settings-search"
          type="search"
          placeholder="設定の検索（名前・説明・settings.json の名前・キー）"
          value={query}
          spellCheck={false}
          onChange={(event) => setQuery(event.target.value)}
        />
        <button type="button" className="settings-link" onClick={() => void switchToDialog()}>
          設定1で開く
        </button>
        <button type="button" className="settings-link" onClick={props.onOpenJson}>
          JSON で開く
        </button>
      </div>
      {notice && (
        <div className="settings-editor-notice" role="status">
          {notice}
        </div>
      )}
      <div className="settings-editor-main">
        <nav className="settings-tree" aria-label="設定の分類">
          {nav
            .filter((item) => visible(item.id))
            .map((item) => (
              <div key={item.id}>
                <button
                  type="button"
                  className={`settings-tree-item ${active === item.id ? "active" : ""}`}
                  onClick={() => jump(item.children?.find((child) => visible(child.id))?.id ?? item.id)}
                >
                  {item.label}
                </button>
                {item.children
                  ?.filter((child) => visible(child.id))
                  .map((child) => (
                    <button
                      key={child.id}
                      type="button"
                      className={`settings-tree-item child ${active === child.id ? "active" : ""}`}
                      onClick={() => jump(child.id)}
                    >
                      {child.label}
                    </button>
                  ))}
              </div>
            ))}
        </nav>
        <div className="settings-list" ref={listRef} onScroll={spy}>
          {nothing && <p className="setting-desc">「{query}」に合う設定はありません。</p>}

          {(sections.view || sections.keys) && heading("cat-editor", "エディター", 2)}
          {sections.view && heading("view", "表示", 3)}
          {show.rainbow && (
            <CheckItem
              title="インデントの色分け"
              jsonKey="editor.indentRainbow"
              label="インデントの段に色を付ける"
              checked={draft.indentRainbow}
              onChange={(on) => change(setDraft, (c) => ({ ...c, indentRainbow: on }))}
            />
          )}
          {show.lens && (
            <CheckItem
              title="診断のある行"
              jsonKey="editor.errorLens"
              label="診断のある行に色とメッセージを出す"
              checked={draft.errorLens}
              onChange={(on) => change(setDraft, (c) => ({ ...c, errorLens: on }))}
            />
          )}
          {show.crumbs && (
            <CheckItem
              title="パンくずリスト"
              jsonKey="editor.breadcrumbs"
              label="エディタの上にパンくずリスト（ファイルの場所）を出す"
              checked={draft.breadcrumbs}
              onChange={(on) => change(setDraft, (c) => ({ ...c, breadcrumbs: on }))}
            />
          )}
          {show.editorSize && (
            <Item title="エディタの文字の大きさ" jsonKey="editor.fontSize" description="エディタの文字の大きさ（px）。">
              <SizeField
                label="エディタの文字の大きさ"
                size={draft.editorFontSize}
                onChange={(size) => setFontSize("editorFontSize", size)}
              />
            </Item>
          )}
          {show.problemSize && (
            <Item title="問題文の文字の大きさ" jsonKey="problem.fontSize" description="問題文の欄の文字の大きさ（px）。">
              <SizeField
                label="問題文の文字の大きさ"
                size={draft.problemFontSize}
                onChange={(size) => setFontSize("problemFontSize", size)}
              />
            </Item>
          )}
          {show.ui && (
            <Item
              title="設定の画面"
              jsonKey="editor.settingsUi"
              description="設定を開くときの形。「設定1」に変えると、次に設定を開くときから窓に重ねるダイアログで開きます（すぐ切り替えるときは、上の「設定1で開く」）。"
            >
              <select
                className="settings-select"
                value={draft.settingsUi}
                onChange={(event) => {
                  const ui = event.target.value as SettingsUi;
                  change(setDraft, (c) => ({ ...c, settingsUi: ui }));
                }}
              >
                {SETTINGS_UIS.map(({ value, label }) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </Item>
          )}
          {sections.keys && heading("keys", "キーの割り当て", 3)}
          {sections.keys && (
            <Item
              title="キーの割り当て"
              jsonKey="keyBindings"
              description="「変更」を押してから、割り当てるキーを押します（Esc でやめる）。⌘・⌃・⌥ のどれかと一緒に押すキーか、F1〜F20 が使えます。"
            >
              <KeysSection
                assigned={assigned}
                capturing={capturing}
                onCapture={(command) => {
                  setCapturing(command);
                  setNotice(null);
                }}
                onSet={setKeys}
                shown={commandShown}
              />
            </Item>
          )}

          {sections.colors && heading("cat-colors", "色", 2)}
          {sections.colors && (
            <div className="setting-item">
              <div className="setting-desc">
                色の選択と濃さ（0〜100%）。変えるとすぐ画面に出ます。settings.json の colors に、変えた色だけを書きます。
              </div>
              <ColorsSection
                colors={draft.colors}
                onSet={setColor}
                shown={colorShown}
                groupIds={(group) => groupIds.get(group) ?? ""}
              />
            </div>
          )}

          {(sections.problem || sections.submit) && heading("cat-problem", "問題とテスト", 2)}
          {sections.problem && heading("problem", "問題文", 3)}
          {show.buttons && (
            <Item
              title="見出しに並べるボタン"
              jsonKey="problem.buttons"
              description="問題文の欄の見出しの行に出すボタン。出さないものは「…」の一覧に入ります。「この問題を作る」は、選択欄で選んだ問題のときだけ出ます。"
            >
              {PROBLEM_BUTTONS.map(({ name, label }) => (
                <label key={name} className="setting-check">
                  <input
                    type="checkbox"
                    checked={problemDraft.buttons.includes(name)}
                    onChange={(event) => {
                      const on = event.target.checked;
                      change(setProblemDraft, (c) => {
                        const buttons = on ? [...c.buttons, name] : c.buttons.filter((b) => b !== name);
                        return { ...c, buttons: PROBLEM_BUTTONS.map((b) => b.name).filter((b) => buttons.includes(b)) };
                      });
                    }}
                  />
                  <span>{label}</span>
                </label>
              ))}
            </Item>
          )}
          {show.language && (
            <Item
              title="問題を作るときの言語"
              jsonKey="problem.language"
              description="「コンテストを作る」「この問題を作る」は、この言語で作ります。"
            >
              <select
                className="settings-select"
                value={problemDraft.language}
                onChange={(event) => {
                  const language = event.target.value as Language;
                  change(setProblemDraft, (c) => ({ ...c, language }));
                }}
              >
                {LANGUAGES.map(({ value, label }) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </Item>
          )}
          {show.fillInput && (
            <CheckItem
              title="input! を入れておく"
              jsonKey="problem.fillInput"
              label="問題を作るときに、solve() に問題文から作った input!（C++・Python は入力のコード）を入れておく。すでにあるファイルは変えません"
              checked={problemDraft.fillInput}
              onChange={(on) => change(setProblemDraft, (c) => ({ ...c, fillInput: on }))}
            />
          )}
          {show.mainLoop && (
            <CheckItem
              title="テストケースが複数のとき"
              jsonKey="problem.mainLoop"
              label="main() に for も書く（Rust は input! の t と for _ in 0..t { solve(); }、C++・Python は T を読む for）"
              checked={problemDraft.mainLoop}
              onChange={(on) => change(setProblemDraft, (c) => ({ ...c, mainLoop: on }))}
            />
          )}
          {sections.submit && heading("submit", "提出", 3)}
          {show.testFirst && (
            <CheckItem
              title="提出の前のサンプルテスト"
              jsonKey="submit.testBeforeSubmit"
              label="提出の前にサンプルテストを動かし、すべて AC でなければ提出しない"
              checked={submitDraft.testBeforeSubmit}
              onChange={(on) => change(setSubmitDraft, (c) => ({ ...c, testBeforeSubmit: on }))}
            />
          )}
          {show.confirm && (
            <CheckItem
              title="提出の確認のダイアログ"
              jsonKey="submit.confirmDialog"
              label="提出の確認のダイアログを出す"
              checked={submitDraft.confirmDialog}
              onChange={(on) => change(setSubmitDraft, (c) => ({ ...c, confirmDialog: on }))}
              note="出さないようにしても、省くのは、今のソースのサンプルテストがすべて AC で、提出の前の検査の注意がないときだけです。出さないようにすると、OS の確認が出ます（キャンセルすると元に戻します）。"
            />
          )}

          {sections.notify && heading("notify", "通知", 2)}
          {show.judge && (
            <CheckItem
              title="判定の通知"
              jsonKey="notify.judge"
              label="IDE から出した提出の判定が決まったときに知らせる"
              checked={notifyDraft.judge}
              onChange={(on) => change(setNotifyDraft, (c) => ({ ...c, judge: on }))}
            />
          )}
          {show.start && (
            <CheckItem
              title="コンテストの開始の通知"
              jsonKey="notify.contestStart"
              label="ツールバーに残り時間を出しているコンテストが始まったときに知らせる"
              checked={notifyDraft.contestStart}
              onChange={(on) => change(setNotifyDraft, (c) => ({ ...c, contestStart: on }))}
              note="どちらも IDE の窓が手前にないときだけ知らせます。オンにするとテストの通知を 1 回出します（初めてなら macOS の許可の確認も出るので「許可」を押してください）。"
            />
          )}

          {sections.past && heading("past", "過去問", 2)}
          {show.user && (
            <Item
              title="AtCoder の ID"
              jsonKey="atcoderProblems.user"
              description="過去問の表の AC 状況に使います。空なら、ログインしているアカウントの ID を使います。ID は AtCoder Problems（kenkoooo.com。非公式のサービス）に送ります。"
            >
              <input
                className="settings-text"
                value={pastDraft.user}
                placeholder="空ならログインしているアカウント"
                maxLength={16}
                spellCheck={false}
                onChange={(event) => {
                  const user = event.target.value;
                  change(setPastDraft, () => ({ user }));
                }}
              />
            </Item>
          )}

          {sections.python && heading("cat-lang", "言語", 2)}
          {sections.python && heading("python", "Python", 3)}
          {show.python && (
            <Item
              title="Python の処理系"
              jsonKey="python.interpreter"
              description={`main.py の実行・テストに使います。${
                python.cpython || python.pypy
                  ? "処理系のパスは settings.json の python.cpython・python.pypy に書いたものを使います。"
                  : "処理系は /opt/homebrew/bin・PATH・/usr/local/bin から探します。"
              }`}
            >
              <select
                className="settings-select"
                value={interpreterDraft}
                onChange={(event) => {
                  const next = event.target.value as Interpreter;
                  change(setInterpreterDraft, () => next);
                }}
              >
                {INTERPRETERS.map(({ value, label }) => (
                  <option key={value} value={value}>
                    {label}
                  </option>
                ))}
              </select>
            </Item>
          )}

          {(sections.snippets || sections.templates) && heading("cat-files", "スニペットとテンプレート", 2)}
          {sections.snippets && heading("snippets", "スニペット", 3)}
          {sections.snippets && (
            <div className="setting-item">
              <div className="setting-desc">
                アプリのフォルダの言語ごとのファイルに、VS Code と同じ書き方で書きます。書き換えたら、IDE の窓に戻ると読み直します。
              </div>
              {show.snippets.map(({ value: language, label }) => (
                <SnippetsSection
                  key={language}
                  language={language}
                  label={label}
                  snippets={props.snippets[language]}
                  onOpen={() => props.onOpenSnippets(language).catch((e) => setNotice(String(e)))}
                  onImport={async () => {
                    setNotice(null);
                    try {
                      setNotice(await props.onImportSnippets(language));
                    } catch (e) {
                      setNotice(String(e));
                    }
                  }}
                />
              ))}
            </div>
          )}
          {sections.templates && heading("templates", "テンプレート", 3)}
          {sections.templates && (
            <div className="setting-item">
              <TemplatesSection onError={setNotice} />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export default SettingsEditor;
