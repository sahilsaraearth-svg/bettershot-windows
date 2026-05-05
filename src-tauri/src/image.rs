//! Image processing module

use base64::{engine::general_purpose, Engine as _};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use imageproc::filter::gaussian_blur_f32;
use std::fs;
use std::path::PathBuf;

use crate::utils::{ensure_dir, generate_filename, resolve_path, AppResult};

/// Region coordinates for cropping
#[derive(Debug, Clone, Copy)]
pub struct CropRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl CropRegion {
    pub fn clamped(
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        img_width: u32,
        img_height: u32,
    ) -> Self {
        let crop_x = x.min(img_width.saturating_sub(1));
        let crop_y = y.min(img_height.saturating_sub(1));
        let crop_width = width.min(img_width.saturating_sub(crop_x));
        let crop_height = height.min(img_height.saturating_sub(crop_y));
        Self { x: crop_x, y: crop_y, width: crop_width, height: crop_height }
    }

    pub fn is_valid(&self) -> bool {
        self.width > 0 && self.height > 0
    }
}

/// Crop an image file and save to a new location
pub fn crop_image(source_path: &str, region: CropRegion, save_dir: &str) -> AppResult<String> {
    let img = image::open(source_path).map_err(|e| format!("Failed to open screenshot: {}", e))?;

    let img_width = img.width();
    let img_height = img.height();
    let region = CropRegion::clamped(region.x, region.y, region.width, region.height, img_width, img_height);

    if !region.is_valid() {
        return Err(format!(
            "Invalid crop region: x={}, y={}, w={}, h={} (image: {}x{})",
            region.x, region.y, region.width, region.height, img_width, img_height
        ));
    }

    let cropped = img.crop_imm(region.x, region.y, region.width, region.height);
    save_image(&cropped, save_dir, "region")
}

/// Save a DynamicImage to a directory with a generated filename
pub fn save_image(img: &DynamicImage, save_dir: &str, prefix: &str) -> AppResult<String> {
    let dest_path = PathBuf::from(save_dir);
    ensure_dir(&dest_path)?;

    let filename = generate_filename(prefix, "png")?;
    let file_path = dest_path.join(&filename);

    img.save(&file_path).map_err(|e| format!("Failed to save image: {}", e))?;
    Ok(resolve_path(&file_path.to_string_lossy()))
}

/// Save base64-encoded image data to a file
pub fn save_base64_image(image_data: &str, save_dir: &str, prefix: &str) -> AppResult<String> {
    // Accept any data URI: data:image/png;base64,... or data:image/jpeg;base64,...
    let base64_data = image_data
        .splitn(2, ',')
        .nth(1)
        .ok_or("Invalid image data: expected data:<type>;base64,<data>")?;

    let image_bytes = general_purpose::STANDARD
        .decode(base64_data)
        .map_err(|e| format!("Failed to decode base64: {}", e))?;

    let dest_path = PathBuf::from(save_dir);
    ensure_dir(&dest_path)?;

    let filename = generate_filename(prefix, "png")?;
    let file_path = dest_path.join(&filename);

    fs::write(&file_path, image_bytes).map_err(|e| format!("Failed to save image: {}", e))?;
    Ok(resolve_path(&file_path.to_string_lossy()))
}

/// Copy a screenshot file to a destination directory
pub fn copy_screenshot_to_dir(source_path: &str, save_dir: &str) -> AppResult<String> {
    let src_path = PathBuf::from(source_path);
    if !src_path.exists() {
        return Err(format!("Screenshot file not found: {}", source_path));
    }

    let dest_path = PathBuf::from(save_dir);
    ensure_dir(&dest_path)?;

    let filename = generate_filename("shot", "png")?;
    let file_path = dest_path.join(&filename);

    fs::copy(&src_path, &file_path).map_err(|e| format!("Failed to copy screenshot: {}", e))?;
    Ok(resolve_path(&file_path.to_string_lossy()))
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
pub struct RenderSettings {
    pub background_type: String,
    pub custom_color: String,
    pub blur_amount: f32,
    pub noise_amount: f32,
    pub border_radius: f32,
    pub padding_top: u32,
    pub padding_bottom: u32,
    pub padding_left: u32,
    pub padding_right: u32,
    pub shadow_blur: f32,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_opacity: f32,
}

fn hex_to_rgba(hex: &str) -> Result<Rgba<u8>, String> {
    let hex = hex.trim_start_matches('#');
    if hex.len() != 6 {
        return Err("Invalid hex color".to_string());
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| "Invalid hex color")?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| "Invalid hex color")?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| "Invalid hex color")?;
    Ok(Rgba([r, g, b, 255]))
}

fn create_background(width: u32, height: u32, background_type: &str, custom_color: &str) -> RgbaImage {
    let mut img = RgbaImage::new(width, height);
    match background_type {
        "transparent" => {
            for pixel in img.pixels_mut() { *pixel = Rgba([0, 0, 0, 0]); }
        }
        "white" => {
            for pixel in img.pixels_mut() { *pixel = Rgba([255, 255, 255, 255]); }
        }
        "black" => {
            for pixel in img.pixels_mut() { *pixel = Rgba([0, 0, 0, 255]); }
        }
        "gray" => {
            for pixel in img.pixels_mut() { *pixel = Rgba([245, 245, 245, 255]); }
        }
        "custom" => {
            let color = hex_to_rgba(custom_color).unwrap_or(Rgba([255, 255, 255, 255]));
            for pixel in img.pixels_mut() { *pixel = color; }
        }
        _ => {
            for pixel in img.pixels_mut() { *pixel = Rgba([255, 255, 255, 255]); }
        }
    }
    img
}

fn apply_noise(img: &mut RgbaImage, amount: f32) {
    if amount <= 0.0 { return; }
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let intensity = (amount * 2.55) as i32;
    for pixel in img.pixels_mut() {
        let noise = rng.gen_range(-intensity..=intensity);
        let r = (pixel[0] as i32 + noise).clamp(0, 255) as u8;
        let g = (pixel[1] as i32 + noise).clamp(0, 255) as u8;
        let b = (pixel[2] as i32 + noise).clamp(0, 255) as u8;
        *pixel = Rgba([r, g, b, pixel[3]]);
    }
}

pub fn render_image_with_effects(image_path: &str, settings: RenderSettings) -> AppResult<String> {
    let img = image::open(image_path).map_err(|e| format!("Failed to open image: {}", e))?;

    let img_width = img.width();
    let img_height = img.height();
    let bg_width = img_width + settings.padding_left + settings.padding_right;
    let bg_height = img_height + settings.padding_top + settings.padding_bottom;

    let mut background = create_background(bg_width, bg_height, &settings.background_type, &settings.custom_color);

    if settings.blur_amount > 0.0 {
        background = gaussian_blur_f32(&background, settings.blur_amount);
    }

    if settings.noise_amount > 0.0 {
        apply_noise(&mut background, settings.noise_amount);
    }

    let img_rgba = img.to_rgba8();
    let mut final_img = RgbaImage::new(bg_width, bg_height);

    for y in 0..bg_height {
        for x in 0..bg_width {
            if x >= settings.padding_left
                && x < settings.padding_left + img_width
                && y >= settings.padding_top
                && y < settings.padding_top + img_height
            {
                let img_x = x - settings.padding_left;
                let img_y = y - settings.padding_top;

                let corner_x = if img_x < settings.border_radius as u32 { img_x }
                    else if img_x >= img_width.saturating_sub(settings.border_radius as u32) { img_width - img_x - 1 }
                    else { u32::MAX };

                let corner_y = if img_y < settings.border_radius as u32 { img_y }
                    else if img_y >= img_height.saturating_sub(settings.border_radius as u32) { img_height - img_y - 1 }
                    else { u32::MAX };

                let in_corner = corner_x < settings.border_radius as u32 && corner_y < settings.border_radius as u32;

                if in_corner {
                    let dist_x = corner_x as f32;
                    let dist_y = corner_y as f32;
                    let corner_dist = (dist_x * dist_x + dist_y * dist_y).sqrt();
                    if corner_dist <= settings.border_radius {
                        let pixel = img_rgba.get_pixel(img_x, img_y);
                        final_img.put_pixel(x, y, *pixel);
                    } else {
                        let bg_pixel = background.get_pixel(x, y);
                        final_img.put_pixel(x, y, *bg_pixel);
                    }
                } else {
                    let pixel = img_rgba.get_pixel(img_x, img_y);
                    final_img.put_pixel(x, y, *pixel);
                }
            } else {
                let bg_pixel = background.get_pixel(x, y);
                final_img.put_pixel(x, y, *bg_pixel);
            }
        }
    }

    let mut buffer = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buffer);
    DynamicImage::ImageRgba8(final_img)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| format!("Failed to encode image: {}", e))?;

    let base64_data = general_purpose::STANDARD.encode(&buffer);
    Ok(format!("data:image/png;base64,{}", base64_data))
}
