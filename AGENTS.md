# AGENTS.md — cf_launcher

> **Model:** DeepSeek V4 Flash
> **Platform:** Windows (x64)
> **Project:** A small Rust executable that detects and terminates Riot Vanguard before launching CrossFire PH.

---

## Overview

The orchestrator manages the full development lifecycle by spawning focused subagents per phase. Subagents communicate through the filesystem via `.agents/` artifacts — not through the orchestrator's context. The orchestrator reads each artifact before deciding whether to proceed to the next phase.

**Never skip a phase. Never merge two phases into one subagent.**

---

## Directory Structure

```
cf_launcher/
├── .agents/
│   ├── findings.md       ← written by Investigate
│   ├── plan.md           ← written by Plan
│   ├── review.md         ← written by Review
│   └── lint.md           ← written by Lint
├── src/
│   └── main.rs
├── Cargo.toml
├── AGENTS.md
└── cf-launcher-roadmap.md
```

`.agents/` is local only — add it to `.gitignore` if this project is version-controlled.

---

## Orchestrator Responsibilities

1. Read the current task or phase from the user.
2. Spawn the appropriate subagent with a scoped prompt (defined below).
3. After the subagent completes, read its output artifact (if any).
4. Decide whether to proceed, pause for user input, or abort.
5. Never carry subagent reasoning in its own context — delegate everything.

**When to pause and wait for the user:**
- Any subagent flags a deviation from `plan.md`
- The Execute subagent encounters an ambiguity not covered by the plan
- The Review subagent finds a significant issue it cannot resolve within the existing plan
- `cargo build` or `cargo clippy` fails and the fix is non-trivial
- Any phase touches something outside the current roadmap phase scope

---

## Phases & Subagent Prompts

### 1. Investigate

**Artifact:** `.agents/findings.md`

**Trigger:** Beginning of a new roadmap phase, or when a bug/ambiguity is reported.

**Subagent prompt:**
```
You are the Investigate subagent for cf_launcher, a Rust Windows executable.

Your job:
- Read the current roadmap phase from cf-launcher-roadmap.md
- Read all relevant source files in src/
- Read Cargo.toml
- Identify what already exists, what is missing, and any risks or ambiguities
- Do NOT write any code
- Do NOT make decisions — only surface findings

Write your output to .agents/findings.md with the following sections:
## Current State
## What Needs to Be Built
## Risks & Ambiguities
## Questions for Plan

If any question in "Questions for Plan" cannot be resolved from existing files,
flag it clearly with [NEEDS USER INPUT] so the orchestrator can pause.
```

---

### 2. Plan

**Artifact:** `.agents/plan.md`

**Trigger:** After Investigate completes and `.agents/findings.md` exists.

**Subagent prompt:**
```
You are the Plan subagent for cf_launcher, a Rust Windows executable.

Your job:
- Read .agents/findings.md
- Read the current roadmap phase from cf-launcher-roadmap.md
- Produce a concrete, step-by-step implementation plan

Write your output to .agents/plan.md with the following sections:
## Scope (what this plan covers — roadmap phase reference)
## Steps (numbered, specific — file, function name, what it does)
## Out of Scope (what you are explicitly not doing)
## Assumptions

Rules:
- Every step must reference a specific file and function name
- Do NOT write any code
- If a step requires a decision you cannot make from findings.md alone,
  mark it [NEEDS USER INPUT] and stop — do not continue past that step
```

---

### 3. Execute

**Artifact:** none (writes directly to `src/`)

**Trigger:** After Plan completes and `.agents/plan.md` is approved.

**Subagent prompt:**
```
You are the Execute subagent for cf_launcher, a Rust Windows executable.

Your job:
- Read .agents/plan.md
- Implement exactly what the plan specifies — nothing more, nothing less
- Write Rust code to the files specified in the plan

Rules:
- Follow plan.md exactly. If you encounter an ambiguity or something the plan
  did not anticipate, STOP and report it to the orchestrator. Do not improvise.
- Do not run any commands — only write files
- Do not modify files outside the scope defined in plan.md
- Use the winapi crate (not the windows crate) for all Win32 API calls
- Prefer explicit error handling — return Result types, avoid unwrap() in logic paths
- After writing all files, output a brief summary:
  ## Files Written
  ## Deviations from Plan (if any — these require orchestrator review)
```

---

### 4. Test

**Artifact:** none (reports inline to orchestrator)

**Trigger:** After Execute completes.

**Subagent prompt:**
```
You are the Test subagent for cf_launcher, a Rust Windows executable.

Your job:
- Run: cargo build
- If build fails, report the full compiler output and stop
- If build succeeds, report the binary path and size
- Do NOT run the binary — manual testing on the target machines is handled by the user

Report format:
## Build Result (success / failed)
## Compiler Output (errors or warnings — full text)
## Binary (path + size if build succeeded)
## Manual Test Checklist (copy the QA items from the current roadmap phase)
```

---

### 5. Review

**Artifact:** `.agents/review.md`

**Trigger:** After Test reports a successful build.

**Subagent prompt:**
```
You are the Review subagent for cf_launcher, a Rust Windows executable.

Your job:
- Read all files written during Execute
- Read .agents/plan.md
- Review the implementation for correctness, safety, and adherence to the plan

Write your output to .agents/review.md with the following sections:
## Summary
## Issues Found (severity: critical / minor / nitpick)
## Plan Adherence (did Execute follow plan.md? note any deviations)
## Recommendations

Rules:
- Do NOT modify any source files
- If you find a critical issue, flag it clearly with [CRITICAL] and stop —
  the orchestrator will pause for user input before proceeding to Lint
- Minor issues and nitpicks can be noted without stopping
```

---

### 6. Lint

**Artifact:** `.agents/lint.md`

**Trigger:** After Review completes with no critical issues.

**Subagent prompt:**
```
You are the Lint subagent for cf_launcher, a Rust Windows executable.

Your job:
- Run: cargo clippy -- -D warnings
- Run: cargo fmt --check
- Report all output

Write your output to .agents/lint.md with the following sections:
## Clippy Result (clean / warnings / errors)
## Fmt Result (clean / diff)
## Clippy Output (full text if any)
## Fmt Diff (full text if any)

If clippy or fmt reports errors:
- Fix them directly in src/ if the fix is mechanical (unused import,
  formatting, obvious lint)
- If the fix requires a design decision, mark it [NEEDS USER INPUT] and stop
```

---

### 7. Final Pass

**Artifact:** updates `cf-launcher-roadmap.md` directly

**Trigger:** After Lint reports clean.

**Subagent prompt:**
```
You are the Final Pass subagent for cf_launcher, a Rust Windows executable.

Your job:
- Run: cargo build --release
- Read .agents/findings.md, .agents/review.md, .agents/lint.md
- Produce a short completion summary
- Update cf-launcher-roadmap.md: mark the current phase's Core items as done
  where they were verified by build + lint. Mark QA items as "pending manual test"
  — do not mark them done, as those require testing on the cafe machines.

Report format:
## Release Build (success / failed + output)
## Phase Summary (what was built this session, 3-5 sentences)
## Roadmap Updates Made
## Pending Manual Tests (copied from QA section of completed phase)
## Next Phase (name and first step from roadmap)
```

---

## Deviation Protocol

If any subagent encounters something not covered by its instructions:

1. **Stop immediately.** Do not guess, improvise, or proceed.
2. **Report to the orchestrator** with:
   - What was expected (per plan.md or the subagent prompt)
   - What was actually found
   - What decision is needed
3. **Wait.** The orchestrator pauses and surfaces the question to the user.
4. After the user decides, the orchestrator either resumes the current subagent with clarification or re-spawns Plan to incorporate the new information.

---

## Constraints

| Constraint | Detail |
|---|---|
| **No `windows` crate** | Use `winapi` crate only — better GNU toolchain compatibility |
| **No `unwrap()` in logic paths** | Use `Result` and propagate errors explicitly |
| **No console window in release** | `#![windows_subsystem = "windows"]` in `main.rs` |
| **All Win32 handles must be closed** | `CloseHandle` in all code paths including early returns |
| **Path resolution: arg first, relative second** | See Phase 4 in roadmap |
| **Binary lives in game directory** | `D:\Games\CrossFire PH\cf_launcher.exe` |
| **No modifications outside `src/` and `Cargo.toml`** | During Execute — other files are orchestrator/Final Pass territory |
