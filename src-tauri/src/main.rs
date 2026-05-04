#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod parser;

use parser::TrackerSnapshot;
use serde::Deserialize;

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
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            db::init(app.handle())
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_log_snapshot,
            set_position_locked,
            set_clickable_rects
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Torch Overlay");
}
