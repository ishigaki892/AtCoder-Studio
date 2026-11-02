// AtCoder のレートの色（400 ごと。灰・茶・緑・水・青・黄・橙・赤。2800 以上は赤）の class（D-230）。
// 色は App.css の .rating-*（tokens.css の --rating-*。暗い背景で読めるように明るくした色）。
// サイドバーの「コンテスト」と、問題文の欄の diff（Phase 24。D-241）で使う
const RATING_COLORS = ["gray", "brown", "green", "cyan", "blue", "yellow", "orange", "red"];

export function ratingClass(value: number): string {
  const band = Math.min(Math.max(Math.floor(value / 400), 0), RATING_COLORS.length - 1);
  return `rating-${RATING_COLORS[band]}`;
}
