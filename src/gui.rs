use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::PCWSTR;

// ── Raw FFI for functions not exported by windows crate ──────────────────

#[link(name = "user32")]
unsafe extern "system" {
    fn EnableWindow(hWnd: HWND, bEnable: i32) -> i32;
    fn GetDlgCtrlID(hWnd: HWND) -> i32;
}

// ── Constants ────────────────────────────────────────────────────────────

const WINDOW_W: i32 = 1160;
const WINDOW_H: i32 = 600;
const ID_DONE: u32 = 1001;
const ID_CANCEL: u32 = 1002;
const TIMER_FLASH: usize = 2001;
const FLASH_DURATION_MS: u64 = 800;

// Static control styles (not directly exported by windows crate v0.61 WWaM)
const SS_BITMAP: u32 = 0x0000000E;
const SS_CENTER: u32 = 0x00000001;

// Colors in BGR format; COLORREF is 0x00BBGGRR
const COLOR_DARK: COLORREF = COLORREF(0x001e1e1e);
const COLOR_AMBER: COLORREF = COLORREF(0x0014b7e2);
const COLOR_LIGHT: COLORREF = COLORREF(0x00cccccc);
const COLOR_RED: COLORREF = COLORREF(0x004444ff);
const COLOR_DARK_RED: COLORREF = COLORREF(0x0000008b);

// ── Helper Functions ─────────────────────────────────────────────────────

fn lo_word(val: usize) -> u16 {
    (val & 0xFFFF) as u16
}

fn hi_word(val: usize) -> u16 {
    ((val >> 16) & 0xFFFF) as u16
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn get_tick_ms() -> u64 {
    unsafe { GetTickCount64() as u64 }
}

fn lerp_color(from: COLORREF, to: COLORREF, t: f32) -> COLORREF {
    let f = from.0;
    let t_c = to.0;
    let fr = (f & 0xFF) as f32;
    let fg = ((f >> 8) & 0xFF) as f32;
    let fb = ((f >> 16) & 0xFF) as f32;
    let tr = (t_c & 0xFF) as f32;
    let tg = ((t_c >> 8) & 0xFF) as f32;
    let tb = ((t_c >> 16) & 0xFF) as f32;
    let r = (fr + (tr - fr) * t) as u32;
    let g = (fg + (tg - fg) * t) as u32;
    let b = (fb + (tb - fb) * t) as u32;
    COLORREF((b << 16) | (g << 8) | r)
}

fn id_as_hmenu(id: u32) -> HMENU {
    HMENU(id as *mut _)
}

fn hinst() -> HINSTANCE {
    unsafe { HINSTANCE(GetModuleHandleW(None).unwrap().0) }
}

/// Convert an integer resource ID to PCWSTR (equivalent of MAKEINTRESOURCEW).
fn resource_id(id: u16) -> PCWSTR {
    PCWSTR(id as usize as *const u16)
}

// ── State ────────────────────────────────────────────────────────────────

struct DialogState {
    bitmaps: [Option<HBITMAP>; 4],
    checking: bool,
    status_msg: String,
    status_error: bool,
    bg_color: COLORREF,
    flash_start_ms: u64,
    flash_active: bool,
    should_launch: Arc<AtomicBool>,
}

impl DialogState {
    fn set_status(&mut self, msg: &str, is_error: bool) {
        self.status_msg = msg.to_string();
        self.status_error = is_error;
    }

    fn start_flash(&mut self) {
        self.flash_active = true;
        self.bg_color = COLOR_DARK_RED;
        self.flash_start_ms = get_tick_ms();
    }
}

// ── Window Procedure ─────────────────────────────────────────────────────

unsafe extern "system" fn dlg_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            let cs = &*(lparam.0 as *const CREATESTRUCTW);
            let state_ptr = cs.lpCreateParams as *mut DialogState;
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
            let state = &mut *state_ptr;
            let hi = hinst();
            let style_img = WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | SS_BITMAP);
            let style_txt = WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | SS_CENTER);
            let style_btn = WINDOW_STYLE(WS_CHILD.0 | WS_VISIBLE.0 | BS_PUSHBUTTON as u32);

            // ── Load BMPs from embedded resources ──
            let img_ids = [1010u32, 1011, 1012, 1013];
            let img_x = [30i32, 256, 480, 706];
            let img_sizes: [(i32, i32); 4] = [(210, 97), (208, 145), (210, 182), (405, 183)];

            for i in 0..4 {
                match LoadImageW(
                    Some(hinst()),
                    resource_id(img_ids[i] as u16),
                    IMAGE_BITMAP,
                    0,
                    0,
                    LR_DEFAULTCOLOR,
                ) {
                    Ok(h) if !h.is_invalid() => {
                        state.bitmaps[i] = Some(HBITMAP(h.0));
                    }
                    _ => {}
                }

                let (w, h) = img_sizes[i];
                if let (Ok(himg), Some(hbm)) = (
                    CreateWindowExW(
                        WS_EX_STATICEDGE,
                        PCWSTR(to_wide("STATIC").as_ptr()),
                        PCWSTR::null(),
                        style_img,
                        img_x[i],
                        100,
                        w,
                        h,
                        Some(hwnd),
                        Some(id_as_hmenu(img_ids[i])),
                        Some(hi),
                        None,
                    ),
                    state.bitmaps[i],
                ) {
                    let _ = SendMessageW(
                        himg,
                        STM_SETIMAGE,
                        Some(WPARAM(IMAGE_BITMAP.0 as _)),
                        Some(LPARAM(hbm.0 as isize)),
                    );
                }
            }

            // ── Fonts ──
            let font_large = CreateFontW(
                -20,
                0,
                0,
                0,
                FW_DONTCARE.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                PROOF_QUALITY,
                FF_DONTCARE.0 as u32,
                PCWSTR(to_wide("Segoe UI").as_ptr()),
            );
            let font_small = CreateFontW(
                -14,
                0,
                0,
                0,
                FW_DONTCARE.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                PROOF_QUALITY,
                FF_DONTCARE.0 as u32,
                PCWSTR(to_wide("Segoe UI").as_ptr()),
            );
            let _font_cap = CreateFontW(
                -12,
                0,
                0,
                0,
                FW_DONTCARE.0 as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                PROOF_QUALITY,
                FF_DONTCARE.0 as u32,
                PCWSTR(to_wide("Segoe UI").as_ptr()),
            );

            // ── Captions below images ──
            // Caption widths wider than images to prevent text clipping
            let cap_widths = [260i32, 260, 260, 405]; // wider than img_sizes for text room
            let cap_x = [5i32, 230, 455, 706]; // centered under each image
            let cap_texts = [
                "Click the ^ to show\nhidden icons",
                "Right-click the\nVanguard icon",
                "Click 'Exit Vanguard'\nin the menu",
                "Click 'Yes'\nto confirm",
            ];
            for i in 0..4 {
                let (_, h) = img_sizes[i];
                if let Ok(hctrl) = CreateWindowExW(
                    WS_EX_TRANSPARENT,
                    PCWSTR(to_wide("STATIC").as_ptr()),
                    PCWSTR(to_wide(cap_texts[i]).as_ptr()),
                    style_txt,
                    cap_x[i],
                    100 + h + 4,
                    cap_widths[i],
                    48,
                    Some(hwnd),
                    Some(id_as_hmenu(1030 + i as u32)),
                    Some(hi),
                    None,
                ) {
                    let _ = SendMessageW(
                        hctrl,
                        WM_SETFONT,
                        Some(WPARAM(_font_cap.0 as _)),
                        Some(LPARAM(1)),
                    );
                }
            }

            // ── Header ──
            if let Ok(header) = CreateWindowExW(
                WS_EX_TRANSPARENT,
                PCWSTR(to_wide("STATIC").as_ptr()),
                PCWSTR(to_wide("Riot Vanguard is running. CrossFire PH cannot start while Vanguard is active.").as_ptr()),
                style_txt,
                0, 16, WINDOW_W, 28,
                Some(hwnd), Some(id_as_hmenu(1020)), Some(hi), None,
            ) {
                let _ = SendMessageW(header, WM_SETFONT, Some(WPARAM(font_large.0 as _)), Some(LPARAM(1)));
            }

            // ── Disclaimer ──
            if let Ok(discl) = CreateWindowExW(
                WS_EX_TRANSPARENT,
                PCWSTR(to_wide("STATIC").as_ptr()),
                PCWSTR(to_wide("If you want to play League of Legends or Valorant afterwards, you must restart this PC.").as_ptr()),
                style_txt,
                0, 48, WINDOW_W, 24,
                Some(hwnd), Some(id_as_hmenu(1021)), Some(hi), None,
            ) {
                let _ = SendMessageW(discl, WM_SETFONT, Some(WPARAM(font_small.0 as _)), Some(LPARAM(1)));
            }

            // ── Done button ──
            let _ = CreateWindowExW(
                WS_EX_TRANSPARENT,
                PCWSTR(to_wide("BUTTON").as_ptr()),
                PCWSTR(to_wide("Done, Launch CrossFire PH").as_ptr()),
                style_btn,
                WINDOW_W / 2 - 240,
                480,
                220,
                36,
                Some(hwnd),
                Some(id_as_hmenu(ID_DONE)),
                Some(hi),
                None,
            );

            // ── Cancel button ──
            let _ = CreateWindowExW(
                WS_EX_TRANSPARENT,
                PCWSTR(to_wide("BUTTON").as_ptr()),
                PCWSTR(to_wide("Cancel").as_ptr()),
                style_btn,
                WINDOW_W / 2 + 20,
                480,
                110,
                36,
                Some(hwnd),
                Some(id_as_hmenu(ID_CANCEL)),
                Some(hi),
                None,
            );

            // ── Status text ──
            if let Ok(hstatus) = CreateWindowExW(
                WS_EX_TRANSPARENT,
                PCWSTR(to_wide("STATIC").as_ptr()),
                PCWSTR::null(),
                style_txt,
                0,
                440,
                WINDOW_W,
                28,
                Some(hwnd),
                Some(id_as_hmenu(1022)),
                Some(hi),
                None,
            ) {
                let _ = SendMessageW(
                    hstatus,
                    WM_SETFONT,
                    Some(WPARAM(font_small.0 as _)),
                    Some(LPARAM(1)),
                );
            }

            LRESULT(0)
        }

        WM_COMMAND => {
            let id = lo_word(wparam.0) as u32;
            let code = hi_word(wparam.0) as u32;
            if code == BN_CLICKED {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                if ptr == 0 {
                    return LRESULT(0);
                }
                let state = &mut *(ptr as *mut DialogState);

                if id == ID_DONE && !state.checking {
                    done_clicked(hwnd, state);
                } else if id == ID_CANCEL {
                    cancel_clicked(hwnd, state);
                }
            }
            LRESULT(0)
        }

        WM_TIMER => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if ptr == 0 {
                return LRESULT(0);
            }
            let state = &mut *(ptr as *mut DialogState);

            if wparam.0 == TIMER_FLASH && state.flash_active {
                let elapsed = get_tick_ms().saturating_sub(state.flash_start_ms);
                if elapsed >= FLASH_DURATION_MS {
                    let _ = KillTimer(Some(hwnd), TIMER_FLASH);
                    state.flash_active = false;
                    state.bg_color = COLOR_DARK;
                } else {
                    let t = 1.0 - (elapsed as f64 / FLASH_DURATION_MS as f64);
                    state.bg_color = lerp_color(COLOR_DARK_RED, COLOR_DARK, t as f32);
                }
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
            LRESULT(0)
        }

        WM_ERASEBKGND => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            let bg = if ptr != 0 {
                let s = &*(ptr as *mut DialogState);
                s.bg_color
            } else {
                COLOR_DARK
            };
            let hdc = HDC(wparam.0 as *mut _);
            let mut rect = RECT::default();
            if GetClientRect(hwnd, &mut rect).is_ok() {
                let brush = CreateSolidBrush(bg);
                let _ = FillRect(hdc, &rect, brush);
                let _ = DeleteObject(brush.into());
            }
            LRESULT(1)
        }

        WM_CTLCOLORSTATIC => {
            let hdc = HDC(wparam.0 as *mut _);
            let hctl = HWND(lparam.0 as *mut _);
            let id = GetDlgCtrlID(hctl) as u32;

            match id {
                1020 | 1021 => {
                    let _ = SetTextColor(hdc, COLOR_AMBER);
                    let _ = SetBkMode(hdc, BACKGROUND_MODE(1)); // TRANSPARENT = 1
                }
                1022 => {
                    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
                    let color = if ptr != 0 {
                        let s = &*(ptr as *mut DialogState);
                        if s.status_error {
                            COLOR_RED
                        } else {
                            COLOR_LIGHT
                        }
                    } else {
                        COLOR_LIGHT
                    };
                    let _ = SetTextColor(hdc, color);
                    let _ = SetBkMode(hdc, BACKGROUND_MODE(1));
                }
                1030..=1033 => {
                    let _ = SetTextColor(hdc, COLOR_LIGHT);
                    let _ = SetBkMode(hdc, BACKGROUND_MODE(1));
                }
                _ => {
                    let _ = SetTextColor(hdc, COLOR_LIGHT);
                    let _ = SetBkMode(hdc, BACKGROUND_MODE(1));
                }
            }

            LRESULT(GetStockObject(NULL_BRUSH).0 as _)
        }

        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }

        WM_DESTROY => {
            let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA);
            if ptr != 0 {
                let state = Box::from_raw(ptr as *mut DialogState);
                for hbm in state.bitmaps.iter().flatten() {
                    let _ = DeleteObject(HGDIOBJ(hbm.0));
                }
            }
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

// ── Click Handlers ───────────────────────────────────────────────────────

fn done_clicked(hwnd: HWND, state: &mut DialogState) {
    state.checking = true;
    state.status_msg.clear();
    state.status_error = false;
    state.flash_active = false;
    state.bg_color = COLOR_DARK;

    unsafe {
        let h_btn = GetDlgItem(Some(hwnd), ID_DONE as _).unwrap();
        EnableWindow(h_btn, 0);
        let _ = SetDlgItemTextW(
            hwnd,
            ID_DONE as _,
            PCWSTR(to_wide("Checking Vanguard...").as_ptr()),
        );
        let _ = InvalidateRect(Some(hwnd), None, true);
    }

    // Retry up to 3 times (1s apart) — Vanguard may still be shutting down
    let mut retries = 0;
    let max_retries = 3;
    let result = loop {
        let procs = crate::find_vanguard_processes();
        match procs {
            Ok(list) if list.is_empty() => {
                break Ok(());
            }
            Ok(_) if retries < max_retries => {
                retries += 1;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            _ => {
                break Err(());
            }
        }
    };

    match result {
        Ok(()) => {
            state.should_launch.store(true, Ordering::Release);
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
        Err(()) => {
            state.checking = false;
            state.set_status("Vanguard is still running. Please try again.", true);
            state.start_flash();

            unsafe {
                let h_btn = GetDlgItem(Some(hwnd), ID_DONE as _).unwrap();
                EnableWindow(h_btn, 1);
                let _ = SetDlgItemTextW(
                    hwnd,
                    ID_DONE as _,
                    PCWSTR(to_wide("Done, Launch CrossFire PH").as_ptr()),
                );
                let _ = SetDlgItemTextW(hwnd, 1022, PCWSTR(to_wide(&state.status_msg).as_ptr()));
                let _ = SetTimer(Some(hwnd), TIMER_FLASH, 16, None);
                let _ = InvalidateRect(Some(hwnd), None, true);
            }
        }
    }
}

fn cancel_clicked(hwnd: HWND, state: &mut DialogState) {
    state.should_launch.store(false, Ordering::Release);
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

// ── Public Entry Point ───────────────────────────────────────────────────

pub fn run_gui() -> bool {
    let should_launch = Arc::new(AtomicBool::new(false));
    let hi = hinst();

    // Register window class
    let class_name = to_wide("CfVanguardDialog");
    let wc = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(dlg_proc),
        hInstance: hi,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW).unwrap() },
        hbrBackground: HBRUSH(1 as _),
        lpszClassName: PCWSTR(class_name.as_ptr()),
        ..Default::default()
    };

    let atom = unsafe { RegisterClassW(&wc) };
    if atom == 0 {
        eprintln!("Failed to register window class");
        return false;
    }

    // Prepare state
    let state = Box::new(DialogState {
        bitmaps: [None, None, None, None],
        checking: false,
        status_msg: String::new(),
        status_error: false,
        bg_color: COLOR_DARK,
        flash_start_ms: 0,
        flash_active: false,
        should_launch: should_launch.clone(),
    });

    // Center window
    let sw = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let sh = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let x = (sw - WINDOW_W).max(0) / 2;
    let y = (sh - WINDOW_H).max(0) / 2;

    let _hwnd = match unsafe {
        CreateWindowExW(
            WS_EX_DLGMODALFRAME,
            PCWSTR(class_name.as_ptr()),
            PCWSTR(to_wide("CrossFire PH Launcher").as_ptr()),
            WINDOW_STYLE(WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_VISIBLE.0),
            x,
            y,
            WINDOW_W,
            WINDOW_H,
            None,
            None,
            Some(hi),
            Some(&*state as *const _ as *const _),
        )
    } {
        Ok(h) if !h.is_invalid() => h,
        _ => {
            eprintln!("Failed to create window");
            return false;
        }
    };

    // Transfer ownership to window
    let _ = Box::into_raw(state);

    // Message loop
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            let _ = DispatchMessageW(&msg);
        }
    }

    should_launch.load(Ordering::Acquire)
}
