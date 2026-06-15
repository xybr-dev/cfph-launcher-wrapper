#![allow(unsafe_op_in_unsafe_fn)]

use std::mem;
use std::path::Path;
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx,
    SC_HANDLE, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_CONTROL_STOP,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED,
};
use windows::Win32::UI::WindowsAndMessaging::{
    IDOK, MB_ICONWARNING, MB_OK, MB_OKCANCEL, MessageBoxW,
};
use windows::core::PCWSTR;

mod gui;

struct SnapshotHandle(HANDLE);
impl Drop for SnapshotHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

struct ServiceHandle(SC_HANDLE);
impl Drop for ServiceHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseServiceHandle(self.0);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ServiceState {
    Stopped,
    Running,
}

fn is_vanguard_process(entry: &PROCESSENTRY32W) -> bool {
    let exe = String::from_utf16_lossy(&entry.szExeFile);
    let exe = exe.trim_end_matches('\0').trim_end();
    exe.eq_ignore_ascii_case("vgc.exe") || exe.eq_ignore_ascii_case("vgtray.exe")
}

fn find_vanguard_processes() -> Result<Vec<(u32, String)>, Box<dyn std::error::Error>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?;
    let _guard = SnapshotHandle(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut procs = Vec::new();
    if unsafe { Process32FirstW(snapshot, &mut entry).is_ok() } {
        if is_vanguard_process(&entry) {
            let name = String::from_utf16_lossy(&entry.szExeFile);
            procs.push((
                entry.th32ProcessID,
                name.trim_end_matches('\0').trim_end().to_string(),
            ));
        }
        while unsafe { Process32NextW(snapshot, &mut entry).is_ok() } {
            if is_vanguard_process(&entry) {
                let name = String::from_utf16_lossy(&entry.szExeFile);
                procs.push((
                    entry.th32ProcessID,
                    name.trim_end_matches('\0').trim_end().to_string(),
                ));
            }
        }
    }
    Ok(procs)
}

fn check_vgk_service() -> Option<ServiceState> {
    let scm = unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) }.ok()?;
    let _g = ServiceHandle(scm);
    let n = wstring("vgk");
    let h = unsafe { OpenServiceW(scm, PCWSTR(n.as_ptr()), SERVICE_QUERY_STATUS) }.ok()?;
    let _g = ServiceHandle(h);
    let mut st: SERVICE_STATUS = unsafe { mem::zeroed() };
    let mut need: u32 = 0;
    unsafe {
        QueryServiceStatusEx(
            h,
            SC_STATUS_PROCESS_INFO,
            Some(std::slice::from_raw_parts_mut(
                &mut st as *mut _ as *mut u8,
                mem::size_of::<SERVICE_STATUS>(),
            )),
            &mut need,
        )
        .ok()?
    };
    let s = st.dwCurrentState.0 as u32;
    if s == SERVICE_RUNNING.0 {
        Some(ServiceState::Running)
    } else if s == SERVICE_STOPPED.0 {
        Some(ServiceState::Stopped)
    } else {
        Some(ServiceState::Running)
    }
}

fn stop_service_gracefully(name: &str) -> bool {
    let nw = wstring(name);
    let scm = match unsafe { OpenSCManagerW(PCWSTR::null(), PCWSTR::null(), SC_MANAGER_CONNECT) } {
        Ok(h) => h,
        Err(e) => {
            eprintln!("  ✗ SCM: {}", e);
            return false;
        }
    };
    let _g1 = ServiceHandle(scm);
    let svc = match unsafe {
        OpenServiceW(
            scm,
            PCWSTR(nw.as_ptr()),
            SERVICE_STOP | SERVICE_QUERY_STATUS,
        )
    } {
        Ok(h) => h,
        Err(_) => {
            println!("  ℹ  {} not found.", name);
            return true;
        }
    };
    let _g2 = ServiceHandle(svc);
    let mut st = unsafe { mem::zeroed::<SERVICE_STATUS>() };
    let mut need: u32 = 0;
    if unsafe {
        QueryServiceStatusEx(
            svc,
            SC_STATUS_PROCESS_INFO,
            Some(std::slice::from_raw_parts_mut(
                &mut st as *mut _ as *mut u8,
                mem::size_of::<SERVICE_STATUS>(),
            )),
            &mut need,
        )
    }
    .is_err()
    {
        eprintln!("  ✗ Failed to query {} status.", name);
        return false;
    }
    let s = st.dwCurrentState.0 as u32;
    if s == SERVICE_STOPPED.0 {
        println!("  ℹ  {} already stopped.", name);
        return true;
    }
    if s != SERVICE_RUNNING.0 {
        println!("  ℹ  {} state={}.", name, s);
        return true;
    }
    println!("  ⏹  Stopping {} ...", name);
    match unsafe { ControlService(svc, SERVICE_CONTROL_STOP, &mut st) } {
        Ok(()) => {
            for i in 0..50 {
                std::thread::sleep(std::time::Duration::from_millis(100));
                let mut ps = unsafe { mem::zeroed::<SERVICE_STATUS>() };
                let mut pn: u32 = 0;
                if unsafe {
                    QueryServiceStatusEx(
                        svc,
                        SC_STATUS_PROCESS_INFO,
                        Some(std::slice::from_raw_parts_mut(
                            &mut ps as *mut _ as *mut u8,
                            mem::size_of::<SERVICE_STATUS>(),
                        )),
                        &mut pn,
                    )
                }
                .is_err()
                {
                    break;
                }
                if ps.dwCurrentState.0 as u32 == SERVICE_STOPPED.0 {
                    println!("  ✓ {} stopped.", name);
                    return true;
                }
                if i == 49 {
                    eprintln!("  ⚠  {} stop timeout.", name);
                }
            }
            false
        }
        Err(e) => {
            let code = unsafe { GetLastError() };
            if code.0 == 1061 {
                println!("  ⚠  {} is tamper-resistant.", name);
                false
            } else {
                eprintln!("  ✗ Failed to stop {}: {}", name, e);
                false
            }
        }
    }
}

// ── Instruction Dialog ─────────────────────────────────────────────────

/// Short Phase 2 warning dialog shown before the gpui window.
/// Warns about Vanguard blocking CrossFire PH and the need to restart
/// for League/Valorant.
fn show_vanguard_warning() -> bool {
    let msg = "Vanguard is preventing CrossFire PH from starting.\n\n\
               If you want to play League of Legends or Valorant afterwards,\n\
               you must restart this PC after exiting Vanguard.\n\n\
               Click OK to open the step-by-step guide, or Cancel to exit.";
    unsafe {
        matches!(
            MessageBoxW(
                None,
                PCWSTR(wstring(msg).as_ptr()),
                PCWSTR(wstring("CrossFire PH Launcher").as_ptr()),
                MB_OKCANCEL | MB_ICONWARNING,
            ),
            IDOK
        )
    }
}

fn wstring(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn build_summary(procs: &[(u32, String)], vgk: Option<ServiceState>) -> Vec<String> {
    let mut l = Vec::new();
    for (pid, name) in procs {
        l.push(format!("  Process: {} (PID {})", name, pid));
    }
    if matches!(vgk, Some(ServiceState::Running)) {
        l.push("  Kernel driver: vgk (active)".to_string());
    }
    l
}

// ── Phase 4: Game Path Resolution & Launch ─────────────────────────────

/// Resolve the path to `patcher_cf2.exe` with the following priority:
/// 1. CLI argument — `std::env::args().nth(1)`
/// 2. Relative fallback — launcher executable parent directory
///
/// Returns an error (with a user-facing message) if the resolved path
/// does not exist or cannot be determined.
fn resolve_game_path() -> Result<std::path::PathBuf, String> {
    // Priority 1: explicit CLI argument
    if let Some(arg) = std::env::args().nth(1) {
        let p = std::path::PathBuf::from(&arg);
        if p.exists() {
            return Ok(p);
        }
        return Err(format!("Specified path not found:\n{}", arg));
    }

    // Priority 2: relative to the launcher executable
    let exe =
        std::env::current_exe().map_err(|e| format!("Cannot determine launcher path: {}", e))?;
    let dir = exe
        .parent()
        .ok_or_else(|| "Cannot determine launcher directory".to_string())?;
    let p = dir.join("patcher_cf2.exe");
    if p.exists() {
        return Ok(p);
    }
    Err(format!(
        "patcher_cf2.exe not found at:\n{}\n\nPlace cf_launcher.exe in the same directory as \
         patcher_cf2.exe, or pass the full path as a command-line argument.",
        p.display()
    ))
}

/// Spawn `patcher_cf2.exe` and return immediately. The launcher does
/// not wait for CrossFire PH to close.
fn launch_game(path: &Path) -> Result<(), String> {
    std::process::Command::new(path)
        .spawn()
        .map_err(|e| format!("Failed to launch {}:\n{}", path.display(), e))?;
    Ok(())
}

/// Show a MessageBoxW error dialog. Prepends a management-contact message
/// before the caller-supplied error text.
fn show_error_dialog(msg: &str) {
    let full = format!(
        "Please contact management and tell them about this error. \
         If possible, you may send this as screenshot accordingly.\n\n{}",
        msg
    );
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(wstring(&full).as_ptr()),
            PCWSTR(wstring("CrossFire PH Launcher - Error").as_ptr()),
            MB_OK | MB_ICONWARNING,
        );
    }
}

fn main() {
    let procs = match find_vanguard_processes() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: {}", e);
            return;
        }
    };
    let vgk = check_vgk_service();
    let has = !procs.is_empty() || matches!(vgk, Some(ServiceState::Running));

    eprintln!("[DEBUG] vgk: {:?} | procs: {}", vgk, procs.len());
    for (pid, name) in &procs {
        eprintln!("  [DEBUG]   PID {}: {}", pid, name);
    }

    if !has {
        println!("No Vanguard detected. Launching...");
        println!("── Launch ──");
        match resolve_game_path() {
            Ok(path) => {
                println!("Game path: {}", path.display());
                if let Err(e) = launch_game(&path) {
                    show_error_dialog(&e);
                }
            }
            Err(e) => {
                show_error_dialog(&e);
            }
        }
        return;
    }

    println!("Vanguard detected:");
    for l in &build_summary(&procs, vgk) {
        println!("{}", l);
    }

    println!("\n── Service Stop ──");
    stop_service_gracefully("vgc");
    stop_service_gracefully("vgk");

    println!("\n── GUI Dialog ──");
    if !show_vanguard_warning() {
        eprintln!("User cancelled.");
        std::process::exit(0);
    }

    if !gui::run_gui() {
        std::process::exit(0);
    }

    println!("\n── Launch ──");
    match resolve_game_path() {
        Ok(path) => {
            println!("Game path: {}", path.display());
            if let Err(e) = launch_game(&path) {
                show_error_dialog(&e);
            }
        }
        Err(e) => {
            show_error_dialog(&e);
        }
    }
}
