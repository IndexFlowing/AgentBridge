import { type DashboardData } from "../ipc";
import { Badge, Card, Empty, fmt } from "../ui";
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
                <div>
                  <b>{item.status ?? t("common.updated")} · {item.project}</b>
                  <small>{item.summary ?? t("common.stateChanged")}</small>
                  <time>{fmt(item.timestamp)}</time>
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