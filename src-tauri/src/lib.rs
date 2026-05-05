//! ClipShot for Windows - Screenshot capture and editing application

mod clipboard;
mod commands;
mod image;
mod screenshot;
mod utils;

use commands::{
    capture_all_monitors, capture_once, capture_region, copy_image_file_to_clipboard,
    crop_and_save_region, capture_screen_for_selector,
    get_desktop_directory, get_mouse_position, get_temp_directory, move_window_to_active_space,
    native_capture_fullscreen, native_capture_interactive, native_capture_window,
    play_screenshot_sound, read_file_as_base64, render_image_with_effects_rust, save_edited_image,
};

use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

fn show_main_window(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("ClipShot")
            .inner_size(1200.0, 800.0)
            .min_inner_size(800.0, 600.0)
            .center()
            .resizable(true)
            .decorations(true)
            .build()?;

        let window_clone = window.clone();
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window_clone.hide();
                api.prevent_close();
            }
        });
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .setup(|app| {
            // ── Main window (hidden on start) ──────────────────────────────
            let window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("ClipShot")
                .inner_size(1200.0, 800.0)
                .min_inner_size(800.0, 600.0)
                .center()
                .resizable(true)
                .decorations(true)
                .visible(false)
                .build()?;

            let window_clone = window.clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let _ = window_clone.hide();
                    api.prevent_close();
                }
            });

            // ── Quick overlay window ───────────────────────────────────────
            let overlay = WebviewWindowBuilder::new(
                app,
                "quick-overlay",
                WebviewUrl::App("index.html?overlay=1".into()),
            )
            .title("ClipShot – Quick Overlay")
            .inner_size(360.0, 240.0)
            .resizable(true)
            .decorations(true)
            .visible(false)
            .always_on_top(true)
            .build()?;

            let overlay_clone = overlay.clone();
            overlay.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let _ = overlay_clone.hide();
                    api.prevent_close();
                }
            });

            // ── Region selector — fullscreen transparent overlay ───────────
            let selector = WebviewWindowBuilder::new(
                app,
                "region-selector",
                WebviewUrl::App("index.html?selector=1".into()),
            )
            .title("Select Region")
            .fullscreen(true)
            .decorations(false)
            .transparent(true)
            .always_on_top(true)
            .skip_taskbar(true)
            .visible(false)
            .build()?;

            let selector_clone = selector.clone();
            selector.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let _ = selector_clone.hide();
                    api.prevent_close();
                }
            });

            // ── System tray ────────────────────────────────────────────────
            use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem};

            let open_item = MenuItemBuilder::with_id("open", "Open ClipShot").build(app)?;
            let capture_region_item = MenuItemBuilder::with_id("capture_region", "Capture Region").build(app)?;
            let capture_screen_item = MenuItemBuilder::with_id("capture_screen", "Capture Screen").build(app)?;
            let capture_window_item = MenuItemBuilder::with_id("capture_window", "Capture Window").build(app)?;
            let preferences_item = MenuItemBuilder::with_id("preferences", "Preferences...")
                .accelerator("Ctrl+,")
                .build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit")
                .accelerator("Ctrl+Q")
                .build(app)?;

            let menu = MenuBuilder::new(app)
                .items(&[
                    &open_item,
                    &PredefinedMenuItem::separator(app)?,
                    &capture_region_item,
                    &capture_screen_item,
                    &capture_window_item,
                    &PredefinedMenuItem::separator(app)?,
                    &preferences_item,
                    &PredefinedMenuItem::separator(app)?,
                    &quit_item,
                ])
                .build()?;

            let _tray = tauri::tray::TrayIconBuilder::new()
                .menu(&menu)
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("ClipShot")
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "open" => {
                        let _ = show_main_window(app);
                    }
                    "capture_region" => {
                        let _ = app.emit("capture-triggered", ());
                    }
                    "capture_screen" => {
                        let _ = app.emit("capture-fullscreen", ());
                    }
                    "capture_window" => {
                        let _ = app.emit("capture-window", ());
                    }
                    "preferences" => {
                        if let Ok(()) = show_main_window(app) {
                            let _ = app.emit("open-preferences", ());
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            capture_once,
            capture_all_monitors,
            capture_region,
            save_edited_image,
            render_image_with_effects_rust,
            get_desktop_directory,
            get_temp_directory,
            native_capture_interactive,
            native_capture_fullscreen,
            native_capture_window,
            play_screenshot_sound,
            get_mouse_position,
            move_window_to_active_space,
            copy_image_file_to_clipboard,
            crop_and_save_region,
            capture_screen_for_selector,
            read_file_as_base64,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
