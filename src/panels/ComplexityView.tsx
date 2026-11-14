// 「テスト結果」の計算量の表示（Phase 45。D-270）。段ごとの表（割合・入力の大きさ・時間・メモリ）、伸び方の指数と見当、
// 時間の制限・メモリの制限との比べ、両対数の小さなグラフ（SVG を手で描く。ライブラリは使わない。D-031）
import type { ComplexityStep, Measured } from "../ipc/run";

// 割合の見せ方（1/128 … 1/2・1）
function scaleText(scale: number): string {
  return scale >= 1 ? "1" : `1/${Math.round(1 / scale)}`;
}

function bytesText(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${bytes} B`;
}

function msText(ms: number): string {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)} 秒` : `${ms} ms`;
}

const WIDTH = 360;
const HEIGHT = 180;
const PAD = { left: 48, right: 12, top: 10, bottom: 28 };

// 両対数のグラフ：横は割合（log2）、縦は時間（log10）。点と、当てはめた直線（割合 1 まで延ばす）と、時間の制限の線
function Chart({ steps, exponent, timeLimitMs }: { steps: ComplexityStep[]; exponent: number | null; timeLimitMs: number }) {
  const points = steps.filter((step) => !step.timedOut);
  if (points.length === 0) return null;
  const xs = { min: Math.log2(1 / 128), max: 0 };
  const values = points.map((step) => Math.max(1, step.medianMs));
  const top = Math.max(...values, timeLimitMs);
  const ys = { min: Math.log10(Math.max(1, Math.min(...values) / 2)), max: Math.log10(top * 1.5) };
  const x = (scale: number) =>
    PAD.left + ((Math.log2(scale) - xs.min) / (xs.max - xs.min)) * (WIDTH - PAD.left - PAD.right);
  const y = (ms: number) =>
    HEIGHT - PAD.bottom - ((Math.log10(Math.max(1, ms)) - ys.min) / (ys.max - ys.min)) * (HEIGHT - PAD.top - PAD.bottom);
  // 当てはめた直線：最後に測れた段を通り、傾きが指数
  const last = points[points.length - 1];
  const line =
    exponent === null
      ? null
      : {
          from: { scale: 1 / 128, ms: last.medianMs * Math.pow(1 / 128 / last.scale, exponent) },
          to: { scale: 1, ms: last.medianMs * Math.pow(1 / last.scale, exponent) },
        };
  const ticks = [1 / 128, 1 / 32, 1 / 8, 1 / 2, 1];
  return (
    <svg
      className="complexity-chart"
      width={WIDTH}
      height={HEIGHT}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      role="img"
      aria-label="大きさの割合と時間の両対数のグラフ"
    >
      <line className="complexity-axis" x1={PAD.left} y1={HEIGHT - PAD.bottom} x2={WIDTH - PAD.right} y2={HEIGHT - PAD.bottom} />
      <line className="complexity-axis" x1={PAD.left} y1={PAD.top} x2={PAD.left} y2={HEIGHT - PAD.bottom} />
      {ticks.map((tick) => (
        <text key={tick} className="complexity-tick" x={x(tick)} y={HEIGHT - 10} textAnchor="middle">
          {scaleText(tick)}
        </text>
      ))}
      <text className="complexity-tick" x={PAD.left - 4} y={y(top) + 4} textAnchor="end">
        {msText(Math.round(top))}
      </text>
      <text className="complexity-tick" x={PAD.left - 4} y={HEIGHT - PAD.bottom} textAnchor="end">
        {msText(Math.round(Math.pow(10, ys.min)))}
      </text>
      {/* 時間の制限 */}
      <line
        className="complexity-limit"
        x1={PAD.left}
        y1={y(timeLimitMs)}
        x2={WIDTH - PAD.right}
        y2={y(timeLimitMs)}
      />
      {line && (
        <line
          className="complexity-fit"
          x1={x(line.from.scale)}
          y1={y(line.from.ms)}
          x2={x(line.to.scale)}
          y2={y(line.to.ms)}
        />
      )}
      {points.map((step) => (
        <circle key={step.scale} className="complexity-point" cx={x(step.scale)} cy={y(step.medianMs)} r={3}>
          <title>{`割合 ${scaleText(step.scale)}：${msText(step.medianMs)}`}</title>
        </circle>
      ))}
    </svg>
  );
}

function ComplexityView({
  steps,
  result,
  summary,
}: {
  steps: ComplexityStep[];
  result: Measured | null;
  summary: string | null;
}) {
  const shown = result?.steps ?? steps;
  return (
    <div className="complexity-view">
      <div className="test-note">
        生成器に大きさの割合（1/128〜1）を渡して作った入力で、解答だけを各段 3 回動かした時間（真ん中の値）とメモリです。手元の時間なので、AtCoder の時間とは違います。
      </div>
      {summary && <div className="test-summary">{summary}</div>}
      {result && (
        <div className="complexity-result">
          {result.exponent !== null && (
            <div>
              伸び方：指数 <strong>{result.exponent.toFixed(2)}</strong>（{result.shape}。入力を 2 倍にすると、時間はおよそ{" "}
              {Math.pow(2, result.exponent).toFixed(1)} 倍。N と N log N は、この方法ではほとんど見分けられません）
            </div>
          )}
          {result.fullMs !== null && (
            <div className={result.fullMs > result.timeLimitMs ? "complexity-over" : undefined}>
              最大の大きさ（割合 1）：{result.estimated ? "約 " : ""}
              {msText(result.fullMs)}
              {result.estimated ? "（伸び方からの見積もり）" : ""}・時間の制限 {msText(result.timeLimitMs)}（compare.json）
              {result.fullMs > result.timeLimitMs ? "。制限を超えそうです" : ""}
            </div>
          )}
          {result.memoryKib !== null && (
            <div className={result.memoryKib > result.memoryLimitMib * 1024 ? "complexity-over" : undefined}>
              メモリ：{(result.memoryKib / 1024).toFixed(1)} MiB・メモリの制限 {result.memoryLimitMib} MiB
            </div>
          )}
          {result.note && <div className="test-note">{result.note}</div>}
        </div>
      )}
      {shown.length > 0 && (
        <div className="complexity-body">
          <table className="complexity-table">
            <thead>
              <tr>
                <th>割合</th>
                <th>入力</th>
                <th>時間</th>
                <th>メモリ</th>
              </tr>
            </thead>
            <tbody>
              {shown.map((step) => (
                <tr key={step.scale}>
                  <td>{scaleText(step.scale)}</td>
                  <td>{bytesText(step.inputBytes)}</td>
                  <td title={step.timesMs.map((ms) => `${ms} ms`).join("・")}>
                    {step.timedOut ? `${msText(step.medianMs)} より長い（止めました）` : msText(step.medianMs)}
                  </td>
                  <td>{step.memoryKib === null ? "—" : `${(step.memoryKib / 1024).toFixed(1)} MiB`}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <Chart steps={shown} exponent={result?.exponent ?? null} timeLimitMs={result?.timeLimitMs ?? 2500} />
        </div>
      )}
    </div>
  );
}

export default ComplexityView;
