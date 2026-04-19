Complete **Phase 10 — CLI** for this project.

## Goal

Deliver a production-usable, scriptable CLI that supports print mode, JSON mode, non-interactive execution, piped I/O, file/task execution, session resume/select, provider/model selection, authentication commands, diagnostics commands, and end-to-end automation-friendly behavior.

## Critical instructions

- Fully inspect the current codebase and architecture before making changes.
- Preserve sound architectural patterns already present, but refactor where needed to support a clean long-term CLI surface.
- Build this as a real operator-facing interface, not as a thin demo wrapper around internal functions.
- Do not leave placeholder commands, fake JSON output, or partially wired modes presented as complete.
- If prerequisite foundations from earlier phases are incomplete, implement the minimum required support so this phase is correct and integrated.
- Keep CLI parsing, command dispatch, interactive/non-interactive execution, transport/runtime integration, output formatting, auth commands, diagnostics commands, and session selection clearly separated behind clean interfaces.
- Ensure the CLI is suitable both for human operators and for shell automation.
- Ensure you update both `README.md` and `project.md` to reflect architecture, commands, usage, examples, output modes, limitations, and phase progress.
- When finished, mark completed items in `project.md` if that file tracks progress.
- Output a concise completion summary, the major files changed, key design decisions, and any deferred follow-up work.

---

# Phase 10 — CLI

## Objectives

Deliver a production-usable scriptable interface.

## Deliverables

- [ ] print mode
- [ ] JSON mode
- [ ] non-interactive mode
- [ ] piped I/O support
- [ ] file/task execution mode

## Tasks

- [ ] Implement CLI argument parsing
- [ ] Implement prompt submission
- [ ] Implement JSON output mode
- [ ] Implement streaming terminal output
- [ ] Implement file-based task execution
- [ ] Implement session resume/select
- [ ] Implement provider/model selection flags
- [ ] Implement auth commands
- [ ] Implement diagnostics commands
- [ ] Add end-to-end CLI tests

## Exit criteria

- [ ] CLI supports interactive-enough scripting workflows
- [ ] JSON mode is stable for automation
- [ ] Operators can authenticate and run tasks from the terminal

---

## Required implementation approach

### 1. Audit the existing architecture first

Before writing code:

- Review the current runtime/session model, RPC or runtime control layer, provider/model selection system, auth subsystem, diagnostics surfaces, event streaming, config system, and current entrypoints if any.
- Identify any existing CLI scaffolding, argument parsing, command wrappers, or host/runtime integration code.
- Determine the correct architectural boundary for the CLI so it remains a thin but production-grade client over stable internal interfaces.
- Refactor fragmented entrypoint or command logic if needed so this phase results in one coherent CLI surface.

### 2. Define a clean CLI architecture

Implement or refine a CLI architecture with clearly separated layers for:

- argument parsing / command model
- command dispatch
- runtime/session invocation
- interactive and non-interactive execution handling
- terminal streaming rendering
- JSON output serialization
- file/task execution ingestion
- auth command handling
- diagnostics command handling
- session selection/resume
- provider/model selection flags
- exit codes and error normalization

Do not collapse all behavior into one monolithic main file.

### 3. Implement CLI argument parsing

Implement robust CLI argument parsing.

Requirements:
- support top-level commands and subcommands cleanly
- support help/usage output
- support flags for output mode, session selection, provider/model selection, auth behavior, and diagnostics where relevant
- validate arguments clearly
- return meaningful errors for invalid combinations
- keep the CLI extensible for future commands

The parsing layer should be explicit, testable, and not dependent on brittle positional-only behavior.

### 4. Implement prompt submission

Implement prompt/task submission from the CLI.

Requirements:
- support direct prompt submission via arguments
- support reading prompt/task input from stdin when piped
- support non-interactive execution cleanly
- integrate with the runtime’s normal session/run flow rather than a one-off execution path
- preserve streaming behavior where applicable
- return sensible final status and exit codes

The CLI should be capable of being driven comfortably from shell scripts.

### 5. Implement JSON output mode

Implement a stable JSON output mode for automation.

Requirements:
- output must be machine-usable and predictable
- define a stable JSON envelope/schema for command results
- support JSON output for success and failure cases
- support JSON output for streaming-capable commands in a clearly documented way
- avoid mixing human-readable text with JSON output on stdout
- if diagnostics or logs must be emitted outside JSON mode, ensure they go to stderr if appropriate and document that behavior

Do not treat JSON mode as an afterthought or partial wrapper around print mode.

### 6. Implement print mode and streaming terminal output

Implement terminal-friendly print mode with streaming behavior.

Requirements:
- stream output to the terminal cleanly during long-running tasks
- render partial/streamed content in a readable way
- surface tool/runtime lifecycle information appropriately if the architecture supports it
- ensure output remains understandable for operators
- handle non-TTY vs TTY behavior sensibly where appropriate

Print mode should feel usable for real terminal operators, not just test demos.

### 7. Implement non-interactive mode

Implement non-interactive mode explicitly.

Requirements:
- allow commands to run fully without prompts or interactive confirmations when the caller requests non-interactive execution
- fail clearly when required approval/auth/user interaction is missing in non-interactive mode
- integrate with automation-friendly exit codes
- ensure scripting workflows do not accidentally hang waiting for input

If the project already has approval hooks, ensure CLI behavior honors them properly.

### 8. Implement piped I/O support

Implement piped I/O support cleanly.

Requirements:
- accept stdin input when data is piped in
- distinguish between TTY/no-stdin/piped-input modes correctly
- support piping prompts, files, or task descriptions according to the CLI’s design
- ensure piped input works in both print and JSON modes where appropriate
- document the supported piping patterns in `README.md`

### 9. Implement file-based task execution

Implement file/task execution mode.

Requirements:
- allow users to execute a task or prompt from a file
- validate file paths and handle missing/unreadable files cleanly
- integrate with the normal prompt/runtime flow
- support both print and JSON output modes
- document expected file formats and examples

If multiple task file formats are appropriate, choose one clean MVP format and document it clearly.

### 10. Implement session resume/select

Implement session resume/select support.

Requirements:
- allow users to resume an existing session by ID or selection mechanism
- allow selection of sessions in a scriptable way
- fail clearly for missing/invalid session targets
- integrate with the runtime/session store semantics from prior phases
- document how session lifecycle works from the CLI

The CLI should not require operators to manually inspect internals just to resume a session.

### 11. Implement provider/model selection flags

Implement provider/model selection flags.

Requirements:
- support selecting provider and/or model from the CLI
- validate requested provider/model combinations where possible
- integrate with the underlying routing/provider subsystem
- expose clear errors for invalid or unavailable providers/models
- document precedence relative to config/defaults/session state

### 12. Implement auth commands

Implement authentication-related CLI commands.

Requirements:
- provide commands to inspect auth status
- provide commands to initiate auth flows where supported by the current project architecture
- provide commands to clear or reset auth state if appropriate
- avoid exposing secrets in terminal output
- ensure auth commands work in both human-usable and automation-friendly ways where reasonable
- align with existing provider auth mechanisms and status APIs from earlier phases

Auth commands should be practical for operators using the terminal, not just placeholder wrappers.

### 13. Implement diagnostics commands

Implement diagnostics commands.

Requirements:
- expose useful health/diagnostic information through the CLI
- include runtime/config/provider/auth/backend state as appropriate for the current architecture
- keep output available in both print and JSON form where reasonable
- ensure diagnostics are useful for troubleshooting automation and operator issues
- document diagnostics commands and expected outputs

### 14. Exit codes and automation behavior

Implement clean exit code behavior.

Requirements:
- success exits with a stable zero exit code
- validation failures, auth failures, runtime failures, unavailable dependencies, and user-cancelled/approval-denied flows should map to sensible non-zero exit behavior
- document exit code behavior if the CLI surface is intended for scripting
- do not swallow runtime failures behind a generic successful exit

### 15. End-to-end CLI tests

Add end-to-end CLI tests.

Required coverage:
- argument parsing
- prompt submission
- JSON output mode
- streaming terminal output behavior where practical
- file-based task execution
- session resume/select
- provider/model selection flags
- auth commands
- diagnostics commands
- piped input behavior
- non-interactive failure/success behavior

Tests should exercise the real CLI entrypoint and process behavior as much as practical, not only internal dispatch helpers.

### 16. Error handling and normalization

Implement consistent CLI-facing error behavior.

Cover at minimum:
- invalid arguments
- conflicting flags
- missing input
- missing/unreadable task file
- invalid session ID
- unavailable provider/model
- auth required/auth failure
- non-interactive approval failure
- runtime execution failure
- JSON serialization/output mode failure

Requirements:
- errors should be clear for humans in print mode
- errors should be structured and stable in JSON mode
- avoid stack-trace dumping to stdout in normal operator flows unless explicitly in debug mode

### 17. Keep CLI and runtime decoupled

The CLI should be a stable client surface, not a place where runtime logic gets duplicated.

Requirements:
- keep business/runtime logic outside the CLI layer
- keep formatting concerns separate from execution concerns
- avoid spreading TTY/stdin/stdout conditionals throughout runtime code
- ensure future UIs or editor integrations can reuse the same underlying interfaces

### 18. Documentation updates

You must update:

#### `project.md`
Include:
- phase progress updates
- completed deliverables/tasks checkboxes
- architecture notes for the CLI
- command structure notes
- output mode notes
- session/auth/provider selection notes
- known limitations or deferred follow-up work

#### `README.md`
Include:
- overview of the CLI
- installation/build/run instructions
- command reference
- print mode behavior
- JSON mode behavior
- non-interactive mode behavior
- piped I/O usage examples
- file/task execution examples
- session resume/select behavior
- provider/model selection usage
- auth commands
- diagnostics commands
- exit code behavior if applicable
- testing instructions
- caveats/limitations

If helpful, add concise examples such as:
- simple prompt execution
- JSON mode execution
- piped input execution
- file-based execution
- session resume
- provider/model selection
- auth status/login
- diagnostics query

---

## Implementation quality bar

- Build a CLI that is genuinely usable by operators and scripts.
- Keep JSON mode stable and machine-usable.
- Keep print mode readable and responsive.
- Make non-interactive behavior safe and predictable.
- Keep auth and diagnostics practical.
- Make session handling scriptable.
- Avoid coupling CLI concerns too tightly to runtime internals.
- Avoid placeholder command surfaces.

## Deliverable expectations

When complete, provide:

1. A summary of what was implemented
2. The main files created or updated
3. The key architectural decisions
4. Any limitations or deferred follow-up work
5. Confirmation that `README.md` and `project.md` were updated

Do not stop at scaffolding. Complete the phase to a working standard consistent with the project’s architecture.