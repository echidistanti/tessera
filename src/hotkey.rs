//! The global hotkey that opens the grid, and the test used to tell whether a combination is
//! free.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_NOREPEAT,
};

use crate::combo::{Availability, Combo};

pub const HOTKEY_ID: i32 = 1;
/// Used only to ask Windows whether a combination can be registered; released at once.
const PROBE_ID: i32 = 2;

fn modifiers(combo: Combo) -> HOT_KEY_MODIFIERS {
    // Combo uses the same bit values as the MOD_* flags.
    HOT_KEY_MODIFIERS(combo.modifiers | MOD_NOREPEAT.0)
}

/// Registers `combo` as the hotkey that opens the grid.
pub fn register(hwnd: HWND, combo: Combo) -> windows::core::Result<()> {
    unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID, modifiers(combo), combo.key) }
}

/// Registers the first combination of `candidates` that Windows accepts and returns it.
pub fn register_first(hwnd: HWND, candidates: &[Combo]) -> Option<Combo> {
    for &combo in candidates {
        match register(hwnd, combo) {
            Ok(()) => {
                tlog!("hotkey registered: {}", combo.label());
                return Some(combo);
            }
            Err(e) => tlog!("hotkey {} rejected by Windows: {}", combo.label(), e),
        }
    }
    None
}

pub fn unregister(hwnd: HWND) {
    unsafe {
        let _ = UnregisterHotKey(Some(hwnd), HOTKEY_ID);
    }
}

/// Asks Windows whether `combo` could be registered right now. The only way to know is to try:
/// the registration is released immediately, so nothing is left behind. `active` is the
/// combination Tessera itself holds, which would otherwise always look taken.
pub fn probe(hwnd: HWND, active: Option<Combo>, combo: Combo) -> Availability {
    if active == Some(combo) {
        return Availability::Active;
    }
    unsafe {
        match RegisterHotKey(Some(hwnd), PROBE_ID, modifiers(combo), combo.key) {
            Ok(()) => {
                let _ = UnregisterHotKey(Some(hwnd), PROBE_ID);
                Availability::Free
            }
            Err(_) => Availability::Taken,
        }
    }
}
