// ボタンゾーン（上のバー）を横にスクロールできるようにする（D-275）
//
// - ホイールを縦に回すと横に送る（横の動きがあるときはそのまま。タブの列と同じ。D-191）
// - スクロールする要素の中では、ボタンの下に出すツールチップ（data-tip の ::after。D-023）が切り取られる。
//   そこで、ツールチップは画面に対して置き（position: fixed。App.css）、その位置（ボタンの下）を CSS の変数で渡す。
//   マウスを乗せたとき・キーボードで選んだとき・スクロールしたときに測り直す
import type { SyntheticEvent, WheelEvent } from "react";

export function scrollToolbarSideways(event: WheelEvent<HTMLElement>) {
  if (Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
    event.currentTarget.scrollLeft += event.deltaY;
  }
}

export function placeToolbarTips(event: SyntheticEvent<HTMLElement>) {
  const width = document.documentElement.clientWidth;
  for (const element of event.currentTarget.querySelectorAll<HTMLElement>("[data-tip]")) {
    const rect = element.getBoundingClientRect();
    element.style.setProperty("--tip-top", `${rect.bottom + 6}px`);
    element.style.setProperty("--tip-left", `${rect.left}px`);
    element.style.setProperty("--tip-right", `${width - rect.right}px`);
  }
}
