use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const LOG_DIR_NAME: &str = "Torch Overlay Diagnostics";
const LOG_FILE_NAME: &str = "debug.log";
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

pub fn install_panic_hook() {
    let previous_hook = std::panic::take_hook();

    std::panic::set_hook(Box::new(move |panic_info| {
        write(format!("panic: {panic_info}"));
        previous_hook(panic_info);
    }));
}

pub fn write(message: impl AsRef<str>) {
    let Some(path) = path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    rotate_if_needed(&path);

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "[{}] {}",
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            message.as_ref()
        );
    }
}

pub fn probe_file(label: &str, path: &Path) {
    match fs::metadata(path) {
        Ok(metadata) => write(format!(
            "{label}: path=\"{}\" exists=true is_file={} len={} modified={:?}",
            path.display(),
            metadata.is_file(),
            metadata.len(),
            metadata.modified().ok()
        )),
        Err(error) => write(format!(
            "{label}: path=\"{}\" exists=false error={error}",
            path.display()
        )),
    }
}

pub fn path() -> Option<PathBuf> {
    dir().map(|path| path.join(LOG_FILE_NAME))
}

pub fn dir() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|path| path.join(LOG_DIR_NAME))
}

fn rotate_if_needed(path: &Path) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };

    if metadata.len() <= MAX_LOG_BYTES {
        return;
    }

    let rotated = path.with_extension("log.old");
    let _ = fs::rename(path, rotated);
}
