//! Notification-area icon, balloon notifications and the two-item menu.

use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_INFO, NIIF_WARNING, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, PostMessageW, SetForegroundWindow,
    TrackPopupMenu, HICON, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED, TPM_NONOTIFY,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_NULL,
};

use crate::util::copy_wide;

pub const WM_TRAY: u32 = WM_APP + 1;
pub const CMD_AUTOSTART: u32 = 100;
pub const CMD_EXIT: u32 = 101;
const TRAY_ID: u32 = 1;

fn base(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    }
}

pub fn add(hwnd: HWND, icon: HICON, tip: &str) -> bool {
    let mut nid = base(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid.hIcon = icon;
    copy_wide(&mut nid.szTip, tip);
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid) }.as_bool()
}

pub fn remove(hwnd: HWND) {
    let nid = base(hwnd);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

pub fn notify(hwnd: HWND, text: &str, warning: bool) {
    let mut nid = base(hwnd);
    nid.uFlags = NIF_INFO;
    nid.dwInfoFlags = if warning { NIIF_WARNING } else { NIIF_INFO };
    copy_wide(&mut nid.szInfoTitle, "Tessera");
    copy_wide(&mut nid.szInfo, text);
    let ok = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) }.as_bool();
    tlog!("notification shown ({ok}): {text}");
}

/// Shows the menu at the cursor and returns the chosen command (0 if dismissed).
pub fn show_menu(hwnd: HWND, autostart: bool) -> u32 {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return 0 };
        let check = if autostart { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check, CMD_AUTOSTART as usize, w!("Start with Windows"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, CMD_EXIT as usize, w!("Exit"));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            pt.x,
            pt.y,
            None,
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        cmd.0.max(0) as u32
    }
}
