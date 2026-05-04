#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod parser;

use parser::TrackerSnapshot;
use serde::Deserialize;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
};
use tauri::Manager;

#[derive(Debug, Deserialize)]
struct ClickableRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[tauri::command]
fn parse_log_snapshot(path: Option<String>) -> Result<TrackerSnapshot, String> {
    let path = path.unwrap_or_else(default_log_path);
    parser::parse_log_file(path).map_err(|error| error.to_string())
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
            write_diagnostic("setup completed");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_log_snapshot,
            set_position_locked,
            set_clickable_rects
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        write_diagnostic(format!("tauri runtime failed: {error:?}"));
        eprintln!("failed to run Torch Overlay: {error}");
        std::process::exit(1);
    }
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
