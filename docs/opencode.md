# OpenCode (Executor)

OpenCode is the V0.2 Executor. ChatGPT (the Brain) plans and reviews through MCP. AgentBridge starts OpenCode against the configured workspace. OpenCode edits files and runs tests. AgentBridge records a compact result. The Brain never writes files.

```
ChatGPT Web
      ↓ MCP
AgentBridge
      ↓ C2C PLAN
OpenCode CLI
      ↓
Local Workspace
      ↓
AgentBridge (status / git_diff / tests)
      ↓ MCP
ChatGPT Web REVIEW
```

OpenCode does not need to know the AgentBridge protocol. AgentBridge translates a validated `C2cPlan` into an `opencode run` prompt.

## Config

```toml
[executor]
type = "opencode"
command = "opencode"
```

`type` is allowlisted (`opencode`, `codex`, `claude`). V0.2 implements OpenCode only. `command` comes from this file, never from an MCP request.

## Loop

1. Brain inspects the repo through read-only MCP tools.
2. Brain calls `task_start` with `goal` + `plan.actions` / `tests` / `success_criteria`.
3. AgentBridge writes `.agentbridge/current.c2c` and starts:

   ```
   opencode run --auto "Read .agentbridge/current.c2c and implement that PLAN. ..."
   ```

   Working directory is the configured workspace. Arguments are structured (no shell string). The full PLAN stays in `current.c2c` so Windows `.cmd` wrappers are not given multiline argv.

4. OpenCode inspects, edits, and runs the listed tests.
5. AgentBridge captures `exit_code`, a short `summary`, `tests`, and `changed_files` (from git). Internal reasoning is discarded.
6. Brain polls `task_status`, then reads `git_diff`, `test_status`, and `execution_summary`.
7. Brain emits REVIEW: DONE, another PLAN, or BLOCKED.

## CLI (optional, same executor)

```bash
agentbridge task start --goal "Create TEST.md" --execute
agentbridge task status
agentbridge task cancel
```

`--execute` starts OpenCode in the foreground and waits. Without it, `task start` only writes the PLAN (useful if you run OpenCode yourself).

Manual recording still works:

```bash
agentbridge task executed --status success --tests "cargo test" --exit-code 0
```

## What OpenCode must not do

The Brain owns planning and review. Do not let OpenCode silently rewrite the GOAL. If it cannot proceed, the Brain emits `BLOCKED`.

## Doctor

`agentbridge doctor` reports:

```
OpenCode installed: yes/no
```

## Security

- OpenCode's cwd is the configured workspace.
- MCP cannot choose the executable or a shell command.
- `--auth-token` is required if the MCP URL is public (Cloudflare Tunnel).
- AgentBridge does not sandbox OpenCode; it is a local process you already trust with the repo.
