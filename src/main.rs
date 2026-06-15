use std::mem;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{IDOK, MB_ICONWARNING, MB_OKCANCEL, MessageBoxW};
use windows::core::w;

struct SnapshotHandle(HANDLE);

impl Drop for SnapshotHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn is_vanguard_process(entry: &PROCESSENTRY32W) -> bool {
    let exe_name = String::from_utf16_lossy(&entry.szExeFile);
    let exe_name = exe_name.trim_end_matches('\0');
    exe_name.eq_ignore_ascii_case("vgc.exe") || exe_name.eq_ignore_ascii_case("vgtray.exe")
}

fn find_vanguard_processes() -> Result<Vec<u32>, Box<dyn std::error::Error>> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }?;
    let _guard = SnapshotHandle(snapshot);

    let mut entry: PROCESSENTRY32W = unsafe { mem::zeroed() };
    entry.dwSize = mem::size_of::<PROCESSENTRY32W>() as u32;

    let mut pids = Vec::new();

    if unsafe { Process32FirstW(snapshot, &mut entry).is_ok() } {
        if is_vanguard_process(&entry) {
            pids.push(entry.th32ProcessID);
        }

        while unsafe { Process32NextW(snapshot, &mut entry).is_ok() } {
            if is_vanguard_process(&entry) {
                pids.push(entry.th32ProcessID);
            }
        }
    }

    Ok(pids)
}

fn show_vanguard_warning() -> bool {
    // MessageBoxW returns MESSAGEBOX_RESULT (infallible in windows crate v0.58).
    // Any unexpected value (neither IDOK nor IDCANCEL) is treated as Cancel.
    let result = unsafe {
        MessageBoxW(
            None,
            w!(
                "Riot Vanguard is currently running and will be force-closed \
to allow CrossFire PH to start.\n\n\
If you want to play League of Legends or Valorant afterwards, \
you will need to restart this PC.\n\n\
Click OK to proceed, or Cancel to exit."
            ),
            w!("CrossFire PH Launcher"),
            MB_OKCANCEL | MB_ICONWARNING,
        )
    };

    result == IDOK
}

fn main() {
    match find_vanguard_processes() {
        Ok(pids) if pids.is_empty() => {
            println!("Vanguard not running.");
        }
        Ok(pids) => {
            let pid_list: Vec<String> = pids.iter().map(|pid| pid.to_string()).collect();
            println!("Vanguard found: [{}]", pid_list.join(", "));

            if show_vanguard_warning() {
                println!("User confirmed. Proceeding...");
            } else {
                std::process::exit(0);
            }
        }
        Err(e) => {
            eprintln!("Error enumerating processes: {}", e);
        }
    }
}
