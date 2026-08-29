# AgentBridge

**Use one AI to think and another AI to code.**

AgentBridge connects AI reasoning environments to local coding agents through **MCP**, allowing a web-based AI to understand and plan changes while a local coding agent executes them in your workspace.

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

You may have a web AI with generous usage and strong reasoning capabilities, while your coding CLI has a more limited subscription quota.

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
        │   AgentBridge   │
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

* inspect the project;
* search the codebase;
* understand architecture;
* investigate bugs;
* design implementation strategies;
* create implementation plans;
* inspect Git diffs;
* review the Executor's work.

The Brain interacts with the workspace through AgentBridge's **read-only MCP interface**.

It does not directly modify files or execute shell commands.

### 🛠️ Executor

The **Executor** is the local coding agent responsible for execution.

It can:

* modify files;
* run commands;
* run tests;
* implement the Brain's plan;
* report execution results.

AgentBridge currently uses **OpenCode** as its Executor.

The architecture is designed so additional coding agents can be supported in the future.

### 🌉 AgentBridge

AgentBridge connects the two.

It provides:

* MCP-based workspace access;
* read-only project inspection;
* structured task delegation;
* task lifecycle management;
* execution status;
* Git diff inspection;
* test status;
* result reporting.

---

## How It Works

A typical workflow looks like this:

```text
User
 │
 ▼
Brain
 │
 ├── Inspect workspace
 ├── Understand architecture
 ├── Investigate problem
 └── Create PLAN
          │
          ▼
     AgentBridge
          │
       C2C PLAN
          │
          ▼
      Executor
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
       Review
          │
     ┌────┴────┐
     │         │
    DONE    PLAN AGAIN
```

This creates a feedback loop:

```text
PLAN → EXECUTE → REVIEW → DONE
             ↑            │
             └── PLAN ────┘
```

The Brain can therefore focus on high-value reasoning while the Executor focuses on actually changing the codebase.

---

## Why This Can Save Coding-Agent Quota

AI coding subscriptions are often metered differently from normal web or chat usage.

A coding agent may consume its allowance while:

* exploring the repository;
* reading files;
* searching for definitions;
* understanding architecture;
* reasoning about an implementation;
* generating a plan;
* implementing changes;
* running tests;
* retrying failed implementations.

That means a significant amount of coding-agent usage can happen **before the first useful code change**.

AgentBridge lets you move much of the exploratory and reasoning-heavy work to another AI interface.

For example:

```text
Gemini Web
    │
    │ understand / reason / plan
    ▼
AgentBridge
    │
    │ compact implementation plan
    ▼
OpenCode CLI
    │
    │ implement / test
    ▼
Your repository
```

The goal is not to bypass quotas.

AgentBridge:

* does not provide additional model credits;
* does not bypass provider limits;
* does not access paid models without authorization;
* does not proxy model APIs.

It simply allows you to **use the AI services and coding agents you already have more efficiently**.

---

## MCP: Giving the Brain Access to Your Workspace

AgentBridge uses the **Model Context Protocol (MCP)** to expose your local project to the Brain.

The Brain can use tools such as:

| Tool                | Purpose                                     |
| ------------------- | ------------------------------------------- |
| `workspace_info`    | Inspect workspace information and Git state |
| `list_directory`    | Explore the project structure               |
| `read_file`         | Read files                                  |
| `search_workspace`  | Search source code                          |
| `git_status`        | Inspect repository status                   |
| `git_diff`          | Review changes                              |
| `test_status`       | Read the latest test result                 |
| `execution_summary` | Read the latest Executor result             |

The Brain does not receive a generic shell interface.

It also cannot directly write files.

This separation is intentional.

---

## C2C: Brain-to-Executor Communication

AgentBridge uses a small structured protocol called **C2C (Context-to-Context)** to communicate between the Brain and Executor.

Instead of passing an entire repository or a huge conversation to the coding agent, the Brain creates a compact implementation plan:

```text
[C2C]
STATE: PLAN
TASK_ID: c2c_12345
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

The source code remains in the local workspace.

Only the task intent and implementation contract cross the Brain → Executor boundary.

This keeps the communication focused and avoids unnecessarily duplicating the entire project context.

---

## Read-Only Brain, Write-Capable Executor

One of the most important design decisions in AgentBridge is the trust boundary.

The Brain can inspect:

```text
workspace
├── source files
├── project structure
├── Git status
├── Git diff
└── test results
```

But it cannot:

```text
✗ write files
✗ delete files
✗ execute shell commands
✗ commit
✗ push
```

The Executor is the component that performs those actions.

```text
                 Read-only
                    │
                    ▼
              ┌───────────┐
              │   Brain   │
              └─────┬─────┘
                    │
                  PLAN
                    │
                    ▼
              ┌───────────┐
              │ Executor  │
              └─────┬─────┘
                    │
             Write / Execute
                    │
                    ▼
              Local Project
```

The Brain decides **what should happen**.

The Executor performs **the implementation inside the local coding environment**.

---

## Quick Start

### 1. Install

The easiest way to install AgentBridge is through Cargo:

```bash
cargo install agentbridge
```

Verify the installation:

```bash
agentbridge --version
```

You can also download pre-built binaries from [GitHub Releases](https://github.com/IndexFlowing/AgentBridge/releases).

### Build from source

```bash
git clone https://github.com/IndexFlowing/AgentBridge.git
cd AgentBridge

cargo install --path .
```

### 2. Start AgentBridge

Start the MCP server:

```bash
agentbridge serve
```

By default, AgentBridge listens on:

```text
http://127.0.0.1:8787/mcp
```

The default configuration is intentionally localhost-only.

You can check your environment with:

```bash
agentbridge doctor
```

![Start AgentBridge](images/start_server.jpg)

### 3. Connect Your Brain

Connect an MCP-capable AI client to AgentBridge.

Your Brain can then inspect the local workspace through the MCP tools.

![Connect MCP](images/connect_mcp.jpg)

The Brain instructions are provided in:

```text
skill/SKILL.md
```

The skill teaches the Brain how to:

1. inspect the workspace;
2. understand the task;
3. create a C2C PLAN;
4. delegate the task;
5. monitor execution;
6. inspect the result;
7. review the changes;
8. finish or create another iteration.

### 4. Give the Brain a Task

For example:

```text
Add Google Search Console URL inspection support to this project.

First understand the existing architecture.
Then create an implementation plan and delegate it to the Executor.
After implementation, review the diff and test results.
```

The Brain can inspect the actual repository instead of relying on files pasted into the conversation.

---

## Using a Web-Based Brain

If your Brain runs in a web environment and cannot directly access localhost, you can expose AgentBridge through a tunnel.

For example, with Cloudflare Tunnel:

```bash
agentbridge serve --allow-any-host
```

Then:

```bash
cloudflared tunnel --url http://127.0.0.1:8787
```

Your MCP endpoint will be available at:

```text
https://<your-tunnel-id>.trycloudflare.com/mcp
```

AgentBridge itself remains a local application.

> **Security:** If you expose AgentBridge outside localhost, use authentication and carefully choose which workspace is exposed. Do not point AgentBridge at your entire home directory.

---

## CLI

```bash
agentbridge init <workspace>

agentbridge serve

agentbridge status

agentbridge doctor

agentbridge task start --goal "..."

agentbridge task executed \
  --status success \
  --tests "cargo test" \
  --exit-code 0
```

Run:

```bash
agentbridge doctor
```

to check your local AgentBridge environment.

---

## Configuration

Global configuration:

```text
~/.agentbridge/config.toml
```

Workspace-specific configuration:

```text
<workspace>/.agentbridge.toml
```

Example:

```toml
workspace = "/absolute/path/to/project"
host = "127.0.0.1"
port = 8787

[security]
max_file_size = 1048576
deny_sensitive_files = true
```

---

## Security

AgentBridge is designed around a simple security model:

> **The Brain receives a read-only view of the workspace you explicitly expose.**

Path traversal and sensitive files are restricted.

Examples of protected paths and patterns include:

```text
../
/etc/passwd
C:\Users\...
~/.ssh
.env
*.pem
*.key
id_rsa
```

Recommended practices:

* Point AgentBridge at a single project.
* Do not expose your home directory.
* Keep the default localhost binding whenever possible.
* If you expose the server remotely, configure authentication.
* Treat a remote Brain as an external service with access to the workspace you expose.

AgentBridge is a local developer tool, not a multi-tenant security boundary.

---

## Architecture

AgentBridge is intentionally built around clear boundaries:

```text
AgentBridge
│
├── MCP Server
│   └── Exposes workspace inspection tools
│
├── Workspace
│   └── Secure filesystem access
│
├── Task Runtime
│   └── PLAN → EXECUTE → RESULT lifecycle
│
├── C2C Protocol
│   └── Brain → Executor communication
│
├── Executor
│   └── Runs the local coding agent
│
└── Git / State
    └── Tracks changes and execution results
```

The key boundary is:

```text
             Remote Brain
                  │
               MCP API
                  │
          ┌───────▼───────┐
          │  AgentBridge  │
          └───────┬───────┘
                  │
              C2C PLAN
                  │
          ┌───────▼───────┐
          │    Executor   │
          └───────┬───────┘
                  │
             Local process
                  │
          ┌───────▼───────┐
          │    Workspace  │
          └───────────────┘
```

---

## What AgentBridge Is Not

AgentBridge is **not another AI coding agent**.

It does not try to replace:

* Gemini
* ChatGPT
* Claude
* OpenCode
* Codex
* your editor
* your existing development workflow

Instead, it connects them.

AgentBridge does not:

* provide AI models;
* provide model credits;
* bypass subscription limits;
* proxy model APIs;
* upload your repository to a hosted service.

It is a **local bridge between AI reasoning and local code execution**.

---

## Current Status

AgentBridge is currently focused on the Brain / Executor workflow.

Current capabilities include:

* Rust-based local MCP server
* workspace inspection
* secure path handling
* Git status and diff inspection
* structured C2C task protocol
* task lifecycle management
* OpenCode Executor integration
* execution status and result reporting
* Brain skill instructions
* local-first architecture

The Executor abstraction is designed to support additional coding agents as the project evolves.

---

## Roadmap

Potential future directions include:

* Additional Executor backends
* Executor selection and routing
* Better task orchestration
* Parallel task execution
* Context optimization
* Persistent task history
* More Brain integrations
* IDE integration
* Richer review workflows

The goal is not to build another monolithic AI coding product.

The goal is to make the AI coding stack **composable**.

---

## Development

Requirements:

```text
Rust 1.88+
Git
```

Run:

```bash
cargo fmt --check

cargo clippy --all-targets --all-features -- -D warnings

cargo test

cargo build --release
```

---

## Philosophy

AI coding does not have to be a single-agent problem.

Different AI products have different strengths, interfaces, context windows, pricing models, and usage limits.

Instead of forcing one agent to handle everything, AgentBridge treats AI coding as a distributed workflow:

```text
        THINK
          │
          ▼
        PLAN
          │
          ▼
      EXECUTE
          │
          ▼
       REVIEW
          │
          ▼
         DONE
```

**Use the AI that is best at thinking.**

**Use the coding agent that is best at doing.**

**Use AgentBridge to connect them.**

---

## Contributing

Contributions are welcome.

If you want to add a new Executor, improve the MCP interface, or enhance the Brain / Executor workflow, feel free to open an issue or pull request.

---

## License

MIT License.
