# 架构

AgentBridge 是一个本地常驻进程，而不是一个 Coding Agent。它连接两端的 AI 角色，并在本地托管工作区、任务与认证。

```text
                    Brain (远端 AI)
        ┌───────────────┼───────────────┐
        │               │               │
     Gemini          Claude         ChatGPT
        │               │               │
        └───────────────┼───────────────┘
                        │  MCP (Streamable HTTP)
                   只读检查 + task_start
                        │
                 ┌──────▼──────┐
                 │ AgentBridge │  Rust Core
                 │  + Web 管理 │
                 └──────┬──────┘
                        │  C2C PLAN
                        ▼
                   OpenCode Executor
                        │
              编辑 / 运行 / 测试
                        │
                        ▼
                 本地项目工作区
                        │
             git_diff / test_status / execution_summary
                        │
                        ▼
                      Brain 审查
```

## 分层

| 层 | 位置 | 职责 |
| --- | --- | --- |
| **MCP Server** | `src/mcp/` | Streamable HTTP 工具。检查类只读；`list_projects` / `switch_project` 选择项目；`task_start` / `task_status` / `task_cancel` 控制本地 Executor |
| **ProjectHub** | `src/projects/` | 单进程托管多个本地仓库；每个项目一个 `Workspace` 与一个 `TaskRuntime`；按项目根目录沙箱隔离 |
| **Workspace** | `src/workspace.rs` | 路径隔离、文件读取、目录列举、关键字搜索 |
| **TaskRuntime** | `src/task.rs` | 任务生命周期：created → planned → running → executed / failed / blocked / cancelled |
| **Executor** | `src/executor/` | OpenCode 适配器：以工作区为 cwd 启动 `opencode run`，捕获退出码/摘要/变更文件，输出脱敏 |
| **C2C 协议** | `src/protocol.rs` | 紧凑的 PLAN / REVIEW 消息；`C2cPlan` 是结构化的 Brain → Executor 载荷 |
| **OAuth 2.1** | `src/oauth/` | 受保护资源元数据、授权服务器元数据、授权码 + PKCE、动态客户端注册、`/mcp` 401 挑战 |
| **Storage** | `src/storage/` | 服务级 SQLite：项目、执行器、任务快照、OAuth 客户端/令牌。启动级配置（host/port/认证/日志）在 `~/.agentbridge/config.toml` |
| **HTTP / Web** | `src/server/`, `src/api/`, `web/` | 在同一进程暴露 `/mcp`、`/oauth/*`、`/api/*`，并从 `/` 托管 Web 控制平面 |
| **Service 生命周期** | `src/service/` | 跨平台进程抽象：`start` / `stop` / `restart` / `status` 记录 AgentBridge server PID，健康探针校验归属，重复启动与误杀均有防护；`serve` 仍是唯一底层前台入口 |
| **CLI** | `src/cli/` | 产品入口：`serve` / `start` / `stop` / `restart` / `status` / `workspace` / `doctor` / `task` |

## 控制平面与管理平面

- **MCP（`/mcp`）**：面向 Brain 的数据平面，受 OAuth 2.1 / 静态 Bearer 保护。只读检查 + 任务委托，不暴露管理写操作。
- **管理 API（`/api/*`）**：面向 Web 控制平面。仅允许回环地址访问，不参与 Brain 的 MCP Token。
- **Web（`/`）**：静态前端（`rust-embed` 打包 `web/`），调用 `/api/*` 完成项目、执行器与设置管理。

CLI/Core 是产品主体；Web 是可选的可视化管理工具；Desktop / Tray 已废弃。

## 状态与持久化

| 数据 | 介质 |
| --- | --- |
| 启动级配置（host/port/allow_any_host/认证/日志） | `~/.agentbridge/config.toml` |
| 后台服务 PID / 监听地址（生命周期状态） | `~/.agentbridge/service.json`（纯进程状态，不依赖 SQLite） |
| 后台服务 stdout/stderr | `~/.agentbridge/service.log` |
| 项目、执行器 | `~/.agentbridge/agentbridge.db`（SQLite） |
| 当前任务快照（每项目） | SQLite `tasks` 表（`state_json`） |
| Executor 读取的任务契约 | `<workspace>/.agentbridge/current.c2c` |
| 运行中的 Executor 进程号 | `<workspace>/.agentbridge/executor.pid`（与 `service.json` 无关） |

服务是否运行由 `~/.agentbridge/service.json` 中的 PID 存活性与 `/health` 探针判定，不查询 SQLite。Windows Service、Linux systemd、macOS launchd 可在 `src/service/backend.rs` 的 `ServiceBackend` 之上扩展，无需改动生命周期逻辑。

MCP 的 `test_status` / `execution_summary` / `task_status` 通过 `Storage` 读取，因此 CLI、Web 与 Brain 共享同一份事实来源。

## 信任边界

Brain 是远端模型，绝不应获得：

- 任意文件系统访问；
- 通用 shell 工具；
- 写文件工具；
- 可执行文件名（OpenCode 的命令来自本机注册表，而非 MCP 请求）；
- 敏感文件（`.env`、密钥、`~/.ssh` 等）。

Executor 是用户已经信任的本地 Agent。AgentBridge 只把它启动在所选工作区内，并记录：

```text
status, summary, exit_code, tests, changed_files, error
```

模型内部推理不会被存储或返回。

## 版本

版本以 `Cargo.toml` 的 `package.version` 为唯一事实来源，文档不硬编码版本号。
