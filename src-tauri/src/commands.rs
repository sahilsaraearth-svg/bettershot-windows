//! Tauri commands module - Windows version

use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::clipboard::copy_image_to_clipboard;
use crate::image::{crop_image, render_image_with_effects, save_base64_image, CropRegion, RenderSettings};
use crate::screenshot::{
    capture_all_monitors as capture_monitors, capture_primary, MonitorShot,
};
use crate::utils::{generate_filename, get_desktop_path, resolve_path};

static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

// Stores the captured screenshot base64 for the region selector flow.
// Written by native_capture_interactive, read by BOTH capture_screen_for_selector
// (for display) AND crop_and_save_region (for cropping).
// We keep a clone so both can use it without fighting over it.
static PENDING_SCREENSHOT_B64: Mutex<Option<String>> = Mutex::new(None);

// ─── Window management ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn move_window_to_active_space(_app_handle: AppHandle) -> Result<(), String> {
    Ok(())
}

// ─── Clipboard ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn copy_image_file_to_clipboard(path: String) -> Result<(), String> {
    copy_image_to_clipboard(&path).map_err(|e| e.to_string())
}

// ─── Basic captures ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn capture_once(
    _app_handle: AppHandle,
    save_dir: String,
    copy_to_clip: bool,
) -> Result<String, String> {
    let path = capture_primary(&save_dir)?;
    if copy_to_clip { copy_image_to_clipboard(&path)?; }
    Ok(path)
}

#[tauri::command]
pub async fn capture_all_monitors(
    _app_handle: AppHandle,
    save_dir: String,
) -> Result<Vec<MonitorShot>, String> {
    capture_monitors(&save_dir)
}

#[tauri::command]
pub async fn capture_region(
    screenshot_path: String,
    x: u32, y: u32, width: u32, height: u32,
    save_dir: String,
) -> Result<String, String> {
    crop_image(&screenshot_path, CropRegion { x, y, width, height }, &save_dir)
}

#[tauri::command]
pub async fn render_image_with_effects_rust(
    image_path: String,
    settings: RenderSettings,
) -> Result<String, String> {
    render_image_with_effects(&image_path, settings)
}

#[tauri::command]
pub async fn save_edited_image(
    image_data: String,
    save_dir: String,
    copy_to_clip: bool,
) -> Result<String, String> {
    let saved_path = save_base64_image(&image_data, &save_dir, "bettershot")?;
    if copy_to_clip { copy_image_to_clipboard(&saved_path)?; }
    Ok(saved_path)
}

#[tauri::command]
pub async fn get_desktop_directory() -> Result<String, String> {
    // strip_unc_prefix handles Windows \\?\ prefix from dirs crate
    get_desktop_path()
}

#[tauri::command]
pub async fn get_temp_directory() -> Result<String, String> {
    // Use resolve_path to expand any 8.3 short path components (e.g. SAHILC~1)
    // Do NOT use .canonicalize() — on Windows it adds \\?\ prefix
    let p = std::env::temp_dir();
    p.to_str()
        .map(|s| resolve_path(s))
        .ok_or_else(|| "Failed to get temp directory".to_string())
}

/// Get the long-form temp dir path (resolves 8.3 short paths via GetLongPathNameW)
fn get_long_temp_dir() -> String {
    resolve_path(&std::env::temp_dir().to_string_lossy())
}

// ─── Native captures ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn native_capture_fullscreen(save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock: {}", e))?;
    // Capture to temp first, then copy to save_dir so caller gets a persistent path
    let temp = get_long_temp_dir();
    let tmp_path = capture_primary(&temp)?;
    // Copy to final save_dir
    let dest = copy_to_save_dir(&tmp_path, &save_dir)?;
    let _ = std::fs::remove_file(&tmp_path);
    Ok(dest)
}

#[tauri::command]
pub async fn native_capture_window(save_dir: String) -> Result<String, String> {
    // Same as fullscreen on Windows — no interactive window picker yet
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock: {}", e))?;
    let temp = get_long_temp_dir();
    let tmp_path = capture_primary(&temp)?;
    let dest = copy_to_save_dir(&tmp_path, &save_dir)?;
    let _ = std::fs::remove_file(&tmp_path);
    Ok(dest)
}

/// Copy a captured temp file into the final save_dir with a clean filename
fn copy_to_save_dir(src: &str, save_dir: &str) -> Result<String, String> {
    use std::path::PathBuf;
    use crate::utils::ensure_dir;

    let dest_dir = PathBuf::from(save_dir);
    ensure_dir(&dest_dir)?;
    let filename = generate_filename("screenshot", "png")?;
    let dest = dest_dir.join(&filename);
    std::fs::copy(src, &dest)
        .map_err(|e| format!("Failed to copy screenshot: {}", e))?;
    Ok(resolve_path(&dest.to_string_lossy()))
}

#[tauri::command]
pub async fn play_screenshot_sound() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("powershell")
            .args(["-WindowStyle", "Hidden", "-Command",
                   "[System.Media.SystemSounds]::Asterisk.Play()"])
            .spawn();
    }
    Ok(())
}

#[tauri::command]
pub async fn get_mouse_position() -> Result<(f64, f64), String> {
    #[cfg(target_os = "windows")]
    {
        use winapi::shared::windef::POINT;
        use winapi::um::winuser::GetCursorPos;
        let mut pt = POINT { x: 0, y: 0 };
        if unsafe { GetCursorPos(&mut pt) } != 0 {
            return Ok((pt.x as f64, pt.y as f64));
        }
    }
    Ok((0.0, 0.0))
}

// ─── Region selector flow ────────────────────────────────────────────────────
//
// 1. App.tsx calls native_capture_interactive
//    → hides all windows, captures screen as base64, stores in PENDING_SCREENSHOT_B64,
//      shows region-selector window, returns "ok"
//
// 2. RegionSelector mounts, calls capture_screen_for_selector
//    → returns a CLONE of PENDING_SCREENSHOT_B64 (does NOT consume it)
//    → displays screenshot, user draws rect
//
// 3. User confirms selection → RegionSelector hides itself,
//    emits region-selected to main window
//
// 4. App.tsx calls crop_and_save_region
//    → reads PENDING_SCREENSHOT_B64 (now consumed/cleared), crops, saves

#[tauri::command]
pub async fn native_capture_interactive(
    app_handle: AppHandle,
    _save_dir: String,
) -> Result<String, String> {
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock: {}", e))?;

    // Hide all app windows before capture
    for label in &["main", "quick-overlay"] {
        if let Some(w) = app_handle.get_webview_window(label) {
            let _ = w.hide();
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(250));

    // Capture screen → base64 (avoids ALL Windows path issues)
    let temp = get_long_temp_dir();
    let path = capture_primary(&temp)?;
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read screenshot file: {}", e))?;
    let _ = std::fs::remove_file(&path);

    use base64::{engine::general_purpose, Engine as _};
    let b64 = format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&bytes));

    // Store for both display (capture_screen_for_selector) and crop (crop_and_save_region)
    {
        let mut lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex error: {}", e))?;
        *lock = Some(b64);
    }

    // Show region-selector window
    if let Some(sel) = app_handle.get_webview_window("region-selector") {
        let _ = sel.show();
        let _ = sel.set_focus();
    }

    Ok("ok".to_string())
}

/// Called by RegionSelector on mount to get the frozen screenshot.
/// Returns a CLONE — does NOT consume the static, so crop_and_save_region can still use it.
#[tauri::command]
pub async fn capture_screen_for_selector() -> Result<String, String> {
    // Try to return stored screenshot first (set by native_capture_interactive)
    {
        let lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex error: {}", e))?;
        if let Some(ref data) = *lock {
            return Ok(data.clone()); // CLONE — don't take() — crop still needs it
        }
    }

    // Fallback: capture a fresh screenshot (selector opened without native_capture_interactive)
    let temp = get_long_temp_dir();
    let path = capture_primary(&temp)?;
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read screenshot: {}", e))?;
    let _ = std::fs::remove_file(&path);

    use base64::{engine::general_purpose, Engine as _};
    let b64 = format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&bytes));

    // Store it so crop_and_save_region can use it
    {
        let mut lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex error: {}", e))?;
        *lock = Some(b64.clone());
    }

    Ok(b64)
}

/// Called after region-selected event. Reads and CLEARS PENDING_SCREENSHOT_B64, crops, saves.
#[tauri::command]
pub async fn crop_and_save_region(
    x: i32, y: i32,
    width: u32, height: u32,
    save_dir: String,
) -> Result<String, String> {
    if width == 0 || height == 0 {
        return Err("Invalid region: width and height must be > 0".to_string());
    }

    // Consume the stored screenshot
    let b64 = {
        let mut lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex error: {}", e))?;
        lock.take()
            .ok_or("No pending screenshot. Was native_capture_interactive called?")?
    };

    use base64::{engine::general_purpose, Engine as _};
    use crate::utils::ensure_dir;
    use std::path::PathBuf;

    let raw = b64.splitn(2, ',').nth(1)
        .ok_or("Malformed base64 data URI")?;
    let bytes = general_purpose::STANDARD.decode(raw)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to decode screenshot image: {}", e))?;

    let iw = img.width();
    let ih = img.height();

    // Clamp region to image bounds
    let cx = (x.max(0) as u32).min(iw.saturating_sub(1));
    let cy = (y.max(0) as u32).min(ih.saturating_sub(1));
    let cw = width.min(iw.saturating_sub(cx));
    let ch = height.min(ih.saturating_sub(cy));

    if cw == 0 || ch == 0 {
        return Err(format!(
            "Region ({},{} {}×{}) is outside image bounds ({}×{})",
            x, y, width, height, iw, ih
        ));
    }

    let cropped = img.crop_imm(cx, cy, cw, ch);

    let dest = PathBuf::from(&save_dir);
    ensure_dir(&dest)?;
    let fname = generate_filename("screenshot", "png")?;
    let out = dest.join(&fname);
    cropped.save(&out)
        .map_err(|e| format!("Failed to save cropped screenshot: {}", e))?;

    Ok(resolve_path(&out.to_string_lossy()))
}

/// Read any file and return it as a base64 data URI.
/// This bypasses Tauri's asset protocol scope checks entirely —
/// no short-path, no \\?\, no $TEMP scope issues.
#[tauri::command]
pub async fn read_file_as_base64(path: String) -> Result<String, String> {
    use base64::{engine::general_purpose, Engine as _};
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("Failed to read file '{}': {}", path, e))?;
    // Detect PNG vs JPEG by magic bytes
    let mime = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        "image/png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else {
        "image/png" // default
    };
    Ok(format!("data:{};base64,{}", mime, general_purpose::STANDARD.encode(&bytes)))
}
