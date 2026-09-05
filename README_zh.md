# AgentBridge

**让一个 AI 思考，让另一个 AI 写代码。**

AgentBridge 通过 **MCP (Model Context Protocol)** 将 Web 端 AI 的深度推理能力与本地 Coding Agent 连接起来。

你可以让更擅长**思考、分析和规划**的 AI（如 Claude 3.7、ChatGPT o3-mini、Gemini 2.5 Pro）负责理解项目并制定计划，让你已经在使用的本地 Coding Agent（如 OpenCode、Claude Code、Aider）负责在本地**修改代码、执行命令和运行测试**。

> **让最擅长思考的 AI 思考，让最适合执行的 Coding Agent 执行。**

[🇺🇸 English](README.md) · [📦 crates.io](https://crates.io/crates/agentbridge) · [🐙 GitHub](https://github.com/IndexFlowing/AgentBridge)

[![Crates.io](https://img.shields.io/crates/v/agentbridge.svg)](https://crates.io/crates/agentbridge)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

---

## 为什么需要 AgentBridge？

现在的 AI Coding 工具通常把两件完全不同的事情绑定在了一起：

* **Reasoning（思考）**：理解代码库、探索项目、分析问题、理解架构、设计解决方案、制定计划以及 Review 修改结果。
* **Execution（执行）**：修改文件、执行命令、运行测试，以及真正完成代码实现。

但实际上，**思考和执行并不一定需要由同一个 AI 完成。**

你可能拥有：

* 一个 Web AI（如 Claude/Gemini），使用额度充足，而且非常擅长复杂逻辑推理；
* 一个 Coding CLI 工具，可以直接操作本地代码，但订阅额度更加昂贵有限。

传统的 Coding Agent 工作流是：

```text
理解 → 探索 → 思考 → 规划 → 编码 → 测试 → Review
```

所有探索与重试步骤都在消耗同一个 Coding Agent 的有限额度。

AgentBridge 把它们拆开：

```text
                 🧠 BRAIN
              Web-based AI
          Gemini / Claude / ...
                 │
            思考 / 分析 / 规划
                 │
                MCP
                 ▼
        ┌─────────────────┐
        │   AgentBridge   │ (默认端口 8040)
        └────────┬────────┘
                 │
             C2C PLAN
                 │
                 ▼
             🛠️ EXECUTOR
            Local Coding Agent
                OpenCode
                 │
           修改 / 执行 / 测试
                 │
                 ▼
          本地项目（可多个）
```

核心思想非常简单：

> **让最擅长思考的 AI 思考，让最适合执行的 Coding Agent 执行。**

---

## 核心角色分工

### 🧠 Brain（大脑）

负责**思考**的 AI。

它可以：

* 列出并切换已挂载的本地项目；
* 查看项目结构与文件列表；
* 搜索并精准阅读代码；
* 理解架构，分析 Bug；
* 制定结构化实现方案（C2C PLAN）；
* 通过 `task_start` 派发执行任务；
* 查看 Git Diff 及测试结果；
* 审查（Review）Executor 的修改。

Brain 通过 AgentBridge 提供的 **只读 MCP 接口** 访问本地项目。**Brain 本身无法直接越权修改文件或执行任意 Shell 脚本。**

### 🛠️ Executor（执行者）

负责**执行**的本地 Coding Agent。

它可以：

* 修改与创建文件；
* 执行本地构建命令；
* 运行单元测试；
* 根据 PLAN 完成代码实现；
* 汇报执行状态与退出码。

AgentBridge 目前原生支持 **OpenCode** 作为执行者，架构设计上支持未来接入更多 CLI 工具（如 Claude Code、Aider、Codex）。

### 🌉 AgentBridge（桥梁）

AgentBridge 负责连接两者，提供：

* Streamable HTTP MCP 服务；
* 面向 ChatGPT / Gemini 自定义 MCP 的 OAuth 2.1 认证；
* 单进程挂载多个本地仓库；
* 安全受控的项目只读检查（按项目沙箱隔离）；
* 结构化任务调度（C2C Protocol）；
* 自动化执行守护（`task_start`、`task_status`、`task_cancel`）；
* 终端实时流式打字输出（`mode = "stream"`）；
* 思考链标签（`<think>`）自动清洗；
* Git Diff 与未跟踪新文件自动捕获；
* 测试结果与生命周期追踪。

---

## 工作流程

一个完整的全自动闭环任务：

```text
用户提出需求
      │
      ▼
    Brain (Claude / ChatGPT / Gemini)
      │
      ├── list_projects / switch_project（挂载了多个仓库时）
      ├── 查看项目 (workspace_info, search_workspace, read_file)
      ├── 理解架构并制定 C2C PLAN
      └── 调用 task_start
              │
              ▼
        AgentBridge (v0.4.0)
              │
           C2C PLAN
              │
              ▼
          Executor (OpenCode CLI)
              │
        ┌─────┼─────┐
        │     │     │
       修改   命令   测试
        │     │     │
        └─────┼─────┘
              │
              ▼
          Git Diff
              │
              ▼
            Brain
              │
            Review (git_diff / test_status)
              │
         ┌────┴────┐
         │         │
        DONE    PLAN AGAIN
```

最终形成一个自动闭环：

```text
PLAN → EXECUTE → REVIEW → DONE
             ↑            │
             └── PLAN ────┘
```

---

## MCP：让 Brain 看见你的项目

AgentBridge 使用 **Model Context Protocol (MCP)** 向 Brain 提供本地 Workspace 的只读访问与任务控制能力。

### 只读检查工具（Safe）

| 工具名称            | 功能描述                                                     |
| ------------------- | ------------------------------------------------------------ |
| `list_projects`     | 列出已挂载的工作区（名称、路径、描述、当前激活）             |
| `switch_project`    | 切换本会话的默认项目                                         |
| `workspace_info`    | 返回工作区路径、语言类型（Rust、Node、Python 等）及 Git 状态 |
| `list_directory`    | 安全列出指定目录结构（防路径穿越）                           |
| `read_file`         | 读取 UTF-8 源码（带大小限制，拦截敏感与二进制文件）          |
| `search_workspace`  | 高性能关键字检索（智能忽略 `node_modules`、`target`、`.git`） |
| `git_status`        | 查看当前 Git 分支、修改/暂存/未跟踪文件状态                  |
| `git_diff`          | 获取修改 Diff（自动格式化并包含新增的未跟踪文件）            |
| `test_status`       | 查看最近一次记录的测试执行结果（不会触发执行）               |
| `execution_summary` | 获取最近一次迭代的结构化汇总                                 |

### 执行控制工具（Autonomous）

| 工具名称      | 功能描述                                                     |
| ------------- | ------------------------------------------------------------ |
| `task_start`  | 提交 C2C Plan，在后台自动唤起本地 OpenCode 编码              |
| `task_status` | 轮询任务生命周期：`running` \| `success` \| `failed` \| `cancelled` |
| `task_cancel` | 安全终止 OpenCode 进程树                                     |

检查类与执行类工具都接受可选参数 `project`。未指定时使用本会话的激活项目（由 `switch_project` 设置，或配置中的默认项目）。路径不能逃出该项目根目录。

---

## C2C：Brain 和 Executor 之间的通信协议

AgentBridge 使用精简结构化的 **C2C (Context-to-Context)** 协议流转任务：

```text
[C2C]
STATE: PLAN
TASK_ID: c2c_20260830_001
ITERATION: 1

GOAL:
为 GSC 客户端添加 URL 检测功能。

ACTIONS:
1. 查看现有的 GSC 客户端实现。
2. 添加 URL inspect 接口。
3. 补充索引状态的单元测试。

TESTS:
cargo test

SUCCESS_CRITERIA:
测试全部通过，API 能正确返回 indexed / not-indexed 状态。
```

真正的源代码始终留在本地工作区。跨越边界的只有任务目标、步骤与测试要求，大幅节省提示词上下文。

---

## 快速开始

### 1. 安装

通过 Cargo 安装：

```bash
cargo install agentbridge
```

验证安装：

```bash
agentbridge --version
```

也可以从 [GitHub Releases](https://github.com/IndexFlowing/AgentBridge/releases) 下载预编译二进制。

#### 从源码编译安装：

```bash
git clone https://github.com/IndexFlowing/AgentBridge.git
cd AgentBridge

cargo install --path .
```

### 2. 初始化项目

```bash
cd /path/to/your/project
agentbridge init . --port 8040
```

`init` 会生成 `.agentbridge.toml`。认证相关字段初始为空（表示不使用）。要固定 OAuth PIN，编辑该文件：

```toml
admin_password = "your-pin-here"
```

AgentBridge **不会**创建或读取项目的 `.env`——那是应用自己的配置（例如 Rust 服务的环境变量）。

环境诊断体检：

```bash
agentbridge doctor
```

### 3. 启动服务

命令行：

```bash
agentbridge serve
```

或打开托盘控制台（启停、PIN、项目、开机启动）：

```bash
agentbridge tray
```

默认监听地址：

```text
http://127.0.0.1:8040/mcp
```

默认启用 OAuth 2.1。若 `.agentbridge.toml` 里设置了 `admin_password`，就用这个 PIN；否则启动横幅会打印随机生成的 **Admin PIN**，用于 `/oauth/authorize`。

```text
➜  Workspaces  : [default] (1 mounted)
➜  MCP Endpoint: http://127.0.0.1:8040/mcp (Streamable HTTP)
➜  Auth        : OAuth 2.1 Enabled (/oauth/authorize)
➜  Admin PIN   : a1b2-c3d4-e5f6
```

仅在受信任的本机调试时使用 `--dev` / `--no-auth`。

---

## 托盘控制台

`agentbridge tray` 会打开桌面控制台（系统托盘 + 设置窗口）：

* 启动 / 停止 MCP 服务
* 复制 MCP 地址和 Admin PIN
* 修改主机、端口、`allow_any_host`、Admin 密码
* 添加 / 移除项目文件夹（写入 `agentbridge.config.json`）
* 开机自动启动

关闭窗口会最小化到托盘；在托盘菜单选 **退出** 才真正退出。配置仍写在 `.agentbridge.toml`，界面不会取代命令行。

### CLI/Core 优先

Executor 的发现、配置、版本探测和任务执行都由 Rust Core 提供，桌面程序只是可选控制面板。Linux、SSH 和无头环境不需要 GUI，可直接使用 `agentbridge doctor`、`agentbridge status`、`agentbridge serve` 和 `agentbridge task ...`。

Executor 的显示名称只用于界面展示，不会改变真实 command、可执行文件路径、类型或稳定 ID。PATH 自动发现结果与已保存配置分开显示，状态依据是命令探测和 `--version` 探测；刷新不会覆盖配置或制造重复条目。

---

## 多项目工作区

一个 AgentBridge 进程可以同时托管多个本地仓库。

### 单个目录

```bash
agentbridge serve D:\Project\MyProject
```

该目录会自动成为名为 `default` 的项目。

### 多个仓库

编写 `agentbridge.config.json`（可参考 `examples/agentbridge.config.json`）：

```json
{
  "projects": [
    {
      "name": "indexflow-core",
      "path": "D:\\Project\\IndexFlow\\IndexFlow-core",
      "description": "IndexFlow 核心后端与引擎",
      "readonly": false
    },
    {
      "name": "mandarin-clips",
      "path": "D:\\Project\\MandarinClips",
      "description": "MandarinClips Web 平台与媒体工具",
      "readonly": false
    }
  ],
  "default_project": "indexflow-core"
}
```

然后：

```bash
agentbridge serve --workspaces agentbridge.config.json
```

若当前目录已有 `agentbridge.config.json`，直接 `agentbridge serve` 也会自动加载。

Brain 应先调用 `list_projects`，再用 `switch_project` 或在单个工具上传入 `project`。每次调用都限制在该项目根目录内，`../` 无法进入兄弟仓库。只读项目会拒绝 `task_start`。

---

## 认证（OAuth 2.1）

ChatGPT 与 Gemini Web 的自定义 MCP 连接遵循 MCP OAuth 2.1 Protected Resource 流程。AgentBridge 在进程内实现了所需端点：

| 端点 | 作用 |
| ---- | ---- |
| `GET /.well-known/oauth-protected-resource` | RFC 9728 资源元数据（`resource`、`authorization_servers`、`mcp:read` / `mcp:write`） |
| `GET /.well-known/oauth-authorization-server` | RFC 8414 发现（`/oauth/authorize`、`/oauth/token`、PKCE S256） |
| `GET/POST /oauth/authorize` | 浏览器授权页（Admin PIN），批准后带 `code` 与 `state` 回跳 |
| `POST /oauth/token` | 用 `code` + `code_verifier` 换取 `access_token` / `refresh_token` |
| `POST /oauth/register` | RFC 7591 动态客户端注册（ChatGPT / Gemini 会走这条） |

未带有效令牌访问 `/mcp` 会返回：

```http
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="mcp", resource_metadata="https://<host>/.well-known/oauth-protected-resource"
```

请求头带上有效的 `Authorization: Bearer <token>`（OAuth access token **或** 静态 `--auth-token`）即可访问。

### 配置方式

在 `.agentbridge.toml` 里填写即可（空 = 不使用）。命令行参数会覆盖文件，其次才是可选的进程环境变量：

| toml / 参数 | 作用 |
| ----------- | ---- |
| `admin_password` / `--admin-password` | `/oauth/authorize` 的 PIN（未设置则启动时生成） |
| `client_id` / `--client-id` | 可选预注册 OAuth 客户端 |
| `client_secret` / `--client-secret` | 可选 client secret |
| `auth_token` / `--auth-token` | 额外的静态 Bearer |
| `no_auth` / `--no-auth` / `--dev` | 关闭 401 挑战（仅建议本机） |
| `allow_any_host` / `--allow-any-host` | Cloudflare Tunnel 需要（放行公网 `Host`） |

---

## 连接 Brain

### 方案 A：Claude Desktop 桌面端（零 API 费用，完全免费）

打开 `%APPDATA%\Claude\claude_desktop_config.json`（Windows）或 `~/Library/Application Support/Claude/claude_desktop_config.json`（macOS）：

```json
{
  "mcpServers": {
    "agentbridge": {
      "command": "npx",
      "args": [
        "-y",
        "mcp-remote",
        "http://127.0.0.1:8040/mcp"
      ]
    }
  }
}
```

*重启 Claude Desktop，聊天框右下角将出现 🔨 图标，所有工具自动就绪。*

本机 Claude Desktop 可以用 `agentbridge serve --dev` 启动，这样本地代理不必再带 Bearer。

### 方案 B：通过 Cloudflare Tunnel 接入 Web AI（Gemini / ChatGPT）

```bash
agentbridge serve --allow-any-host
```

在新终端中将 8040 端口映射到公网：

```bash
cloudflared tunnel --url http://127.0.0.1:8040
```

把远程 MCP 客户端指向：

```text
https://<your-tunnel-id>.trycloudflare.com/mcp
```

1. ChatGPT / Gemini 会先收到 `401`，再开始 OAuth 发现。
2. 在浏览器打开的 `/oauth/authorize` 页面输入启动横幅里的 **Admin PIN** 并批准。
3. 也可以用 `--auth-token` 启动，并在客户端填写 `Authorization: Bearer <token>`。

把 `skill/SKILL.md` 贴进系统提示 / Skill 槽，让模型按 Brain 行事（`list_projects`、只读检查、`task_start`，自己不要改文件）。

---

## 配置文件 (`.agentbridge.toml`)

`agentbridge init` 会在工作区根目录下生成配置：

```toml
workspace = "D:\\Project\\MyProject"
host = "127.0.0.1"
port = 8040
allow_any_host = false                    # 走公网隧道转发时设为 true
auth_token = ""                           # 可选静态 Bearer；空 = 不使用
admin_password = ""                       # OAuth 授权页 PIN；空 = 启动时生成
client_id = ""                            # 可选预注册 OAuth 客户端；空 = 动态注册
client_secret = ""
no_auth = false                           # true 关闭 /mcp 的 401（仅建议本机）

[executor]
type = "opencode"
command = "opencode"                      # Windows 下如果使用 npm 全局安装，请配置为 "opencode.cmd"
mode = "stream"                           # "stream" (终端实时输出) 或 "silent" (静默后台)

[security]
max_file_size = 1048576                   # 单文件上限 1MB
deny_sensitive_files = true               # 拦截 .env, *.pem, *.key, id_rsa
max_diff_bytes = 65536                    # Diff 上限 64KB
```

监听地址、执行者等仍来自 `.agentbridge.toml`（或 `~/.agentbridge/config.toml`）。挂载哪些仓库则来自 `agentbridge.config.json`、`--workspaces` 或 `agentbridge serve <dir>`。

> 💡 **Windows 特别提示**：如果通过 npm 全局安装了 OpenCode，请将 `command` 显式设置为 `opencode.cmd`（或绝对路径），以确保 Windows `CreateProcess` 能够正确拉起批处理脚本（避免 `os error 193`）。

---

## CLI 命令速查

```bash
# 初始化工作区配置
agentbridge init <workspace> --port 8040

# 托盘控制台：启停 MCP、编辑项目和 OAuth、可选开机启动
agentbridge tray

# 启动 MCP 服务（读取 .agentbridge.toml / agentbridge.config.json）
agentbridge serve

# 将单个目录作为名为 default 的项目
agentbridge serve D:\Project\MyProject --allow-any-host

# 挂载多个仓库
agentbridge serve --workspaces agentbridge.config.json

# 本机开发跳过 OAuth
agentbridge serve --dev

# 指定授权页 PIN，并额外接受静态 Bearer
agentbridge serve --admin-password "my-pin" --auth-token "$TOKEN"

# 查看当前工作区状态、Git 改动及任务进度
agentbridge status

# 检查运行环境、Git 及 OpenCode 安装情况
agentbridge doctor

# 在本地发起任务并直接同步等待执行
agentbridge task start --goal "..." --execute

# 取消正在后台运行的执行者进程
agentbridge task cancel
```

---

## 安全与隔离

AgentBridge 采用防御性默认安全设计：

* **严格只读检查**：远程 Brain 永远无法直接在本地执行任意 Shell 命令或随意篡改文件。
* **默认拦截敏感访问**：严格禁止路径穿越（`../`），默认拒绝读取敏感文件（`.env`、`credentials`、`id_rsa`、`*.pem`）。
* **按项目沙箱**：每次工具调用都限制在所选项目根目录内，不能通过 `../` 或绝对路径进入兄弟仓库。
* **进程隔离**：Executor 的工作目录就是该项目目录，并自动清除敏感环境变量。
* **认证防护**：默认启用 MCP OAuth 2.1（授权码 + PKCE、动态客户端注册），供 ChatGPT / Gemini 自定义 MCP 使用；同时仍接受静态 Bearer `auth_token`。未带令牌访问 `/mcp` 返回 `401` 及 `WWW-Authenticate`。
* **公网地址**：只要把端点隧道出去，就应保持 OAuth 开启（或设置 `--auth-token`）。`--no-auth` / `--dev` 仅用于受信任的本机回环。

完整模型见 [docs/security.md](docs/security.md)。

---

## 开源许可证

MIT License © [AgentBridge Contributors](LICENSE)
