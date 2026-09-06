import { useState } from "react";
import type * as React from "react";
import { FolderOpen, Pencil, Plus, Trash2, X } from "lucide-react";
import { desktopApi, type DashboardData } from "../ipc";
import { Badge, Card, Confirm, Empty } from "../ui";
import { t } from "../locale";

export function Projects({
  data,
  reload,
}: {
  data: DashboardData;
  reload: () => void;
}) {
  const [editing, setEditing] = useState<DashboardData["projects"][number]>();
  const [open, setOpen] = useState(false);
  const [confirm, setConfirm] = useState<DashboardData["projects"][number]>();
  const [executors, setExecutors] = useState<{ kind: string; name: string; version?: string }[]>([]);
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");

  const begin = (project?: DashboardData["projects"][number]) => {
    setEditing(project);
    setPath(project?.path ?? "");
    setError("");
    setOpen(true);

    desktopApi.executors().then(list => {
      const candidates = list
        .filter(item => item.available || item.enabled)
        .map(item => ({
          kind: item.kind,
          name: item.name,
          version: item.version,
        }));
      setExecutors(candidates.length ? candidates : [{ kind: "opencode", name: "OpenCode" }]);
    }).catch(e => setError(String(e)));
  };

  async function chooseDirectory() {
    setError("");
    try {
      const selected = await desktopApi.chooseProjectDirectory();
      if (selected) setPath(selected);
    } catch (e) {
      setError(String(e));
    }
  }

  async function save(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    const form = new FormData(event.currentTarget);
    try {
      await desktopApi.saveProject({
        id: editing?.id,
        name: String(form.get("name") ?? ""),
        path,
        executor: String(form.get("executor") ?? ""),
      });
      setOpen(false);
      reload();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function remove() {
    if (!confirm) return;
    setBusy(true);
    setError("");
    try {
      await desktopApi.deleteProject(confirm.id);
      setConfirm(undefined);
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
          <label>{t("projects.eyebrow")}</label>
          <h1>{t("projects.title")}</h1>
          <p>{t("projects.description")}</p>
        </div>
        <button className="button primary" onClick={() => begin()}>
          <Plus size={14} /> {t("projects.add")}
        </button>
      </div>

      <Card>
        {data.projects.length ? (
          <div className="project-table">
            <div className="table-head">
              <span>{t("projects.name")}</span>
              <span>{t("projects.path")}</span>
              <span>{t("projects.executor")}</span>
              <span>{t("projects.actions")}</span>
            </div>
            {data.projects.map(project => (
              <div className="table-row" key={project.id}>
                <b>
                  {project.name}
                  {project.active && <Badge tone="online">{t("projects.default")}</Badge>}
                </b>
                <code>{project.path}</code>
                <span>{project.executor}</span>
                <span className="row-actions">
                  <button
                    className="icon"
                    onClick={() => begin(project)}
                    aria-label={t("projects.edit")}
                  >
                    <Pencil size={14} />
                  </button>
                  <button
                    className="icon"
                    onClick={() => setConfirm(project)}
                    aria-label={t("projects.delete")}
                  >
                    <Trash2 size={14} />
                  </button>
                </span>
              </div>
            ))}
          </div>
        ) : (
          <Empty text={t("projects.noProjects")} />
        )}
        <p className="note">{t("projects.note")}</p>
      </Card>

      {error && (
        <div className="inline-error">
          <X size={15} />
          {error}
        </div>
      )}

      {open && (
        <div className="modal-backdrop" onClick={() => setOpen(false)}>
          <form className="modal" onSubmit={save} onClick={event => event.stopPropagation()}>
            <button
              type="button"
              className="close"
              onClick={() => setOpen(false)}
              aria-label={t("window.close")}
            >
              <X size={16} />
            </button>
            <label>{editing ? t("projects.edit") : t("projects.add")}</label>
            <h2>{editing?.name ?? t("projects.new")}</h2>

            <div className="form">
              <label>
                {t("projects.name")}
                <input
                  name="name"
                  required
                  maxLength={64}
                  defaultValue={editing?.name ?? ""}
                />
              </label>

              <label>
                {t("projects.path")}
                <div className="path-picker">
                  <input
                    name="path"
                    required
                    value={path}
                    onChange={event => setPath(event.target.value)}
                    placeholder="D:\Projects\my-app"
                  />
                  <button
                    type="button"
                    className="button"
                    onClick={chooseDirectory}
                    disabled={busy}
                  >
                    <FolderOpen size={14} /> {t("projects.chooseFolder")}
                  </button>
                </div>
              </label>

              <label>
                {t("projects.executor")}
                <select
                  name="executor"
                  required
                  defaultValue={editing?.executor ?? executors[0]?.kind ?? "opencode"}
                >
                  {executors.map(item => (
                    <option key={item.kind} value={item.kind}>
                      {item.name} {item.version ? `(${item.version})` : `[${item.kind}]`}
                    </option>
                  ))}
                </select>
              </label>

              <div className="form-actions">
                <button type="submit" className="button primary" disabled={busy}>
                  {busy ? t("projects.saving") : t("projects.save")}
                </button>
              </div>
            </div>
          </form>
        </div>
      )}

      {confirm && (
        <Confirm
          title={t("projects.deleteQuestion")}
          onClose={() => setConfirm(undefined)}
          onConfirm={remove}
          busy={busy}
        >
          {t("projects.deleteWarning")} {confirm.name}
        </Confirm>
      )}
    </main>
  );
}