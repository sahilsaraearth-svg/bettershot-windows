//! Screenshot capture module - using xcap for cross-platform support

use serde::Serialize;
use std::path::PathBuf;
use xcap::Monitor;

use crate::utils::{ensure_dir, generate_filename, generate_filename_with_id, AppResult};

/// Represents a captured monitor screenshot with geometry info
#[derive(Serialize, Clone, Debug)]
pub struct MonitorShot {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
    pub path: String,
}

/// Capture screenshots of all available monitors
pub fn capture_all_monitors(save_dir: &str) -> AppResult<Vec<MonitorShot>> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to get monitors: {}", e))?;

    if monitors.is_empty() {
        return Err("No monitors available".into());
    }

    let save_path = PathBuf::from(save_dir);
    ensure_dir(&save_path)?;

    let mut shots = Vec::with_capacity(monitors.len());

    for monitor in monitors {
        let shot = capture_single_monitor(&monitor, &save_path)?;
        shots.push(shot);
    }

    Ok(shots)
}

/// Capture a single monitor screenshot
fn capture_single_monitor(monitor: &Monitor, save_path: &PathBuf) -> AppResult<MonitorShot> {
    let monitor_id = monitor
        .id()
        .map_err(|e| format!("Failed to get monitor id: {}", e))?;

    let image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture monitor {}: {}", monitor_id, e))?;

    let filename = generate_filename_with_id("monitor", monitor_id, "png")?;
    let screenshot_path = save_path.join(&filename);

    image
        .save(&screenshot_path)
        .map_err(|e| format!("Failed to save screenshot: {}", e))?;

    let x = monitor
        .x()
        .map_err(|e| format!("Failed to get monitor x: {}", e))?;
    let y = monitor
        .y()
        .map_err(|e| format!("Failed to get monitor y: {}", e))?;
    let width = monitor
        .width()
        .map_err(|e| format!("Failed to get monitor width: {}", e))?;
    let height = monitor
        .height()
        .map_err(|e| format!("Failed to get monitor height: {}", e))?;
    let scale_factor = monitor
        .scale_factor()
        .map_err(|e| format!("Failed to get monitor scale factor: {}", e))?;

    Ok(MonitorShot {
        id: monitor_id,
        x,
        y,
        width,
        height,
        scale_factor,
        path: screenshot_path.to_string_lossy().into_owned(),
    })
}

/// Capture the primary monitor (first monitor)
pub fn capture_primary(save_dir: &str) -> AppResult<String> {
    let monitors = Monitor::all().map_err(|e| format!("Failed to get monitors: {}", e))?;
    
    let monitor = monitors.into_iter().next().ok_or("No monitors available")?;
    
    let save_path = PathBuf::from(save_dir);
    ensure_dir(&save_path)?;
    
    let image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture screen: {}", e))?;
    
    let filename = generate_filename("screenshot", "png")?;
    let screenshot_path = save_path.join(&filename);
    
    image
        .save(&screenshot_path)
        .map_err(|e| format!("Failed to save screenshot: {}", e))?;
    
    Ok(screenshot_path.to_string_lossy().into_owned())
}

/// Capture a region from the primary monitor
pub fn capture_region_from_screen(
    save_dir: &str,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
) -> AppResult<String> {
    // First capture full screen
    let temp_path = std::env::temp_dir().join(format!("bettershot_region_{}.png", 
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    
    let monitors = Monitor::all().map_err(|e| format!("Failed to get monitors: {}", e))?;
    
    // Find the monitor that contains the region
    let monitor = monitors
        .iter()
        .find(|m| {
            let mx = m.x().unwrap_or(0);
            let my = m.y().unwrap_or(0);
            let mw = m.width().unwrap_or(0) as i32;
            let mh = m.height().unwrap_or(0) as i32;
            x >= mx && y >= my && x + width as i32 <= mx + mw && y + height as i32 <= my + mh
        })
        .or_else(|| monitors.first())
        .ok_or("No monitors available")?;
    
    let image = monitor
        .capture_image()
        .map_err(|e| format!("Failed to capture screen: {}", e))?;
    
    // Calculate offset relative to monitor
    let monitor_x = monitor.x().unwrap_or(0);
    let monitor_y = monitor.y().unwrap_or(0);
    let relative_x = (x - monitor_x).max(0) as u32;
    let relative_y = (y - monitor_y).max(0) as u32;
    
    let img_width = image.width();
    let img_height = image.height();
    
    let crop_x = relative_x.min(img_width.saturating_sub(1));
    let crop_y = relative_y.min(img_height.saturating_sub(1));
    let crop_w = width.min(img_width.saturating_sub(crop_x));
    let crop_h = height.min(img_height.saturating_sub(crop_y));
    
    if crop_w == 0 || crop_h == 0 {
        return Err("Invalid crop region".to_string());
    }
    
    let cropped = image::imageops::crop_imm(&image, crop_x, crop_y, crop_w, crop_h).to_image();
    
    let save_path = PathBuf::from(save_dir);
    ensure_dir(&save_path)?;
    let filename = generate_filename("screenshot", "png")?;
    let output_path = save_path.join(&filename);
    
    cropped.save(&output_path)
        .map_err(|e| format!("Failed to save cropped screenshot: {}", e))?;
    
    let _ = std::fs::remove_file(&temp_path);
    
    Ok(output_path.to_string_lossy().into_owned())
}
