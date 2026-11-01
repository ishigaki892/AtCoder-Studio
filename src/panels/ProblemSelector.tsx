import { useEffect, useState, type FormEvent } from "react";
import { loadTaskList, type ProblemId, type TaskEntry } from "../ipc/problem";

// コンテスト・問題の選択欄（D-115・D-117）。ボタンゾーンの左端に置く
type Props = {
  // 今の問題
  current: ProblemId | null;
  // パッケージのフォルダ名から決まっている（選び直せない）
  fromFolder: boolean;
  // 「紐付けを外す」「フォルダの問題に戻す」（D-178）。フォルダから問題が決まっているときだけ、どちらかを渡す
  onUnlink?: () => void;
  onRelink?: () => void;
  onChoose: (id: ProblemId) => void;
  // 「一覧」を押した（読めたかどうかによらない）。開始前のコンテストの残り時間に使う（D-238）
  onList?: (contest: string) => void;
  onError: (message: string) => void;
};

function ProblemSelector({ current, fromFolder, onUnlink, onRelink, onChoose, onList, onError }: Props) {
  const [contest, setContest] = useState(current?.contest ?? "");
  // 読み込んだ一覧と、そのコンテスト ID
  const [tasks, setTasks] = useState<{ contest: string; list: TaskEntry[] } | null>(null);
  const [loading, setLoading] = useState(false);

  // 問題が変わったら、コンテスト ID の欄をそれに合わせる
  useEffect(() => {
    if (current) {
      setContest(current.contest);
    }
  }, [current?.contest]);

  // コンテスト ID を入れて「一覧」を押したら、問題の一覧を読む（保存してあれば取得しない。D-111）
  async function submit(event: FormEvent) {
    event.preventDefault();
    const id = contest.trim();
    if (!id) {
      return;
    }
    onList?.(id);
    setLoading(true);
    try {
      setTasks({ contest: id, list: await loadTaskList(id, false) });
    } catch (e) {
      setTasks(null);
      onError(String(e));
    } finally {
      setLoading(false);
    }
  }

  const disabledReason = fromFolder ? "パッケージのフォルダ名から問題を決めています" : undefined;
  const unlinkTip = "フォルダの問題から外して、ほかの問題を選べるようにします（パッケージを切り替えると戻ります）";
  const listed =
    tasks !== null &&
    current !== null &&
    current.contest === tasks.contest &&
    tasks.list.some((t) => t.task === current.task);

  return (
    <form className="problem-selector" onSubmit={(event) => void submit(event)}>
      <span className="run-button-wrap" data-tip={disabledReason}>
        <input
          className="problem-contest"
          value={contest}
          placeholder="コンテスト ID（例：abc430）"
          aria-label="コンテスト ID"
          disabled={fromFolder}
          onChange={(event) => setContest(event.target.value)}
        />
      </span>
      <button type="submit" className="run-button" disabled={fromFolder || loading}>
        {loading ? "読み込み中…" : "一覧"}
      </button>
      {onUnlink && (
        <span className="run-button-wrap" data-tip={unlinkTip}>
          <button type="button" className="run-button" onClick={onUnlink}>
            紐付けを外す
          </button>
        </span>
      )}
      {onRelink && (
        <button type="button" className="run-button" onClick={onRelink}>
          フォルダの問題に戻す
        </button>
      )}
      {tasks && !fromFolder && (
        <select
          className="problem-task"
          aria-label="問題"
          value={listed && current ? current.task : ""}
          onChange={(event) => {
            if (event.target.value) {
              onChoose({ contest: tasks.contest, task: event.target.value });
            }
          }}
        >
          <option value="">問題を選ぶ</option>
          {tasks.list.map((task) => (
            <option key={task.task} value={task.task}>
              {task.label} - {task.title}
            </option>
          ))}
        </select>
      )}
    </form>
  );
}

export default ProblemSelector;
