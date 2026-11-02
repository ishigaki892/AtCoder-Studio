import type { StepAction } from "../ipc/debug";
import type { ButtonView, debugButtons } from "./runState";
import { withShortcut, type Command } from "../keybindings";

type Props = {
  run: ButtonView;
  test: ButtonView;
  random: ButtonView;
  interactive: ButtonView;
  debug: ButtonView;
  // 「提出」（Phase 6。D-148 §2.2）。「デバッグ」の右に、少し間をあけて置く
  submit: ButtonView;
  // 提出する問題（例：「awc0186_a」）。有効なボタンのツールチップに出す
  submitLabel: string | null;
  // デバッグ中なら、「デバッグ」の代わりに並べるボタン（D-146 2.5）。デバッグしていなければ null
  debugging: ReturnType<typeof debugButtons> | null;
  // 有効なボタンのツールチップに出す実行の対象（例：「abc の a」）
  targetLabel: string | null;
  onRun: () => void;
  onTest: () => void;
  onRandom: () => void;
  onInteractive: () => void;
  onDebug: () => void;
  onDebugAction: (action: StepAction) => void;
  onDebugStop: () => void;
  onSubmit: () => void;
};

// 1 つのボタン。無効なときもツールチップが出るように、disabled ではなく aria-disabled にする（D-023）。
// ツールチップは title ではなく自前で描く。WebView の title のツールチップが、画面で出なかったため
// （docs/phase2/05-sample-test.md §6）。マウスを乗せたときと、キーボードで選んだときに出す
function RunButton({
  view,
  hint,
  onClick,
  className,
}: {
  view: ButtonView;
  hint: string | undefined;
  onClick: () => void;
  // 見た目を変えるときに足すクラス（「提出」は青）
  className?: string;
}) {
  const disabled = view.disabledReason !== null;
  const tip = view.disabledReason ?? hint;
  return (
    <span className="run-button-wrap" data-tip={tip}>
      <button
        type="button"
        className={`run-button ${className ?? ""}`}
        aria-disabled={disabled}
        aria-description={tip}
        onClick={() => {
          if (!disabled) {
            onClick();
          }
        }}
      >
        {view.label}
      </button>
    </span>
  );
}

// ボタンゾーンの「実行」「サンプルテスト」「ランダムテスト」「対話テスト」「デバッグ」「提出」（D-044・D-100・D-143・D-144・D-146・D-148）。
// デバッグ中は「デバッグ」の位置に、続行／一時停止・ステップオーバー・ステップイン・ステップアウト・停止を並べる
function RunButtons({
  run,
  test,
  random,
  interactive,
  debug,
  submit,
  submitLabel,
  debugging,
  targetLabel,
  onRun,
  onTest,
  onRandom,
  onInteractive,
  onDebug,
  onDebugAction,
  onDebugStop,
  onSubmit,
}: Props) {
  // マウスを乗せたときの説明：ボタンの名前と今のキー（設定で変えたものに合わせる。D-245）と、実行の対象。
  // 実行中で「停止」になっているボタンは、停止のキーを出す。キーのないもの（ランダムテストなど）は名前だけ
  const hint = (view: ButtonView, command: Command | null) => {
    const name =
      view.label === "停止" ? withShortcut("停止", "stop") : command ? withShortcut(view.label, command) : view.label;
    return targetLabel ? `${name}・実行の対象：${targetLabel}` : name;
  };
  return (
    <div className="run-buttons">
      <RunButton view={run} hint={hint(run, "run")} onClick={onRun} />
      <RunButton view={test} hint={hint(test, "test")} onClick={onTest} />
      <RunButton view={random} hint={hint(random, null)} onClick={onRandom} />
      <RunButton view={interactive} hint={hint(interactive, null)} onClick={onInteractive} />
      {debugging ? (
        <span className="debug-buttons" role="group" aria-label="デバッグの操作">
          <RunButton
            view={debugging.resume}
            hint={withShortcut(debugging.resume.action === "pause" ? "一時停止" : "続行", "debug")}
            onClick={() => onDebugAction(debugging.resume.action)}
          />
          <RunButton
            view={debugging.next}
            hint={withShortcut("ステップオーバー", "stepOver")}
            onClick={() => onDebugAction("next")}
          />
          <RunButton
            view={debugging.stepIn}
            hint={withShortcut("ステップイン", "stepIn")}
            onClick={() => onDebugAction("stepIn")}
          />
          <RunButton
            view={debugging.stepOut}
            hint={withShortcut("ステップアウト", "stepOut")}
            onClick={() => onDebugAction("stepOut")}
          />
          <RunButton view={debugging.stop} hint={withShortcut("デバッグを止める", "stop")} onClick={onDebugStop} />
        </span>
      ) : (
        <RunButton view={debug} hint={hint(debug, "debug")} onClick={onDebug} />
      )}
      {/* プルダウンは付けない（D-016）。送る前に、サンプルテストと Rust 側の確認のダイアログで確かめる（D-162） */}
      <span className="submit-gap">
        <RunButton
          view={submit}
          hint={`${withShortcut("提出", "submit")}${submitLabel ? `・AtCoder に提出：${submitLabel}（送る前に確かめます）` : ""}`}
          onClick={onSubmit}
          className="submit-button"
        />
      </span>
    </div>
  );
}

export default RunButtons;
