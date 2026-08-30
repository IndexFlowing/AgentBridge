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
        │   AgentBridge   │ (Port 8030)
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
          Local Workspace
```

The result is simple:

> **Use the AI that is best at thinking, and the coding agent that is best at doing.**

---

## The Core Idea

AgentBridge introduces two explicit roles.

### 🧠 Brain

The **Brain** is the AI responsible for reasoning.

It can:

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
* safe read-only project inspection;
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
 ├── Inspect workspace (workspace_info, search_workspace, read_file)
 ├── Understand architecture & formulate PLAN
 └── Call task_start
          │
          ▼
     AgentBridge (v0.2.3)
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
agentbridge init . --port 8030
```

Check your environment:

```bash
agentbridge doctor
```

### 3. Start AgentBridge

```bash
agentbridge serve
```

By default, AgentBridge listens on:

```text
http://127.0.0.1:8030/mcp
```

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
        "http://127.0.0.1:8030/mcp"
      ]
    }
  }
}
```

*Restart Claude Desktop. The 🔨 icon will appear with all AgentBridge tools loaded.*

### Option B: Remote Web AI via Cloudflare Tunnel (Gemini / ChatGPT)

In `.agentbridge.toml`, set `allow_any_host = true`, then start:

```bash
agentbridge serve
```

In another terminal, expose port 8030:

```bash
cloudflared tunnel --url http://127.0.0.1:8030
```

Point any remote MCP-compatible client to `https://<your-tunnel-id>.trycloudflare.com/mcp`.

---

## Configuration (`.agentbridge.toml`)

`agentbridge init` generates a project-level configuration:

```toml
workspace = "D:\\Project\\MyProject"
host = "127.0.0.1"
port = 8030
allow_any_host = false                    # Set true when routing via Cloudflare Tunnel
auth_token = "optional_bearer_token"      # Recommended for public tunnels

[executor]
type = "opencode"
command = "opencode"                      # On Windows with global npm, use "opencode.cmd"
mode = "stream"                           # "stream" (live terminal output) or "silent" (quiet background)

[security]
max_file_size = 1048576                   # 1MB
deny_sensitive_files = true               # Denies .env, *.pem, *.key, id_rsa
max_diff_bytes = 65536                    # Truncates diffs over 64KB
```

> 💡 **Windows Tip**: If OpenCode is installed globally via npm, set `command = "opencode.cmd"` (or the absolute path) to ensure Windows invokes the batch wrapper rather than the POSIX shell script (preventing `os error 193`).

---

## CLI Reference

```bash
# Initialize project workspace
agentbridge init <workspace> --port 8030

# Start MCP server (loads settings from .agentbridge.toml)
agentbridge serve

# Inspect workspace, git, and executor status
agentbridge status

# Diagnose environment, git, and OpenCode installation
agentbridge doctor

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
* **Process Sandboxing**: The Executor is strictly bounded to the workspace directory.
* **Authentication**: Supports Bearer Token authorization (`auth_token`) for public deployments.

---

## License

MIT License © [AgentBridge Contributors](LICENSE)
