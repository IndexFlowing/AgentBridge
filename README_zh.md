# AgentBridge

[English](README.md) | [中文](README_zh.md)

**用一个 AI 思考，用另一个 AI 写代码。**

通过 MCP 把「大脑」接到本地编码智能体。

用 Gemini、Claude 或 ChatGPT 查看并推理你的本地项目；
由 OpenCode、Codex 或 Claude Code 负责真正改代码。

```
ChatGPT Web
    ↓ MCP
AgentBridge
    ↓ C2C PLAN
OpenCode CLI
    ↓
本地工作区
    ↓ git_diff / 测试
ChatGPT Web REVIEW
```

你的 AI 订阅不必绑死在某一个编码智能体上。
用你喜欢的模型当大脑，用你喜欢的编码智能体当手。

---

### 大脑（Brain）

规划、分析、审查。

### 执行者（Executor）

改文件、跑命令、跑测试。

### MCP

把它们连起来，而不必把整个项目粘进提示词。

大脑永远不能写文件，也不能执行命令。这就是设计本身。

---

## 快速开始

```bash
git clone https://github.com/agentbridge/agentbridge.git
cd agentbridge

cargo install --path .

agentbridge init ~/projects/my-project

agentbridge serve
```

MCP 监听 `http://127.0.0.1:8787/mcp`（仅本机）。

可选：用 [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-apps/do-more-with-tunnels/trycloudflare/) 暴露给网页端 AI：

```bash
agentbridge serve --allow-any-host
```

另开一个终端：

```bash
cloudflared tunnel --url http://127.0.0.1:8787
```

把兼容客户端（Gemini / AI Studio、Claude、ChatGPT 等）指向：

```
https://<id>.trycloudflare.com/mcp
```

把大脑说明装到客户端：[`skill/SKILL.md`](skill/SKILL.md)。

Cloudflare 是可选的。核心功能完全可以只在本机使用。

---

## 工作流

```
1. 在本地启动 AgentBridge。
2. （可选）用 Cloudflare Tunnel 暴露出去。务必使用 --auth-token。
3. 连接支持远程 MCP 的 AI 客户端，并提供 skill/SKILL.md。
4. 给 AI 一个编码任务。
5. 大脑通过 MCP 读取工作区——不要粘贴源文件。
6. 大脑产出一份简短的 C2C PLAN，并调用 task_start。
7. AgentBridge 在配置的工作区里启动 OpenCode。
8. OpenCode 改文件、在本地跑测试。
9. 大脑轮询 task_status，再读取 git_diff / test_status / execution_summary。
10. 大脑写出 REVIEW。
11. DONE、再走一轮 PLAN（再次 task_start，iteration + 1），或 BLOCKED。
```

C2C 消息保持简短。diff 和源码留在 MCP 里。

```
[C2C]
STATE: PLAN
TASK_ID: c2c_12345
ITERATION: 1

GOAL:
给 GSC 客户端加上 URL 检测。

ACTIONS:
1. 阅读现有客户端。
2. 添加 inspect()。
3. 补测试。

TESTS:
cargo test

SUCCESS_CRITERIA:
测试通过；API 能报告 indexed / not-indexed。
```

---

## 命令行

```bash
agentbridge init ~/projects/my-project
agentbridge serve
agentbridge status
agentbridge doctor
agentbridge task start --goal "..."
agentbridge task start --goal "..." --execute
agentbridge task executed --status success --tests "cargo test" --exit-code 0
agentbridge task cancel
```

`init` 会写入 `~/.agentbridge/config.toml` 和 `<workspace>/.agentbridge.toml`：

```toml
workspace = "/absolute/path/to/project"
host = "127.0.0.1"
port = 8787

[executor]
type = "opencode"
command = "opencode"

[security]
max_file_size = 1048576
deny_sensitive_files = true
```

`doctor` 会检查工作区、git、OpenCode、MCP 配置、端口，以及可选的 `cloudflared`。

---

## MCP 工具

只读检查：

| 工具 | 返回内容 |
|------|---------|
| `workspace_info` | 路径、项目类型、是否 git 仓库 |
| `list_directory` | 结构化目录列表 |
| `read_file` | UTF-8 文本（有大小上限） |
| `search_workspace` | 文件、行号、匹配文本 |
| `git_status` | 分支、是否干净、已改 / 已暂存 / 未跟踪 |
| `git_diff` | 工作区或暂存区 diff；过大时截断 |
| `test_status` | 最近一次记录的测试结果（不会真正跑测试） |
| `execution_summary` | 最近一次执行者结果 |

执行者控制（没有 shell，大脑不能指定可执行文件）：

| 工具 | 返回内容 |
|------|---------|
| `task_start` | `task_id` + `running` — 用 C2C PLAN 启动 OpenCode |
| `task_status` | `running` / `success` / `failed` / `blocked` / `cancelled` |
| `task_cancel` | 终止 OpenCode 进程树 |

路径穿越（`../`、`/etc/passwd`、`C:\Users\...`、`~/.ssh`）会被拒绝。
`.env`、`*.pem`、`*.key`、`id_rsa` 以及类似文件名会被拒绝。

---

## 安全警告

本项目会把本地工作区的**只读视图**暴露给远程 AI 模型。

- 不要暴露密钥。把 `workspace` 指到单个项目，永远不要指 `$HOME`。
- 保持默认的 localhost 绑定。
- Cloudflare Tunnel 的 Host 头需要 `--allow-any-host`，这会放宽 DNS 重绑定防护。请同时使用 `--auth-token`——公网 URL 现在可以启动 OpenCode。
- 对公网部署使用认证（`--auth-token` 或配置里的 `auth_token`）。
- 连接大脑之前，先确认 `list_directory` 能看到哪些文件。
- 大脑不能写文件，也不能跑 shell。它可以启动配置好的 OpenCode 执行者。那个进程是你自己的。

这不是多租户安全产品。详见 [docs/security.md](docs/security.md)。

---

## 这不是什么

AgentBridge **不是又一个编码智能体**。

它是现有智能体之间的桥：

- 大脑：Gemini、Claude、ChatGPT……
- 执行者：OpenCode、Codex、Claude Code……

V0.2 不包含 Web UI、账号、数据库、通过 MCP 执行 shell、通过 MCP 改文件，或 Codex / Claude Code 适配器。

---

## 文档

| 文档 | 内容 |
|-----|--------|
| [docs/architecture.md](docs/architecture.md) | 分层与信任边界 |
| [docs/security.md](docs/security.md) | 隔离、密钥、隧道 |
| [docs/gemini.md](docs/gemini.md) | Gemini / AI Studio 远程 MCP |
| [docs/opencode.md](docs/opencode.md) | 执行者工作流 |
| [skill/SKILL.md](skill/SKILL.md) | 给大脑的说明 |

---

## 开发

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

需要 Rust 1.88+，以及 PATH 上的 `git`。

## 许可证

MIT
