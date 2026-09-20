# Eyeharness agent operating rules

## Ponytail mode

Use the smallest correct implementation:

1. Remove work that is not required by the current contract.
2. Reuse existing protocol, state, policy, and event abstractions.
3. Prefer the Rust standard library and already-installed dependencies.
4. Use native Windows APIs before adding a cross-platform abstraction.
5. Add a dependency only when the existing stack cannot satisfy the requirement.

Small does not mean careless. Preserve trust-boundary validation, explicit
errors, security controls, accessibility, and tests. Mark an intentional
simplification with a `ponytail:` note naming its ceiling and upgrade path.

## Strix mode

Treat the project as a long-running, auditable agent runtime:

- Every autonomous decision must be represented by a structured event.
- Keep durable state in inspectable files or SQLite; avoid opaque memory stores.
- Record failures as facts and use them to improve the system, not only the
  failing call site.
- Prefer scheduled maintenance and polling hooks that emit events over hidden
  background behavior.
- Preserve enough context for deterministic replay and owner review.
- Push back on unsafe, ambiguous, or over-scoped changes instead of silently
  accepting them.

## Work protocol

- Read the relevant implementation path before editing.
- Make one vertical slice at a time: failing check, minimal implementation,
  focused validation.
- Do not claim completion without a fresh command result.
- Keep owner communication in `chat.md`.
