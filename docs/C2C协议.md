# C2C 协议

**C2C（Context-to-Context）** 是 Brain 与 Executor 之间的轻量任务契约。它只传递紧凑的 GOAL / ACTIONS / TESTS / SUCCESS_CRITERIA，源码与 diff 始终留在本地工作区，由 MCP 工具按需读取。

## 状态机

```text
INIT → PLAN → EXECUTING → EXECUTED → REVIEW → DONE
                   ↘ BLOCKED
                   ↘ CANCELLED
                                          ↘ PLAN（下一轮迭代）
```

`C2cState` 取值：`INIT`、`PLAN`、`EXECUTING`、`EXECUTED`、`REVIEW`、`DONE`、`BLOCKED`、`CANCELLED`。

`Recommendation` 取值：`DONE` / `PLAN` / `BLOCKED`。

> 任务结果中的 `failed` 是任务状态而不是独立的 C2C 状态：从 `task_status` / `execution_summary` 读取后，再决定是另起 PLAN 还是 BLOCKED。

## 消息字段

`C2cMessage` 字段：

```text
state
task_id
iteration
goal
skills
actions
tests
success_criteria
status
changed_files
result
recommendation
notes
```

线上格式为 `[C2C]` 头 + `KEY:` 行，多行值以块形式书写：

```text
[C2C]
STATE: PLAN
TASK_ID: c2c_20260914130051_0500
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

## C2cPlan

`C2cPlan` 是结构化的 Brain → Executor 载荷：

```text
C2cPlan {
  task_id,
  iteration,
  goal,
  skills,            // 请求的技能名
  actions,           // 具体步骤，不含源码
  tests,             // 测试命令
  success_criteria,
}
```

校验规则：

- `task_id` 非空；`iteration ≥ 1`。
- `goal` ≤ 8000 字符。
- `actions` 1–40 条、每条 ≤ 2000 字符。
- `tests` ≤ 16 条、每条 ≤ 500 字符。
- `success_criteria` 非空且 ≤ 4000 字符。

`to_executor_prompt()` 会渲染成紧凑的 PLAN，并附带 Executor 规则（提示指向路径而非粘贴源码）。

## 交接与 AgentContext

任务契约不写入工作区。AgentBridge 把渲染副本写到：

```text
~/.agentbridge/handoff/<task_id>.c2c
```

- 文件名只保留 `[A-Za-z0-9_-]`，其余字符替换为 `_`。
- 该文件是**非权威渲染副本**，权威任务状态在 SQLite `task_records`。
- 渲染内容 = `C2cPlan` 提示词 + `AgentContext`（Rules / Skills 名称等）。
- **工作区不再生成 `.agentbridge/current.c2c`**（历史遗留，已移除）。

Executor 被启动为：

```text
opencode run --auto "Read ~/.agentbridge/handoff/<task_id>.c2c and implement that PLAN. ..."
```

## 任务生命周期

任务状态（`TaskStatus`）：`created`、`planned`、`running`、`executed`、`review`、`done`、`failed`、`blocked`、`cancelled`。

对外结果状态（`result_status`）：`running`、`failed`、`blocked`、`cancelled`、`planned`、`success`。

运行时实际驱动的流转：

```text
task_start  → planned → running
Executor 结束 → executed / failed（exit 0 且无 error 视为 success）
task_cancel → cancelled
```

> `created`、`review`、`done` 为已定义状态，但当前运行时不主动置位；`blocked` 仅可通过 `task executed --status blocked` 外部写入。

## 迭代

- 一次 `task_start` 生成或延续一个 `task_id`。
- `task_id` 缺省时按 `c2c_YYYYMMDDHHMMSS_nnnn` 新建。
- 显式传入 `task_id` 时必须与该项目已持久化的 task_id 一致；新迭代会让 `iteration` 递增。
- Brain 在 REVIEW 阶段未达成 SUCCESS_CRITERIA 时，携带同一 `task_id` 再次 `task_start`。

## 示例文件

`examples/` 下提供 `plan.c2c.txt`、`executed.c2c.txt`、`review.c2c.txt` 作为格式参考。

## 相关文档

- [Agent体系](Agent体系.md)
- [执行器](执行器.md)
- [MCP接入](MCP接入.md)
- [架构](架构.md)
