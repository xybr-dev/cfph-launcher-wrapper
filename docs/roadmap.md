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

- [ ] With Vanguard present, no argument: full flow — Phase 2 warning → Phase 3 GUI → CF launches.
- [ ] Without Vanguard, no argument: CF launches directly, no dialog shown.
- [ ] With explicit path argument: CF launches using the provided path.
- [ ] Move launcher and run without argument → error dialog ("patcher_cf2.exe not found").
- [ ] Pass nonexistent path as argument → error dialog with the bad path shown.

---

## Phase 5: Build & Distribution

> **Goal:** A release-quality `.exe` — no console window, custom icon, minimal file size — ready to replace the desktop shortcut.

### Core

- [ ] Suppress the console window — `#![windows_subsystem = "windows"]` attribute.
- [ ] Embed a CrossFire PH icon via `build.rs` + `winres`.
- [ ] Release build profile: `opt-level = "z"`, `strip = true`, `lto = true`.
- [ ] Desktop shortcut — replace existing shortcut target with `cf_launcher.exe`.
- [ ] Document one-time setup steps per affected machine (PC2, 3, 5, 6).

### QA

- [ ] Running `cf_launcher.exe` produces no visible console window.
- [ ] Release binary size is reasonable (target: under 5 MB even with gpui-ce).
- [ ] Full end-to-end test on each affected machine: shortcut → warning → GUI dialog → Vanguard cleared → CrossFire launches.

---

## Dependencies & Constraints

### Phase Dependency Order

```
Phase 0 ──► Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► Phase 5
(Setup)    (Detect)    (Dialog)    (GUI)      (Launch)    (Polish)
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
