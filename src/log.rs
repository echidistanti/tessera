//! Minimal file logger: `%LOCALAPPDATA%\Tessera\tessera.log`, truncated at every start.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::System::SystemInformation::GetLocalTime;

static LOG: Mutex<Option<File>> = Mutex::new(None);

macro_rules! tlog {
    ($($arg:tt)*) => { $crate::log::write(format_args!($($arg)*)) };
}

pub fn path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|p| PathBuf::from(p).join("Tessera").join("tessera.log"))
}

pub fn init() {
    let Some(path) = path() else { return };
    if let Some(dir) = path.parent() {
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
    }
    if let (Ok(file), Ok(mut slot)) = (File::create(&path), LOG.lock()) {
        *slot = Some(file);
    }
}

pub fn write(args: std::fmt::Arguments) {
    let t = unsafe { GetLocalTime() };
    if let Ok(mut slot) = LOG.lock() {
        if let Some(file) = slot.as_mut() {
            let _ = writeln!(
                file,
                "{:02}:{:02}:{:02}.{:03} {}",
                t.wHour, t.wMinute, t.wSecond, t.wMilliseconds, args
            );
            let _ = file.flush();
        }
    }
}
