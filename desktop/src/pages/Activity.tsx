import { type DashboardData } from "../ipc";
import { Badge, Card, Empty, Status, fmt } from "../ui";
import { t } from "../locale";

export function ActivityPage({ data }: { data: DashboardData }) {
  return (
    <main className="content">
      <div className="intro">
        <div>
          <label>{t("activity.eyebrow")}</label>
          <h1>{t("activity.title")}</h1>
          <p>{t("activity.description")}</p>
        </div>
        <Badge>{t("common.refreshBased")}</Badge>
      </div>

      <Card>
        {data.activity.length ? (
          <div className="activity large">
            {data.activity.map((item, i) => (
              <div key={`${item.timestamp}-${i}`}>
                <span />
                <div style={{ display: "flex", flexDirection: "column", gap: "4px" }}>
                  {/* 标题：状态徽标 + 项目名称 + 任务目标 */}
                  <div style={{ display: "flex", alignItems: "center", gap: "8px", flexWrap: "wrap" }}>
                    <Status value={item.status ?? undefined} />
                    <b style={{ color: "#ffffff" }}>{item.project}</b>
                    {item.goal && (
                      <span style={{ color: "#c5d1de", fontSize: "13px" }}>
                        — {item.goal}
                      </span>
                    )}
                  </div>

                  {/* 副标题：执行细节说明 */}
                  <small style={{ color: "#9aa6b2", fontSize: "12px", lineHeight: "1.4" }}>
                    {item.summary}
                  </small>

                  {/* 发生时间 */}
                  <time style={{ fontSize: "11px", color: "#7f8b98" }}>
                    {fmt(item.timestamp)}
                  </time>
                </div>
              </div>
            ))}
          </div>
        ) : (
          <Empty text={t("activity.none")} />
        )}
      </Card>
    </main>
  );
}