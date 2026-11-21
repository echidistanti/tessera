//! Global hotkey: Win+Shift+G, falling back to Win+Ctrl+Alt+G.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT,
    MOD_SHIFT, MOD_WIN,
};

pub const HOTKEY_ID: i32 = 1;

pub struct Combo {
    pub modifiers: HOT_KEY_MODIFIERS,
    pub key: u32,
    pub label: &'static str,
}

/// Preferred combination. Kept as constants so it can be exposed in a settings menu later.
pub const PRIMARY: Combo = Combo {
    modifiers: HOT_KEY_MODIFIERS(MOD_WIN.0 | MOD_SHIFT.0),
    key: b'G' as u32,
    label: "Win+Shift+G",
};

pub const FALLBACK: Combo = Combo {
    modifiers: HOT_KEY_MODIFIERS(MOD_WIN.0 | MOD_CONTROL.0 | MOD_ALT.0),
    key: b'G' as u32,
    label: "Win+Ctrl+Alt+G",
};

/// Tries the primary combination, then the fallback. Returns the one that worked.
pub fn register(hwnd: HWND) -> Option<&'static Combo> {
    for combo in [&PRIMARY, &FALLBACK] {
        let modifiers = HOT_KEY_MODIFIERS(combo.modifiers.0 | MOD_NOREPEAT.0);
        match unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID, modifiers, combo.key) } {
            Ok(()) => {
                tlog!("hotkey registered: {}", combo.label);
                return Some(combo);
            }
            Err(e) => tlog!("hotkey {} rejected by Windows: {}", combo.label, e),
        }
    }
    None
}

pub fn unregister(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
    }
}
