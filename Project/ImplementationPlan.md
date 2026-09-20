 # AI Realtime Computer-Use Harness — Implementation Plan

 ## 1\. Project Vision

 Build a high-performance Windows-first AI computer-use harness that allows an AI agent to observe a live desktop, understand UI state, autonomously interact with applications, verify its actions, recover from failures, and expose the computer through a standard agent-facing protocol.

 The harness must be:

 - Realtime
- Low latency
- Autonomous
- Observable
- Recoverable
- Secure
- Extensible
- Installable with one command
- Independent of any particular AI model
- Capable of browser and native desktop interaction
- Human-overridable at any time

 The fundamental loop is:

```
OBSERVE
   ↓
UNDERSTAND
   ↓
PLAN
   ↓
ACT
   ↓
VERIFY
   ↓
RECOVER if necessary
   ↓
OBSERVE again
```

 The harness, not the AI model, owns actual computer execution.

---

 # 2\. High-Level Architecture

```
                         ┌─────────────────────┐
                         │      ANY AI         │
                         │                     │
                         │ reasoning / planning│
                         └──────────┬──────────┘
                                    │
                              MCP / Agent API
                                    │
                         ┌──────────▼──────────┐
                         │    Agent Gateway    │
                         │                      │
                         │ sessions             │
                         │ authentication       │
                         │ validation           │
                         │ rate limits          │
                         └──────────┬──────────┘
                                    │
                              low-latency IPC
                                    │
              ┌─────────────────────▼─────────────────────┐
              │               HARNESS CORE                │
              │                                           │
              │  Observation → State → Action → Verify   │
              │       ↑                         │          │
              │       └──────── Recovery ──────┘          │
              │                                           │
              │  Policy │ Logging │ Replay │ Plugins       │
              └───────────────┬───────────────────────────┘
                              │
               ┌──────────────┼──────────────┐
               ▼              ▼              ▼
           Windows          Browser          HUD
           APIs             APIs             Overlay
```

---

 # 3\. Core Design Principles

 ## 3.1 AI is replaceable

 The harness must not depend on a particular model.

 The following should be replaceable independently:

 - OpenAI-compatible models
- Other model providers
- Local models
- Vision models
- Future agent frameworks

 The AI talks to the harness through a stable protocol.

 ## 3.2 Harness owns execution

 The model can request:

```
click
type
scroll
keypress
navigate
copy
observe
```

 but the harness decides whether and how that action is actually executed.

 ## 3.3 Policy is outside the model

 Never rely on the model to enforce safety.

 Use:

```
AI
 ↓
Policy Engine
 ↓
Action Validator
 ↓
Executor
  ↓
  OS
  ```

  ### 3.3.1 Policy decision contract

  The policy engine must return an explicit decision:

  ```
  allow
  block
  confirm
  ```

  - `allow` → the action proceeds to the executor.
  - `block` → the action is aborted immediately. No retry is permitted unless the agent submits a new, materially different request.
  - `confirm` → a human confirmation prompt is shown; the action waits for a response with a 30-second timeout. On timeout or denial, the action is aborted.

  Policy evaluation is synchronous and must complete before the executor receives the request.

  Every `block` emits `policy.blocked` with the reason. Every `confirm` emits `policy.confirmation_required`.

  ## 3.4 Prefer semantic interaction

 Use:

```
UI Automation / DOM
        ↓
Accessibility
        ↓
OCR
        ↓
Vision
        ↓
Coordinates
```

 Coordinates are the fallback, not the primary mechanism.

---

 # 4\. Technology Stack

 ## 4.1 Core runtime

 Use:

 - Rust
- Tokio
- serde
- serde\_json
- tracing
- tracing-subscriber

 Rust is preferred for the latency-sensitive runtime.

 Reasons:

 - native performance
- predictable memory behavior
- strong concurrency model
- excellent Windows integration
- low runtime overhead
- easy creation of a single native executable

---

 # 5\. Windows Integration

 ## 5.1 Screen capture

 Primary targets:

 - Windows Graphics Capture
- DXGI Desktop Duplication

 Avoid making the hot path dependent on repeatedly copying screenshots through high-level libraries.

 Desired pipeline:

```
GPU framebuffer
      ↓
native capture
      ↓
frame/change detection
      ↓
perception
```

 Use GPU resources efficiently where possible.

 ## 5.2 Mouse and keyboard

 Use native Windows APIs.

 Primary mechanisms:

 - `SendInput`
- `SetCursorPos`
- native keyboard input APIs

 Keep the executor deterministic.

 ## 5.3 Windows UI Automation

 Use Microsoft Windows UI Automation.

 Extract:

 - control type
- name
- text
- bounds
- enabled state
- focused state
- patterns
- parent/child relationships

 Prefer UI Automation invocation over pixel clicking when possible.

---

 # 6\. Browser Integration

 Use:

 - Playwright
- Chrome DevTools Protocol where appropriate
- DOM
- accessibility tree
- browser events

 Browser interaction hierarchy:

```
DOM
 ↓
Accessibility
 ↓
Browser semantic APIs
 ↓
OCR
 ↓
Vision
 ↓
Coordinates
```

 The browser adapter must expose browser state without forcing the AI to infer everything from screenshots.

---

 # 7\. Unified Observation Model

 Create a normalized observation object.

 Example:

```
{
  "observation_id": 1842,
  "timestamp": 1789912345,
  "active_window": {
    "title": "Chrome",
    "application": "Google Chrome"
  },
  "cursor": {
    "x": 812,
    "y": 421
  },
  "elements": [
    {
      "id": "e183",
      "role": "button",
      "name": "Submit",
      "text": "Submit",
      "bounds": [800, 600, 920, 650],
      "enabled": true,
      "confidence": 0.98,
      "source": "accessibility"
    }
  ],
  "text": [],
  "screen_changed": true
}
```

 Observation sources:

 - screenshot
- UI Automation
- DOM
- accessibility tree
- OCR
- computer vision
- active window
- clipboard
- application metadata

---

 # 8\. Perception Architecture

 Create a provider abstraction.

```
PerceptionProvider
├── ScreenshotProvider
├── AccessibilityProvider
├── DOMProvider
├── OCRProvider
└── VisionProvider
```

 Normalize all outputs into:

```
UIElement
├── id
├── role
├── name
├── text
├── bounds
├── state
├── confidence
└── source
```

 This prevents the AI layer from depending on any individual perception technology.

---

 # 9\. Target Resolution

 Create a dedicated `TargetResolver`.

 Example:

```
AI:
click "Submit"

        ↓

TargetResolver

        ├── UI Automation
        ├── DOM
        ├── accessibility
        ├── exact OCR
        ├── fuzzy OCR
        ├── vision
        └── coordinate fallback
```

 Return:

```
{
  "target_id": "e183",
  "confidence": 0.98,
  "method": "accessibility",
  "bounds": [800, 600, 920, 650]
}
```

 Never silently execute low-confidence targets.

  ### 9.1 Confidence contract

  Every resolved target is classified into one of three bands:

  ```
  confidence ≥ 0.80                  → execute
  0.50 ≤ confidence < 0.80            → re-observe, then re-resolve once; if still below 0.80, ask the agent for clarification
  confidence < 0.50                   → abort, log, emit `target.low_confidence`
  ```

  The `confidence` field is part of the TargetResolver result and is always populated.

  Thresholds are configurable per session and must be revisited when a perception provider changes.

  A target that fails the minimum band is never routed to the executor.

---

 # 10\. Action Protocol

 Define stable actions.

 Initial action set:

```
observe
move
click
double_click
type
keypress
hotkey
scroll
drag
copy
wait
```

 Higher-level semantic actions:

```
find
click_target
read
wait_for
select
navigate
```

 The semantic layer can sometimes complete several primitive actions locally, reducing model round trips.

---

 # 11\. Versioned Protocol

 Define protocol versions:

```
harness://protocol/v1
```

 Core entities:

```
Session
Observation
Action
ActionResult
Event
Target
Capability
Policy
```

 Every action should contain an observation reference.

 Example:

```
{
  "type": "action",
  "id": "a_1042",
  "session_id": "s_12",
  "observation_id": 1842,
  "action": "click",
  "target": {
    "text": "Submit"
  }
}
```

 The harness must reject stale actions where appropriate.

  ### 11.1 Observation freshness rule

  Every action references the observation it was based on (`observation_id`).

  Default maximum observation age: 500 ms.

  ```
  if now - observation.timestamp > 500 ms:
      reject action
      emit observation.stale
      queue a fresh observation
      return stale_request to the agent
  ```

  The freshness window is configurable per session and should be relaxed for slow-changing windows.

  An action whose observation is stale is never executed; the harness re-observes first.

  In addition to age, the harness should reject actions based on a changed screen state: if a large region changed after the observation was produced, treat it as stale even within the age window.

---

 # 12\. MCP Integration

 Implement the harness as an MCP-compatible server.

 Expose tools such as:

```
computer.observe
computer.click
computer.move
computer.type
computer.key
computer.hotkey
computer.scroll
computer.copy
computer.wait
```

 MCP is the agent-facing capability interface.

 Do not use MCP as the high-frequency video transport.

  ### 12.1 Adapter layer

  Every MCP tool maps to exactly one internal harness action:

  ```
  computer.observe  → observe
  computer.move     → move
  computer.click    → click
  computer.double_click → double_click
  computer.type     → type
  computer.key      → keypress
  computer.hotkey   → hotkey
  computer.scroll   → scroll
  computer.copy     → copy
  computer.wait     → wait
  ```

  Unknown or malformed MCP requests are rejected with `action.invalid` and an error is returned to the agent. They are never silently dropped or partially executed.

  MCP messages are batched and coalesced into a single harness tick so bursty agent traffic does not produce redundant captures.

---

 # 13\. Realtime Transport

 Use different technologies for different jobs.

 ## Local control

 Prefer:

 - Tokio channels
- shared memory
- Windows named pipes
- direct in-process communication

 ## Remote control

 Consider:

 - QUIC
- WebSocket where simplicity is more important

 ## Realtime video

 Use:

 - WebRTC

 Do not force every screen frame through JSON/base64.

---

 # 14\. Observation Scheduling

 Do not send every screen frame to the AI.

 Implement change-aware observation.

 Example:

```
Mouse movement
    ↓
local event only

Click
    ↓
capture

Keyboard action
    ↓
debounced capture

Large screen change
    ↓
capture

Stable screen
    ↓
no new AI observation
```

 Use:

 - frame differencing
- changed-region detection
- event-triggered capture
- semantic state changes

---

 # 15\. AI Round-Trip Optimization

 The biggest latency source will usually be model inference, not native mouse input.

 Therefore prioritize:

 1. Reduce model calls
2. Use semantic actions
3. Use UI Automation/DOM before vision
4. Use incremental observations
5. Use local models where appropriate
6. Optimize screen capture
7. Optimize IPC

 Example:

 Instead of:

```
AI → click
AI → click
AI → click
AI → click
```

 allow:

```
AI:
"Enable dark mode."

Harness/agent:

Find Settings
→ open Settings
→ find Appearance
→ open Appearance
→ find Dark Mode
→ enable
→ verify

→ return result
```

 Only use autonomous local sequences where policy allows them.

---

 # 16\. Verification Engine

 Every important action should have:

```
Precondition
    ↓
Action
    ↓
Postcondition
```

 Example:

```
Precondition:
Submit exists and is enabled.

Action:
Click Submit.

Postcondition:
Expected state transition occurs.
```

 Return evidence:

```
{
  "success": true,
  "observation_before": 1842,
  "observation_after": 1843,
  "screen_changed": true,
  "verification": {
    "status": "passed"
  }
}
```

---

 # 17\. Recovery Engine

 Handle:

 - missed clicks
- disappeared elements
- popups
- animations
- loading states
- focus changes
- navigation changes
- application crashes
- authentication prompts
- unexpected dialogs

 Recovery flow:

```
ACTION
  │
  ├── success → CONTINUE
  │
  └── failure
        │
        ├── re-observe
        ├── re-resolve target
        ├── retry
        ├── alternative action
        ├── ask agent
        └── abort
```

---

 # 18\. Event System

 Create a central event bus.

 Events:

```
session.started
session.ended

observation.created
observation.changed

agent.connected
agent.disconnected
agent.decision

action.requested
action.validated
action.executed
action.failed
action.invalid

action.stale

target.resolved
target.failed
target.low_confidence

verification.started
verification.passed
verification.failed
verification.not_applicable

observation.stale
observation.fresh

policy.allowed
policy.blocked
policy.confirmation_required

plugin.crash
plugin.invalid
plugin.unloaded

human.takeover
human.taken_over

window.changed
clipboard.changed
dialog.opened
navigation.changed

recovery.started
recovery.completed
recovery.failed
```

 The same event stream feeds:

```
Logger
HUD
Replay system
Metrics
Debug tools
```

---

 # 19\. Logging System

 Use:

 - Rust `tracing`
- OpenTelemetry
- JSONL
- SQLite

 Do not log secrets by default.

 Never automatically persist:

 - passwords
- authentication tokens
- cookies
- private clipboard contents
- sensitive form data

 unless explicitly enabled.

 Example:

```
{
  "timestamp": "2026-09-20T14:32:12.431Z",
  "session_id": "sess_81af",
  "observation_id": 1842,
  "event": "action.executed",
  "action": {
    "type": "click",
    "target": "Submit",
    "x": 812,
    "y": 641
  },
  "result": {
    "success": true,
    "screen_changed": true
  },
  "latency_ms": 43
}
```

---

 # 20\. Storage Architecture

 Use SQLite for:

 - sessions
- events
- actions
- results
- metadata
- metrics

 Use filesystem/object storage for:

 - screenshots
- recordings
- larger artifacts

 Use JSONL for:

 - debugging
- export
- development workflows

 Example:

```
sessions/
└── 2026-09-20/
    └── sess_82af/
        ├── events.jsonl
        ├── metadata.db
        └── frames/
            ├── 001.webp
            ├── 002.webp
            └── 003.webp
```

---

 # 21\. Replay System

 Every session should be replayable.

 Record:

```
screen state
events
actions
timestamps
results
verification
```

 Provide:

```
harness replay SESSION_ID
```

 Replay is critical for debugging autonomous failures.

---

 # 22\. Benchmark System

 Create deterministic tasks.

 Examples:

```
Open browser
Navigate
Search
Copy result

Open Settings
Change setting
Verify

Fill form
Handle validation error
Submit

Download file
Rename file
Move file

Recover from popup
```

 Measure:

```
success rate
time per task
model calls
actions per task
retries
wrong clicks
verification failures
recovery rate
```

 Run the same suite against every release.

---

 # 23\. Windows HUD

 Create a GPU-accelerated, always-on-top floating HUD.

 Preferred direction:

 - Win32
- DirectComposition
- Direct2D/Direct3D

 Properties:

 - borderless
- transparent
- rounded
- DPI-aware
- always-on-top
- non-focus-stealing
- optionally click-through
- adaptive position

The HUD must never become part of the AI's perceived application state.

  Mask/exclude the HUD from AI observations.

  ### 23.1 Perception exclusion contract

  The HUD is excluded from every perception provider before it can reach the observation model.

  Exclusion order of preference:

  1. OS-level exclusion — tag the HUD window so Windows Graphics Capture / DXGI never supplies its pixels.
  2. Capture-region masking — subtract the live HUD bounding rect from the captured frame.
  3. Metadata tagging — mark the HUD window as `HARNESS_OVERLAY` so accessibility and vision providers skip it.

  Exclude all three ways; do not rely on any single mechanism.

  `harness doctor` must validate that the HUD is not present in any observation before a session may start.

---

 # 24\. HUD State Machine

 Visual states:

```
IDLE
  ↓
OBSERVING
  ↓
THINKING
  ↓
TARGETING
  ↓
ACTING
  ↓
VERIFYING
  ├── SUCCESS
  └── RECOVERY
```

 Each state has a distinct visual language.

---

 # 25\. HUD Design

 Normal mode:

```
┌────────────────────────┐
│ ● AI ACTIVE            │
│                        │
│ Chrome                 │
│ Observing...           │
│                        │
│ [ Pause ] [ STOP ]     │
└────────────────────────┘
```

 Expanded mode:

```
┌─────────────────────────────┐
│ AI RUNNING                  │
├─────────────────────────────┤
│ ✓ Open browser          1.1s│
│ ✓ Navigate              0.4s│
│ ✓ Find "Login"          0.1s│
│ ✓ Click Login           0.2s│
│ ✓ Verify                0.3s│
│ ● Type email                │
└─────────────────────────────┘
```

 Display:

 - active application
- current high-level task
- current action
- verification status
- latency
- action count
- pause/stop controls

---

 # 26\. Insane Animation System

 Animations should be GPU-rendered and completely decoupled from harness execution.

 The animation layer subscribes to events.

```
Harness Core
     │
  Event Bus
     │
     ▼
    HUD
     │
     ▼
GPU rendering
```

 If the HUD drops frames, the harness must continue operating normally.

---

 # 27\. AI Alive Animation

 Idle:

```
       ◌
     ◌   ◌
    ◌ AI  ◌
     ◌   ◌
       ◌
```

 Thinking:

 - subtle pulse
- increased energy
- animated ring

 Acting:

 - faster pulse
- target illumination

 Waiting:

 - nearly static

---

 # 28\. Target Animation

 When a target is selected:

```
        ┌─────────────────┐
        │                 │
        │     Submit      │
        │                 │
        └─────────────────┘
                 ↑
                 ◉
```

 Use animated brackets and a confidence indicator.

 Example:

```
TARGET ACQUIRED
Confidence: 98.7%
Method: Accessibility
```

---

 # 29\. AI Cursor

 Optional visual AI cursor:

```
Human cursor → normal Windows cursor

AI cursor → luminous secondary cursor
```

 The AI cursor is purely visual.

 Actual input remains native Windows input.

---

 # 30\. Action Animation

 Sequence:

```
TARGET ACQUIRED
       ↓
pointer trail
       ↓
pointer reaches target
       ↓
click pulse
       ↓
verification ring
       ↓
✓ VERIFIED
```

---

 # 31\. Verification Animation

 Success:

```
       ✦
  ✦    ✓    ✦
       │
───────┼───────
       │
```

 Failure:

```
       ×
───────┼───────
       │
   TARGET LOST
```

 Recovery then begins.

---

 # 32\. Developer Vision Mode

 Provide a developer-only perception overlay.

 Example:

```
┌────────────────────────────────────┐
│ Chrome                             │
│                                    │
│ ┌──────────┐     ┌─────────────┐ │
│ │ Search   │     │   Submit    │ │
│ │ 0.99     │     │    0.98     │ │
│ └──────────┘     └─────────────┘ │
│                                    │
└────────────────────────────────────┘
```

 Colors:

```
cyan    detected
yellow  uncertain
purple  AI target
green   verified
red     blocked/error
```

---

 # 33\. HUD Modes

 ## Normal

 - subtle
- low distraction
- minimal animation

 ## Developer

 - target boxes
- telemetry
- event timeline
- perception visualization
- cursor trails

 ## Demo

 - cinematic
- strong transitions
- particles
- glow
- high-energy animations

 All three modes use the same underlying harness.

---

 # 34\. Telemetry

 Show:

```
Capture       4.2ms
Perception    8.7ms
Target        1.2ms
Action        0.8ms
Verify        3.1ms
FPS           60/120/144
Actions       142
```

 Track latency independently:

```
T_capture
T_perception
T_target
T_action
T_verify
T_model
T_total
```

---

 # 35\. Focus Management

 The HUD must never steal application focus.

 If the user switches windows:

```
AI was controlling:
Chrome

Current:
Notepad
```

 The policy engine decides whether the AI can continue.

 If focus unexpectedly changes:

```
FOCUS CHANGED
AI PAUSED
```

 unless the current task explicitly permits focus changes.

---

 # 36\. Human Takeover

 Provide:

```
Pause
Resume
Stop
Emergency Kill
Human Mode
```

 Human input must always be able to interrupt the agent.

  ### 36.1 Takeover protocol

  Human takeover must work even when:

  - the AI is disconnected
  - the model is stuck
  - the network fails
  - the UI freezes
  - the HUD has rendering problems

  Controls:

  ```
  STOP     → terminate all in-flight actions, persist state, transition to IDLE
  PAUSE    → suspend execution at the next safe boundary; HUD shows PAUSED
  RESUME   → continue from the last safe state
  HUMAN    → transfer input ownership to the human; the agent is disconnected
  ```

  Default emergency hotkey: `Ctrl+Alt+STOP`.

  The emergency path is handled in a dedicated watchdog thread and must never depend on the agent connection, the event bus, or the HUD renderer.

  When takeover occurs, the harness emits `human.takeover` and persists the current session state so a later session can resume deterministically.

---

 # 37\. Security Architecture

 Threats to address:

 - prompt injection
- malicious webpages
- clipboard poisoning
- malicious downloads
- fake UI elements
- focus stealing
- credential exposure
- malicious plugins
- unauthorized remote connections
- sensitive logs

 Important principle:

```
Visual content ≠ authority
```

 A webpage cannot grant itself permission merely by displaying an instruction.

---

 # 38\. Sandbox

 For testing:

```
AI
 ↓
Sandbox VM
 ↓
Browser/Desktop
```

 Run autonomous tasks in disposable environments.

 This enables large-scale testing without risking a real machine.

---

 # 39\. Plugin Architecture

 Plugins:

```
Harness
├── Windows plugin
├── Browser plugin
├── OCR plugin
├── Vision plugin
├── Terminal plugin
└── Application plugins
```

 Each plugin provides a manifest:

```
{
  "name": "browser",
  "version": "1.0",
  "capabilities": [
    "navigate",
    "read",
    "click"
  ]
}
```

 The core runtime should not need to know implementation details.

  ### 39.1 Plugin safety contract

  Plugins are failure-isolated:

  - A plugin crash emits `plugin.crash`; the harness continues and the plugin can be restarted.
  - Invalid plugin output (schema mismatch) emits `plugin.invalid`; the output is discarded.
  - Plugin execution is sandboxed at the OS level (process isolation) where the platform allows it.
  - A plugin that hangs is killed after a configurable timeout.
  - A plugin can only use capabilities declared in its manifest; undisclosed capabilities are denied.

  A rogue or crashed plugin must never be able to halt the harness, corrupt the state engine, or bypass the policy engine.

---

 # 40\. Installation

 Target user experience:

```
irm https://harness.dev/install.ps1 | iex
```

### 40.1 Installer bootstrap contract

- The canonical install URL is `https://harness.dev/install.ps1` (configurable via `HARNESS_INSTALL_URL` for mirrors/staging).
- The bootstrap script downloads signed binaries and a SHA-256 checksum manifest from the official CDN.
- Before executing any downloaded binary, the installer verifies the signature and checksum.
- On verification failure: abort, delete downloaded artifacts, report the checksum mismatch, and exit non-zero.
- The installer never runs a binary that failed verification, even partially.

This prevents a spliced URL or tampered fixture from executing attacker code with the user's privileges.

 The installer should:

 - detect Windows architecture
- download signed binaries
- verify signatures/checksums
- install the native runtime
- install HUD
- install MCP server
- install browser integration
- create configuration
- optionally configure startup
- run diagnostics

 Users should not need to install:

 - Rust
- Python
- Node
- Visual Studio
- CMake

 The product ships compiled.

---

 # 41\. CLI

 Provide:

```
harness install
harness start
harness stop
harness status
harness doctor
harness logs
harness replay <session>
harness update
harness plugins
harness permissions
```

 Example:

```
harness doctor

✓ Windows
✓ Screen capture
✓ Native input
✓ UI Automation
✓ Browser
✓ MCP
✓ GPU
✓ HUD
✓ Permissions

Harness ready.
```

---

 # 42\. Windows Installation Layout

 Program files:

```
C:\Program Files\Harness\
├── harness.exe
├── harness-hud.exe
├── harness-mcp.exe
├── plugins\
├── runtime\
└── config\
```

 User data:

```
%LOCALAPPDATA%\Harness\
├── sessions\
├── logs\
├── recordings\
├── cache\
└── config\
```

 Keep product binaries separate from user data.

---

 # 43\. Updates

 Updates should be atomic.

```
Download
   ↓
Verify signature
   ↓
Install alongside current
   ↓
Health check
   ↓
Switch version
```

 If the new version fails:

```
Health check failed
        ↓
Automatic rollback
        ↓
Previous version restored
```

---

 # 44\. Remote Computer Support

 Future architecture:

```
                 AI
                  │
             Cloud Gateway
                  │
        ┌─────────┴─────────┐
        ▼                   ▼
   Computer A           Computer B
```

 For remote interaction:

 - QUIC
- WebRTC
- authentication
- encryption
- device identity
- policy enforcement

---

 # 45\. Multi-Agent Support

 Future possibility:

```
Agent A
  ↓
Computer 1

Agent B
  ↓
Computer 2

Agent C
  ↓
Browser session
```

 The gateway should therefore treat sessions as first-class resources.

---

 # 46\. Recommended Repository

```
harness/
├── crates/
│   ├── harness-core/
│   ├── harness-capture/
│   ├── harness-input/
│   ├── harness-windows-uia/
│   ├── harness-browser/
│   ├── harness-perception/
│   ├── harness-target-resolver/
│   ├── harness-verifier/
│   ├── harness-recovery/
│   ├── harness-policy/
│   ├── harness-protocol/
│   ├── harness-gateway/
│   ├── harness-logging/
│   ├── harness-replay/
│   ├── harness-plugins/
│   └── harness-hud/
│
├── plugins/
│   ├── browser/
│   └── windows/
│
├── tests/
│   ├── unit/
│   ├── integration/
│   ├── replay/
│   └── benchmark/
│
├── benchmarks/
├── installer/
├── docs/
├── examples/
└── ImplementationPlan.md
```

---

# 47\. Development Phases

  Phases must not begin before the previous phase's deliverable is met.

  Where a phase touches work already listed under an earlier phase (e.g. capture in Phases 1 and 2), the earlier phase owns the mechanism and the later phase owns the higher-level integration:

  - Phase 1 owns native capture and input primitives.
  - Phase 2 consumes those primitives for perception; it does not re-implement capture.
  - Phase 9 (Installer) is scoped to the wrapper and diagnostics, not browser integration internals.

  ## Phase 0 — Protocol

 Implement:

 - session model
- observation model
- action model
- result model
- event model
- protocol versioning

 Deliverable:

```
Stable harness protocol v1
```

 ## Phase 1 — Native Windows Core

 Implement:

 - Rust runtime
- Tokio
- native screen capture
- native mouse
- native keyboard
- clipboard
- active-window detection

 Deliverable:

```
Reliable local computer control
```

 ## Phase 2 — Perception

 Implement:

 - screenshots
- UI Automation
- OCR
- browser DOM
- accessibility
- unified observation model

 Deliverable:

```
Structured desktop state
```

 ## Phase 3 — Autonomous Loop

 Implement:

 - target resolution
- actions
- verification
- recovery
- stale-observation protection

 Deliverable:

```
Observe → Act → Verify → Recover
```

 ## Phase 4 — Agent Interface

 Implement:

 - MCP
- gateway
- sessions
- permissions
- authentication

 Deliverable:

```
Any compatible AI can connect
```

 ## Phase 5 — Realtime

 Implement:

 - event stream
- incremental observations
- changed-region detection
- low-latency IPC
- performance metrics

 Deliverable:

```
Realtime computer-use runtime
```

 ## Phase 6 — HUD

 Implement:

 - Win32/DirectComposition HUD
- focus-safe overlay
- live status
- target visualization
- telemetry
- animation engine

 Deliverable:

```
Premium Windows AI HUD
```

 ## Phase 7 — Logging and Replay

 Implement:

 - structured logs
- SQLite
- event traces
- screenshots
- replay

 Deliverable:

```
Fully debuggable sessions
```

 ## Phase 8 — Security

 Implement:

 - policy engine
- confirmation flows
- emergency stop
- sandbox testing
- plugin permissions
- secret redaction

 Deliverable:

```
Controlled autonomous execution
```

 ## Phase 9 — Installer

 Implement:

 - signed binaries
- one-command bootstrap
- diagnostics
- auto-update
- rollback
- startup configuration

 Deliverable:

```
One-command installation
```

 ## Phase 10 — Benchmarking

 Implement:

 - deterministic task suite
- replay-based tests
- performance metrics
- regression tests

 Deliverable:

```
Objective reliability measurement
```

 ## Phase 11 — Plugins and Remote Execution

 Implement:

 - plugin SDK
- browser plugins
- application adapters
- remote computers
- WebRTC/QUIC

 Deliverable:

```
Extensible computer-use platform
```

---

 # 48\. Performance Priorities

 Optimize in this order:

```
1. Reduce model round trips
2. Reduce observation size
3. Use semantic UI information
4. Incremental screen updates
5. Native capture
6. Native input
7. Local IPC
8. GPU rendering
9. Remote transport
10. Micro-optimizations
```

 Do not optimize `click()` from 1 ms to 0.5 ms while the agent is waiting 800 ms for inference.

---

 # 49\. Reliability Priorities

 The system should prioritize:

```
Correct target
      >
Fast action
```

 A 10 ms click on the wrong button is worse than a 100 ms click on the correct button.

 Target confidence and post-action verification should therefore be first-class metrics.

---

 # 50\. Definition of Done

 The first serious release is complete when this workflow works:

```
One-command install
        ↓
Harness starts
        ↓
HUD appears
        ↓
AI connects
        ↓
AI receives desktop observation
        ↓
AI requests action
        ↓
Policy validates
        ↓
Target resolver finds target
        ↓
Native Windows action executes
        ↓
Harness verifies result
        ↓
HUD animates result
        ↓
Event is logged
        ↓
Observation updates
        ↓
AI continues
        ↓
Failure → recovery
        ↓
Human can interrupt
        ↓
Session is replayable
```

---

 # 51\. Final Target Architecture

```
                           ANY AI
                             │
                             │ MCP / Agent API
                             ▼
                    ┌──────────────────┐
                    │  AGENT GATEWAY   │
                    │                  │
                    │ sessions        │
                    │ auth            │
                    │ policy          │
                    └────────┬─────────┘
                             │
                       Event / IPC
                             │
        ┌────────────────────▼────────────────────┐
        │              RUST HARNESS               │
        │                                          │
        │  ┌──────────────┐   ┌───────────────┐  │
        │  │ Observation  │   │ State Engine  │  │
        │  └──────┬───────┘   └───────┬───────┘  │
        │         │                   │          │
        │         └────────┬──────────┘          │
        │                  ▼                     │
        │           Target Resolver              │
        │                  │                     │
        │                  ▼                     │
        │              Executor                  │
        │                  │                     │
        │                  ▼                     │
        │             Verification               │
        │                  │                     │
        │             ┌────┴────┐                │
        │             ▼         ▼                │
        │         Recovery    Success             │
        │                                          │
        │  Policy │ Logging │ Replay │ Plugins    │
        └──────────────┬───────────────────────────┘
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
       Windows       Browser       HUD
        APIs          APIs       GPU UI
          │
          ▼
       Desktop
```

---

 # 52\. Product Philosophy

 The system should feel like:

 > **An AI-native operating layer for interacting with computers.**

 Not:

 > A macro recorder with an LLM attached.

 The core intellectual property should live in:

```
Perception
+
State
+
Target Resolution
+
Execution
+
Verification
+
Recovery
+
Realtime orchestration
```

 The AI model should be replaceable.

 The computer-control runtime should remain valuable regardless of which model is connected.

---

 # 53\. Immediate Implementation Order

 Start in exactly this order:

```
[01] Protocol
[02] Rust core
[03] Windows capture
[04] Native input
[05] UI Automation
[06] Unified observation
[07] Target resolver
[08] Action executor
[09] Verification
[10] Recovery
[11] Event bus
[12] MCP gateway
[13] Logging
[14] Replay
[15] HUD
[16] Animation system
[17] Policy/security
[18] Installer
[19] Benchmark suite
[20] Plugin SDK
[21] Remote execution
```

 Do not jump to distributed/remote infrastructure before the local Windows runtime is reliable.

 The fundamental product milestone is:

```
OBSERVE
   ↓
UNDERSTAND
   ↓
ACT
   ↓
VERIFY
   ↓
RECOVER
   ↓
REPEAT
```

 Everything else should reinforce that loop.
