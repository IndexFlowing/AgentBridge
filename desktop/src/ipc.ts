import { invoke } from "@tauri-apps/api/core";
export type Page = "Dashboard" | "Projects" | "Executors" | "Tasks" | "Activity" | "Connection" | "Settings";

export type Project = { id: string; name: string; path: string; description: string; readonly: boolean; active: boolean; git_repository: boolean; project_type: string[]; executor: string };
export type Task = { project: string; task_id?: string; lifecycle?: string; status?: string; iteration: number; executor?: string; goal?: string; summary?: string; error?: string; changed_files: string[]; tests?: { status: string; command: string; exit_code?: number; summary?: string; timestamp: string }; created_at?: string; started_at?: string; finished_at?: string; updated_at: string };
export type DashboardData = { gateway: { online: boolean; endpoint: string; host: string; port: number }; projects: Project[]; tasks: Task[]; executor: string; activity: { project: string; task_id?: string; status?: string; summary?: string; timestamp: string }[] };
export type ConnectionData = { config_path: string; workspace: string; host: string; port: number; endpoint: string; auth_enabled: boolean; oauth_enabled: boolean; no_auth: boolean };
export type ProxyData = { enabled: boolean; kind: "http" | "https" | "socks5"; host: string; port: number; username_configured: boolean; password_configured: boolean };
export type Executor = { id: string; name: string; kind: string; command: string; executable?: string; working_directory?: string; proxy_id?: string; enabled: boolean; available: boolean; version?: string; error?: string; status: "available" | "not_found" | "not_executable" | "version_probe_failed"; detected: boolean };
export const desktopApi = {
  dashboard: () => invoke<DashboardData>("dashboard"),
  connection: () => invoke<ConnectionData>("connection"),
  settings: () => invoke<Record<string, unknown>>("settings"),
  proxy: () => invoke<ProxyData>("proxy"),
  availableExecutors: () => invoke<string[]>("available_executors"),
  executors: () => invoke<Executor[]>("executors"),
  chooseExecutorFile: () => invoke<string | null>("choose_executor_file"),
  chooseExecutorDirectory: () => invoke<string | null>("choose_executor_directory"),
  saveExecutor: (input: unknown) => invoke<Executor[]>("save_executor", { input }),
  deleteExecutor: (id: string) => invoke<Executor[]>("delete_executor", { id }),
  testExecutor: (id: string) => invoke<Executor>("test_executor", { id }),
  chooseProjectDirectory: () => invoke<string | null>("choose_project_directory"),
  saveProject: (input: unknown) => invoke<Project[]>("save_project", { input }),
  deleteProject: (id: string) => invoke<Project[]>("delete_project", { id }),
  saveProxy: (input: unknown) => invoke<ProxyData>("save_proxy", { input }),
  testProxy: (input: unknown) => invoke<string>("test_proxy_connection", { input }),
  saveConnection: (input: unknown) => invoke<ConnectionData>("save_connection", { input }),
  cancelTask: (projectName: string, taskId?: string) => invoke<Task>("cancel_task", { projectName, taskId }),
};
