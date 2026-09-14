~~~markdown
# Execution Rules

## 1. Executor Is the Only Writer

All workspace modifications must be performed by the Executor.

The Brain only requests execution.

---

## 2. One Task, One Explicit Goal

Each task should have a clear goal.

Avoid combining unrelated features into a single execution request.

---

## 3. Tests Are Part of the Task

Every implementation PLAN should define appropriate verification.

Examples:

- cargo check
- cargo test
- targeted tests
- integration tests
- static analysis

The Executor should run the tests appropriate to the change.

---

## 4. Review the Actual Diff

Do not trust the Executor summary alone.

The Brain must compare:

```text
GOAL
  ↓
PLAN
  ↓
ACTUAL DIFF
  ↓
TEST RESULT
~~~

------

## 5. No Silent Scope Expansion

The Executor must not implement unrelated cleanup merely because it is convenient.

If additional architectural work is discovered:

- report it
- keep it within the current goal only if necessary
- otherwise create a separate task

------

## 6. Completion Criteria

A task is DONE only when:

- the requested behavior exists
- architecture boundaries remain valid
- tests pass
- actual diff matches the goal
- no known blocker remains

```

```