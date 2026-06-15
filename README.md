# cfph-launcher-wrapper

A lightweight Windows executable that detects running Riot Vanguard processes/vgk driver and launches CrossFire PH — designed for **xygelcafe.net** computer cafe machines.

## Purpose

CrossFire PH is incompatible with Riot Vanguard's kernel-level driver. The root cause: Vanguard's kernel driver (`vgk.sys`) explicitly blocks `sgack.sys` — a kernel-mode anti-cheat/safe-guard driver shipped with CrossFire PH. When `vgk` is active, `sgack.sys` cannot load, and CrossFire PH refuses to start, returning `crossfire init failed`.

This wrapper detects Vanguard upfront, warns the user, and prevents an unnecessary launch attempt.

## How it works

1. Scans running processes for `vgc.exe` and `vgtray.exe`
2. Checks if the `vgk` kernel service is active via SCM
3. If nothing is detected → launches `patcher_cf2.exe`
4. If Vanguard is detected → shows a warning dialog, then a GUI with instructions
5. After the user dismisses the guide, lets them retry — the launcher re-checks for Vanguard before launching

## Screenshots

<div align="center">

![crossfire init failed](https://github.com/user-attachments/assets/8f0c392d-bfa6-498d-bd15-cd1bb6eebdba)

*`patcher_cf2.exe` returns `crossfire init failed` when Vanguard is active.*

---

![Vanguard warning dialog](https://github.com/user-attachments/assets/31ad209f-1649-4dc3-9da2-f087f144cfc7)

*Warning dialog shown when Vanguard is detected.*

---

![Step-by-step instructions](https://github.com/user-attachments/assets/d1a603e6-a1b1-454b-b0d6-4d1f9c29cb12)

*GUI guide for manually exiting Vanguard via the system tray.*

</div>

## Usage

Place `cfph-launcher.exe` in the same directory as `patcher_cf2.exe` and run it. Optionally pass the game path as a command-line argument:

```
cfph-launcher.exe D:\Games\CrossFire PH\patcher_cf2.exe
```

It is recommended to replace the existing `CrossFire PH` desktop shortcut with one pointing to `cfph-launcher.exe` so players always launch through the wrapper.

## Build

```powershell
cargo build --release
```

Requires a Windows toolchain (`x86_64-pc-windows-msvc`).

## License

MIT

## Disclaimer

CrossFire PH is a trademark of **Smilegate Entertainment**. This project is an unofficial utility and is not affiliated with, endorsed by, or sponsored by Smilegate or Riot Games. The CrossFire PH logo and icon are used solely for the purpose of identifying the game being launched. All trademarks and registered trademarks are the property of their respective owners.
