# OpenCode (Executor)

AgentBridge does not drive OpenCode. OpenCode is an Executor: it edits the workspace, runs tests, and records the result.

## Loop

1. Brain inspects the repo through MCP and writes a `[C2C] STATE: PLAN` message.
2. You (or a helper) give that plan to OpenCode.
3. OpenCode implements the actions and runs the `TESTS` command.
4. Record the outcome:

```bash
agentbridge task executed \
  --task-id c2c_12345 \
  --iteration 1 \
  --status success \
  --tests "cargo test" \
  --exit-code 0
```

If you omit `--changed-files` and the workspace is a git repo, AgentBridge fills changed files from `git status`.

5. Brain calls `git_diff`, `test_status`, and `execution_summary`, then emits REVIEW.

## Handing the plan to OpenCode

`agentbridge task start --goal "..."` writes:

```
<workspace>/.agentbridge/current.c2c
```

Point OpenCode at that file:

```text
You are the Executor for AgentBridge.

Read .agentbridge/current.c2c and implement the PLAN.
Do not expand scope. Run the TESTS command from the plan.
When finished, tell me the test command and exit code so I can run
`agentbridge task executed`.
```

After OpenCode finishes, record:

```bash
agentbridge task executed --status success --tests "cargo test" --exit-code 0
```

or `--status failure` / `--status blocked`.

## What OpenCode must not do

The Brain still owns planning and review. Do not let OpenCode silently rewrite the GOAL. If it is blocked, record `--status blocked` and let the Brain emit `BLOCKED`.

## Later adapters

`src/executor.rs` defines `ExecutorAdapter` for a future OpenCode/Codex/Claude Code integration. V0.1 stays CLI-shaped on purpose.
