# Architecture

AgentBridge is a local process. It is not a coding agent.

```
                Brain
       ┌─────────┼─────────┐
       │         │         │
    Gemini     Claude    ChatGPT
       │         │         │
       └─────────┼─────────┘
                 │
       MCP (inspect + task_start)
                 │
          ┌──────▼──────┐
          │ AgentBridge │
          └──────┬──────┘
                 │
              OpenCode
                 │
                 ▼
              Workspace
```

V0.2 proves one loop:

```
ChatGPT Web
        ↓  Remote MCP
   AgentBridge (Rust)
        ↓  C2C PLAN
     OpenCode CLI
        ↓  edits + tests
   Local Workspace
        ↑  git_diff / test_status
   AgentBridge
        ↓  REVIEW
   ChatGPT Web
```

## Layers

1. **MCP server** (`src/mcp.rs`) — Streamable HTTP tools. Inspection is read-only. `task_start` / `task_status` / `task_cancel` control the local Executor.
2. **Workspace** (`src/workspace.rs`) — Path isolation, file read, listing, search.
3. **C2C protocol** (`src/protocol.rs`) — Small PLAN/REVIEW messages. `C2cPlan` is the structured Brain → Executor payload.
4. **Executor** (`src/executor.rs`) — `OpenCodeExecutor` spawns `opencode run` with structured args in the workspace directory.
5. **Task runtime** (`src/task.rs`) — Lifecycle: created → planned → running → executed | failed | blocked | cancelled.
6. **Bridge** (`src/server.rs`) — `http://127.0.0.1:8787/mcp`. Cloudflare Tunnel is optional and external.

There is no Cloudflare logic in the MCP server. A tunnel is just a way to point a public HTTPS URL at localhost.

## Trust boundary

The Brain is a remote model. It must never receive:

- Arbitrary filesystem access
- A generic shell tool
- Write tools
- An executable name (OpenCode's command comes from local config)
- Secret files (`.env`, keys, `~/.ssh`, …)

The Executor is a local agent the user already trusts with the repo. AgentBridge starts it only inside the configured workspace, then records:

```
status, summary, exit_code, tests, changed_files, error
```

Internal model reasoning is not stored or returned.

## State

Task/test state lives at:

```
<workspace>/.agentbridge/state.json
<workspace>/.agentbridge/current.c2c
<workspace>/.agentbridge/executor.pid
```

The MCP server reads this from disk on every `test_status` / `execution_summary` / `task_status` call so the CLI and the Brain stay in sync without a database.
