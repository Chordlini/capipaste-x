use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::ImageEncoder;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::ShortcutState;

const HOTKEY: &str = "ctrl+shift+s";

#[derive(Default)]
struct AppState {
    shot: Mutex<String>, // PNG data URL handed from Rust (or the overlay) to the next window
    // X11 only serves clipboard data while the owner lives, so keep one around.
    clipboard: Mutex<Option<arboard::Clipboard>>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn png_data_url(img: &image::RgbaImage) -> Result<String, String> {
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut buf,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Adaptive,
    )
    .write_image(img, img.width(), img.height(), image::ExtendedColorType::Rgba8)
    .map_err(err)?;
    Ok(format!("data:image/png;base64,{}", B64.encode(buf)))
}

fn decode_data_url(url: &str) -> Result<Vec<u8>, String> {
    let b64 = url.split_once(',').map_or(url, |(_, b)| b);
    B64.decode(b64).map_err(err)
}

/// Grab the monitor under the cursor and cover it with the region-select overlay.
fn capture(app: &AppHandle) -> Result<(), String> {
    if app.get_webview_window("select").is_some() {
        return Ok(());
    }
    let cursor = app.cursor_position().map_err(err)?;
    let mon = app
        .monitor_from_point(cursor.x, cursor.y)
        .ok()
        .flatten()
        .or(app.primary_monitor().ok().flatten())
        .ok_or("no monitor")?;
    // ponytail: xcap works in points on macOS and physical pixels elsewhere.
    let (x, y) = if cfg!(target_os = "macos") {
        let s = mon.scale_factor();
        (cursor.x / s, cursor.y / s)
    } else {
        (cursor.x, cursor.y)
    };
    let img = xcap::Monitor::from_point(x as i32, y as i32)
        .map_err(err)?
        .capture_image()
        .map_err(err)?;
    *app.state::<AppState>().shot.lock().unwrap() = png_data_url(&img)?;

    let win = WebviewWindowBuilder::new(app, "select", WebviewUrl::App("select.html".into()))
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .skip_taskbar(true)
        .shadow(false)
        .visible(false) // the page shows itself once the screenshot is painted
        .build()
        .map_err(err)?;
    win.set_position(*mon.position()).map_err(err)?;
    win.set_size(*mon.size()).map_err(err)?;
    Ok(())
}

#[tauri::command]
fn take_shot(state: State<AppState>) -> String {
    std::mem::take(&mut *state.shot.lock().unwrap())
}

/// Overlay picked a region: hand the crop to a fresh markup card.
#[tauri::command]
fn open_card(app: AppHandle, state: State<AppState>, png: String, w: f64, h: f64) -> Result<(), String> {
    *state.shot.lock().unwrap() = png;
    if let Some(sel) = app.get_webview_window("select") {
        sel.close().map_err(err)?;
    }
    if let Some(old) = app.get_webview_window("card") {
        old.close().map_err(err)?;
    }
    // Card chrome: 10px padding each side, 52px toolbar. The page fits the shot inside.
    let (cw, ch) = (w.clamp(340.0, 900.0) + 20.0, h.min(700.0) + 72.0);
    WebviewWindowBuilder::new(&app, "card", WebviewUrl::App("card.html".into()))
        .title("Capipaste")
        .decorations(false)
        .always_on_top(true)
        .resizable(false)
        .inner_size(cw, ch)
        .center()
        .build()
        .map_err(err)?;
    Ok(())
}

/// Put the finished PNG on the clipboard and keep a copy in Pictures/Capipaste.
#[tauri::command]
fn copy_png(app: AppHandle, state: State<AppState>, png: String) -> Result<String, String> {
    let bytes = decode_data_url(&png)?;
    let img = image::load_from_memory(&bytes).map_err(err)?.to_rgba8();

    let mut cb = state.clipboard.lock().unwrap();
    if cb.is_none() {
        *cb = Some(arboard::Clipboard::new().map_err(err)?);
    }
    cb.as_mut()
        .unwrap()
        .set_image(arboard::ImageData {
            width: img.width() as usize,
            height: img.height() as usize,
            bytes: img.as_raw().into(),
        })
        .map_err(err)?;

    let dir = app.path().picture_dir().map_err(err)?.join("Capipaste");
    std::fs::create_dir_all(&dir).map_err(err)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(err)?
        .as_secs();
    let path = dir.join(format!("Capipaste-{secs}.png"));
    std::fs::write(&path, bytes).map_err(err)?;
    Ok(path.display().to_string())
}

pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcuts([HOTKEY])
                .expect("valid hotkey")
                .with_handler(|app, _, ev| {
                    if ev.state == ShortcutState::Pressed {
                        if let Err(e) = capture(app) {
                            eprintln!("capture failed: {e}");
                        }
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![take_shot, open_card, copy_png])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(app, "capture", "Capture", true, Some(HOTKEY))?,
                    &MenuItem::with_id(app, "quit", "Quit Capipaste", true, None::<&str>)?,
                ],
            )?;
            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("Capipaste")
                .menu(&menu)
                .on_menu_event(|app, ev| match ev.id().as_ref() {
                    "capture" => {
                        if let Err(e) = capture(app) {
                            eprintln!("capture failed: {e}");
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building capipaste");

    // Tray app: closing the last window must not quit.
    app.run(|_, ev| {
        if let RunEvent::ExitRequested { api, code: None, .. } = ev {
            api.prevent_exit();
        }
    });
}
