# AgentBridge

**让一个 AI 负责思考，让本地 Coding Agent 负责执行。**

AgentBridge 是一个本地运行的 **MCP Bridge**：把具备强推理能力的远端 AI（Brain）连接到本机已有的 Coding Agent（Executor），二者通过标准 MCP 协议与 C2C 任务协议协作。Brain 只读地理解代码并制定计划，Executor 在工作区内真正修改文件、运行命令与测试。

> **CLI / Core 是产品主体，Web 是管理工具，Desktop / Tray 已废弃。**

[![Crates.io](https://img.shields.io/crates/v/agentbridge.svg)](https://crates.io/crates/agentbridge)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

---

## 核心定位

AI 编程工具通常同时承担两件事：

- **推理**：理解代码库、定位问题、设计方案、审查改动。
- **执行**：编辑文件、运行命令、跑测试、应用改动。

这两件事不必由同一个模型完成。AgentBridge 把它们拆成三层：

```text
        BRAIN (远端 AI)
   Claude / ChatGPT / Gemini
              │
        只读检查 + task_start
              │  MCP (Streamable HTTP)
              ▼
     ┌───────────────────────┐
     │      AgentBridge      │  本地 Rust 进程
     │  MCP Server + Core    │  CLI 主体 / Web 管理
     └───────────┬───────────┘
                 │  C2C PLAN
                 ▼
        EXECUTOR (本地 Coding Agent)
              OpenCode
                 │
        编辑 / 运行 / 测试
                 │
                 ▼
          本地项目工作区
```

### 三层职责

| 角色 | 职责 | 边界 |
| --- | --- | --- |
| **Brain** | 列出/切换项目、检查目录、搜索、读取文件、制定 C2C PLAN、调用 `task_start`、审查 diff 与测试结果 | 只能通过只读 MCP 工具访问工作区，不能执行任意 shell、不能直接改文件 |
| **AgentBridge** | 连接 Brain 与 Executor：MCP Server、项目与执行器管理、任务运行时、OAuth 2.1、C2C 协议、结果记录 | 不写业务代码；不把管理写操作暴露给 Brain |
| **Executor** | 在工作区内实现 PLAN：改文件、跑命令、跑测试、汇报结果 | 当前正式支持 OpenCode；其它执行器仅可登记，尚不能真正启动 |

---

## 版本事实来源

项目版本**唯一**以 [`Cargo.toml`](Cargo.toml) 的 `package.version` 为准。README 与文档不硬编码版本号，运行时可通过以下命令查看：

```bash
agentbridge --version
```

本 README 与其它文档均不硬编码版本号，避免与 `Cargo.toml` 漂移。

---

## 当前能力

核心能力全部在 Rust Core / CLI 中实现，Web 只是它们的可视化前端：

- **Streamable HTTP MCP Server**：`http://127.0.0.1:8040/mcp`。
- **只读工作区沙箱**：按项目根目录隔离，拒绝路径逃逸与敏感文件。
- **多项目 ProjectHub**：单个进程托管多个本地仓库，每个项目拥有独立 `TaskRuntime`。
- **任务运行时**：`task_start` / `task_status` / `task_cancel`，进程树可安全终止。
- **结构化 C2C 协议**：只传递紧凑的 PLAN / REVIEW，不搬运源码。
- **OpenCode Executor**：在工作区目录内启动 `opencode run`，捕获退出码、测试摘要与变更文件。
- **配置分离**：启动级配置（host/port/allow_any_host/认证/日志）统一存放于 `~/.agentbridge/config.toml`；运行数据（项目、执行器、任务快照、OAuth）存放于 `~/.agentbridge/agentbridge.db`（SQLite）。
- **OAuth 2.1**：授权码 + PKCE + 动态客户端注册，适配 ChatGPT / Gemini 的自定义 MCP 连接。
- **Web 控制平面**：仪表盘、项目管理、执行器管理、设置（仅本机回环访问）。

---

## 快速开始

### 1. 安装

```bash
cargo install agentbridge
agentbridge --version
```

也可以从 [GitHub Releases](https://github.com/IndexFlowing/AgentBridge/releases) 下载预编译产物。从源码构建：

```bash
git clone https://github.com/IndexFlowing/AgentBridge.git
cd AgentBridge
cargo install --path .
```

### 2. 启动 Core

```bash
agentbridge start     # 后台启动本地服务并记录 PID
agentbridge status    # 查看运行状态（running/stopped、PID、监听地址）
```

`start` 会以分离进程方式调用统一的底层入口 `serve`，并把服务 PID 与监听地址写入 `~/.agentbridge/service.json`。重复执行 `start` 不会启动第二个实例。

需要前台运行、直接查看日志时仍可使用：

```bash
agentbridge serve
```

默认监听：

```text
MCP 端点 : http://127.0.0.1:8040/mcp
Web 控制 : http://127.0.0.1:8040/
```

首次启动时会自动创建 `~/.agentbridge/config.toml`（默认 `host = "127.0.0.1"`、`port = 8040`、`allow_any_host = true`）。数据库为空时，ProjectHub 会以当前目录挂载一个名为 `default` 的项目。之后可在 Web 控制平面中增删项目与执行器。

### 3. 打开 Web 控制平面

浏览器访问 `http://127.0.0.1:8040/`：

- **仪表盘**：MCP 网关地址、项目数量、默认执行器、OAuth 已连接客户端。
- **项目管理**：挂载/移除本地工作区（名称 + 绝对路径），写入 SQLite 并热重载 ProjectHub。
- **执行器管理**：登记本地执行器、探测命令是否可用。
- **设置**：查看监听地址、MCP 端点、代理等（涉及监听地址/端口的改动需要重启进程）。

> `/api/*` 管理接口**仅允许回环地址**访问，避免远端 Brain 通过 MCP Token 越权修改本机配置。

### 4. 检查环境

```bash
agentbridge status                # 服务生命周期状态：running/stopped、PID、监听地址
agentbridge workspace             # 当前默认项目、类型、git 状态、任务状态
agentbridge doctor                # config / workspace / port / bind / auth / opencode / git / cloudflared
```

---

## CLI 参考

CLI 是产品的第一入口，完整的管理能力均可在无 GUI 环境使用：

```bash
# 启动 MCP Server + Web 控制平面（前台，统一底层入口）
agentbridge serve
agentbridge serve --host 127.0.0.1 --port 8040
agentbridge serve --allow-any-host          # 通过隧道访问时放开 Host 校验
agentbridge serve --dev                     # 本机调试，关闭 /mcp 401 挑战（等价 --no-auth）
agentbridge serve --admin-password "$PIN"   # 固定 OAuth 授权页 PIN
agentbridge serve --auth-token "$TOKEN"     # 额外静态 Bearer Token

# 服务生命周期（后台，读取 ~/.agentbridge/config.toml）
agentbridge start                           # 后台启动；已在运行时不会重复启动
agentbridge status                          # running/stopped、PID、监听地址
agentbridge stop                            # 停止记录在案的 PID
agentbridge stop --force                    # 健康检查无法确认归属时强制停止
agentbridge restart                         # 等价 stop + start

# 查看默认工作区状态
agentbridge workspace

# 依赖与运行环境诊断
agentbridge doctor

# 任务生命周期（默认作用于第一个/默认项目）
agentbridge task start --goal "..." --tests "cargo test" --execute
agentbridge task status
agentbridge task cancel
agentbridge task executed --status success --tests "cargo test" --exit-code 0
```

认证字段优先级（仅这些字段支持环境变量）：`CLI 参数 > AGENTBRIDGE_* 环境变量 > ~/.agentbridge/config.toml`。
监听地址/端口同样来自 `CLI 参数 > config.toml`，不再依赖 SQLite。修改 `config.toml` 后需要重启服务才会生效：后台服务用 `agentbridge restart`，前台进程用 `agentbridge serve`。

`~/.agentbridge/config.toml` 的常用启动配置：

| 键 | 作用 |
| --- | --- |
| `host` / `port` | 监听地址与端口（默认 `127.0.0.1:8040`） |
| `allow_any_host` | 是否放开 MCP 的 Host 校验（隧道场景，默认 `true`） |
| `auth_token` / `admin_password` | 静态 Bearer Token / OAuth 授权页 PIN |
| `[logging] level` | 日志级别（默认 `info`；`RUST_LOG` 优先级更高） |

---

## MCP 工具

Brain 通过 MCP 与工作区交互。检查类工具严格只读，执行类工具只负责委托。

### 只读检查（安全）

| 工具 | 说明 |
| --- | --- |
| `list_projects` | 列出已挂载项目及当前激活项目 |
| `switch_project` | 切换本会话的激活项目 |
| `workspace_info` | 工作区路径、项目类型（Rust / Node / Python / Go / Java / C/C++）、Git 状态 |
| `list_directory` | 防穿越的结构化目录列表 |
| `read_file` | 读取 UTF-8 文本（大小受限，二进制与敏感文件拒绝） |
| `search_workspace` | 关键字搜索，自动忽略 `node_modules`、`target`、`.git` 等 |
| `git_status` | 分支、变更、暂存、未跟踪文件 |
| `git_diff` | 工作区/暂存 diff，并自动补充未跟踪新文件的 diff |
| `test_status` | 最近一次记录在案的测试结果（不会真正运行测试） |
| `execution_summary` | 最近一次迭代的结构化摘要 |

### 执行委托（自主）

| 工具 | 说明 |
| --- | --- |
| `task_start` | 用校验后的 C2C PLAN 启动本地 Executor |
| `task_status` | 轮询任务生命周期：`running` / `success` / `failed` / `cancelled` / `blocked` |
| `task_cancel` | 安全终止正在运行的 Executor 进程树 |

检查类与执行类工具都接受可选的 `project` 参数；缺省时使用会话的激活项目。任何路径都无法逃出该项目根目录。

---

## C2C：Brain 与 Executor 的通信协议

AgentBridge 用轻量的 **C2C（Context-to-Context）** 消息传递任务契约，源码与 diff 始终留在本地工作区：

```text
[C2C]
STATE: PLAN
TASK_ID: c2c_20260913_001
ITERATION: 1

GOAL:
Add URL inspection support to the GSC client.

ACTIONS:
1. Inspect the existing GSC client.
2. Add URL inspection support.
3. Add tests for indexed and non-indexed URLs.

TESTS:
cargo test

SUCCESS_CRITERIA:
Tests pass and the API correctly reports indexed / non-indexed.
```

典型闭环：

```text
PLAN → EXECUTE → REVIEW → DONE
              ↑            │
              └── PLAN ────┘   （未达成 SUCCESS_CRITERIA 时进入下一轮迭代）
```

---

## 连接 Brain

### 本机 MCP 客户端（Claude Desktop 等）

```json
{
  "mcpServers": {
    "agentbridge": {
      "command": "npx",
      "args": ["-y", "mcp-remote", "http://127.0.0.1:8040/mcp"]
    }
  }
}
```

本机调试可用 `agentbridge serve --dev` 关闭 401 挑战。

### 远程 Web AI（ChatGPT / Gemini）

1. 启动：`agentbridge serve --allow-any-host`。
2. 暴露端口（示例）：

   ```bash
   cloudflared tunnel --url http://127.0.0.1:8040
   ```

3. 将 `https://<your-tunnel-id>.trycloudflare.com/mcp` 配置为远端 MCP 地址。
4. 客户端收到 `401` 后进入 OAuth 流程，在浏览器 `/oauth/authorize` 页面输入启动横幅或 `--admin-password` 指定的 **Admin PIN**。
5. 也可启动时设置 `--auth-token`，直接使用 `Authorization: Bearer <token>`。

最后把 [`skill/SKILL.md`](skill/SKILL.md) 贴入模型的系统提示 / Skill 槽位，使它以 Brain 身份工作（只检查、只规划、通过 `task_start` 委托执行）。

### OAuth 2.1 端点

| 端点 | 作用 |
| --- | --- |
| `GET /.well-known/oauth-protected-resource` | RFC 9728 资源元数据（含 `mcp:read` / `mcp:write`） |
| `GET /.well-known/oauth-authorization-server` | RFC 8414 授权服务器发现（PKCE S256） |
| `GET/POST /oauth/authorize` | 浏览器授权页（Admin PIN）→ 携带 `code`/`state` 重定向 |
| `POST /oauth/token` | 用 `code` + `code_verifier` 换取 access/refresh token |
| `POST /oauth/register` | RFC 7591 动态客户端注册 |

未认证访问 `/mcp` 返回：

```http
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="mcp", resource_metadata="https://<host>/.well-known/oauth-protected-resource"
```

---

## 项目与执行器

- **项目**：存储于 SQLite，由 `ProjectHub` 在内存中构建，每个项目一个 `TaskRuntime`。通过 Web 控制平面增删后，Hub 会立即热重载。
- **执行器**：默认执行器为 OpenCode。当前只有 `kind == "opencode"` 会真正启动进程；Codex / Claude 等可被登记与探测，但在适配器完成前无法运行。
- **任务快照**：每个项目的最新任务状态以 JSON 形式存于 `tasks` 表，同时写出 `<workspace>/.agentbridge/current.c2c` 供 Executor 读取。

数据目录：

```text
~/.agentbridge/config.toml          # 启动级配置（host/port/认证/日志）
~/.agentbridge/agentbridge.db       # 运行数据（项目/执行器/任务/系统设置）
~/.agentbridge/service.json         # 后台服务 PID/监听地址（生命周期状态，不依赖 SQLite）
~/.agentbridge/service.log          # 后台服务 stdout/stderr
<workspace>/.agentbridge/current.c2c # 当前任务的 C2C 计划
<workspace>/.agentbridge/executor.pid# 运行中的 Executor 进程号（与 service.json 无关）
```

---

## 安全模型

- **只读视图**：远端 Brain 无法执行任意 shell，也无法直接覆盖文件，只能通过 `task_start` 委托。
- **默认拒绝**：拒绝路径穿越（`../`）、敏感文件名（`.env`、`credentials*`、`id_rsa`、`*.pem`、`*.key`）与家目录敏感树（`~/.ssh`、`~/.aws` 等）。
- **逐项目沙箱**：每次调用都限定在所选项目根目录内，无法通过 `../` 或绝对路径到达同级仓库。
- **进程隔离**：Executor 以项目目录为工作目录启动；其可执行命令来自本机配置，绝不来自 MCP 请求。
- **管理接口**：`/api/*` 仅回环可访问，与 Brain 使用的 MCP Bearer 分离。
- **认证**：默认启用 OAuth 2.1；公网暴露时必须保留 OAuth 或设置 `--auth-token`。仅在可信回环环境使用 `--no-auth` / `--dev`。
- **日志**：不记录用户输入或系统的 Admin PIN，也不回显 access/refresh token 明文。

详见 [docs/security.md](docs/security.md)。

---

## 开发

```bash
# Rust 检查与测试
cargo fmt --check
cargo check
cargo test

# 构建 Web 控制平面（输出 web/dist，由 rust-embed 打包进二进制）
cd web
npm install
npm run build
```

---

## 文档

- [架构](docs/architecture.md)
- [安全](docs/security.md)
- [OpenCode Executor](docs/opencode.md)
- [Gemini / ChatGPT 接入](docs/gemini.md)
- [Brain Skill](skill/SKILL.md)

---

## 产品边界（重要）

- **CLI / Core 是主体**：`serve`、`start`、`stop`、`restart`、`status`、`workspace`、`doctor`、`task` 与 MCP Server 是产品的完整形态，可在 Linux、SSH、无头环境中使用。
- **Web 是管理工具**：仅提供可视化配置与状态查看，不承载核心业务逻辑，也不负责启动任务。
- **Desktop / Tray 已废弃**：旧桌面端与托盘不再是受支持的产品方向，相关描述已从文档中移除。

---

## License

MIT License © [AgentBridge Contributors](LICENSE)
