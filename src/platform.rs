//! Single instance and start-at-login, on Linux and Windows.

/// Held for the whole life of the app; a second launch finds it taken.
pub struct InstanceLock {
    #[cfg(unix)]
    _file: std::fs::File,
    #[cfg(windows)]
    _handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(unix)]
pub fn single_instance() -> Option<InstanceLock> {
    use std::os::fd::AsRawFd;
    let dir = dirs::runtime_dir().unwrap_or_else(std::env::temp_dir);
    let file = std::fs::File::create(dir.join(format!("{}.lock", crate::settings::APP))).ok()?;
    let ok = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
    ok.then_some(InstanceLock { _file: file })
}

#[cfg(windows)]
pub fn single_instance() -> Option<InstanceLock> {
    use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name: Vec<u16> = "Local\\local-rocket-stats"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 1, name.as_ptr()) };
    if handle.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        return None;
    }
    Some(InstanceLock { _handle: handle })
}

/// "Start at login": an XDG autostart entry on Linux, a HKCU Run value on Windows.
pub mod autostart {
    #[cfg(unix)]
    mod imp {
        use std::path::PathBuf;
        use std::{env, fs, io};

        fn entry_path() -> Option<PathBuf> {
            Some(
                dirs::config_dir()?
                    .join("autostart")
                    .join("local-rocket-stats.desktop"),
            )
        }

        pub fn is_enabled() -> bool {
            entry_path().is_some_and(|p| p.exists())
        }

        pub fn set_enabled(enabled: bool) -> io::Result<()> {
            let path = entry_path().ok_or_else(|| io::Error::other("no config directory"))?;
            if !enabled {
                return match fs::remove_file(&path) {
                    Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                    r => r,
                };
            }
            // Installed from a package: use the command name, which survives upgrades.
            let exe = env::current_exe()?;
            let cmd = if exe.starts_with("/usr") {
                "local-rocket-stats".into()
            } else {
                format!("\"{}\"", exe.display())
            };
            fs::create_dir_all(path.parent().expect("parent"))?;
            fs::write(
                &path,
                format!(
                    "[Desktop Entry]\nType=Application\nName=Local Rocket Stats\n\
                     Comment=Rocket League stats and MMR tracker\nExec={cmd} --no-browser\n\
                     Icon=local-rocket-stats\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
                ),
            )
        }
    }

    #[cfg(windows)]
    mod imp {
        use std::{env, io, ptr};
        use windows_sys::Win32::Foundation::ERROR_SUCCESS;
        use windows_sys::Win32::System::Registry::{
            HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_SZ, RegCloseKey,
            RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW,
        };

        const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
        const VALUE: &str = "LocalRocketStats";

        fn wide(s: &str) -> Vec<u16> {
            s.encode_utf16().chain(Some(0)).collect()
        }

        fn open(access: u32) -> io::Result<HKEY> {
            let mut key: HKEY = ptr::null_mut();
            let rc = unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    wide(RUN_KEY).as_ptr(),
                    0,
                    access,
                    &mut key,
                )
            };
            if rc != ERROR_SUCCESS {
                return Err(io::Error::from_raw_os_error(rc as i32));
            }
            Ok(key)
        }

        pub fn is_enabled() -> bool {
            let Ok(key) = open(KEY_QUERY_VALUE) else {
                return false;
            };
            let rc = unsafe {
                RegQueryValueExW(
                    key,
                    wide(VALUE).as_ptr(),
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            unsafe { RegCloseKey(key) };
            rc == ERROR_SUCCESS
        }

        pub fn set_enabled(enabled: bool) -> io::Result<()> {
            let key = open(KEY_SET_VALUE)?;
            let name = wide(VALUE);
            let rc = if enabled {
                let cmd = wide(&format!(
                    "\"{}\" --no-browser",
                    env::current_exe()?.display()
                ));
                unsafe {
                    RegSetValueExW(
                        key,
                        name.as_ptr(),
                        0,
                        REG_SZ,
                        cmd.as_ptr().cast(),
                        (cmd.len() * 2) as u32,
                    )
                }
            } else {
                unsafe { RegDeleteValueW(key, name.as_ptr()) }
            };
            unsafe { RegCloseKey(key) };
            if rc == ERROR_SUCCESS || !enabled {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(rc as i32))
            }
        }
    }

    pub use imp::{is_enabled, set_enabled};
}

/// On Windows the app has no console: attach to the terminal it was started from, if any,
/// so command-line output is visible.
#[cfg(windows)]
pub fn attach_console() {
    use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

#[cfg(not(windows))]
pub fn attach_console() {}
