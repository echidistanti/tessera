//! Tessera: press Win+Shift+G (or the shortcut chosen in the options panel), pick a rectangle on
//! a 6x6 grid, and the focused window snaps to the matching part of its monitor's work area.

#![windows_subsystem = "windows"]

#[macro_use]
mod log;
mod autostart;
mod combo;
mod grid;
mod hotkey;
mod options;
mod overlay;
mod placement;
mod settings;
mod tray;
mod util;

use std::cell::Cell;

use combo::Combo;
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
    /// The wanted shortcut was refused; `using` works instead.
    Fallback { wanted: Combo, using: Combo },
    /// No shortcut could be registered at all.
    NoHotkey { wanted: Combo },
}

const TRAY_RETRY_TIMER: usize = 1;
const TRAY_RETRY_MAX: u32 = 15;

thread_local! {
    static ICON: Cell<HICON> = const { Cell::new(HICON(std::ptr::null_mut())) };
    /// The shortcut currently registered with Windows, if any.
    static COMBO: Cell<Option<Combo>> = const { Cell::new(None) };
    static NOTICE: Cell<Notice> = const { Cell::new(Notice::None) };
    static TRAY_SHOWN: Cell<bool> = const { Cell::new(false) };
    static TRAY_RETRIES: Cell<u32> = const { Cell::new(0) };
    static TASKBAR_CREATED: Cell<u32> = const { Cell::new(0) };
}

fn tooltip() -> String {
    match COMBO.with(|c| c.get()) {
        Some(combo) => format!("Tessera — {}", combo.label()),
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
            Notice::Fallback { wanted, using } => tray::notify(
                hwnd,
                &format!(
                    "{} is taken by another program. Use {} to open the grid. \
                     Right-click this icon to choose another shortcut.",
                    wanted.label(),
                    using.label()
                ),
                false,
            ),
            Notice::NoHotkey { wanted } => tray::notify(
                hwnd,
                &format!(
                    "Neither {} nor {} is available, so the grid cannot be opened. \
                     Right-click this icon to choose another shortcut.",
                    wanted.label(),
                    Combo::FALLBACK.label()
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

/// Left click: the small menu.
fn on_tray_menu(hwnd: HWND) {
    let enabled = autostart::is_enabled();
    match tray::show_menu(hwnd, enabled) {
        tray::CMD_OPTIONS => on_tray_options(hwnd),
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

/// Right click: the options panel.
fn on_tray_options(hwnd: HWND) {
    options::show(
        hwnd,
        COMBO.with(|c| c.get()),
        autostart::is_enabled(),
        placement::grid_size(),
    );
}

/// Applies what the options panel saved: the grid size, "Start with Windows" and the shortcut.
fn apply_settings(hwnd: HWND, wanted: Combo, start_with_windows: bool, grid_size: i32) {
    if autostart::is_enabled() != start_with_windows {
        autostart::set_enabled(start_with_windows);
    }
    if grid_size != placement::grid_size() && grid::is_valid(grid_size) {
        placement::set_grid_size(grid_size);
        settings::save_grid_size(grid_size);
        tlog!("grid size changed to {}", grid::label(grid_size));
    }
    change_hotkey(hwnd, wanted);
}

/// Replaces the registered shortcut, keeping the old one if Windows refuses the new one.
fn change_hotkey(hwnd: HWND, wanted: Combo) {
    let old = COMBO.with(|c| c.get());
    if old == Some(wanted) {
        settings::save_combo(wanted);
        return;
    }
    hotkey::unregister(hwnd);
    match hotkey::register(hwnd, wanted) {
        Ok(()) => {
            tlog!("hotkey changed to {}", wanted.label());
            COMBO.with(|c| c.set(Some(wanted)));
            settings::save_combo(wanted);
            tray::set_tip(hwnd, &tooltip());
            tray::notify(
                hwnd,
                &format!("Use {} to open the grid.", wanted.label()),
                false,
            );
        }
        Err(e) => {
            tlog!("hotkey {} rejected by Windows: {}", wanted.label(), e);
            if let Some(previous) = old {
                if hotkey::register(hwnd, previous).is_err() {
                    tlog!("could not restore {}", previous.label());
                    COMBO.with(|c| c.set(None));
                    tray::set_tip(hwnd, &tooltip());
                }
            }
            tray::notify(
                hwnd,
                &format!("{} was taken in the meantime. The shortcut is unchanged.", wanted.label()),
                true,
            );
        }
    }
}

extern "system" fn main_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY if wp.0 as i32 == hotkey::HOTKEY_ID => overlay::toggle(hwnd),
        tray::WM_TRAY => {
            match (lp.0 & 0xffff) as u32 {
                WM_LBUTTONUP => on_tray_menu(hwnd),
                WM_RBUTTONUP => on_tray_options(hwnd),
                _ => {}
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

        // The saved shortcut comes first, then the default, then the fallback.
        let wanted = settings::load_combo().unwrap_or(Combo::DEFAULT);
        let combo = hotkey::register_first(hwnd, &[wanted, Combo::DEFAULT, Combo::FALLBACK]);
        COMBO.with(|c| c.set(combo));
        let notice = match combo {
            Some(c) if c == wanted => Notice::None,
            Some(using) => Notice::Fallback { wanted, using },
            None => Notice::NoHotkey { wanted },
        };
        NOTICE.with(|n| n.set(notice));
        ICON.with(|i| i.set(load_icon(instance)));
        show_tray(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            // The options panel handles Tab, Enter and Esc itself.
            if options::handle_message(&msg) {
                continue;
            }
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
    placement::set_grid_size(settings::load_grid_size());
    tlog!("grid {}", grid::label(placement::grid_size()));

    if let Err(e) = run() {
        tlog!("fatal: {e}");
    }
    tlog!("Tessera stopped");
}
