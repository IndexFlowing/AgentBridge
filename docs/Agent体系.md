# Agent 体系

AgentBridge 把「推理」与「执行」拆分为两类角色，并用 **Agent 配置根** 统一管理影响推理行为的 Rules 与 Skills：

- **Brain**（远端 AI）负责理解代码、制定计划、审查结果。
- **Executor**（本地 Coding Agent，当前仅 OpenCode）负责真正改文件、跑命令、跑测试。

Brain 与 Executor 之间的任务契约由 [C2C协议](C2C协议.md) 描述；Executor 的执行细节见 [执行器](执行器.md)。

## Agent 根目录

AgentBridge 唯一的 Agent 配置根是：

```text
~/.agentbridge/.agent/
├── Agent.yaml          # Agent 清单（也接受 agent.yaml）
├── Rules/              # 规则文档（*.md，也接受 rules/）
├── Skills/<name>/      # 技能（SKILL.md，或 skill.yaml + system.md）
└── projects/<name>.yaml# 每个项目的 profile
```

该目录由 `config::agent_root()` 定义，`ensure_agent_root()` 在进程启动时创建根目录与 `projects/`。**工作区不再拥有 `.agent` 配置**，`*.yaml`、`Rules/`、`Skills/` 均以 AgentBridge 托管目录为准（`Agent.yaml`、`Rules`、`Skills` 等大小写变体在区分大小写的文件系统上也可解析）。

## Agent.yaml

`Agent.yaml` 是用户手写的清单，AgentBridge **只读**（不会自动生成）。当前解析器实际读取的字段：

| 字段 | 说明 |
| --- | --- |
| `version` / `name` / `description` | 清单元信息；也支持 `agent.name` / `agent.description` 嵌套形式 |
| `global_rules` | 顶层要加载的规则路径列表 |
| `active_skills` | 顶层要启用的技能名列表 |
| `skills_load` | 顶层要加载的技能路径列表；也支持 `skills.load` 嵌套形式 |
| `rules.load` | 嵌套形式的规则加载列表 |

> 说明：清单中其它字段（例如 `agent.id`、`role`、`responsibilities`、`c2c`、`executor`、`workflow` 等）会被保留但不参与解析。**`agent.yaml` 目前不支持绑定 model / provider / executor**。
>
> YAML 解析是内置的轻量子集（顶层标量 + 一层嵌套），不依赖完整 YAML 库。

## Rules

- 加载顺序：先按清单声明（`global_rules` / `rules.load`），再扫描 `Rules/` 下所有 `*.md`，按规范化相对路径去重。
- 规则路径必须留在 `.agent/` 根内，越界会被拒绝。
- 规则正文会完整渲染进 `AgentContext` 并随任务交接给 Executor。

## Skills

技能有两个来源，都会进入候选集：

1. **Agent 根技能**：`~/.agentbridge/.agent/Skills/<name>/`，由用户直接放置。支持标准 `SKILL.md`（frontmatter `name` / `description` / `version`），也支持轻量 `skill.yaml` + `system.md` 布局。
2. **已安装技能**：通过 CLI / Web / API 安装到 `~/.agentbridge/.agent/skills/`，并在 SQLite `skills` 表登记元数据。支持本地目录、`http(s)://` 与 `github:user/repo`（浅克隆 `git clone --depth 1`）。

技能启用状态：

- 若清单中 `active_skills` / `skills_load` 非空，则以清单为准；否则默认启用所有发现的技能。
- SQLite 中 `enabled` 字段可全局启用/禁用已安装技能。
- `project_skills` 表保存「项目级」策略，解析候选时会减去项目禁用的技能（**写入路径尚未接线**，见 [路线图](路线图.md)）。

`SkillResolver` 对名称与描述做确定性的关键词匹配（仅启用技能，过滤停用词与过短词），用于按任务查询候选技能。

## AgentContext

每次任务执行前，`AgentContextResolver` 会针对项目与任务解析一个 `AgentContext`：

```text
AgentContext {
  task_id, project, workspace, agent_root,
  manifest,   # name / version / description
  rules,      # 全局规则 + 项目 profile 规则（按路径去重，含正文）
  skills,     # 请求技能 + 项目技能 + 清单启用技能 + 发现技能（去重，仅名称）
}
```

解析策略（`src/core/skill/agent.rs`）：

- 只读取 AgentBridge 管理的 `.agent` 根，**绝不读取工作区内的 `.agent`**。
- 合并全局规则与项目 profile 规则，按规范化相对路径去重。
- 技能列表按名称大小写不敏感去重；**只渲染技能名称，不内联技能正文**。
- 若项目 profile 定义了 `workspace`，会覆盖传入的 workspace。
- 解析失败时降级为最小上下文并记录 warn，保证 Executor 仍可运行。

渲染结果会附加到任务交接文件：

```text
AGENT_CONTEXT:
PROJECT:   <项目名>
WORKSPACE: <工作区绝对路径>
AGENT_ROOT:<Agent 根绝对路径>
AGENT_NAME:<清单名称，可选>
CONTEXT_SKILLS: <技能名列表>
CONTEXT_RULES:  <规则正文>
```

## 项目 Profile

`~/.agentbridge/.agent/projects/<name>.yaml` 定义项目级 profile：

```text
ProjectProfile {
  name,
  workspace,   # 可选，覆盖运行时工作区
  rules,       # 项目级规则
  skills,      # 项目级技能
}
```

## 与 C2C / Executor 的关系

```text
task_start
    ↓
AgentContextResolver → AgentContext（Rules + Skills）
    ↓
C2C PLAN + AgentContext 渲染为 handoff 文件
    ↓
Executor 读取 handoff 并执行
```

- Rules / Skills 只影响交给 Executor 的上下文，不直接改代码。
- 权威任务状态在 SQLite，AgentContext 只是渲染副本的一部分。
- Web 的 **Agent** 与 **Agent Context** 页面可只读查看清单、规则与解析结果，见 [Web管理](Web管理.md)。

## CLI 与 API

CLI：

```bash
agentbridge skill list
agentbridge skill show <name>
agentbridge skill install <source> [--name <name>]
agentbridge skill enable <name>
agentbridge skill disable <name>
agentbridge skill remove <name>
agentbridge skill agent [--project <name>]      # 查看 Agent 清单、规则与技能
agentbridge skill rule <name> [--project <name>] # 打印某条规则正文
```

管理 API（`/api/*`，仅回环）：

| 方法 | 路径 | 作用 |
| --- | --- | --- |
| `GET` | `/api/agent` | 返回 Agent 根视图（清单 / 规则 / 技能） |
| `GET` | `/api/agent/context?project=&task_id=&skills=` | 诊断性解析某任务会得到的 AgentContext |
| `GET` | `/api/skills` | 列出已安装技能 |
| `POST` | `/api/skills` | 安装技能 |
| `GET` / `DELETE` | `/api/skills/{name}` | 技能详情 / 移除 |
| `PUT` | `/api/skills/{name}/enable` / `/disable` | 启用 / 禁用 |

MCP 只读工具：

| 工具 | 作用 |
| --- | --- |
| `agent_config` | 返回 Agent 根视图 |
| `list_skills` | 列出当前项目启用的技能 |
| `read_skill` | 读取某个 `SKILL.md` 详情 |

## 相关文档

- [C2C协议](C2C协议.md)
- [执行器](执行器.md)
- [Web管理](Web管理.md)
- [路线图](路线图.md)
