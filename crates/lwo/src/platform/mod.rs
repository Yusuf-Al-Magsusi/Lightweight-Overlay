//! Per-platform code, selected at compile time. Each platform module provides
//! `run(App)`, which shows the overlay and blocks until the user quits.
//! Widgets draw through the `lwo_core::widget::Canvas` trait, so they never
//! see OS types.

use std::path::PathBuf;

use lwo_core::config::Config;
use lwo_core::widget::Widget;

/// Everything the platform needs to run the overlay.
pub struct App {
    pub config: Config,
    /// Where the config came from (and will be saved to), if known.
    #[allow(dead_code)] // used by the settings UI in a later milestone
    pub config_path: Option<PathBuf>,
    pub widgets: Vec<Box<dyn Widget>>,
    /// Problems to tell the user about once at startup.
    pub warnings: Vec<String>,
}

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::run;

#[cfg(not(windows))]
compile_error!("only Windows is supported before v0.1");
