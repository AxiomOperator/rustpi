Complete **Phase 11 — Ratatui TUI** for this project.

## Goal

Build the primary full-screen interactive operator interface using Ratatui, with a usable and readable multi-pane experience for conversation, tool activity, context, session/memory, provider/auth state, and logs/events, including reliable approval and interrupt workflows.

## Critical instructions

- Fully inspect the current codebase and architecture before making changes.
- Preserve sound architectural patterns already present, but refactor where needed to support a clean long-term TUI architecture.
- Build this as a real operator-grade interface, not as a thin demo terminal view.
- Do not leave placeholder panes, fake streaming renderers, or partially wired approval flows presented as complete.
- If prerequisite foundations from earlier phases are incomplete, implement the minimum required support so this phase is correct and integrated.
- Keep TUI shell/layout, runtime/event integration, pane rendering, input handling, approval/interrupt flows, navigation, and state management clearly separated behind clean interfaces.
- Ensure the TUI can realistically become the primary human operator interface for the system.
- Ensure you update both `README.md` and `project.md` to reflect architecture, controls, pane behavior, limitations, and phase progress.
- When finished, mark completed items in `project.md` if that file tracks progress.
- Output a concise completion summary, the major files changed, key design decisions, and any deferred follow-up work.

---

# Phase 11 — Ratatui TUI

## Objectives

Build the primary interactive operator experience.

## Deliverables

- [ ] Ratatui full-screen TUI
- [ ] conversation pane
- [ ] tool activity pane
- [ ] context pane
- [ ] session/memory pane
- [ ] provider/auth pane
- [ ] logs/events pane

## Tasks

- [ ] Implement Ratatui app shell
- [ ] Implement pane layout system
- [ ] Implement streaming conversation renderer
- [ ] Implement tool activity feed
- [ ] Implement session navigation
- [ ] Implement provider/model picker
- [ ] Implement auth status views
- [ ] Implement interrupt/approval workflows
- [ ] Implement memory/context inspection
- [ ] Implement keyboard shortcut system
- [ ] Add TUI snapshot and interaction tests where practical

## Exit criteria

- [ ] TUI is usable as the main operator interface
- [ ] Streaming and tool activity remain readable
- [ ] Approval and interrupt workflows are reliable

---

## Required implementation approach

### 1. Audit the existing architecture first

Before writing code:

- Review the current runtime/session model, event bus/streaming model, tool runtime events, context/memory systems, auth/provider status surfaces, CLI/RPC integration points, and current terminal/UI entrypoints if any.
- Identify any existing terminal rendering code, event consumers, command interfaces, session selectors, or approval/interrupt hooks.
- Determine the correct boundary for the TUI so it becomes a clean client over the runtime rather than a second runtime implementation.
- Refactor fragmented terminal/UI code if needed so this phase results in one coherent TUI architecture.

### 2. Define a clean TUI architecture

Implement or refine a TUI architecture with clearly separated layers for:

- Ratatui app shell and terminal lifecycle
- application state/store
- event ingestion from runtime/RPC/session layers
- pane layout and focus management
- pane renderers
- keyboard shortcut and input handling
- action dispatch/command handling
- interrupt and approval workflows
- provider/model/auth controls
- session and memory/context browsing
- logs/events display
- test harnesses/snapshots where practical

Do not collapse all behavior into one large Ratatui render loop file.

### 3. Implement the Ratatui app shell

Implement the full-screen Ratatui shell.

Requirements:
- initialize/teardown terminal cleanly
- support full-screen application lifecycle
- handle resize events
- recover terminal state on exit/crash as cleanly as practical
- integrate with the project’s async/event runtime appropriately
- provide a clean main event loop structure

The shell should be production-usable and stable enough for daily operator use.

### 4. Implement the pane layout system

Implement a pane layout system for the required views:

- conversation pane
- tool activity pane
- context pane
- session/memory pane
- provider/auth pane
- logs/events pane

Requirements:
- support clear layout composition with Ratatui
- make pane focus/selection explicit
- keep layout readable on real terminal sizes
- support sensible resizing behavior
- ensure the layout is maintainable and extensible
- avoid hardcoded brittle rendering logic scattered across panes

If tabs, splits, collapsible panes, or focus-driven detail views make sense, use them, but keep the MVP operator-friendly.

### 5. Implement the streaming conversation renderer

Implement a conversation pane that renders live/streaming interaction.

Requirements:
- support incremental/streaming content updates
- keep message boundaries readable
- distinguish system/user/assistant/tool/runtime messages appropriately if the architecture exposes those
- handle long content sensibly
- support scroll/navigation where needed
- avoid flicker or unreadable reflow during streaming

This pane should feel like a real primary working surface, not a log dump.

### 6. Implement the tool activity feed

Implement a dedicated tool activity pane/feed.

Requirements:
- show tool lifecycle activity clearly
- represent statuses such as started/stdout/stderr/completed/cancelled/failed where relevant
- keep activity readable without overwhelming the operator
- support navigation into details where practical
- avoid forcing operators to infer tool behavior from raw event spam

This should work well with the existing tool runtime/event model from earlier phases.

### 7. Implement session navigation

Implement session navigation and selection.

Requirements:
- allow browsing/selecting/resuming sessions
- surface current session identity/state clearly
- integrate with persistent session stores from earlier phases
- support keyboard-driven navigation
- fail gracefully when session data is missing/unavailable

The TUI should not require users to leave the interface just to switch sessions.

### 8. Implement the provider/model picker

Implement provider/model selection views and controls.

Requirements:
- show available providers/models
- indicate current selection
- reflect capability/auth availability where relevant
- allow operator-driven switching if supported by the runtime
- validate and surface unavailable/invalid options clearly

Keep this view integrated with the existing provider/routing architecture, not as a fake local picker.

### 9. Implement auth status views

Implement auth status views.

Requirements:
- show provider auth state and any action-needed conditions
- do not expose secrets
- clearly indicate configured/unconfigured/authenticated/expired/error states
- integrate with auth-status surfaces from earlier phases
- allow the operator to understand readiness without diving into logs

### 10. Implement interrupt and approval workflows

Implement reliable interrupt and approval workflows.

Requirements:
- support interrupting/cancelling an in-flight run
- support approving/denying sensitive actions when the runtime requests approval
- clearly surface pending approvals
- make the approval target/action understandable to the operator
- ensure interrupted/cancelled/denied states reflect correctly in the UI
- prevent confusing double-submit or stale-approval behavior

This is one of the highest-value parts of the TUI and must be treated as a first-class flow.

### 11. Implement memory/context inspection

Implement a pane or drill-down flow for inspecting context and memory.

Requirements:
- allow viewing what context was selected/packed
- allow viewing relevant memory snippets or summaries
- keep the view inspectable and useful for debugging agent behavior
- avoid dumping unreadable raw blobs if better structured presentation is possible
- integrate with the context engine and memory systems from earlier phases

This is especially important for making the operator trust the system.

### 12. Implement logs/events pane

Implement a logs/events pane.

Requirements:
- show runtime/system/log/event information clearly
- keep it separate enough from the conversation and tool panes to avoid confusion
- support scrolling/filtering/basic navigation where practical
- ensure event spam does not destroy usability

The pane should help diagnose issues without becoming the entire experience.

### 13. Implement the keyboard shortcut system

Implement a keyboard shortcut/input system.

Requirements:
- support pane focus changes
- support navigation/scrolling
- support session selection actions
- support provider/model selection actions
- support interrupt/approval actions
- support quit/help and other essential operator controls
- make keybindings explicit and discoverable in the UI or docs

Avoid a fragile tangle of hardcoded key handling inside pane render logic.

### 14. State management and event integration

Implement a coherent application state model.

Requirements:
- maintain TUI state separately from render code
- consume runtime/session/tool/auth/context events in a stable way
- prevent stale or contradictory state across panes
- handle high-frequency streaming updates without making the UI unusable
- preserve enough state for navigation, detail inspection, and reliable operator actions

Do not let each pane invent its own disconnected state model.

### 15. Readability and operator usability requirements

The TUI should be the main operator interface.

Requirements:
- prioritize readability over ornamental layout
- keep streaming output stable and understandable
- keep tool activity comprehensible during heavy activity
- keep approval and interrupt flows obvious and low-risk
- handle narrow/medium/wide terminal sizes reasonably
- ensure the interface is usable for real operational work, not only screenshots

### 16. Snapshot and interaction tests

Add TUI tests where practical.

Required where practical:
- pane rendering snapshots
- layout stability tests
- interaction tests for navigation/focus changes
- interaction tests for approval/interrupt workflows
- tests for streaming update rendering behavior
- tests for provider/model selection state rendering
- tests for auth status rendering

Do not force unrealistic testing, but add meaningful automated coverage for the TUI state/rendering model where feasible.

### 17. Error handling and fallback behavior

Implement sensible TUI behavior for failure cases.

Requirements:
- handle missing/unavailable session data cleanly
- handle runtime disconnects/errors visibly
- handle auth/provider errors clearly
- handle empty panes gracefully
- avoid terminal corruption or crashes on malformed event payloads where possible
- ensure operator actions fail clearly and safely

### 18. Keep TUI and runtime decoupled

The TUI should be a client/operator surface over the runtime, not a parallel runtime implementation.

Requirements:
- keep Ratatui concerns separate from business/runtime logic
- keep rendering concerns separate from action dispatch
- keep pane-specific UI code separate from shared domain state
- avoid leaking Ratatui types through the rest of the codebase unnecessarily
- make future alternative UIs possible without rewriting core logic

### 19. Documentation updates

You must update:

#### `project.md`
Include:
- phase progress updates
- completed deliverables/tasks checkboxes
- architecture notes for the TUI
- pane/layout notes
- approval/interrupt workflow notes
- keyboard shortcut notes
- known limitations or deferred follow-up work

#### `README.md`
Include:
- overview of the Ratatui TUI
- how to launch/use the TUI
- pane descriptions
- keyboard shortcuts
- session/provider/auth/context inspection behavior
- approval and interrupt workflow behavior
- testing instructions
- limitations/caveats

If helpful, add concise sections showing:
- pane layout overview
- core keybindings
- typical operator workflow

---

## Implementation quality bar

- Build a real operator-grade interface.
- Keep layout and rendering readable under streaming load.
- Keep approval and interrupt workflows reliable and obvious.
- Make session/context/memory inspection genuinely useful.
- Keep auth and provider views practical.
- Keep state management coherent.
- Avoid a monolithic untestable Ratatui file.
- Avoid fake panes or placeholder behavior.

## Deliverable expectations

When complete, provide:

1. A summary of what was implemented
2. The main files created or updated
3. The key architectural decisions
4. Any limitations or deferred follow-up work
5. Confirmation that `README.md` and `project.md` were updated

Do not stop at scaffolding. Complete the phase to a working standard consistent with the project’s architecture.