import { useEffect, useState } from "react";
import { desktopApi, type ProxyData } from "../ipc";
import { Badge, Card } from "../ui";
import { t } from "../locale";

export function SettingsPage({
  value,
  proxy,
  reload,
}: {
  value?: Record<string, unknown>;
  proxy?: ProxyData;
  reload: () => void;
}) {
  const [form, setForm] = useState({
    enabled: proxy?.enabled ?? false,
    kind: proxy?.kind ?? "http",
    host: proxy?.host ?? "127.0.0.1",
    port: proxy?.port ?? 7897,
    username: "",
    password: "",
  });

  const [saved, setSaved] = useState(false);
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<{ ok: boolean; message: string } | null>(null);

  useEffect(() => {
    if (proxy) {
      setForm(current => ({
        ...current,
        enabled: proxy.enabled,
        kind: proxy.kind,
        host: proxy.host || "127.0.0.1",
        port: proxy.port || 7897,
      }));
    }
  }, [proxy]);

  async function save() {
    setSaved(false);
    setTestResult(null);
    try {
      await desktopApi.saveProxy({
        ...form,
        port: Number(form.port),
        username: form.username || undefined,
        password: form.password || undefined,
      });
      setSaved(true);
      setTimeout(() => setSaved(false), 2500);
      reload();
    } catch (e) {
      setTestResult({ ok: false, message: String(e) });
    }
  }

  async function test() {
    setTesting(true);
    setTestResult(null);
    try {
      const result = await desktopApi.testProxy({
        ...form,
        enabled: true,
        port: Number(form.port),
        username: form.username || undefined,
        password: form.password || undefined,
      });
      setTestResult({ ok: true, message: result });
    } catch (e) {
      setTestResult({ ok: false, message: String(e) });
    } finally {
      setTesting(false);
    }
  }

  return (
    <main className="content">
      <div className="intro">
        <div>
          <label>{t("settings.eyebrow")}</label>
          <h1>{t("settings.title")}</h1>
          <p>{t("settings.description")}</p>
        </div>
      </div>

      <Card title={t("proxy.title")}>
        <div className="form">
          <label className="check">
            <input
              type="checkbox"
              checked={form.enabled}
              onChange={e => setForm({ ...form, enabled: e.target.checked })}
            />
            {t("proxy.enabled")}
          </label>

          <div className="form-grid">
            <label>
              {t("proxy.type")}
              <select
                value={form.kind}
                onChange={e => setForm({ ...form, kind: e.target.value as typeof form.kind })}
              >
                <option value="http">HTTP</option>
                <option value="https">HTTPS</option>
                <option value="socks5">SOCKS5</option>
              </select>
            </label>
            <label>
              端口
              <input
                type="number"
                min="1"
                max="65535"
                value={form.port || ""}
                onChange={e => setForm({ ...form, port: Number(e.target.value) })}
                placeholder="7897"
              />
            </label>
          </div>

          <label>
            {t("proxy.host")}
            <input
              value={form.host}
              onChange={e => setForm({ ...form, host: e.target.value })}
              placeholder="127.0.0.1"
            />
          </label>

          <div className="form-grid">
            <label>
              {t("proxy.username")}
              <input
                value={form.username}
                onChange={e => setForm({ ...form, username: e.target.value })}
                placeholder={proxy?.username_configured ? t("proxy.configured") : ""}
              />
            </label>
            <label>
              {t("proxy.password")}
              <input
                type="password"
                value={form.password}
                onChange={e => setForm({ ...form, password: e.target.value })}
                placeholder={proxy?.password_configured ? t("proxy.configured") : ""}
              />
            </label>
          </div>

          <p className="note">{t("proxy.note")}</p>

          <div className="form-actions" style={{ display: "flex", alignItems: "center", gap: "10px", flexWrap: "wrap" }}>
            <button className="button" onClick={test} disabled={testing}>
              {testing ? t("proxy.testing") : t("proxy.test")}
            </button>
            <button className="button primary" onClick={save}>
              {t("proxy.save")}
            </button>
            {saved && <span style={{ color: "#3dd6c6", fontSize: "14px", fontWeight: 600 }}>✓ {t("proxy.saved")}</span>}
          </div>

          {testResult && (
            <div
              style={{
                marginTop: "12px",
                padding: "10px 14px",
                borderRadius: "8px",
                fontSize: "13px",
                display: "flex",
                alignItems: "center",
                gap: "8px",
                background: testResult.ok ? "#06231f" : "#2a1515",
                border: `1px solid ${testResult.ok ? "#1b4d45" : "#5c2b2b"}`,
                color: testResult.ok ? "#3dd6c6" : "#ff8d8d",
              }}
            >
              {testResult.ok ? "✓" : "✕"} {testResult.message}
            </div>
          )}
        </div>
      </Card>

      <Card title={t("settings.behavior")}>
        <div className="settings">
          <div>
            <b>{t("settings.autoStart")}</b>
            <small>{t("settings.loadedFrom")}: {String(value?.auto_start ?? t("common.notRecorded"))}</small>
          </div>
          <Badge tone="online">{t("settings.supported")}</Badge>
          <div>
            <b>{t("settings.notifications")}</b>
            <small>{t("settings.notImplemented")}</small>
          </div>
          <Badge>{t("settings.comingSoon")}</Badge>
        </div>
      </Card>
    </main>
  );
}