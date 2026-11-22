//! Overlay application entry point. All OS-specific code lives in `platform`.

#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod platform;

use lwo_core::clock::Clock;
use lwo_core::config::{self, Config};
use lwo_core::now_playing::NowPlaying;
use lwo_core::stats::{CpuWidget, GpuWidget, RamWidget};
use lwo_core::widget::Widget;

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

    let mut widgets: Vec<Box<dyn Widget>> = Vec::new();
    if config.widgets.clock.enabled {
        widgets.push(Box::new(Clock::new(
            config.widgets.clock.show_seconds,
            config.widgets.clock.use_24h,
        )));
    }

    if config.widgets.now_playing.enabled {
        widgets.push(Box::new(NowPlaying::new(
            config.widgets.now_playing.show_album,
        )));
    }

    if config.widgets.cpu.enabled {
        widgets.push(Box::new(CpuWidget::new()));
    }
    if config.widgets.ram.enabled {
        widgets.push(Box::new(RamWidget::new()));
    }
    if config.widgets.gpu.enabled {
        widgets.push(Box::new(GpuWidget::new()));
    }

    let app = platform::App {
        config,
        config_path,
        widgets,
        warnings,
    };
    if let Err(e) = platform::run(app) {
        eprintln!("{} failed: {e}", lwo_core::APP_NAME);
        std::process::exit(1);
    }
}
