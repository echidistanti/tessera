//! Small helpers shared by the other modules.

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::System::Diagnostics::Debug::MessageBeep;
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindowTextW, MB_OK,
};

/// Copies `s` into a fixed-size, NUL-terminated UTF-16 buffer, truncating if needed.
pub fn copy_wide(dst: &mut [u16], s: &str) {
    if dst.is_empty() {
        return;
    }
    let max = dst.len() - 1;
    let mut n = 0;
    for unit in s.encode_utf16().take(max) {
        dst[n] = unit;
        n += 1;
    }
    dst[n] = 0;
}

pub fn rect_str(r: &RECT) -> String {
    format!(
        "({}, {})-({}, {}) {}x{}",
        r.left,
        r.top,
        r.right,
        r.bottom,
        r.right - r.left,
        r.bottom - r.top
    )
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

pub fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// The short "no" sound used whenever Tessera refuses to act.
pub fn beep() {
    unsafe {
        let _ = MessageBeep(MB_OK);
    }
}
