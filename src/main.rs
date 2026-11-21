//! Tessera: press Win+Shift+G, pick a rectangle on a 6x6 grid, and the focused window
//! snaps to the matching part of its monitor's work area.

#![windows_subsystem = "windows"]

#[macro_use]
mod log;
mod autostart;
mod hotkey;
mod overlay;
mod placement;
mod tray;
mod util;

use std::cell::Cell;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetSystemMetrics, KillTimer, LoadImageW, PostQuitMessage, RegisterClassW,
    RegisterWindowMessageW, SetTimer, TranslateMessage, HICON, IMAGE_ICON, LR_DEFAULTCOLOR, MSG,
    SM_CXSMICON, SM_CYSMICON, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WM_TIMER,
    WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// Notice to show once the tray icon exists (it may not, right after logon).
#[derive(Clone, Copy)]
enum Notice {
    None,
    Fallback,
    NoHotkey,
}

const TRAY_RETRY_TIMER: usize = 1;
const TRAY_RETRY_MAX: u32 = 15;

thread_local! {
    static ICON: Cell<HICON> = const { Cell::new(HICON(std::ptr::null_mut())) };
    static COMBO: Cell<Option<&'static hotkey::Combo>> = const { Cell::new(None) };
    static NOTICE: Cell<Notice> = const { Cell::new(Notice::None) };
    static TRAY_SHOWN: Cell<bool> = const { Cell::new(false) };
    static TRAY_RETRIES: Cell<u32> = const { Cell::new(0) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

fn tooltip() -> String {
    match COMBO.with(|c| c.get()) {
        Some(combo) => format!("Tessera — {}", combo.label),
        None => "Tessera — no hotkey available".to_string(),
    }
}

/// Adds the tray icon; on success shows the pending notice, on failure retries later.
fn show_tray(hwnd: HWND) {
    if TRAY_SHOWN.with(|t| t.get()) {
        return;
    }
    if tray::add(hwnd, ICON.with(|i| i.get()), &tooltip()) {
        TRAY_SHOWN.with(|t| t.set(true));
        unsafe {
            let _ = KillTimer(Some(hwnd), TRAY_RETRY_TIMER);
        }
        tlog!("tray icon added");
        match NOTICE.with(|n| n.replace(Notice::None)) {
            Notice::None => {}
            Notice::Fallback => tray::notify(
                hwnd,
                &format!(
                    "{} is taken by another program. Use {} to open the grid.",
                    hotkey::PRIMARY.label,
                    hotkey::FALLBACK.label
                ),
                false,
            ),
            Notice::NoHotkey => tray::notify(
                hwnd,
                &format!(
                    "Neither {} nor {} is available, so the grid cannot be opened. \
                     You can exit Tessera from this icon.",
                    hotkey::PRIMARY.label,
                    hotkey::FALLBACK.label
                ),
                true,
            ),
        }
    } else {
        let tries = TRAY_RETRIES.with(|t| t.get()) + 1;
        TRAY_RETRIES.with(|t| t.set(tries));
        tlog!("tray icon not added (attempt {tries}), retrying");
        unsafe {
            if tries >= TRAY_RETRY_MAX {
                let _ = KillTimer(Some(hwnd), TRAY_RETRY_TIMER);
            } else {
                SetTimer(Some(hwnd), TRAY_RETRY_TIMER, 2000, None);
            }
        }
    }
}

fn on_tray_click(hwnd: HWND) {
    let enabled = autostart::is_enabled();
    match tray::show_menu(hwnd, enabled) {
        tray::CMD_AUTOSTART => autostart::set_enabled(!enabled),
        tray::CMD_EXIT => {
            tlog!("exit requested from the tray menu");
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
        }
        _ => {}
    }
}

extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY if wp.0 as i32 == hotkey::HOTKEY_ID => overlay::toggle(hwnd),
        tray::WM_TRAY => {
            let event = (lp.0 & 0xffff) as u32;
            if event == WM_LBUTTONUP || event == WM_RBUTTONUP {
                on_tray_click(hwnd);
            }
        }
        WM_TIMER if wp.0 == TRAY_RETRY_TIMER => show_tray(hwnd),
        WM_DESTROY => {
            hotkey::unregister(hwnd);
            tray::remove(hwnd);
            unsafe { PostQuitMessage(0) };
        }
        m if m != 0 && m == TASKBAR_CREATED.with(|t| t.get()) => {
            // Explorer restarted: the old icon is gone, add it again.
            tlog!("taskbar re-created");
            TRAY_SHOWN.with(|t| t.set(false));
            TRAY_RETRIES.with(|t| t.set(0));
            show_tray(hwnd);
        }
        _ => return unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
    LRESULT(0)
}

fn load_icon(instance: HINSTANCE) -> HICON {
    unsafe {
        let (cx, cy) = (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON));
        match LoadImageW(
            Some(instance),
            PCWSTR(std::ptr::without_provenance(1)), // MAKEINTRESOURCE(1), see assets/tessera.rc
            IMAGE_ICON,
            cx,
            cy,
            LR_DEFAULTCOLOR,
        ) {
            Ok(h) => HICON(h.0),
            Err(e) => {
                tlog!("cannot load the icon resource: {e}");
                HICON(std::ptr::null_mut())
            }
        }
    }
}

fn run() -> windows::core::Result<()> {
    unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None)?.into();

        let class = w!("Tessera.Main");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(main_proc),
            hInstance: instance,
            lpszClassName: class,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err(windows::core::Error::from_thread());
        }
        // Hidden top-level window (not message-only, so it receives TaskbarCreated).
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            w!("Tessera"),
            WS_POPUP,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )?;
        TASKBAR_CREATED.with(|t| t.set(RegisterWindowMessageW(w!("TaskbarCreated"))));

        overlay::create(instance)?;
        autostart::startup_check();

        let combo = hotkey::register(hwnd);
        COMBO.with(|c| c.set(combo));
        let notice = match combo {
            Some(c) if std::ptr::eq(c, &hotkey::PRIMARY) => Notice::None,
            Some(_) => Notice::Fallback,
            None => Notice::NoHotkey,
        };
        NOTICE.with(|n| n.set(notice));
        ICON.with(|i| i.set(load_icon(instance)));
        show_tray(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

fn main() {
    // Single instance first, so a second launch cannot truncate the running instance's log.
    let _mutex = unsafe { CreateMutexW(None, true, w!("Local\\Tessera.SingleInstance")) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return;
    }

    log::init();
    tlog!("Tessera {} starting", env!("CARGO_PKG_VERSION"));
    if let Some(path) = log::path() {
        tlog!("log file: {}", path.display());
    }
    let dpi = unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    tlog!("per-monitor v2 DPI awareness: {}", if dpi.is_ok() { "on" } else { "failed" });
    tlog!("grid {}x{}", placement::COLS, placement::ROWS);

    if let Err(e) = run() {
        tlog!("fatal: {e}");
    }
    tlog!("Tessera stopped");
}
