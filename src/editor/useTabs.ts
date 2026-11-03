import { useRef, useState } from "react";
import type { EditorState } from "@codemirror/state";
import {
  appFileStamps,
  askDiscardChanges,
  askExternalChange,
  fileStamps,
  readAppFile,
  readFile,
  readSource,
  writeAppFile,
  writeFile,
  type Stamp,
} from "../ipc/workspace";
import { appName, isAppKey } from "./readOnlyKey";
import { isCppPath, isPythonPath } from "./language";
import { createEditorState } from "./Editor";
import { fromFileContent, toFileContent, type TextFormat } from "./fileText";
import { notifySaved } from "./lsp";

// 編集が止まってから自動保存するまでの待ち時間（D-037。値はユーザーの判断、2026-10-16）
export const AUTOSAVE_DELAY_MS = 1000;

// エディタのタブ（作業 4b）
export type Tab = {
  // 作業領域のルートからの相対パス。読み取り専用のタブ（readOnly がある）は file: の URI
  path: string;
  format: TextFormat;
  dirty: boolean; // 未保存の変更があるか
  stamp: Stamp; // 最後に読み込んだ・保存したときの更新日時と大きさ（作業 5）
  // 作業領域の外の標準ライブラリ・依存クレートのソース（Phase 3b-2 作業 5。D-133）。編集・保存・自動保存・外部での変更の確認をしない。
  // location は画面に出す場所
  readOnly?: { location: string };
};

function baseName(path: string) {
  return path.slice(path.lastIndexOf("/") + 1);
}

function sameStamp(a: Stamp, b: Stamp) {
  return a.modified === b.modified && a.size === b.size;
}

// タブのキーで、作業領域のファイルか、アプリのフォルダのファイル（settings.json など。D-247）かを分けて読み書きする
function readTabFile(path: string) {
  return isAppKey(path) ? readAppFile(appName(path)) : readFile(path);
}

function writeTabFile(path: string, content: string, expected: Stamp | null) {
  return isAppKey(path) ? writeAppFile(appName(path), content, expected) : writeFile(path, content, expected);
}

// 今の更新日時と大きさ（paths の順）。作業領域のファイルとアプリのフォルダのファイルを、それぞれまとめて聞く
async function tabFileStamps(paths: string[]): Promise<(Stamp | null)[]> {
  const app = paths.filter(isAppKey);
  const workspace = paths.filter((path) => !isAppKey(path));
  const [appStamps, workspaceStamps] = await Promise.all([
    app.length > 0 ? appFileStamps(app.map(appName)) : Promise.resolve([]),
    workspace.length > 0 ? fileStamps(workspace) : Promise.resolve([]),
  ]);
  return paths.map((path) =>
    isAppKey(path) ? (appStamps[app.indexOf(path)] ?? null) : (workspaceStamps[workspace.indexOf(path)] ?? null),
  );
}

// タブの一覧と、開いているタブを扱う。
// エディタの状態（内容・取り消しの履歴・カーソル）はタブごとに参照で持ち、入力のたびに描き直さない。
// タブの一覧も参照で持ち（非同期の処理から最新の値を読むため）、変えたときだけ描き直す
// onSaved は、タブを保存できたとき（外での変更を読み直したときは呼ばない）に呼ぶ。settings.json などを、保存したらすぐ使うため（D-247）
export function useTabs(onError: (message: string) => void, onSaved?: (path: string) => void) {
  const onSavedRef = useRef(onSaved);
  onSavedRef.current = onSaved;
  const tabsRef = useRef<Tab[]>([]);
  const [tabs, setTabsState] = useState<Tab[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const activeRef = useRef<string | null>(null);
  const states = useRef(new Map<string, EditorState>());
  // 自動保存の待ち時間のタイマー（タブごと）
  const timers = useRef(new Map<string, ReturnType<typeof setTimeout>>());
  // 同じファイルの保存を重ねないように、タブごとに順番に行う
  const saving = useRef(new Map<string, Promise<boolean>>());
  // 「再読み込み／上書き」の画面を出している間か、外部での変更を確かめている間は true。
  // 画面を出すとウィンドウのフォーカスが外れて戻るので、その間は確かめ直さない
  const checking = useRef(false);

  async function ask(name: string): Promise<boolean> {
    const wasChecking = checking.current;
    checking.current = true;
    try {
      return await askExternalChange(name);
    } finally {
      checking.current = wasChecking;
    }
  }

  function commit(next: Tab[]) {
    tabsRef.current = next;
    setTabsState(next);
  }

  function find(path: string) {
    return tabsRef.current.find((tab) => tab.path === path);
  }

  function patch(path: string, change: Partial<Tab>) {
    commit(tabsRef.current.map((tab) => (tab.path === path ? { ...tab, ...change } : tab)));
  }

  function activate(path: string | null) {
    activeRef.current = path;
    setActivePath(path);
  }

  function clearTimer(path: string) {
    const timer = timers.current.get(path);
    if (timer !== undefined) {
      clearTimeout(timer);
      timers.current.delete(path);
    }
  }

  // ファイルをタブで開く。すでに開いていれば、そのタブに切り替える。
  // quiet なら、開けなくてもエラーを表示しない（起動したときに前回のタブを開き直すとき）
  async function open(path: string, options?: { quiet?: boolean }) {
    if (states.current.has(path)) {
      activate(path);
      return;
    }
    try {
      const { content, stamp } = await readTabFile(path);
      // 読み込みを待つ間に、同じファイルが開かれていれば（ダブルクリックなど）、そのタブを使う（Phase 10 作業 9）
      if (states.current.has(path)) {
        activate(path);
        return;
      }
      const { text, format } = fromFileContent(content);
      states.current.set(path, createEditorState(text, { snippets: path.endsWith(".rs") || isCppPath(path) || isPythonPath(path), path }));
      commit([...tabsRef.current, { path, format, dirty: false, stamp }]);
      activate(path);
    } catch (e) {
      if (!options?.quiet) {
        onError(String(e));
      }
    }
  }

  // 作業領域の外のソースを、読み取り専用のタブで開く（D-133）。すでに開いていれば、そのタブに切り替える。
  // 開けたら null、開けなければ理由を返す（エラーの表示は呼び出し側で決める）
  async function openReadOnly(uri: string): Promise<string | null> {
    if (states.current.has(uri)) {
      activate(uri);
      return null;
    }
    try {
      const { content, location, language } = await readSource(uri);
      // 読み込みを待つ間に、同じソースが開かれていれば、そのタブを使う（Phase 10 作業 9）
      if (states.current.has(uri)) {
        activate(uri);
        return null;
      }
      const { text } = fromFileContent(content);
      states.current.set(uri, createEditorState(text, { readOnly: true, path: uri, language }));
      commit([
        ...tabsRef.current,
        {
          path: uri,
          format: { crlf: false, bom: false },
          dirty: false,
          stamp: { modified: "0", size: 0 },
          readOnly: { location },
        },
      ]);
      activate(uri);
      return null;
    } catch (e) {
      return String(e);
    }
  }

  // ファイルを読み直し、タブの内容を入れ替える（未保存の編集は捨てる）
  async function reload(path: string) {
    try {
      const { content, stamp } = await readTabFile(path);
      const { text, format } = fromFileContent(content);
      clearTimer(path);
      states.current.set(path, createEditorState(text, { snippets: path.endsWith(".rs") || isCppPath(path) || isPythonPath(path), path }));
      patch(path, { format, stamp, dirty: false });
    } catch (e) {
      onError(String(e));
    }
  }

  // 1 回だけ書く。force なら外部での変更を確かめずに書く
  async function writeOnce(path: string, force: boolean): Promise<boolean> {
    const tab = find(path);
    const state = states.current.get(path);
    if (!tab || !state) {
      return true;
    }
    // 保存する時点の文書。CodeMirror の文書は編集のたびに新しいものになる
    const savedDoc = state.doc;
    const result = await writeTabFile(path, toFileContent(savedDoc.toString(), tab.format), force ? null : tab.stamp);
    if (result.kind === "conflict") {
      // 外部で変更されていた。未保存の編集があるので、再読み込みか上書きかを選んでもらう（D-038）
      if (await ask(baseName(path))) {
        await reload(path);
        return true;
      }
      return writeOnce(path, true);
    }
    // 保存している間に編集があれば、未保存の印を残す
    patch(path, { stamp: result.stamp, dirty: states.current.get(path)?.doc !== savedDoc });
    // rust-analyzer に保存を知らせる（保存時の検査。Phase 3b-1）
    notifySaved(path);
    onSavedRef.current?.(path);
    return true;
  }

  // タブを保存する。未保存の変更がなければ何もしない。成功したら true
  function save(path: string): Promise<boolean> {
    clearTimer(path);
    const previous = saving.current.get(path) ?? Promise.resolve(true);
    const next = previous.then(async () => {
      if (!find(path)?.dirty) {
        return true;
      }
      try {
        return await writeOnce(path, false);
      } catch (e) {
        onError(String(e));
        return false;
      }
    });
    saving.current.set(path, next);
    return next;
  }

  // エディタで編集されたときに呼ぶ。編集が止まってから待ち時間が過ぎたら自動保存する（D-037）
  function update(path: string, state: EditorState, docChanged: boolean) {
    states.current.set(path, state);
    if (!docChanged) {
      return;
    }
    if (!find(path)?.dirty) {
      patch(path, { dirty: true });
    }
    clearTimer(path);
    timers.current.set(
      path,
      setTimeout(() => void save(path), AUTOSAVE_DELAY_MS),
    );
  }

  // 開いているタブを、待ち時間を待たずに保存する（⌘S。D-051）
  async function saveActive() {
    const path = activeRef.current;
    if (path) {
      await save(path);
    }
  }

  // 未保存のタブをすべて保存し、保存できなかったタブのパスを返す
  async function saveAllReporting(): Promise<string[]> {
    const dirty = tabsRef.current.filter((tab) => tab.dirty);
    const results = await Promise.all(dirty.map((tab) => save(tab.path)));
    return dirty.filter((_, index) => !results[index]).map((tab) => tab.path);
  }

  // 未保存のタブをすべて保存する（ウィンドウのフォーカスが外れたとき。D-037）。すべて成功したら true
  async function saveAll(): Promise<boolean> {
    return (await saveAllReporting()).length === 0;
  }

  function remove(path: string) {
    clearTimer(path);
    const current = tabsRef.current;
    const index = current.findIndex((t) => t.path === path);
    if (index < 0) {
      return;
    }
    const rest = current.filter((t) => t.path !== path);
    states.current.delete(path);
    commit(rest);
    if (path === activeRef.current) {
      // 右隣、なければ左隣のタブに切り替える
      const next = rest[index] ?? rest[index - 1] ?? null;
      activate(next ? next.path : null);
    }
  }

  // タブを閉じる。未保存なら先に保存する。保存できなければ（外で消された・書き込めないなど）、
  // 変更を捨てて閉じるかを聞き、捨てないなら閉じない（D-069 を置き換え。Phase 10 作業 1。D-164）
  async function close(path: string) {
    if (find(path)?.dirty && !(await save(path)) && !(await askDiscardChanges([path], "close"))) {
      return;
    }
    remove(path);
  }

  // すべてのタブを閉じる（別の作業領域を開くとき）。discard なら保存しない（呼び出し側が、変更を捨てると決めたとき）。
  // そうでなければ未保存のタブを保存し、保存できなければ捨てるかを聞き、捨てないなら閉じずに false を返す
  async function closeAll(discard = false): Promise<boolean> {
    if (!discard) {
      const failed = await saveAllReporting();
      if (failed.length > 0 && !(await askDiscardChanges(failed, "folder"))) {
        return false;
      }
    }
    for (const path of timers.current.keys()) {
      clearTimer(path);
    }
    states.current.clear();
    commit([]);
    activate(null);
    return true;
  }

  // ウィンドウにフォーカスが戻ったとき、開いているファイルが外部で変更されていないかを確かめる（D-038）
  async function checkExternalChanges() {
    if (tabsRef.current.every((tab) => tab.readOnly) || checking.current) {
      return;
    }
    checking.current = true;
    try {
      // 進行中の保存（フォーカスが外れたときの保存など）が終わるのを待つ。
      // 待たないと、保存する前の更新日時と比べて、外部で変更されたと誤って判定する
      await Promise.all(saving.current.values());
      // 読み取り専用のタブ（作業領域の外）は確かめない
      const current = tabsRef.current.filter((tab) => !tab.readOnly);
      if (current.length === 0) {
        return;
      }
      const stamps = await tabFileStamps(current.map((tab) => tab.path));
      for (const [i, tab] of current.entries()) {
        const stamp = stamps[i];
        // 保存で更新された最新の値と比べる。閉じたタブは確かめない
        const known = find(tab.path)?.stamp;
        if (!known) {
          continue;
        }
        if (stamp === null) {
          onError(`「${baseName(tab.path)}」が見つかりません。ほかのプログラムで移動または削除された可能性があります`);
        } else if (!sameStamp(stamp, known)) {
          if (!find(tab.path)?.dirty) {
            // 未保存の編集がなければ、黙って読み直す
            await reload(tab.path);
          } else if (await ask(baseName(tab.path))) {
            await reload(tab.path);
          } else {
            clearTimer(tab.path);
            await writeOnce(tab.path, true);
          }
        }
      }
    } catch (e) {
      onError(String(e));
    } finally {
      checking.current = false;
    }
  }

  // ファイルの名前が変わったとき、タブのパスを付け替える（内容と未保存の状態はそのまま。作業 4c）
  function renamePath(from: string, to: string) {
    const state = states.current.get(from);
    if (!state) {
      return;
    }
    const wasScheduled = timers.current.has(from);
    clearTimer(from);
    states.current.delete(from);
    states.current.set(to, state);
    commit(tabsRef.current.map((tab) => (tab.path === from ? { ...tab, path: to } : tab)));
    if (activeRef.current === from) {
      activate(to);
    }
    if (wasScheduled) {
      timers.current.set(
        to,
        setTimeout(() => void save(to), AUTOSAVE_DELAY_MS),
      );
    }
  }

  // 名前を変える前に呼ぶ。そのファイルのタブがあれば保存し終える（進行中の保存や自動保存が、
  // 名前を変えた後に古いパスへ書いて失敗し、未保存のまま残らないように）。保存できなければ false
  async function prepareRename(path: string): Promise<boolean> {
    if (!find(path)) {
      return true;
    }
    return save(path);
  }

  // ファイルを削除したとき、未保存かどうかにかかわらずタブを閉じる（削除の確認は済んでいる。D-072）
  function discard(path: string) {
    remove(path);
  }

  function stateOf(path: string): EditorState | undefined {
    return states.current.get(path);
  }

  return {
    tabs,
    activePath,
    setActivePath: activate,
    open,
    openReadOnly,
    reload,
    update,
    saveActive,
    saveAll,
    saveAllReporting,
    close,
    closeAll,
    checkExternalChanges,
    prepareRename,
    renamePath,
    discard,
    stateOf,
  };
}
