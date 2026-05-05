//! Utility functions for common operations

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub type AppResult<T> = Result<T, String>;

/// Resolve a Windows path to its long form and strip any \\?\ prefix.
/// On non-Windows this is a no-op.
/// This fixes:
///   - 8.3 short paths like C:\Users\SAHILC~1\... → C:\Users\sahilcodex\...
///   - \\?\ UNC prefix added by canonicalize()
pub fn resolve_path(path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        // First strip \\?\ if present
        let cleaned = if path.starts_with(r"\\?\") {
            &path[4..]
        } else if path.starts_with(r"//?/") {
            &path[4..]
        } else {
            path
        };

        // Convert to wide string for WinAPI
        let wide: Vec<u16> = OsStr::new(cleaned)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        // Call GetLongPathNameW to resolve 8.3 short path components
        let mut buf = vec![0u16; 32768];
        let len = unsafe {
            winapi::um::fileapi::GetLongPathNameW(
                wide.as_ptr(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };

        if len > 0 && len < buf.len() as u32 {
            let long = OsString::from_wide(&buf[..len as usize]);
            return long.to_string_lossy().into_owned();
        }

        // GetLongPathNameW failed (file may not exist yet) — return cleaned path
        cleaned.to_string()
    }

    #[cfg(not(target_os = "windows"))]
    {
        path.to_string()
    }
}

/// Get the user's Desktop directory path
pub fn get_desktop_path() -> AppResult<String> {
    let desktop = dirs::desktop_dir().ok_or("Failed to get Desktop directory")?;
    Ok(resolve_path(&desktop.to_string_lossy()))
}

/// Get current timestamp in milliseconds
pub fn get_timestamp() -> AppResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("Failed to get timestamp: {}", e))
        .map(|d| d.as_millis() as u64)
}

/// Ensure a directory exists, creating it if necessary
pub fn ensure_dir(path: &PathBuf) -> AppResult<()> {
    fs::create_dir_all(path).map_err(|e| format!("Failed to create directory: {}", e))
}

/// Generate a unique filename with a prefix and timestamp
pub fn generate_filename(prefix: &str, extension: &str) -> AppResult<String> {
    let timestamp = get_timestamp()?;
    Ok(format!("{}_{}.{}", prefix, timestamp, extension))
}

/// Generate a unique filename with prefix, id, and timestamp
pub fn generate_filename_with_id(prefix: &str, id: u32, extension: &str) -> AppResult<String> {
    let timestamp = get_timestamp()?;
    Ok(format!("{}_{}_{}.{}", prefix, id, timestamp, extension))
}

// Keep strip_unc_prefix as an alias for backwards compat within this crate
pub fn strip_unc_prefix(path: &str) -> String {
    resolve_path(path)
}
