// 問題文の区画（Phase 32。D-256）。Rust 側（problem.rs の sections）と同じ見分け方：
// 見出し（h3）ごとに、その h3 の親の要素が、その h3 だけを含むなら、それを区画とする。
// 今の問題と abc020・abc041 の形は section、abc001 の形（div.part の中に h3 と section が並ぶ）は div になる。
// 同じ親に h3 が並ぶときは、その区画は返さない
export function statementSections(container: HTMLElement): { heading: HTMLHeadingElement; section: Element }[] {
  const found: { heading: HTMLHeadingElement; section: Element }[] = [];
  for (const heading of container.querySelectorAll("h3")) {
    const section = heading.parentElement;
    if (section && section.querySelectorAll("h3").length === 1) {
      found.push({ heading, section });
    }
  }
  return found;
}
