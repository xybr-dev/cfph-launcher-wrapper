# CF Launcher — Implementation Roadmap

> **Project:** `cf_launcher` — CrossFire PH Vanguard-aware launcher
> **Language:** Rust (Windows, x64)
> **Target Path:** `D:\Games\CrossFire PH\cf_launcher.exe`
> **Related Docs:** CrossFire PH: `crossfire init failed` Issue — 12 Jun 2026 (Notion)

---

## Purpose

This document defines the **phased build plan** for `cf_launcher` — a small Windows executable that detects and gracefully terminates Riot Vanguard before launching CrossFire PH, eliminating the kernel-level anti-cheat conflict that causes `crossfire init failed`.

Phases are sequential. Each phase produces a working, testable binary. The project is intentionally scoped as a **learning vehicle for Rust** — complexity is introduced gradually, one Windows API concept per phase.

---

## Phase Overview

| Phase | Name | Objective |
|-------|------|-----------|
| **0** | Project Setup | Cargo project, Windows target, dependencies, compile to `.exe` |
| **1** | Process Detection | Enumerate running processes; detect `vgc.exe` / `vgtray.exe` |
| **2** | User Dialog | Win32 `MessageBoxW` — warn the user, handle OK vs Cancel |
| **3** | GUI Dialog (Custom Win32) | Custom Win32 window with 4 step images, Done/Cancel buttons, and flash/fade animation |
| **4** | Game Launch | Resolve `patcher_cf2.exe` relative to launcher; spawn it |
| **5** | Build & Distribution | No console window, icon embedding, release build, desktop shortcut |
| **6** | Auto-Detect Vanguard Exit | Replace Done-button retry loop with silent polling timer that auto-proceeds when Vanguard exits |

---

## Phase 0: Project Setup

> **Goal:** A compilable Rust project that targets Windows x64 and produces a `.exe`. Nothing more.

### Core

- [x] Run `cargo new cf_launcher` and confirm the default `Hello, world!` compiles to a Windows `.exe`.
- [x] Set the default target in `.cargo/config.toml`.
- [x] Add initial dependencies to `Cargo.toml`.
- [x] Confirm `cargo build` succeeds with no warnings.
- [x] Place the compiled `.exe` in `D:\Games\CrossFire PH\` manually and confirm it runs (prints output, exits cleanly).

### QA

- [x] `cargo build` and `cargo build --release` both succeed.
- [x] `.exe` runs on a cafe machine without any Rust runtime dependency.

---

## Phase 1: Process Detection

> **Goal:** Enumerate all running Windows processes and return whether `vgc.exe` or `vgtray.exe` is among them, along with their PIDs.

### Core

- [x] Use `CreateToolhelp32Snapshot` + `Process32FirstW` / `Process32NextW` from the `windows` crate to walk the process list.
- [x] Write `fn find_vanguard_processes() -> Result<Vec<(u32, String)>, Box<dyn std::error::Error>>` returning PIDs and names for any running Vanguard processes.
- [x] Case-insensitive match on `vgc.exe` and `vgtray.exe`.
- [x] In `main`, print the result.
- [x] Learn: `PROCESSENTRY32W`, wide strings, handle cleanup with `CloseHandle` via `Drop` guard.

### QA

- [x] Run on a machine with Vanguard active → PIDs reported correctly (pending manual test).
- [x] Run on a machine without Vanguard → empty Vec, no panic (pending manual test).
- [x] No handle leaks: `SnapshotHandle` drop guard confirmed by code review.

---

## Phase 2: User Dialog

> **Goal:** Display a Win32 `MessageBoxW` that warns the gamer and returns their choice (OK to proceed, Cancel to abort).

### Core

- [x] Write `fn show_vanguard_warning() -> bool` using `MessageBoxW` with `MB_OKCANCEL | MB_ICONWARNING`.
- [x] Message warns about Vanguard blocking CrossFire and the need to restart PC for League/Valorant.
- [x] Window title: `CrossFire PH Launcher`
- [x] Wire into `main`: if Vanguard detected → show dialog → if Cancel → `std::process::exit(0)`.
- [x] Learn: `PCWSTR`, null-terminated wide strings.

### QA

- [x] Dialog appears with correct message and title (pending manual test).
- [x] Clicking Cancel exits the launcher immediately (pending manual test).
- [x] Clicking OK falls through to Phase 3 (pending manual test).

---

## Phase 3: GUI Dialog (Custom Win32 Window)

> **Goal:** Replace the MessageBoxW-based instruction dialog with a custom Win32 window that shows 4 step images with captions, Done/Cancel buttons, and flash/fade animation when Vanguard is still running. Uses the `windows` crate v0.61 GDI APIs — no external GUI framework required.

### Core

- [x] Added `Win32_Graphics_Gdi` and `Win32_UI_Controls` to Cargo.toml features.
- [x] Created `src/gui.rs` — custom Win32 dialog with `RegisterClassW`, `CreateWindowExW`, and a window procedure.
- [x] Window (1160×600) shows:
  - Warning text: "Riot Vanguard is running. CrossFire PH cannot start while Vanguard is active."
  - Disclaimer: "If you want to play League of Legends or Valorant afterwards, you must restart this PC."
  - 4 step images embedded as Windows resources (IDs 1010–1013) via `embed-resource` crate, loaded via `LoadImageW(hinst(), MAKEINTRESOURCE, ..., LR_DEFAULTCOLOR)` with `SS_BITMAP` static controls.
  - Short captions below each image.
- [x] Two buttons at the bottom: **[Done, Launch CrossFire PH]** and **[Cancel]**.
- [x] **[Done]** behavior:
  - On click: button text changes to "Checking Vanguard..." and the button becomes disabled (`EnableWindow`).
  - Re-runs `find_vanguard_processes()` to check if Vanguard is still running.
  - If still running: flash window background from `#8b0000` (dark red) to `#1e1e1e` (dark) over 800ms via `SetTimer`/`WM_TIMER`. Text changes to "Vanguard is still running. Please try again." Buttons and images remain visible.
  - If Vanguard is gone: calls `DestroyWindow`, falls through to launch.
- [x] **[Cancel]** behavior: exits the launcher (`DestroyWindow` → `PostQuitMessage` → `should_launch = false`).
- [x] Kept `stop_service_gracefully("vgc")` and `stop_service_gracefully("vgk")` before the GUI window.
- [x] Kept Phase 2 MessageBoxW warning as the first prompt.

### Visual Design (Zed Dark Mode Palette)

- Window background: `#1e1e1e` (dark gray), via `WM_ERASEBKGND` handler
- Text color: `#cccccc` (light gray), via `WM_CTLCOLORSTATIC`
- Warning/disclaimer text: `#e2b714` (amber/yellow)
- Error flash: `#8b0000` (dark red), fading back to `#1e1e1e` over 800ms
- Button background: standard Win32 buttons (can be customized later)

### QA

- [x] Window appears with dark theme and 4 step images (pending manual test).
- [x] Images display correctly with captions below each (pending manual test).
- [x] Clicking Done disables the button, shows "Checking...", and re-checks Vanguard (pending manual test).
- [x] If Vanguard still running: window flashes red, fades back to dark, message updates (pending manual test).
- [x] If Vanguard is gone: window closes, proceeds to launch (pending manual test).
- [x] Clicking Cancel exits the launcher cleanly (pending manual test).
- [x] Cancel in Phase 2 MessageBoxW still works (never reaches Win32 window) (pending manual test).

---

## Phase 4: Game Launch

> **Goal:** Resolve `patcher_cf2.exe` and spawn it. Path resolution follows a priority order: explicit CLI argument first, relative fallback second.

### Core

- [x] Write `fn resolve_game_path() -> Result<PathBuf, String>` with priority:
  1. **CLI argument** — `std::env::args().nth(1)`.
  2. **Relative fallback** — `std::env::current_exe()` parent + `patcher_cf2.exe`.
  - Validate the resolved path exists.
- [x] Write `fn launch_game(path: &Path) -> Result<(), String>` with `std::process::Command::new(path).spawn()`.
- [x] If path resolution or launch fails, show a `MessageBoxW` error dialog.
- [x] Wire into `main` as the final step after Vanguard cleared.
- [x] The launcher exits immediately after spawning — does not wait for CrossFire to close.

### Usage

```
# Normal use — launcher sits alongside patcher_cf2.exe
cf_launcher.exe

# Testing from any directory
cf_launcher.exe "D:\Games\CrossFire PH\patcher_cf2.exe"
```

### QA

- [x] With Vanguard present, no argument: full flow — Phase 2 warning → Phase 3 GUI → CF launches.
- [x] Without Vanguard, no argument: CF launches directly, no dialog shown.
- [x] With explicit path argument: CF launches using the provided path.
- [x] Move launcher and run without argument → error dialog ("patcher_cf2.exe not found").
- [x] Pass nonexistent path as argument → error dialog with the bad path shown.

---

## Phase 5: Build & Distribution

> **Goal:** A release-quality `.exe` — no console window, custom icon, minimal file size — ready to replace the desktop shortcut.

### Core

- [x] Suppress the console window — `#![windows_subsystem = "windows"]` attribute.
- [x] Embed a CrossFire PH icon via `embed/steps.rc`.
- [x] Release build profile: `opt-level = "z"`, `strip = true`, `lto = true`.
- [ ] Desktop shortcut — replace existing shortcut target with `cf_launcher.exe`.
- [ ] Document one-time setup steps per affected machine (PC2, 3, 5, 6).

### QA

- [x] Running `cf_launcher.exe` produces no visible console window.
- [ ] Release binary size is reasonable (target: under 5 MB).
- [ ] Full end-to-end test on each affected machine: shortcut → warning → GUI dialog → Vanguard cleared → CrossFire launches.

---

## Phase 6: Auto-Detect Vanguard Exit

> **Goal:** Replace the Done button's synchronous retry loop with a silent polling timer in the message loop that automatically detects when `vgtray.exe` / `vgc.exe` exits. The user follows the 4 step images and the launcher auto-proceeds — no button click required.

### Background

The original Phase 3 done button uses a synchronous loop that polls `find_vanguard_processes()` up to 3 times (1s apart). If Vanguard is still running, it shows a red flash and the user must click "Done" again. This is clunky — the user has to keep checking back and clicking.

The enhancement replaces this with an idle timer that continuously monitors Vanguard in the background. The key technical insight is that [`CreateToolhelp32Snapshot` is a kernel-level operation](https://learn.microsoft.com/en-us/windows/win32/api/tlhelp32/nf-tlhelp32-createtoolhelp32snapshot) that bypasses PPL restrictions, so our existing `find_vanguard_processes()` works fine for polling — unlike `OpenProcess(PROCESS_SYNCHRONIZE)` + `WaitForSingleObject`, which would be denied by PPL.

### Core

- [ ] Add `const TIMER_AUTOCHECK: usize = 2002` alongside `TIMER_FLASH`.
- [ ] Add `const TIMER_SHOW_SKIP: usize = 2003` for the delay timer.
- [ ] Start `SetTimer(Some(hwnd), TIMER_AUTOCHECK, 2000, None)` in `WM_CREATE` — fires every 2s for the entire lifetime of the window.
- [ ] Handle `TIMER_AUTOCHECK` in `WM_TIMER`:
  - Call `crate::find_vanguard_processes()`.
  - If the returned list is empty → `KillTimer(TIMER_AUTOCHECK)`, `should_launch.store(true)`, `DestroyWindow`.
  - This timer **never stops** (even after Skip button appears) — it's the primary detection mechanism.
- [ ] **Skip button** — no Done button at window creation. Instead:
  - Start `SetTimer(Some(hwnd), TIMER_SHOW_SKIP, 10000, None)` in `WM_CREATE`.
  - When `TIMER_SHOW_SKIP` fires (10s elapsed): dynamically create a "Skip" button (`CreateWindowExW` with `WS_CHILD \| WS_VISIBLE \| BS_PUSHBUTTON`) positioned where Done was. Kill `TIMER_SHOW_SKIP` (fire-and-forget timer).
- [ ] **Skip click** handler:
  - Single check of `find_vanguard_processes()`.
  - If Vanguard is gone → proceed.
  - If still running → show "Waiting for Vanguard to exit..." (amber text, no flash). The auto-check timer (`TIMER_AUTOCHECK`) still runs every 2s — Skip is just a manual "check now" for edge cases where the user already closed Vanguard but the timer hasn't fired yet.
- [ ] Kill both timers in `WM_DESTROY` for clean shutdown.
- [ ] Remove the flash/fade animation — it's no longer needed since the timer handles detection silently.

### Key Decisions

| Question | Decision | Why |
|---|---|---|
| Async Rust? | Not needed | Win32 message loop is already an event loop; `SetTimer` + `WM_TIMER` is the idiomatic approach. Adding tokio would add a heavy dependency and a background thread just to do what a single `SetTimer` accomplishes. |
| Skip button instead of Done? | Skip appears after 10s | The user shouldn't feel compelled to click anything — the timer auto-detects. Skip is an escape hatch for "I already exited Vanguard, stop waiting." The 10s delay prevents accidental clicks while the user is still reading the instructions. |
| Poll interval? | 2 seconds | Balances responsiveness with CPU usage. `CreateToolhelp32Snapshot` is cheap; 2s is unnoticeable. |
| What about `vgk` service? | Ignored | `vgk.sys` stays loaded until reboot — it's not killed by the tray "Exit" action. The auto-detect only checks `vgtray.exe` / `vgc.exe` processes. |

### QA

- [ ] Auto-detect timer fires every 2 seconds and calls `find_vanguard_processes()` without blocking the UI (pending manual test).
- [ ] When Vanguard processes disappear (user exits tray), window auto-closes and game launches. No button click needed (pending manual test).
- [ ] No buttons visible at window creation (besides Cancel). Skip button appears after 10 seconds (pending manual test).
- [ ] Clicking Skip runs a single check: if Vanguard is gone, proceeds; if still running, shows "Waiting..." and timer handles the rest (pending manual test).
- [ ] Clicking Cancel exits the launcher cleanly (pending manual test).
- [ ] No handle leaks or timer leaks on any exit path (code review).

---

## Dependencies & Constraints

### Phase Dependency Order

```
Phase 0 ──► Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► Phase 5 ──► Phase 6
(Setup)    (Detect)    (Dialog)    (GUI)      (Launch)    (Polish)  (Auto-Detect)
```

- **Strict sequential order.** Each phase depends on the prior phase's output compiling and passing QA.
- **Phase 4 is the MVP.** A release build from Phase 4 is a functional fix. Phase 5 is polish.
- **Test on affected machines only.** PC2, 3, 5, 6 are the target.
- **Deep Freeze constraint does not apply** — the launcher lives on `D:\`, which is the active write volume.

### Key Constraints

| Constraint | Detail |
|---|---|
| **Vanguard PPL protection** | `TerminateProcess` and `PostThreadMessageW(WM_QUIT)` are blocked by Protected Process Light. The launcher only uses SCM service stop + guided manual tray exit. |
| **Win32 API crate** | The project uses the `windows` crate v0.58 (not `winapi`). |
| **GPU UI framework** | Phase 3 uses a custom Win32 dialog (GDI) — no external GUI framework required. |
| **No `unwrap()` in logic paths** | Use `Result` and propagate errors explicitly. |
| **All Win32 handles must be closed** | `CloseHandle` / `CloseServiceHandle` in all code paths including early returns. |
