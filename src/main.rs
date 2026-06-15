#![allow(unsafe_op_in_unsafe_fn)]

use std::mem;
use windows::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Services::{
    CloseServiceHandle, ControlService, OpenSCManagerW, OpenServiceW, QueryServiceStatusEx,
    SC_HANDLE, SC_MANAGER_CONNECT, SC_STATUS_PROCESS_INFO, SERVICE_CONTROL_STOP,
    SERVICE_QUERY_STATUS, SERVICE_RUNNING, SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{IDOK, MB_ICONWARNING, MB_OKCANCEL, MessageBoxW};
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

#[allow(dead_code)]
/// Opens the step images folder in Explorer and shows a dialog
fn show_instruction(first: bool) -> bool {
    // Try to open the steps folder so user can see the images
    let steps_dir = resolve_steps_dir();
    if let Some(dir) = steps_dir {
        let dw = wstring(dir.to_str().unwrap_or(""));
        let verb = wstring("open");
        unsafe {
            let _ = ShellExecuteW(
                None,
                PCWSTR(verb.as_ptr()),
                PCWSTR(dw.as_ptr()),
                PCWSTR::null(),
                PCWSTR::null(),
                windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL,
            );
        }
    }

    let msg = if first {
        "Vanguard is preventing CrossFire PH from starting.\n\n\
         A folder with step-by-step images has been opened.\n\
         Follow these steps to exit Vanguard:\n\n\
         \u{2460} Click \"^\" (show hidden icons) at bottom-right.\n\
         \u{2461} Right-click the Riot Vanguard icon (red/yellow).\n\
         \u{2462} Click \"Exit Vanguard\".\n\
         \u{2463} Click \"Yes\" to confirm.\n\n\
         Click OK after completing all steps, or Cancel to exit."
    } else {
        "Vanguard is still running.\n\n\
         The step images folder is still open for reference.\n\n\
         \u{2460} Click \"^\" to show hidden icons.\n\
         \u{2461} Right-click Riot Vanguard icon.\n\
         \u{2462} Click \"Exit Vanguard\".\n\
         \u{2463} Click \"Yes\" to confirm.\n\n\
         Click OK after exiting, or Cancel."
    };
    unsafe {
        matches!(
            MessageBoxW(
                None,
                PCWSTR(wstring(msg).as_ptr()),
                PCWSTR(wstring("Manual Action Required").as_ptr()),
                MB_OKCANCEL
            ),
            IDOK
        )
    }
}

fn resolve_steps_dir() -> Option<std::path::PathBuf> {
    let rel = std::path::PathBuf::from("docs/steps");
    if rel.is_dir() {
        return Some(rel);
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(p) = exe.parent()
    {
        let p = p.join(&rel);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut cwd = std::env::current_dir().ok()?;
    for _ in 0..3 {
        let p = cwd.join(&rel);
        if p.is_dir() {
            return Some(p);
        }
        cwd.pop();
    }
    None
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
        println!("Launching...");
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

    let steps_dir = resolve_steps_dir().unwrap_or_else(|| {
        eprintln!("Warning: docs/steps/ not found, using relative path");
        std::path::PathBuf::from("docs/steps")
    });

    if !gui::run_gui(&steps_dir) {
        std::process::exit(0);
    }

    println!("\n── Launch ──");
    println!("Launching...");
}
