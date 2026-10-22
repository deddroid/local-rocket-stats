//! Messages go to stderr and to a log file (the tray app has no terminal).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

static FILE: OnceLock<Mutex<File>> = OnceLock::new();

pub fn init() {
    let path = crate::settings::log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    // Keep the file small: start over when it passes 2 MB.
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 2 << 20) {
        let _ = std::fs::remove_file(&path);
    }
    if let Ok(f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = FILE.set(Mutex::new(f));
    }
}

pub fn write(msg: &str) {
    let line = format!("{} {msg}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"));
    eprintln!("{line}");
    if let Some(f) = FILE.get() {
        let mut f = f.lock().unwrap_or_else(|e| e.into_inner());
        let _ = writeln!(f, "{line}");
    }
}

#[macro_export]
macro_rules! logln {
    ($($arg:tt)*) => { $crate::logging::write(&format!($($arg)*)) };
}
