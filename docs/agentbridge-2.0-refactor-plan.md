# AgentBridge 2.0 Refactor Plan

Status: **audit complete — waiting for confirmation before Phase 1**
Date: 2026-09-13
Current crate: `agentbridge` 1.0.4 (`Cargo.toml`)
Desktop crate: `agentbridge-desktop` 0.5.1 (changelog also 0.5.1)

This document is an architecture audit of the current repository, not a rewrite spec. Every claim below is based on current code. **Do not start implementation until this plan is confirmed.**

---

## Key Decisions (proposed)

1. **MCP and Web are control planes; Core owns all business logic.** Existing MCP tools already call `ProjectHub` / `TaskRuntime` / `Workspace`. Web must do the same. Do not add a second task/project implementation in axum handlers.
2. **Do not rewrite Task Runtime.** Persist by adding a `TaskRepository` behind the existing `persist()` / `record_outcome()` path. Keep writing `<workspace>/.agentbridge/state.json` until a later dual-write sunset.
3. **SQLite is service-level history, not a per-project lock file.** `state.json` remains the runtime snapshot for the current task. SQLite stores history + events.
4. **Config search order must not change in 2.0.0.** Current priority is already used by CLI, Tauri, and systemd. New canonical directories are added as fallbacks; existing `~/.agentbridge` and `.agentbridge.toml` keep winning.
5. **`agentbridge init` must stop silently overwriting.** Missing file → create. Existing file → keep. `--force` → overwrite.
6. **Delete Tauri last.** First lift Tauri-only upsert/dashboard/config-patch into Core, then HTTP API, then Web Console, then remove `desktop/`.
7. **`skill/SKILL.md` is the Brain skill. Do not delete it and do not treat it as Skill Manager.** Skill Manager is a new Core module.
8. **Linux default is a user systemd unit, not root.** Confirmed. Keep the existing system unit as an optional packaged path. Windows uses NSSM wrapping `agentbridge serve --config <file>`. Secrets stay in the config file, never on the command line.
9. **Only OpenCode actually executes today.** Discovery lists Codex / Claude / Gemini / Grok, but `ExecutorRegistry` only instantiates `OpenCodeExecutor` for `kind == "opencode"`. Web must show detection truth, not pretend those executors run.
10. **Projects and executors stay file-based in the first SQLite phases.** Moving them into SQLite at the same time as tasks is unnecessary risk.
11. **`/api` is loopback-only and is not the MCP bearer.** Confirmed. Brain tokens must not be able to mutate projects/settings.
12. **Fresh `init` writes canonical dirs; lookup still prefers legacy files.** Confirmed.
13. **Web v1 observes and cancels tasks; it does not start them.** Confirmed.

---

## 1. 当前架构

### 1.1 Repository tree (as it exists)

```text
AgentBridge/
├── Cargo.toml                  # bin `agentbridge` 1.0.4; unused eframe/egui/rfd/auto-launch/open
├── src/
│   ├── main.rs                 # CLI entry
│   ├── lib.rs                  # Core crate
│   ├── config.rs               # Config + path helpers + executor registry IO
│   ├── state.rs                # BridgeState (current task snapshot)
│   ├── task.rs                 # TaskRuntime
│   ├── protocol.rs             # C2C PLAN/REVIEW messages
│   ├── workspace.rs            # sandboxed inspect
│   ├── git.rs
│   ├── doctor.rs
│   ├── tunnel.rs               # cloudflared helper (unused by current CLI/Tauri)
│   ├── cli/                    # init, serve, status, doctor, task, project, executor, proxy
│   ├── mcp/                    # 13 Streamable HTTP tools
│   ├── server/                 # axum: /mcp + OAuth + /health
│   ├── executor/               # discovery, OpenCode spawn, output capture, proxy test
│   ├── projects/               # ProjectHub, projects.toml / agentbridge.config.json
│   └── oauth/                  # OAuth 2.1 + JSON token store
├── desktop/                    # Tauri 2 + React console (separate crate)
│   ├── src/                    # Dashboard, Projects, Executors, Tasks, Activity, Connection, Settings
│   └── src-tauri/              # IPC commands wrapping Core
├── skill/SKILL.md              # Brain prompt (NOT a skill manager)
├── debian/agentbridge.service  # systemd unit, User=root
├── install.sh                  # Linux release installer
├── examples/                   # sample config.toml / agentbridge.config.json
├── docs/                       # architecture, security, executor notes
└── tests/                      # executor, oauth, projects, security
```

There is **no** `src/skill/`, **no** SQLite, **no** REST management API, **no** `agentbridge service`, **no** `agentbridge tray` command, **no** NSSM.

### 1.2 Runtime architecture (today)

```text
Brain (ChatGPT / Gemini / Claude)
        │  MCP Streamable HTTP  (/mcp)
        ▼
┌─────────────────────────────────────────┐
│  axum server (CLI serve OR Tauri spawn) │
│   /health  /oauth/*  /mcp               │
└──────────────┬──────────────────────────┘
               │
               ▼
        AgentBridgeMcp  ──────────► ProjectHub
               │                         │
               │                         ├── Workspace (read-only sandbox)
               │                         └── TaskRuntime (one per project)
               │                                  │
               │                                  ├── BridgeState → <ws>/.agentbridge/state.json
               │                                  ├── current.c2c
               │                                  ├── executor.pid
               │                                  └── OpenCodeExecutor (opencode run --auto)
               │
CLI (init/serve/project/executor/proxy/task)
        │  same Core files, often a *new* ProjectHub
        ▼
Tauri desktop
        │  in-process spawn_server + IPC CRUD
        ▼
   React UI (not HTTP)
```

Intended 2.0:

```text
                 ┌──────────────────┐
                 │   Web Console    │
                 └────────┬─────────┘
                          │ HTTP / SSE
┌─────────────┐           ▼
│ Brain / MCP │────► AgentBridge Core
└─────────────┘           │
                 ┌────────┼─────────┐
                 ▼        ▼         ▼
              Projects  Skills   Executors
                          │
                          ▼
                     Task Runtime
                          │
                          ▼
                    TaskRepository
                          │
                          ▼
                       SQLite
```

### 1.3 What HTTP actually is

`src/server/mod.rs` builds:

| Route | Role |
|---|---|
| `GET /` | banner text |
| `GET /health` | `{status, version}` |
| `/mcp` | Streamable HTTP MCP (the Brain API) |
| `/.well-known/oauth-*` | OAuth metadata |
| `/oauth/authorize`, `/oauth/token`, `/oauth/register`, `/oauth/revoke` | OAuth 2.1 |

There is **no** `/api/projects`, `/api/tasks`, `/api/executors`. Desktop does not call HTTP for management; it uses Tauri IPC.

---

## 2. 已实现能力

These are mature enough to keep. Refactor around them, do not rewrite.

| Capability | Where | Notes |
|---|---|---|
| MCP inspect tools | `src/mcp/mod.rs` | `list_projects`, `switch_project`, `workspace_info`, `list_directory`, `read_file`, `search_workspace`, `git_status`, `git_diff` |
| Task control tools | same | `task_start`, `task_status`, `task_cancel`, plus `test_status`, `execution_summary` |
| Workspace sandbox | `src/workspace.rs` | path jail, sensitive deny, size cap, UTF-8 only |
| C2C protocol | `src/protocol.rs` | PLAN validation, render, executor prompt |
| TaskRuntime lifecycle | `src/task.rs` | start / status / cancel / wait; per-project singleton |
| OpenCode spawn | `src/executor/opencode.rs` | `opencode run --auto`, cwd = workspace, proxy env, kill process tree |
| Output sanitization | `src/executor/output.rs` | 64 KiB capture, strip `<think>`, 2k-char summary |
| Project hub | `src/projects/` | multi-repo, hot reload, readonly flag, per-project runtime |
| OAuth 2.1 | `src/oauth/` | PKCE, DCR, consent PIN, JSON persistence |
| Executor discovery | `src/executor/discovery.rs` | PATH + `--version` probe for opencode/codex/claude/gemini/grok |
| Proxy config + test | `src/config.rs`, `src/executor/proxy.rs`, CLI `proxy` | HTTP/HTTPS/SOCKS5 |
| Doctor | `src/doctor.rs` | config, workspace, git, port, opencode, cloudflared |
| Tests | `tests/executor.rs`, `oauth.rs`, `projects.rs`, `security.rs` | fake opencode success/fail/hang/cancel; sandbox; OAuth flow |
| Debian / Linux installer | `debian/`, `install.sh`, `.github/workflows/release.yml` | amd64/arm64 tarball + .deb |
| Brain skill | `skill/SKILL.md` | the prompt that teaches a Brain how to use AgentBridge |

---

## 3. 半成品能力

These exist, but are incomplete, inconsistent, or documented as if they were finished.

### 3.1 Task history

`BridgeState` is **one current snapshot per workspace**. The next `task_start` overwrites `state.json`. Iterations of the same `task_id` also overwrite. There is no history, no `task_events`, no duration field.

`TaskStatus::{Created, Review, Done}` are defined but **never written** by the runtime. Brain skill documents REVIEW/DONE; Rust only writes Planned → Running → Executed|Failed|Cancelled (CLI can also write Blocked).

### 3.2 Executor registry vs actual execution

- Discovery knows: OpenCode, Codex, Claude Code, Gemini, Grok (`common_executor_definitions`).
- Allowlist is `opencode | codex | claude` (`ALLOWED_EXECUTOR_TYPES`).
- `validate_executor_type("codex")` returns `TypeNotImplemented`.
- `ExecutorRegistry::from_config` only constructs `OpenCodeExecutor` when `kind == "opencode"`.
- Gemini/Grok are discoverable in the UI but cannot start.

### 3.3 Config lifecycle

- `find_config` exists. `load_or_create` does **not**.
- `agentbridge init` **always overwrites** `<workspace>/.agentbridge.toml` and, unless `--local`, also `~/.agentbridge/config.toml`.
- No schema version, no config migration.
- `serve` can start with a synthesized `Config::new` if `dir`/`--workspaces` is passed and no file exists.

### 3.4 Path / CWD coupling (breaks daemon mode)

- `ProjectHub::open` calls `find_config(None)` again, ignoring `--config` already used by `serve`.
- Executor registry is loaded from `Path::new(".")` in the hub (`src/projects/mod.rs`), so `executors.toml` is CWD-relative.
- CLI `executor` defaults to CWD `.agentbridge.toml`.
- CLI `project` defaults to CWD `agentbridge.config.json`.
- systemd unit has **no `WorkingDirectory`**, so CWD discovery of projects/executors is unreliable.

Tauri is more careful: it passes the real config path into `open_with_path`.

### 3.5 Service

- Linux: unit file + `install.sh` exist, but README still says systemd is “planned”.
- Unit runs as **root**.
- `install.sh` default port **8030**, crate default **8040**, example config **8787**.
- `install.sh` runs `init /var/www/agentbridge --local` then `mv .agentbridge.toml` from **installer CWD**, not from the workspace. Config generation is likely broken.
- Windows: **no NSSM, no service CLI**.
- No `agentbridge service *` subcommands.

### 3.6 Tray / egui

README documents `agentbridge tray`. CLI `Commands` has no `Tray`. `eframe`/`egui`/`auto-launch`/`open` remain in root `Cargo.toml` with **zero usages** in `src/`. The live GUI is Tauri.

### 3.7 Logs

No log directory. `serve` uses `tracing_subscriber` to stdout. MCP/OAuth use `eprintln!`. systemd captures journal. There is no structured service log file, and executor stdout is not stored beyond a truncated `summary`.

### 3.8 Desktop vs Core gaps

- Desktop cannot start tasks (no `task_start` command).
- Desktop Tasks page lists **latest snapshot per project**, not history.
- Dashboard “PLAN / EXECUTE / TEST / REVIEW” stepper is cosmetic, not C2C state.
- `UiPrefs.auto_start` / `start_tunnel` are read but not honored; gateway always auto-starts.
- Connected OAuth clients are empty when the gateway is an external `agentbridge serve`.
- Settings “notifications” is hardcoded `false`.
- Search / Ctrl+K is a stub.

### 3.9 Skill Manager

Does not exist. `skill/SKILL.md` is a Brain instruction file.

### 3.10 HTTP management API

Does not exist. Web Console cannot be added by pointing a SPA at the current server.

---

## 4. 重复能力

| Capability | Implementations | Risk |
|---|---|---|
| Start server | CLI `serve` → `server::serve`; Tauri `start_gateway` → `spawn_server` | Env overrides (`AGENTBRIDGE_NO_AUTH`, tokens) only on CLI |
| Task start | MCP → `TaskRuntime`; CLI `--execute` → new `ProjectHub::single`; CLI without `--execute` writes PLAN **without** runtime | Iteration/id rules diverge (CLI no-execute always iteration 1 + new id) |
| Task cancel | MCP uses live hub; CLI/Tauri each construct a **new** hub and cancel via pid file | Does not share `ActiveTask`; races with waiter `record_outcome` |
| Task status | MCP `runtime.status` (reaps); `execution_summary` / `test_status` / CLI `status` / Tauri dashboard read `state.json` directly | Can show `running` after the process died until waiter flushes |
| Project CRUD | CLI `project add/remove` (CWD JSON, no update, executor hardcoded opencode); Tauri upsert by id (config-adjacent JSON, executor required); hub prefers `projects.toml` | Tauri can write JSON while hub still reads TOML |
| Executor CRUD | CLI append-only name/kind/command; Tauri upsert with executable/cwd/proxy_id/enabled | Two different write APIs |
| Proxy update | CLI patch fields; Tauri full-form save with “empty means keep secret” | Secret-preserving rule is Tauri-only |
| DTOs | `BridgeState`, MCP JSON payloads, Tauri `TaskData`/`DashboardData`, CLI println | Three shapes for one snapshot |
| Inspect tools | MCP tools vs `mcp::eval_tool` (tests only, no hub) | Fine for tests; do not grow it into a second API |
| Version numbers | crate 1.0.4, desktop 0.5.1, changelog 0.5.1, installer fallback v1.0.1 | Release confusion |
| Config copies | `init` writes project + user files with the same blob | They drift; `find_config` prefers CWD |

**Rule for 2.0:** one Core function per mutation (`upsert_project`, `upsert_executor`, `patch_config`, `apply_proxy_update`). CLI, HTTP, and (until removal) Tauri call that function.

---

## 5. Tauri 迁移表

Principle: **delete the UI, not the capability.**

### 5.1 Commands

| Tauri command | Business capability | Already in Core? | HTTP today? | MCP? | Before deleting Tauri | Final destination |
|---|---|---|---|---|---|---|
| `dashboard` | Cross-project snapshot + activity + gateway + clients | Pieces only | `/health` only | partial `list_projects` / `task_status` | **Lift aggregation into Core** | HTTP `GET /api/system/dashboard` → Web Dashboard |
| `start_gateway` | In-process `spawn_server`, or attach if port busy | `spawn_server` yes | no | no | Need Service Manager or API | Service Manager + System page. Do not stop foreign CLI processes. |
| `stop_gateway` | Stop only Tauri-managed handle | `ServeHandle::stop` | no | no | Same | Service Manager |
| `connection` | Redacted listen/auth view | `Config` | no | no | optional | `GET /api/settings` |
| `save_connection` | Patch host/port/auth/executor; keep secrets if blank | `save_to_path` only | no | no | **Lift patch helper** | Core `patch_config` → `PUT /api/settings` |
| `settings` | executor mode + ui.toml + fake notifications | `load_ui_prefs` | no | no | drop fake notifications | Settings page; autostart via Service Manager |
| `proxy` / `save_proxy` / `test_proxy_connection` | Read/write/test proxy | yes + CLI | no | no | lift “keep secret if blank” + draft test | `GET/PUT /api/settings/proxy`, `POST /api/executors/proxy/test` |
| `executors` / `test_executor` / `delete_executor` | List/scan/delete | yes + CLI | no | no | optional | `GET/DELETE /api/executors` |
| `save_executor` | Upsert + extra fields | CLI is append-only | no | no | **Lift upsert** | Core + HTTP + CLI `executor add\|update` |
| `available_executors` | Unique kinds filter | discovery exists | no | no | no | Web derives from list |
| `choose_executor_file` / `choose_executor_directory` | rfd pickers | no | no | no | no | **Drop** (Web path input) |
| `save_project` / `delete_project` | Upsert/delete by id + executor | CLI add/remove by name | no | list only | **Lift upsert** | Core + HTTP. **Not MCP.** |
| `choose_project_directory` | rfd folder picker | no | no | no | no | **Drop** |
| `cancel_task` | Cancel by project name | `TaskRuntime::cancel`; MCP yes; CLI default project only | no | yes | optional | MCP keep; HTTP `POST /api/tasks/:id/cancel`; CLI add `--project` |

### 5.2 Pages

| Page | Keep as Web? | Notes |
|---|---|---|
| Dashboard | Yes, rewrite against HTTP | Real running tasks, not fake PLAN/EXECUTE stepper |
| Projects | Yes | CRUD via API |
| Executors | Yes | Show detection status; do not claim unimplemented kinds can run |
| Tasks | Yes | Needs SQLite history or it stays “one row per project” |
| Activity | Merge into Dashboard / Task events | Today it is synthesized from the same snapshot |
| Connection | Merge into Settings / System | Listen address + MCP URL + auth |
| Settings | Yes, expand | Must call Rust config, not a second store |
| Titlebar / frameless window | Drop | Browser chrome |
| Search stub | Drop or implement later | |
| Skills / Logs / System | **New** | Not in Tauri |

### 5.3 Hidden logic that must move to Core

1. Dashboard aggregation (all projects’ `state.json` + sort).
2. Activity synthesis (replace with `task_events` once SQLite exists).
3. Executor upsert (id, executable, cwd, proxy_id, enabled, display_name).
4. Project upsert (id, executor, description, readonly) and a **single** projects file policy.
5. Config/proxy secret-preserving patch.
6. Gateway “already listening vs we spawned it” (`managed` flag) — becomes service status.

### 5.4 Must not move to MCP

Project/executor/proxy/config writes, gateway start/stop, file pickers. The Brain is untrusted. Management stays on local HTTP (loopback + existing auth) or CLI.

---

## 6. SQLite 设计

### 6.1 What SQLite is for

| Store | Medium |
|---|---|
| Current task lock / Brain polling snapshot | keep `<workspace>/.agentbridge/state.json` + `current.c2c` + `executor.pid` |
| Task history, iterations, timeline | SQLite |
| AgentBridge service logs | **files**, not SQLite |
| Projects / executors (v2.0 first releases) | keep TOML/JSON files |
| OAuth clients/tokens | keep `~/.agentbridge/oauth_*.json` until a later phase |

Do **not** `CREATE TABLE IF NOT EXISTS` in startup ad hoc. Use versioned SQL files.

### 6.2 Abstraction

```text
TaskRuntime.persist / record_outcome / mark_failed / mark_cancelled
        │
        ▼
   TaskStore (new)
        │  still writes state.json + current.c2c
        ▼
   TaskRepository  (trait)
        ├── SqliteTaskRepository
        └── NoopTaskRepository   (tests / first boot before db)
```

`TaskRuntime` must not import rusqlite. Even a later Postgres swap should only replace the repository crate/module.

Suggested module:

```text
src/storage/
├── mod.rs
├── path.rs          # db path from config/data dir
├── migrate.rs       # apply migrations/*
├── task_repo.rs     # trait
└── sqlite/
    ├── mod.rs
    └── task_repo.rs
```

### 6.3 Schema (from current `BridgeState`, not from a greenfield guess)

Primary key is `(id, iteration)` because the runtime **reuses** `task_id` across retries and only mints a new id after `Done` or `Cancelled` (and `Done` is not even written today). A single `id` primary key would collapse iterations.

```sql
-- migrations/0001_initial.sql

CREATE TABLE schema_migrations (
    version     INTEGER PRIMARY KEY,
    name        TEXT NOT NULL,
    applied_at  TEXT NOT NULL
);

CREATE TABLE tasks (
    id                  TEXT NOT NULL,          -- c2c_...
    iteration           INTEGER NOT NULL DEFAULT 1,
    project_id          TEXT,
    project_name        TEXT,
    workspace           TEXT,
    executor_id         TEXT,                   -- registry id if known
    executor_name       TEXT,                   -- today's BridgeState.executor
    goal                TEXT,
    actions_json        TEXT,                   -- JSON array
    success_criteria    TEXT,
    tests_plan_json     TEXT,                   -- plan.tests
    c2c_state           TEXT,                   -- PLAN/EXECUTING/EXECUTED/...
    lifecycle           TEXT,                   -- planned/running/executed/failed/...
    result_status       TEXT,                   -- running/success/failed/cancelled/blocked/planned
    exit_code           INTEGER,
    summary             TEXT,
    error               TEXT,
    changed_files_json  TEXT,
    test_status         TEXT,
    test_command        TEXT,
    test_exit_code      INTEGER,
    test_summary        TEXT,
    started_at          TEXT,
    finished_at         TEXT,
    duration_ms         INTEGER,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    PRIMARY KEY (id, iteration)
);

CREATE INDEX idx_tasks_project_updated ON tasks(project_id, updated_at DESC);
CREATE INDEX idx_tasks_result_updated  ON tasks(result_status, updated_at DESC);
CREATE INDEX idx_tasks_created         ON tasks(created_at DESC);
CREATE INDEX idx_tasks_workspace       ON tasks(workspace);

CREATE TABLE task_events (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id       TEXT NOT NULL,
    iteration     INTEGER NOT NULL,
    event_type    TEXT NOT NULL,
    message       TEXT,
    metadata_json TEXT,
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_events_task ON task_events(task_id, iteration, id);
```

Later, optional:

```sql
-- 0002_oauth.sql   (only when replacing oauth JSON)
-- 0003_skills.sql  (skill enablement overrides)
```

Do not put projects/executors into 0001.

### 6.4 Event types (synthesized from existing transitions)

There is no event log today. Events are **new**, generated at the existing write points:

| event_type | When (current code) |
|---|---|
| `task_planned` | `BridgeState::apply_plan` |
| `executor_started` | `mark_running` after spawn |
| `executor_spawn_failed` | `mark_failed` |
| `executor_exited` | `record_outcome` (include exit_code in metadata) |
| `files_changed` | `collect_changed_files` after finish |
| `tests_recorded` | `finalize_tests` |
| `task_succeeded` | exit 0 path |
| `task_failed` | nonzero / crash path |
| `task_cancelled` | `mark_cancelled` |
| `task_blocked` | CLI `task executed --status blocked` |

Do **not** ingest tracing/eprintln into this table.

### 6.5 Dual-write rules

1. `TaskRuntime` remains the **only** SQLite writer for live tasks.
2. CLI `task start` (no `--execute`) and `task executed` must be routed through the same persist helper, or they will desync.
3. Tauri/CLI cancel must not open a second repository; they should call Core, which uses pid-file fallback as today.
4. On identity change (`new task_id` or `iteration + 1`), INSERT a new tasks row. Never overwrite a finished iteration.
5. Current snapshot `state.json` continues to hold only the live row (Brain compatibility).
6. Failed SQLite writes must **not** fail the executor. Log and keep `state.json`. History is best-effort until the dual-write period is proven.

### 6.6 Migration strategy

- Ship SQL under `migrations/` (crate-included via `include_str!` or a directory next to the db).
- On serve start: open db, read `schema_migrations`, apply missing files in order, in a transaction.
- Never delete/overwrite an existing db file.
- Never “reset” on version mismatch; fail startup with a clear error.
- Backup recommendation (Phase 10): copy `agentbridge.db` next to itself as `.bak-<version>` before applying a new migration in production releases.

### 6.7 What not to persist

- Raw executor stdout/stderr (64 KiB buffer, discarded). Optional later: `logs/tasks/<id>/executor.log` as a **file**.
- MCP tool-call chatter (`eprintln!`).
- OAuth pending codes / PKCE verifiers (short-lived, in-memory is correct).

---

## 7. Config 设计

### 7.1 Current sources (do not silently change)

**File search (`find_config`):**

1. `--config` (must exist)
2. `./.agentbridge.toml`
3. `~/.agentbridge/config.toml` (`dirs::home_dir()`)
4. Unix only: `/etc/agentbridge/config.toml`

**Field overlay in `serve` only:**

| Field | Priority |
|---|---|
| `auth_token`, `client_id`, `client_secret`, `admin_password` | CLI flag > `AGENTBRIDGE_*` env > toml |
| `no_auth` | `--no-auth` / `--dev` **or** toml `no_auth` **or** `AGENTBRIDGE_NO_AUTH` in `{1,true,yes}` |
| `host` / `port` | CLI flag > toml (**no env today**) |
| `allow_any_host` | CLI flag **or** toml |

Tauri `start_gateway` uses toml only (no env overlay). That difference should be documented and then unified **toward the CLI serve rules**, because those are the documented daemon rules.

### 7.2 Current files

| File | Role |
|---|---|
| `.agentbridge.toml` / `~/.agentbridge/config.toml` / `/etc/agentbridge/config.toml` | listen, auth, executor, proxy, security, workspace |
| `executors.toml` (beside config, but hub currently loads from `.`) | executor registry |
| `projects.toml` or `agentbridge.config.json` | mounted projects |
| `~/.agentbridge/ui.toml` | `auto_start`, `start_tunnel` (mostly unused) |
| `~/.agentbridge/oauth_clients.json`, `oauth_tokens.json` | OAuth |
| `<workspace>/.agentbridge/state.json`, `current.c2c`, `executor.pid` | current task |

`Config` fields: `workspace, host, port, allow_any_host, auth_token, admin_password, client_id, client_secret, no_auth, tunnel_token, tunnel_hostname, executor, proxy, security`.

### 7.3 Target lifecycle (add, don’t invent a new priority)

```text
defaults()
platform_config_dir()     # new canonical dir, plus legacy aliases
config_path()             # resolve using EXISTING search order first
load()                    # error if missing
load_or_create()          # create only if missing
save()                    # never called by init unless create or --force
```

`init` contract:

```text
does not exist → create
exists         → keep, print path, exit 0
--force        → overwrite
```

This is a **behavior change** vs today (`init` always overwrites). It is required for data safety and should be explicit in the changelog.

### 7.4 Recommended directories (not activated until Config phase)

Do **not** pick one Linux layout in code before confirming §Open Questions. Recommendation for a **local developer control plane**:

**Windows (user):**

```text
%LOCALAPPDATA%\AgentBridge\
├── config.toml
├── executors.toml
├── projects.toml
├── data\
│   └── agentbridge.db
├── logs\
│   └── agentbridge.log
└── skills\
```

**Linux (user service — recommended default):**

```text
~/.config/agentbridge/config.toml
~/.config/agentbridge/executors.toml
~/.config/agentbridge/projects.toml
~/.local/share/agentbridge/agentbridge.db
~/.local/share/agentbridge/skills/
~/.local/state/agentbridge/logs/agentbridge.log
```

**Linux (system package — keep as optional):**

```text
/etc/agentbridge/config.toml
/var/lib/agentbridge/agentbridge.db
/var/log/agentbridge/agentbridge.log
```

**Legacy aliases (must keep working):**

```text
~/.agentbridge/config.toml
./.agentbridge.toml
<workspace>/.agentbridge/state.json
```

Lookup after Config phase:

1. `--config`
2. `./.agentbridge.toml`          (unchanged)
3. `~/.agentbridge/config.toml`   (unchanged — still wins over new paths)
4. new canonical user config
5. `/etc/agentbridge/config.toml` (unix)

So existing users are not migrated until they opt in (`agentbridge config migrate` in a later phase).

### 7.5 `init` / generate

- `agentbridge init [workspace] [--port] [--local] [--force]`
- `--local`: only project `.agentbridge.toml` (already exists)
- without `--local`: create user canonical config **if missing**
- Never write secrets onto argv examples in service files

Add `agentbridge config path` / `agentbridge config show` (redacted) so Web/System and humans can see which file won.

### 7.6 Web Settings

Web edits **must** `PATCH` the Rust `Config` and `save_to_path` the resolved file. No `localStorage` config. Restart-required keys (`host`, `port`, `no_auth`) return `restart_required: true`.

---

## 8. Service 设计

### 8.1 CLI shape

Fits current clap structure:

```text
agentbridge service install [--user|--system]
agentbridge service uninstall
agentbridge service start
agentbridge service stop
agentbridge service restart
agentbridge service status
```

Implementation lives in `src/service/` (Service Manager). Web System page calls HTTP which calls this module. Web never shells out to `nssm` / `systemctl`.

### 8.2 Windows + NSSM

```text
nssm install AgentBridge "C:\Program Files\AgentBridge\agentbridge.exe"
nssm set AgentBridge AppParameters "serve --config %LOCALAPPDATA%\AgentBridge\config.toml"
nssm set AgentBridge AppDirectory  %LOCALAPPDATA%\AgentBridge
nssm set AgentBridge AppStdout     %LOCALAPPDATA%\AgentBridge\logs\service.out.log
nssm set AgentBridge AppStderr     %LOCALAPPDATA%\AgentBridge\logs\service.err.log
nssm set AgentBridge AppRotateFiles 1
nssm set AgentBridge Start SERVICE_AUTO_START
```

Rules:

- **No** `--admin-password`, `--auth-token`, `--client-secret` on `AppParameters`.
- NSSM is a dependency to detect/install (document; optionally vendor a copy later).
- Service account: the interactive user (so PATH sees `opencode` / `claude` and so `%LOCALAPPDATA%` is the user’s). LocalSystem would look at the system profile and break executor detection.
- Status: `nssm status AgentBridge` parsed by Service Manager.

### 8.3 Linux + systemd

**User unit (recommended default):**

```ini
[Unit]
Description=AgentBridge local coding control plane
After=network-online.target

[Service]
Type=simple
ExecStart=/usr/bin/agentbridge serve --config %h/.config/agentbridge/config.toml
WorkingDirectory=%h
Restart=on-failure
RestartSec=3
# secrets stay in the config file
Environment=RUST_LOG=info

[Install]
WantedBy=default.target
```

Installed to `~/.config/systemd/user/agentbridge.service`. Control: `systemctl --user`.

**System unit (packaged, optional):**

Keep `debian/agentbridge.service` but change:

- `User=` / `Group=` must **not** be root. Use a dedicated `agentbridge` user **or** document that system mode is for a specific workspace owner.
- Set `WorkingDirectory=` and `EnvironmentFile=-/etc/agentbridge/env` (non-secret overrides only).
- Config file mode 0600.
- Do not put PIN/token on `ExecStart`.

### 8.4 Current Linux installer — required fixes before calling it 2.0

1. Stop running the daemon as root.
2. Fix `init` → config copy path (`/var/www/agentbridge/.agentbridge.toml`, not CWD).
3. Stop using `/var/www/agentbridge` as a dummy coding workspace unless the user asked for a system service.
4. Align default port with crate (8040) or read it from generated config.
5. README must stop saying systemd is only “planned”.

### 8.5 Secrets

Allowed: config.toml on disk with 0600; Windows ACL on the user profile directory; OAuth JSON already 0600 on Unix.

Forbidden: NSSM `AppParameters`, systemd `ExecStart=`, `ps` listings.

`authorize_post` currently logs submitted password **and** the real admin password (`src/oauth/http.rs`). That is a security defect to fix in the Service/Security phase (small, safe, should not wait for a rewrite).

---

## 9. Skill Manager 设计

### 9.1 Do not confuse with Brain skill

| Path | Role |
|---|---|
| `skill/SKILL.md` | **AgentBridge Brain Skill** — tells a remote Brain how to inspect and call `task_start`. Keep. Publish. Do not feed it to OpenCode as a project skill. |
| `src/skill/` (new) | Skill Manager — discovers, lists, enables, and injects **coding** skills into Executor runs |

### 9.2 Suggested module

```text
src/skill/
├── mod.rs
├── model.rs
├── parser.rs        # SKILL.md front matter + body
├── discovery.rs
├── manager.rs
└── installer.rs     # copy/create into global or project dir
```

Exact filenames can change; the boundary is: Skill Manager is Core, not Web.

### 9.3 Data model

```text
Skill {
  id, name, description,
  path,             # SKILL.md path
  scope,            # global | project
  project_id,       # if project
  enabled,
  source,           # agentbridge | claude | opencode | bundled
  body,             # markdown after front matter
  updated_at
}
```

Persistence for 2.0: **files are the source of truth**. SQLite may later cache enable/disable overrides (`0003_skills.sql`). Do not invent a parallel skill database in v1 of the manager.

### 9.4 Scope and discovery (order)

1. AgentBridge global: `<data_dir>/skills/*/SKILL.md`
2. Project: `<workspace>/.agentbridge/skills/*/SKILL.md`
3. Adjacent conventions (read-only discovery, do not move/delete):
   - `<workspace>/.claude/skills/*/SKILL.md`
   - `<workspace>/.opencode/skills/*/SKILL.md`
4. Never treat repo-root `skill/SKILL.md` of **AgentBridge itself** as a project skill of some other repo.

Enable/disable: a small `skills.toml` next to config (global) or `<workspace>/.agentbridge/skills.toml` (project). Missing file = all discovered enabled.

### 9.5 API / Web

```text
GET    /api/skills
GET    /api/skills/:id
POST   /api/skills              # create SKILL.md in global or project
PUT    /api/skills/:id          # edit
POST   /api/skills/:id/enable
POST   /api/skills/:id/disable
DELETE /api/skills/:id          # only AgentBridge-managed copies, not foreign .claude skills
```

Web: list, search, detail, create/edit markdown, scope filter, enable/disable.

### 9.6 Task injection

On `task_start`, Core collects **enabled** skills for that project (project overrides global). Injection options (pick one in implementation, default conservative):

1. Append a short “Available skills” section to `current.c2c` notes, with paths. Executor already reads `current.c2c`.
2. Copy enabled skill dirs into a temp `.agentbridge/run/<task_id>/skills` and mention that path.

Do not paste full skill bodies into MCP responses. Do not let the Brain supply arbitrary skill text as executable content.

---

## 10. Web Console 信息架构

New tree: `web/` (Vite + React, reuse desktop UI pieces where they are still valid). Talks **only** to HTTP API.

```text
web/
├── Dashboard
├── Projects
│     └── :id
├── Executors
│     └── :id
├── Skills
│     └── :id
├── Tasks
│     └── :id
├── Logs
├── Settings
│     ├── General
│     ├── Server
│     ├── Storage
│     ├── Executors
│     ├── Security
│     └── Logging
└── System
```

### Dashboard

Purpose: **what is AgentBridge doing right now.**

- Gateway/service status, MCP URL (copy)
- Running tasks (live)
- Today’s tasks, success/fail/cancel counts (from SQLite; until then, latest snapshots)
- Executor health (detected vs configured vs actually runnable)
- Project count, skill count
- Recent tasks
- Recent `task_events` (Activity tab can die)

Not a vanity ops screen. No fake PLAN/EXECUTE/TEST/REVIEW pipeline.

### Projects

- List: name, path, git dirty/clean, default executor, readonly
- Detail: path, git status, default executor, project skills, recent tasks, project config
- Add/edit/delete via API (typed path; optional directory picker in browser is limited — path string is the source of truth)

### Executors

- Rows from **saved registry ∪ discovery**
- Columns: name, kind, status (`available` / `not_found` / `not_executable` / `version_probe_failed` / `not_implemented`), version, path, default, proxy
- Creating a Gemini/Grok/Codex row is allowed; starting a task with it must fail with the existing `TypeNotImplemented` until that adapter exists
- Test = `scan_executor` (and later a real spawn smoke test for OpenCode only)

### Skills

- Discovery list, search, detail, create, edit, enable/disable, delete (managed only)
- Global vs project
- Link from Project detail

### Tasks

- Filters: Running / Completed (`success` + `executed`) / Failed / Cancelled / Blocked
- List from SQLite (fallback: current snapshots if db empty)
- Detail: goal, project, executor, skills used, lifecycle, result_status, duration, exit_code, changed files, tests
- Timeline from `task_events`
- Iterations: group by `task_id`, show Iteration 1..N
- Actions: cancel (running only). **No start from Web in v1** unless we explicitly add it — today the Brain starts tasks. A “start” button would be a new product decision.

### Logs

Service logs, not task timeline.

- File tail: time, level, module, optional `project` / `task_id` fields once tracing is structured
- Filter level + search
- Never dump these into SQLite

### Settings

Tabs call `GET/PUT /api/settings`. Restart-required keys flagged. Secrets write-only (show “configured”, never echo).

### System

- Version (`/health`), uptime, service status, db health, executor health, disk paths
- Service install/uninstall/start/stop/restart via `/api/system/service/*` → Service Manager
- Doctor checks (reuse `src/doctor.rs`)

---

## 11. 新目录结构

Incremental — do not move everything on day one.

```text
AgentBridge/
├── Cargo.toml                      # drop unused eframe/egui/rfd/auto-launch/open when tray is confirmed dead
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── config/                     # split config.rs when it grows (paths, model, io)
│   ├── storage/                    # NEW: migrations runner + TaskRepository
│   ├── task.rs                     # keep; persist via TaskStore
│   ├── skill/                      # NEW
│   ├── service/                    # NEW: nssm + systemd
│   ├── api/                        # NEW: REST handlers, thin, call Core
│   ├── mcp/
│   ├── server/                     # still owns bind, CORS, merge mcp + oauth + api + static web
│   ├── executor/
│   ├── projects/
│   ├── oauth/
│   ├── cli/
│   │   └── service.rs              # NEW subcommand
│   └── ...
├── migrations/
│   └── 0001_initial.sql
├── web/                            # NEW console (Vite/React)
├── skill/SKILL.md                  # KEEP Brain skill
├── desktop/                        # DELETE in Phase 9, not before
├── debian/
├── install.sh
└── docs/
```

Static web assets: `axum` serves `web/dist` from `/` and keeps `/mcp` / `/api` / `/oauth`. Dev mode: Vite proxy to the Rust port.

---

## 12. 重构顺序

Each phase must leave `agentbridge serve` + MCP tools + tests green. Prefer mergeable slices over a branch that rewrites everything.

### Phase 1 — Architecture freeze / Core extraction

**Goal:** make Tauri a thin client of Core, without SQLite yet.

- Extract `upsert_project`, `upsert_executor`, `patch_config`, `apply_proxy_update`, `dashboard_snapshot` into Core.
- Point Tauri commands at those functions (behavior preserved, including secret-keep-if-blank).
- Fix hub to load `executors.toml` from the **resolved config path**, not `"."`.
- Pass `--config` path into `ProjectHub::open_with_path` from `serve`.
- Align CLI `project`/`executor` with the same helpers (still files).
- Remove unused `eval` growth; keep tests.

**Why first:** otherwise HTTP and Web will copy Tauri’s private logic again.

**Out of scope:** schema, NSSM, deleting desktop.

### Phase 2 — Config lifecycle and paths

- `load_or_create`, `init` exists-keep / `--force`
- `agentbridge config path|show`
- Canonical dir helpers + legacy fallbacks (search order unchanged)
- Redacted settings DTO for API
- Unify Tauri/CLI env overlay for serve options
- Fix `install.sh` config copy bug (safe, isolated)

### Phase 3 — SQLite foundation

- `src/storage`, `migrations/0001_initial.sql`
- Open db at data dir; run migrations; `/health` (or doctor) reports db ok
- `NoopTaskRepository` default so TaskRuntime behavior is unchanged
- Tests: apply migrations twice (idempotent), never wipe an existing file

### Phase 4 — Task persistence

- Dual-write from `TaskRuntime` persist points
- Route CLI `task start` / `task executed` through the same persist helper
- `GET` list/detail/events used later by API
- Keep `state.json` as Brain snapshot
- Tests: iteration inserts a new row; cancel writes `task_cancelled`; history survives next start

**Do not change** spawn/cancel/wait semantics.

### Phase 5 — HTTP management API

Same process as MCP:

```text
/api/projects
/api/executors
/api/skills          # stub 501 until Phase 6 if needed, or skip routes until then
/api/tasks
/api/logs
/api/settings
/api/system
```

- Loopback default; reuse existing OAuth/static token or a local-only cookie later. First version: same bearer as MCP **or** loopback-only without exposing writes to the Brain’s token if that is too broad — **decision in Open Questions**.
- Task live updates: **SSE** on `/api/tasks/stream` (MCP stack already speaks SSE). WebSocket is optional later; do not add a second realtime stack now.
- Handlers call Core only.

### Phase 6 — Skill Manager

- Discover + parse + enable/disable
- API + injection into `current.c2c`
- Keep `skill/SKILL.md` untouched

### Phase 7 — Service Manager

- `agentbridge service *`
- Windows NSSM, Linux user systemd; optional system unit cleanup (non-root)
- File logging (tracing → `logs/agentbridge.log` + stdout)
- System API for install/start/stop/status
- Fix oauth password logging

### Phase 8 — Web Console

- New `web/` SPA
- Reuse desktop React pages where they already match (Projects, Executors, Tasks) but switch IPC → `fetch`
- New: Skills, Logs, System; rewrite Dashboard
- Browser-verify every page (desktop + mobile widths)

### Phase 9 — Tauri removal

Parity checklist:

- [ ] Dashboard, Projects, Executors, Tasks, Settings, System work on HTTP
- [ ] Gateway start/stop is service-based, not in-process spawn
- [ ] No remaining Tauri-only mutation
- [ ] README/tray docs updated
- [ ] Drop `desktop/`, `src-tauri/`
- [ ] Drop unused root GUI crates (`eframe`, `egui`, `rfd`, `auto-launch`, `open`)

### Phase 10 — Integration / release

- Version alignment (crate / web / changelog)
- Optional sunset of dual-write **only after** Brain + CLI + Web all read SQLite-backed status through Core (state.json can remain as a compatibility mirror)
- README: AgentBridge is a local service; Web and MCP are fronts
- Release linux artifacts + Windows NSSM instructions
- Do not auto-delete user `~/.agentbridge` or workspace `.agentbridge/`

---

## Risks

| Risk | Why it is real | Mitigation |
|---|---|---|
| Dual writers of `state.json` | MCP waiter, CLI executed, CLI start, Tauri cancel | Single persist helper; CLI/Tauri must use it |
| SQLite PK on `task_id` alone | Runtime reuses id across iterations | PK `(id, iteration)` |
| Losing history while “persisting” | Naive UPDATE of current snapshot | INSERT on identity change |
| Daemon CWD | systemd has no WorkingDirectory; hub loads `executors.toml` from `.` | Phase 1 path fix |
| `init` overwrite | Current code always writes | Phase 2 contract |
| Root systemd | install.sh + unit | User unit default; change system unit |
| Secrets on cmdline | Easy mistake when writing NSSM/systemd | Service Manager forbids those flags |
| Deleting Tauri too early | No REST API today | Phase 9 last |
| Deleting Brain skill | Easy to confuse with Skill Manager | Explicit keep |
| Unimplemented executors in UI | Discovery lists 5, runtime runs 1 | Status `not_implemented` |
| Web using MCP bearer for admin writes | Brain token could call `/api/projects` | Separate local management auth or bind-admin-to-loopback |
| SQLite in the workspace | Would scatter dbs and miss cross-project dashboard | One service-level db |
| Changing config search order | Breaks existing installs | Fallbacks only |

---

## Confirmed decisions (2026-09-13)

These were open questions in the first draft; they are now locked:

1. **Linux service default = user systemd.** `systemctl --user`. Config under `~/.config/agentbridge`. Do not run as root. A system unit may remain as an optional packaged path, not the workstation default.
2. **Management API auth = loopback-only.** `/api/*` is reachable on `127.0.0.1` without the Brain’s MCP bearer. Non-loopback hosts must not expose management writes. MCP OAuth/token continues to protect `/mcp` only.
3. **Config paths = read legacy first, write new dirs on fresh init.** `find_config` order does not change. If no legacy file exists, `init` writes `%LOCALAPPDATA%\AgentBridge` (Windows) or XDG (`~/.config/agentbridge`) on Linux. Existing `~/.agentbridge` and `.agentbridge.toml` keep winning.
4. **Web Console v1 does not `task_start`.** Dashboard/Tasks observe and cancel. Starting a task remains Brain/MCP (and CLI `--execute`).

Still deferred (not blocking Phase 1):

- Windows NSSM logon user vs LocalSystem — recommend the interactive user so PATH sees executors.
- Whether the .deb keeps `/var/www/agentbridge` as a dummy system workspace — recommend drop for the developer product.
- Web stack: new `web/` (recommended) vs stripping Tauri from `desktop/src`.

---

## What this phase did not do

- No large code changes
- No SQLite added
- No Tauri deleted
- No Skill Manager implemented
- No service CLI added

A durable copy of this plan lives at `docs/agentbridge-2.0-refactor-plan.md`. Implementation starts only after confirmation, beginning with Phase 1.
