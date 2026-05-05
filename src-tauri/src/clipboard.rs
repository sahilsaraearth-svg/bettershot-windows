//! Clipboard operations module - cross-platform using arboard

use crate::utils::AppResult;
use arboard::Clipboard;
use std::path::Path;

/// Copy an image file to the system clipboard
pub fn copy_image_to_clipboard(image_path: &str) -> AppResult<()> {
    let path = Path::new(image_path);
    
    let img = image::open(path)
        .map_err(|e| format!("Failed to open image for clipboard: {}", e))?;
    
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    
    let image_data = arboard::ImageData {
        width: width as usize,
        height: height as usize,
        bytes: rgba.into_raw().into(),
    };
    
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("Failed to open clipboard: {}", e))?;
    
    clipboard
        .set_image(image_data)
        .map_err(|e| format!("Failed to copy image to clipboard: {}", e))?;
    
    Ok(())
}

/// Copy text to the system clipboard
pub fn copy_text_to_clipboard(text: &str) -> AppResult<()> {
    let mut clipboard = Clipboard::new()
        .map_err(|e| format!("Failed to open clipboard: {}", e))?;
    
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to copy text to clipboard: {}", e))?;
    
    Ok(())
}
