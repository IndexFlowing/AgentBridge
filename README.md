# AgentBridge

**Use one AI to think and another AI to code.**

AgentBridge connects AI reasoning environments to local coding agents through **MCP (Model Context Protocol)**, allowing a web-based AI to understand and plan changes while a local coding agent executes them in your workspace.

> **Let the AI with the best reasoning access think. Let the coding agent you already use execute.**

[🇨🇳 中文](README_zh.md) · [📦 crates.io](https://crates.io/crates/agentbridge) · [🐙 GitHub](https://github.com/IndexFlowing/AgentBridge)

[![Crates.io](https://img.shields.io/crates/v/agentbridge.svg)](https://crates.io/crates/agentbridge)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

---

## Why AgentBridge?

AI coding tools increasingly combine two different jobs:

* **Reasoning** — understanding a codebase, investigating problems, designing solutions, and reviewing changes.
* **Execution** — editing files, running commands, running tests, and applying changes.

These two jobs do not necessarily need to be performed by the same AI.

You may have a web AI with generous usage and strong reasoning capabilities (such as Claude 3.7 Sonnet, o3-mini, or Gemini 2.5 Pro), while your local coding CLI has a more limited subscription quota.

Without a bridge, the coding agent has to spend its quota on everything:

```text
Understand → Explore → Reason → Plan → Code → Test → Review
```

AgentBridge separates the workflow:

```text
                 BRAIN
          Web-based AI
       Gemini / Claude / ...
                 │
          Reason / Plan
                 │
                MCP
                 ▼
        ┌─────────────────┐
        │   AgentBridge   │ (Port 8040)
        └────────┬────────┘
                 │
             C2C PLAN
                 │
                 ▼
             EXECUTOR
          Local coding agent
             OpenCode
                 │
        Edit / Run / Test
                 │
                 ▼
       Local project(s)
```

The result is simple:

> **Use the AI that is best at thinking, and the coding agent that is best at doing.**

---

## The Core Idea

AgentBridge introduces two explicit roles.

### 🧠 Brain

The **Brain** is the AI responsible for reasoning.

It can:

* list and switch among mounted local projects;
* inspect the project and directory structure;
* search the codebase;
* read files;
* understand architecture and investigate bugs;
* create structured implementation plans (C2C PLAN);
* trigger execution via `task_start`;
* inspect Git diffs and test results;
* review the Executor's work.

The Brain interacts with the workspace through AgentBridge's **read-only MCP interface**. It does not directly execute arbitrary shell commands or overwrite files.

### 🛠️ Executor

The **Executor** is the local coding agent responsible for execution.

It can:

* modify and create files;
* run build and test commands;
* implement the Brain's plan;
* report execution results.

AgentBridge currently features **OpenCode** as its primary Executor. The architecture is designed so additional coding agents (Claude Code, Aider, Codex) can be supported in the future.

### 🌉 AgentBridge

AgentBridge connects the two.

It provides:

* Streamable HTTP MCP server;
* MCP OAuth 2.1 for ChatGPT / Gemini custom MCP connections;
* multi-project workspace hosting in a single process;
* safe read-only project inspection (sandboxed per project);
* structured task delegation (C2C Protocol);
* autonomous task supervisor (`task_start`, `task_status`, `task_cancel`);
* live terminal output streaming (`mode = "stream"`);
* automatic reasoning token (`<think>`) stripping;
* Git diff & untracked file capture;
* test status reporting.

---

## How It Works

A typical workflow looks like this:

```text
User
 │
 ▼
Brain (Claude / ChatGPT / Gemini)
 │
 ├── list_projects / switch_project (when several repos are mounted)
 ├── Inspect workspace (workspace_info, search_workspace, read_file)
 ├── Understand architecture & formulate PLAN
 └── Call task_start
          │
          ▼
     AgentBridge (v0.4.0)
          │
       C2C PLAN
          │
          ▼
      Executor (OpenCode CLI)
          │
   ├── Edit files
   ├── Run commands
   └── Run tests
          │
          ▼
    Git Diff / Result
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

This creates an autonomous feedback loop:

```text
PLAN → EXECUTE → REVIEW → DONE
             ↑            │
             └── PLAN ────┘
```

---

## MCP: Giving the Brain Access to Your Workspace

AgentBridge uses the **Model Context Protocol (MCP)** to expose your local project and task controls to the Brain.

### Inspection Tools (Read-Only & Safe)

| Tool                | Description                                                  |
| ------------------- | ------------------------------------------------------------ |
| `list_projects`     | List mounted workspaces (name, path, description, active)    |
| `switch_project`    | Change this session's active project                         |
| `workspace_info`    | Inspect workspace path, project types (Rust, Node, Python, Go), and Git repository state |
| `list_directory`    | Traversal-safe structured directory listing                  |
| `read_file`         | Read UTF-8 files (size-capped, binaries & secrets denied)    |
| `search_workspace`  | High-speed keyword search with smart ignores (`node_modules`, `target`, `.git`) |
| `git_status`        | Inspect branch, changed, staged, and untracked files         |
| `git_diff`          | Working tree or staged diff (automatically formats newly-created untracked files) |
| `test_status`       | Read the latest recorded test execution result               |
| `execution_summary` | Read structured summary of the latest iteration              |

### Execution & Control Tools (Autonomous)

| Tool          | Description                                                  |
| ------------- | ------------------------------------------------------------ |
| `task_start`  | Spawns local OpenCode with a validated `C2cPlan`             |
| `task_status` | Polls task lifecycle: `running` \| `success` \| `failed` \| `cancelled` |
| `task_cancel` | Terminates the running executor process tree safely          |

Inspection and executor tools accept an optional `project` argument. If omitted, they use the session's active project (set by `switch_project`, or the configured default). Paths cannot escape that project's root.

---

## C2C: Brain-to-Executor Communication

AgentBridge standardizes agent communication using lightweight **C2C (Context-to-Context)** messages:

```text
[C2C]
STATE: PLAN
TASK_ID: c2c_20260830_001
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

The source code remains in the local workspace. Only the structured task contract crosses the boundary, minimizing context token consumption.

---

## Quick Start

### 1. Install

Install via Cargo:

```bash
cargo install agentbridge
```

Verify the installation:

```bash
agentbridge --version
```

You can also download pre-built binaries from [GitHub Releases](https://github.com/IndexFlowing/AgentBridge/releases).

#### Build from source:

```bash
git clone https://github.com/IndexFlowing/AgentBridge.git
cd AgentBridge

cargo install --path .
```

### 2. Initialize Your Project

```bash
cd /path/to/your/project
agentbridge init . --port 8040
```

`init` writes `.agentbridge.toml`. Auth fields start empty (ignored). Set a stable OAuth PIN in that file:

```toml
admin_password = "your-pin-here"
```

AgentBridge does not create or read a project `.env` — that file belongs to the app (for example a Rust service).

Check your environment:

```bash
agentbridge doctor
```

### 3. Start AgentBridge

CLI:

```bash
agentbridge serve
```

Or open the tray console (start/stop, PIN, projects, boot autostart):

```bash
agentbridge tray
```

By default, AgentBridge listens on:

```text
http://127.0.0.1:8040/mcp
```

OAuth 2.1 is enabled by default. If `.agentbridge.toml` has `admin_password`, that PIN is used. Otherwise the startup banner prints a generated **Admin PIN** for `/oauth/authorize`.

```text
➜  Workspaces  : [default] (1 mounted)
➜  MCP Endpoint: http://127.0.0.1:8040/mcp (Streamable HTTP)
➜  Auth        : OAuth 2.1 Enabled (/oauth/authorize)
➜  Admin PIN   : a1b2-c3d4-e5f6
```

Use `--dev` / `--no-auth` only for trusted localhost debugging.

---

## Tray console

`agentbridge tray` opens a desktop control panel (system tray + settings window):

* start / stop the MCP server
* copy MCP URL and Admin PIN
* edit host, port, `allow_any_host`, Admin password
* add/remove project folders (writes `agentbridge.config.json`)
* start with Windows (login item)

Closing the window hides it to the tray; use **退出** on the tray menu to quit. Config is still `.agentbridge.toml` — the UI does not replace the CLI.

### CLI/core first

Executor discovery, configuration, version probing, and task execution are Rust core capabilities. The desktop application is an optional control panel; Linux, SSH, and headless use do not require it. Use `agentbridge doctor`, `agentbridge status`, `agentbridge serve`, and `agentbridge task ...` from a terminal.

Executor display names are labels only. The command, executable path, executor type, and stable ID remain the runtime identity. PATH discovery is shown separately from saved configuration and is based on a command plus `--version` probe; refreshing discovery never overwrites saved entries.

The complete headless management surface is available after `cargo install agentbridge`:

| Command | Purpose |
| --- | --- |
| `init`, `serve`, `status`, `doctor` | Configure, run, inspect, and diagnose the MCP service |
| `project list\|add\|remove` | Manage `agentbridge.config.json` projects |
| `executor list\|add\|remove\|test` | Discover, persist, remove, and probe executors in `executors.toml` |
| `proxy show\|set\|test` | Configure and test the executor proxy in `.agentbridge.toml` |
| `task start\|executed\|status\|cancel` | Run and record the task lifecycle without a desktop session |
| `tray` | Optional desktop control panel |

All management commands are file-based and do not require a GUI or database. They work from Windows, macOS, Linux, SSH, and headless shells. On Linux or SSH, use `serve`, `project`, `executor`, `proxy`, and `task`; `tray` is optional and may not be available on a headless display. Core behavior remains in Rust; the desktop application only presents controls and IPC.

### Linux Distribution + Service (Planned)

Linux currently supports the Rust CLI when installed with Cargo. A packaged Linux installation experience is not available yet. The planned distribution and service work will add:

* x86_64 and aarch64 Linux release artifacts;
* a `curl`-based installer for those release artifacts;
* `systemd` service installation and lifecycle management;
* a configuration-file-driven resident Core process;
* a GUI that is an optional management client and does not own the Core lifecycle.

These are roadmap items, not commands that can be used today. An APT repository is intentionally not planned at this stage. Until packaged releases and service integration are implemented, developers should use `cargo install agentbridge` and manage `agentbridge serve` directly.

---

## Multi-project workspaces

A single AgentBridge process can host several local repositories.

### Single directory

```bash
agentbridge serve D:\Project\MyProject
```

That directory becomes a project named `default`.

### Several repositories

Create `agentbridge.config.json` (see `examples/agentbridge.config.json`):

```json
{
  "projects": [
    {
      "name": "indexflow-core",
      "path": "D:\\Project\\IndexFlow\\IndexFlow-core",
      "description": "IndexFlow core backend and engine",
      "readonly": false
    },
    {
      "name": "mandarin-clips",
      "path": "D:\\Project\\MandarinClips",
      "description": "MandarinClips web platform and media tools",
      "readonly": false
    }
  ],
  "default_project": "indexflow-core"
}
```

Then:

```bash
agentbridge serve --workspaces agentbridge.config.json
```

If `agentbridge.config.json` is in the current directory, `agentbridge serve` picks it up automatically.

The Brain should call `list_projects` first, then either `switch_project` or pass `project` on individual tools. Each call is sandboxed to that project's root; `../` cannot reach a sibling repo. `task_start` is rejected on `readonly` projects.

---

## Authentication (OAuth 2.1)

ChatGPT and Gemini Web custom MCP connections expect the MCP OAuth 2.1 protected-resource flow. AgentBridge implements it in-process:

| Endpoint | Role |
| -------- | ---- |
| `GET /.well-known/oauth-protected-resource` | RFC 9728 resource metadata (`resource`, `authorization_servers`, `mcp:read` / `mcp:write`) |
| `GET /.well-known/oauth-authorization-server` | RFC 8414 discovery (`/oauth/authorize`, `/oauth/token`, PKCE S256) |
| `GET/POST /oauth/authorize` | Browser consent page (Admin PIN) → redirect with `code` and `state` |
| `POST /oauth/token` | Exchange `code` + `code_verifier` for `access_token` / `refresh_token` |
| `POST /oauth/register` | RFC 7591 dynamic client registration (used by ChatGPT / Gemini) |

Unauthenticated requests to `/mcp` return:

```http
HTTP/1.1 401 Unauthorized
WWW-Authenticate: Bearer realm="mcp", resource_metadata="https://<host>/.well-known/oauth-protected-resource"
```

A valid `Authorization: Bearer <token>` (OAuth access token **or** static `--auth-token`) grants access.

### Configuration

Fill the fields in `.agentbridge.toml` (empty = unused). CLI flags override the file, then optional process environment variables:

| toml / flag | Purpose |
| ----------- | ------- |
| `admin_password` / `--admin-password` | PIN on `/oauth/authorize` (generated if unset) |
| `client_id` / `--client-id` | Optional pre-registered OAuth client |
| `client_secret` / `--client-secret` | Optional client secret |
| `auth_token` / `--auth-token` | Extra static Bearer token |
| `no_auth` / `--no-auth` / `--dev` | Disable the 401 challenge (localhost only) |
| `allow_any_host` / `--allow-any-host` | Required for Cloudflare Tunnel `Host` headers |

---

## Connecting Your Brain

### Option A: Claude Desktop (Zero-API, 100% Free)

Open `%APPDATA%\Claude\claude_desktop_config.json` (Windows) or `~/Library/Application Support/Claude/claude_desktop_config.json` (macOS):

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

*Restart Claude Desktop. The 🔨 icon will appear with all AgentBridge tools loaded.*

For local Claude Desktop you can start with `agentbridge serve --dev` so the proxy does not need a Bearer token.

### Option B: Remote Web AI via Cloudflare Tunnel (Gemini / ChatGPT)

```bash
agentbridge serve --allow-any-host
```

In another terminal, expose port 8040:

```bash
cloudflared tunnel --url http://127.0.0.1:8040
```

Point the remote MCP client at:

```text
https://<your-tunnel-id>.trycloudflare.com/mcp
```

1. ChatGPT / Gemini will receive `401` and start OAuth discovery.
2. Complete the in-browser `/oauth/authorize` page with the **Admin PIN** from the AgentBridge banner.
3. Alternatively, start with `--auth-token` and paste `Authorization: Bearer <token>` in the client.

Paste `skill/SKILL.md` into the system instructions / skill slot so the model behaves as the Brain (`list_projects`, inspect, `task_start` — never edit files itself).

---

## Configuration (`.agentbridge.toml`)

`agentbridge init` generates a project-level configuration:

```toml
workspace = "D:\\Project\\MyProject"
host = "127.0.0.1"
port = 8040
allow_any_host = false                    # Set true when routing via Cloudflare Tunnel
auth_token = ""                           # Optional static Bearer; empty = unused
admin_password = ""                       # OAuth authorize PIN; empty = generate at startup
client_id = ""                            # Optional pre-registered OAuth client; empty = DCR
client_secret = ""
no_auth = false                           # true disables the /mcp 401 challenge (localhost only)

[executor]
type = "opencode"
command = "opencode"                      # On Windows with global npm, use "opencode.cmd"
mode = "stream"                           # "stream" (live terminal output) or "silent" (quiet background)

[security]
max_file_size = 1048576                   # 1MB
deny_sensitive_files = true               # Denies .env, *.pem, *.key, id_rsa
max_diff_bytes = 65536                    # Truncates diffs over 64KB
```

Server listen settings still come from `.agentbridge.toml` (or `~/.agentbridge/config.toml`). Mounted repositories come from `agentbridge.config.json`, `--workspaces`, or `agentbridge serve <dir>`.

> 💡 **Windows Tip**: If OpenCode is installed globally via npm, set `command = "opencode.cmd"` (or the absolute path) to ensure Windows invokes the batch wrapper rather than the POSIX shell script (preventing `os error 193`).

---

## CLI Reference

```bash
# Initialize project workspace
agentbridge init <workspace> --port 8040

# Tray console: start/stop MCP, edit projects and OAuth, optional boot autostart
agentbridge tray

# Start MCP server (loads .agentbridge.toml / agentbridge.config.json)
agentbridge serve

# Single directory as project `default`
agentbridge serve D:\Project\MyProject --allow-any-host

# Multiple repositories
agentbridge serve --workspaces agentbridge.config.json

# Skip OAuth on localhost
agentbridge serve --dev

# Set the authorize-page PIN and an extra static Bearer token
agentbridge serve --admin-password "my-pin" --auth-token "$TOKEN"

# Inspect workspace, git, and executor status
agentbridge status

# Diagnose environment, git, and OpenCode installation
agentbridge doctor

# Manage projects without the desktop panel
agentbridge project list --workspaces agentbridge.config.json
agentbridge project add backend ./backend --default
agentbridge project remove backend

# Discover and manage executors
agentbridge executor list
agentbridge executor add "Local OpenCode" --kind opencode --command opencode
agentbridge executor test

# Configure and test the executor proxy
agentbridge proxy show
agentbridge proxy set --kind socks5 --host 127.0.0.1 --port 1080
agentbridge proxy test

# Manually start a task from CLI and execute synchronously
agentbridge task start --goal "..." --execute

# Cancel running executor task
agentbridge task cancel
```

---

## Security

AgentBridge is built with defensive defaults:

* **Explicit Read-Only View**: The remote Brain cannot execute arbitrary shell commands or overwrite files.
* **Deny-by-Default File Access**: Restricts path traversal (`../`) and sensitive file patterns (`.env`, `credentials`, `id_rsa`, `*.pem`).
* **Per-project sandbox**: Every tool call is confined to the selected project's root. Sibling workspaces are not reachable via `../` or an absolute path.
* **Process isolation**: The Executor is started with cwd set to that project directory.
* **Authentication**: MCP OAuth 2.1 (authorization code + PKCE, dynamic client registration) for ChatGPT / Gemini custom MCP, plus optional static Bearer `auth_token`. Unauthenticated `/mcp` returns `401` with `WWW-Authenticate`.
* **Public URLs**: Leave OAuth enabled (or set `--auth-token`) whenever you tunnel the endpoint. Use `--no-auth` / `--dev` only on trusted loopback.

See [docs/security.md](docs/security.md) for the full model.

---

## License

MIT License © [AgentBridge Contributors](LICENSE)
