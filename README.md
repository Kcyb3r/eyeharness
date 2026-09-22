# eyeharness

**An auditable computer-use harness for Ubuntu desktops and Android devices.**

`eyeharness` is the control layer between an AI agent and a real computer. It
observes the current state, validates proposed actions, applies policy,
executes through a platform backend, verifies the result, and exposes
structured evidence. The AI can plan; the harness owns the trust boundary and
the side effects.

> **Status:** experimental development software. The Ubuntu desktop path is
> live-tested on X11, and the Android path is live-tested through ADB. Do not
> connect an unattended production system or a personal Android device
> without reviewing the actions and permissions first.

## Why a harness?

Computer-use models are good at deciding *what* to do, but raw model output
should not directly control a device. A harness adds the missing operational
boundary:

```text
observe → validate → policy gate → execute → verify → recover
```

Every action is tied to a session and an observation. Stale observations,
invalid targets, disabled elements, and policy violations are rejected rather
than silently converted into clicks or key presses.

## Capabilities

- Live desktop observation through X11 window metadata, geometry, cursor state,
  active-window state, and normalized accessibility-style elements.
- Ubuntu X11 input through `xdotool`: clicks, typing, key presses, hotkeys,
  scrolling, dragging, and waits.
- Android device discovery and control through ADB.
- Android UIAutomator observation with normalized text, content descriptions,
  classes, bounds, resource IDs, enabled state, and focus state.
- Bounded Android screenshots with automatic age, count, and size cleanup.
- MCP over stdio for use from Copilot, opencode, or another MCP-compatible
  client.
- Structured protocol types for observations, actions, targets, policy, and
  events.
- Verification and recovery primitives instead of success-shaped fallbacks.
- HUD state, progress smoothing, notifications, and animation primitives.
- JSONL logging and replay-oriented crates for auditable runtime evidence.
- Bounded self-learning profiles that record outcomes but require explicit
  user approval before a strategy becomes discoverable.

## How desktop control works

The current Linux desktop backend uses the native X11 session:

1. The perception provider queries display geometry, the pointer, the active
   window, visible windows, titles, bounds, and process IDs with `xdotool`.
2. The observation is normalized into the harness protocol.
3. The core checks session identity, observation freshness, target confidence,
   element state, and policy.
4. The executor sends only the approved primitive action through `xdotool`.
5. The harness observes again and reports verification evidence.

This is intentionally a window-level backend. It does not yet provide full
widget-level AT-SPI2 semantics, screenshot/OCR perception, or reliable control
of every compositor-managed KDE panel surface. X11 is required for the current
desktop input path; Wayland support is not complete.

## How mobile control works

Android control uses the already-installed `adb` executable and an explicitly
selected device serial, or the first connected device in the `device` state.

1. The ADB client discovers devices and queries screen dimensions and the
   foreground window.
2. `uiautomator dump` is parsed into normalized observation elements.
3. Package names are validated before application launch.
4. Approved actions are translated into bounded ADB operations: tap, type,
   key event, swipe, drag, launch, wait, or screenshot.
5. The device state is queried again so the caller receives current evidence.

Arbitrary shell execution is deliberately not exposed through MCP. This keeps
the Android surface smaller and preserves the policy boundary. The current
MCP Android tools are dedicated `adb_*` operations; Android is not silently
swapped into the shared desktop `HarnessCore`.

## Architecture

```text
AI agent
   │ MCP / stdio
   ▼
harness-mcp
   │
   ▼
HarnessCore
   ├── protocol      observations, actions, targets, events
   ├── policy        validation and fail-closed decisions
   ├── state         sessions and freshness
   ├── perception    desktop or Android observations
   ├── input         X11 execution
   ├── verifier      post-action evidence
   ├── recovery      bounded recovery decisions
   ├── logging       redacted JSONL evidence
   └── HUD           status and animation state
        ├── Ubuntu X11 / xdotool
        └── Android / ADB / UIAutomator
```

The workspace is split into small Rust crates so platform backends, policy,
protocol, verification, logging, and future browser integrations can evolve
independently.

## Technology stack

| Area | Technology |
| --- | --- |
| Language | Rust 2021 |
| Workspace | Cargo, resolver 2 |
| Agent protocol | Model Context Protocol over stdio |
| Serialization | `serde`, `serde_json` |
| Errors | `thiserror` |
| Async/runtime foundations | `tokio` |
| Diagnostics | `tracing`, `tracing-subscriber` |
| Ubuntu desktop input | X11 session + `xdotool` |
| Ubuntu desktop observation | X11 window metadata via `xdotool` |
| Android control | Android Debug Bridge (`adb`) |
| Android UI observation | UIAutomator XML |
| HUD fallback | `notify-send` plus Rust animation state |
| Browser backend | CDP scaffold; not production-complete |
| License | MIT OR Apache-2.0 |

## Requirements

### Ubuntu desktop

- Ubuntu or a compatible Linux distribution
- An active **X11** session
- Rust stable and Cargo
- `xdotool`
- `notify-send` for the notification HUD fallback

Install the native tools on Ubuntu:

```bash
sudo apt update
sudo apt install xdotool libnotify-bin
```

### Android

- Android platform tools with `adb`
- USB debugging enabled, or a reachable emulator/device
- A device authorized for the current user

Confirm the connection:

```bash
adb devices
```

## Build and test

From the repository root:

```bash
cargo check --workspace
cargo test --workspace
cargo build -p harness-mcp
```

The MCP binary is produced at:

```text
target/debug/harness-mcp
```

## Run the MCP server

Start the server directly:

```bash
./target/debug/harness-mcp
```

It reads newline-delimited JSON-RPC requests from stdin and writes responses
to stdout. A minimal health check is:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"ping","params":{}}' \
  | ./target/debug/harness-mcp
```

The repository includes an `opencode.json` example that registers the local
binary as the `eyeharness` MCP server. Update its absolute path if the
checkout is moved.

## MCP tools

The desktop server currently exposes:

- `observe` — return the live normalized desktop observation.
- `execute` — policy-gated coordinate click.
- `keypress` — send a key or hotkey.
- `type` — type text into the focused application.
- `launch` — open an application through KDE KRunner.
- `verify` — verify the latest live observation.
- `hud` — show a lightweight desktop status notification.

Android tools include:

- `adb_devices`
- `adb_screenshot`
- `adb_tap`
- `adb_type`
- `adb_key`
- `adb_launch`
- `adb_current_app`

Learning tools include:

- `learn_record` — record a success or failure for a candidate strategy.
- `learn_approve` — explicitly approve or revoke a strategy.
- `learn_lookup` — return only approved, non-expired strategies.
- `learn_export` — inspect the complete local profile.
- `learn_reset` — delete all or profile-specific learned entries.

Learning is deliberately conservative. Profiles are stored as inspectable JSON
under `$EYEHARNESS_LEARNING_PATH`, or
`$XDG_STATE_HOME/eyeharness/learning.json` when configured. New strategies
start unapproved, expire after 30 days of inactivity, are capped at 512
entries, and reject common secret-bearing fields. Learning changes preference
and recovery suggestions; it cannot grant permissions, bypass policy, or run
arbitrary shell commands.

Example desktop observation request:

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"observe","arguments":{}}}
```

Example Android device query:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"adb_devices","arguments":{}}}
```

## Safety model

- Actions must reference the current session and observation.
- Freshness is bounded; stale observations cannot authorize execution.
- Disabled elements and invalid structures are rejected.
- Target confidence is checked before execution.
- Android package names are validated.
- Arbitrary ADB shell commands are not exposed.
- Screenshot retention is bounded by age, count, individual size, and total
  size.
- Failures are returned explicitly and should become structured evidence.
- Human control remains possible at the desktop and device level.

This is a control harness, not a sandbox. An approved click can still have
real consequences.

## Workspace layout

| Crate | Responsibility |
| --- | --- |
| `harness-core` | Observation, gating, execution, verification orchestration |
| `harness-protocol` | Stable wire and domain types |
| `harness-policy` | Fail-closed structural and policy checks |
| `harness-events` | Structured event bus |
| `harness-perception` | Perception interfaces |
| `harness-windows-uia` | Current Linux/X11 desktop provider |
| `harness-input` | Native desktop input backends |
| `harness-adb` | Android ADB provider and executor |
| `harness-verifier` | Post-action verification |
| `harness-recovery` | Recovery primitives |
| `harness-logging` | Redacted JSONL logging |
| `harness-replay` | Replay-oriented runtime support |
| `harness-hud` | HUD state and animation primitives |
| `harness-mcp` | MCP server and agent-facing tools |
| `harness-browser` | Incomplete browser/CDP backend |

## Current limitations and roadmap

- Add robust launch verification based on the requested process/window.
- Add a dedicated window-focus operation to the desktop MCP surface.
- Improve verification for compositor surfaces where window metadata does not
  change.
- Add AT-SPI2 widget perception and screenshot/OCR evidence.
- Add Wayland capability discovery and a native Wayland-compatible backend.
- Complete the browser/CDP backend.
- Wire all MCP audit events into durable redacted JSONL logs.
- Replace notification-only HUD fallback with a transparent click-through X11
  overlay.
- Unify Android observations and execution with a selectable shared core path.
- Add stronger Android key mapping and real double-tap semantics.

## Contributing

Keep changes small and auditable. Reuse existing protocol, policy, event, and
verification abstractions. Add tests for protocol or policy behavior and
avoid introducing a dependency when the Rust standard library or an existing
backend is sufficient.

```bash
cargo fmt --all -- --check
cargo test --workspace
```

## License

Licensed under either of:

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
- [MIT License](https://opensource.org/licenses/MIT)

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in this project is dual-licensed under these terms.
