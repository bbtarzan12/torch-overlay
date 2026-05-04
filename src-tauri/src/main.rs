#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
#[cfg(test)]
#[allow(dead_code)]
mod parser;
mod tracker;

use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
};
use tauri::{LogicalSize, Manager, State, Window};
use tracker::{LogTracker, TrackerSnapshot};

struct AppState {
    tracker: Mutex<LogTracker>,
}

#[derive(Debug, Deserialize)]
struct ClickableRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[tauri::command]
fn set_position_locked(locked: bool) -> Result<(), String> {
    let _ = locked;
    Ok(())
}

#[tauri::command]
fn set_clickable_rects(rects: Vec<ClickableRect>) -> Result<(), String> {
    let _total_area: f64 = rects
        .iter()
        .map(|rect| rect.width.max(0.0) * rect.height.max(0.0) + rect.x * 0.0 + rect.y * 0.0)
        .sum();

    Ok(())
}

#[tauri::command]
fn set_overlay_window_size(window: Window, width: f64, height: f64) -> Result<(), String> {
    let width = width.clamp(360.0, 2400.0);
    let height = height.clamp(46.0, 900.0);

    window
        .set_size(LogicalSize::new(width, height))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn set_overlay_opacity(window: Window, opacity: f64) -> Result<(), String> {
    apply_window_opacity(&window, opacity)
}

#[tauri::command]
fn tracker_snapshot(state: State<AppState>) -> Result<TrackerSnapshot, String> {
    let mut tracker = state
        .tracker
        .lock()
        .map_err(|error| format!("tracker lock poisoned: {error}"))?;
    tracker.snapshot()
}

#[tauri::command]
fn reset_tracker_session(state: State<AppState>) -> Result<TrackerSnapshot, String> {
    let mut tracker = state
        .tracker
        .lock()
        .map_err(|error| format!("tracker lock poisoned: {error}"))?;
    tracker.reset_session()?;
    tracker.snapshot()
}

#[tauri::command]
fn set_manual_item_price(
    state: State<AppState>,
    config_base_id: i64,
    price_in_crystal: f64,
) -> Result<TrackerSnapshot, String> {
    let mut tracker = state
        .tracker
        .lock()
        .map_err(|error| format!("tracker lock poisoned: {error}"))?;
    tracker.set_manual_item_price(config_base_id, price_in_crystal)
}

#[tauri::command]
fn set_item_ignored(
    state: State<AppState>,
    config_base_id: i64,
    ignored: bool,
) -> Result<TrackerSnapshot, String> {
    let mut tracker = state
        .tracker
        .lock()
        .map_err(|error| format!("tracker lock poisoned: {error}"))?;
    tracker.set_item_ignored(config_base_id, ignored)
}

fn default_log_path() -> String {
    r"D:\SteamLibrary\steamapps\common\Torchlight Infinite\UE_game\TorchLight\Saved\Logs\UE_game.log"
    .to_string()
}

fn main() {
    install_diagnostics();
    write_diagnostic("process starting");

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            write_diagnostic("setup starting");
            write_diagnostic(format!(
                "app data dir: {:?}",
                app.path().app_data_dir().ok()
            ));
            db::init(app.handle()).map_err(|error| {
                write_diagnostic(format!("db init failed: {error:?}"));
                Box::new(error) as Box<dyn std::error::Error>
            })?;
            let db_path = db::path(app.handle()).map_err(|error| {
                write_diagnostic(format!("db path failed: {error:?}"));
                Box::new(error) as Box<dyn std::error::Error>
            })?;
            let tracker = LogTracker::new(PathBuf::from(default_log_path()), db_path);
            app.manage(AppState {
                tracker: Mutex::new(tracker),
            });
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_size(LogicalSize::new(1404.0, 46.0));
            }
            write_diagnostic("setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            tracker_snapshot,
            reset_tracker_session,
            set_manual_item_price,
            set_item_ignored,
            set_position_locked,
            set_clickable_rects,
            set_overlay_window_size,
            set_overlay_opacity
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        write_diagnostic(format!("tauri runtime failed: {error:?}"));
        eprintln!("failed to run Torch Overlay: {error}");
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn apply_window_opacity(window: &Window, opacity: f64) -> Result<(), String> {
    use windows::Win32::{
        Foundation::COLORREF,
        UI::WindowsAndMessaging::{
            GetWindowLongW, SetLayeredWindowAttributes, SetWindowLongW, GWL_EXSTYLE, LWA_ALPHA,
            WS_EX_LAYERED,
        },
    };

    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let alpha = (opacity.clamp(0.05, 1.0) * 255.0).round() as u8;

    unsafe {
        let style = GetWindowLongW(hwnd, GWL_EXSTYLE);
        SetWindowLongW(hwnd, GWL_EXSTYLE, style | WS_EX_LAYERED.0 as i32);
        SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA)
            .map_err(|error| error.to_string())?;
    }

    Ok(())
}

#[cfg(not(windows))]
fn apply_window_opacity(_window: &Window, _opacity: f64) -> Result<(), String> {
    Ok(())
}

fn install_diagnostics() {
    let previous_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        write_diagnostic(format!("panic: {panic_info}"));
        previous_hook(panic_info);
    }));
}

fn write_diagnostic(message: impl AsRef<str>) {
    let Some(path) = diagnostic_log_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "[{}] {}",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            message.as_ref()
        );
    }
}

fn diagnostic_log_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join("Torch Overlay Diagnostics").join("startup.log"))
}
