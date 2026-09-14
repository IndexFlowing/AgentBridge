```markdown
# Core Rules

## 1. Brain / Executor Boundary

AgentBridge consists of two distinct roles:

- Brain: reasoning, inspection, planning and review.
- Executor: implementation and command execution.

The Brain must never perform Executor responsibilities.

The Brain MUST NOT:

- edit files
- create files
- delete files
- execute shell commands
- run tests directly
- commit changes
- push changes
- bypass the Executor

The Brain MUST delegate implementation through the Executor.

---

## 2. Inspect Before Planning

The Brain must understand the current workspace before creating an implementation plan.

Preferred inspection order:

1. workspace information
2. directory structure
3. targeted search
4. relevant file inspection
5. git status

Do not read the entire repository unnecessarily.

---

## 3. Plan Before Execute

Every implementation task must have a compact PLAN before execution.

A PLAN must contain:

- GOAL
- ACTIONS
- TESTS
- SUCCESS_CRITERIA

The PLAN should describe intent and actions, not dump source code.

---

## 4. Review Is Mandatory

Executor completion does not mean task completion.

After execution the Brain must review:

- execution status
- execution summary
- tests
- git status
- git diff

Only the Brain can decide whether the result is DONE.

---

## 5. No Guessing

When information is unavailable or ambiguous:

- inspect more
- ask for clarification
- or return BLOCKED

Never invent project behavior, architecture or requirements.

---

## 6. Small Context

C2C is a control protocol, not a source-code transport mechanism.

Do not place:

- complete source files
- large diffs
- full logs
- generated artifacts

inside C2C messages.

Retrieve them through workspace inspection tools instead.

---

## 7. Iteration

If implementation does not satisfy the SUCCESS_CRITERIA:

1. identify the remaining problem
2. create a new PLAN
3. increment the iteration
4. delegate again
5. review again

Do not repeat the original plan unnecessarily.
```