//! Tauri commands module

use std::sync::Mutex;
use tauri::{AppHandle, Manager};

use crate::clipboard::copy_image_to_clipboard;
use crate::image::{crop_image, render_image_with_effects, save_base64_image, CropRegion, RenderSettings};
use crate::screenshot::{
    capture_all_monitors as capture_monitors, capture_primary, MonitorShot,
};
use crate::utils::{generate_filename, get_desktop_path, resolve_path};

static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

// Stores the full-screen base64 for the region selector flow.
// Written by native_capture_interactive.
// Read (cloned) by capture_screen_for_selector for display.
// Consumed by crop_and_save_region for cropping.
static PENDING_SCREENSHOT_B64: Mutex<Option<String>> = Mutex::new(None);

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Returns temp dir as long path string
fn temp_dir() -> String {
    resolve_path(&std::env::temp_dir().to_string_lossy())
}

/// Read a file and encode as data URI. The canonical way to pass images to the
/// webview — no asset protocol scope issues, no path format issues.
fn file_to_data_uri(path: &str) -> Result<String, String> {
    use base64::{engine::general_purpose, Engine as _};
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Failed to read '{}': {}", path, e))?;
    let mime = if bytes.starts_with(&[0x89, 0x50, 0x4E, 0x47]) { "image/png" }
               else if bytes.starts_with(&[0xFF, 0xD8]) { "image/jpeg" }
               else { "image/png" };
    Ok(format!("data:{};base64,{}", mime, general_purpose::STANDARD.encode(&bytes)))
}

// ─── Window ───────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn move_window_to_active_space(_app_handle: AppHandle) -> Result<(), String> {
    Ok(())
}

// ─── Clipboard ────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn copy_image_file_to_clipboard(path: String) -> Result<(), String> {
    copy_image_to_clipboard(&path).map_err(|e| e.to_string())
}

// ─── Basic captures ───────────────────────────────────────────────────────────

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

// ─── Render / save ────────────────────────────────────────────────────────────

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
    let saved_path = save_base64_image(&image_data, &save_dir, "clipshot")?;
    if copy_to_clip { copy_image_to_clipboard(&saved_path)?; }
    Ok(saved_path)
}

// ─── Directory helpers ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn get_desktop_directory() -> Result<String, String> {
    get_desktop_path()
}

#[tauri::command]
pub async fn get_temp_directory() -> Result<String, String> {
    Ok(temp_dir())
}

// ─── Native fullscreen / window capture ───────────────────────────────────────
//
// Returns a data URI (data:image/png;base64,...) so the editor can load it
// directly without going through Tauri's asset protocol.
// The file is ALSO saved to save_dir so the user has a copy on disk.

#[tauri::command]
pub async fn native_capture_fullscreen(save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock: {}", e))?;

    // Save file to disk
    let path = capture_primary(&save_dir)?;

    // Return as data URI — editor reads this, no asset protocol needed
    let data_uri = file_to_data_uri(&path)?;
    Ok(data_uri)
}

#[tauri::command]
pub async fn native_capture_window(save_dir: String) -> Result<String, String> {
    // Same as fullscreen on Windows for now
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock: {}", e))?;
    let path = capture_primary(&save_dir)?;
    let data_uri = file_to_data_uri(&path)?;
    Ok(data_uri)
}

// ─── Region selector flow ─────────────────────────────────────────────────────
//
// 1. App.tsx calls native_capture_interactive
//    → captures screen → stores base64 in PENDING_SCREENSHOT_B64
//    → shows region-selector window
//
// 2. RegionSelector mounts → calls capture_screen_for_selector
//    → returns CLONE of PENDING_SCREENSHOT_B64 for display (does NOT clear it)
//
// 3. User draws region → App.tsx calls crop_and_save_region
//    → consumes PENDING_SCREENSHOT_B64, crops, saves to disk
//    → returns data URI so editor loads instantly

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

    // Capture to temp, encode to base64, delete temp file
    let tmp = temp_dir();
    let path = capture_primary(&tmp)?;
    let data_uri = file_to_data_uri(&path)?;
    let _ = std::fs::remove_file(&path);

    // Store for region-selector display + crop
    {
        let mut lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex: {}", e))?;
        *lock = Some(data_uri);
    }

    // Show region-selector window
    if let Some(sel) = app_handle.get_webview_window("region-selector") {
        let _ = sel.show();
        let _ = sel.set_focus();
    }

    Ok("ok".to_string())
}

/// Returns a CLONE of the stored screenshot for the selector overlay to display.
/// Does NOT consume it — crop_and_save_region needs it too.
#[tauri::command]
pub async fn capture_screen_for_selector() -> Result<String, String> {
    {
        let lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex: {}", e))?;
        if let Some(ref data) = *lock {
            return Ok(data.clone());
        }
    }

    // Fallback: no stored screenshot, capture fresh
    let tmp = temp_dir();
    let path = capture_primary(&tmp)?;
    let data_uri = file_to_data_uri(&path)?;
    let _ = std::fs::remove_file(&path);

    {
        let mut lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex: {}", e))?;
        *lock = Some(data_uri.clone());
    }

    Ok(data_uri)
}

/// Crops the stored screenshot and saves to disk.
/// Returns a data URI for the editor to display directly.
#[tauri::command]
pub async fn crop_and_save_region(
    x: i32, y: i32,
    width: u32, height: u32,
    save_dir: String,
) -> Result<String, String> {
    if width == 0 || height == 0 {
        return Err("Invalid region: width/height must be > 0".to_string());
    }

    // Consume the stored screenshot
    let data_uri = {
        let mut lock = PENDING_SCREENSHOT_B64.lock()
            .map_err(|e| format!("Mutex: {}", e))?;
        lock.take()
            .ok_or("No pending screenshot — was native_capture_interactive called?")?
    };

    use base64::{engine::general_purpose, Engine as _};
    use crate::utils::ensure_dir;
    use std::path::PathBuf;

    // Decode base64 → image
    let raw = data_uri.splitn(2, ',').nth(1)
        .ok_or("Malformed base64 data URI")?;
    let bytes = general_purpose::STANDARD.decode(raw)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    let iw = img.width();
    let ih = img.height();

    // Clamp to image bounds
    let cx = (x.max(0) as u32).min(iw.saturating_sub(1));
    let cy = (y.max(0) as u32).min(ih.saturating_sub(1));
    let cw = width.min(iw.saturating_sub(cx));
    let ch = height.min(ih.saturating_sub(cy));

    if cw == 0 || ch == 0 {
        return Err(format!(
            "Region ({},{} {}×{}) is outside bounds ({}×{})",
            x, y, width, height, iw, ih
        ));
    }

    let cropped = img.crop_imm(cx, cy, cw, ch);

    // Save to disk
    let dest_dir = PathBuf::from(&save_dir);
    ensure_dir(&dest_dir)?;
    let fname = generate_filename("screenshot", "png")?;
    let out = dest_dir.join(&fname);
    cropped.save(&out)
        .map_err(|e| format!("Failed to save: {}", e))?;

    // Return as data URI — no file path, no asset protocol
    let cropped_bytes = std::fs::read(&out)
        .map_err(|e| format!("Failed to read saved crop: {}", e))?;
    Ok(format!(
        "data:image/png;base64,{}",
        general_purpose::STANDARD.encode(&cropped_bytes)
    ))
}

// ─── Misc ─────────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn play_screenshot_sound() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    unsafe {
        winapi::um::winuser::MessageBeep(0x00000000);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_mouse_position() -> Result<(i32, i32), String> {
    #[cfg(target_os = "windows")]
    {
        let mut point = winapi::shared::windef::POINT { x: 0, y: 0 };
        unsafe { winapi::um::winuser::GetCursorPos(&mut point); }
        return Ok((point.x, point.y));
    }
    #[cfg(not(target_os = "windows"))]
    Ok((0, 0))
}

/// Read a file and return it as a base64 data URI.
/// Exposed as a command so the frontend can load arbitrary image files
/// without going through Tauri's asset protocol.
#[tauri::command]
pub async fn read_file_as_base64(path: String) -> Result<String, String> {
    file_to_data_uri(&path)
}
