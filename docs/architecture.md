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
            MCP (read-only)
                 │
          ┌──────▼──────┐
          │ AgentBridge │
          └──────┬──────┘
                 │
       ┌─────────┼─────────┐
       │         │         │
   OpenCode    Codex    Claude Code
                 │
                 ▼
              Workspace
```

V0.1 proves one loop:

```
Gemini Web / AI Studio
        ↓  Remote MCP
   AgentBridge (Rust)
        ↓  read-only tools
   Local Workspace
        ↑  edits + tests
     OpenCode
```

## Layers

1. **MCP server** (`src/mcp.rs`) — Streamable HTTP tools. Read-only.
2. **Workspace** (`src/workspace.rs`) — Path isolation, file read, listing, search.
3. **C2C protocol** (`src/protocol.rs`) — Small text messages between Brain and Executor.
4. **Bridge** (`src/server.rs`) — `http://127.0.0.1:8787/mcp`. Cloudflare Tunnel is optional and external.

There is no Cloudflare logic in the MCP server. A tunnel is just a way to point a public HTTPS URL at localhost.

## Trust boundary

The Brain is a remote model. It must never receive:

- Arbitrary filesystem access
- Shell execution
- Write tools
- Secret files (`.env`, keys, `~/.ssh`, …)

The Executor is a local agent the user already trusts with the repo. It writes files, runs tests, and records the outcome:

```bash
agentbridge task executed --status success --tests "cargo test" --exit-code 0
```

Native executor adapters (`ExecutorAdapter`) are a stub in V0.1. Recording results through the CLI is enough to close the review loop.

## State

Task/test state lives at:

```
<workspace>/.agentbridge/state.json
<workspace>/.agentbridge/current.c2c
```

The MCP server reads this from disk on every `test_status` / `execution_summary` call so the CLI and the Brain stay in sync without a database.
