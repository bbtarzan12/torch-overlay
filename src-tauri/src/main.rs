#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod diagnostics;
mod offline_items;
#[cfg(test)]
#[allow(dead_code)]
mod parser;
mod tracker;

use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex, OnceLock},
};
use tauri::{LogicalSize, Manager, PhysicalPosition, State, Window, WindowEvent};
use tracker::{LogTracker, TrackerSnapshot};

struct AppState {
    tracker: Mutex<LogTracker>,
    manual_log_path_file: Option<PathBuf>,
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
fn open_diagnostics_folder() -> Result<(), String> {
    let Some(path) = diagnostics::dir() else {
        return Err("failed to resolve diagnostics folder".to_string());
    };

    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    diagnostics::write(format!(
        "open diagnostics folder requested path=\"{}\"",
        path.display()
    ));

    open_folder(&path)
}

#[tauri::command]
fn open_game_log_folder(state: State<AppState>) -> Result<(), String> {
    let tracker = state
        .tracker
        .lock()
        .map_err(|error| format!("tracker lock poisoned: {error}"))?;
    let Some(path) = tracker.game_log_path().parent() else {
        return Err("failed to resolve game log folder".to_string());
    };

    open_folder(path)
}

#[tauri::command]
fn set_game_log_path(state: State<AppState>, path: String) -> Result<TrackerSnapshot, String> {
    let path = PathBuf::from(path.trim());

    if !path.is_file() {
        diagnostics::write(format!(
            "manual game log path rejected path=\"{}\"",
            path.display()
        ));
        return Err("입력한 경로에 로그 파일이 없습니다.".to_string());
    }

    if let Some(manual_log_path_file) = &state.manual_log_path_file {
        if let Some(parent) = manual_log_path_file.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        fs::write(manual_log_path_file, path.to_string_lossy().as_bytes())
            .map_err(|error| error.to_string())?;
    }

    let mut tracker = state
        .tracker
        .lock()
        .map_err(|error| format!("tracker lock poisoned: {error}"))?;
    tracker.set_log_path(path)
}

fn open_folder(path: &Path) -> Result<(), String> {
    diagnostics::write(format!("open folder requested path=\"{}\"", path.display()));

    #[cfg(windows)]
    {
        Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|error| error.to_string())?;
    }

    #[cfg(not(windows))]
    {
        let _ = path;
        return Err("opening folders is only supported on Windows".to_string());
    }

    Ok(())
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

fn resolve_game_log_path(manual_log_path_file: Option<&Path>) -> PathBuf {
    let candidates = game_log_candidates(
        manual_log_path_file
            .and_then(load_manual_game_log_path)
            .filter(|path| !path.as_os_str().is_empty()),
    );

    for candidate in &candidates {
        let exists = candidate.path.is_file();
        diagnostics::write(format!(
            "game log candidate source={} path=\"{}\" exists={exists}",
            candidate.source,
            candidate.path.display()
        ));

        if exists {
            return candidate.path.clone();
        }
    }

    diagnostics::write("no game log candidate exists; falling back to default path");
    PathBuf::from(default_log_path())
}

#[derive(Debug)]
struct GameLogCandidate {
    source: String,
    path: PathBuf,
}

fn game_log_candidates(manual_log_path: Option<PathBuf>) -> Vec<GameLogCandidate> {
    let mut candidates = Vec::new();

    if let Some(path) = env::var_os("TORCH_OVERLAY_GAME_LOG").map(PathBuf::from) {
        push_log_candidate(&mut candidates, "env:TORCH_OVERLAY_GAME_LOG", path);
    }

    if let Some(path) = manual_log_path {
        push_log_candidate(&mut candidates, "manual-setting", path);
    }

    for steam_root in steam_library_roots() {
        push_log_candidate(
            &mut candidates,
            "steam-libraryfolders",
            steam_root
                .join("steamapps")
                .join("common")
                .join("Torchlight Infinite")
                .join("UE_game")
                .join("TorchLight")
                .join("Saved")
                .join("Logs")
                .join("UE_game.log"),
        );
    }

    push_log_candidate(
        &mut candidates,
        "default",
        PathBuf::from(default_log_path()),
    );

    for drive in b'C'..=b'Z' {
        let prefix = format!("{}:\\", drive as char);
        for steam_root in [
            PathBuf::from(format!("{prefix}SteamLibrary")),
            PathBuf::from(format!("{prefix}Steam")),
            PathBuf::from(format!("{prefix}Program Files (x86)\\Steam")),
            PathBuf::from(format!("{prefix}Program Files\\Steam")),
        ] {
            push_log_candidate(
                &mut candidates,
                "drive-scan",
                steam_root
                    .join("steamapps")
                    .join("common")
                    .join("Torchlight Infinite")
                    .join("UE_game")
                    .join("TorchLight")
                    .join("Saved")
                    .join("Logs")
                    .join("UE_game.log"),
            );
        }
    }

    candidates
}

fn manual_game_log_path_file(app: &tauri::AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|path| path.join("game-log-path.txt"))
}

fn load_manual_game_log_path(path: &Path) -> Option<PathBuf> {
    let raw = fs::read_to_string(path).ok()?;
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        None
    } else {
        diagnostics::write(format!("manual game log path loaded path=\"{trimmed}\""));
        Some(PathBuf::from(trimmed))
    }
}

fn push_log_candidate(candidates: &mut Vec<GameLogCandidate>, source: &str, path: PathBuf) {
    if candidates
        .iter()
        .any(|candidate| path_eq_ignore_ascii_case(&candidate.path, &path))
    {
        return;
    }

    candidates.push(GameLogCandidate {
        source: source.to_string(),
        path,
    });
}

fn steam_library_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for steam_root in steam_install_roots() {
        push_unique_path(&mut roots, steam_root.clone());

        let library_file = steam_root.join("steamapps").join("libraryfolders.vdf");
        let Ok(raw) = fs::read_to_string(&library_file) else {
            diagnostics::write(format!(
                "steam library file missing path=\"{}\"",
                library_file.display()
            ));
            continue;
        };

        diagnostics::write(format!(
            "steam library file loaded path=\"{}\" bytes={}",
            library_file.display(),
            raw.len()
        ));

        for library_path in parse_steam_library_paths(&raw) {
            push_unique_path(&mut roots, library_path);
        }
    }

    roots
}

fn steam_install_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(program_files) = env::var_os(variable).map(PathBuf::from) {
            push_unique_path(&mut roots, program_files.join("Steam"));
        }
    }

    for drive in b'C'..=b'Z' {
        let prefix = format!("{}:\\", drive as char);
        push_unique_path(&mut roots, PathBuf::from(format!("{prefix}Steam")));
        push_unique_path(
            &mut roots,
            PathBuf::from(format!("{prefix}Program Files (x86)\\Steam")),
        );
        push_unique_path(
            &mut roots,
            PathBuf::from(format!("{prefix}Program Files\\Steam")),
        );
    }

    roots
}

fn parse_steam_library_paths(raw: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    for line in raw.lines() {
        let values = quoted_values(line);

        if values.len() >= 2 && values[0] == "path" {
            push_unique_path(&mut paths, PathBuf::from(values[1].replace("\\\\", "\\")));
        } else if values.len() == 2
            && values[0]
                .chars()
                .all(|character| character.is_ascii_digit())
        {
            push_unique_path(&mut paths, PathBuf::from(values[1].replace("\\\\", "\\")));
        }
    }

    paths
}

fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;
    let mut escaped = false;

    for character in line.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            continue;
        }

        if in_quote && character == '\\' {
            escaped = true;
            continue;
        }

        if character == '"' {
            if in_quote {
                values.push(current.clone());
                current.clear();
            }
            in_quote = !in_quote;
            continue;
        }

        if in_quote {
            current.push(character);
        }
    }

    values
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if paths
        .iter()
        .any(|existing| path_eq_ignore_ascii_case(existing, &path))
    {
        return;
    }

    paths.push(path);
}

fn path_eq_ignore_ascii_case(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(right.to_string_lossy().as_ref())
}

fn main() {
    diagnostics::install_panic_hook();
    diagnostics::write("process starting");

    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            diagnostics::write("setup starting");
            diagnostics::write(format!(
                "app data dir: {:?}",
                app.path().app_data_dir().ok()
            ));
            db::init(app.handle()).map_err(|error| {
                diagnostics::write(format!("db init failed: {error:?}"));
                Box::new(error) as Box<dyn std::error::Error>
            })?;
            let db_path = db::path(app.handle()).map_err(|error| {
                diagnostics::write(format!("db path failed: {error:?}"));
                Box::new(error) as Box<dyn std::error::Error>
            })?;
            diagnostics::probe_file("tracker db", &db_path);

            let manual_log_path_file = manual_game_log_path_file(app.handle());
            let log_path = resolve_game_log_path(manual_log_path_file.as_deref());
            diagnostics::probe_file("selected game log", &log_path);

            let tracker = LogTracker::new(log_path, db_path);
            app.manage(AppState {
                tracker: Mutex::new(tracker),
                manual_log_path_file,
            });
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_size(LogicalSize::new(1380.0, 30.0));
                restore_window_position(&window, app.handle());
                install_window_position_persistence(&window, app.handle());
                install_overlay_hit_test(&window);
            }
            diagnostics::write("setup completed");
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
            set_overlay_opacity,
            open_diagnostics_folder,
            open_game_log_folder,
            set_game_log_path
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        diagnostics::write(format!("tauri runtime failed: {error:?}"));
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
