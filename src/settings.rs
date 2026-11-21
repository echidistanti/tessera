//! Settings kept under `HKEY_CURRENT_USER\Software\Tessera`.

use std::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD,
};

use crate::combo::Combo;

pub const APP_KEY: PCWSTR = w!("Software\\Tessera");
const HOTKEY_MODIFIERS: PCWSTR = w!("HotkeyModifiers");
const HOTKEY_KEY: PCWSTR = w!("HotkeyKey");

pub fn get_dword(name: PCWSTR) -> Option<u32> {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            APP_KEY,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut value as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    (status == ERROR_SUCCESS).then_some(value)
}

pub fn set_dword(name: PCWSTR, value: u32) -> bool {
    let status = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            APP_KEY,
            name,
            REG_DWORD.0,
            Some(&value as *const u32 as *const c_void),
            std::mem::size_of::<u32>() as u32,
        )
    };
    status == ERROR_SUCCESS
}

/// The shortcut the user chose, if one is saved and still valid.
pub fn load_combo() -> Option<Combo> {
    let combo = Combo {
        modifiers: get_dword(HOTKEY_MODIFIERS)?,
        key: get_dword(HOTKEY_KEY)?,
    };
    if combo.is_valid() {
        Some(combo)
    } else {
        tlog!("ignoring an invalid saved shortcut ({:#x}, {:#x})", combo.modifiers, combo.key);
        None
    }
}

pub fn save_combo(combo: Combo) {
    let ok = set_dword(HOTKEY_MODIFIERS, combo.modifiers) && set_dword(HOTKEY_KEY, combo.key);
    tlog!("shortcut {} saved: {ok}", combo.label());
}
