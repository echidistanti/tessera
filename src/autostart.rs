//! "Start with Windows" through the per-user Run key.

use std::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD, REG_SZ,
    RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};

const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("Tessera");
const APP_KEY: PCWSTR = w!("Software\\Tessera");
const FIRST_RUN_VALUE: PCWSTR = w!("Initialized");

fn command_line() -> Option<String> {
    std::env::current_exe()
        .ok()
        .map(|p| format!("\"{}\"", p.display()))
}

fn read_run_value() -> Option<String> {
    let mut buf = [0u16; 1024];
    let mut size = std::mem::size_of_val(&buf) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            RUN_VALUE,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

pub fn is_enabled() -> bool {
    read_run_value().is_some()
}

pub fn set_enabled(enable: bool) {
    let status = if enable {
        let Some(cmd) = command_line() else {
            tlog!("autostart: cannot resolve the executable path");
            return;
        };
        let data: Vec<u16> = cmd.encode_utf16().chain(Some(0)).collect();
        unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(data.as_ptr() as *const c_void),
                (data.len() * 2) as u32,
            )
        }
    } else {
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE) }
    };
    tlog!("autostart set to {enable}: status {}", status.0);
}

/// First launch enables autostart; later launches keep the registered path up to date.
pub fn startup_check() {
    let mut flag = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let initialized = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            APP_KEY,
            FIRST_RUN_VALUE,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut flag as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    } == ERROR_SUCCESS;

    if !initialized {
        tlog!("first launch: enabling autostart");
        set_enabled(true);
        let one = 1u32;
        unsafe {
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                APP_KEY,
                FIRST_RUN_VALUE,
                REG_DWORD.0,
                Some(&one as *const u32 as *const c_void),
                std::mem::size_of::<u32>() as u32,
            );
        }
    } else if let (Some(current), Some(wanted)) = (read_run_value(), command_line()) {
        if !current.eq_ignore_ascii_case(&wanted) {
            tlog!("autostart path changed ({current} -> {wanted}), updating");
            set_enabled(true);
        }
    }
    tlog!("autostart enabled: {}", is_enabled());
}
