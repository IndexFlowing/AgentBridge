const API_BASE = process.env.NODE_ENV === 'development' ? 'http://127.0.0.1:8030' : '';

export async function fetchApi<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    ...options,
    headers: { 'Content-Type': 'application/json', ...(options?.headers || {}) },
  });
  if (!res.ok) {
    let msg = `HTTP Error ${res.status}`;
    try {
      const err = await res.json();
      msg = err.message || err.error || msg;
    } catch {
      try { msg = await res.text() || msg; } catch {}
    }
    throw new Error(msg);
  }
  if (res.status === 204 || res.headers.get("content-length") === "0") return null as any;
  return res.json();
}

export type Project = { id: string; name: string; path: string; description: string; readonly: boolean; active: boolean; git_repository: boolean; project_type: string[]; executor: string; };
export type Executor = { id: string; name: string; kind: string; command: string; executable?: string; working_directory?: string; enabled: boolean; status: string; detected: boolean; version?: string };
export type ConnectionData = { config_path: string; workspace: string; host: string; port: number; endpoint: string; auth_enabled: boolean; oauth_enabled: boolean; no_auth: boolean; };
export type GatewayStatus = { online: boolean; endpoint: string; host: string; port: number; managed: boolean; };
export type DashboardData = { gateway: GatewayStatus; projects: Project[]; tasks: any[]; executor: string; activity: any[]; };
export type ProxyData = { enabled: boolean; kind: string; host: string; port: number; username_configured: boolean; password_configured: boolean; };

export const api = {
  // System & Dashboard
  getDashboard: () => fetchApi<DashboardData>("/api/system/dashboard"),
  getConnection: () => fetchApi<ConnectionData>("/api/system/connection"),
  saveConnection: (data: any) => fetchApi<ConnectionData>("/api/system/connection", { method: "PUT", body: JSON.stringify(data) }),
  
  // Proxy
  getProxy: () => fetchApi<ProxyData>("/api/proxy"),
  saveProxy: (data: any) => fetchApi<ProxyData>("/api/proxy", { method: "PUT", body: JSON.stringify(data) }),
  testProxy: (data: any) => fetchApi<string>("/api/proxy/test", { method: "POST", body: JSON.stringify(data) }),

  // Projects
  getProjects: async () => {
    const data = await fetchApi<DashboardData>("/api/system/dashboard");
    return data.projects;
  },
  saveProject: (data: any) => fetchApi<Project[]>("/api/projects", { method: "POST", body: JSON.stringify(data) }),
  deleteProject: (id: string) => fetchApi<Project[]>(`/api/projects/${encodeURIComponent(id)}`, { method: "DELETE" }),

  // Executors
  getExecutors: () => fetchApi<Executor[]>("/api/executors"),
  saveExecutor: (data: any) => fetchApi<Executor[]>("/api/executors", { method: "POST", body: JSON.stringify(data) }),
  deleteExecutor: (id: string) => fetchApi<Executor[]>(`/api/executors/${encodeURIComponent(id)}`, { method: "DELETE" }),
  testExecutor: (id: string) => fetchApi<Executor>(`/api/executors/${encodeURIComponent(id)}/test`, { method: "POST" }),
};