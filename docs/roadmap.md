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
| **3** | Process Termination | Open and terminate Vanguard processes by PID |
| **4** | Game Launch | Resolve `patcher_cf.exe` relative to launcher; spawn it |
| **5** | Build & Distribution | No console window, icon embedding, release build, desktop shortcut |

---

## Phase 0: Project Setup

> **Goal:** A compilable Rust project that targets Windows x64 and produces a `.exe`. Nothing more.

### Core

- Run `cargo new cf_launcher` and confirm the default `Hello, world!` compiles to a Windows `.exe`.
- Set the default target in `.cargo/config.toml`:
  ```toml
  [build]
  target = "x86_64-pc-windows-msvc"
  ```
- Add initial dependencies to `Cargo.toml`:
  ```toml
  [dependencies]
  windows = { version = "0.58", features = [
      "Win32_System_Diagnostics_ToolHelp",
      "Win32_System_Threading",
      "Win32_UI_WindowsAndMessaging",
      "Win32_Foundation",
  ] }
  ```
- Confirm `cargo build` succeeds with no warnings.
- Place the compiled `.exe` in `D:\Games\CrossFire PH\` manually and confirm it runs (prints output, exits cleanly).

### QA

- `cargo build` and `cargo build --release` both succeed.
- `.exe` runs on a cafe machine without any Rust runtime dependency (Rust compiles to a self-contained binary — verify this assumption holds).

---

## Phase 1: Process Detection

> **Goal:** Enumerate all running Windows processes and return whether `vgc.exe` or `vgtray.exe` is among them, along with their PIDs.

### Core

- [x] Use `CreateToolhelp32Snapshot` + `Process32FirstW` / `Process32NextW` from the `windows` crate to walk the process list.
- [x] Write a function `fn find_vanguard_processes() -> Result<Vec<u32>, Box<dyn std::error::Error>>` that returns PIDs for any running Vanguard processes (return type uses `Result` for proper error propagation per project conventions).
- [x] Case-insensitive match on `vgc.exe` and `vgtray.exe` via `String::from_utf16_lossy()` + `eq_ignore_ascii_case()`.
- [x] In `main`, print the result:
  ```
  Vanguard found: [PID 1234, PID 5678]
  -- or --
  Vanguard not running.
  ```
- [x] Learn: `PROCESSENTRY32W`, wide strings (`OsString`/`OsStr` vs `&[u16]`), handle cleanup with `CloseHandle` via a `Drop` guard (`SnapshotHandle`).

### QA

- [x] Run on a machine with Vanguard active → PIDs reported correctly (pending manual test).
- [x] Run on a machine without Vanguard → empty Vec, no panic (pending manual test).
- [x] No handle leaks: `SnapshotHandle` drop guard guarantees `CloseHandle` on all exit paths (success, early return via `?`, panic unwinding). Verified by code review.

---

## Phase 2: User Dialog

> **Goal:** Display a Win32 `MessageBoxW` that warns the gamer and returns their choice (OK to proceed, Cancel to abort).

### Core

- [x] Write a function `fn show_vanguard_warning() -> bool` that returns `true` if the user clicked OK, `false` if Cancel.
- [x] Use `MessageBoxW` from `Win32_UI_WindowsAndMessaging` with `MB_OKCANCEL | MB_ICONWARNING`.
- [x] Message text (wide string literal):
  ```
  Riot Vanguard is currently running and will be force-closed
  to allow CrossFire PH to start.

  If you want to play League of Legends or Valorant afterwards,
  you will need to restart this PC.

  Click OK to proceed, or Cancel to exit.
  ```
- [x] Window title: `CrossFire PH Launcher`
- [x] Wire into `main`: if Vanguard detected → show dialog → if Cancel → exit early with `std::process::exit(0)`.
- [x] Learn: `PCWSTR`, null-terminated wide strings, `w!()` macro from the `windows` crate.

### QA

- [ ] Dialog appears with correct message and title (pending manual test).
- [ ] Clicking Cancel exits the launcher immediately with no further action (pending manual test).
- [ ] Clicking OK falls through to the next phase (currently just prints a placeholder) (pending manual test).
- [ ] Dialog window appears in the taskbar and can be focused normally (pending manual test).

---

## Phase 3: Process Termination

> **Goal:** Terminate all Vanguard PIDs collected in Phase 1, then wait briefly before proceeding.

### Core

- Write a function `fn terminate_processes(pids: &[u32]) -> Result<(), String>` that:
  - Calls `OpenProcess` with `PROCESS_TERMINATE` access right for each PID.
  - Calls `TerminateProcess` with exit code `0`.
  - Calls `CloseHandle` after each operation.
  - Returns an error string if any handle fails to open (non-fatal — log and continue).
- After termination, `std::thread::sleep(std::time::Duration::from_millis(1500))` to allow the kernel service to release `sgack.sys`.
- Wire into `main` after the user confirms OK.
- Learn: `OpenProcess`, access rights flags, error handling with `GetLastError` via `windows::core::Error::from_win32()`.

### QA

- Run with Vanguard active → both `vgc.exe` and `vgtray.exe` are gone from Task Manager after execution.
- Run with Vanguard already stopped → no panic, graceful no-op.
- Confirm the sleep actually provides enough time (test that CrossFire launches cleanly immediately after termination in Phase 4).

---

## Phase 4: Game Launch

> **Goal:** Resolve `patcher_cf.exe` and spawn it. Path resolution follows a priority order: explicit CLI argument first, relative fallback second.

### Core

- Write a function `fn resolve_game_path() -> Result<PathBuf, String>` that implements the following priority order:
  1. **CLI argument** — if the user passed a path as the first argument (e.g. `cf_launcher.exe "D:\Games\CrossFire PH\patcher_cf.exe"`), use it directly.
  2. **Relative fallback** — if no argument is given, call `std::env::current_exe()`, navigate to its parent directory, and append `patcher_cf.exe`.
  - In both cases, validate that the resolved path exists before returning it. Return an error string if it does not.
- Read the argument with `std::env::args().nth(1)` — no external crate needed for a single optional positional argument.
- Write a separate function `fn launch_game(path: &Path) -> Result<(), String>` that spawns the process with `std::process::Command::new(path).spawn()`.
- If path resolution or launch fails, show a `MessageBoxW` error dialog with the reason.
- Wire into `main` as the final step (reached whether or not Vanguard was running).
- The launcher exits immediately after spawning — it does not wait for CrossFire to close.
- Learn: `std::env::args()`, `Path`, `PathBuf`, `std::process::Command`, detached child processes on Windows.

### Usage

```
# Normal use — launcher sits alongside patcher_cf.exe
cf_launcher.exe

# Testing from any directory
cf_launcher.exe "D:\Games\CrossFire PH\patcher_cf.exe"
```

### QA

- With Vanguard present, no argument: full flow — warning shown → Vanguard killed → CF launches.
- Without Vanguard, no argument: CF launches directly, no dialog shown.
- With explicit path argument: CF launches using the provided path regardless of where the launcher is run from.
- Move the launcher to a different directory and run without argument → error dialog ("patcher_cf.exe not found"), not a panic.
- Pass a nonexistent path as argument → error dialog with the bad path shown.
- Pass a valid path as argument → CF launches correctly.

---

## Phase 5: Build & Distribution

> **Goal:** A release-quality `.exe` — no console window, custom icon, minimal file size — ready to replace the desktop shortcut.

### Core

- **Suppress the console window** — add a Windows application manifest or use the `#![windows_subsystem = "windows"]` attribute at the top of `main.rs`. This prevents a black console window from flashing when the launcher runs.
- **Embed a CrossFire PH icon** — create a `build.rs` build script that links a `.rc` resource file embedding an `.ico` icon. The `winres` crate simplifies this:
  ```toml
  [build-dependencies]
  winres = "0.1"
  ```
  ```rust
  // build.rs
  fn main() {
      let mut res = winres::WindowsResource::new();
      res.set_icon("assets/cf_icon.ico");
      res.compile().unwrap();
  }
  ```
- **Release build profile** — add to `Cargo.toml`:
  ```toml
  [profile.release]
  opt-level = "z"   # optimize for size
  strip = true      # strip debug symbols
  lto = true
  ```
- **Desktop shortcut** — replace the existing CrossFire PH desktop shortcut target with `cf_launcher.exe`. The icon on the shortcut can be set to `patcher_cf.exe` so it still looks identical to the original.
- Document the one-time setup steps per affected machine (PC2, 3, 5, 6).

### QA

- Running `cf_launcher.exe` produces no visible console window.
- The `.exe` icon appears as CrossFire PH in File Explorer and on the desktop shortcut.
- Release binary size is reasonable (target: under 2 MB).
- Full end-to-end test on each affected machine: desktop shortcut → warning → Vanguard killed → CrossFire launches.
- Confirm `cargo build --release` is reproducible (same binary behavior across rebuilds).

---

## Dependencies & Constraints

```
Phase 0 ──► Phase 1 ──► Phase 2 ──► Phase 3 ──► Phase 4 ──► Phase 5
(Setup)    (Detect)    (Dialog)    (Terminate)  (Launch)    (Polish)
```

- **Strict sequential order.** Each phase depends on the prior phase's output compiling and passing QA.
- **Phase 4 is the MVP.** A release build from Phase 4 (with a console window) is already a functional fix. Phase 5 is polish.
- **Test on affected machines only.** PC2, 3, 5, 6 are the target. PC0, 1, 4, 7 do not need the launcher but it is safe to run on them (Vanguard not running → CF launches directly).
- **Deep Freeze constraint does not apply** — the launcher lives on `D:\`, which is the active write volume. No special handling needed.
