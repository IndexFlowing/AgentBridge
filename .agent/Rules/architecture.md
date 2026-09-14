~~~markdown
# Architecture Rules

## 1. Prefer Separation of Concerns

Each capability should have one clear responsibility.

Do not place:

- configuration
- business logic
- persistence
- execution
- transport
- presentation

inside the same module merely for convenience.

---

## 2. Minimize Change Amplification

A small feature should normally require changes in a small number of modules.

If a seemingly small change requires widespread unrelated modifications,
stop and evaluate the architecture before continuing.

Look for:

- Shotgun Surgery
- Change Amplification
- Feature Envy
- God Objects
- Circular Dependencies
- Hidden Coupling
- Leaky Abstractions

---

## 3. Depend on Capabilities, Not Implementations

Higher-level components should depend on stable abstractions.

Avoid coupling business logic directly to:

- a specific CLI
- a specific AI provider
- a specific executor
- a specific HTTP framework
- a specific database implementation

unless the boundary explicitly requires it.

---

## 4. Keep Infrastructure Replaceable

Executor, AI provider, proxy, storage and transport should remain replaceable.

For example:

```text
AI Capability
    ↓
Provider Abstraction
    ↓
OpenCode / Gemini / Other Provider
~~~

not:

```text
Business Logic
    ↓
OpenCode-specific implementation
```

------

## 5. Prefer Composition

When adding a capability, prefer composing existing abstractions over modifying
large central modules.

Avoid turning a central service into a collection of unrelated feature branches.

------

## 6. Data Has an Explicit Owner

Every persistent piece of state must have a clearly defined owner.

Do not duplicate the same state across:

- files
- memory
- SQLite
- runtime structures

unless there is a documented reason.

When SQLite is the authoritative store, temporary files must not silently become
a second source of truth.

------

## 7. Project State vs Runtime State

Project-level persistent state belongs to the Project Profile / persistence layer.

Current task state belongs to AgentContext / C2C.

Executor runtime state belongs to the Executor subsystem.

Do not mix these layers.

------

## 8. Refactoring Rule

When architecture problems are discovered during implementation:

1. identify the actual boundary problem
2. determine the smallest useful abstraction
3. refactor the boundary
4. then implement the feature

Do not hide architectural problems with additional conditional logic.

```

```