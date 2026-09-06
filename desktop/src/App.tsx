import { useEffect, useState, type MouseEvent } from "react";
import { Activity, CircleHelp, Cpu, FolderKanban, LayoutDashboard, ListTodo, Minimize2, PlugZap, RefreshCw, Search, Settings, Sparkles, Square, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { desktopApi, type DashboardData, type ConnectionData, type Executor, type Page, type ProxyData } from "./ipc";
import { ActivityPage, Connection, Dashboard, Projects, SettingsPage, Tasks } from "./pages";
import { ExecutorPage } from "./executors-page";
import { ErrorState, Loading } from "./ui";
import { t } from "./locale";
import packageJson from "../package.json";

const appVersion = `v${packageJson.version}`;

const nav: { label: Page; icon: typeof Activity; key: string }[] = [
  { label: "Dashboard", key: "nav.dashboard", icon: LayoutDashboard },
  { label: "Projects", key: "nav.projects", icon: FolderKanban },
  { label: "Executors", key: "nav.executors", icon: Cpu },
  { label: "Tasks", key: "nav.tasks", icon: ListTodo },
  { label: "Activity", key: "nav.activity", icon: Activity },
  { label: "Connection", key: "nav.connection", icon: PlugZap },
  { label: "Settings", key: "nav.settings", icon: Settings },
];

function Titlebar() {
  const window = getCurrentWindow();
  const runWindowAction = (action: () => Promise<void>, name: string) => {
    void action().catch(error => console.error(`[titlebar] failed to ${name}`, error));
  };
  const drag = (event: MouseEvent<HTMLElement>) => {
    if (event.button !== 0 || (event.target instanceof Element && event.target.closest("button"))) return;
    runWindowAction(() => window.startDragging(), "start dragging");
  };
  return (
    <header className="titlebar" data-tauri-drag-region onMouseDown={drag}>
      <div className="titlebar-name" data-tauri-drag-region>
        <span><Sparkles size={13} /></span>
        <b>AgentBridge</b>
      </div>
      <div className="window-actions">
        <button onClick={() => runWindowAction(() => window.minimize(), "minimize window")} aria-label={t("window.minimize")}>
          <Minimize2 size={13} />
        </button>
        <button onClick={() => runWindowAction(() => window.toggleMaximize(), "toggle window maximize")} aria-label={t("window.maximize")}>
          <Square size={12} />
        </button>
        <button className="window-close" onClick={() => runWindowAction(() => window.close(), "close window")} aria-label={t("window.close")}>
          <X size={14} />
        </button>
      </div>
    </header>
  );
}

function Sidebar({ page, setPage, tasks }: { page: Page; setPage: (p: Page) => void; tasks: number }) {
  return (
    <aside className="sidebar">
      <div className="brand">
        <span><Sparkles size={17} /></span>
        <b>AgentBridge<small>{t("app.subtitle")}</small></b>
      </div>
      <div className="workspace">
        <strong>{t("workspace.local")}</strong>
        <small>{t("workspace.state")}</small>
      </div>
      <nav>
        {nav.map(({ label, key, icon: Icon }) => (
          <button className={page === label ? "active" : ""} onClick={() => setPage(label)} key={label}>
            <Icon size={16} />
            {t(key)}
            {label === "Tasks" && tasks > 0 && <em>{tasks}</em>}
          </button>
        ))}
      </nav>
      <div className="sidebar-foot">
        <span><CircleHelp size={15} /> {t("nav.documentation")}</span>
        <small>{t("nav.local")}</small>
        <b className="app-version">AgentBridge {appVersion}</b>
      </div>
    </aside>
  );
}

function Topbar({ page, refresh }: { page: Page; refresh: () => void }) {
  const current = nav.find(item => item.label === page);
  return (
    <header className="topbar">
      <span>{t("topbar.workspace")} / <b>{current ? t(current.key) : page}</b></span>
      <div>
        <button className="search"><Search size={14} />{t("topbar.search")} <kbd>Ctrl K</kbd></button>
        <button className="icon" onClick={refresh} aria-label={t("topbar.refresh")}><RefreshCw size={16} /></button>
      </div>
    </header>
  );
}

export default function App() {
  const [page, setPage] = useState<Page>("Dashboard");
  const [data, setData] = useState<DashboardData>();
  const [connection, setConnection] = useState<ConnectionData>();
  const [settings, setSettings] = useState<Record<string, unknown>>();
  const [proxy, setProxy] = useState<ProxyData>();
  const [executors, setExecutors] = useState<Executor[]>([]);
  const [error, setError] = useState("");

  const load = () => {
    setError("");
    Promise.all([
      desktopApi.dashboard(),
      desktopApi.connection(),
      desktopApi.settings(),
      desktopApi.proxy(),
      desktopApi.executors(),
    ])
      .then(([d, c, s, p, e]) => {
        setData(d);
        setConnection(c);
        setSettings(s);
        setProxy(p);
        setExecutors(e);
      })
      .catch(e => setError(String(e)));
  };

  useEffect(() => {
    load();
    const onKey = (e: KeyboardEvent) => {
      if (e.key.toLowerCase() === "r" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        load();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <div className="app">
      <Titlebar />
      {error ? (
        <>
          <Sidebar page={page} setPage={setPage} tasks={0} />
          <ErrorState error={error} retry={load} />
        </>
      ) : !data ? (
        <div className="loading-pane"><Loading /></div>
      ) : (
        <>
          <Sidebar page={page} setPage={setPage} tasks={data.tasks.filter(t => t.lifecycle === "running").length} />
          <div className="shell">
            <Topbar page={page} refresh={load} />
            {page === "Dashboard" && <Dashboard data={data} setPage={setPage} reload={load} />}
            {page === "Projects" && <Projects data={data} reload={load} />}
            {page === "Executors" && <ExecutorPage value={executors} proxy={proxy} reload={load} />}
            {page === "Tasks" && <Tasks data={data} reload={load} />}
            {page === "Activity" && <ActivityPage data={data} />}
            {page === "Connection" && <Connection value={connection} settings={settings} reload={load} />}
            {page === "Settings" && <SettingsPage value={settings} proxy={proxy} reload={load} />}
          </div>
        </>
      )}
    </div>
  );
}