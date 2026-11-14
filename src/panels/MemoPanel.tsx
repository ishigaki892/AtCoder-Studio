// サイドバーの「メモ」（Phase 39。D-263）。上は、問題文の欄に出している問題のメモの欄（問題が変われば、その問題のメモに
// 切り替わる）。書くのが止まってから 1 秒で保存する（ソースの自動保存と同じ。D-037・D-075）。
// 下は、メモのある問題の一覧（最後に書いた日の新しい順）。押すと、その問題を問題文の欄に出す。
// 記録は Rust 側（problem_marks.rs の memo・memoOn）。一覧の場所・diff は、保存してある AtCoder Problems のデータだけで出す
import { useEffect, useRef, useState } from "react";
import type { ProblemId } from "../ipc/problem";
import { memoProblems, MEMO_LIMIT, type ProblemMarks, type RetryRow } from "../ipc/problemMarks";

type Props = {
  // 問題文の欄の問題（なければ書けない）
  current: ProblemId | null;
  // 変わったら一覧を読み直す
  marks: ProblemMarks;
  // サイドバーで「メモ」を出しているか（出したときに一覧を読み直す）
  visible: boolean;
  onSave: (id: string, memo: string) => Promise<void>;
  onSelect: (id: ProblemId) => void;
};

// 保存を待つ時間（ミリ秒）
const SAVE_DELAY = 1000;

// 問題の場所（例：ABC042 C / ARC058 C）。データがなければ問題 ID
function placeText(row: RetryRow): string {
  return row.places.length > 0
    ? row.places.map((place) => `${place.contest.toUpperCase()} ${place.index}`).join(" / ")
    : row.problem;
}

// メモの 1 行目（空の行は飛ばす）
export function firstLine(memo: string | undefined): string {
  return (memo ?? "").split("\n").find((line) => line.trim() !== "")?.trim() ?? "";
}

function MemoPanel({ current, marks, visible, onSave, onSelect }: Props) {
  const task = current?.task ?? null;
  const [text, setText] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [rows, setRows] = useState<RetryRow[]>([]);
  // まだ保存していない書きかけ（問題 ID と中身）。問題を切り替える・閉じるときに先に保存する
  const pending = useRef<{ id: string; memo: string } | null>(null);
  const timer = useRef<number | null>(null);
  const seq = useRef(0);

  async function flush() {
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
      timer.current = null;
    }
    const next = pending.current;
    pending.current = null;
    if (!next) {
      return;
    }
    setSaving(true);
    try {
      await onSave(next.id, next.memo);
      setError(null);
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  // 問題が変わったら、書きかけを先に保存してから、その問題のメモを出す
  useEffect(() => {
    void flush();
    setText(task ? (marks[task]?.memo ?? "") : "");
    setError(null);
    // marks は、問題が変わったときの中身だけを使う（書いている間に、保存の結果で欄を書き換えない）
  }, [task]);

  // 閉じるとき（タブやサイドバーではなく、アプリの画面から外れるとき）も、書きかけを保存する
  useEffect(() => () => void flush(), []);

  function change(value: string) {
    setText(value);
    if (!task) {
      return;
    }
    pending.current = { id: task, memo: value };
    if (timer.current !== null) {
      window.clearTimeout(timer.current);
    }
    timer.current = window.setTimeout(() => void flush(), SAVE_DELAY);
  }

  // 一覧：出したとき・記録が変わったとき
  useEffect(() => {
    if (!visible) {
      return;
    }
    const mine = ++seq.current;
    memoProblems()
      .then((view) => {
        if (mine === seq.current) {
          setRows(view.rows);
        }
      })
      .catch((e) => {
        if (mine === seq.current) {
          setError(String(e));
        }
      });
  }, [visible, marks]);

  const count = [...text].length;
  return (
    <div className="memo-panel">
      <div className="memo-editor">
        <div className="memo-title">
          {task ? <span title={task}>{task} のメモ</span> : <span>メモ</span>}
          <span className={`memo-count ${count > MEMO_LIMIT ? "over" : ""}`}>
            {saving ? "保存しています… " : ""}
            {count.toLocaleString()} / {MEMO_LIMIT.toLocaleString()}
          </span>
        </div>
        {task ? (
          <textarea
            className="memo-text"
            aria-label={`${task} のメモ`}
            placeholder="解き方の要点・はまった所・学んだことなど（書くのが止まってから 1 秒で保存します）"
            value={text}
            onChange={(e) => change(e.target.value)}
            onBlur={() => void flush()}
          />
        ) : (
          <p className="contest-note">問題を表示すると、その問題のメモを書けます</p>
        )}
        {error && (
          <p className="contest-error" role="alert">
            {error}
          </p>
        )}
      </div>
      <div className="memo-list">
        <div className="memo-title">メモのある問題（{rows.length}）</div>
        {rows.length === 0 ? (
          <p className="contest-note">まだメモはありません</p>
        ) : (
          <ul>
            {rows.map((row) => (
              <li key={row.problem} className={row.problem === task ? "current" : ""}>
                <button
                  type="button"
                  title={`${row.problem}\n${marks[row.problem]?.memo ?? ""}`}
                  onClick={() => onSelect({ contest: row.contest, task: row.problem })}
                >
                  <span className="memo-place">{placeText(row)}</span>
                  <span className="memo-date">{row.since ?? "—"}</span>
                  <span className="memo-line">{firstLine(marks[row.problem]?.memo)}</span>
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}

export default MemoPanel;
