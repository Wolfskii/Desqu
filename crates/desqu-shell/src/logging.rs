//! Process-wide `tracing` setup.

use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::sync::Mutex;

/// Logs bigger than this are moved aside at start-up (one old copy is kept).
const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

/// Log to stderr only. `default_filter` is used when `RUST_LOG` is not set, e.g.
/// `"screenbound_lib=info,warn"`.
pub fn init(default_filter: &str) {
    install(default_filter, None);
}

/// Like [`init`], and also append to `%APPDATA%\<identifier>\logs\app.log`. A release build has no
/// console, so without this nothing a module logs could be read afterwards.
pub fn init_with_file(identifier: &str, default_filter: &str) {
    install(default_filter, open_log(identifier));
}

pub fn log_path(identifier: &str) -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(identifier).join("logs").join("app.log"))
}

fn open_log(identifier: &str) -> Option<File> {
    let path = log_path(identifier)?;
    fs::create_dir_all(path.parent()?).ok()?;
    if fs::metadata(&path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = fs::rename(&path, path.with_extension("log.old"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn install(default_filter: &str, file: Option<File>) {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_filter));
    let builder = tracing_subscriber::fmt().with_env_filter(filter).with_target(true);
    let _ = match file {
        Some(file) => builder.with_ansi(false).with_writer(Mutex::new(file)).try_init(),
        None => builder.try_init(),
    };
}
