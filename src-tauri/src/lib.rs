mod dictation;
mod settings;

use std::str::FromStr;
use std::sync::Mutex;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use image::ImageEncoder;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

struct AppState {
    shot: Mutex<String>, // PNG data URL handed from Rust (or the overlay) to the next window
    // X11 only serves clipboard data while the owner lives, so keep one around.
    clipboard: Mutex<Option<arboard::Clipboard>>,
    settings: Mutex<settings::Settings>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            shot: Mutex::new(String::new()),
            clipboard: Mutex::new(None),
            settings: Mutex::new(settings::Settings::default()),
        }
    }
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
    .write_image(
        img,
        img.width(),
        img.height(),
        image::ExtendedColorType::Rgba8,
    )
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
async fn open_card(
    app: AppHandle,
    state: State<'_, AppState>,
    png: String,
    w: f64,
    h: f64,
) -> Result<(), String> {
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsPayload {
    settings: settings::Settings,
    microphones: Vec<String>,
    models: Vec<dictation::SpeechModelInfo>,
    compute: dictation::ComputeInfo,
}

#[tauri::command]
fn get_settings(app: AppHandle, state: State<AppState>) -> Result<SettingsPayload, String> {
    Ok(SettingsPayload {
        settings: state.settings.lock().unwrap().clone(),
        microphones: dictation::microphones()?,
        models: dictation::list_models(&app)?,
        compute: dictation::compute_info(),
    })
}

fn uses_right_alt(value: &settings::Settings) -> bool {
    value.dictate_hotkey.eq_ignore_ascii_case("RightAlt")
}

fn register_shortcuts(app: &AppHandle, value: &settings::Settings) -> Result<(), String> {
    Shortcut::from_str(&value.capture_hotkey).map_err(err)?;
    if !uses_right_alt(value) {
        Shortcut::from_str(&value.dictate_hotkey).map_err(err)?;
    }
    if value
        .capture_hotkey
        .eq_ignore_ascii_case(&value.dictate_hotkey)
    {
        return Err("Capture and Dictate need different shortcuts".into());
    }
    app.global_shortcut()
        .register(value.capture_hotkey.as_str())
        .map_err(err)?;
    if !uses_right_alt(value) {
        if let Err(problem) = app
            .global_shortcut()
            .register(value.dictate_hotkey.as_str())
        {
            let _ = app
                .global_shortcut()
                .unregister(value.capture_hotkey.as_str());
            return Err(problem.to_string());
        }
    }
    Ok(())
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<AppState>,
    value: settings::Settings,
) -> Result<(), String> {
    let old = state.settings.lock().unwrap().clone();
    app.global_shortcut().unregister_all().map_err(err)?;
    if let Err(problem) = register_shortcuts(&app, &value) {
        let _ = register_shortcuts(&app, &old);
        return Err(format!("That shortcut could not be registered: {problem}"));
    }
    settings::save(&app, &value)?;
    *state.settings.lock().unwrap() = value;
    let selected = state.settings.lock().unwrap().speech_model.clone();
    let warm_app = app.clone();
    std::thread::spawn(move || {
        if let Err(problem) = dictation::prepare_model(&warm_app, &selected) {
            eprintln!("speech model warmup failed: {problem}");
        }
    });
    Ok(())
}

#[tauri::command]
async fn download_speech_model(app: AppHandle, id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || dictation::download_model(app, id))
        .await
        .map_err(err)?
}

#[tauri::command]
fn delete_speech_model(app: AppHandle, id: String) -> Result<(), String> {
    dictation::delete_model(&app, &id)
}

#[tauri::command]
fn dictation_status(state: State<dictation::DictationState>) -> dictation::DictationStatus {
    dictation::status(state.inner())
}

fn model_chip(id: &str) -> &'static str {
    match id {
        "tiny-en-q5" => "TINY",
        "small-en-q5" => "SMALL",
        "nemotron-3.5-q8" => "NEMO 3.5",
        "nemotron-en-q8" => "NEMO EN",
        "parakeet-tdt-q8" => "PARAKEET",
        _ => "BASE",
    }
}

fn begin_dictation(app: &AppHandle) -> Result<(), String> {
    let value = app.state::<AppState>().settings.lock().unwrap().clone();
    if let Err(problem) = dictation::prepare_model(app, &value.speech_model) {
        dictation::show_error(app, &problem);
        return Err(problem);
    }
    dictation::start(
        app,
        Some(&value.microphone),
        model_chip(&value.speech_model),
    )
}

fn end_dictation(app: &AppHandle) -> Result<(), String> {
    let value = app.state::<AppState>().settings.lock().unwrap().clone();
    dictation::finish(app, &value.speech_model, value.tidy, &value.vocabulary)
}

#[cfg(target_os = "windows")]
fn listen_for_right_alt(app: AppHandle) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_RMENU};

    std::thread::spawn(move || {
        let mut was_down = false;
        loop {
            let enabled = uses_right_alt(&app.state::<AppState>().settings.lock().unwrap());
            let is_down = enabled && unsafe { GetAsyncKeyState(VK_RMENU as i32) } < 0;
            if is_down != was_down {
                let result = if is_down {
                    begin_dictation(&app)
                } else {
                    end_dictation(&app)
                };
                if let Err(problem) = result {
                    eprintln!("Right Alt dictation failed: {problem}");
                }
            }
            was_down = is_down;
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
    });
}

#[tauri::command]
fn open_settings(app: AppHandle) -> Result<(), String> {
    show_settings(&app)
}

fn show_settings(app: &AppHandle) -> Result<(), String> {
    if let Some(win) = app.get_webview_window("settings") {
        win.show().map_err(err)?;
        win.set_focus().map_err(err)?;
        return Ok(());
    }
    WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("Capipaste Settings")
        .inner_size(760.0, 650.0)
        .min_inner_size(680.0, 560.0)
        .center()
        .build()
        .map_err(err)?;
    Ok(())
}

pub fn run() {
    let app = tauri::Builder::default()
        .manage(AppState::default())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, ev| {
                    let value = app.state::<AppState>().settings.lock().unwrap().clone();
                    let capture_shortcut = Shortcut::from_str(&value.capture_hotkey).ok();
                    let dictate_shortcut = Shortcut::from_str(&value.dictate_hotkey).ok();
                    if ev.state == ShortcutState::Pressed
                        && capture_shortcut.as_ref() == Some(shortcut)
                    {
                        if let Err(e) = capture(app) {
                            eprintln!("capture failed: {e}");
                        }
                    } else if dictate_shortcut.as_ref() == Some(shortcut) {
                        let result = if ev.state == ShortcutState::Pressed {
                            begin_dictation(app)
                        } else {
                            end_dictation(app)
                        };
                        if let Err(e) = result {
                            eprintln!("dictation failed: {e}");
                        }
                    }
                })
                .build(),
        )
        .manage(dictation::DictationState::default())
        .invoke_handler(tauri::generate_handler![
            take_shot,
            open_card,
            copy_png,
            get_settings,
            save_settings,
            download_speech_model,
            delete_speech_model,
            dictation_status,
            open_settings
        ])
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let saved = settings::load(app.handle());
            *app.state::<AppState>().settings.lock().unwrap() = saved.clone();
            if let Err(problem) = register_shortcuts(app.handle(), &saved) {
                eprintln!("shortcuts unavailable: {problem}");
            }
            #[cfg(target_os = "windows")]
            listen_for_right_alt(app.handle().clone());

            let warm_app = app.handle().clone();
            let warm_model = saved.speech_model.clone();
            std::thread::spawn(move || {
                if let Err(problem) = dictation::prepare_model(&warm_app, &warm_model) {
                    eprintln!("speech model warmup failed: {problem}");
                }
            });

            let dictate_accelerator = if uses_right_alt(&saved) {
                None
            } else {
                Some(saved.dictate_hotkey.as_str())
            };

            let menu = Menu::with_items(
                app,
                &[
                    &MenuItem::with_id(
                        app,
                        "capture",
                        "Capture",
                        true,
                        Some(saved.capture_hotkey.as_str()),
                    )?,
                    &MenuItem::with_id(app, "dictate", "Dictate", true, dictate_accelerator)?,
                    &MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?,
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
                    "dictate" => {
                        let active = app
                            .state::<dictation::DictationState>()
                            .status
                            .lock()
                            .unwrap()
                            .phase
                            == "recording";
                        let result = if active {
                            end_dictation(app)
                        } else {
                            begin_dictation(app)
                        };
                        if let Err(e) = result {
                            eprintln!("dictation failed: {e}");
                        }
                    }
                    "settings" => {
                        if let Err(e) = show_settings(app) {
                            eprintln!("settings failed: {e}");
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
        if let RunEvent::ExitRequested {
            api, code: None, ..
        } = ev
        {
            api.prevent_exit();
        }
    });
}
