# AgentBridge

[English](README.md) | [中文](README_zh.md)

**Use one AI to think and another AI to code.**

Connect AI brains to coding agents through MCP.

Use Gemini, Claude, or ChatGPT to inspect and reason about
your local project while OpenCode, Codex, or Claude Code
handles the actual implementation.

```
ChatGPT Web
    ↓ MCP
AgentBridge
    ↓ C2C PLAN
OpenCode CLI
    ↓
Local Workspace
    ↓ git_diff / tests
ChatGPT Web REVIEW
```

Your AI subscriptions don't have to be locked to one coding agent.
Use your favorite AI as the brain and your favorite coding agent as the hands.

---

### Brain

Plans, analyzes, reviews.

### Executor

Edits, runs commands, tests.

### MCP

Connects them without copying the project into prompts.

The Brain never writes files or executes commands. That is the point.

---

## Quick start

```bash
git clone https://github.com/agentbridge/agentbridge.git
cd agentbridge

cargo install --path .

agentbridge init ~/projects/my-project

agentbridge serve
```

MCP listens on `http://127.0.0.1:8787/mcp` (localhost only).

Optional — expose it to a web AI with [Cloudflare Tunnel](https://developers.cloudflare.com/cloudflare-one/connections/connect-apps/do-more-with-tunnels/trycloudflare/):

```bash
agentbridge serve --allow-any-host
```

In another terminal:

```bash
cloudflared tunnel --url http://127.0.0.1:8787
```

Point a compatible client (Gemini / AI Studio, Claude, ChatGPT, …) at:

```
https://<id>.trycloudflare.com/mcp
```

Install the Brain instructions from [`skill/SKILL.md`](skill/SKILL.md).

Cloudflare is optional. The core tool works entirely on localhost.

---

## Workflow

```
1. Start AgentBridge locally.
2. (Optional) Expose it through Cloudflare Tunnel. Use --auth-token.
3. Connect a remote MCP-capable AI client. Give it skill/SKILL.md.
4. Give the AI a coding task.
5. The Brain reads the workspace through MCP — not through pasted files.
6. The Brain produces a compact C2C PLAN and calls task_start.
7. AgentBridge starts OpenCode in the configured workspace.
8. OpenCode edits files and runs tests locally.
9. The Brain polls task_status, then reads git_diff / test_status / execution_summary.
10. The Brain writes a REVIEW.
11. DONE, PLAN again (task_start, iteration + 1), or BLOCKED.
```

C2C messages stay small. Diffs and source stay in MCP.

```
[C2C]
STATE: PLAN
TASK_ID: c2c_12345
ITERATION: 1

GOAL:
Add URL inspection to the GSC client.

ACTIONS:
1. Read the current client.
2. Add inspect().
3. Add tests.

TESTS:
cargo test

SUCCESS_CRITERIA:
Tests pass; the API reports indexed / not-indexed.
```

---

## CLI

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

`init` writes `~/.agentbridge/config.toml` and `<workspace>/.agentbridge.toml`:

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

`doctor` checks the workspace, git, OpenCode, MCP config, port, and optional `cloudflared`.

---

## MCP tools

Read-only inspection:

| Tool | Returns |
|------|---------|
| `workspace_info` | Path, project types, git? |
| `list_directory` | Structured listing |
| `read_file` | UTF-8 text (size-capped) |
| `search_workspace` | File, line, matching text |
| `git_status` | Branch, dirty, changed / staged / untracked |
| `git_diff` | Working tree or staged; truncated if huge |
| `test_status` | Last recorded test run (does not execute) |
| `execution_summary` | Last Executor result |

Executor control (no shell, no executable from the Brain):

| Tool | Returns |
|------|---------|
| `task_start` | `task_id` + `running` — starts OpenCode with a C2C PLAN |
| `task_status` | `running` / `success` / `failed` / `blocked` / `cancelled` |
| `task_cancel` | Stops the OpenCode process tree |

Path traversal (`../`, `/etc/passwd`, `C:\Users\...`, `~/.ssh`) is rejected.
`.env`, `*.pem`, `*.key`, `id_rsa`, and similar names are denied.

---

## Security warning

This project exposes a read-only view of your local workspace to a remote AI model.

- Do not expose secrets. Point `workspace` at one project, never `$HOME`.
- Keep the default localhost bind.
- `--allow-any-host` is required for Cloudflare Tunnel Host headers; it widens DNS-rebinding protection. Pair it with `--auth-token` — a public URL can start OpenCode.
- Use authentication for public deployments (`--auth-token` or `auth_token` in config).
- Review what `list_directory` can see before you connect a Brain.
- The Brain cannot write files or run a shell. It can start the configured OpenCode executor. That process is yours.

This is not a multi-tenant security product. Read [docs/security.md](docs/security.md).

---

## What this is not

AgentBridge is **not another coding agent**.

It is a bridge between existing agents:

- Brain: Gemini, Claude, ChatGPT, …
- Executor: OpenCode, Codex, Claude Code, …

V0.2 does not include a web UI, accounts, a database, shell-over-MCP, file editing over MCP, or Codex / Claude Code adapters.

---

## Docs

| Doc | Topic |
|-----|--------|
| [docs/architecture.md](docs/architecture.md) | Layers and trust boundary |
| [docs/security.md](docs/security.md) | Isolation, secrets, tunnels |
| [docs/gemini.md](docs/gemini.md) | Remote MCP in Gemini / AI Studio |
| [docs/opencode.md](docs/opencode.md) | Executor workflow |
| [skill/SKILL.md](skill/SKILL.md) | Instructions for the Brain |

---

## Develop

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

Requires Rust 1.88+ and a `git` binary on PATH.

## License

MIT
