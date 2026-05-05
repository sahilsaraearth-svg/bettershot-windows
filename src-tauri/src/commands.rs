//! Tauri commands module - Windows version

use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

use crate::clipboard::copy_image_to_clipboard;
use crate::image::{crop_image, render_image_with_effects, save_base64_image, CropRegion, RenderSettings};
use crate::screenshot::{
    capture_all_monitors as capture_monitors, capture_primary, MonitorShot,
};
use crate::utils::get_desktop_path;

static CAPTURE_LOCK: Mutex<()> = Mutex::new(());

#[tauri::command]
pub async fn move_window_to_active_space(_app_handle: AppHandle) -> Result<(), String> {
    // No-op on Windows - windows are automatically on the current virtual desktop
    Ok(())
}

#[tauri::command]
pub async fn copy_image_file_to_clipboard(path: String) -> Result<(), String> {
    copy_image_to_clipboard(&path).map_err(|e| e.to_string())
}

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

/// Render image with effects using Rust (optimized for blur)
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

/// Get the user's Desktop directory path (cross-platform)
#[tauri::command]
pub async fn get_desktop_directory() -> Result<String, String> {
    get_desktop_path()
}

/// Get the system temp directory path (cross-platform)
#[tauri::command]
pub async fn get_temp_directory() -> Result<String, String> {
    let temp_dir = std::env::temp_dir();
    let canonical = temp_dir.canonicalize().unwrap_or(temp_dir);
    canonical
        .to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Failed to convert temp directory path to string".to_string())
}

/// Capture fullscreen - captures all monitors
#[tauri::command]
pub async fn native_capture_fullscreen(save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    capture_primary(&save_dir)
}

/// Capture specific window - on Windows we capture the primary screen
/// (full interactive window selection is handled by the frontend overlay)
#[tauri::command]
pub async fn native_capture_window(save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    capture_primary(&save_dir)
}

/// Play screenshot sound
#[tauri::command]
pub async fn play_screenshot_sound() -> Result<(), String> {
    // On Windows, we can play a system sound
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("powershell")
            .args(["-Command", "[System.Media.SystemSounds]::Beep.Play()"])
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
    
    // Fallback
    Ok((0.0, 0.0))
}

/// Interactive region capture - starts the region selector overlay
/// The actual capture happens through the frontend's RegionSelector component
#[tauri::command]
pub async fn native_capture_interactive(app_handle: AppHandle, save_dir: String) -> Result<String, String> {
    let _lock = CAPTURE_LOCK
        .lock()
        .map_err(|e| format!("Failed to acquire lock: {}", e))?;

    // Capture all monitors to temp dir
    let temp_dir = std::env::temp_dir();
    let temp_dir_str = temp_dir.to_string_lossy().to_string();
    
    // Capture the screen
    let screenshot_path = capture_primary(&temp_dir_str)?;
    
    // Store the path so the overlay can use it
    // Emit event to the selector overlay
    app_handle
        .emit("screenshot-ready-for-selection", screenshot_path.clone())
        .map_err(|e| format!("Failed to emit event: {}", e))?;
    
    Ok(screenshot_path)
}

/// Called after user selects a region - crops and saves the screenshot
#[tauri::command]
pub async fn crop_and_save_region(
    screenshot_path: String,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    save_dir: String,
) -> Result<String, String> {
    if width == 0 || height == 0 {
        return Err("Invalid selection region".to_string());
    }

    let region = CropRegion {
        x: x.max(0) as u32,
        y: y.max(0) as u32,
        width,
        height,
    };

    crop_image(&screenshot_path, region, &save_dir)
}

/// Get all monitor screenshots as base64 for the region selector overlay
#[tauri::command]
pub async fn capture_screen_for_selector() -> Result<String, String> {
    let temp_dir = std::env::temp_dir();
    let temp_dir_str = temp_dir.to_string_lossy().to_string();
    
    let path = capture_primary(&temp_dir_str)?;
    
    // Read file and encode as base64
    let data = std::fs::read(&path)
        .map_err(|e| format!("Failed to read screenshot: {}", e))?;
    
    let _ = std::fs::remove_file(&path);
    
    use base64::{engine::general_purpose, Engine as _};
    let encoded = general_purpose::STANDARD.encode(&data);
    Ok(format!("data:image/png;base64,{}", encoded))
}
