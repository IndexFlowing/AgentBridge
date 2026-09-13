# OpenCode（Executor）

OpenCode 是 AgentBridge 当前正式支持、也是唯一会真正启动的 Executor。

Brain（远端 AI）通过 MCP 规划与审查；AgentBridge 针对所选工作区启动 OpenCode；OpenCode 修改文件并运行测试；AgentBridge 记录精简结果。Brain 从不直接写文件。

```text
Brain (ChatGPT / Gemini / Claude)
      │ MCP
      ▼
AgentBridge
      │ C2C PLAN
      ▼
OpenCode CLI
      │
本地项目工作区
      │
AgentBridge (status / git_diff / tests)
      │ MCP
      ▼
Brain REVIEW
```

OpenCode 不需要理解 AgentBridge 协议。AgentBridge 会把校验过的 `C2cPlan` 翻译成一段 `opencode run` 提示词。

## 配置

- 默认执行器为 `opencode`。
- 其它执行器可在 Web 控制平面登记（写入 SQLite），命令字段与可执行路径由本机提供。
- 执行器类型受白名单约束；`kind == "opencode"` 会被实例化，其它类型在适配器实现前无法启动。
- MCP 请求不能指定可执行文件或 shell 命令。

## 一次任务闭环

1. Brain 通过只读 MCP 工具检查仓库。
2. Brain 调用 `task_start`，传入 `goal` 与 `plan.actions` / `tests` / `success_criteria`。
3. AgentBridge 写入 `<workspace>/.agentbridge/current.c2c`，并启动：

   ```text
   opencode run --auto "Read .agentbridge/current.c2c and implement that PLAN. ..."
   ```

   工作目录为所选项目；参数以结构化形式传递（不经过 shell 字符串）。完整 PLAN 保存在 `current.c2c`，避免向 Windows `.cmd` 包装器传入多行 argv。

4. OpenCode 检查、修改文件并运行列出的测试。
5. AgentBridge 捕获 `exit_code`、简短 `summary`、`tests` 与 `changed_files`（来自 git）；内部推理被丢弃。
6. Brain 轮询 `task_status`，随后读取 `git_diff`、`test_status`、`execution_summary`。
7. Brain 给出 REVIEW：DONE、再次 PLAN，或 BLOCKED。

## CLI（可选，使用同一个 Executor）

```bash
agentbridge task start --goal "Create TEST.md" --tests "cargo test" --execute
agentbridge task status
agentbridge task cancel
```

带 `--execute` 会前台启动 OpenCode 并等待；不带时 `task start` 只写入 PLAN（适合自己运行 OpenCode 的场景）。

手动回填结果：

```bash
agentbridge task executed --status success --tests "cargo test" --exit-code 0
```

## OpenCode 不应做的事

规划与审查归 Brain。不要让 OpenCode 静默改写 GOAL；若无法继续，Brain 应输出 `BLOCKED`。

## 诊断

`agentbridge doctor` 会报告 OpenCode 是否安装、版本探测结果，以及 config / workspace / port / bind / auth / git / cloudflared 的状态。

## 安全

- OpenCode 的 cwd 是所选项目目录。
- MCP 不能选择可执行文件或 shell 命令。
- 公网 MCP URL 必须启用 OAuth 2.1（或 `--auth-token`）。
- AgentBridge 不对 OpenCode 施加 OS 沙箱；它是用户已信任的本地进程。
