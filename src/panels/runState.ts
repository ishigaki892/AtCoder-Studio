// 実行の状態と、ボタンゾーンのボタンの表示（D-013 の状態表、D-023 の無効の理由）

// 待機中、通常実行中、サンプルテスト中、ランダムテスト中、対話テスト中、デバッグ中。
// IDE 全体で同時に動く処理は 1 つだけ（D-013。デバッグも含む。D-146 2.5）
export type RunState =
  | "idle"
  | "running"
  | "testing"
  | "randomTesting"
  | "interactiveTesting"
  // 計算量を測っている（Phase 45。D-270）
  | "measuring"
  | "debugging";

// デバッグ中の段階：ビルド・起動の途中、プログラムが動いている、止まっている
export type DebugPhase = "starting" | "running" | "stopped";

export type ButtonView = {
  label: string;
  // 無効なら、その理由（ツールチップで示す。D-023）。有効なら null
  disabledReason: string | null;
};

const BUSY = "別の処理を実行中です";
const NO_TARGET = "実行の対象のファイルがありません";
// D-128：信頼していない作業領域では、実行・サンプルテストを使わない。「信頼しない」も記録するので、開き直しても聞かない。
// ファイル一覧の上の「信頼する…」から信頼し直す（D-172。Rust 側の文と同じ案内）
export const UNTRUSTED = "このフォルダを信頼していないので使えません（ファイル一覧の上の「信頼する…」から信頼できます）";

// 結果の「このケースだけ実行」「失敗したケースだけ再実行」を押せない理由。押せるなら null（Phase 4a 作業 2。D-023）。
// 実行の対象は、そのテストを実行したときの対象（hasTarget）。今開いているファイルではない
export function caseRunBlock(state: RunState, hasTarget: boolean, trusted: boolean): string | null {
  if (state !== "idle") {
    return BUSY;
  }
  if (!trusted) {
    return UNTRUSTED;
  }
  return hasTarget ? null : NO_TARGET;
}

// D-013 の状態表。hasTarget は、実行の対象のバイナリが決まっているか（D-088）。trusted は作業領域を信頼しているか（D-128）
export function runButtons(
  state: RunState,
  hasTarget: boolean,
  trusted: boolean,
): {
  run: ButtonView;
  test: ButtonView;
  random: ButtonView;
  interactive: ButtonView;
  complexity: ButtonView;
  debug: ButtonView;
} {
  // 動いている処理のボタンだけが「停止」になり、ほかは押せない
  const busy = (active: RunState | null) => ({
    run: { label: active === "running" ? "停止" : "実行", disabledReason: active === "running" ? null : BUSY },
    test: {
      label: active === "testing" ? "停止" : "サンプルテスト",
      disabledReason: active === "testing" ? null : BUSY,
    },
    random: {
      label: active === "randomTesting" ? "停止" : "ランダムテスト",
      disabledReason: active === "randomTesting" ? null : BUSY,
    },
    interactive: {
      label: active === "interactiveTesting" ? "停止" : "対話テスト",
      disabledReason: active === "interactiveTesting" ? null : BUSY,
    },
    complexity: {
      label: active === "measuring" ? "停止" : "計算量",
      disabledReason: active === "measuring" ? null : BUSY,
    },
    // デバッグ中は、このボタンの代わりにデバッグの操作のボタンを並べる（debugButtons）
    debug: { label: "デバッグ", disabledReason: BUSY },
  });
  if (state !== "idle") {
    return busy(state);
  }
  const reason = !trusted ? UNTRUSTED : hasTarget ? null : NO_TARGET;
  return {
    run: { label: "実行", disabledReason: reason },
    test: { label: "サンプルテスト", disabledReason: reason },
    random: { label: "ランダムテスト", disabledReason: reason },
    interactive: { label: "対話テスト", disabledReason: reason },
    complexity: { label: "計算量", disabledReason: reason },
    debug: { label: "デバッグ", disabledReason: reason },
  };
}

// 「提出」ボタン（Phase 6 作業 3。D-148 §2.2）。押せる条件：信頼した作業領域で、実行の対象と問題が決まっていて、
// ほかの処理（実行・テスト・デバッグ）の間でも、提出を送っている間でもない。判定を待っている間は押せる
export function submitButton(
  state: RunState,
  hasTarget: boolean,
  trusted: boolean,
  hasProblem: boolean,
  sending: boolean,
): ButtonView {
  const reason =
    state !== "idle"
      ? BUSY
      : sending
        ? "提出を送っているところです"
        : !trusted
          ? UNTRUSTED
          : !hasTarget
            ? NO_TARGET
            : !hasProblem
              ? "問題が選ばれていません"
              : null;
  return { label: "提出", disabledReason: reason };
}

const NOT_STOPPED = "プログラムが止まっているときだけ使えます";
const STARTING = "デバッグを始めているところです";

// デバッグ中に並べるボタン（D-146 2.5）。続行と一時停止は 1 つのボタンで、止まっているときは「続行」、
// 動いているときは「一時停止」。ステップの 3 つは止まっているときだけ。停止はいつでも
export function debugButtons(phase: DebugPhase): {
  resume: ButtonView & { action: "continue" | "pause" };
  next: ButtonView;
  stepIn: ButtonView;
  stepOut: ButtonView;
  stop: ButtonView;
} {
  const step = (label: string): ButtonView => ({
    label,
    disabledReason: phase === "stopped" ? null : phase === "starting" ? STARTING : NOT_STOPPED,
  });
  return {
    resume:
      phase === "running"
        ? { label: "一時停止", disabledReason: null, action: "pause" }
        : {
            label: "続行",
            disabledReason: phase === "starting" ? STARTING : null,
            action: "continue",
          },
    next: step("ステップオーバー"),
    stepIn: step("ステップイン"),
    stepOut: step("ステップアウト"),
    stop: { label: "停止", disabledReason: null },
  };
}
