import { memo, useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import type { CaseResult, Diagnostic } from "../ipc/run";
import type { LogEntry } from "./runLog";
import { copyText } from "./copyText";
import { splitLocations, type Location } from "./logLinks";
import HistoryView from "./HistoryView";
import TestResults, { AddCase, type TestState } from "./TestResults";
import DebugConsole from "./DebugConsole";
import type { ConsoleEntry } from "./debugView";
import type { HistoryEntry } from "../ipc/history";

// 下部パネルのタブ（原典 §4.7、D-043）。デバッグコンソールは Phase 5 作業 3（D-146 2.8）
export const PANEL_TABS = [
  { id: "log", label: "実行ログ" },
  { id: "results", label: "テスト結果" },
  { id: "errors", label: "エラー一覧" },
  { id: "debugConsole", label: "デバッグコンソール" },
  // 実行履歴（Phase 4a 作業 4。D-137。原典 §4.7 のタブの候補にない追加）
  { id: "history", label: "履歴" },
] as const;

export type PanelTab = (typeof PANEL_TABS)[number]["id"];

type Props = {
  selected: PanelTab;
  onSelect: (tab: PanelTab) => void;
  // 実行の対象（例：「A の abc011_1」）。タブの列の右端に出す（D-105）
  targetLabel: string | null;
  // rust-analyzer の状態（例：「rust-analyzer：準備完了」）。「対象」の左に出す（D-130）。null なら出さない
  lspStatus: { text: string; failed: boolean } | null;
  // 実行ログ。null ならまだ実行していない
  log: LogEntry[] | null;
  diagnostics: Diagnostic[] | null;
  // サンプルテストの結果。null ならまだ実行していない
  testState: TestState | null;
  // エラー一覧の項目・実行ログの場所を押したときに、その場所へ移動する（Phase 4a 作業 1。D-137）
  onOpenLocation: (file: string, line: number, column: number | null) => void;
  // 実行ログを消す（表示だけ。保存しているものはない）
  onClearLog: () => void;
  // テスト結果の「このケースだけ実行」「失敗したケースだけ再実行」（Phase 4a 作業 2）。blocked は押せない理由
  caseRunBlocked: string | null;
  onRunCases: (names: string[]) => void;
  // テスト結果の「このケースをデバッグ」（Phase 5 作業 3。D-146 2.4）。押せない理由は caseRunBlocked と同じ
  onDebugCase: (name: string) => void;
  // デバッグコンソール（Phase 5 作業 3。D-146 2.8）。entries が null ならまだデバッグしていない。
  // consoleBlocked は入力欄を使えない理由
  consoleEntries: ConsoleEntry[] | null;
  consoleBlocked: string | null;
  onEvaluate: (expression: string) => void;
  // 「出力を期待出力として保存」（Phase 4a 作業 3）と、その結果の知らせ
  onSaveExpected: (result: CaseResult) => void;
  testNotice: string | null;
  // 「テストケースを足す」（Phase 25 作業 1。D-243）。addCaseBlocked は押せない理由。足せたら true
  addCaseBlocked: string | null;
  onAddCase: (input: string, expected: string) => Promise<boolean>;
  // 実行履歴（Phase 4a 作業 4）。null なら作業領域を開いていない
  history: HistoryEntry[] | null;
  onClearHistory: () => void;
  // 履歴の提出の行の「詳細」（Phase 6）
  onOpenSubmission: (contest: string, submissionId: number) => void;
  // 実行中は消せない（記録と重ならないように）
  historyClearBlocked: string | null;
};

// 場所（ファイル名:行:列）は、押すとエディタで開く。作業領域の外などで開けない場所は、呼び出し側が理由を示す
function LocationButton({
  location,
  children,
  onOpen,
}: {
  location: Location;
  children: string;
  onOpen: Props["onOpenLocation"];
}) {
  return (
    <button
      type="button"
      className="location-link"
      title="エディタで開く"
      onClick={() => onOpen(location.file, location.line, location.column)}
    >
      {children}
    </button>
  );
}

// 長いログでは、場所を探す処理を省く（出力のたびに全体を調べ直すため）
const LINKIFY_LIMIT = 200_000;

// 1 つの出力のかたまり。中身（entry）が同じなら描き直さない。出力が届くと、変わるのは最後のかたまりだけ
// （appendLog は前のかたまりをそのまま使う）なので、場所を探し直すのも最後のかたまりだけになる（Phase 10 作業 10）
const LogText = memo(function LogText({
  entry,
  onOpen,
}: {
  entry: LogEntry;
  onOpen: Props["onOpenLocation"];
}) {
  // 標準出力は利用者のプログラムの出力そのものなので、場所とみなさない。標準エラー（panic）とビルドの出力（コンパイルエラー）だけ
  const linkable =
    (entry.kind === "stderr" || entry.kind === "build") && entry.text.length <= LINKIFY_LIMIT;
  const segments = useMemo(() => (linkable ? splitLocations(entry.text) : null), [linkable, entry.text]);
  return (
    <span className={`run-log-${entry.kind}`}>
      {segments
        ? segments.map((segment, index) =>
            segment.location ? (
              <LocationButton key={index} location={segment.location} onOpen={onOpen}>
                {segment.text}
              </LocationButton>
            ) : (
              segment.text
            ),
          )
        : entry.text}
    </span>
  );
});

function RunLogView({ log, onOpen }: { log: LogEntry[]; onOpen: Props["onOpenLocation"] }) {
  const end = useRef<HTMLDivElement>(null);
  // 場所を押したときの処理は、描き直しで変わっても、かたまりを描き直さなくてよいように、参照で渡す
  const onOpenRef = useRef(onOpen);
  onOpenRef.current = onOpen;
  const open = useCallback<Props["onOpenLocation"]>(
    (file, line, column) => onOpenRef.current(file, line, column),
    [],
  );
  // いちばん下を見ているか。上へ移って読んでいる間は、出力が増えても下へ追わない（Phase 10 作業 10）
  const stick = useRef(true);
  useEffect(() => {
    const box = end.current?.closest(".panel-body");
    if (!box) {
      return;
    }
    const onScroll = () => {
      stick.current = box.scrollHeight - box.scrollTop - box.clientHeight < 24;
    };
    box.addEventListener("scroll", onScroll);
    return () => box.removeEventListener("scroll", onScroll);
  }, []);
  // 出力が増えたら、いちばん下を見ていたときだけ、いちばん下を表示する
  useEffect(() => {
    if (stick.current) {
      end.current?.scrollIntoView({ block: "end" });
    }
  }, [log]);
  return (
    <pre className="run-log">
      {log.map((entry, index) => (
        <LogText key={index} entry={entry} onOpen={open} />
      ))}
      <div ref={end} />
    </pre>
  );
}

function DiagnosticsView({
  diagnostics,
  onOpen,
}: {
  diagnostics: Diagnostic[];
  onOpen: Props["onOpenLocation"];
}) {
  if (diagnostics.length === 0) {
    return <div className="panel-empty">エラー・警告はありません</div>;
  }
  return (
    <ul className="diagnostics">
      {diagnostics.map((item, index) => (
        <li key={index} className={`diagnostic diagnostic-${item.level}`}>
          <div className="diagnostic-head">
            <span className="diagnostic-level">{item.level === "error" ? "エラー" : item.level === "warning" ? "警告" : item.level}</span>
            {item.file &&
              (item.line !== null ? (
                <LocationButton
                  location={{ file: item.file, line: item.line, column: null }}
                  onOpen={onOpen}
                >
                  {`${item.file}:${item.line}`}
                </LocationButton>
              ) : (
                <span className="diagnostic-place">{item.file}</span>
              ))}
          </div>
          <pre className="diagnostic-rendered">{item.rendered}</pre>
        </li>
      ))}
    </ul>
  );
}

// 下部パネル
function BottomPanel({
  selected,
  onSelect,
  targetLabel,
  lspStatus,
  log,
  diagnostics,
  testState,
  onOpenLocation,
  onClearLog,
  caseRunBlocked,
  onRunCases,
  onDebugCase,
  consoleEntries,
  consoleBlocked,
  onEvaluate,
  onSaveExpected,
  addCaseBlocked,
  onAddCase,
  testNotice,
  history,
  onClearHistory,
  onOpenSubmission,
  historyClearBlocked,
}: Props) {
  const tabRefs = useRef<(HTMLButtonElement | null)[]>([]);
  // 「コピーしました」を少しの間だけ出す。失敗したときは「コピーできませんでした」
  const [copyNote, setCopyNote] = useState<string | null>(null);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (copyTimer.current !== null) {
        clearTimeout(copyTimer.current);
      }
    },
    [],
  );
  const logIsEmpty = !log || log.every((entry) => entry.text === "");

  async function handleCopy() {
    const copied = await copyText((log ?? []).map((entry) => entry.text).join(""));
    setCopyNote(copied ? "コピーしました" : "コピーできませんでした");
    if (copyTimer.current !== null) {
      clearTimeout(copyTimer.current);
    }
    copyTimer.current = setTimeout(() => setCopyNote(null), 2000);
  }

  // 左右の矢印キーでタブを移る（tablist の操作）
  function handleKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") {
      return;
    }
    event.preventDefault();
    const index = PANEL_TABS.findIndex((tab) => tab.id === selected);
    const step = event.key === "ArrowRight" ? 1 : -1;
    const next = (index + step + PANEL_TABS.length) % PANEL_TABS.length;
    onSelect(PANEL_TABS[next].id);
    tabRefs.current[next]?.focus();
  }

  return (
    <>
      <div className="panel-tabs" role="tablist" onKeyDown={handleKeyDown}>
        {PANEL_TABS.map((tab, index) => (
          <button
            key={tab.id}
            ref={(element) => {
              tabRefs.current[index] = element;
            }}
            type="button"
            role="tab"
            className={`panel-tab ${tab.id === selected ? "active" : ""}`}
            aria-selected={tab.id === selected}
            tabIndex={tab.id === selected ? 0 : -1}
            onClick={() => onSelect(tab.id)}
          >
            {tab.label}
            {tab.id === "errors" && diagnostics && diagnostics.length > 0
              ? ` (${diagnostics.length})`
              : ""}
          </button>
        ))}
        {selected === "log" && (
          <span className="panel-actions">
            {copyNote && <span className="panel-note">{copyNote}</span>}
            <button
              type="button"
              className="panel-action"
              disabled={logIsEmpty}
              title={logIsEmpty ? "ログがありません" : "実行ログの全文をコピーします"}
              onClick={() => void handleCopy()}
            >
              コピー
            </button>
            <button
              type="button"
              className="panel-action"
              disabled={logIsEmpty}
              title={logIsEmpty ? "ログがありません" : "実行ログを消去します（次の実行までそのままです）"}
              onClick={onClearLog}
            >
              消去
            </button>
          </span>
        )}
        {lspStatus && (
          <span className={`panel-lsp ${lspStatus.failed ? "failed" : ""}`}>{lspStatus.text}</span>
        )}
        <span className="panel-target">対象：{targetLabel ?? "なし"}</span>
      </div>
      <div className="panel-body" role="tabpanel">
        {selected === "log" && log ? (
          logIsEmpty ? (
            <div className="panel-empty">ログは空です</div>
          ) : (
            <RunLogView log={log} onOpen={onOpenLocation} />
          )
        ) : selected === "history" ? (
          history ? (
            <HistoryView
              entries={history}
              onClear={onClearHistory}
              onOpenSubmission={onOpenSubmission}
              clearBlocked={historyClearBlocked}
            />
          ) : (
            <div className="panel-empty">フォルダが開かれていません</div>
          )
        ) : selected === "debugConsole" ? (
          <DebugConsole entries={consoleEntries} inputBlocked={consoleBlocked} onEvaluate={onEvaluate} />
        ) : selected === "errors" && diagnostics ? (
          <DiagnosticsView diagnostics={diagnostics} onOpen={onOpenLocation} />
        ) : selected === "results" ? (
          <>
            <AddCase blocked={addCaseBlocked} onAdd={onAddCase} />
            {testState ? (
              <TestResults
                state={testState}
                blocked={caseRunBlocked}
                onRunCases={onRunCases}
                onDebugCase={onDebugCase}
                onSaveExpected={onSaveExpected}
                notice={testNotice}
              />
            ) : (
              <>
                {testNotice && <div className="test-notice">{testNotice}</div>}
                <div className="panel-empty">まだ実行していません</div>
              </>
            )}
          </>
        ) : (
          <div className="panel-empty">まだ実行していません</div>
        )}
      </div>
    </>
  );
}

export default BottomPanel;
