//! Tauri commands module - Windows version

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

use crate::clipboard::copy_image_to_clipboard;
use crate::image::{crop_image, render_image_with_effects, save_base64_image, CropRegion, RenderSettings};
use crate::screenshot::{
    capture_all_monitors as capture_monitors, capture_primary, MonitorShot,
};
use crate::utils::{generate_filename, get_desktop_path};

static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

// ─── Window management helpers ───────────────────────────────────────────────

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

/// Quick capture of primary monitor
#[tauri::command]
pub async fn capture_once(
    _app_handle: AppHandle,
    save_dir: String,
    copy_to_clip: bool,
) -> Result<String, String> {
    let screenshot_path = capture_primary(&save_dir)?;
    if copy_to_clip {
        copy_image_to_clipboard(&screenshot_path)?;
    }
    Ok(screenshot_path)
}

/// Capture all monitors with geometry info
#[tauri::command]
pub async fn capture_all_monitors(
    _app_handle: AppHandle,
    save_dir: String,
) -> Result<Vec<MonitorShot>, String> {
    capture_monitors(&save_dir)
}

/// Crop a region from a screenshot
#[tauri::command]
pub async fn capture_region(
    screenshot_path: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    save_dir: String,
) -> Result<String, String> {
    let region = CropRegion { x, y, width, height };
    crop_image(&screenshot_path, region, &save_dir)
}

/// Render image with effects using Rust (blur, padding, etc.)
#[tauri::command]
pub async fn render_image_with_effects_rust(
    image_path: String,
    settings: RenderSettings,
) -> Result<String, String> {
    render_image_with_effects(&image_path, settings)
}

/// Save an edited image from base64 data
#[tauri::command]
pub async fn save_edited_image(
    image_data: String,
    save_dir: String,
    copy_to_clip: bool,
) -> Result<String, String> {
    let saved_path = save_base64_image(&image_data, &save_dir, "bettershot")?;
    if copy_to_clip {
        copy_image_to_clipboard(&saved_path)?;
    }
    Ok(saved_path)
}

/// Get the user's Desktop directory path
#[tauri::command]
pub async fn get_desktop_directory() -> Result<String, String> {
    get_desktop_path()
}

/// Get the system temp directory path
#[tauri::command]
pub async fn get_temp_directory() -> Result<String, String> {
    let temp_dir = std::env::temp_dir();
    let canonical = temp_dir.canonicalize().unwrap_or(temp_dir);
    canonical
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Failed to convert temp directory path to string".to_string())
}

// ─── Native capture ───────────────────────────────────────────────────────────

/// Capture fullscreen using xcap
#[tauri::command]
pub async fn native_capture_fullscreen(_save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock error: {}", e))?;
    let temp_dir = std::env::temp_dir().to_string_lossy().to_string();
    capture_primary(&temp_dir)
}

/// Capture window — on Windows falls back to primary screen capture.
/// The user clicks the window they want in the interactive flow.
#[tauri::command]
pub async fn native_capture_window(_save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock error: {}", e))?;
    let temp_dir = std::env::temp_dir().to_string_lossy().to_string();
    capture_primary(&temp_dir)
}

/// Play screenshot sound on Windows via PowerShell
#[tauri::command]
pub async fn play_screenshot_sound() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("powershell")
            .args([
                "-WindowStyle", "Hidden",
                "-Command",
                "[System.Media.SystemSounds]::Asterisk.Play()",
            ])
            .spawn();
    }
    Ok(())
}

/// Get the current mouse cursor position
#[tauri::command]
pub async fn get_mouse_position() -> Result<(f64, f64), String> {
    #[cfg(target_os = "windows")]
    {
        use winapi::shared::windef::POINT;
        use winapi::um::winuser::GetCursorPos;
        let mut point = POINT { x: 0, y: 0 };
        let result = unsafe { GetCursorPos(&mut point) };
        if result != 0 {
            return Ok((point.x as f64, point.y as f64));
        }
    }
    Ok((0.0, 0.0))
}

// ─── Region selector flow ─────────────────────────────────────────────────────
//
// Flow:
//   1. Frontend calls `native_capture_interactive` → we hide main window,
//      capture screen to temp, show region-selector window, return screenshot path.
//   2. RegionSelector webview renders screenshot, user draws rect, emits
//      `region-selected` event with {x,y,w,h,screenshotPath}.
//   3. Frontend calls `crop_and_save_region` → we crop and return final path.

/// Step 1: capture screen as base64, show selector overlay, return base64 data
/// We use base64 to avoid all Windows path issues (short paths, backslashes, asset scope)
#[tauri::command]
pub async fn native_capture_interactive(app_handle: AppHandle, _save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK.lock().map_err(|e| format!("Lock error: {}", e))?;

    // Hide all windows so they don't appear in the capture
    if let Some(w) = app_handle.get_webview_window("main") {
        let _ = w.hide();
    }
    if let Some(w) = app_handle.get_webview_window("quick-overlay") {
        let _ = w.hide();
    }

    // Wait for windows to actually disappear
    std::thread::sleep(std::time::Duration::from_millis(250));

    // Capture screen to temp file
    let temp_dir = std::env::temp_dir();
    let temp_dir_str = temp_dir.to_string_lossy().to_string();
    let screenshot_path = capture_primary(&temp_dir_str)?;

    // Read file and encode as base64 — avoids ALL path issues on Windows
    let data = std::fs::read(&screenshot_path)
        .map_err(|e| format!("Failed to read screenshot: {}", e))?;
    let _ = std::fs::remove_file(&screenshot_path);

    use base64::{engine::general_purpose, Engine as _};
    let base64_data = format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&data));

    // Show region-selector window
    if let Some(sel) = app_handle.get_webview_window("region-selector") {
        let _ = sel.show();
        let _ = sel.set_focus();
    }

    // Send base64 screenshot directly to the selector
    app_handle
        .emit("screenshot-ready-for-selection", base64_data.clone())
        .map_err(|e| format!("Emit error: {}", e))?;

    // Return the base64 data — App.tsx stores this as pendingRegionCaptureRef
    Ok(base64_data)
}

/// Step 3: crop the selected region from base64 screenshot data
/// `screenshot_data` is either a file path OR a base64 data URI (data:image/png;base64,...)
#[tauri::command]
pub async fn crop_and_save_region(
    screenshot_data: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    save_dir: String,
) -> Result<String, String> {
    if width == 0 || height == 0 {
        return Err("Invalid selection region".to_string());
    }

    use base64::{engine::general_purpose, Engine as _};
    use crate::utils::{ensure_dir, generate_filename};
    use std::path::PathBuf;

    // Decode base64 data URI or read from file path
    let img = if screenshot_data.starts_with("data:image") {
        let b64 = screenshot_data
            .splitn(2, ',')
            .nth(1)
            .ok_or("Invalid base64 data URI")?;
        let bytes = general_purpose::STANDARD.decode(b64)
            .map_err(|e| format!("Base64 decode error: {}", e))?;
        image::load_from_memory(&bytes)
            .map_err(|e| format!("Failed to decode image: {}", e))?
    } else {
        image::open(&screenshot_data)
            .map_err(|e| format!("Failed to open screenshot: {}", e))?
    };

    let img_w = img.width();
    let img_h = img.height();

    let cx = (x.max(0) as u32).min(img_w.saturating_sub(1));
    let cy = (y.max(0) as u32).min(img_h.saturating_sub(1));
    let cw = width.min(img_w.saturating_sub(cx));
    let ch = height.min(img_h.saturating_sub(cy));

    if cw == 0 || ch == 0 {
        return Err(format!("Crop region out of bounds: {}x{} at ({},{}), image is {}x{}", width, height, x, y, img_w, img_h));
    }

    let cropped = img.crop_imm(cx, cy, cw, ch);

    let save_path = PathBuf::from(&save_dir);
    ensure_dir(&save_path)?;
    let filename = generate_filename("screenshot", "png")?;
    let out_path = save_path.join(&filename);
    cropped.save(&out_path)
        .map_err(|e| format!("Failed to save cropped screenshot: {}", e))?;

    Ok(out_path.to_string_lossy().into_owned())
}

/// Capture screen and return base64 PNG — used by RegionSelector to show
/// the frozen screenshot as background while user draws a rect
#[tauri::command]
pub async fn capture_screen_for_selector() -> Result<String, String> {
    let temp_dir = std::env::temp_dir().to_string_lossy().to_string();
    let path = capture_primary(&temp_dir)?;

    let data = std::fs::read(&path)
        .map_err(|e| format!("Failed to read screenshot: {}", e))?;
    let _ = std::fs::remove_file(&path);

    use base64::{engine::general_purpose, Engine as _};
    Ok(format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&data)))
}
