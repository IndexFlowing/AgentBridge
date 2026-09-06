import { useState } from "react";
import { Check, Wifi } from "lucide-react";
import { desktopApi, type DashboardData, type Page } from "../ipc";
import { Badge, Card, Empty, Switch, fmt } from "../ui";
import { t } from "../locale";

export function Dashboard({
  data,
  setPage,
  reload,
}: {
  data: DashboardData;
  setPage: (p: Page) => void;
  reload?: () => void;
}) {
  const running = data.tasks.filter(task => task.lifecycle === "running").length;
  const [busy, setBusy] = useState(false);

  async function handleToggle(nextState: boolean) {
    setBusy(true);
    try {
      if (nextState) {
        await desktopApi.startGateway();
      } else {
        await desktopApi.stopGateway();
      }
      reload?.();
    } catch (err) {
      console.error("failed to toggle gateway", err);
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="content">
      {/* 1. 页面头部介绍与 Clash 同款网关总开关 */}
      <div className="intro">
        <div>
          <label>{t("dashboard.eyebrow")}</label>
          <h1>{t("dashboard.title")}</h1>
          <p>{t("dashboard.description")}</p>
        </div>

        <div style={{ background: "#1a222c", padding: "10px 16px", borderRadius: "12px", border: "1px solid #2c3846", display: "flex", alignItems: "center" }}>
          <Switch
            checked={data.gateway.online}
            onChange={handleToggle}
            disabled={busy}
            label={busy ? "切换中..." : data.gateway.online ? "网关服务运行中" : "网关服务已关闭"}
          />
        </div>
      </div>

      {/* 2. 状态统计卡片 */}
      <div className="stats">
        <Card>
          <label>{t("dashboard.gateway")}</label>
          <strong>{data.gateway.online ? t("common.online") : t("common.offline")}</strong>
          <small>{data.gateway.endpoint}</small>
        </Card>
        <Card>
          <label>{t("dashboard.projects")}</label>
          <strong>{data.projects.length}</strong>
          <small>{t("dashboard.workspaces")}</small>
        </Card>
        <Card>
          <label>{t("dashboard.tasks")}</label>
          <strong>{data.tasks.length}</strong>
          <small>{running} {t("dashboard.running")}</small>
        </Card>
        <Card>
          <label>{t("dashboard.executor")}</label>
          <strong>{data.executor}</strong>
          <small>{t("dashboard.adapter")}</small>
        </Card>
      </div>

      {/* 3. 主体分栏 */}
      <div className="columns">
        <div>
          <Card title={t("dashboard.lifecycle")}>
            <div className="lifecycle">
              {["PLAN", "EXECUTE", "TEST", "REVIEW"].map((step, i) => (
                <div
                  key={step}
                  className={i === 0 ? "done" : i === 1 && running ? "current" : ""}
                >
                  <span>{i === 0 ? <Check size={15} /> : i + 1}</span>
                  <b>{step}</b>
                  <small>
                    {i === 0
                      ? t("dashboard.stateRecorded")
                      : i === 1 && running
                      ? t("dashboard.inProgress")
                      : t("dashboard.awaiting")}
                  </small>
                </div>
              ))}
            </div>
          </Card>

          <Card
            title={t("dashboard.mounted")}
            action={
              <button className="text" onClick={() => setPage("Projects")}>
                {t("dashboard.viewAll")}
              </button>
            }
          >
            {data.projects.length ? (
              <div className="rows">
                {data.projects.map(project => (
                  <div className="row" key={project.name}>
                    <span className="avatar">{project.name[0]}</span>
                    <div>
                      <b>{project.name}</b>
                      <small>{project.path}</small>
                    </div>
                    <Badge tone={project.git_repository ? "online" : "neutral"}>
                      {project.git_repository ? "Git" : t("dashboard.noGit")}
                    </Badge>
                  </div>
                ))}
              </div>
            ) : (
              <Empty text={t("dashboard.noProjects")} detail={t("dashboard.addWorkspace")} />
            )}
          </Card>
        </div>

        <div>
          <Card title={t("dashboard.gatewayStatus")}>
            <div className="gateway">
              <Wifi size={30} color={data.gateway.online ? "#3dd6c6" : "#ff8d8d"} />
              <b>{data.gateway.online ? "就绪 · 等待 AI 接入" : "网关服务已关闭"}</b>
              <small>监听端点: {data.gateway.host}:{data.gateway.port}/mcp</small>
            </div>

            {/* 实时连接的 AI 客户端列表 */}
            {data.gateway.clients && data.gateway.clients.length > 0 && (
              <div style={{ marginTop: "12px", borderTop: "1px solid #242e3a", paddingTop: "10px" }}>
                <div style={{ fontSize: "12px", color: "#9aa6b2", marginBottom: "6px" }}>当前活跃连接:</div>
                {data.gateway.clients.map(client => (
                  <div key={client.client_id} style={{ display: "flex", justifyContent: "space-between", alignItems: "center", fontSize: "12px", padding: "4px 0" }}>
                    <span style={{ color: "#3dd6c6", fontWeight: 600 }}>● {client.client_name}</span>
                    <code style={{ fontSize: "11px", color: "#7f8b98" }} title={client.client_id}>
                      {client.client_id.slice(0, 16)}...
                    </code>
                  </div>
                ))}
              </div>
            )}
          </Card>

          <Card
            title={t("dashboard.recent")}
            action={
              <button className="text" onClick={() => setPage("Activity")}>
                {t("dashboard.openActivity")}
              </button>
            }
          >
            {data.activity.length ? (
              <div className="activity">
                {data.activity.slice(0, 4).map((item, i) => (
                  <div key={`${item.timestamp}-${i}`}>
                    <span />
                    <div>
                      <b>{item.status ?? t("common.updated")} · {item.project}</b>
                      <small>{item.summary ?? t("common.stateChanged")}</small>
                      <time>{fmt(item.timestamp)}</time>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <Empty text={t("dashboard.noActivity")} />
            )}
          </Card>
        </div>
      </div>
    </main>
  );
}