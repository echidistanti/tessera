//! Everything about the target window: validation, geometry and the actual move.

use std::ffi::c_void;
use std::sync::atomic::{AtomicI32, Ordering};

use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND, RECT};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetWindowRect, GetWindowThreadProcessId, IsHungAppWindow, IsIconic,
    IsWindow, IsWindowVisible, IsZoomed, SetWindowPos, ShowWindow, GA_ROOT, SWP_NOACTIVATE,
    SWP_NOOWNERZORDER, SWP_NOZORDER, SW_RESTORE,
};

use crate::grid;
use crate::util::{beep, class_name, rect_str, window_title};

/// Grid size: set from the saved settings at startup and from the options panel.
static GRID_SIZE: AtomicI32 = AtomicI32::new(grid::DEFAULT);

/// Columns of the grid. The grid is square, but columns and rows are kept apart in the maths.
pub fn cols() -> i32 {
    GRID_SIZE.load(Ordering::Relaxed)
}

pub fn rows() -> i32 {
    GRID_SIZE.load(Ordering::Relaxed)
}

pub fn grid_size() -> i32 {
    GRID_SIZE.load(Ordering::Relaxed)
}

/// Changes the grid size; ignored unless it is one of the presets.
pub fn set_grid_size(size: i32) {
    if grid::is_valid(size) {
        GRID_SIZE.store(size, Ordering::Relaxed);
    }
}

/// Shell surfaces that must never be moved (desktop, taskbars, Start, task view…).
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "MultitaskingViewFrame",
    "ForegroundStaging",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
];

#[derive(Clone, Copy)]
pub struct Target {
    pub hwnd: HWND,
    /// Work area (monitor minus taskbar) of the monitor holding the window.
    pub work: RECT,
    /// Visible bounds of the window (without the invisible resize borders).
    pub frame: RECT,
    pub dpi: u32,
}

/// Window rect as Windows reports it, and the visible frame drawn by DWM.
fn rects(hwnd: HWND) -> (RECT, RECT) {
    let mut window = RECT::default();
    let mut frame = RECT::default();
    unsafe {
        let _ = GetWindowRect(hwnd, &mut window);
        let ok = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut frame as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_ok();
        if !ok {
            frame = window;
        }
    }
    (window, frame)
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(process, TOKEN_QUERY, &mut token).ok()?;
        let mut elevation = TOKEN_ELEVATION::default();
        let mut len = 0u32;
        let res = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut TOKEN_ELEVATION as *mut c_void),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut len,
        );
        let _ = CloseHandle(token);
        res.ok()?;
        Some(elevation.TokenIsElevated != 0)
    }
}

/// True when the window belongs to an elevated process (unknown counts as not elevated).
fn window_elevated(hwnd: HWND) -> bool {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == 0 {
            return false;
        }
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return false;
        };
        let elevated = token_elevated(process).unwrap_or(false);
        let _ = CloseHandle(process);
        elevated
    }
}

fn self_elevated() -> bool {
    token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false)
}

/// Checks that `hwnd` (the foreground window) can be moved and gathers its geometry.
pub fn inspect(hwnd: HWND, own: &[HWND]) -> Result<Target, String> {
    if hwnd.is_invalid() {
        return Err("no foreground window".into());
    }
    let root = unsafe { GetAncestor(hwnd, GA_ROOT) };
    let hwnd = if root.is_invalid() { hwnd } else { root };
    if own.contains(&hwnd) {
        return Err("the foreground window is Tessera itself".into());
    }

    let class = class_name(hwnd);
    tlog!(
        "target: hwnd={:?} class=\"{}\" title=\"{}\"",
        hwnd.0,
        class,
        window_title(hwnd)
    );
    if SHELL_CLASSES.contains(&class.as_str()) {
        return Err(format!("shell window ({class})"));
    }
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() {
            return Err("window is not visible".into());
        }
        if IsHungAppWindow(hwnd).as_bool() {
            return Err("window is not responding".into());
        }
    }
    if window_elevated(hwnd) && !self_elevated() {
        return Err("window belongs to an elevated (administrator) process".into());
    }

    let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err("cannot read monitor information".into());
    }
    let (mut dpi_x, mut dpi_y) = (96u32, 96u32);
    unsafe {
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
    }

    let (window, frame) = rects(hwnd);
    tlog!(
        "target: window {} | frame {} | work area {} | dpi {}",
        rect_str(&window),
        rect_str(&frame),
        rect_str(&info.rcWork),
        dpi_x
    );
    Ok(Target {
        hwnd,
        work: info.rcWork,
        frame,
        dpi: dpi_x,
    })
}

/// Position of grid line `i` (0..=n) along a span starting at `start` of length `len`.
/// The same formula is used for every edge, so neighbouring cells always meet exactly.
pub fn grid_line(start: i32, len: i32, i: i32, n: i32) -> i32 {
    start + ((len as f64) * (i as f64) / (n as f64)).round() as i32
}

/// Cell containing a desktop point, clamped to the grid.
pub fn cell_at(work: &RECT, x: i32, y: i32) -> (i32, i32) {
    let w = (work.right - work.left).max(1);
    let h = (work.bottom - work.top).max(1);
    let c = ((x - work.left) as i64 * cols() as i64).div_euclid(w as i64) as i32;
    let r = ((y - work.top) as i64 * rows() as i64).div_euclid(h as i64) as i32;
    (c.clamp(0, cols() - 1), r.clamp(0, rows() - 1))
}

/// Moves and resizes the window so that its visible frame covers cells (c0,r0)..=(c1,r1).
pub fn apply(hwnd: HWND, work: RECT, c0: i32, r0: i32, c1: i32, r1: i32) {
    let ww = work.right - work.left;
    let wh = work.bottom - work.top;
    let want = RECT {
        left: grid_line(work.left, ww, c0, cols()),
        top: grid_line(work.top, wh, r0, rows()),
        right: grid_line(work.left, ww, c1 + 1, cols()),
        bottom: grid_line(work.top, wh, r1 + 1, rows()),
    };
    tlog!(
        "apply: cells ({c0},{r0})-({c1},{r1}) -> visible rect {}",
        rect_str(&want)
    );

    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            tlog!("apply: target window no longer exists");
            beep();
            return;
        }
        if IsZoomed(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            tlog!("apply: restoring maximized/minimized window first");
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
    }

    // Two passes: if the first move changes the window's DPI or frame, the second one
    // recomputes the invisible borders and corrects the result.
    for pass in 1..=2 {
        let (window, frame) = rects(hwnd);
        let border = |d: i32| d.clamp(0, 64);
        let left = border(frame.left - window.left);
        let top = border(frame.top - window.top);
        let right = border(window.right - frame.right);
        let bottom = border(window.bottom - frame.bottom);
        let x = want.left - left;
        let y = want.top - top;
        let w = (want.right - want.left) + left + right;
        let h = (want.bottom - want.top) + top + bottom;
        tlog!(
            "apply pass {pass}: invisible borders l{left} t{top} r{right} b{bottom}; SetWindowPos({x}, {y}, {w}, {h})"
        );

        let result = unsafe {
            SetWindowPos(
                hwnd,
                None,
                x,
                y,
                w,
                h,
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER,
            )
        };
        if let Err(e) = result {
            tlog!("apply: SetWindowPos failed: {e}");
            beep();
            return;
        }

        let (_, after) = rects(hwnd);
        tlog!("apply pass {pass}: visible frame now {}", rect_str(&after));
        if after == want {
            break;
        }
    }
}
