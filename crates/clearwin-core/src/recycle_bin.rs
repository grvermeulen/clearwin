use std::io;

/// Items and bytes in the Recycle Bin across all drives.
#[cfg(windows)]
pub fn query() -> (u64, u64) {
    use windows_sys::Win32::UI::Shell::{SHQueryRecycleBinW, SHQUERYRBINFO};

    // SAFETY: SHQUERYRBINFO is plain data; cbSize is set before the call as the API requires.
    unsafe {
        let mut info: SHQUERYRBINFO = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHQUERYRBINFO>() as u32;
        if SHQueryRecycleBinW(std::ptr::null(), &mut info) < 0 {
            return (0, 0);
        }
        (info.i64NumItems.max(0) as u64, info.i64Size.max(0) as u64)
    }
}

#[cfg(windows)]
pub fn empty() -> io::Result<()> {
    use windows_sys::Win32::UI::Shell::{
        SHEmptyRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI, SHERB_NOSOUND,
    };

    // Windows reports an error when asked to empty a bin that is already empty.
    if query().0 == 0 {
        return Ok(());
    }
    // SAFETY: null window and null root path mean "no owner window, all drives".
    let hr = unsafe {
        SHEmptyRecycleBinW(
            std::ptr::null_mut(),
            std::ptr::null(),
            SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
        )
    };
    if hr < 0 {
        return Err(io::Error::other(format!(
            "Windows returned error 0x{hr:08X}"
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
pub fn query() -> (u64, u64) {
    (0, 0)
}

#[cfg(not(windows))]
pub fn empty() -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "the Recycle Bin only exists on Windows",
    ))
}
