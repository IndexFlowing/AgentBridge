import { useState } from "react";
import { FileCode2, X } from "lucide-react";
import { desktopApi, type DashboardData, type Task } from "../ipc";
import { Badge, Card, Confirm, Empty, Status, fmt } from "../ui";
import { t } from "../locale";

export function Tasks({
  data,
  reload,
}: {
  data: DashboardData;
  reload: () => void;
}) {
  const [selected, setSelected] = useState<Task>();
  const [confirm, setConfirm] = useState<Task>();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  async function cancel() {
    if (!confirm) return;
    setBusy(true);
    setError("");
    try {
      await desktopApi.cancelTask(confirm.project, confirm.task_id);
      setConfirm(undefined);
      setSelected(undefined);
      reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="content">
      <div className="intro">
        <div>
          <label>{t("tasks.eyebrow")}</label>
          <h1>{t("tasks.title")}</h1>
          <p>{t("tasks.description")}</p>
        </div>
        <Badge>{data.tasks.length} {t("common.recorded")}</Badge>
      </div>

      <Card>
        {data.tasks.length ? (
          <div className="task-list">
            {data.tasks.map(task => (
              <button
                className="task"
                key={`${task.project}-${task.task_id}`}
                onClick={() => setSelected(task)}
              >
                <div>
                  <b>{task.goal ?? t("common.noData")}</b>
                  <small>
                    {task.project} · 第 {task.iteration} 次 · {t("common.updated")} {fmt(task.updated_at)}
                  </small>
                </div>
                <Status value={task.status ?? task.lifecycle} />
              </button>
            ))}
          </div>
        ) : (
          <Empty text={t("tasks.noTasks")} detail={t("tasks.noTasksDetail")} />
        )}
      </Card>

      {error && (
        <div className="inline-error">
          <X size={15} />
          {error}
        </div>
      )}

      {/* 任务详情查看弹窗 */}
      {selected && (
        <div className="modal-backdrop" onClick={() => setSelected(undefined)}>
          <div
            className="modal task-detail"
            role="dialog"
            aria-modal="true"
            aria-labelledby="task-title"
            onClick={event => event.stopPropagation()}
          >
            <button
              className="close"
              onClick={() => setSelected(undefined)}
              aria-label={t("window.close")}
            >
              <X size={16} />
            </button>
            <label>{t("tasks.detail")}</label>
            <h2 id="task-title">{selected.goal ?? t("common.noData")}</h2>

            <div className="detail-status">
              <Status value={selected.status ?? selected.lifecycle} />
              <span>{t("common.updated")} {fmt(selected.updated_at)}</span>
            </div>

            <div className={`summary ${selected.error ? "has-error" : ""}`}>
              <b>{selected.error ? t("tasks.error") : t("tasks.summary")}</b>
              <p>{selected.error ?? selected.summary ?? t("common.noData")}</p>
            </div>

            <dl>
              <dt>Task ID</dt>
              <dd><code>{selected.task_id ?? t("common.notRecorded")}</code></dd>
              <dt>{t("projects.title")}</dt>
              <dd>{selected.project}</dd>
              <dt>{t("dashboard.executor")}</dt>
              <dd>{selected.executor ?? t("common.notRecorded")}</dd>
              <dt>开始时间</dt>
              <dd>{fmt(selected.started_at)}</dd>
              <dt>结束时间</dt>
              <dd>{fmt(selected.finished_at)}</dd>
            </dl>

            <div className="detail-section">
              <b>
                <FileCode2 size={15} /> {t("tasks.changedFiles")}{" "}
                <span>{selected.changed_files.length}</span>
              </b>
              <code className="file-list">
                {selected.changed_files.length
                  ? selected.changed_files.join("\n")
                  : t("common.noData")}
              </code>
            </div>

            {selected.tests && (
              <div className="detail-section">
                <b>{t("tasks.testResults")}</b>
                <p className="test-result">
                  {selected.tests.command} · {selected.tests.status} · {selected.tests.summary ?? ""}
                </p>
              </div>
            )}

            <button className="button danger full" onClick={() => setConfirm(selected)}>
              {t("tasks.cancel")}
            </button>
          </div>
        </div>
      )}

      {/* 取消任务确认弹窗 */}
      {confirm && (
        <Confirm
          title={t("tasks.cancel")}
          onConfirm={cancel}
          onClose={() => setConfirm(undefined)}
          busy={busy}
        >
          {t("tasks.cancelQuestion")}
        </Confirm>
      )}
    </main>
  );
}