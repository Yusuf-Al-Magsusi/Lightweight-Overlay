//! Overlay application entry point. All OS-specific code lives in `platform`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod platform;
mod widgets;

use lwo_core::config::{self, Config};

fn main() {
    let mut warnings = Vec::new();

    let (config, config_path) = match config::default_path() {
        Some(path) => {
            let loaded = Config::load_or_create(&path);
            warnings.extend(loaded.warning);
            (loaded.config, Some(path))
        }
        None => {
            warnings.push("APPDATA is not set; using default settings.".to_string());
            (Config::default(), None)
        }
    };
    warnings.extend(config.problems());

    let app = platform::App {
        config,
        config_path,
        warnings,
    };
    if let Err(e) = platform::run(app) {
        eprintln!("{} failed: {e}", lwo_core::APP_NAME);
        std::process::exit(1);
    }
}
