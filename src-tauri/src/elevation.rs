//! Some folders (like the Windows temp folder) can only be cleaned as administrator.

use tauri::AppHandle;

#[tauri::command]
pub fn is_admin() -> bool {
    #[cfg(windows)]
    {
        // SAFETY: takes no arguments and only reads the current process token.
        unsafe { windows_sys::Win32::UI::Shell::IsUserAnAdmin() != 0 }
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// Starts a new elevated copy of ClearWin (Windows asks for permission) and closes this one.
#[tauri::command]
pub fn restart_as_admin(app: AppHandle) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        use windows_sys::Win32::UI::Shell::ShellExecuteW;
        use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let wide = |s: &std::ffi::OsStr| s.encode_wide().chain([0]).collect::<Vec<u16>>();
        let verb = wide("runas".as_ref());
        let file = wide(exe.as_os_str());
        // SAFETY: both strings are null-terminated UTF-16 and outlive the call.
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                verb.as_ptr(),
                file.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        // Values of 32 or less are errors, including the user declining the prompt.
        if result as isize <= 32 {
            return Err("Administrator rights were not granted.".into());
        }
        app.exit(0);
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = app;
        Err("Only available on Windows.".into())
    }
}
