// web/src/api/types.ts
// Wire types mirrored from the Rust Core serde contracts.
// Keep these in sync with src/models/console.rs and src/dashboard.rs.
// Do NOT add fields the backend does not serialize.

export type ProxyKind = 'http' | 'https' | 'socks5';

export interface ConnectedClient {
  client_id: string;
  client_name: string;
  has_active_token: boolean;
}

export interface GatewayStatus {
  online: boolean;
  endpoint: string;
  host: string;
  port: number;
  managed: boolean;
  clients: ConnectedClient[];
}

export interface Project {
  id: string;
  name: string;
  path: string;
  description: string;
  readonly: boolean;
  active: boolean;
  git_repository: boolean;
  project_type: string[];
  executor: string;
}

export interface TestResult {
  status: string;
  command: string;
  exit_code?: number;
  summary?: string;
  timestamp: string;
}

export interface TaskData {
  project: string;
  task_id: string | null;
  lifecycle: string | null;
  status: string | null;
  iteration: number;
  executor: string | null;
  goal: string | null;
  summary: string | null;
  error: string | null;
  changed_files: string[];
  tests: TestResult | null;
  created_at: string | null;
  started_at: string | null;
  finished_at: string | null;
  updated_at: string;
}

export interface ActivityItem {
  project: string;
  task_id: string | null;
  status: string | null;
  goal: string | null;
  summary: string | null;
  timestamp: string;
}

export interface DashboardData {
  gateway: GatewayStatus;
  projects: Project[];
  tasks: TaskData[];
  executor: string;
  activity: ActivityItem[];
}

export interface ConnectionData {
  config_path: string;
  workspace: string;
  host: string;
  port: number;
  endpoint: string;
  auth_enabled: boolean;
  oauth_enabled: boolean;
  no_auth: boolean;
}

export interface ProxyData {
  enabled: boolean;
  kind: ProxyKind;
  host: string;
  port: number;
  username_configured: boolean;
  password_configured: boolean;
}

export interface Executor {
  id: string;
  name: string;
  kind: string;
  command: string;
  executable?: string;
  working_directory?: string;
  proxy_id?: string;
  enabled: boolean;
  available: boolean;
  version?: string;
  error?: string;
  status: string;
  detected: boolean;
}

export interface ProjectInput {
  id?: string;
  name: string;
  path: string;
  executor: string;
}

export interface ExecutorInput {
  id?: string;
  name: string;
  kind: string;
  command: string;
  executable?: string;
  working_directory?: string;
  proxy_id?: string;
  enabled: boolean;
}

export interface ProxyInput {
  enabled: boolean;
  kind: ProxyKind;
  host: string;
  port: number;
  username?: string;
  password?: string;
}

export type ExecutorMode = 'stream' | 'silent';

export interface ConnectionInput {
  host: string;
  port: number;
  no_auth: boolean;
  allow_any_host?: boolean;
  auth_token?: string;
  admin_password?: string;
  executor_command: string;
  executor_mode: ExecutorMode;
}
