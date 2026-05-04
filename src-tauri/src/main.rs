#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod offline_items;
#[cfg(test)]
#[allow(dead_code)]
mod parser;
mod tracker;

use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tauri::{LogicalSize, Manager, PhysicalPosition, State, Window, WindowEvent};
use tracker::{LogTracker, TrackerSnapshot};

struct AppState {
    tracker: Mutex<LogTracker>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
struct SavedWindowPosition {
    x: i32,
    y: i32,
}

#[derive(Debug, Clone)]
struct PhysicalClickableRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(windows)]
#[derive(Debug, Default)]
struct OverlayHitTestState {
    position_locked: bool,
    clickable_rects: Vec<PhysicalClickableRect>,
    previous_wndproc: isize,
}

#[cfg(windows)]
static OVERLAY_HIT_TEST_STATE: OnceLock<Arc<Mutex<OverlayHitTestState>>> = OnceLock::new();

#[derive(Debug, Deserialize)]
struct ClickableRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[tauri::command]
fn set_position_locked(locked: bool) -> Result<(), String> {
    update_position_locked_state(locked);
    Ok(())
}

#[tauri::command]
fn set_clickable_rects(window: Window, rects: Vec<ClickableRect>) -> Result<(), String> {
    update_clickable_rects(&window, rects);
    Ok(())
}

#[tauri::command]
fn set_overlay_window_size(window: Window, width: f64, height: f64) -> Result<(), String> {
    let width = width.clamp(360.0, 2400.0);
    let height = height.clamp(30.0, 900.0);

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
        .plugin(tauri_plugin_process::init())
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
                let _ = window.set_size(LogicalSize::new(1380.0, 30.0));
                restore_window_position(&window, app.handle());
                install_window_position_persistence(&window, app.handle());
                install_overlay_hit_test(&window);
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

fn restore_window_position(window: &tauri::WebviewWindow, app: &tauri::AppHandle) {
    let Some(position) = load_window_position(app) else {
        return;
    };

    let _ = window.set_position(PhysicalPosition::new(position.x, position.y));
}

fn install_window_position_persistence(window: &tauri::WebviewWindow, app: &tauri::AppHandle) {
    let Some(path) = window_position_path(app) else {
        return;
    };

    window.on_window_event(move |event| {
        if let WindowEvent::Moved(position) = event {
            save_window_position(
                &path,
                SavedWindowPosition {
                    x: position.x,
                    y: position.y,
                },
            );
        }
    });
}

fn load_window_position(app: &tauri::AppHandle) -> Option<SavedWindowPosition> {
    let path = window_position_path(app)?;
    let raw = fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

fn save_window_position(path: &Path, position: SavedWindowPosition) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(raw) = serde_json::to_string(&position) {
        let _ = fs::write(path, raw);
    }
}

fn window_position_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|path| path.join("window-position.json"))
}

#[cfg(windows)]
fn overlay_hit_test_state() -> &'static Arc<Mutex<OverlayHitTestState>> {
    OVERLAY_HIT_TEST_STATE.get_or_init(|| Arc::new(Mutex::new(OverlayHitTestState::default())))
}

#[cfg(windows)]
fn install_overlay_hit_test(window: &tauri::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowLongPtrW, GWLP_WNDPROC};

    let Ok(hwnd) = window.hwnd() else {
        return;
    };

    let Ok(mut state) = overlay_hit_test_state().lock() else {
        return;
    };

    if state.previous_wndproc != 0 {
        return;
    }

    unsafe {
        state.previous_wndproc =
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, overlay_wnd_proc as *const () as isize);
    }
}

#[cfg(not(windows))]
fn install_overlay_hit_test(_window: &tauri::WebviewWindow) {}

#[cfg(windows)]
fn update_position_locked_state(locked: bool) {
    if let Ok(mut state) = overlay_hit_test_state().lock() {
        state.position_locked = locked;
    }
}

#[cfg(not(windows))]
fn update_position_locked_state(_locked: bool) {}

#[cfg(windows)]
fn update_clickable_rects(window: &Window, rects: Vec<ClickableRect>) {
    let scale_factor = window.scale_factor().unwrap_or(1.0);

    let physical_rects = rects
        .into_iter()
        .filter(|rect| rect.width > 0.0 && rect.height > 0.0)
        .map(|rect| PhysicalClickableRect {
            x: rect.x * scale_factor,
            y: rect.y * scale_factor,
            width: rect.width * scale_factor,
            height: rect.height * scale_factor,
        })
        .collect::<Vec<_>>();

    if let Ok(mut state) = overlay_hit_test_state().lock() {
        state.clickable_rects = physical_rects;
    }
}

#[cfg(not(windows))]
fn update_clickable_rects(_window: &Window, _rects: Vec<ClickableRect>) {}

#[cfg(windows)]
unsafe extern "system" fn overlay_wnd_proc(
    hwnd: windows::Win32::Foundation::HWND,
    msg: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::{
        Foundation::LRESULT,
        UI::WindowsAndMessaging::{
            CallWindowProcW, DefWindowProcW, HTTRANSPARENT, WM_NCHITTEST, WNDPROC,
        },
    };

    if msg == WM_NCHITTEST && should_pass_through_overlay(hwnd, lparam) {
        return LRESULT(HTTRANSPARENT as isize);
    }

    let previous_wndproc = overlay_hit_test_state()
        .lock()
        .ok()
        .map(|state| state.previous_wndproc)
        .unwrap_or_default();

    if previous_wndproc == 0 {
        return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) };
    }

    let previous: WNDPROC = unsafe { std::mem::transmute(previous_wndproc) };
    unsafe { CallWindowProcW(previous, hwnd, msg, wparam, lparam) }
}

#[cfg(windows)]
fn should_pass_through_overlay(
    hwnd: windows::Win32::Foundation::HWND,
    lparam: windows::Win32::Foundation::LPARAM,
) -> bool {
    use windows::Win32::{Foundation::RECT, UI::WindowsAndMessaging::GetWindowRect};

    let Ok(state) = overlay_hit_test_state().lock() else {
        return false;
    };

    if !state.position_locked {
        return false;
    }

    let screen_x = (lparam.0 & 0xffff) as i16 as i32;
    let screen_y = ((lparam.0 >> 16) & 0xffff) as i16 as i32;
    let mut window_rect = RECT::default();

    if unsafe { GetWindowRect(hwnd, &mut window_rect) }.is_err() {
        return false;
    }

    let local_x = (screen_x - window_rect.left) as f64;
    let local_y = (screen_y - window_rect.top) as f64;
    let over_clickable = state.clickable_rects.iter().any(|rect| {
        local_x >= rect.x
            && local_x <= rect.x + rect.width
            && local_y >= rect.y
            && local_y <= rect.y + rect.height
    });

    !over_clickable
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
