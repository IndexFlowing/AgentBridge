# AgentBridge

**Decouple AI reasoning from code execution.**

Use one AI as the **Brain** and another as the **Executor**.

AgentBridge connects web-based AI assistants such as **Gemini, ChatGPT, and Claude** to local coding agents such as **OpenCode** through MCP, allowing them to collaborate on the same local workspace.

> **Let the AI with the best reasoning access think. Let the coding agent you already pay for execute.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

---

## Why AgentBridge?

Modern AI coding tools often combine two very different jobs:

1. **Reasoning** — understanding the codebase, investigating problems, designing an implementation, reviewing changes.
2. **Execution** — editing files, running commands, running tests, and applying the implementation.

But the AI product that is best for reasoning is not necessarily the AI product you want to spend your coding-agent quota on.

For example, you may have:

* a web AI account with a generous usage allowance and excellent reasoning;
* a coding CLI subscription with more limited agent usage;
* a local coding workspace where the actual implementation needs to happen.

Without a bridge, you have to choose one AI and use its quota for everything.

**AgentBridge separates these responsibilities.**

```text
             REASONING
        ┌─────────────────┐
        │   Web AI        │
        │ Gemini / Claude  │
        │ ChatGPT / ...   │
        └────────┬────────┘
                 │
                 │ MCP
                 ▼
        ┌─────────────────┐
        │  AgentBridge    │
        │                 │
        │  Read workspace │
        │  Create PLAN    │
        │  Review result  │
        └────────┬────────┘
                 │
             C2C PLAN
                 │
                 ▼
        ┌─────────────────┐
        │    Executor     │
        │    OpenCode     │
        │      / ...      │
        └────────┬────────┘
                 │
          edit / test / run
                 │
                 ▼
        ┌─────────────────┐
        │ Local Workspace │
        └─────────────────┘
```

The important idea is simple:

> **Don't make one AI do everything. Let each AI do the job it is best suited for.**

---

## The Core Concept

AgentBridge introduces two explicit roles.

### 🧠 Brain

The Brain is the AI you use for **reasoning**.

It can:

* inspect your project;
* search the codebase;
* understand architecture;
* investigate bugs;
* design implementation strategies;
* create implementation plans;
* review the changes made by the Executor.

The Brain **does not modify your files**.

It only sees the workspace through AgentBridge's read-only MCP interface.

### 🛠️ Executor

The Executor is the coding agent responsible for **execution**.

It can:

* modify files;
* run commands;
* run tests;
* implement the Brain's plan;
* report the execution result.

In the current version, AgentBridge uses **OpenCode** as the Executor.

The architecture is intentionally designed around an Executor interface so additional coding agents can be integrated later.

### 🌉 AgentBridge

AgentBridge is the layer between them.

It provides:

* MCP access to the local workspace;
* read-only project inspection;
* structured task delegation;
* task lifecycle management;
* execution status;
* Git diff inspection;
* test status;
* result reporting.

The Brain never needs to receive your entire repository in its prompt.

Instead, it explores the real workspace through MCP.

---

# The Workflow

A typical AgentBridge workflow looks like this:

```text
1. User gives a coding task
          │
          ▼
2. Brain inspects the workspace
          │
          ▼
3. Brain reasons about the problem
          │
          ▼
4. Brain creates a compact PLAN
          │
          ▼
5. AgentBridge sends the PLAN to Executor
          │
          ▼
6. Executor edits files and runs tests
          │
          ▼
7. AgentBridge records the result
          │
          ▼
8. Brain inspects git diff + test status
          │
          ▼
9. Brain reviews the implementation
          │
       ┌──┴───────────────┐
       │                  │
      DONE             PLAN AGAIN
                          │
                          ▼
                    another iteration
```

This creates a simple feedback loop:

```text
PLAN → EXECUTE → REVIEW → DONE
             ↑            │
             └── PLAN ────┘
```

The Brain can therefore remain focused on high-value reasoning while the Executor handles the mechanical work of changing the codebase.

---

# Why This Can Save AI Coding Quota

AI coding subscriptions are often metered differently from normal chat or web usage.

A coding agent may consume quota while:

* reading files;
* exploring the repository;
* searching for definitions;
* reasoning about architecture;
* generating an implementation;
* editing files;
* running tests;
* retrying failed implementations.

This means a single coding agent can spend a significant amount of its allowance **before it even starts writing useful code**.

AgentBridge allows you to move the exploratory and reasoning-heavy part to another AI interface.

For example:

```text
Gemini Web
    │
    │ reasoning / architecture / planning
    ▼
AgentBridge
    │
    │ compact implementation plan
    ▼
OpenCode CLI
    │
    │ implementation / tests
    ▼
Your repository
```

Instead of spending your coding-agent quota on the entire conversation, you can reserve it primarily for the part that actually requires local execution.

### The goal is not to bypass quotas.

AgentBridge does not provide additional model credits, circumvent provider limits, or access paid models without authorization.

It simply lets you **route work between the AI services and coding agents you already use**.

---

# A Different Way to Think About AI Coding

Most AI coding workflows look like this:

```text
User
  │
  ▼
One AI
  │
  ├── understand project
  ├── reason
  ├── plan
  ├── edit files
  ├── run tests
  └── review
```

AgentBridge turns it into:

```text
User
  │
  ▼
Brain
  │
  ├── understand
  ├── reason
  ├── plan
  └── review
       │
       ▼
   AgentBridge
       │
       ▼
   Executor
       │
       ├── edit
       ├── run
       └── test
```

This is the fundamental design principle behind AgentBridge:

> **Reasoning and execution are separate resources.**

You can choose the best AI for each one.

---

# MCP as the Bridge

AgentBridge uses the **Model Context Protocol (MCP)** to connect the Brain to your local workspace.

The Brain gets structured tools for inspecting the project:

| Tool                | Purpose                              |
| ------------------- | ------------------------------------ |
| `workspace_info`    | Project information and Git status   |
| `list_directory`    | Explore the workspace                |
| `read_file`         | Read a file                          |
| `search_workspace`  | Search source code                   |
| `git_status`        | Inspect repository state             |
| `git_diff`          | Review Executor changes              |
| `test_status`       | Read the latest recorded test result |
| `execution_summary` | Read the Executor result             |

The Brain does **not** receive a generic shell interface.

It also cannot directly write files.

This separation is intentional.

---

# C2C: Brain-to-Executor Protocol

AgentBridge uses a small structured protocol called **C2C (Context-to-Context)** to communicate between the Brain and Executor.

Instead of sending source code or a huge conversation to the coding agent, the Brain produces a compact implementation plan:

```text
[C2C]
STATE: PLAN
TASK_ID: c2c_12345
ITERATION: 1

GOAL:
Add URL inspection to the GSC client.

ACTIONS:
1. Inspect the current GSC client.
2. Add URL inspection support.
3. Add tests for indexed and non-indexed URLs.

TESTS:
cargo test

SUCCESS_CRITERIA:
Tests pass and the API correctly reports indexed / not-indexed.
```

The source code stays in the local workspace.

Only the intent and implementation contract cross the Brain → Executor boundary.

This has two important benefits:

1. **Lower context overhead**
2. **Clear separation of responsibility**

The Brain does not need to paste your repository into the Executor's prompt.

The Executor works directly against the same local workspace.

---

# Read-Only Brain, Write-Capable Executor

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

This gives the architecture a clear separation:

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

The Executor decides **how to execute the supplied plan inside the coding environment**.

---

# Quick Start

## 1. Install

Requirements:

* Rust 1.88+
* Git
* OpenCode
* An MCP-capable AI client

Clone and install AgentBridge:

```bash
git clone https://github.com/IndexFlowing/AgentBridge.git
cd AgentBridge

cargo install --path .
```

---

## 2. Initialize a Workspace

Point AgentBridge at the project you want the Brain to inspect:

```bash
agentbridge init ~/projects/my-project
```

This creates the AgentBridge configuration and associates the MCP server with that workspace.

---

## 3. Start the MCP Server

```bash
agentbridge serve
```

By default, AgentBridge listens on:

```text
http://127.0.0.1:8787/mcp
```

The default configuration is intentionally localhost-only.

---

## 4. Connect Your Brain

Connect a compatible MCP client such as Gemini, Claude, ChatGPT, or another MCP-capable AI client to AgentBridge.

Then provide the Brain instructions from:

```text
skill/SKILL.md
```

The skill teaches the AI how to:

1. inspect the workspace;
2. create a C2C PLAN;
3. delegate the task;
4. monitor execution;
5. inspect the result;
6. review the changes;
7. finish or create another iteration.

---

## 5. Give the Brain a Task

For example:

```text
Add Google Search Console URL inspection support to this project.

First understand the existing architecture.
Then create an implementation plan and delegate it to the Executor.
After implementation, review the diff and test results.
```

The Brain can inspect the actual repository rather than relying on files pasted into the conversation.

---

# Exposing AgentBridge to a Web AI

If your Brain runs in a web environment and cannot access localhost directly, you can expose AgentBridge through a tunnel.

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

Cloudflare Tunnel is **optional**.

AgentBridge itself works entirely locally.

> **Security:** If you expose AgentBridge outside localhost, use authentication and understand that the remote Brain can read the workspace exposed by the server. Never point AgentBridge at your entire home directory.

See [Security](docs/security.md) for details.

---

# CLI

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

Use:

```bash
agentbridge doctor
```

to check:

* workspace configuration;
* Git availability;
* MCP configuration;
* server port;
* optional Cloudflare Tunnel configuration;
* Executor availability.

---

# Configuration

AgentBridge stores global configuration in:

```text
~/.agentbridge/config.toml
```

and workspace-specific configuration in:

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

# Security

AgentBridge is designed around a simple security model:

**The remote Brain receives a read-only view of the configured workspace.**

Path traversal and sensitive files are explicitly restricted.

Examples of blocked paths include:

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

Important recommendations:

* Point AgentBridge at a single project.
* Do not expose your home directory.
* Keep the default localhost binding whenever possible.
* If you expose the server remotely, configure authentication.
* Treat the Brain as an external service with access to the workspace you explicitly expose.

AgentBridge is a local developer tool, not a multi-tenant security boundary.

See [`docs/security.md`](docs/security.md).

---

# Architecture

The project is intentionally small and has clear boundaries:

```text
AgentBridge
│
├── MCP Server
│   └── Exposes workspace inspection tools
│
├── Workspace
│   └── Filesystem access with security restrictions
│
├── Task Runtime
│   └── PLAN → EXECUTE → RESULT lifecycle
│
├── C2C Protocol
│   └── Compact Brain → Executor communication
│
├── Executor
│   └── Runs the local coding agent
│
└── Git / State
    └── Tracks changes and execution results
```

The important boundary is:

```text
             Remote Brain
                  │
               MCP API
                  │
          ┌───────▼───────┐
          │  AgentBridge   │
          └───────┬───────┘
                  │
              C2C PLAN
                  │
          ┌───────▼───────┐
          │    Executor    │
          └───────┬───────┘
                  │
             Local process
                  │
          ┌───────▼───────┐
          │    Workspace   │
          └────────────────┘
```

---

# What AgentBridge Is Not

AgentBridge is **not** another AI coding agent.

It does not attempt to replace:

* Gemini
* ChatGPT
* Claude
* OpenCode
* Codex
* your editor
* your existing development workflow

Instead, it connects them.

AgentBridge also does not:

* provide AI models;
* provide additional model credits;
* bypass subscription limits;
* proxy model APIs;
* upload your repository to a hosted service.

It is a **local bridge between AI reasoning and local code execution**.

---

# Current Status

AgentBridge is currently focused on the Brain / Executor workflow.

Current implementation:

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

# Roadmap

Potential future directions include:

* More Executor backends
* Better task orchestration
* Executor selection and routing
* Parallel task execution
* Improved context optimization
* Persistent task history
* More Brain integrations
* Better IDE integration
* Richer review workflows

The goal is not to build another monolithic AI coding product.

The goal is to make the AI coding stack **composable**.

---

# Development

```bash
cargo fmt --check

cargo clippy --all-targets --all-features -- -D warnings

cargo test

cargo build --release
```

Requires:

```text
Rust 1.88+
Git
```

---

# Philosophy

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

Use the AI that is best at **thinking**.

Use the coding agent that is best at **doing**.

Use AgentBridge to connect them.

---

# License

MIT
