import { useEffect, useState } from "react";
import { X } from "lucide-react";
import { desktopApi, type ConnectionData } from "../ipc";
import { Card, CopyButton, SaveNotice } from "../ui";
import { t } from "../locale";

export function Connection({
  value,
  settings,
  reload,
}: {
  value?: ConnectionData;
  settings?: Record<string, unknown>;
  reload: () => void;
}) {
  const [form, setForm] = useState({
    host: value?.host ?? "",
    port: value?.port ?? 0,
    no_auth: value?.no_auth ?? false,
    auth_token: "",
    admin_password: "",
    executor_command: String(settings?.executor_command ?? "opencode"),
    executor_mode: String(settings?.executor_mode ?? "stream"),
  });

  const [saved, setSaved] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (value) {
      setForm(current => ({
        ...current,
        host: value.host,
        port: value.port,
        no_auth: value.no_auth,
        executor_command: String(settings?.executor_command ?? current.executor_command),
        executor_mode: String(settings?.executor_mode ?? current.executor_mode),
      }));
    }
  }, [value, settings]);

  if (!value) return null;

  async function save() {
    setError("");
    setSaved(false);
    try {
      await desktopApi.saveConnection({
        ...form,
        port: Number(form.port),
        auth_token: form.auth_token || undefined,
        admin_password: form.admin_password || undefined,
      });
      setSaved(true);
      reload();
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <main className="content">
      <div className="intro">
        <div>
          <label>{t("connection.eyebrow")}</label>
          <h1>{t("connection.title")}</h1>
          <p>{t("connection.description")}</p>
        </div>
      </div>

      <Card title={t("connection.gateway")}>
        <div className="form">
          <div className="form-grid">
            <label>
              {t("connection.host")}
              <input
                value={form.host}
                onChange={event => setForm({ ...form, host: event.target.value })}
              />
            </label>
            <label>
              {t("connection.port")}
              <input
                type="number"
                min="1"
                max="65535"
                value={form.port || ""}
                onChange={event => setForm({ ...form, port: Number(event.target.value) })}
              />
            </label>
          </div>

          <label className="check">
            <input
              type="checkbox"
              checked={form.no_auth}
              onChange={event => setForm({ ...form, no_auth: event.target.checked })}
            />{" "}
            {t("connection.disableAuth")}
          </label>

          <p className="note">
            {t("connection.endpoint")}: <code>{value.endpoint}</code>{" "}
            <CopyButton value={value.endpoint} />
            <br />
            {t("connection.config")}: <code>{value.config_path}</code>
          </p>

          <div className="form-actions">
            <button className="button primary" onClick={save}>
              {t("connection.save")}
            </button>
            {saved && <SaveNotice />}
          </div>

          {error && (
            <div className="inline-error">
              <X size={15} />
              {error}
            </div>
          )}
        </div>
      </Card>

      <Card title={t("connection.authentication")}>
        <div className="form">
          <label>
            {t("connection.token")}
            <input
              type="password"
              autoComplete="new-password"
              placeholder={value.auth_enabled ? t("connection.configured") : t("connection.notConfigured")}
              value={form.auth_token}
              onChange={event => setForm({ ...form, auth_token: event.target.value })}
            />
          </label>

          <label>
            {t("connection.password")}
            <input
              type="password"
              autoComplete="new-password"
              placeholder={value.oauth_enabled ? t("connection.configured") : t("connection.notConfigured")}
              value={form.admin_password}
              onChange={event => setForm({ ...form, admin_password: event.target.value })}
            />
          </label>

          <p className="note">{t("connection.secretNote")}</p>
        </div>
      </Card>
    </main>
  );
}